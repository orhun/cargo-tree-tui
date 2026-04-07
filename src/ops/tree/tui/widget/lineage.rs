use crate::core::DependencyTree;
use ratatui::style::Style;

use super::state::{VisIdx, VisibleNode};

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
    pub edge_style: Option<Style>,
    pub is_group: bool,
}

impl Lineage {
    /// Builds lineage information for a visible node position.
    pub fn build(
        tree: &DependencyTree,
        visible_nodes: &[VisibleNode],
        vis_idx: VisIdx,
        selected_vis_idx: Option<VisIdx>,
    ) -> Option<Self> {
        let vnode = visible_nodes.get(vis_idx.0)?;
        tree.node(vnode.id)?;

        let is_last = vnode.is_last_non_group_child;

        let mut lineage = Vec::new();
        let mut current_vis = vnode.parent_vis_idx;

        while let Some(ancestor_vis) = current_vis {
            let ancestor_vnode = visible_nodes.get(ancestor_vis.0)?;
            let ancestor = tree.node(ancestor_vnode.id)?;

            if let Some(grand_vis) = ancestor_vnode.parent_vis_idx {
                let has_more_siblings = !ancestor_vnode.is_last_non_group_child;
                let grand_node_id = visible_nodes[grand_vis.0].id;
                let edge_style = tree
                    .node(grand_node_id)
                    .and_then(|parent| parent.as_group().map(|group| group.kind.style()));
                lineage.push(LineageSegment {
                    has_more_siblings,
                    edge_style,
                    is_group: ancestor.is_group(),
                });
            }
            current_vis = ancestor_vnode.parent_vis_idx;
        }

        lineage.reverse();
        Some(Lineage {
            segments: lineage,
            is_last,
            is_selected: selected_vis_idx == Some(vis_idx),
        })
    }
}
