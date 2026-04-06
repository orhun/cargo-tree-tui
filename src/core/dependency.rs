use std::path::PathBuf;

use anyhow::{Context, Result};
use cargo::{
    GlobalContext,
    core::{
        Package, PackageId, Workspace,
        compiler::{CompileKind, CompileKindFallback, RustcTargetData},
        dependency::DepKind,
        resolver::features::{CliFeatures, ForceAllTargets, HasDevUnits},
    },
    ops,
    util::important_paths::find_root_manifest_for_wd,
};
use clap_cargo::style::{DEP_BUILD, DEP_DEV, DEP_NORMAL};
use ratatui::style::Style;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

/// Key type for uniquely identifying nodes in the dependency tree.
///
/// This consists of the [`PackageId`] and an optional parent [`NodeId`] to
/// differentiate multiple appearances of the same package in different tree locations.
type NodeKey = (PackageId, Option<NodeId>);

/// Identifier for a node within the dependency tree arena.
///
/// The `usize` represents the index into the arena vector.
/// This is used for efficient storage and traversal of the tree structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyType {
    Normal,
    Dev,
    Build,
}

impl DependencyType {
    fn label(&self) -> &'static str {
        match self {
            Self::Normal => "[dependencies]",
            Self::Dev => "[dev-dependencies]",
            Self::Build => "[build-dependencies]",
        }
    }

    pub fn style(&self) -> Style {
        match self {
            Self::Normal => DEP_NORMAL.into(),
            Self::Dev => DEP_DEV.into(),
            Self::Build => DEP_BUILD.into(),
        }
    }
}

impl TryFrom<DepKind> for DependencyType {
    type Error = ();

    fn try_from(value: DepKind) -> Result<Self, Self::Error> {
        match value {
            DepKind::Normal => Ok(Self::Normal),
            DepKind::Development => Ok(Self::Dev),
            DepKind::Build => Ok(Self::Build),
        }
    }
}

/// Flat representation of a dependency node in the tree.
///
/// See [`DependencyTree`] for the full tree structure.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Crate name.
    pub name: String,
    /// Crate version.
    pub version: String,
    /// Local manifest directory (only for workspace members).
    pub manifest_dir: Option<String>,
    /// Whether this crate exposes a proc-macro target.
    pub is_proc_macro: bool,
    /// Optional parent pointer for quick upward navigation.
    pub parent: Option<NodeId>,
    /// Children represented as node indices for downward traversal.
    pub children: Vec<NodeId>,
}

/// Dependency group node (e.g. `[dev-dependencies]`) within the tree.
#[derive(Debug, Clone)]
pub struct DependencyGroup {
    /// Group kind in Cargo metadata.
    pub kind: DependencyType,
    /// Optional parent pointer for quick upward navigation.
    pub parent: Option<NodeId>,
    /// Children represented as node indices for downward traversal.
    pub children: Vec<NodeId>,
}

/// Unified dependency node type for the tree arena.
#[derive(Debug, Clone)]
pub enum DependencyNode {
    Crate(Dependency),
    Group(DependencyGroup),
}

impl DependencyGroup {
    pub fn label(&self) -> &'static str {
        self.kind.label()
    }
}

impl DependencyNode {
    pub fn parent(&self) -> Option<NodeId> {
        match self {
            Self::Crate(node) => node.parent,
            Self::Group(node) => node.parent,
        }
    }

    pub fn children(&self) -> &[NodeId] {
        match self {
            Self::Crate(node) => &node.children,
            Self::Group(node) => &node.children,
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group(_))
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Crate(node) => node.name.as_str(),
            Self::Group(node) => node.label(),
        }
    }

    pub fn as_dependency(&self) -> Option<&Dependency> {
        match self {
            Self::Crate(node) => Some(node),
            _ => None,
        }
    }

    pub fn as_group(&self) -> Option<&DependencyGroup> {
        match self {
            Self::Group(node) => Some(node),
            _ => None,
        }
    }
}

