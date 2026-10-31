use std::{sync::Arc, sync::mpsc, thread, time::Duration};

use anyhow::{Result, bail};
use crossterm::event::{self, Event as CrosstermEvent};

use cargo_tree_tui::{
    core::ResolutionOptions,
    ops::tree::tui::TuiTree,
    ops::tree::tui::{
        draw_tui,
        state::{
            Event, SearchRequest, SearchResult, TreeOptions, TreeResolutionRequest,
            TreeResolutionResult, TuiChannels, TuiState,
        },
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
    let tree_options = TreeOptions {
        manifest_path: args.manifest_path,
        resolution: ResolutionOptions {
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
        },
        no_dedupe,
    };
    let dependency_tree = Arc::new(TuiTree::new(
        cargo_tree_tui::core::DependencyTree::load_with_options_and_format(
            tree_options.manifest_path.clone(),
            &tree_options.resolution,
            format.requirements(),
        )?,
        tree_options.no_dedupe,
    ));

    let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let search_event_tx = event_tx.clone();
    let worker_handle = thread::spawn(move || search_worker(search_rx, search_event_tx));
    let (resolution_tx, resolution_rx) = mpsc::channel::<TreeResolutionRequest>();
    let (resolution_resume_tx, resolution_resume_rx) = mpsc::channel();
    let resolution_event_tx = event_tx.clone();
    let resolution_handle = thread::spawn(move || {
        resolution_worker(resolution_rx, resolution_resume_rx, resolution_event_tx)
    });

    let mut state = TuiState::new(
        dependency_tree,
        TuiChannels {
            search: search_tx,
            resolution: resolution_tx,
            resolution_resume: resolution_resume_tx,
        },
        depth,
        tui_prefix,
        tui_charset,
        format,
        tree_options,
    );
    let mut terminal = Some(ratatui::init());

    while state.running {
        if let Some(terminal) = &mut terminal {
            terminal.draw(|frame| draw_tui(frame, &mut state))?;
        }

        while let Ok(event) = event_rx.try_recv() {
            let resolution_finished = matches!(&event, Event::TreeResolved(_));
            state.handle_event(event);
            if resolution_finished && terminal.is_none() {
                terminal = Some(ratatui::init());
            }
        }

        if terminal.is_none() {
            if let Ok(event) = event_rx.recv_timeout(Duration::from_millis(16)) {
                let resolution_finished = matches!(&event, Event::TreeResolved(_));
                state.handle_event(event);
                if resolution_finished {
                    terminal = Some(ratatui::init());
                }
            }
            continue;
        }

        if event::poll(Duration::from_millis(16))?
            && let CrosstermEvent::Key(key_event) = event::read()?
        {
            state.handle_event(Event::Key(key_event));
            if state.resolution_pending() && terminal.is_some() {
                terminal = None;
                ratatui::restore();
                state.resume_resolution();
            }
        }
    }

    drop(state);
    if terminal.take().is_some() {
        ratatui::restore();
    }
    let _ = worker_handle.join();
    drop(resolution_handle);
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

fn search_worker(search_rx: mpsc::Receiver<SearchRequest>, event_tx: mpsc::Sender<Event>) {
    while let Ok(mut request) = search_rx.recv() {
        while let Ok(next_request) = search_rx.try_recv() {
            request = next_request;
        }

        let search_state = TreeWidgetState::search(&request.dependency_tree, &request.query);
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

fn resolution_worker(
    resolution_rx: mpsc::Receiver<TreeResolutionRequest>,
    resolution_resume_rx: mpsc::Receiver<()>,
    event_tx: mpsc::Sender<Event>,
) {
    while let Ok(mut request) = resolution_rx.recv() {
        while let Ok(next_request) = resolution_rx.try_recv() {
            request = next_request;
        }

        if resolution_resume_rx.recv().is_err() {
            break;
        }

        let TreeResolutionRequest {
            generation,
            options,
            format_requirements,
        } = request;
        let result = cargo_tree_tui::core::DependencyTree::load_with_options_and_format(
            options.manifest_path.clone(),
            &options.resolution,
            format_requirements,
        )
        .map(|tree| Arc::new(TuiTree::new(tree, options.no_dedupe)))
        .map_err(|error| format!("{error:#}"));
        if event_tx
            .send(Event::TreeResolved(TreeResolutionResult {
                generation,
                options,
                result,
            }))
            .is_err()
        {
            break;
        }
    }
}
