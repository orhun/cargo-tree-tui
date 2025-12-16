use std::collections::HashSet;

use crate::core::{DependencyTree, DependencyType, NodeId};

use super::viewport::Viewport;

/// UI-level representation of an item in the dependency tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeItem {
    Crate {
        id: NodeId,
    },
    DependencyGroup {
        parent: NodeId,
        kind: DependencyType,
    },
}

/// [`TreeWidget`] state that tracks open nodes and the current selection.
///
/// [`TreeWidget`]: super::TreeWidget
#[derive(Debug)]
pub struct TreeWidgetState {
    /// Set of expanded nodes.
    pub open: HashSet<TreeItem>,
    /// Currently selected node.
    pub selected: Option<TreeItem>,
    /// Current viewport.
    viewport: Viewport,
    /// Cached visible nodes.
    visible_cache: Vec<VisibleRow>,
    /// Indicates whether the visible cache is outdated.
    dirty: bool,
}

/// Visible item metadata used for navigation.
#[derive(Debug, Clone, Copy)]
pub struct VisibleRow {
    /// Visible tree item.
    pub item: TreeItem,
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
        }
    }
}

/// Returns the parent [`TreeItem`] in the UI hierarchy.
pub(crate) fn ui_parent(tree: &DependencyTree, item: TreeItem) -> Option<TreeItem> {
    match item {
        TreeItem::Crate { id } => {
            let node = tree.node(id)?;
            let parent_id = node.parent?;
            let parent_node = tree.node(parent_id)?;
            let edge_kind = parent_node
                .children
                .iter()
                .find(|edge| edge.target == id)?
                .kind;

            match edge_kind {
                DependencyType::Normal => Some(TreeItem::Crate { id: parent_id }),
                kind => Some(TreeItem::DependencyGroup {
                    parent: parent_id,
                    kind,
                }),
            }
        }
        TreeItem::DependencyGroup { parent, .. } => Some(TreeItem::Crate { id: parent }),
    }
}

/// Returns the ordered UI children of a given [`TreeItem`].
pub(crate) fn child_items(tree: &DependencyTree, item: TreeItem) -> Vec<TreeItem> {
    match item {
        TreeItem::Crate { id } => crate_child_items(tree, id),
        TreeItem::DependencyGroup { parent, kind } => group_children(tree, parent, kind),
    }
}

/// Returns siblings for the given [`TreeItem`] within the UI tree.
pub(crate) fn siblings_of(tree: &DependencyTree, item: TreeItem) -> Vec<TreeItem> {
    if let Some(parent) = ui_parent(tree, item) {
        child_items(tree, parent)
    } else {
        tree.roots()
            .iter()
            .map(|&id| TreeItem::Crate { id })
            .collect()
    }
}

fn crate_child_items(tree: &DependencyTree, id: NodeId) -> Vec<TreeItem> {
    let Some(node) = tree.node(id) else {
        return Vec::new();
    };

    let mut items = Vec::new();
    let mut group_order = Vec::new();

    for edge in &node.children {
        match edge.kind {
            DependencyType::Normal => items.push(TreeItem::Crate { id: edge.target }),
            kind => {
                if !group_order.contains(&kind) {
                    group_order.push(kind);
                }
            }
        }
    }

    for kind in group_order {
        items.push(TreeItem::DependencyGroup { parent: id, kind });
    }

    items
}

fn group_children(tree: &DependencyTree, parent: NodeId, kind: DependencyType) -> Vec<TreeItem> {
    let Some(node) = tree.node(parent) else {
        return Vec::new();
    };

    node.children
        .iter()
        .filter(|edge| edge.kind == kind)
        .map(|edge| TreeItem::Crate { id: edge.target })
        .collect()
}

impl TreeWidgetState {
    /// Moves the selection to the next visible dependency.
    pub fn select_next(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let selected = match self.selected {
            Some(item) => item,
            None => return,
        };

        let next = {
            let visible = self.visible_rows(tree);
            let Some(current_index) = Self::selected_index(visible, selected) else {
                return;
            };

            visible.get(current_index + 1).map(|row| row.item)
        };

        if let Some(next_item) = next {
            self.selected = Some(next_item);
        }
    }

    /// Moves the selection to the previous visible dependency.
    pub fn select_previous(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let selected = match self.selected {
            Some(item) => item,
            None => return,
        };

        let previous = {
            let visible = self.visible_rows(tree);
            let Some(current_index) = Self::selected_index(visible, selected) else {
                return;
            };

            if current_index > 0 {
                Some(visible[current_index - 1].item)
            } else {
                None
            }
        };

        if let Some(previous_item) = previous {
            self.selected = Some(previous_item);
        }
    }

    /// Expands the selected node or moves into its first child when already expanded.
    pub fn expand(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let selected = match self.selected {
            Some(item) => item,
            None => return,
        };

        let children = child_items(tree, selected);
        if children.is_empty() {
            return;
        };

        if self.open.insert(selected) {
            self.insert_descendants(selected, tree);
            self.dirty = false;
            return;
        }

        if let Some(first_child) = children.first() {
            self.selected = Some(*first_child);
        }
    }

    /// Collapses the selected node or moves focus to its parent when already closed.
    pub fn collapse(&mut self, tree: &DependencyTree) {
        if !self.ensure_selection(tree) {
            return;
        }

        let selected = match self.selected {
            Some(item) => item,
            None => return,
        };

        let has_children = !child_items(tree, selected).is_empty();

        // If the node has children and is open, close it first.
        if has_children && self.open.remove(&selected) {
            self.prune_descendants(selected);
            self.dirty = false;
            return;
        }

        // Otherwise move focus to its parent when possible.
        if let Some(parent) = ui_parent(tree, selected) {
            self.selected = Some(parent);
        }
    }

