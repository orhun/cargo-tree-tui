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
use cargo_util::paths::normalize_path;
use clap_cargo::style::{DEP_BUILD, DEP_DEV, DEP_NORMAL};
use compact_str::CompactString;
use ratatui::style::Style;
use rustc_hash::FxHashMap;

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
    pub fn label(&self) -> &'static str {
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

impl From<DepKind> for DependencyType {
    fn from(value: DepKind) -> Self {
        match value {
            DepKind::Normal => Self::Normal,
            DepKind::Development => Self::Dev,
            DepKind::Build => Self::Build,
        }
    }
}

/// Flat representation of a dependency node in the deduplicated tree.
///
/// See [`DependencyTree`] for the full tree structure.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Crate name.
    pub name: CompactString,
    /// Crate version.
    pub version: CompactString,
    /// Local manifest directory (only for workspace members).
    pub manifest_dir: Option<CompactString>,
    /// Whether this crate exposes a proc-macro target.
    pub is_proc_macro: bool,
    /// Children represented as node indices for downward traversal.
    pub children: Vec<NodeId>,
}

impl From<&PackageSnapshot> for Dependency {
    fn from(snapshot: &PackageSnapshot) -> Self {
        Dependency {
            name: snapshot.name.clone(),
            version: snapshot.version.clone(),
            manifest_dir: snapshot.manifest_dir.clone(),
            is_proc_macro: snapshot.is_proc_macro,
            children: Vec::new(), // filled in by wire_edges
        }
    }
}

/// Dependency group node (e.g. `[dev-dependencies]`) within the deduplicated tree.
#[derive(Debug, Clone)]
pub struct DependencyGroup {
    /// Group kind in Cargo metadata.
    pub kind: DependencyType,
    /// Children represented as node indices for downward traversal.
    pub children: Vec<NodeId>,
}

impl DependencyGroup {
    pub fn label(&self) -> &'static str {
        self.kind.label()
    }
}

/// Unified dependency node type for the deduplicated tree arena.
#[derive(Debug, Clone)]
pub enum DependencyNode {
    Crate(Dependency),
    Group(DependencyGroup),
}

