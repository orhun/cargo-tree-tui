use std::path::PathBuf;

use cargo::core::dependency::DepKind;
use cargo_tree_tui::core::dependency::DependencyType;
use cargo_tree_tui::core::{Dependency, DependencyGroup, DependencyNode, DependencyTree, NodeId};

fn project_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
}

#[test]
fn load_returns_tree_for_own_manifest() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();
    assert_eq!(tree.workspace_name, "cargo-tree-tui");
}

#[test]
fn load_has_single_root() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();
    assert_eq!(
        tree.roots().len(),
        1,
        "single-crate workspace should have one root"
    );
}

#[test]
fn root_node_is_crate_with_correct_name() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();
    let root_id = tree.roots()[0];
    let root = tree.node(root_id).unwrap();

    assert!(
        matches!(root, DependencyNode::Crate(_)),
        "root should be a Crate node"
    );
    assert_eq!(root.display_name(), "cargo-tree-tui");
}

#[test]
fn root_has_children() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();
    let root_id = tree.roots()[0];
    let root = tree.node(root_id).unwrap();

    assert!(
        !root.children().is_empty(),
        "root should have at least one child (this crate has dependencies)"
    );
}

#[test]
fn all_nodes_reachable_from_roots() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    let mut visited = vec![false; tree.nodes.len()];
    let mut stack: Vec<NodeId> = tree.roots().to_vec();

    while let Some(id) = stack.pop() {
        if visited[id.0] {
            continue;
        }
        visited[id.0] = true;
        if let Some(node) = tree.node(id) {
            stack.extend_from_slice(node.children());
        }
    }

    let reachable = visited.iter().filter(|&&v| v).count();
    assert_eq!(
        reachable,
        tree.nodes.len(),
        "every node in the arena should be reachable from the roots"
    );
}

#[test]
fn crate_nodes_excludes_groups() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    for id in tree.crate_nodes() {
        let node = tree.node(id).unwrap();
        assert!(
            !node.is_group(),
            "crate_nodes should not contain group nodes"
        );
    }
}

#[test]
fn crate_nodes_covers_all_crates() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    let crate_count = tree
        .nodes
        .iter()
        .filter(|n| matches!(n, DependencyNode::Crate(_)))
        .count();
    assert_eq!(
        tree.crate_nodes().count(),
        crate_count,
        "crate_nodes should list every Crate node in the arena"
    );
}

#[test]
fn known_dependency_present() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    let has_ratatui = tree.crate_nodes().any(|id| {
        tree.node(id)
            .map(|n| n.display_name() == "ratatui")
            .unwrap_or(false)
    });
    assert!(has_ratatui, "ratatui should appear as a dependency");
}

#[test]
fn parent_child_links_consistent() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    for (idx, node) in tree.nodes.iter().enumerate() {
        let node_id = NodeId(idx);
        for &child_id in node.children() {
            assert!(
                tree.node(child_id).is_some(),
                "child id {:?} should be valid",
                child_id
            );
            assert!(
                tree.parents[child_id.0].contains(&node_id),
                "parents of child {:?} should include {:?}",
                child_id,
                node_id
            );
        }
    }
}

#[test]
fn root_nodes_have_no_parent() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    for &root_id in tree.roots() {
        assert!(
            tree.parents[root_id.0].is_empty(),
            "root node {:?} should have no parent",
            root_id
        );
    }
}

#[test]
fn group_nodes_have_valid_kind() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    for (idx, node) in tree.nodes.iter().enumerate() {
        if let DependencyNode::Group(group) = node {
            // label() should return a non-empty string for any valid kind
            assert!(!group.label().is_empty(), "group label should not be empty");
            let group_id = NodeId(idx);
            assert!(
                !tree.parents[group_id.0].is_empty(),
                "group nodes should always have a parent"
            );
        }
    }
}

#[test]
fn dev_dependencies_under_group() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    // pretty_assertions is a dev dep of this crate
    let pa_id = tree
        .crate_nodes()
        .find(|&id| {
            tree.node(id)
                .map(|n| n.display_name() == "pretty_assertions")
                .unwrap_or(false)
        })
        .expect("pretty_assertions should be in the tree as a dev dependency");

    let parents = &tree.parents[pa_id.0];
    assert!(
        !parents.is_empty(),
        "pretty_assertions should have at least one parent"
    );
    let under_dev_group = parents.iter().any(|&parent_id| {
        tree.node(parent_id)
            .and_then(|n| n.as_group())
            .map(|g| g.kind == DependencyType::Dev)
            .unwrap_or(false)
    });
    assert!(
        under_dev_group,
        "pretty_assertions should be nested under a [dev-dependencies] group"
    );
}

