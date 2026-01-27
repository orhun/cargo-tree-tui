use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Paragraph, Scrollbar, ScrollbarState, StatefulWidget, Widget},
};

use crate::{core::DependencyTree, ops::tree::tui::widget::viewport::Viewport};

use self::{breadcrumb::Breadcrumb, render::RenderContext};

pub use self::{render::RenderOutput, state::TreeWidgetState, style::TreeWidgetStyle};

mod breadcrumb;
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
        let RenderOutput {
            lines,
            context_lines,
            total_lines,
            viewport,
        } = {
            let mut ctx = RenderContext::new(self.tree, state, &self.style, block_ref);
            ctx.render(area)
        };

        if let Some(block) = block_ref {
            block.clone().render(viewport.area, buf);
        }

        let mut content_area = viewport.inner;
        let context_lines_len = context_lines.len() as u16;
        let context_area = if viewport.offset > 0 && content_area.height > context_lines_len {
            let area = Rect {
                y: content_area.y,
                height: context_lines_len,
                ..content_area
            };
            content_area.y = content_area.y.saturating_add(context_lines_len);
            content_area.height = content_area.height.saturating_sub(context_lines_len);
            Some(area)
        } else {
            None
        };

        let breadcrumb_area = if content_area.height > 0 {
            content_area.height = content_area.height.saturating_sub(1);
            Some(Rect {
                y: content_area.y.saturating_add(content_area.height),
                height: 1,
                ..content_area
            })
        } else {
            None
        };

        if let Some(area) = context_area {
            Paragraph::new(context_lines)
                .style(self.style.context_style)
                .render(area, buf);
        }

        if content_area.height > 0 {
            Paragraph::new(lines)
                .style(self.style.style)
                .render(content_area, buf);
        }

        if let Some(area) = breadcrumb_area {
            Breadcrumb::new(self.tree, state, &self.style).render(area, buf);
        }

        if let Some(scrollbar) = self.scrollbar {
            render_scrollbar(scrollbar, &viewport, total_lines, buf);
        }
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

    let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(viewport.height))
        .position(viewport.offset)
        .viewport_content_length(viewport.height);

    scrollbar.render(viewport.inner, buf, &mut scrollbar_state);
}
