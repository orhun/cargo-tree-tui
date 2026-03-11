use std::collections::HashSet;

use crate::core::{DependencyNode, DependencyTree, NodeId};

use super::viewport::Viewport;

/// [`TreeWidget`] state that tracks open nodes and the current selection.
///
/// [`TreeWidget`]: super::TreeWidget
#[derive(Debug)]
pub struct TreeWidgetState {
    /// Set of expanded nodes.
    pub open: HashSet<NodeId>,
    /// Currently selected node.
    pub selected: Option<NodeId>,
    /// Nodes kept visible by the active search.
    pub search_visible_nodes: HashSet<NodeId>,
    /// Nodes whose crate names directly match the active search query.
    search_matches: HashSet<NodeId>,
    /// Current viewport.
    pub viewport: Viewport,
    /// Cached visible nodes.
    visible_cache: Vec<VisibleNode>,
    /// Cached search-filtered visible nodes.
    search_visible_cache: Vec<VisibleNode>,
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

/// Search result payload applied to the visible tree state.
#[derive(Debug, Default, Clone)]
pub struct SearchState {
    /// Nodes kept visible by the active search.
    pub visible_nodes: HashSet<NodeId>,
    /// Nodes whose crate names directly match the active search query.
    pub matches: HashSet<NodeId>,
}

impl Default for TreeWidgetState {
    fn default() -> Self {
        Self {
            open: HashSet::new(),
            selected: None,
            search_visible_nodes: HashSet::new(),
            search_matches: HashSet::new(),
            viewport: Viewport::default(),
            visible_cache: Vec::new(),
            search_visible_cache: Vec::new(),
            dirty: true,
        }
    }
}

impl TreeWidgetState {
    /// Clears any active search filtering state.
    pub fn clear_search(&mut self) {
        self.search_visible_nodes.clear();
        self.search_matches.clear();
        self.search_visible_cache.clear();
    }

    /// Returns whether a node directly matches the active search query.
    pub fn is_search_match(&self, node_id: NodeId) -> bool {
        self.search_matches.contains(&node_id)
    }

    /// Applies externally computed search state to the visible tree.
    pub fn apply_search_state(&mut self, search_state: SearchState) {
        self.search_visible_nodes = search_state.visible_nodes;
        self.search_matches = search_state.matches;
        self.rebuild_filtered_visible();
    }

    /// Computes search-filtered nodes by matching crate names against an ASCII-lowercased query.
    pub fn search(tree: &DependencyTree, query: &str) -> SearchState {
        if query.is_empty() {
            return SearchState::default();
        }

        let mut search_state = SearchState::default();
        let query = query.to_ascii_lowercase();

        for &root in tree.roots() {
            Self::collect_filtered(
                tree,
                root,
                &query,
                &mut search_state.visible_nodes,
                &mut search_state.matches,
            );
        }

        search_state
    }

    /// Returns the set of nodes in the search-filtered view, if search is active.
    pub fn active_search_visible_nodes(&self) -> Option<&HashSet<NodeId>> {
        (!self.search_visible_cache.is_empty()).then_some(&self.search_visible_nodes)
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

        if self.open.contains(&selected) {
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

        if self.open.insert(selected) {
            self.insert_descendants(selected, tree);
            self.rebuild_filtered_visible();
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
        if !node.children().is_empty() && self.open.remove(&selected) {
            self.prune_descendants(selected);
            self.rebuild_filtered_visible();
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
        self.open.clear();
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

            self.open.insert(id);
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
        if self.dirty {
            self.rebuild_visible(tree);
            self.dirty = false;
        }

        self.active_visible_nodes()
    }

    fn active_visible_nodes(&self) -> &[VisibleNode] {
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
        self.rebuild_filtered_visible();
    }

    fn rebuild_filtered_visible(&mut self) {
        self.search_visible_cache.clear();
        if self.search_visible_nodes.is_empty() {
            return;
        }

        self.search_visible_cache.extend(
            self.visible_cache
                .iter()
                .copied()
                .filter(|node| self.search_visible_nodes.contains(&node.id)),
        );
    }

    fn collect_filtered(
        tree: &DependencyTree,
        id: NodeId,
        query: &str,
        search_visible_nodes: &mut HashSet<NodeId>,
        search_matches: &mut HashSet<NodeId>,
    ) -> bool {
        let Some(node) = tree.node(id) else {
            return false;
        };

        let is_match = matches!(
            node,
            DependencyNode::Crate(dependency)
                if dependency.name.to_ascii_lowercase().contains(query)
        );

        let mut has_visible_child = false;
        for &child in node.children() {
            has_visible_child |=
                Self::collect_filtered(tree, child, query, search_visible_nodes, search_matches);
        }

        if is_match || has_visible_child {
            search_visible_nodes.insert(id);
            if is_match {
                search_matches.insert(id);
            }
            true
        } else {
            false
        }
    }

    fn collect_visible(
        open: &HashSet<NodeId>,
        tree: &DependencyTree,
        id: NodeId,
        depth: usize,
        out: &mut Vec<VisibleNode>,
    ) {
        out.push(VisibleNode { id, depth });

        if !open.contains(&id) {
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
        let visible = self.visible_nodes(tree);

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
        let visible = self.visible_nodes(tree);
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
        self.open.clear();
        for i in 0..tree.nodes.len() {
            let id = NodeId(i);
            if let Some(node) = tree.node(id) {
                // Only mark non-leaf nodes as open, leaves stay implicit.
                if !node.children().is_empty() {
                    self.open.insert(id);
                }
            }
        }
        self.dirty = true;
        self.ensure_selection(tree);
    }
}
