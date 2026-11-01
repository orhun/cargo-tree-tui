use std::borrow::Cow;

use clap_cargo::style::{HEADER, NOP, VALID, WARN};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget, Widget, Wrap,
    },
};

use crate::core::Dependency;

pub struct MetadataPopup<'a> {
    dependency: Option<&'a Dependency>,
}

impl<'a> MetadataPopup<'a> {
    pub fn new(dependency: Option<&'a Dependency>) -> Self {
        Self { dependency }
    }

    fn lines(&self) -> Text<'_> {
        let Some(dependency) = self.dependency else {
            return Text::from(" No dependency selected ");
        };

        let metadata = dependency.format_metadata.as_deref();
        let mut lines = Vec::with_capacity(15);
        push_field_styled(&mut lines, "Name", &dependency.name, Style::from(WARN));
        push_field(&mut lines, "Version", &dependency.version);
        push_field(
            &mut lines,
            "Edition",
            metadata
                .and_then(|metadata| metadata.edition.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "Rust version",
            metadata
                .and_then(|metadata| metadata.rust_version.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "Targets",
            metadata
                .and_then(|metadata| metadata.targets.as_deref())
                .unwrap_or("Not specified"),
        );

        if let Some(package_id) = &dependency.package_id {
            let source = package_id.source_id();
            let source = if source.is_crates_io() {
                "crates.io"
            } else {
                metadata
                    .and_then(|metadata| metadata.source.as_deref())
                    .unwrap_or("Not specified")
            };
            push_field(&mut lines, "Source", source);
        }

        push_field(
            &mut lines,
            "Description",
            metadata
                .and_then(|metadata| metadata.description.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "License",
            metadata
                .and_then(|metadata| metadata.license.as_deref())
                .unwrap_or("Not specified"),
        );
        push_link_field(
            &mut lines,
            "Repository",
            metadata.and_then(|metadata| metadata.repository.as_deref()),
        );
        push_link_field(
            &mut lines,
            "Homepage",
            metadata.and_then(|metadata| metadata.homepage.as_deref()),
        );
        push_link_field(
            &mut lines,
            "Documentation",
            metadata.and_then(|metadata| metadata.documentation.as_deref()),
        );
        push_field(
            &mut lines,
            "Authors",
            metadata
                .and_then(|metadata| metadata.authors.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "Keywords",
            metadata
                .and_then(|metadata| metadata.keywords.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "Categories",
            metadata
                .and_then(|metadata| metadata.categories.as_deref())
                .unwrap_or("Not specified"),
        );
        if let Some(library) = metadata.and_then(|metadata| metadata.library.as_deref()) {
            push_field(&mut lines, "Library", library);
        }
        if dependency.is_proc_macro {
            push_field(&mut lines, "Target", "proc-macro");
        }
        if let Some(path) = &dependency.manifest_dir {
            push_field(&mut lines, "Manifest", path.as_str());
        }

        Text::from(lines)
    }
}

impl StatefulWidget for MetadataPopup<'_> {
    type State = usize;

    fn render(self, area: Rect, buf: &mut Buffer, scroll: &mut Self::State) {
        let width = area.width / 2;
        let height = area.height.saturating_sub(2).min(20);
        if width < 4 || height < 3 {
            return;
        }
        let lines = self.lines();
        let popup_area = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: area.y + area.height.saturating_sub(height).saturating_sub(2),
            width,
            height,
        };

        Clear.render(popup_area, buf);
        let title_style = Style::from(HEADER)
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::REVERSED);
        let block = Block::default()
            .title(Line::from(Span::styled(
                " DEPENDENCY METADATA ",
                title_style,
            )))
            .borders(Borders::ALL)
            .border_style(Style::from(HEADER));
        let inner = block.inner(popup_area);
        let content_area = Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        };
        let scrollbar_area = Rect {
            x: inner.right().saturating_sub(1),
            width: 1,
            ..inner
        };
        let content_height = wrapped_line_count(&lines, content_area.width as usize);
        let paragraph = Paragraph::new(lines)
            .style(Style::from(NOP))
            .wrap(Wrap { trim: true });
        let max_scroll = content_height.saturating_sub(content_area.height as usize);
        *scroll = (*scroll).min(max_scroll);
        let scroll_offset = (*scroll).min(u16::MAX as usize) as u16;
        let mut scrollbar_state = ScrollbarState::new(max_scroll)
            .position(*scroll)
            .viewport_content_length(content_area.height as usize);

        block.render(popup_area, buf);
        paragraph
            .scroll((scroll_offset, 0))
            .render(content_area, buf);
        if max_scroll > 0 {
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .track_symbol(Some("┆"))
                .thumb_symbol("▐")
                .render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }
}

fn wrapped_line_count(text: &Text<'_>, width: usize) -> usize {
    text.lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(width.max(1)))
        .sum()
}

fn push_field<'a>(lines: &mut Vec<Line<'a>>, label: &'static str, value: impl Into<Cow<'a, str>>) {
    push_field_styled(lines, label, value, Style::default());
}

fn push_field_styled<'a>(
    lines: &mut Vec<Line<'a>>,
    label: &'static str,
    value: impl Into<Cow<'a, str>>,
    value_style: Style,
) {
    const LABEL_SPACING: &str = "              ";
    let padding = LABEL_SPACING.len().saturating_sub(label.len());
    lines.push(Line::from(vec![
        Span::styled(label, Style::from(VALID)),
        Span::raw(&LABEL_SPACING[..padding]),
        Span::raw(value).style(value_style),
    ]));
}

fn push_link_field<'a>(lines: &mut Vec<Line<'a>>, label: &'static str, value: Option<&'a str>) {
    push_field(lines, label, value.unwrap_or("Not specified"));
    if let Some(value) = lines.last_mut().and_then(|line| line.spans.last_mut()) {
        value.style = value.style.add_modifier(Modifier::ITALIC);
    }
}
