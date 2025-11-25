use std::collections::HashSet;

use crate::core::{DependencyTree, NodeId};

/// [`TreeWidget`] state that tracks open nodes and the current selection.
///
/// [`TreeWidget`]: crate::util::tui::tree_widget::TreeWidget
#[derive(Debug, Clone, Default)]
pub struct TreeWidgetState {
    /// Set of expanded nodes.
    pub open: HashSet<NodeId>,
    /// Currently selected node.
    pub selected: Option<NodeId>,
    /// Viewport height for page-based navigation (set by the widget during render).
    pub viewport_height: usize,
}

/// Visible node metadata used for navigation.
#[derive(Debug, Clone, Copy)]
pub struct VisibleNode {
    /// Node identifier.
    pub id: NodeId,
    /// Depth in the tree hierarchy.
    pub depth: usize,
}

impl TreeWidgetState {
    /// Moves the selection to the next visible dependency.
    pub fn select_next(&mut self, tree: &DependencyTree) {
        let visible = match self.ensure_selection(tree) {
            Some(visible) => visible,
            None => return,
        };

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        if let Some(current_index) = Self::selected_index(&visible, selected)
            && current_index + 1 < visible.len()
        {
            self.selected = Some(visible[current_index + 1].id);
        }
    }

    /// Moves the selection to the previous visible dependency.
    pub fn select_previous(&mut self, tree: &DependencyTree) {
        let visible = match self.ensure_selection(tree) {
            Some(visible) => visible,
            None => return,
        };

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        if let Some(current_index) = Self::selected_index(&visible, selected)
            && current_index > 0
        {
            self.selected = Some(visible[current_index - 1].id);
        }
    }

    /// Moves the selection to the first visible dependency.
    pub fn select_first(&mut self, tree: &DependencyTree) {
        let visible = match self.ensure_selection(tree) {
            Some(visible) => visible,
            None => return,
        };

        if !visible.is_empty() {
            self.selected = Some(visible[0].id);
        }
    }

    /// Moves the selection to the last visible dependency.
    pub fn select_last(&mut self, tree: &DependencyTree) {
        let visible = match self.ensure_selection(tree) {
            Some(visible) => visible,
            None => return,
        };

        if let Some(last) = visible.last() {
            self.selected = Some(last.id);
        }
    }

    /// Moves selection down by half a page.
    pub fn select_half_page_down(&mut self, tree: &DependencyTree) {
        let jump = (self.viewport_height / 2).max(1);
        self.select_by_offset(tree, jump as isize);
    }

    /// Moves selection up by half a page.
    pub fn select_half_page_up(&mut self, tree: &DependencyTree) {
        let jump = (self.viewport_height / 2).max(1);
        self.select_by_offset(tree, -(jump as isize));
    }

    /// Moves selection down by a full page.
    pub fn select_page_down(&mut self, tree: &DependencyTree) {
        let jump = self.viewport_height.max(1);
        self.select_by_offset(tree, jump as isize);
    }

    /// Moves selection up by a full page.
    pub fn select_page_up(&mut self, tree: &DependencyTree) {
        let jump = self.viewport_height.max(1);
        self.select_by_offset(tree, -(jump as isize));
    }

    /// Moves selection by a signed offset (positive = down, negative = up).
    fn select_by_offset(&mut self, tree: &DependencyTree, offset: isize) {
        let visible = match self.ensure_selection(tree) {
            Some(visible) => visible,
            None => return,
        };

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        if let Some(current_index) = Self::selected_index(&visible, selected) {
            let new_index = if offset >= 0 {
                (current_index + offset as usize).min(visible.len().saturating_sub(1))
            } else {
                current_index.saturating_sub((-offset) as usize)
            };
            self.selected = Some(visible[new_index].id);
        }
    }

    /// Jumps to the parent of the currently selected node.
    pub fn select_parent(&mut self, tree: &DependencyTree) {
        if self.ensure_selection(tree).is_none() {
            return;
        }

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        if let Some(node) = tree.node(selected)
            && let Some(parent) = node.parent
        {
            self.selected = Some(parent);
        }
    }

    /// Jumps to the next sibling at the same depth.
    pub fn select_next_sibling(&mut self, tree: &DependencyTree) {
        let visible = match self.ensure_selection(tree) {
            Some(visible) => visible,
            None => return,
        };

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        let Some(current_index) = Self::selected_index(&visible, selected) else {
            return;
        };

        let current_depth = visible[current_index].depth;

        // Find next node at the same depth
        for node in visible.iter().skip(current_index + 1) {
            if node.depth == current_depth {
                self.selected = Some(node.id);
                return;
            }
            // If we encounter a shallower node, there are no more siblings
            if node.depth < current_depth {
                break;
            }
        }
    }

