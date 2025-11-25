use ratatui::{
    buffer::Buffer,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarState, StatefulWidget, Widget},
};

use crate::core::{Dependency, DependencyTree, NodeId};

use super::widget_state::TreeWidgetState;

/// Visual configuration for [`TreeWidget`].
#[derive(Debug)]
pub struct TreeWidgetStyle {
    highlight_style: Style,
    style: Style,
    name_style: Style,
    version_style: Style,
    suffix_style: Style,
    branch_symbol: &'static str,
    last_branch_symbol: &'static str,
    continuation_symbol: &'static str,
    empty_symbol: &'static str,
}

/// TODO: Use styles defined in <https://docs.rs/clap-cargo/latest/clap_cargo/style/index.html>
/// This requires using the `anstyle` feature of Ratatui, which is not released yet.
/// See <https://github.com/orhun/cargo-tree-tui/issues/9>
impl Default for TreeWidgetStyle {
    fn default() -> Self {
        Self {
            highlight_style: Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
            style: Style::default(),
            name_style: Style::default(),
            version_style: Style::default().fg(Color::Green),
            suffix_style: Style::default().fg(Color::Cyan),
            branch_symbol: "├── ",
            last_branch_symbol: "└── ",
            continuation_symbol: "│   ",
            empty_symbol: "    ",
        }
    }
}

/// A tree widget for displaying hierarchical dependencies.
#[derive(Debug)]
pub struct TreeWidget<'a> {
    tree: &'a DependencyTree,
    root_label: Option<&'a str>,
    block: Option<Block<'a>>,
    scrollbar: Option<Scrollbar<'a>>,
    style: TreeWidgetStyle,
}

/// Viewport information for rendering the tree widget.
#[derive(Debug, Copy, Clone, Default)]
pub(crate) struct Viewport {
    /// The full area allocated for the widget.
    pub area: Rect,
    /// The inner area after accounting for borders and padding.
    pub inner: Rect,
    /// Height of the inner area.
    pub height: usize,
    /// Current scroll offset.
    pub offset: usize,
    /// Maximum scroll offset.
    pub max_offset: usize,
}

impl Viewport {
    fn new(
        area: Rect,
        block: Option<&Block<'_>>,
        selected_line: usize,
        total_lines: usize,
    ) -> Self {
        let inner = block.map(|b| b.inner(area)).unwrap_or(area);
        let height = inner.height as usize;

        let mut offset = if height == 0 {
            0
        } else {
            let center_line = if height == 0 { 0 } else { height.div_ceil(2) };
            selected_line.saturating_sub(center_line)
        };

        let max_offset = if height == 0 {
            0
        } else {
            total_lines.saturating_sub(height)
        };
        offset = offset.min(max_offset);

        Self {
            area,
            inner,
            height,
            offset,
            max_offset,
        }
    }
}

impl<'a> TreeWidget<'a> {
    pub fn new(tree: &'a DependencyTree) -> Self {
        Self {
            tree,
            root_label: None,
            block: None,
            scrollbar: None,
            style: TreeWidgetStyle::default(),
        }
    }

    pub fn root_label(mut self, label: &'a str) -> Self {
        self.root_label = Some(label);
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn scrollbar(mut self, scrollbar: Scrollbar<'a>) -> Self {
        self.scrollbar = Some(scrollbar);
        self
    }
}

impl StatefulWidget for TreeWidget<'_> {
    type State = TreeWidgetState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let root_line_offset = usize::from(self.root_label.is_some());
        let position = state.selected_position(self.tree).unwrap_or_default();
        let position_line = position + root_line_offset + 1;
        let total_lines = state.visible_nodes(self.tree).len() + root_line_offset;

        let viewport = Viewport::new(area, self.block.as_ref(), position_line, total_lines);
        state.update_viewport(viewport);

        let mut lines: Vec<Line> = Vec::new();
        let mut lineage = Vec::new();

        if let Some(label) = self.root_label {
            lines.push(Line::from(label.to_string()));
        }

        // NOTE: Instead of processing the entire tree, we could optimize by
        // only rendering visible nodes.
        Self::render_children(
            &mut lines,
            self.tree,
            self.tree.roots(),
            state,
            &mut lineage,
            &self.style,
            self.root_label.is_some(),
        );

        let mut visible_lines: Vec<Line> = lines
            .into_iter()
            .skip(viewport.offset)
            .take(viewport.height)
            .collect();

