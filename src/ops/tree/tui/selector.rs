use clap_cargo::style::{HEADER, NOP, VALID};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget,
    },
};

#[derive(Debug, Clone)]
pub struct SelectorOption {
    pub value: String,
    pub label: String,
    pub checked: bool,
    pub exclusive: bool,
}

impl SelectorOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>, checked: bool) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            checked,
            exclusive: false,
        }
    }

    pub fn exclusive(mut self) -> Self {
        self.exclusive = true;
        self
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
    pub filter: Option<String>,
    max_height: usize,
    visible_options: Vec<usize>,
}

impl SelectorState {
    pub fn new(title: impl Into<String>, options: Vec<SelectorOption>) -> Self {
        let visible_options = (0..options.len()).collect();
        Self {
            title: title.into(),
            options,
            selected: 0,
            applying: false,
            message: None,
            filter: None,
            max_height: 14,
            visible_options,
        }
    }

    pub fn filterable(mut self) -> Self {
        self.filter = Some(String::new());
        self
    }

    pub fn max_height(mut self, max_height: usize) -> Self {
        self.max_height = max_height;
        self
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
            .min(self.visible_options.len().saturating_sub(1));
    }

    pub fn push_filter_char(&mut self, ch: char) {
        if let Some(filter) = &mut self.filter {
            filter.push(ch);
            self.selected = 0;
            self.refresh_visible_options();
        }
    }

    pub fn pop_filter_char(&mut self) {
        if let Some(filter) = &mut self.filter
            && filter.pop().is_some()
        {
            self.selected = 0;
            self.refresh_visible_options();
        }
    }

    pub fn visible_options(&self) -> &[usize] {
        &self.visible_options
    }

    fn refresh_visible_options(&mut self) {
        let filter = self.filter.as_deref().unwrap_or_default().to_lowercase();
        self.visible_options = self
            .options
            .iter()
            .enumerate()
            .filter_map(|(index, option)| {
                option
                    .label
                    .to_lowercase()
                    .contains(&filter)
                    .then_some(index)
            })
            .collect();
        self.selected = self
            .selected
            .min(self.visible_options.len().saturating_sub(1));
    }

    pub fn toggle_selected(&mut self) {
        let Some(&option_index) = self.visible_options.get(self.selected) else {
            return;
        };
        let option = &self.options[option_index];
        let was_checked = option.checked;
        let is_exclusive = option.exclusive;

        if is_exclusive && !was_checked {
            for option in &mut self.options {
                option.checked = false;
            }
            self.options[option_index].checked = true;
        } else {
            self.options[option_index].checked = !was_checked;
            if !is_exclusive && !was_checked {
                for option in &mut self.options {
                    if option.exclusive {
                        option.checked = false;
                    }
                }
            }
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
        let filter_width = self.state.filter.as_ref().map_or(0, |filter| {
            "Filter: ".chars().count() + filter.chars().count() + 2
        });
        let footer_width = if self.state.applying {
            "Resolving dependency graph...".len()
        } else if self.state.filter.is_some() {
            "Type filter   Backspace erase   Space toggle   Enter apply   Esc cancel".len()
        } else {
            "Space toggle   Enter apply   Esc cancel".len()
        }
        .saturating_add(2);
        let width = options_width
            .max(filter_width)
            .max(message_width)
            .max(footer_width)
            .saturating_add(2)
            .min(usize::from(area.width));
        let height = self
            .state
            .options
            .len()
            .saturating_add(5)
            .saturating_add(usize::from(self.state.filter.is_some()))
            .saturating_add(usize::from(self.state.message.is_some()))
            .min(self.state.max_height)
            .min(usize::from(area.height));
        Size::new(width as u16, height as u16)
    }

    pub fn cursor_position(&self, area: Rect) -> Option<Position> {
        let filter = self.state.filter.as_ref()?;
        if self.state.applying {
            return None;
        }

        let size = self.size(area);
        let popup_area = Rect {
            x: area.x + area.width.saturating_sub(size.width) / 2,
            y: area.y + area.height.saturating_sub(size.height) / 2,
            width: size.width,
            height: size.height,
        };
        let x = popup_area
            .x
            .saturating_add(2)
            .saturating_add(Line::from("Filter: ").width() as u16)
            .saturating_add(Line::from(filter.as_str()).width() as u16)
            .min(popup_area.right().saturating_sub(2));
        Some(Position::new(x, popup_area.y.saturating_add(1)))
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

        let items = self
            .state
            .visible_options()
            .iter()
            .map(|&index| {
                let option = &self.state.options[index];
                let checkbox = if option.checked { "✔" } else { "□" };
                ListItem::new(Line::from(vec![
                    Span::raw(" "),
                    Span::raw(checkbox),
                    Span::raw(" "),
                    Span::raw(option.label.as_str()),
                    Span::raw(" "),
                ]))
            })
            .collect::<Vec<_>>();

        let list = List::new(items)
            .highlight_style(selected_style)
            .highlight_symbol("");
        let mut list_state = ListState::default().with_selected(
            (!self.state.visible_options().is_empty()).then_some(self.state.selected),
        );
        let footer_height = 1 + u16::from(self.state.message.is_some());
        let mut constraints = Vec::with_capacity(5);
        if self.state.filter.is_some() {
            constraints.push(Constraint::Length(1));
        }
        constraints.extend([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(footer_height),
        ]);
        let areas = Layout::vertical(constraints).split(inner);
        let offset = usize::from(self.state.filter.is_some());
        if let Some(filter) = &self.state.filter {
            let filter_line = Line::from(vec![
                Span::raw(" "),
                Span::styled("Filter: ", Style::from(VALID)),
                Span::raw(filter.as_str()),
            ]);
            Paragraph::new(filter_line)
                .style(Style::from(NOP))
                .render(areas[0], buf);
        }
        let list_area = areas[offset + 1];
        let footer_area = areas[offset + 3];
        StatefulWidget::render(list, list_area, buf, &mut list_state);
        if list_area.height > 0
            && self.state.visible_options().len() > usize::from(list_area.height)
        {
            let mut scrollbar_state = ScrollbarState::new(
                self.state
                    .visible_options()
                    .len()
                    .saturating_sub(usize::from(list_area.height)),
            )
            .position(list_state.offset())
            .viewport_content_length(usize::from(list_area.height));
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_style(Style::from(NOP).add_modifier(Modifier::DIM))
                .thumb_style(Style::from(HEADER).add_modifier(Modifier::BOLD))
                .begin_style(Style::from(NOP))
                .end_style(Style::from(NOP))
                .render(list_area, buf, &mut scrollbar_state);
        }

        let mut footer_lines = Vec::with_capacity(footer_height as usize);
        if let Some(message) = &self.state.message {
            footer_lines.push(Line::from(format!(" {message} ")));
        }
        let footer = if self.state.applying {
            Line::from("Resolving dependency graph...")
        } else if self.state.filter.is_some() {
            let key_style = Style::from(VALID);
            Line::from(vec![
                Span::raw(" "),
                Span::styled("Type", key_style),
                Span::raw(" filter   "),
                Span::styled("Backspace", key_style),
                Span::raw(" erase   "),
                Span::styled("Space", key_style),
                Span::raw(" toggle   "),
                Span::styled("Enter", key_style),
                Span::raw(" apply   "),
                Span::styled("Esc", key_style),
                Span::raw(" cancel "),
            ])
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
