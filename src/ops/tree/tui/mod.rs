pub mod state;
pub mod widget;
pub mod widget_state;

use ratatui::{
    Frame,
    layout::Margin,
    widgets::{Block, Scrollbar, ScrollbarOrientation},
};

use state::TuiState;
use widget::TreeWidget;

pub fn draw_tui(frame: &mut Frame, state: &mut TuiState) {
    let area = frame.area().inner(Margin {
        horizontal: 0,
        vertical: 1,
    });
    let tree_widget = TreeWidget::new(&state.dependency_tree)
        .block(Block::bordered())
        .scrollbar(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
        );
    frame.render_stateful_widget(tree_widget, area, &mut state.tree_widget_state);
}
