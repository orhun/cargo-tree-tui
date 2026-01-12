use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Block,
};

use crate::core::{Dependency, DependencyNode, DependencyTree};

use super::{
    lineage::Lineage,
    state::{TreeWidgetState, VisibleNode},
    style::TreeWidgetStyle,
    viewport::Viewport,
};

#[derive(Default)]
pub struct RenderOutput<'a> {
    pub lines: Vec<Line<'a>>,
    pub total_lines: usize,
    pub viewport: Viewport,
    pub render_breadcrumb: bool,
}

/// Context for rendering the dependency tree.
///
/// # Note for lifetimes
///
/// - `'a` is the lifetime of the dependency tree and style references.
/// - `'s` is the lifetime of the mutable state reference.
///
/// The reason why we keep them separate is to let the mutable borrow of the
/// state end before we later borrow the state immutably (e.g. for breadcrumb
/// rendering) while still holding references to the tree and style.
pub struct RenderContext<'a, 's> {
    pub tree: &'a DependencyTree,
    pub state: &'s mut TreeWidgetState,
    pub style: &'a TreeWidgetStyle,
    pub block: Option<&'a Block<'a>>,
}

impl<'a, 's> RenderContext<'a, 's> {
    pub fn new(
        tree: &'a DependencyTree,
        state: &'s mut TreeWidgetState,
        style: &'a TreeWidgetStyle,
        block: Option<&'a Block<'a>>,
    ) -> Self {
        Self {
            tree,
            state,
            style,
            block,
        }
    }

    pub fn render(&mut self, area: Rect) -> RenderOutput<'a> {
        let Some(selected_idx) = self.state.selected_position(self.tree) else {
            return RenderOutput::default();
        };

        let visible_nodes = self.state.visible_nodes(self.tree).to_vec();
        let selected_line = selected_idx + 1;
        let total_lines = visible_nodes.len();

        let viewport = Viewport::new(area, self.block, selected_line, total_lines);
        self.state.update_viewport(viewport);

        let render_breadcrumb = viewport.offset > 0;
        let content_height = if render_breadcrumb {
            viewport.height.saturating_sub(1)
        } else {
            viewport.height
        };
        let mut lines = Vec::with_capacity(content_height);

        let start_flat = if render_breadcrumb {
            viewport.offset + 1
        } else {
            viewport.offset
        };
        let end_flat = (start_flat + content_height).min(total_lines);
        for flat_id in start_flat..end_flat {
            if let Some(node) = visible_nodes.get(flat_id)
                && let Some(line) = self.render_node(node)
            {
                lines.push(line);
            }
        }

        RenderOutput {
            lines,
            total_lines,
            viewport,
            render_breadcrumb,
        }
    }

    pub fn render_node(&self, node: &VisibleNode) -> Option<Line<'a>> {
        let node_data = self.tree.node(node.id)?;
        let lineage = Lineage::build(self.tree, node.id, self.state.selected)?;
        let has_children = !node_data.children().is_empty();
        let is_open = self.state.open.contains(&node.id);
        let is_group = node_data.is_group();

        let is_root = node_data.parent().is_none();
        let show_connector = !is_root;

        let mut spans = Vec::new();

        let toggle = if has_children {
            if is_open {
                format!("{} ", self.style.node_open_symbol)
            } else {
                format!("{} ", self.style.node_closed_symbol)
            }
        } else {
            format!("{} ", self.style.node_symbol)
        };

        if show_connector {
            for segment in &lineage.segments {
                if segment.is_group {
                    continue;
                }
                let symbol = if segment.has_more_siblings {
                    self.style.continuation_symbol
                } else {
                    self.style.empty_symbol
                };
                let segment_style = segment.edge_style.unwrap_or(self.style.style);
                spans.push(Span::styled(symbol, segment_style));
            }

            if !is_group {
                let connector = if lineage.is_last {
                    self.style.last_branch_symbol
                } else {
                    self.style.branch_symbol
                };
                let parent_group_style = node_data
                    .parent()
                    .and_then(|parent_id| self.tree.node(parent_id))
                    .and_then(|parent| parent.as_group().map(|group| group.kind.style()));
                let connector_style = parent_group_style.unwrap_or(self.style.style);
                spans.push(Span::styled(connector, connector_style));
                spans.push(Span::styled(toggle, self.style.style));
            }
        }

        let name_style = if lineage.is_selected {
            self.style.highlight_style
        } else {
            self.style.name_style
        };

        match node_data {
            DependencyNode::Crate(dependency) => {
                spans.push(Span::styled(dependency.name.clone(), name_style));
                if !dependency.version.is_empty() {
                    spans.push(Span::styled(
                        format!(" v{}", dependency.version),
                        self.style.version_style,
                    ));
                }

                if let Some(extra) = format_suffixes(dependency, self.style) {
                    spans.extend(extra);
                }
            }
            DependencyNode::Group(group) => {
                let group_style = if lineage.is_selected {
                    self.style.highlight_style
                } else {
                    group.kind.style()
                };
                spans.push(Span::styled(group.label().to_string(), group_style));
            }
        }

        Some(Line::from(spans))
    }
}

/// Formats suffixes for a dependency node.
fn format_suffixes<'a>(node: &Dependency, style: &TreeWidgetStyle) -> Option<Vec<Span<'a>>> {
    let mut suffixes = Vec::new();

    if let Some(path) = &node.manifest_dir {
        suffixes.push(path.clone());
    }

    if node.is_proc_macro {
        suffixes.push("proc-macro".to_string());
    }

    if suffixes.is_empty() {
        return None;
    }

    let mut spans = Vec::new();
    spans.push(Span::styled(" (", style.style));

    for (idx, suffix) in suffixes.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(", ", style.style));
        }
        spans.push(Span::styled(suffix.clone(), style.suffix_style));
    }

    spans.push(Span::styled(")", style.style));

    Some(spans)
}
