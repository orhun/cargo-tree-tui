use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::core::DependencyTree;

use super::widget_state::TreeWidgetState;

#[derive(Debug)]
pub struct TuiState {
    pub running: bool,
    pub dependency_tree: DependencyTree,
    pub tree_widget_state: TreeWidgetState,
}

impl TuiState {
    pub fn new(manifest_path: Option<PathBuf>) -> Result<Self> {
        let dependency_tree = DependencyTree::load(manifest_path)?;
        let mut tree_widget_state = TreeWidgetState::default();
        tree_widget_state.open_to_depth(&dependency_tree, 3);
        Ok(TuiState {
            running: true,
            dependency_tree,
            tree_widget_state,
        })
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => {
                self.running = false;
            }
            KeyCode::Down => {
                self.tree_widget_state.select_next(&self.dependency_tree);
            }
            KeyCode::Up => {
                self.tree_widget_state
                    .select_previous(&self.dependency_tree);
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
