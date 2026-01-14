use cargo_tree_tui::core::dependency::DependencyType;
use cargo_tree_tui::core::{Dependency, DependencyGroup, DependencyNode, DependencyTree, NodeId};
use cargo_tree_tui::ops::tree::tui::widget::render::RenderContext;
use cargo_tree_tui::ops::tree::tui::widget::{TreeWidget, TreeWidgetState, TreeWidgetStyle};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;

pub enum TestNodeKind {
    Crate,
    Group(DependencyType),
}

pub struct TestNode {
    pub name: &'static str,
    pub parent: Option<usize>,
    pub children: &'static [usize],
    pub kind: TestNodeKind,
}

pub fn build_tree(nodes: &[TestNode]) -> DependencyTree {
    let mut arena = Vec::with_capacity(nodes.len());
    for node in nodes {
        let parent = node.parent.map(NodeId);
        let children = node.children.iter().copied().map(NodeId).collect();
        let node = match node.kind {
            TestNodeKind::Crate => DependencyNode::Crate(Dependency {
                name: node.name.to_string(),
                version: String::new(),
                manifest_dir: None,
                is_proc_macro: false,
                parent,
                children,
            }),
            TestNodeKind::Group(kind) => DependencyNode::Group(DependencyGroup {
                kind,
                parent,
                children,
            }),
        };
        arena.push(node);
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

pub fn render_tree_context(tree: &DependencyTree) -> String {
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

pub fn render_tree_widget(
    tree: &DependencyTree,
    state: &mut TreeWidgetState,
    area: Rect,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            let frame_area = frame.area();
            TreeWidget::new(tree).render(frame_area, frame.buffer_mut(), state);
        })
        .unwrap();
    terminal
        .backend()
        .to_string()
        .lines()
        .map(|s| s.trim_start_matches('"').trim_end_matches('"').trim_end())
        .collect::<Vec<&str>>()
        .join("\n")
}
