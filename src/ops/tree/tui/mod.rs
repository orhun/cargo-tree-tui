pub mod state;
pub mod widget;
pub mod widget_state;

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation},
};

use state::TuiState;
use widget::TreeWidget;

pub fn draw_tui(frame: &mut Frame, state: &mut TuiState) {
    let layout = Layout::vertical([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    let tree_widget = TreeWidget::new(&state.dependency_tree)
        .block(Block::bordered())
        .scrollbar(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
        );
    frame.render_stateful_widget(tree_widget, layout[0], &mut state.tree_widget_state);

    let help = Paragraph::new("←/→ expand/collapse | ↑/↓ navigate | PgUp/PgDn scroll | q quit");
    frame.render_widget(help, layout[1]);
}
