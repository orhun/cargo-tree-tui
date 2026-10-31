use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result, bail};
use cargo::{
    GlobalContext,
    core::{
        Package, PackageId, PackageIdSpec, PackageIdSpecQuery, Workspace,
        compiler::{CompileKind, RustcTargetData},
        dependency::DepKind,
        resolver::features::{CliFeatures, ForceAllTargets, HasDevUnits},
    },
    ops,
    util::important_paths::find_root_manifest_for_wd,
};
use cargo_util::paths::normalize_path;
use clap_cargo::style::{DEP_BUILD, DEP_DEV, DEP_NORMAL};
use ratatui::style::Style;
use rustc_hash::{FxHashMap, FxHashSet};

/// Identifier for a node within the dependency tree arena.
///
/// The `usize` represents the index into the arena vector.
/// This is used for efficient storage and traversal of the tree structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    pub name: String,
    /// Crate version.
    pub version: String,
    /// Local manifest directory (only for workspace members).
    pub manifest_dir: Option<String>,
    /// Whether this crate exposes a proc-macro target.
    pub is_proc_macro: bool,
    /// Metadata requested by the package display format.
    pub format_metadata: Option<Arc<PackageFormatMetadata>>,
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
            format_metadata: snapshot.format_metadata.clone(),
            children: Vec::new(), // filled in by wire_edges
        }
    }
}

/// Package metadata fields needed by a Cargo-style tree format.
#[derive(Debug, Clone, Copy, Default)]
pub struct PackageFormatRequirements {
    pub license: bool,
    pub repository: bool,
    pub features: bool,
    pub library: bool,
    pub source: bool,
}

/// Optional Cargo package values used by a formatted tree label.
#[derive(Debug, Clone, Default)]
pub struct PackageFormatMetadata {
    pub license: Option<String>,
    pub repository: Option<String>,
    pub features: Option<String>,
    pub library: Option<String>,
    pub source: Option<String>,
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
///
/// Example:
///
/// app
/// |- foo
/// |  `- baz
/// `- bar
///    `- baz
///
/// nodes:
///   0 = app(children = [1, 2])
///   1 = foo(children = [3])
///   2 = bar(children = [3])
///   3 = baz(children = [])
///
/// parents:
///   0 -> []
///   1 -> [0]
///   2 -> [0]
///   3 -> [1, 2]
#[derive(Debug, Clone)]
pub struct DependencyTree {
    /// Name of the root package (or workspace placeholder when missing).
    pub workspace_name: String,
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
        Self::load_with_options(manifest_path, &ResolutionOptions::default())
    }

    /// Resolves the Cargo workspace using the supplied Cargo resolver options.
    pub fn load_with_options(
        manifest_path: Option<PathBuf>,
        options: &ResolutionOptions,
    ) -> Result<Self> {
        Self::load_with_options_and_format(
            manifest_path,
            options,
            PackageFormatRequirements::default(),
        )
    }

