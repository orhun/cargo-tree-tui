use ratatui::{layout::Rect, widgets::Block};

/// Viewport information for rendering the tree widget.
#[derive(Debug, Copy, Clone, Default)]
pub struct Viewport {
    /// The full area allocated for the widget.
    pub area: Rect,
    /// The inner area after accounting for borders and padding.
    pub inner: Rect,
    /// Height of the inner area.
    pub height: usize,
    /// Current scroll offset.
    pub offset: usize,
    /// Maximum scroll offset.
    pub max_offset: usize,
}

impl Viewport {
    pub fn new(
        area: Rect,
        block: Option<&Block<'_>>,
        selected_line: usize,
        total_lines: usize,
    ) -> Self {
        let inner = block.map(|b| b.inner(area)).unwrap_or(area);
        let height = inner.height as usize;

        let mut offset = if height == 0 {
            0
        } else {
            let center_line = height.div_ceil(2);
            selected_line.saturating_sub(center_line)
        };

        let max_offset = if height == 0 {
            0
        } else {
            total_lines.saturating_sub(height)
        };

        offset = offset.min(max_offset);

        Self {
            area,
            inner,
            height,
            offset,
            max_offset,
        }
    }
}
