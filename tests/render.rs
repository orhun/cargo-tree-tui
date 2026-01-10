mod common;

use cargo_tree_tui::core::dependency::DependencyType;
use common::{TestNode, TestNodeKind, build_tree, render_tree};
use pretty_assertions::assert_eq;

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
    let tree_str = render_tree(&tree);
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
    assert_eq!(expected.trim(), render_tree(&tree).trim());
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
    assert_eq!(expected.trim(), render_tree(&tree).trim());
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
    assert_eq!(expected.trim(), render_tree(&tree).trim());
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
    assert_eq!(expected.trim(), render_tree(&tree).trim());
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
    assert_eq!(expected.trim(), render_tree(&tree).trim());
}
