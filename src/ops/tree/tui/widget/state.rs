use crate::core::{DependencyNode, DependencyTree, NodeId};

use super::viewport::Viewport;

/// [`TreeWidget`] state that tracks open nodes and the current selection.
///
/// [`TreeWidget`]: super::TreeWidget
#[derive(Debug)]
pub struct TreeWidgetState {
    /// Open/closed state indexed by node id.
    pub open: Vec<bool>,
    /// Currently selected node.
    pub selected: Option<NodeId>,
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
    /// Last visible non-group child per parent in `visible_cache`, indexed by parent node id.
    visible_last_non_group_child: Vec<Option<NodeId>>,
    /// Flattened visible tree restricted to the active search result.
    search_visible_cache: Vec<VisibleNode>,
    /// Last visible non-group child per parent in `search_visible_cache`, indexed by parent node id.
    search_visible_last_non_group_child: Vec<Option<NodeId>>,
    /// Indicates whether the visible cache is outdated.
    dirty: bool,
}

/// Visible node metadata used for navigation.
#[derive(Debug, Clone, Copy)]
pub struct VisibleNode {
    /// Node identifier.
    pub id: NodeId,
    /// Depth in the tree hierarchy.
    pub depth: usize,
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
        }
    }
}

impl TreeWidgetState {
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
        self.visible_last_non_group_child.resize(len, None);
        self.search_visible_last_non_group_child.resize(len, None);
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
        self.rebuild_filtered_visible(tree);
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
    /// This lets the render path answer "is this the final visible branch?" with one
    /// indexed lookup rather than rescanning siblings in the current filtered slice.
    pub fn active_last_visible_non_group_child(&self) -> Option<&[Option<NodeId>]> {
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

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        let next = {
            let visible = self.visible_nodes(tree);
            let Some(current_index) = Self::selected_index(visible, selected) else {
                return;
            };

            visible.get(current_index + 1).map(|node| node.id)
        };

        if let Some(next_id) = next {
            self.selected = Some(next_id);
        }
    }

    /// Moves the selection to the previous visible dependency.
    pub fn select_previous(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        let previous = {
            let visible = self.visible_nodes(tree);
            let Some(current_index) = Self::selected_index(visible, selected) else {
                return;
            };

            if current_index > 0 {
                Some(visible[current_index - 1].id)
            } else {
                None
            }
        };

        if let Some(previous_id) = previous {
            self.selected = Some(previous_id);
        }
    }

    /// Expands or collapses (toggles) the selected node.
    pub fn toggle(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }
        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        let Some(node) = tree.node(selected) else {
            return;
        };

        if node.children().is_empty() {
            return;
        }

        if self.open[selected.0] {
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

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        let Some(node) = tree.node(selected) else {
            return;
        };

        if node.children().is_empty() {
            return;
        }

        if !self.open[selected.0] {
            self.open[selected.0] = true;
            self.insert_descendants(selected, tree);
            self.rebuild_filtered_visible(tree);
            self.dirty = false;
            return;
        }

        self.selected = Some(node.children()[0]);
    }

    /// Collapses the selected node or moves focus to its parent when already closed.
    pub fn collapse(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        let Some(node) = tree.node(selected) else {
            return;
        };

        // If the node has children and is open, close it first.
        if !node.children().is_empty() && self.open[selected.0] {
            self.open[selected.0] = false;
            self.prune_descendants(selected);
            self.rebuild_filtered_visible(tree);
            self.dirty = false;
            return;
        }

        // Otherwise move focus to its parent when possible.
        if let Some(parent) = node.parent() {
            self.selected = Some(parent);
        }
    }

    /// Moves the selection to the parent node, if any.
    pub fn select_parent(&mut self, tree: &DependencyTree) {
        let Some(selected) = self.selected else {
            return;
        };

        if let Some(node) = tree.node(selected)
            && let Some(parent) = node.parent()
        {
            self.selected = Some(parent);
        }
    }

    /// Moves the selection to the next sibling, if any.
    pub fn select_next_sibling(&mut self, tree: &DependencyTree) {
        let Some(selected) = self.selected else {
            return;
        };
        let Some(node) = tree.node(selected) else {
            return;
        };

        let siblings: &[NodeId] = if let Some(parent) = node.parent() {
            let Some(parent_node) = tree.node(parent) else {
                return;
            };
            parent_node.children()
        } else {
            tree.roots()
        };

        let Some(pos) = siblings.iter().position(|&id| id == selected) else {
            return;
        };

        if pos + 1 < siblings.len() {
            self.selected = Some(siblings[pos + 1]);
        }
    }