/// Container for the resolved dependency tree scoped to the current workspace.
///
/// The tree is stored in an arena-like structure where each node references its children
/// by their indices. This allows for efficient traversal and manipulation of the tree.
///
/// See also [`Dependency`].
#[derive(Debug, Clone)]
pub struct DependencyTree {
    /// Name of the root package (or workspace placeholder when missing).
    pub workspace_name: String,
    /// Arena storing all dependency nodes.
    pub nodes: Vec<DependencyNode>,
    /// Workspace members represented as node ids (entry points into the arena).
    pub roots: Vec<NodeId>,
    /// Flat list of crate nodes for search.
    pub crate_nodes: Vec<NodeId>,
}

impl DependencyTree {
    /// Loads Cargo workspace information (optionally for a specific manifest) and converts it into a
    /// [`DependencyTree`] with recursively resolved children.
    pub fn load(manifest_path: Option<PathBuf>) -> Result<Self> {
        let gctx = GlobalContext::default().context("failed to initialize Cargo context")?;
        let manifest_path = resolve_manifest_path(&gctx, manifest_path)?;
        let ws = Workspace::new(&manifest_path, &gctx).context("failed to load Cargo workspace")?;
        let requested_kinds = CompileKind::from_requested_targets_with_fallback(
            ws.gctx(),
            &[],
            CompileKindFallback::JustHost,
        )
        .context("failed to determine Cargo target kinds")?;
        let mut target_data =
            RustcTargetData::new(&ws, &requested_kinds).context("failed to load target data")?;
        let specs = ops::Packages::All(Vec::new())
            .to_package_id_specs(&ws)
            .context("failed to resolve workspace package specs")?;
        let ws_resolve = ops::resolve_ws_with_opts(
            &ws,
            &mut target_data,
            &requested_kinds,
            &CliFeatures::new_all(true),
            &specs,
            HasDevUnits::Yes,
            ForceAllTargets::Yes,
            false,
        )
        .context("failed to resolve Cargo dependencies")?;
        let pkg_set = ws_resolve.pkg_set;
        let resolve = ws_resolve.targeted_resolve;

        let workspace_name = ws
            .current_opt()
            .map(|pkg| pkg.name().to_string())
            .unwrap_or_else(|| "workspace".to_string());

        let mut package_map: HashMap<PackageId, &Package> =
            ws.members().map(|pkg| (pkg.package_id(), pkg)).collect();
        package_map.extend(pkg_set.packages().map(|pkg| (pkg.package_id(), pkg)));

        let mut nodes = Vec::new();
        let mut roots = Vec::new();
        let mut node_map: HashMap<NodeKey, NodeId> = HashMap::default();

        for package in ws.members() {
            let mut ancestors = HashSet::default();
            if let Some(dependency) = Self::build_dependency_node(
                package.package_id(),
                None,
                &resolve,
                &package_map,
                &mut node_map,
                &mut nodes,
                &mut ancestors,
            ) {
                roots.push(dependency);
            }
        }

        Ok(Self {
            workspace_name,
            crate_nodes: nodes
                .iter()
                .enumerate()
                .filter_map(|(idx, node)| (!node.is_group()).then_some(NodeId(idx)))
                .collect(),
            nodes,
            roots,
        })
    }

    /// Returns immutable access to a node identified by `id`.
    pub fn node(&self, id: NodeId) -> Option<&DependencyNode> {
        self.nodes.get(id.0)
    }

    /// Returns the workspace root node ids that should be rendered.
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// Returns the crate node ids that can be matched by search.
    pub fn crate_nodes(&self) -> &[NodeId] {
        &self.crate_nodes
    }

