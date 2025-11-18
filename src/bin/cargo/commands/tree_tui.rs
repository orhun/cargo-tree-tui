use anyhow::Result;
use crossterm::event::{self, Event};

use cargo_tree_tui::ops::tree::tui::{draw_tui, state::TuiState};

use crate::cli::TreeArgs;

/// Entry point for the `cargo tree-tui` command.
pub fn run(args: TreeArgs) -> Result<()> {
    let mut state = TuiState::new(args.manifest_path)?;
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
