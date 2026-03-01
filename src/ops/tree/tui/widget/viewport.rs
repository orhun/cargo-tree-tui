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
    /// Creates a new viewport for the given area and optional block.
    pub fn new(area: Rect, block: Option<&Block<'_>>) -> Self {
        let inner = block.map(|b| b.inner(area)).unwrap_or(area);
        let height = inner.height as usize;
        Self {
            area,
            inner,
            height,
            offset: 0,
            max_offset: 0,
        }
    }

    /// Centers the viewport on the given focus line, accounting for reserved lines.
    pub fn center_on(
        mut self,
        focus_line: usize,
        total_lines: usize,
        reserved_lines: usize,
    ) -> Self {
        if self.height > 0 && reserved_lines > 0 {
            self.height = self.height.saturating_sub(reserved_lines);
        }
        if self.height > 0 {
            if self.height == 0 {
                self.offset = 0;
                self.max_offset = 0;
            } else {
                let center_line = self.height.div_ceil(2);
                let offset = focus_line.saturating_sub(center_line);
                let max_offset = total_lines.saturating_sub(self.height);

                self.offset = offset;
                self.max_offset = max_offset;
            }
        }
        self
    }

    /// Clamps the current offset to valid bounds based on total lines and extra offset.
    pub fn clamp_offset(&mut self, total_lines: usize, extra_offset: usize) {
        if self.offset >= self.max_offset {
            let max_offset = total_lines.saturating_sub(self.height) + extra_offset;
            self.offset = self.offset.min(max_offset);
        }
    }
}
