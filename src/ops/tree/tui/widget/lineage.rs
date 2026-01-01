use crate::core::{DependencyTree, NodeId};

use super::style::TreeWidgetStyle;

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
    pub fn build(tree: &DependencyTree, node_id: NodeId, selected: Option<NodeId>) -> Option<Self> {
        let node = tree.node(node_id)?;

        let is_last = match node.parent() {
            Some(parent_id) => {
                let parent = tree.node(parent_id)?;
                parent.children().last().copied() == Some(node_id)
            }
            None => true,
        };

        let mut lineage = Vec::new();
        let mut current = node.parent();

        while let Some(ancestor_id) = current {
            let ancestor = tree.node(ancestor_id)?;
            let has_more_siblings = if let Some(grand_id) = ancestor.parent() {
                let grand = tree.node(grand_id)?;
                grand.children().last().copied() != Some(ancestor_id)
            } else {
                false
            };

            lineage.push(has_more_siblings);
            current = ancestor.parent();
        }

        lineage.reverse();
        Some(Lineage {
            segments: lineage,
            is_last,
            is_selected: selected == Some(node_id),
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
