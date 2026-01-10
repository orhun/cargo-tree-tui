use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Paragraph, Scrollbar, StatefulWidget, Widget},
};

use crate::core::DependencyTree;

use self::render::{RenderContext, render_scrollbar};

pub use self::{render::RenderOutput, state::TreeWidgetState, style::TreeWidgetStyle};

mod lineage;
pub mod render;
pub mod state;
mod style;
mod viewport;

/// A tree widget for displaying hierarchical dependencies.
#[derive(Debug)]
pub struct TreeWidget<'a> {
    tree: &'a DependencyTree,
    block: Option<Block<'a>>,
    scrollbar: Option<Scrollbar<'a>>,
    style: TreeWidgetStyle,
}

impl<'a> TreeWidget<'a> {
    pub fn new(tree: &'a DependencyTree) -> Self {
        Self {
            tree,
            block: None,
            scrollbar: None,
            style: TreeWidgetStyle::default(),
        }
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
        if state.visible_nodes(self.tree).is_empty() {
            return;
        }

        let block_ref = self.block.as_ref();
        let mut ctx = RenderContext::new(self.tree, state, &self.style, block_ref);

        let RenderOutput {
            lines,
            total_lines,
            viewport,
        } = ctx.render(area);

        let mut paragraph = Paragraph::new(lines).style(self.style.style);
        if let Some(block) = block_ref {
            paragraph = paragraph.block(block.clone());
        }

        paragraph.render(viewport.area, buf);

        if let Some(scrollbar) = self.scrollbar {
            render_scrollbar(scrollbar, &viewport, total_lines, buf);
        }
    }
}
