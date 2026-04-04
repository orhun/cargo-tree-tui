use rustc_hash::FxHashSet;

use crate::core::{DependencyNode, DependencyTree, NodeId};

use super::viewport::Viewport;

/// Index into a flattened visible-node cache.
///
/// Distinguishes visible-cache positions from [`NodeId`] at the type level
/// so the two kinds of `usize` index cannot be accidentally mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VisIdx(pub usize);

/// [`TreeWidget`] state that tracks open nodes and the current selection.
///
/// [`TreeWidget`]: super::TreeWidget
#[derive(Debug)]
pub struct TreeWidgetState {
    /// Open/closed state indexed by node id.
    pub open: Vec<bool>,
    /// Virtual position of the selected node in the full flattened tree.
    selected_virtual_pos: Option<usize>,
    /// Nodes kept visible by the active search, indexed by node id.
    search_visible_nodes: Vec<bool>,
    /// Nodes whose crate names directly match the active search query, indexed by node id.
    search_matches: Vec<bool>,
    /// Node ids whose `search_visible_nodes` bit is currently set, used for cheap resets.
    search_visible_ids: Vec<NodeId>,
    /// Node ids whose `search_matches` bit is currently set, used for cheap resets and refinement.
    search_match_ids: Vec<NodeId>,
    /// Current viewport.
    pub viewport: Viewport,
    /// Materialized window of visible nodes around the viewport.
    visible_cache: Vec<VisibleNode>,
    /// Last visible non-group child per parent in `visible_cache`, indexed by parent visible index.
    visible_last_non_group_child: Vec<Option<VisIdx>>,
    /// Materialized window of visible nodes for the active search result.
    search_visible_cache: Vec<VisibleNode>,
    /// Last visible non-group child per parent in `search_visible_cache`.
    search_visible_last_non_group_child: Vec<Option<VisIdx>>,
    /// Number of ancestor prefix nodes at the start of `visible_cache`.
    prefix_len: usize,
    /// Number of ancestor prefix nodes at the start of `search_visible_cache`.
    search_prefix_len: usize,
    /// Memoized visible-subtree sizes indexed by NodeId.
    subtree_sizes: Vec<usize>,
    /// Memoized search-filtered subtree sizes indexed by NodeId.
    search_subtree_sizes: Vec<usize>,
    /// Total number of virtual lines (sum of root subtree sizes).
    total_virtual_lines: usize,
    /// Total number of virtual lines for the search-filtered view.
    search_total_virtual_lines: usize,
    /// Whether subtree sizes need recomputation.
    subtree_dirty: bool,
    /// Whether the visible cache needs re-materialization.
    dirty: bool,
    /// Indicates sibling links / last-child map need recomputation for `visible_cache`.
    visible_metadata_dirty: bool,
    /// Indicates sibling links / last-child map need recomputation for `search_visible_cache`.
    search_metadata_dirty: bool,
}

/// Visible node metadata used for navigation and rendering.
#[derive(Debug, Clone, Copy)]
pub struct VisibleNode {
    /// Node identifier.
    pub id: NodeId,
    /// Depth in the tree hierarchy.
    pub depth: usize,
    /// Position in the full virtual flattened list.
    pub virtual_pos: usize,
    /// Index of the parent in the same visible cache, or `None` for roots.
    pub parent_vis_idx: Option<VisIdx>,
    /// Next sibling in the same visible cache sharing the same parent.
    pub next_sibling: Option<VisIdx>,
    /// Previous sibling in the same visible cache sharing the same parent.
    pub prev_sibling: Option<VisIdx>,
}

/// Search result payload computed off the UI thread.
#[derive(Debug, Clone)]
pub struct SearchState {
    /// Nodes kept visible by the active search, indexed by node id.
    pub visible_nodes: Vec<bool>,
    /// Nodes whose crate names directly match the active search query, indexed by node id.
    pub matches: Vec<bool>,
    /// Nodes visible because of the active search.
    pub visible_ids: Vec<NodeId>,
    /// Nodes that directly match the active search.
    pub match_ids: Vec<NodeId>,
}

impl SearchState {
    fn new(node_count: usize) -> Self {
        Self {
            visible_nodes: vec![false; node_count],
            matches: vec![false; node_count],
            visible_ids: Vec::new(),
            match_ids: Vec::new(),
        }
    }
}

