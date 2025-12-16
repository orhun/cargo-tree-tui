use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Scrollbar, StatefulWidget},
};

use crate::core::{Dependency, DependencyTree, DependencyType};

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
    pub root_label: Option<&'a str>,
    pub block: Option<&'a Block<'a>>,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        tree: &'a DependencyTree,
        state: &'a mut TreeWidgetState,
        style: &'a TreeWidgetStyle,
        root_label: Option<&'a str>,
        block: Option<&'a Block<'a>>,
    ) -> Self {
        Self {
            tree,
            state,
            style,
            root_label,
            block,
        }
    }

    pub fn render(&mut self, area: Rect) -> RenderOutput<'a> {
        let Some(selected_idx) = self.state.selected_position(self.tree) else {
            return RenderOutput::default();
        };

        let visible_nodes = self.state.visible_nodes(self.tree).to_vec();
        let root_line_offset = usize::from(self.root_label.is_some());

        let selected_line = selected_idx + root_line_offset + 1;
        let total_lines = visible_nodes.len() + root_line_offset;

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
            if let Some(label) = self.root_label {
                lines.push(Line::from(label.to_string()));
            }

            let available = viewport.height.saturating_sub(lines.len());
            let max_nodes = available.min(visible_nodes.len());

            for node in visible_nodes.iter().take(max_nodes) {
                if let Some(line) = self.render_node(node) {
                    lines.push(line);
                }
            }
        } else {
            lines.push(self.breadcrumb());

            let total_lines = visible_nodes.len() + root_line_offset;

            let start_flat = viewport.offset + 1;
            let end_flat = (viewport.offset + viewport.height).min(total_lines);
            for flat_id in start_flat..end_flat {
                let node_id = flat_id.saturating_sub(root_line_offset);
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
        let has_children = !node_data.children.is_empty();
        let is_open = self.state.open.contains(&node.id);

        let is_root = node_data.parent.is_none();
        let is_group = node_data.is_group;
        let allow_root_connector = if lineage.depth() <= 1 {
            self.root_label.is_some()
        } else {
            true
        };
        let show_connector = !is_root && (allow_root_connector || lineage.has_segments());

        let mut spans = Vec::new();
        let indent = lineage.indent(self.style);

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
            spans.extend(indent);
            let connector = if lineage.is_last {
                self.style.last_branch_symbol
            } else {
                self.style.branch_symbol
            };
            if node_data.type_ != Some(DependencyType::Normal) && !is_group {
                spans.push(Span::styled(
                    connector.to_string(),
                    node_data
                        .type_
                        .map(|v| v.style())
                        .unwrap_or(self.style.style),
                ));
                spans.push(Span::styled(toggle, self.style.style));
            } else {
                spans.push(Span::styled(
                    format!("{connector}{toggle}"),
                    self.style.style,
                ));
            };
        }

        let name_style = if lineage.is_selected {
            self.style.highlight_style
        } else if is_group {
            node_data
                .type_
                .map(|v| v.style())
                .unwrap_or(self.style.style)
        } else {
            self.style.name_style
        };

        spans.push(Span::styled(node_data.name.clone(), name_style));
        if !is_group {
            spans.push(Span::styled(
                format!(" v{}", node_data.version),
                self.style.version_style,
            ));
        }

        if let Some(extra) = format_suffixes(node_data, self.style) {
            spans.extend(extra);
        }

        Some(Line::from(spans))
    }

    pub fn breadcrumb(&self) -> Line<'a> {
        let mut names = Vec::new();
        let mut current = self.state.selected;

        while let Some(id) = current {
            if let Some(node) = self.tree.node(id) {
                names.push(node.name.clone());
                current = node.parent;
            } else {
                break;
            }
        }
        names.reverse();

        let mut spans = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let is_last = i + 1 == names.len();
            let name_style = if is_last {
                self.style.highlight_style
            } else {
                self.style.style
            };

            spans.push(Span::styled(name.clone(), name_style));

            if !is_last {
                spans.push(Span::styled(" → ", self.style.style));
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
