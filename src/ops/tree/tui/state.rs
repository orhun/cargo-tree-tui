use std::path::PathBuf;
use std::sync::{Arc, OnceLock, mpsc::Sender};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use super::TuiTree;
use super::selector::{SelectorOption, SelectorState};
use crate::core::dependency::DependencyType;
use crate::core::{PackageFormatRequirements, ResolutionOptions};

use super::widget::{SearchState, TreeCharset, TreeFormat, TreePrefix, TreeWidgetState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
    SearchResults,
    Selector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectorKind {
    EdgeKinds,
    Targets,
}

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    SearchResult(SearchResult),
    TreeResolved(TreeResolutionResult),
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub generation: u64,
    pub query: String,
    pub dependency_tree: Arc<TuiTree>,
}

#[derive(Debug)]
pub struct SearchResult {
    pub generation: u64,
    pub query: String,
    pub search_state: SearchState,
}

#[derive(Debug, Clone)]
pub struct TreeResolutionRequest {
    pub generation: u64,
    pub options: TreeOptions,
    pub format_requirements: PackageFormatRequirements,
}

#[derive(Debug)]
pub struct TreeResolutionResult {
    pub generation: u64,
    pub options: TreeOptions,
    pub result: Result<Arc<TuiTree>, String>,
}

/// CLI options used to resolve and prepare the tree shown in the TUI.
#[derive(Debug, Clone)]
pub struct TreeOptions {
    pub manifest_path: Option<PathBuf>,
    pub resolution: ResolutionOptions,
    pub no_dedupe: bool,
}

pub struct TuiChannels {
    pub search: Sender<SearchRequest>,
    pub resolution: Sender<TreeResolutionRequest>,
}

#[derive(Debug)]
pub struct TuiState {
    pub running: bool,
    pub dependency_tree: Arc<TuiTree>,
    pub tree_options: TreeOptions,
    pub selector: Option<SelectorState>,
    selector_kind: Option<SelectorKind>,
    pub prefix: TreePrefix,
    pub charset: TreeCharset,
    pub format: TreeFormat,
    pub tree_widget_state: TreeWidgetState,
    pub show_help: bool,
    pub input_mode: InputMode,
    pub search_query: String,
    pub search_running: bool,
    spinner_frame: usize,
    search_generation: u64,
    search_tx: Sender<SearchRequest>,
    resolution_generation: u64,
    resolution_tx: Sender<TreeResolutionRequest>,
}