        if viewport.offset > 0 {
            let breadcrumb = Self::breadcrumb_line(self.tree, state, &self.style);
            visible_lines.remove(0);
            visible_lines.insert(0, breadcrumb);
        }

        let mut paragraph = Paragraph::new(visible_lines).style(self.style.style);
        if let Some(block) = self.block {
            paragraph = paragraph.block(block);
        }

        paragraph.render(viewport.area, buf);

        if let Some(scrollbar) = self.scrollbar {
            Self::render_scrollbar(scrollbar, &viewport, total_lines, buf);
        }
    }
}

impl<'a> TreeWidget<'a> {
    /// Recursively renders dependencies.
    fn render_children(
        lines: &mut Vec<Line>,
        tree: &DependencyTree,
        nodes: &[NodeId],
        state: &TreeWidgetState,
        lineage: &mut Vec<bool>,
        style: &TreeWidgetStyle,
        allow_root_connector: bool,
    ) {
        for (index, node_id) in nodes.iter().enumerate() {
            let is_last = index + 1 == nodes.len();
            Self::render_node(
                lines,
                tree,
                *node_id,
                state,
                lineage,
                style,
                allow_root_connector,
                is_last,
            );
        }
    }

    /// Renders a single dependency node.
    #[allow(clippy::too_many_arguments)]
    fn render_node(
        lines: &mut Vec<Line>,
        tree: &DependencyTree,
        node_id: NodeId,
        state: &TreeWidgetState,
        lineage: &mut Vec<bool>,
        style: &TreeWidgetStyle,
        allow_root_connector: bool,
        is_last: bool,
    ) {
        let Some(node) = tree.node(node_id) else {
            return;
        };

        let is_open = state.open.contains(&node_id);
        let is_selected = state.selected == Some(node_id);
        let is_root = node.parent.is_none();

        let show_connector = !is_root && (allow_root_connector || !lineage.is_empty());
        let indent = Self::make_indent(lineage, style);
        let rendered =
            RenderedNode::build(node, is_selected, is_last, &indent, show_connector, style);
        lines.push(rendered.line);

        if is_open && !node.children.is_empty() {
            if is_root {
                lineage.push(false);
            } else {
                lineage.push(!is_last);
            }
            Self::render_children(lines, tree, &node.children, state, lineage, style, true);
            lineage.pop();
        }
    }

    /// Renders the scrollbar if applicable.
    fn render_scrollbar(
        scrollbar: Scrollbar<'a>,
        viewport: &Viewport,
        total_lines: usize,
        buf: &mut Buffer,
    ) {
        if viewport.height == 0 || viewport.max_offset == 0 {
            return;
        }

        let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(viewport.height))
            .position(viewport.offset)
            .viewport_content_length(viewport.height);
        scrollbar.render(
            viewport.inner.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            buf,
            &mut scrollbar_state,
        );
    }

    /// Generates indentation based on lineage.
    fn make_indent(lineage: &[bool], style: &TreeWidgetStyle) -> String {
        lineage
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

    /// Generates a breadcrumb line for the selected node.
    fn breadcrumb_line(
        tree: &DependencyTree,
        state: &TreeWidgetState,
        style: &TreeWidgetStyle,
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
            spans.push(Span::styled(name.clone(), name_style));
            if !is_last {
                spans.push(Span::styled(" → ", style.style));
            }
        }

        Line::from(spans)
    }
}

/// A rendered dependency node line.
struct RenderedNode<'a> {
    line: Line<'a>,
}

impl<'a> RenderedNode<'a> {
    /// Builds a rendered node line.
    fn build(
        node: &Dependency,
        is_selected: bool,
        is_last: bool,
        indent: &str,
        show_connector: bool,
        style: &TreeWidgetStyle,
    ) -> Self {
        let mut spans = Vec::new();

        if show_connector {
            let connector = if is_last {
                style.last_branch_symbol
            } else {
                style.branch_symbol
            };
            spans.push(Span::styled(format!("{indent}{connector}"), style.style));
        }

        let name_style = if is_selected {
            style.highlight_style
        } else {
            style.name_style
        };

        spans.push(Span::styled(node.name.clone(), name_style));
        spans.push(Span::styled(
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
    fn format_suffixes(node: &Dependency, style: &TreeWidgetStyle) -> Option<Vec<Span<'a>>> {
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
}