impl Default for TreeWidgetState {
    fn default() -> Self {
        Self {
            open: Vec::new(),
            visible_last_non_group_child: Vec::new(),
            selected_virtual_pos: None,
            search_visible_nodes: Vec::new(),
            search_matches: Vec::new(),
            search_visible_ids: Vec::new(),
            search_match_ids: Vec::new(),
            viewport: Viewport::default(),
            visible_cache: Vec::new(),
            search_visible_cache: Vec::new(),
            search_visible_last_non_group_child: Vec::new(),
            prefix_len: 0,
            search_prefix_len: 0,
            subtree_sizes: Vec::new(),
            search_subtree_sizes: Vec::new(),
            total_virtual_lines: 0,
            search_total_virtual_lines: 0,
            subtree_dirty: true,
            dirty: true,
            visible_metadata_dirty: true,
            search_metadata_dirty: false,
        }
    }
}

/// Output of windowed materialization.
struct MaterializeResult {
    /// Materialized nodes: ancestor prefix + viewport window.
    nodes: Vec<VisibleNode>,
    /// Number of ancestor prefix nodes at the start.
    prefix_len: usize,
}

struct MaterializeCtx<'a> {
    tree: &'a DependencyTree,
    open: &'a [bool],
    sizes: &'a [usize],
    filter: Option<&'a [bool]>,
    virtual_pos: usize,
    window_start: usize,
    window_end: usize,
    /// Stack of ancestors on the current DFS path: (NodeId, depth, virtual_pos).
    ancestor_stack: Vec<(NodeId, usize, usize)>,
    prefix_emitted: bool,
    output: Vec<VisibleNode>,
    prefix_len: usize,
}

impl MaterializeCtx<'_> {
    fn materialize_node(&mut self, id: NodeId, depth: usize, parent_ancestor_idx: Option<usize>) {
        let my_vpos = self.virtual_pos;
        let subtree_size = self.sizes[id.0];

        // Entirely before window — skip subtree
        if my_vpos + subtree_size <= self.window_start {
            self.virtual_pos += subtree_size;
            return;
        }

        // Entirely past window — stop
        if my_vpos >= self.window_end {
            return;
        }

        // This node is in or overlaps the window.
        self.virtual_pos += 1;

        let in_window = my_vpos >= self.window_start;

        if in_window {
            // Emit ancestor prefix if this is the first in-window node.
            if !self.prefix_emitted {
                self.emit_ancestor_prefix();
            }

            let parent_vis_idx = if depth == 0 {
                None
            } else {
                // Find the parent in the output buffer.
                // The parent is the last ancestor_stack entry, which was either emitted
                // as prefix or as a previous window node.
                self.find_parent_vis_idx(parent_ancestor_idx)
            };

            self.output.push(VisibleNode {
                id,
                depth,
                virtual_pos: my_vpos,
                parent_vis_idx,
                next_sibling: None,
                prev_sibling: None,
            });
        }

        // Recurse into children if open.
        if self.open[id.0]
            && let Some(node) = self.tree.node(id)
        {
            let my_ancestor_idx = self.ancestor_stack.len();
            self.ancestor_stack.push((id, depth, my_vpos));

            for &child in node.children() {
                if self.filter.is_some_and(|f| !f[child.0]) {
                    continue;
                }
                if self.virtual_pos >= self.window_end {
                    break;
                }
                let child_size = self.sizes[child.0];
                if self.virtual_pos + child_size <= self.window_start {
                    self.virtual_pos += child_size;
                    continue;
                }
                self.materialize_node(child, depth + 1, Some(my_ancestor_idx));
            }

            self.ancestor_stack.pop();
        }
    }

    /// Emits ancestor prefix nodes for lineage/breadcrumb rendering.
    fn emit_ancestor_prefix(&mut self) {
        self.prefix_emitted = true;

        if self.ancestor_stack.is_empty() {
            return;
        }

        for i in 0..self.ancestor_stack.len() {
            let (anc_id, anc_depth, anc_vpos) = self.ancestor_stack[i];

            let parent_vis_idx = if anc_depth == 0 || i == 0 {
                None
            } else {
                // Parent is the previous ancestor in the prefix.
                Some(VisIdx(self.output.len() - 1))
            };

            self.output.push(VisibleNode {
                id: anc_id,
                depth: anc_depth,
                virtual_pos: anc_vpos,
                parent_vis_idx,
                next_sibling: None,
                prev_sibling: None,
            });
        }

        self.prefix_len = self.output.len();
    }

    /// Finds the VisIdx of the parent node in the output buffer.
    fn find_parent_vis_idx(&self, parent_ancestor_idx: Option<usize>) -> Option<VisIdx> {
        let parent_ancestor_idx = parent_ancestor_idx?;
        let (parent_id, _, parent_vpos) = self.ancestor_stack[parent_ancestor_idx];

        // Search the output buffer for this parent.
        // It's either in the prefix or was emitted as a window node.
        // Search from the end since the parent was recently emitted.
        self.output
            .iter()
            .rposition(|n| n.id == parent_id && n.virtual_pos == parent_vpos)
            .map(VisIdx)
    }
}

