use std::path::PathBuf;

use anyhow::{Context, Result};
use cargo_metadata::{
    DependencyKind, Metadata, MetadataCommand, Node, Package, PackageId, TargetKind,
};
use clap_cargo::style::{DEP_BUILD, DEP_DEV, DEP_NORMAL};
use compact_str::CompactString;
use ratatui::style::Style;
use rustc_hash::{FxHashMap, FxHashSet};

// ── Core types ───────────────────────────────────────────────────────

/// Identifier for a node within the dependency tree arena.
///
/// The `usize` represents the index into the arena vector.
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

// ── Node types ───────────────────────────────────────────────────────

/// Flat representation of a dependency node in the tree.
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

impl From<&Package> for Dependency {
    fn from(package: &Package) -> Self {
        let manifest_dir = if package.source.is_none() {
            package
                .manifest_path
                .parent()
                .map(|parent| CompactString::from(parent.as_str()))
        } else {
            None
        };

        let is_proc_macro = package.targets.iter().any(|target| {
            target
                .kind
                .iter()
                .any(|kind| matches!(kind, TargetKind::ProcMacro))
        });

        Dependency {
            name: CompactString::from(package.name.as_str()),
            version: CompactString::from(package.version.to_string()),
            manifest_dir,
            is_proc_macro,
            children: Vec::new(), // filled in by wire_edges
        }
    }
}

/// Dependency group node (e.g. `[dev-dependencies]`) within the tree.
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

/// Unified dependency node type for the tree arena.
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

// ── DependencyTree ───────────────────────────────────────────────────

/// Container for the resolved dependency tree scoped to the current workspace.
///
/// The tree is stored in an arena-like structure where each node references its children
/// by their indices. This allows for efficient traversal and manipulation of the tree.
///
/// See also [`Dependency`].
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
    /// Flat list of crate nodes for search.
    pub crate_nodes: Vec<NodeId>,
}

impl DependencyTree {
    /// Loads Cargo metadata (optionally for a specific manifest) and converts it into a
    /// [`DependencyTree`] with iteratively resolved children.
    pub fn load(manifest_path: Option<PathBuf>) -> Result<Self> {
        let mut cmd = MetadataCommand::new();

        if let Some(path) = manifest_path {
            cmd.manifest_path(path);
        }

        let metadata = cmd.exec().context("failed to execute Cargo metadata")?;

        let workspace_name = metadata
            .root_package()
            .map(|pkg| CompactString::from(pkg.name.as_str()))
            .unwrap_or_else(|| CompactString::from("workspace"));

        let deps = build_dependency_tree(&metadata)?;

        let crate_nodes: Vec<NodeId> = deps
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, node)| (!node.is_group()).then_some(NodeId(idx)))
            .collect();

        Ok(DependencyTree {
            workspace_name,
            crate_nodes,
            parents: deps.parents,
            nodes: deps.nodes,
            roots: deps.roots,
        })
    }

    /// Returns immutable access to a node identified by `id`.
    pub fn node(&self, id: NodeId) -> Option<&DependencyNode> {
        self.nodes.get(id.0)
    }

    /// Returns the first parent of the given node, or `None` for root nodes.
    ///
    /// In a deduplicated tree a node can have multiple parents; this returns
    /// the first one which is sufficient for lineage walks where the node
    /// appears at a single visible position.
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        self.parents.get(id.0).and_then(|p| p.first().copied())
    }

    /// Returns the workspace root node ids that should be rendered.
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// Returns the crate node ids that can be matched by search.
    pub fn crate_nodes(&self) -> &[NodeId] {
        &self.crate_nodes
    }
}

// ── Tree construction pipeline ───────────────────────────────────────
//
// metadata ─► resolve_metadata() ─► collect_packages() ─► wire_edges()
//                ResolvedMetadata     CollectedPackages     parents vec

struct BuildResult {
    roots: Vec<NodeId>,
    nodes: Vec<DependencyNode>,
    parents: Vec<Vec<NodeId>>,
}

fn build_dependency_tree(metadata: &Metadata) -> Result<BuildResult> {
    let resolved = resolve_metadata(metadata)?;
    let mut collected = collect_packages(&resolved);
    let parents = wire_edges(&resolved, &collected.pkg_index, &mut collected.nodes);

    Ok(BuildResult {
        roots: collected.roots,
        nodes: collected.nodes,
        parents,
    })
}

// ── Step 1: resolve_metadata ─────────────────────────────────────────