    /// Moves the selection to the parent node, if any.
    pub fn select_parent(&mut self, tree: &DependencyTree) {
        let Some(selected) = self.selected else {
            return;
        };
        if let Some(parent) = ui_parent(tree, selected) {
            self.selected = Some(parent);
        };
    }

    /// Moves the selection to the next sibling, if any.
    pub fn select_next_sibling(&mut self, tree: &DependencyTree) {
        let Some(selected) = self.selected else {
            return;
        };
        let siblings = siblings_of(tree, selected);
        let Some(pos) = siblings.iter().position(|&item| item == selected) else {
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
        let siblings = siblings_of(tree, selected);
        let Some(pos) = siblings.iter().position(|&item| item == selected) else {
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
            Some(item) => item,
            None => return,
        };

        let next = {
            let visible = self.visible_rows(tree);
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

            visible.get(next_index as usize).map(|row| row.item)
        };

        if let Some(next_item) = next {
            self.selected = Some(next_item);
        }
    }

    /// Opens all nodes up to the specified depth.
    pub fn open_to_depth(&mut self, tree: &DependencyTree, max_depth: usize) {
        if max_depth == 0 {
            return;
        }
        self.open.clear();
        for &root in tree.roots() {
            self.open_node(tree, TreeItem::Crate { id: root }, 1, max_depth);
        }
        self.dirty = true;
        self.ensure_selection(tree);
    }

    fn open_node(&mut self, tree: &DependencyTree, item: TreeItem, depth: usize, max_depth: usize) {
        if depth >= max_depth {
            return;
        }

        let children = child_items(tree, item);
        // Do not mark leaves as open to avoid confusing collapse semantics.
        if children.is_empty() {
            return;
        }

        self.open.insert(item);
        for child in children {
            self.open_node(tree, child, depth + 1, max_depth);
        }
    }

    /// Removes all descendants of `id` from the visible cache in-place.
    fn prune_descendants(&mut self, item: TreeItem) {
        let Some(start) = self.visible_cache.iter().position(|row| row.item == item) else {
            return;
        };
        let Some(depth) = self.visible_cache.get(start).map(|row| row.depth) else {
            return;
        };

        let first_descendant = start + 1;
        if first_descendant >= self.visible_cache.len() {
            return;
        }

        let end = self.visible_cache[first_descendant..]
            .iter()
            .position(|row| row.depth <= depth)
            .map(|offset| first_descendant + offset)
            .unwrap_or(self.visible_cache.len());

        self.visible_cache.drain(first_descendant..end);
    }

    /// Inserts the visible descendants of `id` into the cache in-place.
    fn insert_descendants(&mut self, item: TreeItem, tree: &DependencyTree) {
        let Some(start) = self.visible_cache.iter().position(|row| row.item == item) else {
            return;
        };
        let Some(depth) = self.visible_cache.get(start).map(|row| row.depth) else {
            return;
        };

        let mut subtree = Vec::new();
        Self::collect_visible(&self.open, tree, item, depth, &mut subtree);
        if subtree.len() <= 1 {
            return;
        }

        let insert_at = start + 1;
        self.visible_cache
            .splice(insert_at..insert_at, subtree.into_iter().skip(1));
    }

    /// Returns cached visible nodes along with their depth in the hierarchy.
    pub fn visible_rows(&mut self, tree: &DependencyTree) -> &[VisibleRow] {
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
            Self::collect_visible(
                open,
                tree,
                TreeItem::Crate { id: root },
                0,
                &mut self.visible_cache,
            );
        }
    }

    fn collect_visible(
        open: &HashSet<TreeItem>,
        tree: &DependencyTree,
        item: TreeItem,
        depth: usize,
        out: &mut Vec<VisibleRow>,
    ) {
        out.push(VisibleRow { item, depth });

        if !open.contains(&item) {
            return;
        }

        for child in child_items(tree, item) {
            Self::collect_visible(open, tree, child, depth + 1, out);
        }
    }

    /// Ensures the selection points to a valid visible node, defaulting to the first entry.
    ///
    /// Returns `true` if a valid selection exists after the operation.
    fn ensure_selection(&mut self, tree: &DependencyTree) -> bool {
        let _ = self.visible_rows(tree);

        if self.visible_cache.is_empty() {
            self.selected = None;
            return false;
        }

        if let Some(selected) = self.selected
            && self.visible_cache.iter().any(|row| row.item == selected)
        {
            return true;
        }

        self.selected = Some(self.visible_cache[0].item);
        true
    }

    /// Returns the index of the selected node among visible nodes.
    pub fn selected_position(&mut self, tree: &DependencyTree) -> Option<usize> {
        if !self.ensure_selection(tree) {
            return None;
        }

        let selected = self.selected?;
        let visible = self.visible_rows(tree);
        Self::selected_index(visible, selected)
    }

    /// Helper to find the index of the selected node in the visible list.
    fn selected_index(visible: &[VisibleRow], target: TreeItem) -> Option<usize> {
        visible.iter().position(|row| row.item == target)
    }

    /// Updates the available viewport.
    pub(crate) fn update_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }

    /// Expands all nodes in the tree.
    pub fn expand_all(&mut self, tree: &DependencyTree) {
        self.open.clear();
        for &root in tree.roots() {
            self.open_node(tree, TreeItem::Crate { id: root }, 1, usize::MAX);
        }
        self.dirty = true;
        self.ensure_selection(tree);
    }
}
