pub mod state;
pub mod widget;
pub mod widget_state;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation},
    Frame,
};

use state::TuiState;
use widget::TreeWidget;

// ─────────────────────────────────────────────────────────────────────────────
// Colors
// ─────────────────────────────────────────────────────────────────────────────
const BORDER_COLOR: Color = Color::Rgb(88, 92, 112);
const TITLE_COLOR: Color = Color::Rgb(255, 183, 77);
const HELP_KEY_COLOR: Color = Color::Rgb(180, 142, 255);
const HELP_DESC_COLOR: Color = Color::Rgb(138, 143, 163);
const HELP_SEPARATOR: Color = Color::Rgb(68, 71, 90);

pub fn draw_tui(frame: &mut Frame, state: &mut TuiState) {
    let area = frame.area();

    // Layout: tree area + help bar at bottom
    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);

    draw_tree(frame, state, chunks[0]);
    draw_help_bar(frame, chunks[1]);

    // Draw help panel overlay if toggled
    if state.show_help {
        draw_help_panel(frame, area);
    }
}

fn draw_tree(frame: &mut Frame, state: &mut TuiState, area: Rect) {
    let visible = state.tree_widget_state.visible_nodes(&state.dependency_tree);
    let position = state
        .tree_widget_state
        .selected_position(&state.dependency_tree)
        .map(|p| p + 1)
        .unwrap_or(0);
    let total = visible.len();

    let title = format!(
        " {} ",
        state.dependency_tree.workspace_name
    );
    let counter = format!(" {}/{} ", position, total);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER_COLOR))
        .title(Span::styled(
            title,
            Style::default().fg(TITLE_COLOR).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(vec![Span::styled(
            counter,
            Style::default().fg(HELP_DESC_COLOR),
        )]).right_aligned());

    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .thumb_symbol("┃")
        .track_symbol(Some("│"))
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_style(Style::default().fg(TITLE_COLOR))
        .track_style(Style::default().fg(BORDER_COLOR));

    let tree_widget = TreeWidget::new(&state.dependency_tree)
        .block(block)
        .scrollbar(scrollbar);

    frame.render_stateful_widget(tree_widget, area, &mut state.tree_widget_state);
}

fn draw_help_bar(frame: &mut Frame, area: Rect) {
    let shortcuts = [
        ("↑↓/jk", "navigate"),
        ("←→/hl", "collapse/expand"),
        ("o", "toggle"),
        ("p", "parent"),
        ("[]", "siblings"),
        ("1-9", "depth"),
        ("?", "help"),
        ("q", "quit"),
    ];

    let mut spans = Vec::new();
    for (i, (key, desc)) in shortcuts.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(HELP_SEPARATOR)));
        }
        spans.push(Span::styled(
            *key,
            Style::default()
                .fg(HELP_KEY_COLOR)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}", desc),
            Style::default().fg(HELP_DESC_COLOR),
        ));
    }

    let help_line = Line::from(spans);
    let paragraph = Paragraph::new(help_line);
    frame.render_widget(paragraph, area);
}

fn draw_help_panel(frame: &mut Frame, area: Rect) {
    // Center the help panel
    let panel_width = 52.min(area.width.saturating_sub(4));
    let panel_height = 20.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(panel_width)) / 2;
    let y = (area.height.saturating_sub(panel_height)) / 2;
    let panel_area = Rect::new(x, y, panel_width, panel_height);

    // Clear background
    frame.render_widget(Clear, panel_area);

    let help_sections = vec![
        ("Navigation", vec![
            ("↑ / k", "Move up"),
            ("↓ / j", "Move down"),
            ("← / h", "Collapse / go to parent"),
            ("→ / l", "Expand / go to first child"),
            ("p", "Jump to parent"),
            ("[ / ]", "Previous / next sibling"),
        ]),
        ("Fast Movement", vec![
            ("gg / Home", "Go to first item"),
            ("G / End", "Go to last item"),
            ("Ctrl+u / Ctrl+d", "Half page up / down"),
            ("PgUp / PgDn", "Full page up / down"),
        ]),
        ("Expand/Collapse", vec![
            ("o / Space / Enter", "Toggle node"),
            ("O", "Expand all children"),
            ("c", "Collapse entire tree"),
            ("1-9", "Expand to depth level"),
        ]),
        ("General", vec![
            ("?", "Toggle this help"),
            ("q / Esc", "Quit"),
        ]),
    ];

    let mut lines = vec![Line::from("")];

    for (section, items) in help_sections {
        lines.push(Line::from(Span::styled(
            format!(" {}", section),
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in items {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("   {:18}", key),
                    Style::default().fg(HELP_KEY_COLOR),
                ),
                Span::styled(desc, Style::default().fg(HELP_DESC_COLOR)),
            ]));
        }
        lines.push(Line::from(""));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER_COLOR))
        .title(Span::styled(
            " Keyboard Shortcuts ",
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, panel_area);
}