impl TreeWidgetState {
    /// Finds the node at the given virtual position in the active cache.
    fn find_by_vpos(&self, vpos: usize) -> Option<(VisIdx, &VisibleNode)> {
        self.active_visible_nodes()
            .iter()
            .enumerate()
            .find(|(_, n)| n.virtual_pos == vpos)
            .map(|(i, n)| (VisIdx(i), n))
    }

    /// Returns the `NodeId` of the currently selected visible position.
    ///
    /// Returns `None` if nothing is selected or the cache doesn't contain the
    /// selected position (call [`ensure_visible_nodes`] first).
    pub fn selected_node_id(&self) -> Option<NodeId> {
        let vpos = self.selected_virtual_pos?;
        self.find_by_vpos(vpos).map(|(_, n)| n.id)
    }

    /// Sets the selection to the virtual position of the given `NodeId`.
    ///
    /// Requires a DFS walk using subtree sizes to locate the first occurrence.
    pub fn set_selected_node_id(&mut self, tree: &DependencyTree, id: NodeId) {
        self.ensure_subtree_sizes(tree);
        let sizes = self.active_subtree_sizes();
        let filter = self.active_filter();
        let roots = tree.roots();

        self.selected_virtual_pos = find_virtual_pos(tree, &self.open, sizes, filter, roots, id);
        self.dirty = true;
    }

    /// Grows all node-indexed caches to match the current tree size.
    fn ensure_node_capacity(&mut self, tree: &DependencyTree) {
        let len = tree.nodes.len();
        if self.open.len() == len {
            return;
        }

        self.open.resize(len, false);
        self.search_visible_nodes.resize(len, false);
        self.search_matches.resize(len, false);
    }

    /// Clears any active search filtering state.
    pub fn clear_search(&mut self) {
        for node_id in self.search_visible_ids.drain(..) {
            self.search_visible_nodes[node_id.0] = false;
        }
        for node_id in self.search_match_ids.drain(..) {
            self.search_matches[node_id.0] = false;
        }
        self.search_visible_cache.clear();
        self.search_visible_last_non_group_child.fill(None);
        self.search_subtree_sizes.clear();
        self.search_total_virtual_lines = 0;
        self.search_prefix_len = 0;
        // Rematerialize the main view with the current selection.
        self.dirty = true;
    }

    /// Returns whether a node directly matches the active search query.
    pub fn is_search_match(&self, node_id: NodeId) -> bool {
        self.search_matches.get(node_id.0).copied().unwrap_or(false)
    }

    /// Applies externally computed search state to the visible tree.
    pub fn apply_search_state(&mut self, tree: &DependencyTree, search_state: SearchState) {
        self.ensure_node_capacity(tree);
        self.search_visible_nodes = search_state.visible_nodes;
        self.search_matches = search_state.matches;
        self.search_visible_ids = search_state.visible_ids;
        self.search_match_ids = search_state.match_ids;
        self.rebuild_search_view(tree);
    }

    /// Updates search-filtered nodes by matching crate names case-sensitively.
    pub fn set_search_query(&mut self, tree: &DependencyTree, query: &str) {
        if query.is_empty() {
            self.clear_search();
            return;
        }

        self.apply_search_state(tree, Self::search(tree, query));
    }