impl DependencyNode {
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

/// Deduplicated dependency tree: one arena node per unique package.
///
/// Parent relationships are stored in a separate reverse-index rather than
/// on each node, since a deduplicated node can have multiple parents.
#[derive(Debug, Clone)]
pub struct DependencyTree {
    /// Name of the root package (or workspace placeholder when missing).
    pub workspace_name: CompactString,
    /// Arena storing all dependency nodes.
    pub nodes: Vec<DependencyNode>,
    /// For each node, the list of parent node ids (reverse index of children).
    pub parents: Vec<Vec<NodeId>>,
    /// Workspace members represented as node ids (entry points into the arena).
    pub roots: Vec<NodeId>,
}

impl DependencyTree {
    /// Resolves the Cargo workspace via the `cargo` library and converts the
    /// resolved graph into a [`DependencyTree`].
    pub fn load(manifest_path: Option<PathBuf>) -> Result<Self> {
        let resolved = ResolvedWorkspace::load(manifest_path)?;
        let workspace_name = resolved.workspace_name.clone();
        let deps = build_dependency_tree(&resolved);

        Ok(DependencyTree {
            workspace_name,
            parents: deps.parents,
            nodes: deps.nodes,
            roots: deps.roots,
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
    pub fn crate_nodes(&self) -> impl Iterator<Item = NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, node)| (!node.is_group()).then_some(NodeId(idx)))
    }
}

/// Snapshot of a Cargo package with the fields required fields.
pub struct PackageSnapshot {
    name: CompactString,
    version: CompactString,
    manifest_dir: Option<CompactString>,
    is_proc_macro: bool,
}

impl PackageSnapshot {
    fn from_package(package: &Package) -> Self {
        let manifest_dir = package
            .package_id()
            .source_id()
            .is_path()
            .then(|| package.root().display().to_string().into());

        Self {
            name: package.name().as_str().into(),
            version: package.version().to_string().into(),
            manifest_dir,
            is_proc_macro: package.proc_macro(),
        }
    }
}

/// Resolved Cargo workspace with the data required to build the dependency tree.
struct ResolvedWorkspace {
    workspace_name: CompactString,
    packages: FxHashMap<PackageId, PackageSnapshot>,
    /// Deduplicated, classified outgoing edges keyed by source package.
    edges: FxHashMap<PackageId, Vec<(PackageId, DependencyType)>>,
    workspace_ids: Vec<PackageId>,
}

impl ResolvedWorkspace {
    fn load(manifest_path: Option<PathBuf>) -> Result<Self> {
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
            .map(|pkg| pkg.name().as_str().into())
            .unwrap_or_else(|| "workspace".into());

        // Snapshot every reachable package: workspace members first (so a
        // member that also appears in pkg_set keeps its workspace identity),
        // then everything else from the resolved package set.
        let mut packages: FxHashMap<PackageId, PackageSnapshot> = FxHashMap::default();
        for pkg in ws.members() {
            packages.insert(pkg.package_id(), PackageSnapshot::from_package(pkg));
        }
        for pkg in pkg_set.packages() {
            packages
                .entry(pkg.package_id())
                .or_insert_with(|| PackageSnapshot::from_package(pkg));
        }

        // Build classified, kind-deduplicated edges keyed by source package.
        let mut edges: FxHashMap<PackageId, Vec<(PackageId, DependencyType)>> =
            FxHashMap::default();
        for &pkg_id in packages.keys() {
            let mut classified: Vec<(PackageId, DependencyType)> = Vec::new();
            for (dep_id, deps) in resolve.deps(pkg_id) {
                let mut seen_normal = false;
                let mut seen_dev = false;
                let mut seen_build = false;
                for dep in deps.iter() {
                    let kind = DependencyType::from(dep.kind());
                    let already = match kind {
                        DependencyType::Normal => std::mem::replace(&mut seen_normal, true),
                        DependencyType::Dev => std::mem::replace(&mut seen_dev, true),
                        DependencyType::Build => std::mem::replace(&mut seen_build, true),
                    };
                    if !already {
                        classified.push((dep_id, kind));
                    }
                }
            }
            edges.insert(pkg_id, classified);
        }

        let workspace_ids = ws.members().map(|pkg| pkg.package_id()).collect();

        Ok(ResolvedWorkspace {
            workspace_name,
            packages,
            edges,
            workspace_ids,
        })
    }
}

fn resolve_manifest_path(gctx: &GlobalContext, manifest_path: Option<PathBuf>) -> Result<PathBuf> {
    let raw = match manifest_path {
        Some(path) if path.is_absolute() => path,
        Some(path) => gctx.cwd().join(path),
        None => find_root_manifest_for_wd(gctx.cwd()).context("failed to find Cargo.toml")?,
    };
    // Cargo's `Workspace::new` compares manifest paths against the normalized
    // paths it discovers via filesystem walks. Without lexical normalization,
    // an input like `../zed/Cargo.toml` produces `.../cwd/../zed/Cargo.toml`
    // and trips `validate_members` with a "wrong workspace" error.
    Ok(normalize_path(&raw))
}

struct BuildResult {
    roots: Vec<NodeId>,
    nodes: Vec<DependencyNode>,
    parents: Vec<Vec<NodeId>>,
}

fn build_dependency_tree(resolved: &ResolvedWorkspace) -> BuildResult {
    let mut collected = collect_packages(resolved);
    let parents = wire_edges(resolved, &collected.pkg_index, &mut collected.nodes);

    BuildResult {
        roots: collected.roots,
        nodes: collected.nodes,
        parents,
    }
}

/// The node arena with empty children, a package-to-node index, and root ids.
struct CollectedPackages {
    nodes: Vec<DependencyNode>,
    pkg_index: FxHashMap<PackageId, NodeId>,
    roots: Vec<NodeId>,
}

/// Create one `DependencyNode::Crate` per reachable package.
///
/// DFS from workspace roots. Nodes have empty children at this point.
fn collect_packages(resolved: &ResolvedWorkspace) -> CollectedPackages {
    let capacity = resolved.packages.len();
    let mut remaining: Vec<PackageId> = Vec::with_capacity(capacity);
    remaining.extend(resolved.workspace_ids.iter().copied());

    let mut nodes: Vec<DependencyNode> = Vec::with_capacity(capacity);
    let mut pkg_index: FxHashMap<PackageId, NodeId> =
        FxHashMap::with_capacity_and_hasher(capacity, Default::default());

    while let Some(package_id) = remaining.pop() {
        if pkg_index.contains_key(&package_id) {
            continue;
        }

        let Some(snapshot) = resolved.packages.get(&package_id) else {
            continue;
        };

        let node_id = NodeId(nodes.len());
        nodes.push(DependencyNode::Crate(Dependency::from(snapshot)));
        pkg_index.insert(package_id, node_id);

        if let Some(deps) = resolved.edges.get(&package_id) {
            remaining.extend(deps.iter().map(|(dep_id, _)| *dep_id));
        }
    }

    let roots = resolved
        .workspace_ids
        .iter()
        .filter_map(|pkg_id| pkg_index.get(pkg_id).copied())
        .collect();

    CollectedPackages {
        nodes,
        pkg_index,
        roots,
    }
}

/// Classify each crate's deps by kind.
///
/// Attaches normal deps as direct children, creates group nodes for dev/build
/// deps, and builds the parents reverse-index.
fn wire_edges(
    resolved: &ResolvedWorkspace,
    pkg_index: &FxHashMap<PackageId, NodeId>,
    nodes: &mut Vec<DependencyNode>,
) -> Vec<Vec<NodeId>> {
    let mut parents: Vec<Vec<NodeId>> = vec![Vec::new(); nodes.len()];

    for (pkg_id, &node_id) in pkg_index.iter() {
        let Some(edges) = resolved.edges.get(pkg_id) else {
            continue;
        };

        let mut classified = ClassifiedDeps::populate(edges, pkg_index);
        let mut children: Vec<NodeId> = Vec::with_capacity(
            classified.normal.len()
                + classified.has_dev() as usize  // expanded as group, so one child
                + classified.has_build() as usize,
        );

        // Normal deps are direct children of the crate node.
        for &child_id in &classified.normal {
            children.push(child_id);
            parents[child_id.0].push(node_id);
        }

        // Dev and build deps go under group nodes.
        for (kind, group_deps) in [
            (DependencyType::Dev, &mut classified.dev),
            (DependencyType::Build, &mut classified.build),
        ] {
            if group_deps.is_empty() {
                continue;
            }

            let group_id = NodeId(nodes.len());
            for &child_id in group_deps.iter() {
                parents[child_id.0].push(group_id);
            }

            nodes.push(DependencyNode::Group(DependencyGroup {
                kind,
                children: std::mem::take(group_deps),
            }));

            parents.push(vec![node_id]);
            children.push(group_id);
        }

        if let Some(DependencyNode::Crate(dep)) = nodes.get_mut(node_id.0) {
            dep.children = children;
        }
    }

    parents
}

#[derive(Default)]
struct ClassifiedDeps {
    normal: Vec<NodeId>,
    dev: Vec<NodeId>,
    build: Vec<NodeId>,
}

impl ClassifiedDeps {
    /// Classify a package's edges into normal, dev, and build buckets.
    fn populate(
        edges: &[(PackageId, DependencyType)],
        pkg_index: &FxHashMap<PackageId, NodeId>,
    ) -> Self {
        let mut classified = ClassifiedDeps::default();

        for &(dep_id, kind) in edges {
            let Some(&child_id) = pkg_index.get(&dep_id) else {
                continue;
            };

            match kind {
                DependencyType::Normal => classified.normal.push(child_id),
                DependencyType::Dev => classified.dev.push(child_id),
                DependencyType::Build => classified.build.push(child_id),
            }
        }

        classified
    }

    fn has_dev(&self) -> bool {
        !self.dev.is_empty()
    }

    fn has_build(&self) -> bool {
        !self.build.is_empty()
    }
}
