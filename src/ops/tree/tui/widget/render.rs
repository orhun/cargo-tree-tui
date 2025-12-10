use ratatui::{
    buffer::Buffer,
    text::Line,
    widgets::{Scrollbar, StatefulWidget},
};

use crate::core::{Dependency, DependencyTree, NodeId};

use super::{
    lineage::Lineage,
    state::{TreeWidgetState, VisibleNode},
    style::TreeWidgetStyle,
    viewport::Viewport,
};

pub fn render_lines<'a>(
    visible_nodes: &[VisibleNode],
    state: &'a TreeWidgetState,
    tree: &'a DependencyTree,
    style: &'a TreeWidgetStyle,
    root_label: Option<&'a str>,
    viewport: Viewport,
    root_line_offset: usize,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(viewport.height);
    if viewport.height == 0 {
        return lines;
    }

    let selected = state.selected;
    let root_label_present = root_label.is_some();

    if viewport.offset == 0 {
        if let Some(label) = root_label {
            lines.push(Line::from(label.to_string()));
        }

        let available = viewport.height.saturating_sub(lines.len());
        let max_nodes = available.min(visible_nodes.len());

        for node in visible_nodes.iter().take(max_nodes) {
            if let Some(line) =
                render_visible_node(tree, node, state, selected, root_label_present, style)
            {
                lines.push(line);
            }
        }
    } else {
        lines.push(breadcrumb_line(tree, state, style));

        let total_lines = visible_nodes.len() + root_line_offset;

        let start_flat = viewport.offset + 1;
        let end_flat = (viewport.offset + viewport.height).min(total_lines);
        for flat_id in start_flat..end_flat {
            let node_id = flat_id.saturating_sub(root_line_offset);
            if let Some(node) = visible_nodes.get(node_id)
                && let Some(line) =
                    render_visible_node(tree, node, state, selected, root_label_present, style)
            {
                lines.push(line);
            }
        }
    }

    lines
}

/// Renders a single visible dependency line.
fn render_visible_node<'a>(
    tree: &'a DependencyTree,
    node: &VisibleNode,
    state: &'a TreeWidgetState,
    selected: Option<NodeId>,
    root_label_present: bool,
    style: &'a TreeWidgetStyle,
) -> Option<Line<'a>> {
    let node_data = tree.node(node.id)?;
    let lineage = Lineage::build(tree, node.id, selected)?;
    let has_children = !node_data.children.is_empty();
    let is_open = state.open.contains(&node.id);

    let is_root = node_data.parent.is_none();
    let allow_root_connector = if lineage.depth() <= 1 {
        root_label_present
    } else {
        true
    };
    let show_connector = !is_root && (allow_root_connector || lineage.has_segments());

    let indent = lineage.indent(style);
    let rendered = RenderedNode::build(
        node_data,
        &lineage,
        &indent,
        show_connector,
        has_children,
        is_open,
        style,
    );
    Some(rendered.line)
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

/// Generates a breadcrumb line for the selected node.
pub fn breadcrumb_line<'a>(
    tree: &'a DependencyTree,
    state: &'a TreeWidgetState,
    style: &'a TreeWidgetStyle,
) -> Line<'a> {
    let mut names = Vec::new();
    let mut current = state.selected;

    while let Some(id) = current {
        if let Some(node) = tree.node(id) {
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
            style.highlight_style
        } else {
            style.style
        };

        spans.push(ratatui::text::Span::styled(name.clone(), name_style));

        if !is_last {
            spans.push(ratatui::text::Span::styled(" → ", style.style));
        }
    }

    Line::from(spans)
}

/// A rendered dependency node line.
#[derive(Debug)]
pub struct RenderedNode<'a> {
    pub line: Line<'a>,
}

impl<'a> RenderedNode<'a> {
    /// Builds a rendered node line.
    pub fn build(
        node: &Dependency,
        lineage: &Lineage,
        indent: &str,
        show_connector: bool,
        has_children: bool,
        is_open: bool,
        style: &TreeWidgetStyle,
    ) -> Self {
        let mut spans = Vec::new();

        let toggle = if has_children {
            if is_open {
                format!("{} ", style.node_open_symbol)
            } else {
                format!("{} ", style.node_closed_symbol)
            }
        } else {
            format!("{} ", style.node_symbol)
        };

        if show_connector {
            let connector = if lineage.is_last {
                style.last_branch_symbol
            } else {
                style.branch_symbol
            };
            spans.push(ratatui::text::Span::styled(
                format!("{indent}{connector}{toggle}"),
                style.style,
            ));
        }

        let name_style = if lineage.is_selected {
            style.highlight_style
        } else {
            style.name_style
        };

        spans.push(ratatui::text::Span::styled(node.name.clone(), name_style));
        spans.push(ratatui::text::Span::styled(
            format!(" v{}", node.version),
            style.version_style,
        ));

        if let Some(extra) = Self::format_suffixes(node, style) {
            spans.extend(extra);
        }

        Self {
            line: Line::from(spans),
        }
    }

    /// Formats suffixes for a dependency node.
    fn format_suffixes(
        node: &Dependency,
        style: &TreeWidgetStyle,
    ) -> Option<Vec<ratatui::text::Span<'a>>> {
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
        spans.push(ratatui::text::Span::styled(" (", style.style));

        for (idx, suffix) in suffixes.iter().enumerate() {
            if idx > 0 {
                spans.push(ratatui::text::Span::styled(", ", style.style));
            }
            spans.push(ratatui::text::Span::styled(
                suffix.clone(),
                style.suffix_style,
            ));
        }

        spans.push(ratatui::text::Span::styled(")", style.style));

        Some(spans)
    }
}
