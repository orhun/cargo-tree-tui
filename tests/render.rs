mod common;

use cargo_tree_tui::core::NodeId;
use cargo_tree_tui::core::dependency::DependencyType;
use cargo_tree_tui::ops::tree::tui::widget::TreeWidgetState;
use common::{TestNode, TestNodeKind, build_tree, render_tree_context, render_tree_widget};
use pretty_assertions::assert_eq;
use ratatui::layout::Rect;

#[test]
fn basic() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1, 2],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "a",
            parent: Some(0),
            children: &[3],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "b",
            parent: Some(0),
            children: &[],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "c",
            parent: Some(1),
            children: &[],
            kind: TestNodeKind::Crate,
        },
    ];

    let expected = r#"
root
├──▾ a
│  └──• c
└──• b
"#;

    let tree = build_tree(&nodes);
    let tree_str = render_tree_context(&tree);
    assert_eq!(expected.trim(), tree_str.trim());
}

#[test]
fn root_dev_dependencies_header() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "dev",
            parent: Some(0),
            children: &[2],
            kind: TestNodeKind::Group(DependencyType::Dev),
        },
        TestNode {
            name: "a",
            parent: Some(1),
            children: &[],
            kind: TestNodeKind::Crate,
        },
    ];

    let expected = r#"
root
[dev-dependencies]
└──• a
"#;

    let tree = build_tree(&nodes);
    assert_eq!(expected.trim(), render_tree_context(&tree).trim());
}

#[test]
fn root_normal_deps_then_dev_header() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1, 2],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "a",
            parent: Some(0),
            children: &[],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "dev",
            parent: Some(0),
            children: &[3],
            kind: TestNodeKind::Group(DependencyType::Dev),
        },
        TestNode {
            name: "b",
            parent: Some(2),
            children: &[],
            kind: TestNodeKind::Crate,
        },
    ];

    let expected = r#"
root
└──• a
[dev-dependencies]
└──• b
"#;

    let tree = build_tree(&nodes);
    assert_eq!(expected.trim(), render_tree_context(&tree).trim());
}

#[test]
fn nested_dev_dependencies_header() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "a",
            parent: Some(0),
            children: &[2],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "dev",
            parent: Some(1),
            children: &[3],
            kind: TestNodeKind::Group(DependencyType::Dev),
        },
        TestNode {
            name: "b",
            parent: Some(2),
            children: &[],
            kind: TestNodeKind::Crate,
        },
    ];

    let expected = r#"
root
└──▾ a
   [dev-dependencies]
   └──• b
"#;

    let tree = build_tree(&nodes);
    assert_eq!(expected.trim(), render_tree_context(&tree).trim());
}

#[test]
fn nested_header_preserves_guides() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1, 2],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "x",
            parent: Some(0),
            children: &[],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "a",
            parent: Some(0),
            children: &[3],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "dev",
            parent: Some(2),
            children: &[4],
            kind: TestNodeKind::Group(DependencyType::Dev),
        },
        TestNode {
            name: "b",
            parent: Some(3),
            children: &[],
            kind: TestNodeKind::Crate,
        },
    ];

    let expected = r#"
root
├──• x
└──▾ a
   [dev-dependencies]
   └──• b
"#;

    let tree = build_tree(&nodes);
    assert_eq!(expected.trim(), render_tree_context(&tree).trim());
}

#[test]
fn nested_group_header_with_following_sibling() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1, 4],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "x",
            parent: Some(0),
            children: &[2],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "dev",
            parent: Some(1),
            children: &[3],
            kind: TestNodeKind::Group(DependencyType::Dev),
        },
        TestNode {
            name: "b",
            parent: Some(2),
            children: &[],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "a",
            parent: Some(0),
            children: &[],
            kind: TestNodeKind::Crate,
        },
    ];

    let expected = r#"
root
├──▾ x
│  [dev-dependencies]
│  └──• b
└──• a
"#;

    let tree = build_tree(&nodes);
    assert_eq!(expected.trim(), render_tree_context(&tree).trim());
}

#[test]
fn breadcrumb_when_scrolled() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "a",
            parent: Some(0),
            children: &[2],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "b",
            parent: Some(1),
            children: &[3],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "c",
            parent: Some(2),
            children: &[4],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "d",
            parent: Some(3),
            children: &[5],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "e",
            parent: Some(4),
            children: &[6],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "f",
            parent: Some(5),
            children: &[7],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "g",
            parent: Some(6),
            children: &[],
            kind: TestNodeKind::Crate,
        },
    ];

    let tree = build_tree(&nodes);
    let mut state = TreeWidgetState::default();
    state.expand_all(&tree);
    state.selected = Some(NodeId(7));

    let area = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 5,
    };

    let expected = r#"
root
└──▾ a
         └──▾ d
            └──▾ e
root → a → b → c → d → e → f → g
"#;

    let output = render_tree_widget(&tree, &mut state, area);
    assert_eq!(expected.trim(), output.trim());

    let area = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 5,
    };

    let expected = r#"
root
└──▾ a
         └──▾ d
            └──▾ e
root → a → b → … → g
"#;

    let output = render_tree_widget(&tree, &mut state, area);
    assert_eq!(expected.trim(), output.trim());
}

#[test]
fn context_bar_when_scrolled() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "a",
            parent: Some(0),
            children: &[2],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "b",
            parent: Some(1),
            children: &[3],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "c",
            parent: Some(2),
            children: &[4],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "d",
            parent: Some(3),
            children: &[5],
            kind: TestNodeKind::Crate,
        },
        TestNode {
            name: "e",
            parent: Some(4),
            children: &[],
            kind: TestNodeKind::Crate,
        },
    ];

    let tree = build_tree(&nodes);
    let mut state = TreeWidgetState::default();
    state.expand_all(&tree);
    state.selected = Some(NodeId(5));

    let area = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 5,
    };

    let expected = r#"
root
   └──▾ b
      └──▾ c
         └──▾ d
root → a → b → … → e
"#;

    let output = render_tree_widget(&tree, &mut state, area);
    assert_eq!(expected.trim(), output.trim());
}