impl TuiState {
    pub fn new(
        dependency_tree: Arc<TuiTree>,
        channels: TuiChannels,
        depth: Option<usize>,
        prefix: TreePrefix,
        charset: TreeCharset,
        format: TreeFormat,
        tree_options: TreeOptions,
    ) -> Self {
        let mut tree_widget_state = TreeWidgetState::default();
        if let Some(depth) = depth {
            tree_widget_state.open_to_dependency_depth(&dependency_tree, depth);
        } else {
            tree_widget_state.expand_all(&dependency_tree);
        }
        TuiState {
            running: true,
            dependency_tree,
            tree_options,
            selector: None,
            selector_kind: None,
            prefix,
            charset,
            format,
            tree_widget_state,
            show_help: false,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            search_running: false,
            spinner_frame: 0,
            search_generation: 0,
            search_tx: channels.search,
            resolution_generation: 0,
            resolution_tx: channels.resolution,
        }
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key_event) => self.handle_key_event(key_event),
            Event::SearchResult(search_result) => self.handle_search_result(search_result),
            Event::TreeResolved(result) => self.handle_tree_resolved(result),
        }
    }

    pub fn advance_spinner(&mut self) {
        if self.search_running {
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
        }
    }

    pub fn search_prompt_symbol(&self) -> char {
        const FRAMES: [char; 4] = ['|', '/', '-', '\\'];
        if self.search_running {
            FRAMES[self.spinner_frame % FRAMES.len()]
        } else {
            '/'
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if self.show_help {
            // Close help popup on any key press
            self.show_help = false;
        }
        if key_event.kind != KeyEventKind::Press && key_event.modifiers.is_empty() {
            return;
        }

        if self.input_mode == InputMode::Selector {
            self.handle_selector_key(key_event.code);
            return;
        }

        if self.input_mode == InputMode::Search {
            match key_event.code {
                KeyCode::Esc => {
                    self.clear_search();
                }
                KeyCode::Enter => {
                    if self.search_query.is_empty() {
                        self.clear_search();
                    } else {
                        self.input_mode = InputMode::SearchResults;
                    }
                }
                KeyCode::Backspace => {
                    if self.search_query.pop().is_none() {
                        self.clear_search();
                    } else {
                        self.request_search();
                    }
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.request_search();
                }
                _ => {}
            }
            return;
        }

        match (key_event.code, key_event.modifiers) {
            (KeyCode::Esc, _) if self.input_mode == InputMode::SearchResults => {
                self.clear_search();
            }
            (KeyCode::Char('q'), _) => {
                self.running = false;
            }
            (KeyCode::Char('?'), _) => {
                self.show_help = !self.show_help;
            }
            (KeyCode::Char('/'), _) => {
                self.input_mode = InputMode::Search;
            }
            (KeyCode::Char('e'), _) => {
                self.open_edge_selector();
            }
            (KeyCode::Char('t'), _) => {
                self.open_target_selector();
            }
            (KeyCode::Char('p'), _) => {
                self.tree_widget_state.select_parent(&self.dependency_tree);
            }
            (KeyCode::Char(']'), _) => {
                self.tree_widget_state
                    .select_next_sibling(&self.dependency_tree);
            }
            (KeyCode::Char('['), _) => {
                self.tree_widget_state
                    .select_previous_sibling(&self.dependency_tree);
            }
            (KeyCode::Down, _) => {
                self.tree_widget_state.select_next(&self.dependency_tree);
            }
            (KeyCode::Up, _) => {
                self.tree_widget_state
                    .select_previous(&self.dependency_tree);
            }
            (KeyCode::PageDown, _) => {
                self.tree_widget_state.page_down(&self.dependency_tree);
            }
            (KeyCode::PageUp, _) => {
                self.tree_widget_state.page_up(&self.dependency_tree);
            }
            (KeyCode::Char(' '), _) => {
                self.tree_widget_state.toggle(&self.dependency_tree);
            }
            (KeyCode::Right, _) => {
                self.tree_widget_state.expand(&self.dependency_tree);
            }
            (KeyCode::Left, _) => {
                self.tree_widget_state.collapse(&self.dependency_tree);
            }
            _ => {}
        }
    }

    fn handle_search_result(&mut self, search_result: SearchResult) {
        if search_result.generation != self.search_generation
            || search_result.query != self.search_query
        {
            return;
        }

        self.search_running = false;
        self.tree_widget_state
            .apply_search_state(&self.dependency_tree, search_result.search_state);
    }

    fn open_edge_selector(&mut self) {
        let selected = edge_kinds(&self.tree_options.resolution);
        self.selector = Some(SelectorState::new(
            "Dependency edge kinds",
            vec![
                SelectorOption::new(edge_flag(EDGE_KINDS[0]), "Normal dependencies", selected[0]),
                SelectorOption::new(edge_flag(EDGE_KINDS[1]), "Build dependencies", selected[1]),
                SelectorOption::new(
                    edge_flag(EDGE_KINDS[2]),
                    "Development dependencies",
                    selected[2],
                ),
                SelectorOption::new(
                    "features",
                    "Feature edges",
                    feature_edges(&self.tree_options.resolution),
                ),
            ],
        ));
        self.selector_kind = Some(SelectorKind::EdgeKinds);
        self.input_mode = InputMode::Selector;
    }

    fn open_target_selector(&mut self) {
        let current_targets = target_selection(&self.tree_options.resolution);
        let mut candidates = target_choices().to_vec();
        for target in &self.tree_options.resolution.targets {
            if target != "all" && !candidates.contains(target) {
                candidates.push(target.clone());
            }
        }
        let mut options = Vec::with_capacity(candidates.len() + 1);
        options.push(
            SelectorOption::new(
                DEFAULT_TARGET,
                "Default target",
                current_targets
                    .iter()
                    .any(|target| target == DEFAULT_TARGET),
            )
            .exclusive(),
        );
        options.extend(candidates.iter().map(|value| {
            let option = SelectorOption::new(
                value.clone(),
                value.clone(),
                current_targets.iter().any(|target| target == value),
            );
            if value == "all" {
                option.exclusive()
            } else {
                option
            }
        }));

        self.selector = Some(
            SelectorState::new("Target triples", options)
                .filterable()
                .max_height(20),
        );
        self.selector_kind = Some(SelectorKind::Targets);
        self.input_mode = InputMode::Selector;
    }

    fn handle_selector_key(&mut self, key: KeyCode) {
        let Some(selector) = self.selector.as_mut() else {
            self.input_mode = InputMode::Normal;
            return;
        };
        if selector.applying {
            if key == KeyCode::Char('q') {
                self.running = false;
            }
            return;
        }

        match key {
            KeyCode::Esc => {
                self.selector = None;
                self.selector_kind = None;
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Up => selector.move_up(),
            KeyCode::Down => selector.move_down(),
            KeyCode::Char(' ') => selector.toggle_selected(),
            KeyCode::Backspace => selector.pop_filter_char(),
            KeyCode::Char(ch) => selector.push_filter_char(ch),
            KeyCode::Enter => self.apply_selector_selection(),
            _ => {}
        }
    }

    fn apply_selector_selection(&mut self) {
        match self.selector_kind {
            Some(SelectorKind::EdgeKinds) => self.apply_edge_selection(),
            Some(SelectorKind::Targets) => self.apply_target_selection(),
            None => {}
        }
    }

    fn apply_edge_selection(&mut self) {
        let Some(selector) = self.selector.as_ref() else {
            return;
        };
        let selected_values = selector.checked_values();
        let selected =
            EDGE_KINDS.map(|kind| selected_values.iter().any(|value| value == edge_flag(kind)));
        let features = selected_values.iter().any(|value| value == "features");
        if selected == edge_kinds(&self.tree_options.resolution)
            && features == feature_edges(&self.tree_options.resolution)
        {
            self.selector = None;
            self.selector_kind = None;
            self.input_mode = InputMode::Normal;
            return;
        }

        if features && self.tree_options.resolution.duplicates {
            if let Some(selector) = self.selector.as_mut() {
                selector.message =
                    Some("Feature edges cannot be combined with --duplicates".to_owned());
            }
            return;
        }

        let mut options = self.tree_options.clone();
        set_edge_options(&mut options.resolution, selected, features);
        self.resolution_generation = self.resolution_generation.wrapping_add(1);
        let request = TreeResolutionRequest {
            generation: self.resolution_generation,
            format_requirements: self.format.requirements(),
            options,
        };

        if self.resolution_tx.send(request).is_ok() {
            if let Some(selector) = self.selector.as_mut() {
                selector.applying = true;
                selector.message = None;
            }
        } else if let Some(selector) = self.selector.as_mut() {
            selector.message = Some("Could not start dependency resolution".to_owned());
        }
    }

    fn apply_target_selection(&mut self) {
        let Some(selector) = self.selector.as_ref() else {
            return;
        };
        let selected = selector.checked_values();
        let selected_targets = if selected.iter().any(|target| target == "all") {
            vec!["all".to_owned()]
        } else if selected.is_empty() || selected.iter().all(|target| target == DEFAULT_TARGET) {
            vec![DEFAULT_TARGET.to_owned()]
        } else {
            selected
                .into_iter()
                .filter(|target| target != DEFAULT_TARGET)
                .collect()
        };
        if selected_targets == target_selection(&self.tree_options.resolution) {
            self.selector = None;
            self.selector_kind = None;
            self.input_mode = InputMode::Normal;
            return;
        }

        let mut options = self.tree_options.clone();
        options.resolution.targets = if selected_targets == [DEFAULT_TARGET] {
            Vec::new()
        } else {
            selected_targets
        };
        options.resolution.all_targets = false;
        self.resolution_generation = self.resolution_generation.wrapping_add(1);
        let request = TreeResolutionRequest {
            generation: self.resolution_generation,
            format_requirements: self.format.requirements(),
            options,
        };

        if self.resolution_tx.send(request).is_ok() {
            if let Some(selector) = self.selector.as_mut() {
                selector.applying = true;
                selector.message = None;
            }
        } else if let Some(selector) = self.selector.as_mut() {
            selector.message = Some("Could not start dependency resolution".to_owned());
        }
    }

    fn handle_tree_resolved(&mut self, result: TreeResolutionResult) {
        if result.generation != self.resolution_generation {
            return;
        }

        match result.result {
            Ok(dependency_tree) => {
                self.search_generation = self.search_generation.wrapping_add(1);
                self.search_query.clear();
                self.search_running = false;
                self.tree_options = result.options;
                self.dependency_tree = dependency_tree;
                self.tree_widget_state = TreeWidgetState::default();
                if let Some(depth) = self.tree_options.resolution.depth {
                    self.tree_widget_state
                        .open_to_dependency_depth(&self.dependency_tree, depth);
                } else {
                    self.tree_widget_state.expand_all(&self.dependency_tree);
                }
                self.selector = None;
                self.selector_kind = None;
                self.input_mode = InputMode::Normal;
            }
            Err(error) => {
                if let Some(selector) = self.selector.as_mut() {
                    selector.applying = false;
                    selector.message = Some(error);
                }
            }
        }
    }

    fn request_search(&mut self) {
        self.search_generation += 1;
        let request = SearchRequest {
            generation: self.search_generation,
            query: self.search_query.clone(),
            dependency_tree: Arc::clone(&self.dependency_tree),
        };

        if request.query.is_empty() {
            self.search_running = false;
            self.tree_widget_state.clear_search();
            return;
        }

        self.search_running = true;
        let _ = self.search_tx.send(request);
    }

    fn clear_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.search_generation += 1;
        self.search_query.clear();
        self.search_running = false;
        self.tree_widget_state.clear_search();
    }
}