#[test]
fn dedup_each_crate_version_appears_once() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    // Group crate node ids by (name, version) — the logical package identity.
    let mut by_pkg: std::collections::HashMap<(&str, &str), Vec<NodeId>> =
        std::collections::HashMap::new();

    for id in tree.crate_nodes() {
        let dep = tree.node(id).unwrap().as_dependency().unwrap();
        by_pkg
            .entry((&dep.name, &dep.version))
            .or_default()
            .push(id);
    }

    for ((name, version), ids) in &by_pkg {
        assert_eq!(
            ids.len(),
            1,
            "'{name} v{version}' should appear exactly once in the \
             deduplicated arena, but found {} occurrences",
            ids.len()
        );
    }
}

#[test]
fn dedup_shared_child_referenced_by_multiple_parents() {
    let tree = DependencyTree::load(Some(project_manifest())).unwrap();

    // Find crates that have more than one parent (shared children).
    let multi_parent: Vec<NodeId> = tree
        .crate_nodes()
        .filter(|id| tree.parents[id.0].len() > 1)
        .collect();

    assert!(
        !multi_parent.is_empty(),
        "at least one crate should be referenced by multiple parents \
         (shared subtree via dedup)"
    );

    // Each shared crate should have a single NodeId referenced from
    // multiple parent children lists.
    for &id in &multi_parent {
        for &parent_id in &tree.parents[id.0] {
            let parent = tree.node(parent_id).unwrap();
            assert!(
                parent.children().contains(&id),
                "parent {:?} claims to be parent of {:?} but doesn't \
                 list it as a child",
                parent_id,
                id
            );
        }
    }
}

#[test]
fn as_dependency_returns_some_for_crate() {
    let dep = DependencyNode::Crate(Dependency {
        name: "foo".into(),
        version: "1.0.0".into(),
        manifest_dir: None,
        is_proc_macro: false,
        children: vec![],
    });
    assert!(dep.as_dependency().is_some());
    assert!(dep.as_group().is_none());
    assert!(!dep.is_group());
}

#[test]
fn as_group_returns_some_for_group() {
    let group = DependencyNode::Group(DependencyGroup {
        kind: DependencyType::Dev,
        children: vec![],
    });
    assert!(group.as_group().is_some());
    assert!(group.as_dependency().is_none());
    assert!(group.is_group());
}

#[test]
fn display_name_for_crate_and_group() {
    let crate_node = DependencyNode::Crate(Dependency {
        name: "serde".into(),
        version: "1.0.0".into(),
        manifest_dir: None,
        is_proc_macro: false,
        children: vec![NodeId(1)],
    });
    assert_eq!(crate_node.display_name(), "serde");

    let group_node = DependencyNode::Group(DependencyGroup {
        kind: DependencyType::Build,
        children: vec![],
    });
    assert_eq!(group_node.display_name(), "[build-dependencies]");
}

#[test]
fn dependency_type_from_dep_kind() {
    assert_eq!(
        DependencyType::from(DepKind::Normal),
        DependencyType::Normal
    );
    assert_eq!(
        DependencyType::from(DepKind::Development),
        DependencyType::Dev
    );
    assert_eq!(DependencyType::from(DepKind::Build), DependencyType::Build);
}

#[test]
fn dependency_type_labels() {
    assert_eq!(DependencyType::Normal.label(), "[dependencies]");
    assert_eq!(DependencyType::Dev.label(), "[dev-dependencies]");
    assert_eq!(DependencyType::Build.label(), "[build-dependencies]");
}

#[test]
fn dependency_type_styles_are_distinct() {
    let normal = DependencyType::Normal.style();
    let dev = DependencyType::Dev.style();
    let build = DependencyType::Build.style();

    assert_ne!(normal, dev);
    assert_ne!(normal, build);
    assert_ne!(dev, build);
}
