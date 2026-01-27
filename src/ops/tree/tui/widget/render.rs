use ratatui::{
    layout::Rect,
    style::{Modifier, Stylize},
    text::{Line, Span},
    widgets::Block,
};

use crate::core::{Dependency, DependencyNode, DependencyTree, NodeId};

use super::{
    lineage::Lineage,
    state::{TreeWidgetState, VisibleNode},
    style::TreeWidgetStyle,
    viewport::Viewport,
};

#[derive(Default)]
pub struct RenderOutput<'a> {
    pub lines: Vec<Line<'a>>,
    pub context_lines: Vec<Line<'a>>,
    pub total_lines: usize,
    pub viewport: Viewport,
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

        let mut viewport = Viewport::new(area, self.block).center_on(selected_line, total_lines, 1);
        self.state.update_viewport(viewport);

        let context_lines =
            self.render_context_lines(&visible_nodes, viewport.offset.min(viewport.max_offset));

        let content_height = viewport.height.saturating_sub(context_lines.len());

        viewport.clamp_offset(total_lines, context_lines.len());
        self.state.update_viewport(viewport);

        let start_flat = viewport.offset;
        let mut lines = Vec::with_capacity(content_height);
        let end_flat = (start_flat + content_height).min(total_lines);
        for flat_id in start_flat..end_flat {
            let is_last_rendered = flat_id + 1 == end_flat;
            if let Some(node) = visible_nodes.get(flat_id)
                && let Some(line) = self.render_node(node.id, is_last_rendered, false)
            {
                lines.push(line);
            }
        }

        RenderOutput {
            lines,
            context_lines,
            total_lines,
            viewport,
        }
    }

    pub fn render_node(
        &self,
        node_id: NodeId,
        show_more_below: bool,
        context_lines: bool,
    ) -> Option<Line<'a>> {
        let node_data = self.tree.node(node_id)?;
        let lineage = Lineage::build(self.tree, node_id, self.state.selected)?;
        let has_children = !node_data.children().is_empty();
        let is_open = self.state.open.contains(&node_id);
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

                let base_style = if context_lines {
                    Modifier::DIM.into()
                } else {
                    segment.edge_style.unwrap_or(self.style.style)
                };

                let (symbol, style) = match (segment.has_more_siblings, show_more_below) {
                    (true, true) => (
                        self.style.more_below_symbol,
                        base_style.add_modifier(Modifier::DIM),
                    ),
                    (true, false) => (self.style.continuation_symbol, base_style),
                    (false, _) => (self.style.empty_symbol, base_style),
                };

                spans.push(Span::styled(symbol, style));
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

    fn render_context_lines(&self, visible_nodes: &[VisibleNode], offset: usize) -> Vec<Line<'a>> {
        if offset == 0 {
            return Vec::new();
        }

        let Some(first_visible) = visible_nodes.get(offset) else {
            return Vec::new();
        };

        // Collect ancestors bottom → top
        let mut ancestors = Vec::new();
        let mut current = self.tree.node(first_visible.id).and_then(|n| n.parent());

        while let Some(id) = current {
            ancestors.push(id);
            current = self.tree.node(id).and_then(|n| n.parent());
        }

        // Render top → bottom, limited
        ancestors
            .into_iter()
            .rev()
            .filter_map(|id| self.render_node(id, false, true))
            .map(|line| line.add_modifier(Modifier::DIM))
            .collect()
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
