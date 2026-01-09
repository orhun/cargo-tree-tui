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
            if let Some(grand_id) = ancestor.parent() {
                let grand = tree.node(grand_id)?;
                let has_more_siblings = grand.children().last().copied() != Some(ancestor_id);
                lineage.push(LineageSegment {
                    has_more_siblings,
                    style: ancestor.as_group().map(|group| group.kind.style()),
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

}
