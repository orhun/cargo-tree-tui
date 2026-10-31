use clap_cargo::style::{HEADER, NOP, VALID};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, StatefulWidget, Widget,
    },
};

#[derive(Debug, Clone)]
pub struct SelectorOption {
    pub value: String,
    pub label: String,
    pub checked: bool,
}

impl SelectorOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>, checked: bool) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            checked,
        }
    }
}

/// Reusable state for a popup that selects multiple named options.
#[derive(Debug, Clone)]
pub struct SelectorState {
    pub title: String,
    pub options: Vec<SelectorOption>,
    pub selected: usize,
    pub applying: bool,
    pub message: Option<String>,
}

impl SelectorState {
    pub fn new(title: impl Into<String>, options: Vec<SelectorOption>) -> Self {
        Self {
            title: title.into(),
            options,
            selected: 0,
            applying: false,
            message: None,
        }
    }

    pub fn checked_values(&self) -> Vec<String> {
        self.options
            .iter()
            .filter(|option| option.checked)
            .map(|option| option.value.clone())
            .collect()
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        self.selected = self
            .selected
            .saturating_add(1)
            .min(self.options.len().saturating_sub(1));
    }

    pub fn toggle_selected(&mut self) {
        if let Some(option) = self.options.get_mut(self.selected) {
            option.checked = !option.checked;
        }
    }
}

pub struct SelectorPopup<'a> {
    state: &'a SelectorState,
}

impl<'a> SelectorPopup<'a> {
    pub fn new(state: &'a SelectorState) -> Self {
        Self { state }
    }

    fn size(&self, area: Rect) -> Size {
        let options_width = self
            .state
            .options
            .iter()
            .map(|option| option.label.chars().count().saturating_add(4))
            .max()
            .unwrap_or(0);
        let message_width = self
            .state
            .message
            .as_ref()
            .map_or(0, |message| message.chars().count().saturating_add(2));
        let footer_width = if self.state.applying {
            "Resolving dependency graph...".len()
        } else {
            "Space toggle   Enter apply   Esc cancel".len()
        }
        .saturating_add(2);
        let width = options_width
            .max(message_width)
            .max(footer_width)
            .saturating_add(2)
            .min(usize::from(area.width));
        let height = self
            .state
            .options
            .len()
            .saturating_add(5)
            .saturating_add(usize::from(self.state.message.is_some()))
            .min(usize::from(area.height));
        Size::new(width as u16, height as u16)
    }
}

impl Widget for SelectorPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let size = self.size(area);
        let popup_area = Rect {
            x: area.x + area.width.saturating_sub(size.width) / 2,
            y: area.y + area.height.saturating_sub(size.height) / 2,
            width: size.width,
            height: size.height,
        };
        Clear.render(popup_area, buf);

        let block = Block::new()
            .title(format!(" {} ", self.state.title.to_uppercase()))
            .title_style(
                Style::from(HEADER)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .borders(Borders::ALL)
            .border_style(Style::from(HEADER));
        let selected_style = Style::from(HEADER).add_modifier(Modifier::BOLD);
        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let mut items = vec![ListItem::new(Line::default())];
        items.extend(self.state.options.iter().map(|option| {
            let checkbox = if option.checked { "✔" } else { "□" };
            ListItem::new(Line::from(vec![
                Span::raw(" "),
                Span::raw(checkbox),
                Span::raw(" "),
                Span::raw(option.label.as_str()),
                Span::raw(" "),
            ]))
        }));
        items.push(ListItem::new(Line::default()));

        let list = List::new(items)
            .highlight_style(selected_style)
            .highlight_symbol("");
        let mut list_state = ListState::default().with_selected(Some(self.state.selected + 1));
        let footer_height = 1 + u16::from(self.state.message.is_some());
        let [list_area, footer_area] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(footer_height)]).areas(inner);
        StatefulWidget::render(list, list_area, buf, &mut list_state);

        let mut footer_lines = Vec::with_capacity(footer_height as usize);
        if let Some(message) = &self.state.message {
            footer_lines.push(Line::from(format!(" {message} ")));
        }
        let footer = if self.state.applying {
            Line::from("Resolving dependency graph...")
        } else {
            let key_style = Style::from(VALID);
            Line::from(vec![
                Span::raw(" "),
                Span::styled("Space", key_style),
                Span::raw(" toggle   "),
                Span::styled("Enter", key_style),
                Span::raw(" apply   "),
                Span::styled("Esc", key_style),
                Span::raw(" cancel "),
            ])
        }
        .style(Style::from(NOP));
        footer_lines.push(footer);

        Paragraph::new(footer_lines)
            .style(Style::from(NOP))
            .render(footer_area, buf);
    }
}
