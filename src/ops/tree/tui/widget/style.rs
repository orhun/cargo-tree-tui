use ratatui::style::{Color, Modifier, Style};

/// Visual configuration for [`TreeWidget`](super::TreeWidget).
#[derive(Debug)]
pub struct TreeWidgetStyle {
    pub highlight_style: Style,
    pub style: Style,
    pub name_style: Style,
    pub version_style: Style,
    pub suffix_style: Style,
    pub node_symbol: char,
    pub node_closed_symbol: char,
    pub node_open_symbol: char,
    pub branch_symbol: &'static str,
    pub last_branch_symbol: &'static str,
    pub continuation_symbol: &'static str,
    pub empty_symbol: &'static str,
}

/// TODO: Use styles defined in <https://docs.rs/clap-cargo/latest/clap_cargo/style/index.html>
/// This requires using the `anstyle` feature of Ratatui, which is not released yet.
/// See <https://github.com/orhun/cargo-tree-tui/issues/9>
impl Default for TreeWidgetStyle {
    fn default() -> Self {
        Self {
            highlight_style: Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
            style: Style::default(),
            name_style: Style::default(),
            version_style: Style::default().fg(Color::Green),
            suffix_style: Style::default().fg(Color::Cyan),
            node_symbol: '•',
            node_closed_symbol: '▸',
            node_open_symbol: '▾',
            branch_symbol: "├──",
            last_branch_symbol: "└──",
            continuation_symbol: "│  ",
            empty_symbol: "   ",
        }
    }
}
