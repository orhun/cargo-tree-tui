use crate::core::DependencyTree;

use super::{
    state::{TreeItem, child_items, ui_parent},
    style::TreeWidgetStyle,
};

/// Lineage information for a dependency node.
#[derive(Debug)]
pub struct Lineage {
    /// For each ancestor from root → parent, whether there are more siblings (`true` = draw continuation).
    pub segments: Vec<bool>,
    /// Whether the current node is the last child of its parent.
    pub is_last: bool,
    /// Whether this node is the currently selected one.
    pub is_selected: bool,
}

impl Lineage {
    /// Builds lineage information for a node.
    pub fn build(
        tree: &DependencyTree,
        item: TreeItem,
        selected: Option<TreeItem>,
    ) -> Option<Self> {
        let parent = ui_parent(tree, item);

        let is_last = if let Some(parent_item) = parent {
            let siblings = child_items(tree, parent_item);
            siblings.last().copied() == Some(item)
        } else {
            true
        };

        let mut lineage = Vec::new();
        let mut current = parent;

        while let Some(ancestor) = current {
            let grand_parent = ui_parent(tree, ancestor);
            let has_more_siblings = if let Some(grand) = grand_parent {
                let siblings = child_items(tree, grand);
                siblings
                    .last()
                    .copied()
                    .map(|last| last != ancestor)
                    .unwrap_or(false)
            } else {
                false
            };

            lineage.push(has_more_siblings);
            current = grand_parent;
        }

        lineage.reverse();
        Some(Lineage {
            segments: lineage,
            is_last,
            is_selected: selected == Some(item),
        })
    }

    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    pub fn has_segments(&self) -> bool {
        !self.segments.is_empty()
    }

    pub fn indent(&self, style: &TreeWidgetStyle) -> String {
        self.segments
            .iter()
            .map(|&has_more| {
                if has_more {
                    style.continuation_symbol
                } else {
                    style.empty_symbol
                }
            })
            .collect()
    }
}