    /// Moves the selection to the previous sibling, if any.
    pub fn select_previous_sibling(&mut self, tree: &DependencyTree) {
        let Some(selected) = self.selected else {
            return;
        };
        let Some(node) = tree.node(selected) else {
            return;
        };

        let siblings: &[NodeId] = if let Some(parent) = node.parent() {
            let Some(parent_node) = tree.node(parent) else {
                return;
            };
            parent_node.children()
        } else {
            tree.roots()
        };

        let Some(pos) = siblings.iter().position(|&id| id == selected) else {
            return;
        };

        if pos > 0 {
            self.selected = Some(siblings[pos - 1]);
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

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        let next = {
            let visible = self.visible_nodes(tree);
            let Some(current_index) = Self::selected_index(visible, selected) else {
                return;
            };

            let len = visible.len() as isize;
            if len == 0 {
                return;
            }

            let mut next_index = current_index as isize + delta;
            if next_index < 0 {
                next_index = 0;
            } else if next_index >= len {
                next_index = len - 1;
            }

            visible.get(next_index as usize).map(|node| node.id)
        };

        if let Some(next_id) = next {
            self.selected = Some(next_id);
        }
    }

    /// Opens all nodes up to the specified depth.
    pub fn open_to_depth(&mut self, tree: &DependencyTree, max_depth: usize) {
        if max_depth == 0 {
            return;
        }
        self.ensure_node_capacity(tree);
        self.open.fill(false);
        for &root in tree.roots() {
            self.open_node(tree, root, 1, max_depth);
        }
        self.dirty = true;
        self.ensure_selection(tree);
    }

    fn open_node(&mut self, tree: &DependencyTree, id: NodeId, depth: usize, max_depth: usize) {
        if depth >= max_depth {
            return;
        }

        if let Some(node) = tree.node(id) {
            // Do not mark leaves as open to avoid confusing collapse semantics.
            if node.children().is_empty() {
                return;
            }

            self.open[id.0] = true;
            for &child in node.children() {
                self.open_node(tree, child, depth + 1, max_depth);
            }
        }
    }

    /// Removes all descendants of `id` from the visible cache in-place.
    fn prune_descendants(&mut self, id: NodeId) {
        let Some(start) = self.visible_cache.iter().position(|node| node.id == id) else {
            return;
        };
        let Some(depth) = self.visible_cache.get(start).map(|node| node.depth) else {
            return;
        };

        let first_descendant = start + 1;
        if first_descendant >= self.visible_cache.len() {
            return;
        }

        let end = self.visible_cache[first_descendant..]
            .iter()
            .position(|node| node.depth <= depth)
            .map(|offset| first_descendant + offset)
            .unwrap_or(self.visible_cache.len());

        self.visible_cache.drain(first_descendant..end);
    }

    /// Inserts the visible descendants of `id` into the cache in-place.
    fn insert_descendants(&mut self, id: NodeId, tree: &DependencyTree) {
        let Some(start) = self.visible_cache.iter().position(|node| node.id == id) else {
            return;
        };
        let Some(depth) = self.visible_cache.get(start).map(|node| node.depth) else {
            return;
        };

        let mut subtree = Vec::new();
        Self::collect_visible(&self.open, tree, id, depth, &mut subtree);
        if subtree.len() <= 1 {
            return;
        }

        let insert_at = start + 1;
        self.visible_cache
            .splice(insert_at..insert_at, subtree.into_iter().skip(1));
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

    fn rebuild_visible(&mut self, tree: &DependencyTree) {
        self.visible_cache.clear();
        let open = &self.open;
        for &root in tree.roots() {
            Self::collect_visible(open, tree, root, 0, &mut self.visible_cache);
        }
        Self::populate_last_non_group_child_map(
            tree,
            &self.visible_cache,
            &mut self.visible_last_non_group_child,
        );
        self.rebuild_filtered_visible(tree);
    }

    /// Rebuilds the visible slice for the active search result while preserving the main cache.
    fn rebuild_filtered_visible(&mut self, tree: &DependencyTree) {
        self.search_visible_cache.clear();
        self.search_visible_last_non_group_child.fill(None);
        if self.search_match_ids.is_empty() {
            return;
        }

        self.search_visible_cache.extend(
            self.visible_cache
                .iter()
                .copied()
                .filter(|node| self.search_visible_nodes[node.id.0]),
        );
        Self::populate_last_non_group_child_map(
            tree,
            &self.search_visible_cache,
            &mut self.search_visible_last_non_group_child,
        );
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

            current = tree.node(node_id).and_then(|node| node.parent());
        }
    }

    fn collect_visible(
        open: &[bool],
        tree: &DependencyTree,
        id: NodeId,
        depth: usize,
        out: &mut Vec<VisibleNode>,
    ) {
        out.push(VisibleNode { id, depth });

        if !open[id.0] {
            return;
        }

        if let Some(node) = tree.node(id) {
            for &child in node.children() {
                Self::collect_visible(open, tree, child, depth + 1, out);
            }
        }
    }

    /// Ensures the selection points to a valid visible node, defaulting to the first entry.
    ///
    /// Returns `true` if a valid selection exists after the operation.
    fn ensure_selection(&mut self, tree: &DependencyTree) -> bool {
        let selected = self.selected;
        self.ensure_visible_nodes(tree);
        let visible = self.active_visible_nodes();

        if visible.is_empty() {
            self.selected = None;
            return false;
        }

        if let Some(selected) = selected
            && visible.iter().any(|node| node.id == selected)
        {
            return true;
        }

        self.selected = Some(visible[0].id);
        true
    }

    /// Returns the index of the selected node among visible nodes.
    pub fn selected_position(&mut self, tree: &DependencyTree) -> Option<usize> {
        if !self.ensure_selection(tree) {
            return None;
        }

        let selected = self.selected?;
        let visible = self.active_visible_nodes();
        Self::selected_index(visible, selected)
    }

    /// Helper to find the index of the selected node in the visible list.
    fn selected_index(visible: &[VisibleNode], target: NodeId) -> Option<usize> {
        visible.iter().position(|node| node.id == target)
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

    /// Records, for each parent in the active visible slice, the last visible non-group child.
    ///
    /// The render path uses this cache to decide whether a node should draw a continuing
    /// branch (`├──`) or a terminating branch (`└──`) without rescanning siblings.
    /// Group nodes are skipped because they are labels and do not keep branch guides alive.
    fn populate_last_non_group_child_map(
        tree: &DependencyTree,
        visible_nodes: &[VisibleNode],
        target: &mut [Option<NodeId>],
    ) {
        target.fill(None);
        for node in visible_nodes {
            let Some(tree_node) = tree.node(node.id) else {
                continue;
            };

            if tree_node.is_group() {
                continue;
            }

            if let Some(parent_id) = tree_node.parent() {
                target[parent_id.0] = Some(node.id);
            }
        }
    }
}
