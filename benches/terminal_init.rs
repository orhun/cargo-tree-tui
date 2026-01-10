use divan::Bencher;

#[divan::bench(ignore = std::env::var("BENCH_TERMINAL").is_err())]
fn terminal_init() {
    let _terminal = ratatui::init();
    ratatui::restore();
}

#[divan::bench(ignore = std::env::var("BENCH_TERMINAL").is_err())]
fn terminal_init_multiple(bencher: Bencher) {
    bencher.bench(|| {
        let _terminal = ratatui::init();
        divan::black_box(());
        ratatui::restore();
    });
}