    /// Recursively constructs dependency nodes.
    ///
    /// # Notes
    ///
    /// - Each [`NodeKey`] is stored in `node_map` to avoid duplicating nodes.
    /// - The `parent` parameter allows tracking the parent node during recursion.
    fn build_dependency_node(
        package_id: PackageId,
        parent: Option<NodeId>,
        resolve: &cargo::core::Resolve,
        package_map: &HashMap<PackageId, &Package>,
        node_map: &mut HashMap<NodeKey, NodeId>,
        nodes: &mut Vec<DependencyNode>,
        ancestors: &mut HashSet<PackageId>,
    ) -> Option<NodeId> {
        debug_assert!(!ancestors.contains(&package_id));

        let key = (package_id, parent);
        if let Some(&existing) = node_map.get(&key) {
            return Some(existing);
        }

        let node_id = Self::push_crate_node(package_id, parent, package_map, node_map, nodes)?;
        ancestors.insert(package_id);

        let mut normal = Vec::new();
        let mut dev = Vec::new();
        let mut build = Vec::new();

        for (dep_id, deps) in resolve.deps(package_id) {
            let mut seen_kinds = HashSet::default();
            for dep in deps.iter() {
                let Ok(dep_type) = DependencyType::try_from(dep.kind()) else {
                    continue;
                };

                if !seen_kinds.insert(dep_type) {
                    continue;
                }

                match dep_type {
                    DependencyType::Normal => normal.push(dep_id),
                    DependencyType::Dev => dev.push(dep_id),
                    DependencyType::Build => build.push(dep_id),
                }
            }
        }

        let mut children = Vec::new();
        for dep_id in normal {
            let child = if ancestors.contains(&dep_id) {
                Self::push_crate_node(dep_id, Some(node_id), package_map, node_map, nodes)
            } else {
                Self::build_dependency_node(
                    dep_id,
                    Some(node_id),
                    resolve,
                    package_map,
                    node_map,
                    nodes,
                    ancestors,
                )
            };

            if let Some(child) = child {
                children.push(child);
            }
        }

        let mut build_group = |kind: DependencyType, deps: Vec<PackageId>| {
            if deps.is_empty() {
                return;
            }

            let group_id = NodeId(nodes.len());
            nodes.push(DependencyNode::Group(DependencyGroup {
                kind,
                parent: Some(node_id),
                children: Vec::new(),
            }));
            children.push(group_id);

            let group_children = deps
                .into_iter()
                .filter_map(|dep_id| {
                    if ancestors.contains(&dep_id) {
                        Self::push_crate_node(dep_id, Some(group_id), package_map, node_map, nodes)
                    } else {
                        Self::build_dependency_node(
                            dep_id,
                            Some(group_id),
                            resolve,
                            package_map,
                            node_map,
                            nodes,
                            ancestors,
                        )
                    }
                })
                .collect();

            if let Some(DependencyNode::Group(group)) = nodes.get_mut(group_id.0) {
                group.children = group_children;
            }
        };

        build_group(DependencyType::Dev, dev);
        build_group(DependencyType::Build, build);

        ancestors.remove(&package_id);
        if let Some(DependencyNode::Crate(dependency)) = nodes.get_mut(node_id.0) {
            dependency.children = children;
        }

        Some(node_id)
    }

    fn push_crate_node(
        package_id: PackageId,
        parent: Option<NodeId>,
        package_map: &HashMap<PackageId, &Package>,
        node_map: &mut HashMap<NodeKey, NodeId>,
        nodes: &mut Vec<DependencyNode>,
    ) -> Option<NodeId> {
        let key = (package_id, parent);
        if let Some(&existing) = node_map.get(&key) {
            return Some(existing);
        }

        let package = *package_map.get(&package_id)?;
        let manifest_dir = package
            .package_id()
            .source_id()
            .is_path()
            .then(|| package.root().display().to_string());
        let is_proc_macro = package.proc_macro();

        let node_id = NodeId(nodes.len());
        nodes.push(DependencyNode::Crate(Dependency {
            name: package.name().to_string(),
            version: package.version().to_string(),
            manifest_dir,
            is_proc_macro,
            parent,
            children: Vec::new(),
        }));
        node_map.insert(key, node_id);
        Some(node_id)
    }
}

fn resolve_manifest_path(gctx: &GlobalContext, manifest_path: Option<PathBuf>) -> Result<PathBuf> {
    match manifest_path {
        Some(path) if path.is_absolute() => Ok(path),
        Some(path) => Ok(gctx.cwd().join(path)),
        None => find_root_manifest_for_wd(gctx.cwd()).context("failed to find Cargo.toml"),
    }
}
