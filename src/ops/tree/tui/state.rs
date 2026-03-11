use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::core::DependencyTree;

use super::widget::{SearchState, TreeWidgetState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
    SearchResults,
}

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    SearchResult(SearchResult),
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub generation: u64,
    pub query: String,
}

#[derive(Debug)]
pub struct SearchResult {
    pub generation: u64,
    pub query: String,
    pub search_state: SearchState,
}

#[derive(Debug)]
pub struct TuiState {
    pub running: bool,
    pub dependency_tree: DependencyTree,
    pub tree_widget_state: TreeWidgetState,
    pub show_help: bool,
    pub input_mode: InputMode,
    pub search_query: String,
    pub search_running: bool,
    spinner_frame: usize,
    search_generation: u64,
    search_tx: Sender<SearchRequest>,
}

impl TuiState {
    pub fn new(dependency_tree: DependencyTree, search_tx: Sender<SearchRequest>) -> Self {
        let mut tree_widget_state = TreeWidgetState::default();
        tree_widget_state.expand_all(&dependency_tree);
        TuiState {
            running: true,
            dependency_tree,
            tree_widget_state,
            show_help: false,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            search_running: false,
            spinner_frame: 0,
            search_generation: 0,
            search_tx,
        }
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key_event) => self.handle_key_event(key_event),
            Event::SearchResult(search_result) => self.handle_search_result(search_result),
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
            .apply_search_state(search_result.search_state);
    }

    fn request_search(&mut self) {
        self.search_generation += 1;
        let request = SearchRequest {
            generation: self.search_generation,
            query: self.search_query.clone(),
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
