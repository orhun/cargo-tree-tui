use std::{sync::Arc, sync::mpsc, thread, time::Duration};

use anyhow::{Result, bail};
use crossterm::event::{self, Event as CrosstermEvent};

use cargo_tree_tui::{
    core::ResolutionOptions,
    ops::tree::tui::TuiTree,
    ops::tree::tui::{
        draw_tui,
        state::{Event, SearchRequest, SearchResult, TuiState},
        widget::{TreeCharset, TreeFormat, TreePrefix, TreeWidgetState},
    },
};

use crate::cli::TreeArgs;

/// Entry point for the `cargo tree-tui` command.
pub fn run(args: TreeArgs) -> Result<()> {
    let no_dedupe = args.no_dedupe || args.all;
    let (depth, workspace_depth) = match args.depth {
        Some(crate::cli::Depth::Number(depth)) => (Some(depth), false),
        Some(crate::cli::Depth::Workspace) => (None, true),
        None => (None, false),
    };
    let prefix = resolve_prefix(args.prefix, args.no_indent, args.prefix_depth)?;
    let format = TreeFormat::parse(&args.format)?;
    let tui_prefix = match prefix {
        crate::cli::Prefix::Depth => TreePrefix::Depth,
        crate::cli::Prefix::Indent => TreePrefix::Indent,
        crate::cli::Prefix::None => TreePrefix::None,
    };
    let tui_charset = args.charset.map_or_else(
        || {
            if cargo::core::Shell::new().out_unicode() {
                TreeCharset::Utf8
            } else {
                TreeCharset::Ascii
            }
        },
        |charset| match charset {
            crate::cli::Charset::Utf8 => TreeCharset::Utf8,
            crate::cli::Charset::Ascii => TreeCharset::Ascii,
        },
    );
    let resolution_options = ResolutionOptions {
        workspace: args.workspace,
        packages: args.package,
        exclude: args.exclude,
        edges: args.edges,
        no_dev_dependencies: args.no_dev_dependencies,
        prune: args.prune,
        invert: args.invert,
        duplicates: args.duplicates,
        depth,
        workspace_depth,
        targets: args.target,
        all_targets: args.all_targets,
        features: args.features,
        all_features: args.all_features,
        no_default_features: args.no_default_features,
    };
    let dependency_tree = Arc::new(TuiTree::new(
        cargo_tree_tui::core::DependencyTree::load_with_options_and_format(
            args.manifest_path,
            &resolution_options,
            format.requirements(),
        )?,
        no_dedupe,
    ));

    let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let worker_tree = Arc::clone(&dependency_tree);
    let worker_handle = thread::spawn(move || search_worker(worker_tree, search_rx, event_tx));

    let mut state = TuiState::new(
        dependency_tree,
        search_tx,
        depth,
        tui_prefix,
        tui_charset,
        format,
    );
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

fn resolve_prefix(
    prefix: Option<crate::cli::Prefix>,
    no_indent: bool,
    prefix_depth: bool,
) -> Result<crate::cli::Prefix> {
    if no_indent && prefix_depth {
        bail!("`--no-indent` conflicts with `--prefix-depth`");
    }

    let legacy_prefix = if no_indent {
        Some(crate::cli::Prefix::None)
    } else if prefix_depth {
        Some(crate::cli::Prefix::Depth)
    } else {
        None
    };

    if let (Some(prefix), Some(legacy_prefix)) = (prefix, legacy_prefix)
        && prefix != legacy_prefix
    {
        bail!("`--prefix` conflicts with its deprecated alias");
    }

    Ok(prefix
        .or(legacy_prefix)
        .unwrap_or(crate::cli::Prefix::Indent))
}

fn search_worker(
    dependency_tree: Arc<TuiTree>,
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
