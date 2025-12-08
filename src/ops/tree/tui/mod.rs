pub mod help;
pub mod state;
pub mod widget;
pub mod widget_state;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation},
};

use help::HelpPopup;
use state::TuiState;
use widget::TreeWidget;

pub fn draw_tui(frame: &mut Frame, state: &mut TuiState) {
    let [tree_area, help_text_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    draw_tree(frame, tree_area, state);
    draw_help_text(frame, help_text_area);
    if state.show_help {
        draw_help_popup(frame);
    }
}

pub fn draw_tree(frame: &mut Frame, area: Rect, state: &mut TuiState) {
    let tree_widget = TreeWidget::new(&state.dependency_tree).scrollbar(
        Scrollbar::new(ScrollbarOrientation::VerticalLeft)
            .track_symbol(Some("┆"))
            .thumb_symbol("▐")
            .begin_symbol(Some("▴"))
            .end_symbol(Some("▾")),
    );
    frame.render_stateful_widget(tree_widget, area, &mut state.tree_widget_state);
}

pub fn draw_help_text(frame: &mut Frame, area: Rect) {
    let key_style = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::REVERSED);

    let text = Line::from(vec![
        " q ".bold(),
        Span::styled(" QUIT ", key_style),
        " ? ".bold(),
        Span::styled(" HELP ", key_style),
    ]);

    let paragraph = Paragraph::new(text).style(Style::default().bg(Color::Black).fg(Color::White));
    frame.render_widget(paragraph, area);
}

pub fn draw_help_popup(frame: &mut Frame) {
    let help_popup = HelpPopup::default();
    let size = help_popup.size();
    let area = Rect {
        x: frame.area().right().saturating_sub(size.width + 2),
        y: frame.area().bottom().saturating_sub(size.height + 2),
        width: size.width,
        height: size.height,
    };
    let area = frame.area().clamp(area);
    frame.render_widget(help_popup, area);
}
