pub mod state;
pub mod widget;
pub mod widget_state;

use ratatui::{
    Frame,
    widgets::{Block, Scrollbar, ScrollbarOrientation},
};

use state::TuiState;
use widget::TreeWidget;

pub fn draw_tui(frame: &mut Frame, state: &mut TuiState) {
    let tree_widget = TreeWidget::new(&state.dependency_tree)
        .block(Block::bordered())
        .scrollbar(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
        );
    frame.render_stateful_widget(tree_widget, frame.area(), &mut state.tree_widget_state);
}
