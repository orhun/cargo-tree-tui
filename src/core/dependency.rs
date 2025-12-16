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
    fn label(&self) -> String {
        match self {
            Self::Normal => "[dependencies]".to_string(),
            Self::Dev => "[dev-dependencies]".to_string(),
            Self::Build => "[build-dependencies]".to_string(),
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
    /// Type of dependency.
    pub type_: Option<DependencyType>,
    /// Whether this node is a synthetic dependency group rather than a package.
    pub is_group: bool,
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
    pub nodes: Vec<Dependency>,
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
                DependencyType::Normal,
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
    pub fn node(&self, id: NodeId) -> Option<&Dependency> {
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
        dep_type: DependencyType,
        resolve_nodes: &HashMap<&PackageId, &cargo_metadata::Node>,
        package_map: &HashMap<&PackageId, &cargo_metadata::Package>,
        node_map: &mut HashMap<(PackageId, Option<NodeId>), NodeId>,
        nodes: &mut Vec<Dependency>,
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
        nodes.push(Dependency {
            name: package.name.to_string(),
            version: package.version.to_string(),
            manifest_dir,
            is_proc_macro,
            parent,
            children: Vec::new(),
            type_: Some(dep_type),
            is_group: false,
        });
        node_map.insert(key, node_id);

        // Recursively build child nodes.
        if let Some(node) = resolve_nodes.get(package_id) {
            let mut group_nodes: HashMap<DependencyType, NodeId> = HashMap::default();
            let mut children = Vec::new();

            for dep in &node.deps {
                let next_dep_type = dep
                    .dep_kinds
                    .iter()
                    .filter_map(|kind_info| match kind_info.kind {
                        DependencyKind::Development => Some(DependencyType::Dev),
                        DependencyKind::Build => Some(DependencyType::Build),
                        DependencyKind::Normal => None,
                        DependencyKind::Unknown => None,
                    })
                    .next()
                    .unwrap_or(DependencyType::Normal);

                let parent_id = if next_dep_type == DependencyType::Normal {
                    node_id
                } else {
                    *group_nodes.entry(next_dep_type).or_insert_with(|| {
                        let group_id = NodeId(nodes.len());
                        nodes.push(Dependency {
                            name: next_dep_type.label(),
                            version: String::new(),
                            manifest_dir: None,
                            is_proc_macro: false,
                            parent: Some(node_id),
                            children: Vec::new(),
                            type_: Some(next_dep_type),
                            is_group: true,
                        });
                        children.push(group_id);
                        group_id
                    })
                };

                if let Some(child) = Self::build_dependency_node(
                    &dep.pkg,
                    Some(parent_id),
                    next_dep_type,
                    resolve_nodes,
                    package_map,
                    node_map,
                    nodes,
                ) {
                    if parent_id == node_id {
                        children.push(child);
                    } else {
                        nodes[parent_id.0].children.push(child);
                    }
                }
            }

            nodes[node_id.0].children = children;
        }

        Some(node_id)
    }
}
