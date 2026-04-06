use std::{sync::mpsc, thread, time::Duration};

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent};

use cargo_tree_tui::{
    core::DependencyTree,
    ops::tree::tui::{
        draw_tui,
        state::{Event, SearchRequest, SearchResult, TuiState},
        widget::TreeWidgetState,
    },
};

use crate::cli::TreeArgs;

/// Entry point for the `cargo tree-tui` command.
pub fn run(args: TreeArgs) -> Result<()> {
    let dependency_tree = DependencyTree::load(args.manifest_path)?;

    let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let worker_tree = dependency_tree.clone();
    let worker_handle = thread::spawn(move || search_worker(worker_tree, search_rx, event_tx));

    let mut state = TuiState::new(dependency_tree, search_tx);
    let mut terminal = ratatui::init();

    while state.running {
        terminal.draw(|frame| draw_tui(frame, &mut state))?;

        while let Ok(event) = event_rx.try_recv() {
            state.handle_event(event);
        }

        if event::poll(Duration::from_millis(16))?
            && let CrosstermEvent::Key(key_event) = event::read()?
        {
            state.handle_event(Event::Key(key_event));
        }
    }

    drop(state);
    ratatui::restore();
    let _ = worker_handle.join();
    Ok(())
}

fn search_worker(
    dependency_tree: DependencyTree,
    search_rx: mpsc::Receiver<SearchRequest>,
    event_tx: mpsc::Sender<Event>,
) {
    while let Ok(mut request) = search_rx.recv() {
        while let Ok(next_request) = search_rx.try_recv() {
            request = next_request;
        }

        let search_state = TreeWidgetState::search(&dependency_tree, &request.query);
        let event = Event::SearchResult(SearchResult {
            generation: request.generation,
            query: request.query,
            search_state,
        });

        if event_tx.send(event).is_err() {
            break;
        }
    }
}
