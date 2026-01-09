mod common;

use common::{TestNode, build_tree, render_tree};
use pretty_assertions::assert_eq;

#[test]
fn render_simple_tree() {
    let nodes = [
        TestNode {
            name: "root",
            parent: None,
            children: &[1, 2],
        },
        TestNode {
            name: "a",
            parent: Some(0),
            children: &[3],
        },
        TestNode {
            name: "b",
            parent: Some(0),
            children: &[],
        },
        TestNode {
            name: "c",
            parent: Some(1),
            children: &[],
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
