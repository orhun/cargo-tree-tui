use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

const KEY_BINDINGS: &str = r#"
 ?   Help 
 /   Search
 ←   Collapse selected
 →   Expand selected
 [   Go to previous sibling
 ]   Go to next sibling
 p   Go to parent
 q   Quit 
"#;

#[derive(Debug)]
pub struct HelpPopupStyle {
    border: Style,
    title: Style,
    default: Style,
}

impl Default for HelpPopupStyle {
    fn default() -> Self {
        HelpPopupStyle {
            border: Style::default().fg(ratatui::style::Color::Yellow),
            title: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            default: Style::default().fg(Color::White),
        }
    }
}

#[derive(Debug)]
pub struct HelpPopup<'a> {
    title: Line<'a>,
    content: Text<'a>,
    style: HelpPopupStyle,
}

impl Default for HelpPopup<'_> {
    fn default() -> Self {
        HelpPopup {
            title: Line::from("Commands"),
            content: Text::from(KEY_BINDINGS.trim_start_matches('\n')),
            style: HelpPopupStyle::default(),
        }
    }
}

impl<'a> HelpPopup<'a> {
    pub fn size(&self) -> Size {
        Size {
            width: (self.content.width() + 2) as u16,
            height: (self.content.height() + 2) as u16,
        }
    }
}

impl Widget for HelpPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let block = Block::new()
            .title(self.title)
            .title_style(self.style.title)
            .borders(Borders::ALL)
            .border_style(self.style.border);

        Paragraph::new(self.content)
            .style(self.style.default)
            .block(block)
            .render(area, buf);
    }
}
