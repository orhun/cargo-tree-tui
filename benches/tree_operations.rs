use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;
use ratatui::{Terminal, backend::TestBackend};

use cargo_tree_tui::core::{Dependency, DependencyNode, DependencyTree, NodeId};
use cargo_tree_tui::ops::tree::tui::widget::render::RenderContext;
use cargo_tree_tui::ops::tree::tui::widget::{TreeWidget, TreeWidgetState, TreeWidgetStyle};

/// Tree size configurations: (label, breadth, depth).
const TREE_SIZES: &[(&str, usize, usize)] = &[
    ("medium (780 nodes)", 5, 4),
    ("large (5460 nodes)", 4, 6),
    ("xlarge (55986 nodes)", 6, 6),
];

/// Builds a synthetic dependency tree with `breadth` children per node and `depth` levels.
fn build_synthetic_tree(breadth: usize, depth: usize) -> DependencyTree {
    let mut nodes = Vec::new();
    let mut roots = Vec::new();

    // Build root nodes
    for _ in 0..breadth {
        let id = NodeId(nodes.len());
        roots.push(id);
        nodes.push(DependencyNode::Crate(Dependency {
            name: format!("crate-{}", id.0).into(),
            version: "0.1.0".into(),
            manifest_dir: None,
            is_proc_macro: false,
            children: Vec::new(),
        }));
    }

    // Build children level by level
    let mut parent_range = 0..roots.len();
    for _level in 1..depth {
        let new_start = nodes.len();
        for parent_idx in parent_range.clone() {
            let mut child_ids = Vec::new();
            for _ in 0..breadth {
                let child_id = NodeId(nodes.len());
                child_ids.push(child_id);
                nodes.push(DependencyNode::Crate(Dependency {
                    name: format!("crate-{}", child_id.0).into(),
                    version: "0.1.0".into(),
                    manifest_dir: None,
                    is_proc_macro: false,
                    children: Vec::new(),
                }));
            }
            if let DependencyNode::Crate(ref mut dep) = nodes[parent_idx] {
                dep.children = child_ids;
            }
        }
        parent_range = new_start..nodes.len();
    }

    // Build parents vec: for each node, collect its parent(s)
    let mut parents = vec![Vec::new(); nodes.len()];
    for (idx, node) in nodes.iter().enumerate() {
        let children = match node {
            DependencyNode::Crate(dep) => &dep.children,
            DependencyNode::Group(grp) => &grp.children,
        };
        for &child in children {
            parents[child.0].push(NodeId(idx));
        }
    }

    let crate_nodes = (0..nodes.len()).map(NodeId).collect();

    DependencyTree {
        workspace_name: "bench-workspace".into(),
        crate_nodes,
        nodes,
        parents,
        roots,
    }
}

fn bench_expand_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("expand_all");

    for &(label, breadth, depth) in TREE_SIZES {
        let tree = build_synthetic_tree(breadth, depth);
        group.bench_with_input(BenchmarkId::new("expand_all", label), &tree, |b, tree| {
            b.iter(|| {
                let mut state = TreeWidgetState::default();
                state.expand_all(tree);
            });
        });
    }

    group.finish();
}

fn bench_visible_nodes(c: &mut Criterion) {
    let mut group = c.benchmark_group("visible_nodes");

    for &(label, breadth, depth) in TREE_SIZES {
        let tree = build_synthetic_tree(breadth, depth);
        group.bench_with_input(BenchmarkId::new("rebuild", label), &tree, |b, tree| {
            b.iter(|| {
                let mut state = TreeWidgetState::default();
                state.expand_all(tree);
                state.visible_nodes(tree);
            });
        });
    }

    group.finish();
}

fn bench_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search");

    for &(label, breadth, depth) in TREE_SIZES {
        let tree = build_synthetic_tree(breadth, depth);

        // Search for a substring that matches ~10% of nodes
        group.bench_with_input(BenchmarkId::new("compute", label), &tree, |b, tree| {
            b.iter(|| TreeWidgetState::search(tree, "crate-1"));
        });

        // Apply search state to widget
        group.bench_with_input(BenchmarkId::new("apply", label), &tree, |b, tree| {
            let search_state = TreeWidgetState::search(tree, "crate-1");
            b.iter(|| {
                let mut state = TreeWidgetState::default();
                state.expand_all(tree);
                state.apply_search_state(tree, search_state.clone());
            });
        });
    }

    group.finish();
}

fn bench_render_context(c: &mut Criterion) {
    let mut group = c.benchmark_group("render");

    let area = Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 50,
    };

    for &(label, breadth, depth) in TREE_SIZES {
        let tree = build_synthetic_tree(breadth, depth);

        group.bench_with_input(BenchmarkId::new("context", label), &tree, |b, tree| {
            let style = TreeWidgetStyle::default();
            b.iter(|| {
                let mut state = TreeWidgetState::default();
                state.expand_all(tree);
                let mut ctx = RenderContext::new(tree, &mut state, &style, None);
                ctx.render(area);
            });
        });

        group.bench_with_input(BenchmarkId::new("widget_full", label), &tree, |b, tree| {
            b.iter(|| {
                let mut state = TreeWidgetState::default();
                state.expand_all(tree);
                let mut terminal =
                    Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
                terminal
                    .draw(|frame| {
                        let frame_area = frame.area();
                        TreeWidget::new(tree).render(frame_area, frame.buffer_mut(), &mut state);
                    })
                    .unwrap();
            });
        });
    }

    group.finish();
}

fn bench_navigation(c: &mut Criterion) {
    let mut group = c.benchmark_group("navigation");

    let tree = build_synthetic_tree(6, 6); // ~56k nodes

    group.bench_function("select_next", |b| {
        let mut state = TreeWidgetState::default();
        state.expand_all(&tree);
        b.iter(|| {
            state.select_next(&tree);
        });
    });

    group.bench_function("select_previous", |b| {
        let mut state = TreeWidgetState::default();
        state.expand_all(&tree);
        // Move to the middle first
        for _ in 0..500 {
            state.select_next(&tree);
        }
        b.iter(|| {
            state.select_previous(&tree);
            state.select_next(&tree);
        });
    });

    group.bench_function("toggle_expand_collapse", |b| {
        let mut state = TreeWidgetState::default();
        state.expand_all(&tree);
        b.iter(|| {
            state.collapse(&tree);
            state.expand(&tree);
        });
    });

    group.bench_function("page_down_up", |b| {
        let mut state = TreeWidgetState::default();
        state.expand_all(&tree);
        state.visible_nodes(&tree);
        // Set a realistic viewport height
        state.viewport.height = 50;
        b.iter(|| {
            state.page_down(&tree);
            state.page_up(&tree);
        });
    });

    group.finish();
}

fn bench_load(c: &mut Criterion) {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");

    c.bench_function("DependencyTree::load", |b| {
        b.iter(|| cargo_tree_tui::core::DependencyTree::load(Some(manifest.clone())).unwrap());
    });
}

criterion_group!(
    benches,
    bench_load,
    bench_expand_all,
    bench_visible_nodes,
    bench_search,
    bench_render_context,
    bench_navigation,
);
criterion_main!(benches);
