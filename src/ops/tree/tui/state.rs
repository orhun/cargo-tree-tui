use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::core::DependencyTree;

use super::widget_state::TreeWidgetState;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum PendingKey {
    #[default]
    None,
    /// Waiting for second 'g' to execute gg (go to top)
    G,
}

#[derive(Debug)]
pub struct TuiState {
    pub running: bool,
    pub dependency_tree: DependencyTree,
    pub tree_widget_state: TreeWidgetState,
    pub show_help: bool,
    pending_key: PendingKey,
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
            show_help: false,
            pending_key: PendingKey::None,
        })
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) {
        // If help panel is open, close it on any key except ?
        if self.show_help {
            match key_event.code {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.show_help = false;
                }
                _ => {
                    self.show_help = false;
                }
            }
            return;
        }

        // Handle pending key sequences (like 'gg')
        if self.pending_key == PendingKey::G {
            self.pending_key = PendingKey::None;
            if key_event.code == KeyCode::Char('g') {
                self.tree_widget_state.select_first(&self.dependency_tree);
                return;
            }
            // If not 'g', fall through to handle the key normally
        }

        match key_event.code {
            // === Help ===
            KeyCode::Char('?') => {
                self.show_help = true;
            }

            // === Quit ===
            KeyCode::Char('q') | KeyCode::Esc => {
                self.running = false;
            }

            // === Basic Navigation (Arrow Keys) ===
            KeyCode::Down => {
                self.tree_widget_state.select_next(&self.dependency_tree);
            }
            KeyCode::Up => {
                self.tree_widget_state
                    .select_previous(&self.dependency_tree);
            }
            KeyCode::Right => {
                self.tree_widget_state.expand(&self.dependency_tree);
            }
            KeyCode::Left => {
                self.tree_widget_state.collapse(&self.dependency_tree);
            }

            // === Vim-Style Navigation ===
            KeyCode::Char('j') => {
                self.tree_widget_state.select_next(&self.dependency_tree);
            }
            KeyCode::Char('k') => {
                self.tree_widget_state
                    .select_previous(&self.dependency_tree);
            }
            KeyCode::Char('l') => {
                self.tree_widget_state.expand(&self.dependency_tree);
            }
            KeyCode::Char('h') => {
                self.tree_widget_state.collapse(&self.dependency_tree);
            }

            // === Jump to Top/Bottom ===
            KeyCode::Char('g') => {
                self.pending_key = PendingKey::G;
            }
            KeyCode::Char('G') => {
                self.tree_widget_state.select_last(&self.dependency_tree);
            }
            KeyCode::Home => {
                self.tree_widget_state.select_first(&self.dependency_tree);
            }
            KeyCode::End => {
                self.tree_widget_state.select_last(&self.dependency_tree);
            }

            // === Page Navigation ===
            KeyCode::Char('d') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.tree_widget_state
                    .select_half_page_down(&self.dependency_tree);
            }
            KeyCode::Char('u') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.tree_widget_state
                    .select_half_page_up(&self.dependency_tree);
            }
            KeyCode::PageDown => {
                self.tree_widget_state
                    .select_page_down(&self.dependency_tree);
            }
            KeyCode::PageUp => {
                self.tree_widget_state.select_page_up(&self.dependency_tree);
            }

            // === Tree-Specific Navigation ===
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

            // === Expand/Collapse Controls ===
            KeyCode::Char('o') => {
                self.tree_widget_state.toggle(&self.dependency_tree);
            }
            KeyCode::Char('O') => {
                self.tree_widget_state.expand_all(&self.dependency_tree);
            }
            KeyCode::Char('c') => {
                self.tree_widget_state.collapse_all(&self.dependency_tree);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.tree_widget_state.toggle(&self.dependency_tree);
            }

            // === Expand to Depth (1-9) ===
            KeyCode::Char(n) if n.is_ascii_digit() && n != '0' => {
                let depth = n.to_digit(10).unwrap() as usize;
                self.tree_widget_state
                    .open_to_depth(&self.dependency_tree, depth);
            }

            _ => {}
        }
    }
}
