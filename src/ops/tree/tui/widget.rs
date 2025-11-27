use ratatui::{
    buffer::Buffer,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarState, StatefulWidget, Widget},
};

use crate::core::{Dependency, DependencyTree, NodeId};

use super::widget_state::{TreeWidgetState, VisibleNode};

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
            let center_line = height.div_ceil(2);
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
        let visible_nodes = state.visible_nodes(self.tree).to_vec();
        if visible_nodes.is_empty() {
            return;
        }

        let root_line_offset = usize::from(self.root_label.is_some());
        let selected_id = match state.selected {
            Some(selected_id) => visible_nodes
                .iter()
                .position(|node| node.id == selected_id)
                .unwrap_or_else(|| {
                    state.selected = Some(visible_nodes[0].id);
                    0
                }),
            None => {
                state.selected = Some(visible_nodes[0].id);
                0
            }
        };
        let selected_line = selected_id + root_line_offset + 1;
        let total_lines = visible_nodes.len() + root_line_offset;

        let viewport = Viewport::new(area, self.block.as_ref(), selected_line, total_lines);
        state.update_viewport(viewport);

        let lines = Self::render_lines(
            &visible_nodes,
            state,
            self.tree,
            &self.style,
            self.root_label,
            viewport,
            root_line_offset,
        );

        let mut paragraph = Paragraph::new(lines).style(self.style.style);
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
    fn render_lines(
        visible_nodes: &[VisibleNode],
        state: &TreeWidgetState,
        tree: &DependencyTree,
        style: &TreeWidgetStyle,
        root_label: Option<&str>,
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
            // Top of the tree: optional root label + first visible nodes.
            if let Some(label) = root_label {
                lines.push(Line::from(label.to_string()));
            }

            let available = viewport.height.saturating_sub(lines.len());
            let max_nodes = available.min(visible_nodes.len());

            for node in visible_nodes.iter().take(max_nodes) {
                if let Some(line) =
                    Self::render_visible_node(tree, node, selected, root_label_present, style)
                {
                    lines.push(line);
                }
            }
        } else {
            // Scrolled: show breadcrumb on top, then the slice of visible nodes.
            lines.push(Self::breadcrumb_line(tree, state, style));

            let total_lines = visible_nodes.len() + root_line_offset;

            let start_flat = viewport.offset + 1;
            let end_flat = (viewport.offset + viewport.height).min(total_lines);

            for flat_id in start_flat..end_flat {
                let node_id = flat_id.saturating_sub(root_line_offset);
                if let Some(node) = visible_nodes.get(node_id)
                    && let Some(line) =
                        Self::render_visible_node(tree, node, selected, root_label_present, style)
                {
                    lines.push(line);
                }
            }
        }

        lines
    }

    /// Renders a single visible dependency line.
    fn render_visible_node(
        tree: &DependencyTree,
        node: &VisibleNode,
        selected: Option<NodeId>,
        root_label_present: bool,
        style: &TreeWidgetStyle,
    ) -> Option<Line<'a>> {
        let node_data = tree.node(node.id)?;
        let (lineage, is_last) = Self::build_lineage(tree, node.id)?;

        let is_root = node_data.parent.is_none();
        let allow_root_connector = if lineage.len() <= 1 {
            root_label_present
        } else {
            true
        };
        let show_connector = !is_root && (allow_root_connector || !lineage.is_empty());

        let indent = Self::make_indent(&lineage, style);
        let rendered = RenderedNode::build(
            node_data,
            selected == Some(node.id),
            is_last,
            &indent,
            show_connector,
            style,
        );
        Some(rendered.line)
    }

    /// Builds lineage information:
    ///
    /// - `Vec<bool>`: for each ancestor from root → parent, whether there are more siblings (`true` = draw continuation).
    /// - `bool`: whether the current node is the last child of its parent.
    fn build_lineage(tree: &DependencyTree, node_id: NodeId) -> Option<(Vec<bool>, bool)> {
        let node = tree.node(node_id)?;

        // Is this node the last among its siblings?
        let is_last = match node.parent {
            Some(parent_id) => {
                let parent = tree.node(parent_id)?;
                parent.children.last().copied() == Some(node_id)
            }
            None => true,
        };

        // For each ancestor, we record whether it has further siblings after it.
        let mut lineage = Vec::new();
        let mut current = node.parent;

        // Traverse up to the root to build the lineage.
        while let Some(ancestor_id) = current {
            let ancestor = tree.node(ancestor_id)?;
            let has_more_siblings = if let Some(grand_id) = ancestor.parent {
                let grand = tree.node(grand_id)?;
                grand.children.last().copied() != Some(ancestor_id)
            } else {
                false
            };

            lineage.push(has_more_siblings);
            current = ancestor.parent;
        }

        lineage.reverse();
        Some((lineage, is_last))
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

    /// Generates a breadcrumb line for the selected node.
    fn breadcrumb_line(
        tree: &DependencyTree,
        state: &TreeWidgetState,
        style: &TreeWidgetStyle,
    ) -> Line<'a> {
        let mut names = Vec::new();
        let mut current = state.selected;

        // Populate the names from the selected node up to the root.
        while let Some(id) = current {
            if let Some(node) = tree.node(id) {
                names.push(node.name.clone());
                current = node.parent;
            } else {
                break;
            }
        }
        names.reverse();

        // Create spans for the breadcrumb line.
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
