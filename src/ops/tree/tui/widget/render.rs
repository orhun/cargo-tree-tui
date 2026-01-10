use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Scrollbar, StatefulWidget},
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
}

pub struct RenderContext<'a> {
    pub tree: &'a DependencyTree,
    pub state: &'a mut TreeWidgetState,
    pub style: &'a TreeWidgetStyle,
    pub block: Option<&'a Block<'a>>,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        tree: &'a DependencyTree,
        state: &'a mut TreeWidgetState,
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

        let mut lines = Vec::with_capacity(viewport.height);

        if viewport.height == 0 {
            return RenderOutput {
                lines,
                total_lines,
                viewport: Viewport::default(),
            };
        }

        if viewport.offset == 0 {
            let max_nodes = viewport.height.min(visible_nodes.len());

            for node in visible_nodes.iter().take(max_nodes) {
                if let Some(line) = self.render_node(node) {
                    lines.push(line);
                }
            }
        } else {
            lines.push(self.breadcrumb());

            let start_flat = viewport.offset + 1;
            let end_flat = (viewport.offset + viewport.height).min(total_lines);
            for flat_id in start_flat..end_flat {
                let node_id = flat_id;
                if let Some(node) = visible_nodes.get(node_id)
                    && let Some(line) = self.render_node(node)
                {
                    lines.push(line);
                }
            }
        }

        RenderOutput {
            lines,
            total_lines,
            viewport,
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

    pub fn breadcrumb(&self) -> Line<'a> {
        let mut crumbs = Vec::new();
        let mut current = self.state.selected;

        while let Some(id) = current {
            if let Some(node) = self.tree.node(id) {
                let group_style = node.as_group().map(|group| group.kind.style());
                crumbs.push((
                    node.display_name().to_string(),
                    group_style,
                    node.is_group(),
                ));
                current = node.parent();
            } else {
                break;
            }
        }
        crumbs.reverse();

        let mut spans = Vec::new();
        for (i, (name, group_style, is_group)) in crumbs.iter().enumerate() {
            let is_last = i + 1 == crumbs.len();
            let name_style = if *is_group {
                group_style.unwrap_or(self.style.style)
            } else {
                self.style.style
            };

            spans.push(Span::styled(name.clone(), name_style));

            if !is_last {
                let arrow_style = if *is_group {
                    group_style.unwrap_or(self.style.style)
                } else {
                    self.style.style
                };
                spans.push(Span::styled(" → ", arrow_style));
            }
        }

        Line::from(spans)
    }
}

/// Renders the scrollbar if applicable.
pub fn render_scrollbar(
    scrollbar: Scrollbar<'_>,
    viewport: &Viewport,
    total_lines: usize,
    buf: &mut Buffer,
) {
    if viewport.height == 0 || viewport.max_offset == 0 {
        return;
    }

    let mut scrollbar_state =
        ratatui::widgets::ScrollbarState::new(total_lines.saturating_sub(viewport.height))
            .position(viewport.offset)
            .viewport_content_length(viewport.height);

    scrollbar.render(viewport.inner, buf, &mut scrollbar_state);
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
