use anyhow::Result;
use crossterm::event::{self, Event};

use cargo_tree_tui::{
    core::DependencyTree,
    ops::tree::tui::{draw_tui, state::TuiState},
};

use crate::cli::TreeArgs;

/// Entry point for the `cargo tree-tui` command.
pub fn run(args: TreeArgs) -> Result<()> {
    let dependency_tree = DependencyTree::load(args.manifest_path)?;
    let mut state = TuiState::new(dependency_tree)?;
    let mut terminal = ratatui::init();

    while state.running {
        terminal.draw(|frame| draw_tui(frame, &mut state))?;

        if let Event::Key(key_event) = event::read()? {
            state.handle_key_event(key_event);
        }
    }

    ratatui::restore();
    Ok(())
}
