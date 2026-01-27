use clap_cargo::style::{DEP_BUILD, NOP, PLACEHOLDER, WARN};
use ratatui::style::{Modifier, Style};

/// Visual configuration for [`TreeWidget`](super::TreeWidget).
#[derive(Debug)]
pub struct TreeWidgetStyle {
    pub highlight_style: Style,
    pub style: Style,
    pub context_style: Style,
    pub name_style: Style,
    pub version_style: Style,
    pub suffix_style: Style,
    pub node_symbol: char,
    pub node_closed_symbol: char,
    pub node_open_symbol: char,
    pub branch_symbol: &'static str,
    pub last_branch_symbol: &'static str,
    pub continuation_symbol: &'static str,
    pub more_below_symbol: &'static str,
    pub empty_symbol: &'static str,
}

impl Default for TreeWidgetStyle {
    fn default() -> Self {
        Self {
            highlight_style: Style::from(WARN).add_modifier(Modifier::BOLD),
            style: NOP.into(),
            context_style: Modifier::DIM.into(),
            name_style: NOP.into(),
            version_style: PLACEHOLDER.into(),
            suffix_style: DEP_BUILD.into(),
            node_symbol: '•',
            node_closed_symbol: '▸',
            node_open_symbol: '▾',
            branch_symbol: "├──",
            last_branch_symbol: "└──",
            continuation_symbol: "│  ",
            more_below_symbol: "│↓ ",
            empty_symbol: "   ",
        }
    }
}