    /// Resolves the workspace and collects only the metadata requested by the
    /// package display format.
    pub fn load_with_options_and_format(
        manifest_path: Option<PathBuf>,
        options: &ResolutionOptions,
        format_requirements: PackageFormatRequirements,
    ) -> Result<Self> {
        let resolved = ResolvedWorkspace::load(manifest_path, options, format_requirements)?;
        let workspace_name = resolved.workspace_name.clone();
        let mut collected = collect_packages(&resolved);
        let parents = wire_edges(&resolved, &collected.pkg_index, &mut collected.nodes);
        let tree = DependencyTree {
            workspace_name,
            parents,
            nodes: collected.nodes,
            roots: collected.roots,
        };
        Ok(tree)
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

/// Cargo resolver options that affect which packages and dependency edges are
/// present in the dependency tree.
#[derive(Debug, Default)]
pub struct ResolutionOptions {
    /// Resolve the tree for all workspace members.
    pub workspace: bool,
    /// Resolve the tree for the selected packages.
    pub packages: Vec<String>,
    /// Exclude workspace members when `workspace` is enabled.
    pub exclude: Vec<String>,
    /// Dependency kinds to retain in the tree.
    pub edges: Vec<String>,
    /// Deprecated alias for excluding development dependencies.
    pub no_dev_dependencies: bool,
    /// Packages to omit wherever they occur as dependencies.
    pub prune: Vec<String>,
    /// Packages whose reverse dependencies should be displayed.
    pub invert: Option<Vec<String>>,
    /// Show packages that occur with multiple versions and their reverse dependencies.
    pub duplicates: bool,
    /// Maximum dependency depth to expand initially in the TUI.
    pub depth: Option<usize>,
    /// Explicit target triples. The special value `all` is handled like
    /// `--all-targets`.
    pub targets: Vec<String>,
    /// Resolve dependencies for every target, including tests and examples.
    pub all_targets: bool,
    /// Features requested on the command line.
    pub features: Vec<String>,
    /// Activate every available feature.
    pub all_features: bool,
    /// Do not activate the default feature.
    pub no_default_features: bool,
}

#[derive(Debug, Clone, Copy)]
struct EdgeKinds {
    normal: bool,
    build: bool,
    dev: bool,
}

impl EdgeKinds {
    fn from_options(options: &ResolutionOptions) -> Result<Self> {
        let mut kinds: Vec<&str> = options.edges.iter().map(String::as_str).collect();
        if options.no_dev_dependencies {
            kinds.push("no-dev");
        }

        for kind in &kinds {
            if matches!(*kind, "all" | "features" | "public" | "no-proc-macro") {
                bail!("`--edges={kind}` is not supported by cargo-tree-tui yet");
            }
        }

        if kinds.is_empty() {
            return Ok(Self::all());
        }

        let has_negative = kinds.iter().any(|kind| kind.starts_with("no-"));
        if has_negative {
            if kinds
                .iter()
                .any(|kind| matches!(*kind, "all" | "normal" | "build" | "dev"))
            {
                bail!("dependency kinds cannot be mixed with `no-*` dependency kinds");
            }

            let mut selected = Self::all();
            for kind in kinds {
                match kind {
                    "no-normal" => selected.normal = false,
                    "no-build" => selected.build = false,
                    "no-dev" => selected.dev = false,
                    _ => bail!("invalid dependency kind `{kind}`"),
                }
            }
            return Ok(selected);
        }

        let mut selected = Self {
            normal: false,
            build: false,
            dev: false,
        };
        for kind in kinds {
            match kind {
                "normal" => selected.normal = true,
                "build" => selected.build = true,
                "dev" => selected.dev = true,
                _ => bail!("invalid dependency kind `{kind}`"),
            }
        }
        Ok(selected)
    }

    fn all() -> Self {
        Self {
            normal: true,
            build: true,
            dev: true,
        }
    }

    fn includes(self, kind: DependencyType) -> bool {
        match kind {
            DependencyType::Normal => self.normal,
            DependencyType::Build => self.build,
            DependencyType::Dev => self.dev,
        }
    }
}

/// Snapshot of a Cargo package with the fields required fields.
pub struct PackageSnapshot {
    name: String,
    version: String,
    manifest_dir: Option<String>,
    is_proc_macro: bool,
    format_metadata: Option<Arc<PackageFormatMetadata>>,
}

impl PackageSnapshot {
    fn from_package(
        package: &Package,
        features: Option<String>,
        format_requirements: PackageFormatRequirements,
    ) -> Self {
        let package_id = package.package_id();
        let source_id = package_id.source_id();
        let manifest_dir = package
            .package_id()
            .source_id()
            .is_path()
            .then(|| package.root().display().to_string());
        let metadata = package.manifest().metadata();
        let format_metadata = PackageFormatMetadata {
            license: format_requirements
                .license
                .then(|| metadata.license.clone())
                .flatten(),
            repository: format_requirements
                .repository
                .then(|| metadata.repository.clone())
                .flatten(),
            features,
            library: format_requirements
                .library
                .then(|| {
                    package
                        .targets()
                        .iter()
                        .find(|target| target.is_lib())
                        .map(|target| target.crate_name().to_string())
                })
                .flatten(),
            source: (format_requirements.source && !source_id.is_crates_io())
                .then(|| source_id.to_string()),
        };
        let format_metadata = (format_metadata.license.is_some()
            || format_metadata.repository.is_some()
            || format_metadata.features.is_some()
            || format_metadata.library.is_some()
            || format_metadata.source.is_some())
        .then(|| Arc::new(format_metadata));

        Self {
            name: package.name().as_str().to_owned(),
            version: package.version().to_string(),
            manifest_dir,
            is_proc_macro: package.proc_macro(),
            format_metadata,
        }
    }
}

/// Resolved Cargo workspace with the data required to build the dependency tree.
struct ResolvedWorkspace {
    workspace_name: String,
    packages: FxHashMap<PackageId, PackageSnapshot>,
    /// Deduplicated, classified outgoing edges keyed by source package.
    edges: FxHashMap<PackageId, Vec<(PackageId, DependencyType)>>,
    root_ids: Vec<PackageId>,
}

impl ResolvedWorkspace {
    /// Resolve a Cargo workspace into the minimal data needed to build the
    /// deduplicated dependency tree.
    ///
    /// This loads the workspace through Cargo's own resolver, snapshots each
    /// reachable package into a compact [`PackageSnapshot`], classifies outgoing
    /// edges by dependency kind, and records the workspace member ids that act
    /// as graph roots.
    fn load(
        manifest_path: Option<PathBuf>,
        options: &ResolutionOptions,
        format_requirements: PackageFormatRequirements,
    ) -> Result<Self> {
        let gctx = GlobalContext::default().context("failed to initialize Cargo context")?;
        let manifest_path = resolve_manifest_path(&gctx, manifest_path)?;
        let ws = Workspace::new(&manifest_path, &gctx).context("failed to load Cargo workspace")?;

        let all_targets =
            options.all_targets || (options.targets.len() == 1 && options.targets[0] == "all");
        let requested_targets = if all_targets {
            &[]
        } else {
            options.targets.as_slice()
        };
        let requested_kinds = CompileKind::from_requested_targets(ws.gctx(), requested_targets)
            .context("failed to determine Cargo target kinds")?;

        let mut target_data =
            RustcTargetData::new(&ws, &requested_kinds).context("failed to load target data")?;
        let packages = ops::Packages::from_flags(
            options.workspace,
            options.exclude.clone(),
            options.packages.clone(),
        )
        .context("failed to select Cargo packages")?;
        let specs = packages
            .to_package_id_specs(&ws)
            .context("failed to resolve selected package specs")?;
        let edge_kinds = EdgeKinds::from_options(options)?;
        let prune_specs = options
            .prune
            .iter()
            .map(|spec| {
                PackageIdSpec::parse(spec)
                    .with_context(|| format!("invalid package spec to prune: `{spec}`"))
            })
            .collect::<Result<Vec<_>>>()?;
        let cli_features = CliFeatures::from_command_line(
            &options.features,
            options.all_features,
            !options.no_default_features,
        )
        .context("failed to parse Cargo features")?;
        let ws_resolve = ops::resolve_ws_with_opts(
            &ws,
            &mut target_data,
            &requested_kinds,
            &cli_features,
            &specs,
            if edge_kinds.dev {
                HasDevUnits::Yes
            } else {
                HasDevUnits::No
            },
            if all_targets {
                ForceAllTargets::Yes
            } else {
                ForceAllTargets::No
            },
            false,
        )
        .context("failed to resolve Cargo dependencies")?;

        let pkg_set = ws_resolve.pkg_set;
        let resolve = ws_resolve.targeted_resolve;

        let workspace_name = ws
            .current_opt()
            .map(|pkg| pkg.name().as_str().to_owned())
            .unwrap_or_else(|| "workspace".to_owned());

        // Snapshot every reachable package: workspace members first (so a
        // member that also appears in pkg_set keeps its workspace identity),
        // then everything else from the resolved package set.
        let mut packages: FxHashMap<PackageId, PackageSnapshot> = FxHashMap::default();
        for pkg in ws.members() {
            let features = format_requirements
                .features
                .then(|| {
                    resolve
                        .features(pkg.package_id())
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .filter(|features| !features.is_empty());
            packages.insert(
                pkg.package_id(),
                PackageSnapshot::from_package(pkg, features, format_requirements),
            );
        }
        for pkg in pkg_set.packages() {
            packages.entry(pkg.package_id()).or_insert_with(|| {
                let features = format_requirements
                    .features
                    .then(|| {
                        resolve
                            .features(pkg.package_id())
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .filter(|features| !features.is_empty());
                PackageSnapshot::from_package(pkg, features, format_requirements)
            });
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
                    if !edge_kinds.includes(kind)
                        || prune_specs.iter().any(|spec| spec.matches(dep_id))
                    {
                        continue;
                    }
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

        let selected_roots: Vec<PackageId> = ws
            .members()
            .filter_map(|pkg| {
                let package_id = pkg.package_id();
                specs
                    .iter()
                    .any(|spec| spec.matches(package_id))
                    .then_some(package_id)
            })
            .collect();

        let selected_reachable = reachable_packages(&selected_roots, &edges);
        let invert_specs = invert_package_specs(options)?;
        let invert_roots = if invert_specs.is_empty() {
            Vec::new()
        } else {
            packages_matching_specs(&invert_specs, &selected_reachable)?
        };

        let (tree_roots, graph_roots, inverted) = if options.duplicates {
            let graph_roots = if invert_roots.is_empty() {
                selected_roots.clone()
            } else {
                invert_roots
            };
            let reachable = reachable_packages(&graph_roots, &edges);
            let mut duplicates = duplicate_packages(&reachable);
            if duplicates.is_empty() {
                bail!("no duplicate packages found in the selected dependency tree");
            }
            duplicates.sort_unstable();
            (duplicates, graph_roots, true)
        } else if options.invert.is_some() {
            if invert_roots.is_empty() {
                bail!("`--invert` requires a package spec or `--package`");
            }
            (invert_roots, selected_roots.clone(), true)
        } else {
            (selected_roots.clone(), selected_roots, false)
        };

        let graph_scope = reachable_packages(&graph_roots, &edges);
        edges.retain(|package_id, dependencies| {
            if !graph_scope.contains(package_id) {
                return false;
            }
            dependencies.retain(|(dependency_id, _)| graph_scope.contains(dependency_id));
            true
        });

        if inverted {
            edges = invert_edges(&edges);
        }

        Ok(ResolvedWorkspace {
            workspace_name,
            packages,
            edges,
            root_ids: tree_roots,
        })
    }
}

fn invert_package_specs(options: &ResolutionOptions) -> Result<Vec<String>> {
    let Some(specs) = &options.invert else {
        return Ok(Vec::new());
    };
    if !specs.is_empty() {
        return Ok(specs.clone());
    }
    if options.packages.is_empty() {
        bail!("`--invert` requires a package spec or `--package`");
    }
    Ok(options.packages.clone())
}

fn packages_matching_specs(
    spec_values: &[String],
    candidates: &FxHashSet<PackageId>,
) -> Result<Vec<PackageId>> {
    let mut matched = FxHashSet::default();
    for value in spec_values {
        let spec = PackageIdSpec::parse(value)
            .with_context(|| format!("invalid package spec to invert: `{value}`"))?;
        let mut found = false;
        for &package_id in candidates {
            if spec.matches(package_id) {
                matched.insert(package_id);
                found = true;
            }
        }
        if !found {
            bail!("package spec `{value}` is not in the selected dependency tree");
        }
    }
    let mut matched: Vec<_> = matched.into_iter().collect();
    matched.sort_unstable();
    Ok(matched)
}

fn reachable_packages(
    roots: &[PackageId],
    edges: &FxHashMap<PackageId, Vec<(PackageId, DependencyType)>>,
) -> FxHashSet<PackageId> {
    let mut reachable = FxHashSet::default();
    let mut pending = roots.to_vec();
    while let Some(package_id) = pending.pop() {
        if !reachable.insert(package_id) {
            continue;
        }
        if let Some(dependencies) = edges.get(&package_id) {
            pending.extend(dependencies.iter().map(|(dependency_id, _)| *dependency_id));
        }
    }
    reachable
}

fn duplicate_packages(reachable: &FxHashSet<PackageId>) -> Vec<PackageId> {
    let mut packages_by_name: FxHashMap<String, Vec<PackageId>> = FxHashMap::default();
    for &package_id in reachable {
        packages_by_name
            .entry(package_id.name().as_str().to_owned())
            .or_default()
            .push(package_id);
    }
    packages_by_name
        .into_values()
        .filter(|packages| {
            let first_version = packages[0].version();
            packages
                .iter()
                .any(|package_id| package_id.version() != first_version)
        })
        .flatten()
        .collect()
}

fn invert_edges(
    edges: &FxHashMap<PackageId, Vec<(PackageId, DependencyType)>>,
) -> FxHashMap<PackageId, Vec<(PackageId, DependencyType)>> {
    let mut inverted = FxHashMap::<PackageId, Vec<(PackageId, DependencyType)>>::default();
    for (&package_id, dependencies) in edges {
        for &(dependency_id, kind) in dependencies {
            inverted
                .entry(dependency_id)
                .or_default()
                .push((package_id, kind));
        }
    }
    for dependencies in inverted.values_mut() {
        dependencies.sort_unstable();
    }
    inverted
}

/// Helper function to resolve the manifest path, handling absolute vs relative paths and
/// defaulting to finding the workspace root when no path is provided.
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

/// The node arena with empty children, a package-to-node index, and root ids.
struct CollectedPackages {
    nodes: Vec<DependencyNode>,
    pkg_index: FxHashMap<PackageId, NodeId>,
    roots: Vec<NodeId>,
}

/// Create one [`DependencyNode::Crate`] per reachable package.
///
/// Starting from the workspace roots, walk the resolved graph and assign each
/// unique package a stable arena node id. Child links are filled in later.
fn collect_packages(resolved: &ResolvedWorkspace) -> CollectedPackages {
    let capacity = resolved.packages.len();
    let mut remaining: Vec<PackageId> = Vec::with_capacity(capacity);
    remaining.extend(resolved.root_ids.iter().copied());

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
        .root_ids
        .iter()
        .filter_map(|pkg_id| pkg_index.get(pkg_id).copied())
        .collect();

    CollectedPackages {
        nodes,
        pkg_index,
        roots,
    }
}

/// Wire the dependency edges between the already-collected arena nodes.
///
/// Normal dependencies become direct children of the crate node.
///
/// Dev and build dependencies are grouped under synthetic
/// `[dev-dependencies]` / `[build-dependencies]` nodes.
///
/// While attaching those child links, this pass also builds the reverse
/// parent index for every node.
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