    /// Computes search-filtered nodes without mutating widget state.
    pub fn search(tree: &DependencyTree, query: &str) -> SearchState {
        if query.is_empty() {
            return SearchState::new(tree.nodes.len());
        }

        let mut search_state = SearchState::new(tree.nodes.len());

        for &node_id in tree.crate_nodes() {
            let Some(DependencyNode::Crate(dependency)) = tree.node(node_id) else {
                continue;
            };

            if dependency.name.contains(query) {
                search_state.matches[node_id.0] = true;
                search_state.match_ids.push(node_id);
                Self::include_ancestors(
                    tree,
                    node_id,
                    &mut search_state.visible_nodes,
                    &mut search_state.visible_ids,
                );
            }
        }

        search_state
    }

    /// Returns the last visible non-group child per parent for the active view.
    pub fn active_last_visible_non_group_child(&self) -> Option<&[Option<VisIdx>]> {
        if self.search_visible_cache.is_empty() {
            Some(&self.visible_last_non_group_child)
        } else {
            Some(&self.search_visible_last_non_group_child)
        }
    }

    /// Moves the selection to the next visible dependency.
    pub fn select_next(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let Some(vpos) = self.selected_virtual_pos else {
            return;
        };
        let total = self.active_total_virtual_lines();
        if vpos + 1 < total {
            self.selected_virtual_pos = Some(vpos + 1);
            self.dirty = true;
        }
    }

    /// Moves the selection to the previous visible dependency.
    pub fn select_previous(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let Some(vpos) = self.selected_virtual_pos else {
            return;
        };
        if vpos > 0 {
            self.selected_virtual_pos = Some(vpos - 1);
            self.dirty = true;
        }
    }

    /// Expands or collapses (toggles) the selected node.
    pub fn toggle(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        self.ensure_visible_nodes(tree);
        let Some(node_id) = self.selected_node_id() else {
            return;
        };
        let Some(node) = tree.node(node_id) else {
            return;
        };

        if node.children().is_empty() {
            return;
        }

        if self.open[node_id.0] {
            self.collapse(tree);
        } else {
            self.expand(tree);
        }
    }

    /// Expands the selected node or moves into its first child when already expanded.
    pub fn expand(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        let Some(vpos) = self.selected_virtual_pos else {
            return;
        };
        self.ensure_visible_nodes(tree);
        let Some(node_id) = self.selected_node_id() else {
            return;
        };
        let Some(node) = tree.node(node_id) else {
            return;
        };

        if node.children().is_empty() {
            return;
        }

        if !self.open[node_id.0] {
            self.open[node_id.0] = true;
            self.subtree_dirty = true;
            self.dirty = true;
            return;
        }

        // Already open — move into first child (next virtual position).
        let total = self.active_total_virtual_lines();
        if vpos + 1 < total {
            self.selected_virtual_pos = Some(vpos + 1);
            self.dirty = true;
        }
    }

    /// Collapses the selected node or moves focus to its parent when already closed.
    pub fn collapse(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        self.ensure_visible_nodes(tree);
        let Some(node_id) = self.selected_node_id() else {
            return;
        };
        let Some(node) = tree.node(node_id) else {
            return;
        };

        // If the node has children and is open, close it.
        if !node.children().is_empty() && self.open[node_id.0] {
            self.open[node_id.0] = false;
            self.subtree_dirty = true;
            self.dirty = true;
            return;
        }

        // Otherwise move focus to parent.
        self.select_parent(tree);
    }

    /// Moves the selection to the parent node, if any.
    pub fn select_parent(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        // Find the selected node in the cache and read its parent's virtual_pos.
        let Some(vpos) = self.selected_virtual_pos else {
            return;
        };
        if let Some((_, vnode)) = self.find_by_vpos(vpos)
            && let Some(parent_vis) = vnode.parent_vis_idx
            && let Some(parent_node) = self.active_visible_nodes().get(parent_vis.0)
        {
            self.selected_virtual_pos = Some(parent_node.virtual_pos);
            self.dirty = true;
        }
    }

    /// Moves the selection to the next sibling, if any.
    pub fn select_next_sibling(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        self.ensure_visible_metadata(tree);
        let Some(vpos) = self.selected_virtual_pos else {
            return;
        };
        if let Some((_, vnode)) = self.find_by_vpos(vpos)
            && let Some(next) = vnode.next_sibling
            && let Some(next_node) = self.active_visible_nodes().get(next.0)
        {
            self.selected_virtual_pos = Some(next_node.virtual_pos);
            self.dirty = true;
        }
    }

