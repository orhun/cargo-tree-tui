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

    /// Scrolls the viewport so that `focus_line` (0-indexed) stays visible,
    /// reusing the previous offset to keep the view stable.
    ///
    /// The returned `offset` may exceed `max_offset` — the render pipeline
    /// calls [`clamp_offset`] after accounting for context/breadcrumb lines.
    pub fn scroll_into_view(
        mut self,
        focus_line: usize,
        total_lines: usize,
        reserved_lines: usize,
        prev_offset: usize,
    ) -> Self {
        if self.height > 0 && reserved_lines > 0 {
            self.height = self.height.saturating_sub(reserved_lines);
        }
        if self.height == 0 {
            self.offset = 0;
            self.max_offset = 0;
            return self;
        }

        self.max_offset = total_lines.saturating_sub(self.height);
        let margin = (self.height / 4).max(1);

        // Start from the previous offset.
        let mut offset = prev_offset;

        // Scroll down if the selection is too close to the bottom edge.
        let bottom_threshold = offset + self.height.saturating_sub(margin);
        if focus_line >= bottom_threshold {
            offset = (focus_line + margin).saturating_sub(self.height);
        }

        // Scroll up if the selection is too close to the top edge.
        let top_threshold = offset + margin.saturating_sub(1);
        if focus_line < offset || (offset > 0 && focus_line <= top_threshold) {
            offset = focus_line.saturating_sub(margin.saturating_sub(1));
        }

        // When offset > 0, context/ancestor lines will consume at least 1 row,
        // reducing the usable content height. Re-check the bottom bound.
        if offset > 0 {
            let content_height = self.height.saturating_sub(1);
            if content_height > 0 && focus_line >= offset + content_height {
                offset = focus_line + 1 - content_height;
            }
        }

        self.offset = offset;
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
