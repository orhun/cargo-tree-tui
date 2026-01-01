use std::path::PathBuf;

use anyhow::{Context, Result};
use cargo_metadata::{DependencyKind, MetadataCommand, Node, Package, PackageId, TargetKind};
use ratatui::style::{Color, Style};
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
            Self::Normal => Style::default(),
            Self::Dev => Style::default().fg(Color::Magenta),
            Self::Build => Style::default().fg(Color::Blue),
        }
    }
}

impl TryFrom<DependencyKind> for DependencyType {
    type Error = ();
    fn try_from(value: DependencyKind) -> Result<Self, Self::Error> {
        match value {
            DependencyKind::Normal => Ok(Self::Normal),
            DependencyKind::Development => Ok(Self::Dev),
            DependencyKind::Build => Ok(Self::Build),
            _ => Err(()),
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
}

impl DependencyTree {
    /// Loads Cargo metadata (optionally for a specific manifest) and converts it into a
    /// [`DependencyTree`] with recursively resolved children.
    pub fn load(manifest_path: Option<PathBuf>) -> Result<Self> {
        let mut cmd = MetadataCommand::new();

        if let Some(path) = manifest_path {
            cmd.manifest_path(path);
        }

        let metadata = cmd.exec().context("failed to execute Cargo metadata")?;

        let workspace_name = metadata
            .root_package()
            .map(|pkg| pkg.name.to_string())
            .unwrap_or_else(|| "workspace".to_string());

        let resolve = metadata
            .resolve
            .as_ref()
            .context("failed to resolve dependency graph")?;

        // Create maps for easy lookup.
        let package_map: HashMap<&PackageId, &Package> =
            metadata.packages.iter().map(|pkg| (&pkg.id, pkg)).collect();

        // Map of [`PackageId`] to [`Node`] for quick access during tree construction.
        let resolve_nodes: HashMap<&PackageId, &Node> =
            resolve.nodes.iter().map(|node| (&node.id, node)).collect();

        // Main tree construction (these types will be filled in during recursion).
        let mut nodes = Vec::new();
        let mut roots = Vec::new();
        let mut node_map: HashMap<NodeKey, NodeId> = HashMap::default();

        // For only including packages that belong to the current workspace
        // to avoid third-party crates.
        let workspace_package_ids: HashSet<PackageId> = metadata
            .workspace_members
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        // Recursively build dependency nodes for each workspace member.
        for package in metadata
            .packages
            .iter()
            .filter(|pkg| workspace_package_ids.contains(&pkg.id))
        {
            if let Some(dependency) = Self::build_dependency_node(
                &package.id,
                None,
                &resolve_nodes,
                &package_map,
                &mut node_map,
                &mut nodes,
            ) {
                roots.push(dependency);
            }
        }

        Ok(DependencyTree {
            workspace_name,
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

    /// Recursively constructs dependency nodes.
    ///
    /// # Notes
    ///
    /// - Each [`NodeKey`] is stored in `node_map` to avoid duplicating nodes.
    /// - The `parent` parameter allows tracking the parent node during recursion.
    fn build_dependency_node(
        package_id: &PackageId,
        parent: Option<NodeId>,
        resolve_nodes: &HashMap<&PackageId, &cargo_metadata::Node>,
        package_map: &HashMap<&PackageId, &cargo_metadata::Package>,
        node_map: &mut HashMap<(PackageId, Option<NodeId>), NodeId>,
        nodes: &mut Vec<DependencyNode>,
    ) -> Option<NodeId> {
        // Avoid duplicating nodes by checking the map first.
        let key = (package_id.clone(), parent);
        if let Some(&existing) = node_map.get(&key) {
            return Some(existing);
        }

        // Retrieve package information.
        let package = *package_map.get(package_id)?;
        let manifest_dir = if package.source.is_none() {
            package
                .manifest_path
                .parent()
                .map(|parent| parent.to_string())
        } else {
            None
        };
        let is_proc_macro = package.targets.iter().any(|target| {
            target
                .kind
                .iter()
                .any(|kind| matches!(kind, TargetKind::ProcMacro))
        });

        // Create the new dependency node.
        let node_id = NodeId(nodes.len());
        nodes.push(DependencyNode::Crate(Dependency {
            name: package.name.to_string(),
            version: package.version.to_string(),
            manifest_dir,
            is_proc_macro,
            parent,
            children: Vec::new(),
        }));
        node_map.insert(key, node_id);

        // Recursively build child nodes.
        let Some(node) = resolve_nodes.get(package_id) else {
            return Some(node_id);
        };

        let mut normal = Vec::new();
        let mut dev = Vec::new();
        let mut build = Vec::new();

        for dep in &node.deps {
            let dep_type = dep
                .dep_kinds
                .iter()
                .find_map(|kind| DependencyType::try_from(kind.kind).ok());
            match dep_type {
                Some(DependencyType::Normal) => normal.push(&dep.pkg),
                Some(DependencyType::Dev) => dev.push(&dep.pkg),
                Some(DependencyType::Build) => build.push(&dep.pkg),
                None => {}
            }
        }

        let mut children = Vec::new();
        for dep_id in normal {
            if let Some(child) = Self::build_dependency_node(
                dep_id,
                Some(node_id),
                resolve_nodes,
                package_map,
                node_map,
                nodes,
            ) {
                children.push(child);
            }
        }

        let mut build_group = |kind: DependencyType, deps: Vec<&PackageId>| {
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

            let children = deps
                .into_iter()
                .filter_map(|dep_id| {
                    Self::build_dependency_node(
                        dep_id,
                        Some(group_id),
                        resolve_nodes,
                        package_map,
                        node_map,
                        nodes,
                    )
                })
                .collect();

            if let Some(DependencyNode::Group(group)) = nodes.get_mut(group_id.0) {
                group.children = children;
            }
        };

        build_group(DependencyType::Dev, dev);
        build_group(DependencyType::Build, build);

        if let Some(DependencyNode::Crate(dependency)) = nodes.get_mut(node_id.0) {
            dependency.children = children;
        }

        Some(node_id)
    }
}