/// Resolved cargo metadata ready for tree construction.
struct ResolvedMetadata<'a> {
    packages: FxHashMap<&'a PackageId, &'a Package>,
    resolve_nodes: FxHashMap<&'a PackageId, &'a Node>,
    workspace_ids: Vec<PackageId>,
}

/// Validate the resolve graph and build lookup maps from cargo metadata.
fn resolve_metadata(metadata: &Metadata) -> Result<ResolvedMetadata<'_>> {
    let resolve = metadata
        .resolve
        .as_ref()
        .context("failed to resolve dependency graph")?;

    let packages = metadata.packages.iter().map(|pkg| (&pkg.id, pkg)).collect();

    let resolve_nodes = resolve.nodes.iter().map(|node| (&node.id, node)).collect();

    let workspace_members: FxHashSet<&PackageId> = metadata.workspace_members.iter().collect();
    let workspace_ids = metadata
        .packages
        .iter()
        .rev()
        .filter(|pkg| workspace_members.contains(&pkg.id))
        .map(|pkg| pkg.id.clone())
        .collect();

    Ok(ResolvedMetadata {
        packages,
        resolve_nodes,
        workspace_ids,
    })
}

// ── Step 2: collect_packages ─────────────────────────────────────────

/// The node arena with empty children, a package-to-node index, and root ids.
struct CollectedPackages {
    nodes: Vec<DependencyNode>,
    pkg_index: FxHashMap<PackageId, NodeId>,
    roots: Vec<NodeId>,
}

/// BFS from workspace roots: create one `DependencyNode::Crate` per reachable
/// package. Nodes have empty children at this point.
fn collect_packages(resolved: &ResolvedMetadata<'_>) -> CollectedPackages {
    let capacity = resolved.packages.len();
    let mut remaining: Vec<&PackageId> = Vec::with_capacity(capacity);
    remaining.extend(resolved.workspace_ids.iter());

    let mut nodes: Vec<DependencyNode> = Vec::with_capacity(capacity);
    let mut pkg_index: FxHashMap<PackageId, NodeId> =
        FxHashMap::with_capacity_and_hasher(capacity, Default::default());

    while let Some(package_id) = remaining.pop() {
        if pkg_index.contains_key(package_id) {
            continue;
        }

        let package = resolved.packages[package_id];

        let node_id = NodeId(nodes.len());
        nodes.push(DependencyNode::Crate(Dependency::from(package)));
        pkg_index.insert(package_id.clone(), node_id);

        if let Some(node) = resolved.resolve_nodes.get(package_id) {
            remaining.extend(node.deps.iter().map(|dep| &dep.pkg));
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

// ── Step 3: wire_edges ───────────────────────────────────────────────

/// Second pass: classify each crate's deps by kind, attach normal deps as
/// direct children, create group nodes for dev/build deps, and build the
/// parents reverse-index.
fn wire_edges(
    resolved: &ResolvedMetadata<'_>,
    pkg_index: &FxHashMap<PackageId, NodeId>,
    nodes: &mut Vec<DependencyNode>,
) -> Vec<Vec<NodeId>> {
    let mut parents: Vec<Vec<NodeId>> = vec![Vec::new(); nodes.len()];
    let mut classified = ClassifiedDeps::default();

    for (pkg_id, &node_id) in pkg_index.iter() {
        let Some(resolved_node) = resolved.resolve_nodes.get(pkg_id) else {
            continue;
        };

        classified.clear();
        classified.populate(resolved_node, pkg_index);

        let mut children: Vec<NodeId> = Vec::with_capacity(
            classified.normal.len() + classified.has_dev as usize + classified.has_build as usize,
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
    has_dev: bool,
    has_build: bool,
}

impl ClassifiedDeps {
    fn clear(&mut self) {
        self.normal.clear();
        self.dev.clear();
        self.build.clear();
        self.has_dev = false;
        self.has_build = false;
    }

    /// Classify a resolved node's dependencies into normal, dev, and build buckets.
    fn populate(&mut self, resolved_node: &Node, pkg_index: &FxHashMap<PackageId, NodeId>) {
        for dep in &resolved_node.deps {
            let dep_type = dep
                .dep_kinds
                .iter()
                .find_map(|kind| DependencyType::try_from(kind.kind).ok());

            let Some(&child_id) = pkg_index.get(&dep.pkg) else {
                continue;
            };

            match dep_type {
                Some(DependencyType::Normal) => self.normal.push(child_id),
                Some(DependencyType::Dev) => {
                    self.dev.push(child_id);
                    self.has_dev = true;
                }
                Some(DependencyType::Build) => {
                    self.build.push(child_id);
                    self.has_build = true;
                }
                None => {}
            }
        }
    }
}
