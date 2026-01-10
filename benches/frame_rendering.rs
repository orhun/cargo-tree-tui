use cargo_tree_tui::ops::tree::tui::draw_tui;
use cargo_tree_tui::ops::tree::tui::state::TuiState;
use cargo_tree_tui::ops::tree::tui::widget::TreeWidgetState;

fn create_test_state() -> TuiState {
    TuiState::new(None).expect("Failed to create TUI state")
}

#[divan::bench]
fn render_single_frame() {
    let mut state = create_test_state();

    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal.draw(|frame| draw_tui(frame, &mut state)).unwrap();
}

#[divan::bench]
fn render_small_frame() {
    let mut state = create_test_state();

    let backend = ratatui::backend::TestBackend::new(20, 10);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal.draw(|frame| draw_tui(frame, &mut state)).unwrap();
}

#[divan::bench]
fn render_medium_frame() {
    let mut state = create_test_state();

    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal.draw(|frame| draw_tui(frame, &mut state)).unwrap();
}

#[divan::bench]
fn render_large_frame() {
    let mut state = create_test_state();

    let backend = ratatui::backend::TestBackend::new(200, 60);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal.draw(|frame| draw_tui(frame, &mut state)).unwrap();
}

#[divan::bench]
fn render_multiple_frames_setup() {
    let mut state = create_test_state();

    let backend = ratatui::backend::TestBackend::new(80, 24);
    let _terminal = ratatui::Terminal::new(backend).unwrap();

    divan::black_box(&mut state);
}

#[divan::bench]
fn create_widget_state() {
    divan::black_box(TreeWidgetState::default());
}
