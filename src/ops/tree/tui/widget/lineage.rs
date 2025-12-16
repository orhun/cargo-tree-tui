use ratatui::text::Span;

use crate::core::{DependencyTree, DependencyType, NodeId};

use super::style::TreeWidgetStyle;

/// Segment information for each ancestor.
#[derive(Debug)]
pub struct LineageSegment {
    /// Whether there are more siblings at this ancestor level (`true` = draw continuation).
    pub has_more_siblings: bool,
    /// Dependency type of the ancestor, used for styling connectors.
    pub type_: Option<DependencyType>,
}

/// Lineage information for a dependency node.
#[derive(Debug)]
pub struct Lineage {
    /// For each ancestor from root → parent, connector metadata.
    pub segments: Vec<LineageSegment>,
    /// Whether the current node is the last child of its parent.
    pub is_last: bool,
    /// Whether this node is the currently selected one.
    pub is_selected: bool,
}

impl Lineage {
    /// Builds lineage information for a node.
    pub fn build(tree: &DependencyTree, node_id: NodeId, selected: Option<NodeId>) -> Option<Self> {
        let node = tree.node(node_id)?;

        let is_last = match node.parent {
            Some(parent_id) => {
                let parent = tree.node(parent_id)?;
                parent.children.last().copied() == Some(node_id)
            }
            None => true,
        };

        let mut lineage = Vec::new();
        let mut current = node.parent;

        while let Some(ancestor_id) = current {
            let ancestor = tree.node(ancestor_id)?;
            let has_more_siblings = if let Some(grand_id) = ancestor.parent {
                let grand = tree.node(grand_id)?;
                grand.children.last().copied() != Some(ancestor_id)
            } else {
                false
            };

            lineage.push(LineageSegment {
                has_more_siblings,
                type_: ancestor.type_,
            });
            current = ancestor.parent;
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

    pub fn indent<'a>(&self, style: &TreeWidgetStyle) -> Vec<Span<'a>> {
        self.segments
            .iter()
            .map(|segment| {
                let symbol = if segment.has_more_siblings {
                    style.continuation_symbol
                } else {
                    style.empty_symbol
                };
                let span_style = segment
                    .type_
                    .filter(|ty| ty != &DependencyType::Normal)
                    .map(|ty| ty.style())
                    .unwrap_or(style.style);
                Span::styled(symbol.to_string(), span_style)
            })
            .collect()
    }
}
