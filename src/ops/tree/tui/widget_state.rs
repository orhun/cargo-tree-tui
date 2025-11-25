use std::collections::HashSet;

use crate::{
    core::{DependencyTree, NodeId},
    ops::tree::tui::widget::Viewport,
};

/// [`TreeWidget`] state that tracks open nodes and the current selection.
///
/// [`TreeWidget`]: crate::util::tui::tree_widget::TreeWidget
#[derive(Debug, Clone, Default)]
pub struct TreeWidgetState {
    /// Set of expanded nodes.
    pub open: HashSet<NodeId>,
    /// Currently selected node.
    pub selected: Option<NodeId>,
    /// Current viewport.
    viewport: Viewport,
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

        self.selected = Some(visible[next_index as usize].id);
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

    /// Updates the available viewport.
    pub(crate) fn update_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }
}