    /// Moves the selection to the previous sibling, if any.
    pub fn select_previous_sibling(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        self.ensure_visible_metadata(tree);
        let Some(vpos) = self.selected_virtual_pos else {
            return;
        };
        if let Some((_, vnode)) = self.find_by_vpos(vpos)
            && let Some(prev) = vnode.prev_sibling
            && let Some(prev_node) = self.active_visible_nodes().get(prev.0)
        {
            self.selected_virtual_pos = Some(prev_node.virtual_pos);
            self.dirty = true;
        }
    }

    /// Moves the selection up by approximately one page.
    pub fn page_up(&mut self, tree: &DependencyTree) {
        let step = self.viewport.height.saturating_sub(1).max(1) as isize;
        self.move_by(tree, -step);
    }

    /// Moves the selection down by approximately one page.
    pub fn page_down(&mut self, tree: &DependencyTree) {
        let step = self.viewport.height.saturating_sub(1).max(1) as isize;
        self.move_by(tree, step);
    }

    /// Moves the selection by a specified delta.
    fn move_by(&mut self, tree: &DependencyTree, delta: isize) {
        if !self.ensure_selection(tree) {
            return;
        }
        let Some(vpos) = self.selected_virtual_pos else {
            return;
        };
        let total = self.active_total_virtual_lines() as isize;
        if total == 0 {
            return;
        }

        let next = (vpos as isize + delta).clamp(0, total - 1) as usize;
        if next != vpos {
            self.selected_virtual_pos = Some(next);
            self.dirty = true;
        }
    }

    /// Opens all nodes up to the specified depth.
    pub fn open_to_depth(&mut self, tree: &DependencyTree, max_depth: usize) {
        if max_depth == 0 {
            return;
        }
        self.ensure_node_capacity(tree);
        self.open.fill(false);
        let mut ancestors = FxHashSet::default();
        for &root in tree.roots() {
            self.open_node(tree, root, 1, max_depth, &mut ancestors);
        }
        self.subtree_dirty = true;
        self.dirty = true;
        self.ensure_selection(tree);
    }

    fn open_node(
        &mut self,
        tree: &DependencyTree,
        id: NodeId,
        depth: usize,
        max_depth: usize,
        ancestors: &mut FxHashSet<NodeId>,
    ) {
        if depth >= max_depth {
            return;
        }

        if let Some(node) = tree.node(id) {
            if node.children().is_empty() {
                return;
            }

            self.open[id.0] = true;
            ancestors.insert(id);
            for &child in node.children() {
                if !ancestors.contains(&child) {
                    self.open_node(tree, child, depth + 1, max_depth, ancestors);
                }
            }
            ancestors.remove(&id);
        }
    }

    /// Returns cached visible nodes along with their depth in the hierarchy.
    pub fn visible_nodes(&mut self, tree: &DependencyTree) -> &[VisibleNode] {
        self.ensure_visible_nodes(tree);
        self.active_visible_nodes()
    }

    /// Recomputes subtree sizes if dirty.
    fn ensure_subtree_sizes(&mut self, tree: &DependencyTree) {
        if !self.subtree_dirty {
            return;
        }

        self.ensure_node_capacity(tree);

        self.total_virtual_lines =
            compute_subtree_sizes(tree, &self.open, None, &mut self.subtree_sizes);

        if !self.search_match_ids.is_empty() {
            self.search_total_virtual_lines = compute_subtree_sizes(
                tree,
                &self.open,
                Some(&self.search_visible_nodes),
                &mut self.search_subtree_sizes,
            );
        }

        self.subtree_dirty = false;
    }

    /// Rebuilds the visible caches lazily when tree openness has changed.
    pub fn ensure_visible_nodes(&mut self, tree: &DependencyTree) {
        if !self.dirty && !self.subtree_dirty {
            return;
        }

        self.ensure_subtree_sizes(tree);
        self.rebuild_visible(tree);
        self.dirty = false;
    }

