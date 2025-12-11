use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::core::DependencyTree;

use super::widget::TreeWidgetState;

#[derive(Debug)]
pub struct TuiState {
    pub running: bool,
    pub dependency_tree: DependencyTree,
    pub tree_widget_state: TreeWidgetState,
    pub show_help: bool,
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
        })
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) {
        if self.show_help {
            // Close help popup on any key press
            self.show_help = false;
        }

        match key_event.code {
            KeyCode::Char('q') => {
                self.running = false;
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
            }
            KeyCode::Char('p') => {
                self.tree_widget_state.select_parent(&self.dependency_tree);
            }
            KeyCode::Char(']') => {
                self.tree_widget_state
                    .select_next_sibling(&self.dependency_tree);
            }
            KeyCode::Char('[') => {
                self.tree_widget_state
                    .select_previous_sibling(&self.dependency_tree);
            }
            KeyCode::Char('/') => {
                // Start live search. Query starts empty and user types to filter.
                self.tree_widget_state.set_search_query(&self.dependency_tree, String::new());
            }
            KeyCode::Char('c') => {
                // Clear persisted highlights
                if self.tree_widget_state.search_persist {
                    self.tree_widget_state.cancel_search(false);
                }
            }
            KeyCode::Char(ch) => {
                if self.tree_widget_state.search_active {
                    let mut q = self.tree_widget_state.search_query.clone();
                    q.push(ch);
                    self.tree_widget_state.set_search_query(&self.dependency_tree, q);
                }
            }
            KeyCode::Backspace => {
                if self.tree_widget_state.search_active {
                    let mut q = self.tree_widget_state.search_query.clone();
                    q.pop();
                    self.tree_widget_state.set_search_query(&self.dependency_tree, q);
                }
            }
            KeyCode::Enter => {
                if self.tree_widget_state.search_active {
                    self.tree_widget_state.commit_search();
                }
            }
            KeyCode::Esc => {
                if self.tree_widget_state.search_active {
                    // Cancel and restore previous selection
                    self.tree_widget_state.cancel_search(true);
                } else if self.tree_widget_state.search_persist {
                    // Clear persisted highlights without restoring
                    self.tree_widget_state.cancel_search(false);
                }
            }
            KeyCode::Down => {
                if self.tree_widget_state.search_active || self.tree_widget_state.search_persist {
                    self.tree_widget_state.next_match();
                } else {
                    self.tree_widget_state.select_next(&self.dependency_tree);
                }
            }
            KeyCode::Up => {
                if self.tree_widget_state.search_active || self.tree_widget_state.search_persist {
                    self.tree_widget_state.previous_match();
                } else {
                    self.tree_widget_state.select_previous(&self.dependency_tree);
                }
            }
            KeyCode::PageDown => {
                self.tree_widget_state.page_down(&self.dependency_tree);
            }
            KeyCode::PageUp => {
                self.tree_widget_state.page_up(&self.dependency_tree);
            }
            KeyCode::Right => {
                self.tree_widget_state.expand(&self.dependency_tree);
            }
            KeyCode::Left => {
                self.tree_widget_state.collapse(&self.dependency_tree);
            }
            _ => {}
        }
    }
}
