use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Paragraph, Scrollbar, StatefulWidget, Widget},
};

use crate::core::DependencyTree;

use self::{
    render::{render_lines, render_scrollbar},
    viewport::Viewport,
};

pub use self::{render::RenderedNode, state::TreeWidgetState, style::TreeWidgetStyle};

mod lineage;
mod render;
pub mod state;
mod style;
mod viewport;

/// A tree widget for displaying hierarchical dependencies.
#[derive(Debug)]
pub struct TreeWidget<'a> {
    tree: &'a DependencyTree,
    root_label: Option<&'a str>,
    block: Option<Block<'a>>,
    scrollbar: Option<Scrollbar<'a>>,
    style: TreeWidgetStyle,
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

        let lines = render_lines(
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
            render_scrollbar(scrollbar, &viewport, total_lines, buf);
        }
    }
}