    /// Lazily computes sibling links and last-child map for the active visible cache.
    pub fn ensure_visible_metadata(&mut self, tree: &DependencyTree) {
        if self.visible_metadata_dirty {
            Self::populate_sibling_links(&mut self.visible_cache);
            Self::populate_last_non_group_child_map(
                tree,
                &self.visible_cache,
                &mut self.visible_last_non_group_child,
            );
            self.visible_metadata_dirty = false;
        }
        if self.search_metadata_dirty && !self.search_visible_cache.is_empty() {
            Self::populate_sibling_links(&mut self.search_visible_cache);
            Self::populate_last_non_group_child_map(
                tree,
                &self.search_visible_cache,
                &mut self.search_visible_last_non_group_child,
            );
            self.search_metadata_dirty = false;
        }
    }

    /// Returns the currently active visible slice.
    pub fn active_visible_nodes(&self) -> &[VisibleNode] {
        if self.search_visible_cache.is_empty() {
            &self.visible_cache
        } else {
            &self.search_visible_cache
        }
    }

    /// Returns the total virtual line count for the active view.
    fn active_total_virtual_lines(&self) -> usize {
        if !self.search_match_ids.is_empty() {
            self.search_total_virtual_lines
        } else {
            self.total_virtual_lines
        }
    }

    /// Returns the active subtree sizes slice.
    fn active_subtree_sizes(&self) -> &[usize] {
        if !self.search_match_ids.is_empty() && !self.search_subtree_sizes.is_empty() {
            &self.search_subtree_sizes
        } else {
            &self.subtree_sizes
        }
    }

    /// Returns the active filter, if searching.
    fn active_filter(&self) -> Option<&[bool]> {
        if !self.search_match_ids.is_empty() {
            Some(&self.search_visible_nodes)
        } else {
            None
        }
    }

    fn rebuild_visible(&mut self, tree: &DependencyTree) {
        let vpos = self.selected_virtual_pos.unwrap_or(0);

        // Use viewport height if known, otherwise a generous default.
        let viewport_height = if self.viewport.height > 0 {
            self.viewport.height
        } else {
            50
        };

        // Estimate window_start from the selected position, with generous
        // buffer to ensure the materialized window covers what render needs.
        // Start well before the selection so ancestor prefix nodes are included.
        let window_start = vpos.saturating_sub(viewport_height);

        // Materialize enough for viewport + buffer for scrolling.
        let window_count = viewport_height * 2;

        let (sizes, filter): (&[usize], Option<&[bool]>) = if !self.search_match_ids.is_empty() {
            (&self.search_subtree_sizes, Some(&self.search_visible_nodes))
        } else {
            (&self.subtree_sizes, None)
        };

        let result = materialize_window(
            tree,
            &self.open,
            sizes,
            filter,
            tree.roots(),
            window_start,
            window_count,
        );

        if !self.search_match_ids.is_empty() {
            self.search_visible_cache = result.nodes;
            self.search_prefix_len = result.prefix_len;
            self.search_metadata_dirty = true;
        } else {
            self.visible_cache = result.nodes;
            self.prefix_len = result.prefix_len;
            self.visible_metadata_dirty = true;
        }
    }

    /// Rebuilds the search view after applying new search state.
    fn rebuild_search_view(&mut self, tree: &DependencyTree) {
        if self.search_match_ids.is_empty() {
            self.search_visible_cache.clear();
            self.search_subtree_sizes.clear();
            self.search_total_virtual_lines = 0;
            return;
        }

        self.search_total_virtual_lines = compute_subtree_sizes(
            tree,
            &self.open,
            Some(&self.search_visible_nodes),
            &mut self.search_subtree_sizes,
        );

        // Clamp selection to search view bounds.
        if let Some(vpos) = self.selected_virtual_pos
            && vpos >= self.search_total_virtual_lines
            && self.search_total_virtual_lines > 0
        {
            self.selected_virtual_pos = Some(self.search_total_virtual_lines - 1);
        }

        self.subtree_dirty = false;
        self.dirty = true;
        self.ensure_visible_nodes(tree);
    }

    /// Marks a matching node and all of its visible ancestors in the search bitset.
    fn include_ancestors(
        tree: &DependencyTree,
        id: NodeId,
        search_visible_nodes: &mut [bool],
        search_visible_ids: &mut Vec<NodeId>,
    ) {
        let mut current = Some(id);
        while let Some(node_id) = current {
            if search_visible_nodes[node_id.0] {
                break;
            }

            search_visible_nodes[node_id.0] = true;
            search_visible_ids.push(node_id);

            current = tree.parent_of(node_id);
        }
    }