    /// Jumps to the previous sibling at the same depth.
    pub fn select_previous_sibling(&mut self, tree: &DependencyTree) {
        let visible = match self.ensure_selection(tree) {
            Some(visible) => visible,
            None => return,
        };

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        let Some(current_index) = Self::selected_index(&visible, selected) else {
            return;
        };

        let current_depth = visible[current_index].depth;

        // Find previous node at the same depth
        for node in visible.iter().take(current_index).rev() {
            if node.depth == current_depth {
                self.selected = Some(node.id);
                return;
            }
            // If we encounter a shallower node, there are no more siblings
            if node.depth < current_depth {
                break;
            }
        }
    }

    /// Toggles the expanded state of the currently selected node.
    pub fn toggle(&mut self, tree: &DependencyTree) {
        if self.ensure_selection(tree).is_none() {
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

        if self.open.contains(&selected) {
            self.open.remove(&selected);
        } else {
            self.open.insert(selected);
        }
    }

    /// Recursively expands all descendants of the selected node.
    pub fn expand_all(&mut self, tree: &DependencyTree) {
        if self.ensure_selection(tree).is_none() {
            return;
        }

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        self.expand_recursive(tree, selected);
    }

    fn expand_recursive(&mut self, tree: &DependencyTree, id: NodeId) {
        let Some(node) = tree.node(id) else {
            return;
        };

        if !node.children.is_empty() {
            self.open.insert(id);
            for &child in &node.children {
                self.expand_recursive(tree, child);
            }
        }
    }

    /// Collapses all nodes in the tree.
    pub fn collapse_all(&mut self, tree: &DependencyTree) {
        self.open.clear();
        // Move selection to first root if available
        if let Some(&first_root) = tree.roots().first() {
            self.selected = Some(first_root);
        }
    }

    /// Expands the selected node or moves into its first child when already expanded.
    pub fn expand(&mut self, tree: &DependencyTree) {
        if self.ensure_selection(tree).is_none() {
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
            return;
        }

        self.selected = Some(node.children[0]);
    }

    /// Collapses the selected node or moves focus to its parent when already closed.
    pub fn collapse(&mut self, tree: &DependencyTree) {
        if self.ensure_selection(tree).is_none() {
            return;
        }

        let selected = match self.selected {
            Some(id) => id,
            None => return,
        };

        if self.open.remove(&selected) {
            return;
        }

        if let Some(node) = tree.node(selected)
            && let Some(parent) = node.parent
        {
            self.selected = Some(parent);
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
        let _ = self.ensure_selection(tree);
    }

    fn open_node(&mut self, tree: &DependencyTree, id: NodeId, depth: usize, max_depth: usize) {
        if depth >= max_depth {
            return;
        }

        self.open.insert(id);
        if let Some(node) = tree.node(id) {
            for &child in &node.children {
                self.open_node(tree, child, depth + 1, max_depth);
            }
        }
    }

    /// Returns visible nodes along with their depth in the hierarchy.
    pub fn visible_nodes(&self, tree: &DependencyTree) -> Vec<VisibleNode> {
        let mut out = Vec::new();
        for &root in tree.roots() {
            self.collect_visible(tree, root, 0, &mut out);
        }
        out
    }

    fn collect_visible(
        &self,
        tree: &DependencyTree,
        id: NodeId,
        depth: usize,
        out: &mut Vec<VisibleNode>,
    ) {
        out.push(VisibleNode { id, depth });

        if !self.open.contains(&id) {
            return;
        }

        if let Some(node) = tree.node(id) {
            for &child in &node.children {
                self.collect_visible(tree, child, depth + 1, out);
            }
        }
    }

    /// Ensures the selection points to a valid visible node, defaulting to the first entry.
    fn ensure_selection(&mut self, tree: &DependencyTree) -> Option<Vec<VisibleNode>> {
        let visible = self.visible_nodes(tree);

        if visible.is_empty() {
            self.selected = None;
            return None;
        }

        if let Some(selected) = self.selected
            && visible.iter().any(|node| node.id == selected)
        {
            return Some(visible);
        }

        self.selected = Some(visible[0].id);
        Some(visible)
    }

    /// Returns the index of the selected node among visible nodes.
    pub fn selected_position(&mut self, tree: &DependencyTree) -> Option<usize> {
        let visible = self.ensure_selection(tree)?;
        let selected = self.selected?;
        Self::selected_index(&visible, selected)
    }

    /// Helper to find the index of the selected node in the visible list.
    fn selected_index(visible: &[VisibleNode], target: NodeId) -> Option<usize> {
        visible.iter().position(|node| node.id == target)
    }
}
