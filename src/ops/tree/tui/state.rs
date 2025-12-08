use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::core::DependencyTree;

use super::widget_state::TreeWidgetState;

#[derive(Debug)]
pub struct TuiState {
    pub running: bool,
    pub dependency_tree: DependencyTree,
    pub tree_widget_state: TreeWidgetState,
    pub show_help: bool,
    pub search_query: Option<String>,
}

impl TuiState {
    pub fn new(dependency_tree: DependencyTree) -> Result<Self> {
        let mut tree_widget_state = TreeWidgetState::default();
        tree_widget_state.expand_all(&dependency_tree);
        Ok(TuiState {
            running: true,
            dependency_tree,
            tree_widget_state,
            show_help: false,
            search_query: None,
        })
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) {
        if self.search_query.is_some() {
            match key_event.code {
                KeyCode::Esc => {
                    self.search_query = None;
                }
                KeyCode::Backspace => {
                    if let Some(query) = &mut self.search_query {
                        if query.pop().is_some() {
                            self.update_search();
                        } else {
                            self.search_query = None;
                        }
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(query) = &mut self.search_query {
                        query.push(c);
                        self.update_search();
                    }
                }
                _ => {}
            }
            return;
        }

        match (key_event.code, key_event.modifiers) {
            (KeyCode::Char('q'), _) => {
                self.running = false;
            }
            (KeyCode::Char('?'), _) => {
                self.show_help = !self.show_help;
            }
            (KeyCode::Char('/'), _) => {
                self.search_query = Some(String::new());
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
        if let Some(query) = &self.search_query {
            self.tree_widget_state.search(&self.dependency_tree, query);
        }
    }
}