    /// Ensures the selection points to a valid visible node, defaulting to position 0.
    ///
    /// Returns `true` if a valid selection exists after the operation.
    fn ensure_selection(&mut self, tree: &DependencyTree) -> bool {
        self.ensure_subtree_sizes(tree);
        let total = self.active_total_virtual_lines();

        if total == 0 {
            self.selected_virtual_pos = None;
            return false;
        }

        match self.selected_virtual_pos {
            Some(vpos) if vpos < total => true,
            Some(_) => {
                self.selected_virtual_pos = Some(total - 1);
                self.dirty = true;
                true
            }
            None => {
                self.selected_virtual_pos = Some(0);
                self.dirty = true;
                true
            }
        }
    }

    /// Returns the VisIdx of the selected node within the materialized cache.
    pub fn selected_position(&mut self, tree: &DependencyTree) -> Option<VisIdx> {
        if !self.ensure_selection(tree) {
            return None;
        }
        self.ensure_visible_nodes(tree);
        self.selected_vis_idx()
    }

    /// Returns the cached selection VisIdx without triggering a rebuild.
    pub fn selected_position_cached(&self) -> Option<VisIdx> {
        self.selected_vis_idx()
    }

    /// Finds the VisIdx of the selected virtual position in the active cache.
    fn selected_vis_idx(&self) -> Option<VisIdx> {
        let vpos = self.selected_virtual_pos?;
        self.find_by_vpos(vpos).map(|(idx, _)| idx)
    }

    /// Returns the total virtual line count for scrollbar/viewport calculations.
    pub fn total_lines(&mut self, tree: &DependencyTree) -> usize {
        self.ensure_subtree_sizes(tree);
        self.active_total_virtual_lines()
    }

    /// Returns the selected virtual position.
    pub fn selected_virtual_pos(&self) -> Option<usize> {
        self.selected_virtual_pos
    }

    /// Updates the available viewport.
    pub(crate) fn update_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }

    /// Expands all nodes in the tree.
    pub fn expand_all(&mut self, tree: &DependencyTree) {
        self.ensure_node_capacity(tree);
        self.open.fill(false);
        for i in 0..tree.nodes.len() {
            let id = NodeId(i);
            if let Some(node) = tree.node(id) {
                // Only mark non-leaf nodes as open, leaves stay implicit.
                if !node.children().is_empty() {
                    self.open[id.0] = true;
                }
            }
        }
        self.subtree_dirty = true;
        self.dirty = true;
        self.ensure_selection(tree);
    }

    /// Records, for each parent visible position, the last visible non-group child position.
    fn populate_last_non_group_child_map(
        tree: &DependencyTree,
        visible_nodes: &[VisibleNode],
        target: &mut Vec<Option<VisIdx>>,
    ) {
        target.clear();
        target.resize(visible_nodes.len(), None);

        for (vis_idx, node) in visible_nodes.iter().enumerate() {
            let Some(tree_node) = tree.node(node.id) else {
                continue;
            };

            if tree_node.is_group() {
                continue;
            }

            if let Some(parent_vis_idx) = node.parent_vis_idx {
                target[parent_vis_idx.0] = Some(VisIdx(vis_idx));
            }
        }
    }

    /// Links each visible node to its next and previous sibling (same parent).
    fn populate_sibling_links(visible_nodes: &mut [VisibleNode]) {
        let mut last_child_of: Vec<Option<VisIdx>> = vec![None; visible_nodes.len()];

        for i in 0..visible_nodes.len() {
            let Some(parent) = visible_nodes[i].parent_vis_idx else {
                continue;
            };

            let current = VisIdx(i);
            if let Some(prev) = last_child_of[parent.0] {
                visible_nodes[prev.0].next_sibling = Some(current);
                visible_nodes[i].prev_sibling = Some(prev);
            }
            last_child_of[parent.0] = Some(current);
        }
    }
}

/// Finds the virtual position of the first occurrence of a `NodeId` in the virtual tree.
fn find_virtual_pos(
    tree: &DependencyTree,
    open: &[bool],
    sizes: &[usize],
    filter: Option<&[bool]>,
    roots: &[NodeId],
    target: NodeId,
) -> Option<usize> {
    let mut vpos = 0usize;
    for &root in roots {
        if filter.is_some_and(|f| !f[root.0]) {
            continue;
        }
        if let Some(found) = find_vpos_recursive(tree, open, sizes, filter, root, target, &mut vpos)
        {
            return Some(found);
        }
    }
    None
}