const EDGE_KINDS: [DependencyType; 3] = [
    DependencyType::Normal,
    DependencyType::Build,
    DependencyType::Dev,
];

const DEFAULT_TARGET: &str = "__cargo_tree_tui_default_target";

fn target_choices() -> &'static [String] {
    static TARGET_CHOICES: OnceLock<Vec<String>> = OnceLock::new();
    TARGET_CHOICES
        .get_or_init(|| {
            cargo::util::command_prelude::get_target_triples_with_all()
                .into_iter()
                .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
                .collect()
        })
        .as_slice()
}

fn target_selection(options: &ResolutionOptions) -> Vec<String> {
    if options.all_targets || options.targets.iter().any(|target| target == "all") {
        vec!["all".to_owned()]
    } else if options.targets.is_empty() {
        vec![DEFAULT_TARGET.to_owned()]
    } else {
        options.targets.clone()
    }
}

fn edge_flag(kind: DependencyType) -> &'static str {
    match kind {
        DependencyType::Normal => "normal",
        DependencyType::Build => "build",
        DependencyType::Dev => "dev",
    }
}

fn excluded_edge_flag(kind: DependencyType) -> &'static str {
    match kind {
        DependencyType::Normal => "no-normal",
        DependencyType::Build => "no-build",
        DependencyType::Dev => "no-dev",
    }
}

