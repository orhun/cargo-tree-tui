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
    /// Index into the active visible cache identifying the selected position.
    selected: Option<VisIdx>,
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
    /// Flattened visible tree for the current expansion state.
    visible_cache: Vec<VisibleNode>,
    /// Last visible non-group child per parent in `visible_cache`, indexed by parent visible index.
    visible_last_non_group_child: Vec<Option<VisIdx>>,
    /// Flattened visible tree restricted to the active search result.
    search_visible_cache: Vec<VisibleNode>,
    /// Last visible non-group child per parent in `search_visible_cache`, indexed by parent visible index.
    search_visible_last_non_group_child: Vec<Option<VisIdx>>,
    /// Indicates whether the visible cache is outdated.
    dirty: bool,
    /// Indicates sibling links / last-child map need recomputation for `visible_cache`.
    visible_metadata_dirty: bool,
    /// Indicates sibling links / last-child map need recomputation for `search_visible_cache`.
    search_metadata_dirty: bool,
}

/// Visible node metadata used for navigation.
#[derive(Debug, Clone, Copy)]
pub struct VisibleNode {
    /// Node identifier.
    pub id: NodeId,
    /// Depth in the tree hierarchy.
    pub depth: usize,
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
            selected: None,
            search_visible_nodes: Vec::new(),
            search_matches: Vec::new(),
            search_visible_ids: Vec::new(),
            search_match_ids: Vec::new(),
            viewport: Viewport::default(),
            visible_cache: Vec::new(),
            search_visible_cache: Vec::new(),
            search_visible_last_non_group_child: Vec::new(),
            dirty: true,
            visible_metadata_dirty: true,
            search_metadata_dirty: false,
        }
    }
}

impl TreeWidgetState {
    /// Returns the `NodeId` of the currently selected visible position.
    pub fn selected_node_id(&self) -> Option<NodeId> {
        let idx = self.selected?;
        self.active_visible_nodes().get(idx.0).map(|node| node.id)
    }

    /// Sets the selection to the visible position of the given `NodeId`.
    ///
    /// When the same `NodeId` appears at multiple visible positions, selects
    /// the first occurrence.
    pub fn set_selected_node_id(&mut self, tree: &DependencyTree, id: NodeId) {
        self.ensure_visible_nodes(tree);
        let visible = self.active_visible_nodes();
        self.selected = visible.iter().position(|node| node.id == id).map(VisIdx);
    }

    /// Grows all node-indexed caches to match the current tree size.
    ///
    /// The widget uses parallel `Vec`s keyed by `NodeId` instead of hash maps/sets so
    /// search, expansion, and render-time membership checks become cheap indexed loads.
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
        self.rebuild_filtered_visible();
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
    ///
    /// Callers must call [`ensure_visible_metadata`] first if the cache may be dirty.
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

