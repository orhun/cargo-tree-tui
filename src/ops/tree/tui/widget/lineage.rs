use crate::core::{DependencyTree, NodeId};
use ratatui::style::Style;

/// Lineage information for a dependency node.
#[derive(Debug)]
pub struct Lineage {
    /// For each ancestor from root → parent, whether there are more siblings (`true` = draw continuation).
    pub segments: Vec<LineageSegment>,
    /// Whether the current node is the last child of its parent.
    pub is_last: bool,
    /// Whether this node is the currently selected one.
    pub is_selected: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LineageSegment {
    pub has_more_siblings: bool,
    pub style: Option<Style>,
    pub is_group: bool,
}

impl Lineage {
    /// Builds lineage information for a node.
    pub fn build(tree: &DependencyTree, node_id: NodeId, selected: Option<NodeId>) -> Option<Self> {
        let node = tree.node(node_id)?;

        let is_last = match node.parent() {
            Some(parent_id) => !Self::has_more_visible_siblings(tree, parent_id, node_id),
            None => true,
        };

        let mut lineage = Vec::new();
        let mut current = node.parent();

        while let Some(ancestor_id) = current {
            let ancestor = tree.node(ancestor_id)?;
            if let Some(grand_id) = ancestor.parent() {
                let has_more_siblings =
                    Self::has_more_visible_siblings(tree, grand_id, ancestor_id);
                lineage.push(LineageSegment {
                    has_more_siblings,
                    style: ancestor.as_group().map(|group| group.kind.style()),
                    is_group: ancestor.is_group(),
                });
            }
            current = ancestor.parent();
        }

        lineage.reverse();
        Some(Lineage {
            segments: lineage,
            is_last,
            is_selected: selected == Some(node_id),
        })
    }

    /// Returns true if the given node has any non-group sibling after it.
    ///
    /// # Notes
    ///
    /// - We use this when deciding `├──` vs `└──` and whether to draw `│` guides.
    /// - Group headers are labels, so they don't keep the branch guide `│` alive.
    fn has_more_visible_siblings(
        tree: &DependencyTree,
        parent_id: NodeId,
        node_id: NodeId,
    ) -> bool {
        // Missing parent means no siblings to consider.
        let Some(parent) = tree.node(parent_id) else {
            return false;
        };

        for &child in parent.children().iter().rev() {
            // Stop once we reach the current node.
            if child == node_id {
                break;
            }

            if let Some(node) = tree.node(child) {
                // Any non-group sibling keeps the branch alive.
                if !node.is_group() {
                    return true;
                }
            }
        }

        // Only group siblings (or none) appear after this node.
        false
    }
}