fn edge_kinds(options: &ResolutionOptions) -> [bool; 3] {
    let edges = &options.edges;
    let has_all = edges.iter().any(|kind| kind == "all");
    let has_exclusions = EDGE_KINDS
        .iter()
        .any(|kind| edges.iter().any(|edge| edge == excluded_edge_flag(*kind)));
    let has_inclusions = EDGE_KINDS
        .iter()
        .any(|kind| edges.iter().any(|edge| edge == edge_flag(*kind)));

    let mut selected = if has_all || (!has_exclusions && !has_inclusions) {
        [true; 3]
    } else if has_exclusions {
        std::array::from_fn(|index| {
            !edges
                .iter()
                .any(|edge| edge == excluded_edge_flag(EDGE_KINDS[index]))
        })
    } else {
        std::array::from_fn(|index| {
            edges
                .iter()
                .any(|edge| edge == edge_flag(EDGE_KINDS[index]))
        })
    };

    if options.no_dev_dependencies {
        selected[2] = false;
    }
    selected
}

fn feature_edges(options: &ResolutionOptions) -> bool {
    options
        .edges
        .iter()
        .any(|kind| matches!(kind.as_str(), "features" | "all"))
}

fn set_edge_options(options: &mut ResolutionOptions, selected: [bool; 3], features: bool) {
    options.edges.retain(|kind| {
        !matches!(kind.as_str(), "all" | "features")
            && !EDGE_KINDS.iter().any(|edge| kind == edge_flag(*edge))
            && !EDGE_KINDS
                .iter()
                .any(|edge| kind == excluded_edge_flag(*edge))
    });
    options.no_dev_dependencies = false;

    if selected == [true; 3] && features {
        options.edges.push("all".to_owned());
        return;
    }
    if features {
        options.edges.push("features".to_owned());
    }
    if selected == [true; 3] {
        options.edges.extend(
            EDGE_KINDS
                .into_iter()
                .map(|kind| edge_flag(kind).to_owned()),
        );
        return;
    }

    let use_exclusions = selected == [false; 3];
    options.edges.extend(
        EDGE_KINDS
            .into_iter()
            .zip(selected)
            .filter_map(|(kind, is_selected)| {
                let include_kind = if use_exclusions {
                    !is_selected
                } else {
                    is_selected
                };
                include_kind.then(|| {
                    if use_exclusions {
                        excluded_edge_flag(kind)
                    } else {
                        edge_flag(kind)
                    }
                    .to_owned()
                })
            }),
    );
}
