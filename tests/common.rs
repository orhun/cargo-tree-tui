use cargo_tree_tui::core::{Dependency, DependencyNode, DependencyTree, NodeId};
use cargo_tree_tui::ops::tree::tui::widget::render::RenderContext;
use cargo_tree_tui::ops::tree::tui::widget::{TreeWidgetState, TreeWidgetStyle};
use ratatui::layout::Rect;

pub struct TestNode {
    pub name: &'static str,
    pub parent: Option<usize>,
    pub children: &'static [usize],
}

pub fn build_tree(nodes: &[TestNode]) -> DependencyTree {
    let mut arena = Vec::with_capacity(nodes.len());
    for node in nodes {
        let parent = node.parent.map(NodeId);
        let children = node.children.iter().copied().map(NodeId).collect();
        arena.push(DependencyNode::Crate(Dependency {
            name: node.name.to_string(),
            version: String::new(),
            manifest_dir: None,
            is_proc_macro: false,
            parent,
            children,
        }));
    }

    let roots = nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| node.parent.is_none().then_some(NodeId(idx)))
        .collect();

    DependencyTree {
        workspace_name: "workspace".to_string(),
        nodes: arena,
        roots,
    }
}

pub fn render_tree(tree: &DependencyTree) -> String {
    let mut state = TreeWidgetState::default();
    state.expand_all(tree);

    let area = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };

    let style = TreeWidgetStyle::default();
    let mut context = RenderContext::new(tree, &mut state, &style, None);
    let output = context.render(area);

    output
        .lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<String>>()
        .join("\n")
}
