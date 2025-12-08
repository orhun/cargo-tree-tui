use std::collections::HashSet;

use crate::{
    core::{DependencyTree, NodeId},
    ops::tree::tui::widget::Viewport,
};

/// [`TreeWidget`] state that tracks open nodes and the current selection.
///
/// [`TreeWidget`]: crate::util::tui::tree_widget::TreeWidget
#[derive(Debug)]
pub struct TreeWidgetState {
    /// Set of expanded nodes.
    pub open: HashSet<NodeId>,
    /// Currently selected node.
    pub selected: Option<NodeId>,
    /// Current viewport.
    viewport: Viewport,
    /// Cached visible nodes.
    visible_cache: Vec<VisibleNode>,
    /// Indicates whether the visible cache is outdated.
    dirty: bool,
    /// Current search query (lowercased) used for highlighting.
    pub search_query: Option<String>,
    /// Nodes that match the active search.
    search_matches: Vec<NodeId>,
}

/// Visible node metadata used for navigation.
#[derive(Debug, Clone, Copy)]
pub struct VisibleNode {
    /// Node identifier.
    pub id: NodeId,
    /// Depth in the tree hierarchy.
    pub depth: usize,
}

impl Default for TreeWidgetState {
    fn default() -> Self {
        Self {
            open: HashSet::new(),
            selected: None,
            viewport: Viewport::default(),
            visible_cache: Vec::new(),
            dirty: true,
            search_query: None,
            search_matches: Vec::new(),
        }
    }
}

impl TreeWidgetState {
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

        if node.children.is_empty() {
            return;
        }

        if self.open.insert(selected) {
            self.dirty = true;
            return;
        }

        self.selected = Some(node.children[0]);
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
        if !node.children.is_empty() && self.open.remove(&selected) {
            self.dirty = true;
            return;
        }

        // Otherwise move focus to its parent when possible.
        if let Some(parent) = node.parent {
            self.selected = Some(parent);
        }
    }

    /// Moves the selection to the parent node, if any.
    pub fn select_parent(&mut self, tree: &DependencyTree) {
        let Some(selected) = self.selected else {
            return;
        };

        if let Some(node) = tree.node(selected)
            && let Some(parent) = node.parent
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

        let Some(parent) = node.parent else {
            return;
        };
        let Some(parent_node) = tree.node(parent) else {
            return;
        };

        let Some(pos) = parent_node.children.iter().position(|&id| id == selected) else {
            return;
        };

        if pos + 1 < parent_node.children.len() {
            self.selected = Some(parent_node.children[pos + 1]);
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

        let Some(parent) = node.parent else {
            return;
        };
        let Some(parent_node) = tree.node(parent) else {
            return;
        };

        let Some(pos) = parent_node.children.iter().position(|&id| id == selected) else {
            return;
        };

        if pos > 0 {
            self.selected = Some(parent_node.children[pos - 1]);
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
            if node.children.is_empty() {
                return;
            }

            self.open.insert(id);
            for &child in &node.children {
                self.open_node(tree, child, depth + 1, max_depth);
            }
        }
    }

    /// Returns cached visible nodes along with their depth in the hierarchy.
    pub fn visible_nodes(&mut self, tree: &DependencyTree) -> &[VisibleNode] {
        if self.dirty {
            self.rebuild_visible(tree);
            self.dirty = false;
        }
        &self.visible_cache
    }

    fn rebuild_visible(&mut self, tree: &DependencyTree) {
        self.visible_cache.clear();
        let open = &self.open;
        for &root in tree.roots() {
            Self::collect_visible(open, tree, root, 0, &mut self.visible_cache);
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
            for &child in &node.children {
                Self::collect_visible(open, tree, child, depth + 1, out);
            }
        }
    }

    /// Ensures the selection points to a valid visible node, defaulting to the first entry.
    ///
    /// Returns `true` if a valid selection exists after the operation.
    fn ensure_selection(&mut self, tree: &DependencyTree) -> bool {
        let _ = self.visible_nodes(tree);

        if self.visible_cache.is_empty() {
            self.selected = None;
            return false;
        }

        if let Some(selected) = self.selected
            && self.visible_cache.iter().any(|node| node.id == selected)
        {
            return true;
        }

        self.selected = Some(self.visible_cache[0].id);
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

    /// Returns whether the node matches the active search.
    pub fn is_search_match(&self, id: NodeId) -> bool {
        self.search_query.is_some() && self.search_matches.contains(&id)
    }

    /// Expands all nodes in the tree.
    pub fn expand_all(&mut self, tree: &DependencyTree) {
        self.open.clear();
        for i in 0..tree.nodes.len() {
            let id = NodeId(i);
            if let Some(node) = tree.node(id) {
                // Only mark non-leaf nodes as open, leaves stay implicit.
                if !node.children.is_empty() {
                    self.open.insert(id);
                }
            }
        }
        self.dirty = true;
        self.ensure_selection(tree);
    }

    /// Updates search state and moves selection to the next visible match when available.
    pub fn search(&mut self, tree: &DependencyTree, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            self.search_query = None;
            self.search_matches.clear();
            return;
        }

        let needle = query.to_ascii_lowercase();
        self.search_query = Some(needle.clone());
        self.search_matches.clear();

        for (idx, node) in tree.nodes.iter().enumerate() {
            if node.name.to_ascii_lowercase().contains(&needle) {
                self.search_matches.push(NodeId(idx));
            }
        }

        if self.search_matches.is_empty() || !self.ensure_selection(tree) {
            return;
        }

        let visible = self.visible_nodes(tree).to_vec();
        let matches = self.search_matches.clone();
        let Some(current_index) = self
            .selected
            .and_then(|id| Self::selected_index(&visible, id))
        else {
            return;
        };

        // Prefer the first match at or after the current selection, otherwise wrap.
        let mut next_match = None;
        for (idx, visible_node) in visible.iter().enumerate().skip(current_index) {
            if matches.contains(&visible_node.id) {
                next_match = Some(idx);
                break;
            }
        }

        if next_match.is_none() {
            next_match = visible
                .iter()
                .enumerate()
                .find(|(_, node)| matches.contains(&node.id))
                .map(|(idx, _)| idx);
        }

        if let Some(idx) = next_match {
            if let Some(target) = visible.get(idx) {
                self.selected = Some(target.id);
            }
        }
    }
}
