pub mod help;
pub mod state;
pub mod widget;

use clap_cargo::style::{HEADER, USAGE};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation},
};

use help::HelpPopup;
use state::TuiState;
use widget::TreeWidget;

pub fn draw_tui(frame: &mut Frame, state: &mut TuiState) {
    draw_tree(frame, frame.area(), state);
    draw_help_text(frame, frame.area());
    if state.show_help {
        draw_help_popup(frame);
    }
}

pub fn draw_tree(frame: &mut Frame, area: Rect, state: &mut TuiState) {
    let tree_widget = TreeWidget::new(&state.dependency_tree).scrollbar(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .track_symbol(Some("┆"))
            .thumb_symbol("▐")
            .begin_symbol(Some("▴"))
            .end_symbol(Some("▾")),
    );
    frame.render_stateful_widget(tree_widget, area, &mut state.tree_widget_state);
}

pub fn draw_help_text(frame: &mut Frame, area: Rect) {
    let key_style = Style::from(HEADER)
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::REVERSED);

    let text = Line::from(vec![
        " q ".bold(),
        Span::styled(" QUIT ", key_style),
        " ? ".bold(),
        Span::styled(" HELP ", key_style),
    ]);

    let area = Rect {
        x: area.right().saturating_sub(text.width() as u16 + 2),
        y: area.bottom().saturating_sub(1),
        width: text.width() as u16,
        height: 1,
    };

    let paragraph = Paragraph::new(text).style(Style::from(USAGE));
    frame.render_widget(paragraph, area);
}

pub fn draw_help_popup(frame: &mut Frame) {
    let help_popup = HelpPopup::default();
    let size = help_popup.size();
    let area = Rect {
        x: frame.area().right().saturating_sub(size.width + 1),
        y: frame.area().bottom().saturating_sub(size.height + 1),
        width: size.width,
        height: size.height,
    };
    let area = frame.area().clamp(area);
    frame.render_widget(help_popup, area);
}
