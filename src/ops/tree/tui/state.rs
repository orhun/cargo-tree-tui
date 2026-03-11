use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::core::DependencyTree;

use super::widget::TreeWidgetState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
    SearchResults,
}

#[derive(Debug)]
pub struct TuiState {
    pub running: bool,
    pub dependency_tree: DependencyTree,
    pub tree_widget_state: TreeWidgetState,
    pub show_help: bool,
    pub input_mode: InputMode,
    pub search_query: String,
}

impl TuiState {
    pub fn new(manifest_path: Option<PathBuf>) -> Result<Self> {
        let dependency_tree = DependencyTree::load(manifest_path)?;
        let mut tree_widget_state = TreeWidgetState::default();
        tree_widget_state.expand_all(&dependency_tree);
        Ok(TuiState {
            running: true,
            dependency_tree,
            tree_widget_state,
            show_help: false,
            input_mode: InputMode::Normal,
            search_query: String::new(),
        })
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) {
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
                        self.update_search();
                    }
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.update_search();
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
                self.update_search();
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

    fn update_search(&mut self) {
        self.tree_widget_state
            .set_search_query(&self.dependency_tree, &self.search_query);
    }

    fn clear_search(&mut self) {
        self.input_mode = InputMode::Normal;
        self.search_query.clear();
        self.update_search();
    }
}
