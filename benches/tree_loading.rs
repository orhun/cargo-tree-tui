//! Benchmarks for loading the dependency tree.
//!
//!  FIXME:: These benchmarks benchmark a throwaway
//! type. When the crate is moved into `cargo`,
//! This will have to be replaced.
use cargo_tree_tui::core::DependencyTree;

#[divan::bench]
fn load_current_project() {
    divan::black_box(DependencyTree::load(None).unwrap());
}

#[divan::bench]
fn load_current_project_multiple_times(bencher: divan::Bencher) {
    bencher.bench(|| {
        divan::black_box(DependencyTree::load(None).unwrap());
    });
}

#[divan::bench]
fn load_manifest_path() {
    let manifest_path = Some(std::path::PathBuf::from("./Cargo.toml"));
    divan::black_box(DependencyTree::load(manifest_path).unwrap());
}

#[divan::bench(min_time = 2)]
fn load_with_warmup() {
    let _ = DependencyTree::load(None);
    divan::black_box(DependencyTree::load(None).unwrap());
}