fn find_vpos_recursive(
    tree: &DependencyTree,
    open: &[bool],
    sizes: &[usize],
    filter: Option<&[bool]>,
    id: NodeId,
    target: NodeId,
    vpos: &mut usize,
) -> Option<usize> {
    if id == target {
        return Some(*vpos);
    }

    *vpos += 1;

    if open[id.0]
        && let Some(node) = tree.node(id)
    {
        for &child in node.children() {
            if filter.is_some_and(|f| !f[child.0]) {
                continue;
            }
            if child != target && sizes[child.0] == 0 {
                continue;
            }
            if let Some(found) = find_vpos_recursive(tree, open, sizes, filter, child, target, vpos)
            {
                return Some(found);
            }
        }
    }

    None
}

/// Computes memoized visible-subtree sizes for all nodes.
///
/// `subtree_size[id] = 1 + (if open[id]: Σ subtree_size[child])`.
/// A `computed` guard prevents recomputing already-visited nodes (DAG memoization).
/// An `in_progress` guard breaks hypothetical cycles by treating in-progress nodes as leaves.
fn compute_subtree_sizes(
    tree: &DependencyTree,
    open: &[bool],
    filter: Option<&[bool]>,
    sizes: &mut Vec<usize>,
) -> usize {
    sizes.clear();
    sizes.resize(tree.nodes.len(), 0);
    let mut computed = vec![false; tree.nodes.len()];
    let mut in_progress = vec![false; tree.nodes.len()];

    let mut total = 0usize;
    for &root in tree.roots() {
        if filter.is_some_and(|f| !f[root.0]) {
            continue;
        }
        total += compute_size_recursive(
            tree,
            open,
            filter,
            root,
            sizes,
            &mut computed,
            &mut in_progress,
        );
    }
    total
}

fn compute_size_recursive(
    tree: &DependencyTree,
    open: &[bool],
    filter: Option<&[bool]>,
    id: NodeId,
    sizes: &mut [usize],
    computed: &mut [bool],
    in_progress: &mut [bool],
) -> usize {
    if in_progress[id.0] {
        return 1; // cycle break
    }
    if computed[id.0] {
        return sizes[id.0];
    }

    in_progress[id.0] = true;

    let mut size: usize = 1;
    if open[id.0]
        && let Some(node) = tree.node(id)
    {
        for &child in node.children() {
            if filter.is_some_and(|f| !f[child.0]) {
                continue;
            }
            size += compute_size_recursive(tree, open, filter, child, sizes, computed, in_progress);
        }
    }

    sizes[id.0] = size;
    computed[id.0] = true;
    in_progress[id.0] = false;
    size
}

/// Materializes a window of visible nodes around a viewport range.
///
/// The output contains an ancestor prefix (for lineage/breadcrumb rendering)
/// followed by the viewport window nodes. Only nodes within `[window_start, window_end)`
/// are emitted, plus ancestors of the first emitted node.
fn materialize_window(
    tree: &DependencyTree,
    open: &[bool],
    sizes: &[usize],
    filter: Option<&[bool]>,
    roots: &[NodeId],
    window_start: usize,
    window_count: usize,
) -> MaterializeResult {
    let mut ctx = MaterializeCtx {
        tree,
        open,
        sizes,
        filter,
        virtual_pos: 0,
        window_start,
        window_end: window_start + window_count,
        // Ancestor stack: (NodeId, depth, virtual_pos) for nodes on the current DFS path
        ancestor_stack: Vec::with_capacity(64),
        prefix_emitted: false,
        output: Vec::with_capacity(window_count + 64),
        prefix_len: 0,
    };

    for &root in roots {
        if filter.is_some_and(|f| !f[root.0]) {
            continue;
        }
        if ctx.virtual_pos >= ctx.window_end {
            break;
        }
        let root_size = sizes[root.0];
        if ctx.virtual_pos + root_size <= ctx.window_start {
            ctx.virtual_pos += root_size;
            continue;
        }
        ctx.materialize_node(root, 0, None);
    }

    MaterializeResult {
        nodes: ctx.output,
        prefix_len: ctx.prefix_len,
    }
}