        let Some(VisIdx(idx)) = self.selected else { return };
        let visible = self.active_visible_nodes();
        if idx + 1 < visible.len() {
            self.selected = Some(VisIdx(idx + 1));
        }
    }

    /// Moves the selection to the previous visible dependency.
    pub fn select_previous(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let Some(VisIdx(idx)) = self.selected else { return };
        if idx > 0 {
            self.selected = Some(VisIdx(idx - 1));
        }
    }

    /// Expands or collapses (toggles) the selected node.
    pub fn toggle(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
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
        let Some(idx) = self.selected else { return };
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
            self.expand_splice(tree, idx, node_id);
            // Selection stays at same vis_idx: the expanded node didn't move.
            return;
        }

        // Already open — move into first child (next visible entry at deeper depth).
        if idx.0 + 1 < self.active_visible_nodes().len() {
            self.selected = Some(VisIdx(idx.0 + 1));
        }
    }

    /// Collapses the selected node or moves focus to its parent when already closed.
    pub fn collapse(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        let Some(idx) = self.selected else { return };
        let Some(node_id) = self.selected_node_id() else {
            return;
        };
        let Some(node) = tree.node(node_id) else {
            return;
        };

        // If the node has children and is open, close it first.
        if !node.children().is_empty() && self.open[node_id.0] {
            self.collapse_splice(idx, node_id);
            // Selection stays at same vis_idx: the collapsed node didn't move.
            return;
        }

        // Otherwise move focus to its parent in the visible cache.
        let visible = self.active_visible_nodes();
        if let Some(parent_vis_idx) = visible.get(idx.0).and_then(|v| v.parent_vis_idx) {
            self.selected = Some(parent_vis_idx);
        }
    }

    /// Moves the selection to the parent node, if any.
    pub fn select_parent(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        let Some(idx) = self.selected else { return };
        let visible = self.active_visible_nodes();
        if let Some(parent_vis_idx) = visible.get(idx.0).and_then(|v| v.parent_vis_idx) {
            self.selected = Some(parent_vis_idx);
        }
    }

    /// Moves the selection to the next sibling, if any.
    pub fn select_next_sibling(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        self.ensure_visible_metadata(tree);
        let Some(idx) = self.selected else { return };
        let visible = self.active_visible_nodes();
        if let Some(next) = visible[idx.0].next_sibling {
            self.selected = Some(next);
        }
    }

    /// Moves the selection to the previous sibling, if any.
    pub fn select_previous_sibling(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        self.ensure_visible_metadata(tree);
        let Some(idx) = self.selected else { return };
        let visible = self.active_visible_nodes();
        if let Some(prev) = visible[idx.0].prev_sibling {
            self.selected = Some(prev);
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
        let Some(VisIdx(idx)) = self.selected else { return };
        let len = self.active_visible_nodes().len() as isize;
        if len == 0 {
            return;
        }

        let next = (idx as isize + delta).clamp(0, len - 1) as usize;
        self.selected = Some(VisIdx(next));
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
            // Do not mark leaves as open to avoid confusing collapse semantics.
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

    /// Rebuilds the visible caches lazily when tree openness has changed.
    pub fn ensure_visible_nodes(&mut self, tree: &DependencyTree) {
        if self.dirty {
            self.rebuild_visible(tree);
            self.dirty = false;
        }
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
    ///
    /// Search reuses the main visible cache and layers a filtered slice on top of it, so
    /// callers can treat both modes uniformly.
    pub fn active_visible_nodes(&self) -> &[VisibleNode] {
        if self.search_visible_cache.is_empty() {
            &self.visible_cache
        } else {
            &self.search_visible_cache
        }
    }

    /// Incrementally expands a node by splicing its newly-visible descendants
    /// into `visible_cache`, avoiding a full-tree DFS rebuild.
    fn expand_splice(&mut self, tree: &DependencyTree, vis_idx: VisIdx, node_id: NodeId) {
        self.open[node_id.0] = true;

        let parent_depth = self.visible_cache[vis_idx.0].depth;
        let mut new_nodes = Vec::new();
        let mut ancestors = FxHashSet::default();

        if let Some(node) = tree.node(node_id) {
            ancestors.insert(node_id);
            for &child in node.children() {
                if !ancestors.contains(&child) {
                    Self::collect_visible(
                        &self.open,
                        tree,
                        child,
                        parent_depth + 1,
                        None, // parent_vis_idx recomputed below
                        &mut new_nodes,
                        &mut ancestors,
                    );
                }
            }
        }

        if new_nodes.is_empty() {
            return;
        }

        let insert_pos = vis_idx.0 + 1;
        self.visible_cache.splice(insert_pos..insert_pos, new_nodes);
        self.recompute_metadata();
    }

    /// Incrementally collapses a node by draining its descendants from
    /// `visible_cache`, avoiding a full-tree DFS rebuild.
    fn collapse_splice(&mut self, vis_idx: VisIdx, node_id: NodeId) {
        self.open[node_id.0] = false;

        let parent_depth = self.visible_cache[vis_idx.0].depth;
        let start = vis_idx.0 + 1;

        // Descendants are the contiguous block with depth > parent_depth.
        let end = self.visible_cache[start..]
            .iter()
            .position(|n| n.depth <= parent_depth)
            .map(|offset| start + offset)
            .unwrap_or(self.visible_cache.len());

        if start < end {
            self.visible_cache.drain(start..end);
            self.recompute_metadata();
        }
    }

    /// Recomputes all derived metadata after an incremental splice.
    fn recompute_metadata(&mut self) {
        Self::recompute_parent_vis_idx(&mut self.visible_cache);
        self.visible_metadata_dirty = true;
        self.rebuild_filtered_visible();
    }

    /// Recomputes `parent_vis_idx` for all entries using the `depth` field.
    ///
    /// Because the cache is in DFS order, a depth stack tracks the most recent
    /// node at each depth. For a node at depth `d`, its parent is the last node
    /// seen at depth `d - 1`.
    fn recompute_parent_vis_idx(visible_nodes: &mut [VisibleNode]) {
        let mut depth_stack: Vec<VisIdx> = Vec::with_capacity(64);

        for (i, node) in visible_nodes.iter_mut().enumerate() {
            let depth = node.depth;
            depth_stack.truncate(depth);

            node.parent_vis_idx = if depth == 0 {
                None
            } else {
                depth_stack.last().copied()
            };

            if depth_stack.len() == depth {
                depth_stack.push(VisIdx(i));
            } else {
                depth_stack[depth] = VisIdx(i);
            }
        }
    }

    fn rebuild_visible(&mut self, tree: &DependencyTree) {
        self.visible_cache.clear();
        let open = &self.open;
        let mut ancestors = FxHashSet::default();
        for &root in tree.roots() {
            Self::collect_visible(
                open,
                tree,
                root,
                0,
                None,
                &mut self.visible_cache,
                &mut ancestors,
            );
        }
        self.visible_metadata_dirty = true;
        self.rebuild_filtered_visible();
    }

    /// Rebuilds the visible slice for the active search result while preserving the main cache.
    fn rebuild_filtered_visible(&mut self) {
        self.search_visible_cache.clear();
        self.search_visible_last_non_group_child.fill(None);
        if self.search_match_ids.is_empty() {
            return;
        }

        // Map from original visible_cache index to new search_visible_cache index.
        let mut old_to_new: Vec<Option<VisIdx>> = vec![None; self.visible_cache.len()];

        for (old_idx, node) in self.visible_cache.iter().enumerate() {
            if self.search_visible_nodes[node.id.0] {
                let new_idx = VisIdx(self.search_visible_cache.len());
                old_to_new[old_idx] = Some(new_idx);

                let remapped_parent = node
                    .parent_vis_idx
                    .and_then(|old_parent| old_to_new[old_parent.0]);

                self.search_visible_cache.push(VisibleNode {
                    id: node.id,
                    depth: node.depth,
                    parent_vis_idx: remapped_parent,
                    next_sibling: None,
                    prev_sibling: None,
                });
            }
        }

        self.search_metadata_dirty = true;
    }

    /// Marks a matching node and all of its visible ancestors in the search bitset.
    ///
    /// `search_visible_ids` tracks which bits were flipped so clearing or replacing search
    /// state only touches previously marked nodes rather than resetting the whole vector.
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

    fn collect_visible(
        open: &[bool],
        tree: &DependencyTree,
        id: NodeId,
        depth: usize,
        parent_vis_idx: Option<VisIdx>,
        out: &mut Vec<VisibleNode>,
        ancestors: &mut FxHashSet<NodeId>,
    ) {
        let my_vis_idx = VisIdx(out.len());
        out.push(VisibleNode {
            id,
            depth,
            parent_vis_idx,
            next_sibling: None,
            prev_sibling: None,
        });

        if !open[id.0] {
            return;
        }

        if let Some(node) = tree.node(id) {
            ancestors.insert(id);
            for &child in node.children() {
                if !ancestors.contains(&child) {
                    Self::collect_visible(
                        open,
                        tree,
                        child,
                        depth + 1,
                        Some(my_vis_idx),
                        out,
                        ancestors,
                    );
                }
            }
            ancestors.remove(&id);
        }
    }

    /// Ensures the selection points to a valid visible node, defaulting to the first entry.
    ///
    /// Returns `true` if a valid selection exists after the operation.
    fn ensure_selection(&mut self, tree: &DependencyTree) -> bool {
        self.ensure_visible_nodes(tree);
        let visible = self.active_visible_nodes();

        if visible.is_empty() {
            self.selected = None;
            return false;
        }

        if let Some(idx) = self.selected
            && idx.0 < visible.len()
        {
            return true;
        }

        self.selected = Some(VisIdx(0));
        true
    }

    /// Returns the index of the selected node among visible nodes.
    pub fn selected_position(&mut self, tree: &DependencyTree) -> Option<VisIdx> {
        if !self.ensure_selection(tree) {
            return None;
        }
        self.selected
    }

    /// Returns the cached selection index without triggering a rebuild.
    pub fn selected_position_cached(&self) -> Option<VisIdx> {
        self.selected
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
        self.dirty = true;
        self.ensure_selection(tree);
    }

    /// Records, for each parent visible position, the last visible non-group child position.
    ///
    /// The render path uses this cache to decide whether a node should draw a continuing
    /// branch (`├──`) or a terminating branch (`└──`) without rescanning siblings.
    /// Group nodes are skipped because they are labels and do not keep branch guides alive.
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
    ///
    /// Because nodes are stored in DFS order, children of the same parent appear
    /// in order but not contiguously. A single forward pass with per-parent tracking
    /// is sufficient to wire up both directions.
    fn populate_sibling_links(visible_nodes: &mut [VisibleNode]) {
        // Track the last child seen for each parent visible index.
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
