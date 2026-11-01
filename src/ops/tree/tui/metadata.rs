use std::borrow::Cow;

use cargo::core::PackageId;
use clap_cargo::style::{HEADER, NOP, VALID, WARN};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget, Tabs, Widget, Wrap,
    },
};
use rustc_hash::FxHashMap;

use crate::{
    core::{
        Dependency, DependencyNode, DependencyTree, DependencyUsage, NodeId,
        dependency::DependencyType,
    },
    ops::tree::tui::{
        TuiTree,
        widget::{TreeCharset, TreeFormat, TreePrefix, TreeWidget, TreeWidgetState},
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MetadataTab {
    #[default]
    Package,
    Usage,
}

impl MetadataTab {
    fn index(self) -> usize {
        match self {
            Self::Package => 0,
            Self::Usage => 1,
        }
    }

    pub fn next(&mut self) {
        *self = match self {
            Self::Package => Self::Usage,
            Self::Usage => Self::Package,
        };
    }

    pub fn previous(&mut self) {
        self.next();
    }
}

#[derive(Debug, Default)]
pub struct MetadataPopupState {
    pub tab: MetadataTab,
    pub scroll: usize,
    pub(crate) page_size: usize,
    source_package_nodes: Option<FxHashMap<PackageId, NodeId>>,
    usage_tree: Option<UsageTree>,
}

#[derive(Debug)]
struct UsageTree {
    package_id: Option<cargo::core::PackageId>,
    tree: TuiTree,
    state: TreeWidgetState,
    usage_indices: Vec<Vec<usize>>,
    parent_nodes: FxHashMap<PackageId, NodeId>,
}

impl MetadataPopupState {
    pub fn invalidate_usage_tree(&mut self) {
        self.usage_tree = None;
        self.source_package_nodes = None;
    }

    pub fn move_usage_selection(&mut self, down: bool) {
        let Some(usage_tree) = &mut self.usage_tree else {
            return;
        };
        if down {
            usage_tree.state.select_next(&usage_tree.tree);
        } else {
            usage_tree.state.select_previous(&usage_tree.tree);
        }
    }
}

pub struct MetadataPopup<'a> {
    dependency: Option<&'a Dependency>,
    usages: Option<&'a [DependencyUsage]>,
    source_tree: &'a TuiTree,
    prefix: TreePrefix,
    charset: TreeCharset,
    format: &'a TreeFormat,
}

impl<'a> MetadataPopup<'a> {
    pub fn new(
        dependency: Option<&'a Dependency>,
        usages: Option<&'a [DependencyUsage]>,
        source_tree: &'a TuiTree,
        prefix: TreePrefix,
        charset: TreeCharset,
        format: &'a TreeFormat,
    ) -> Self {
        Self {
            dependency,
            usages,
            source_tree,
            prefix,
            charset,
            format,
        }
    }

    fn package_lines(&self) -> Text<'_> {
        let Some(dependency) = self.dependency else {
            return Text::from(" No dependency selected ");
        };

        let metadata = dependency.format_metadata.as_deref();
        let mut lines = Vec::with_capacity(15);
        push_field_styled(&mut lines, "Name", &dependency.name, Style::from(WARN));
        push_field(&mut lines, "Version", &dependency.version);
        push_field(
            &mut lines,
            "Edition",
            metadata
                .and_then(|metadata| metadata.edition.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "Rust version",
            metadata
                .and_then(|metadata| metadata.rust_version.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "Targets",
            metadata
                .and_then(|metadata| metadata.targets.as_deref())
                .unwrap_or("Not specified"),
        );

        if let Some(package_id) = &dependency.package_id {
            let source = package_id.source_id();
            let source = if source.is_crates_io() {
                "crates.io"
            } else {
                metadata
                    .and_then(|metadata| metadata.source.as_deref())
                    .unwrap_or("Not specified")
            };
            push_field(&mut lines, "Source", source);
        }

        push_field(
            &mut lines,
            "Description",
            metadata
                .and_then(|metadata| metadata.description.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "License",
            metadata
                .and_then(|metadata| metadata.license.as_deref())
                .unwrap_or("Not specified"),
        );
        push_link_field(
            &mut lines,
            "Repository",
            metadata.and_then(|metadata| metadata.repository.as_deref()),
        );
        push_link_field(
            &mut lines,
            "Homepage",
            metadata.and_then(|metadata| metadata.homepage.as_deref()),
        );
        push_link_field(
            &mut lines,
            "Documentation",
            metadata.and_then(|metadata| metadata.documentation.as_deref()),
        );
        push_field(
            &mut lines,
            "Authors",
            metadata
                .and_then(|metadata| metadata.authors.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "Keywords",
            metadata
                .and_then(|metadata| metadata.keywords.as_deref())
                .unwrap_or("Not specified"),
        );
        push_field(
            &mut lines,
            "Categories",
            metadata
                .and_then(|metadata| metadata.categories.as_deref())
                .unwrap_or("Not specified"),
        );
        if let Some(library) = metadata.and_then(|metadata| metadata.library.as_deref()) {
            push_field(&mut lines, "Library", library);
        }
        if dependency.is_proc_macro {
            push_field(&mut lines, "Target", "proc-macro");
        }
        if let Some(path) = &dependency.manifest_dir {
            push_field(&mut lines, "Manifest", path.as_str());
        }

        Text::from(lines)
    }
}

impl StatefulWidget for MetadataPopup<'_> {
    type State = MetadataPopupState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let width = area.width / 2;
        let height = area.height.saturating_sub(2).min(20);
        if width < 4 || height < 3 {
            return;
        }
        let popup_area = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: area.y + area.height.saturating_sub(height).saturating_sub(2),
            width,
            height,
        };

        Clear.render(popup_area, buf);
        let title_style = Style::from(HEADER)
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::REVERSED);
        let block = Block::default()
            .title(Line::from(Span::styled(
                " DEPENDENCY METADATA ",
                title_style,
            )))
            .borders(Borders::ALL)
            .border_style(Style::from(HEADER));
        let inner = block.inner(popup_area);
        let tabs_area = Rect {
            x: inner.x.saturating_add(1),
            width: inner.width.saturating_sub(2),
            height: 1,
            ..inner
        };
        let body_area = Rect {
            x: inner.x.saturating_add(1),
            width: inner.width.saturating_sub(2),
            y: inner.y.saturating_add(1),
            height: inner.height.saturating_sub(1),
        };
        let content_area = Rect {
            width: body_area.width.saturating_sub(1),
            ..body_area
        };
        let scrollbar_area = Rect {
            x: body_area.right().saturating_sub(1),
            width: 1,
            ..body_area
        };
        let selected_style =
            Style::from(HEADER).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        Tabs::new(["Package", "Usage"])
            .select(state.tab.index())
            .highlight_style(selected_style)
            .divider(Span::styled("│", Style::from(NOP)))
            .render(tabs_area, buf);
        block.render(popup_area, buf);
        match state.tab {
            MetadataTab::Package => {
                let lines = self.package_lines();
                let content_height = wrapped_line_count(&lines, content_area.width as usize);
                state.page_size = content_area.height as usize;
                let paragraph = Paragraph::new(lines)
                    .style(Style::from(NOP))
                    .wrap(Wrap { trim: true });
                let max_scroll = content_height.saturating_sub(content_area.height as usize);
                state.scroll = state.scroll.min(max_scroll);
                let scroll_offset = state.scroll.min(u16::MAX as usize) as u16;
                let mut scrollbar_state = ScrollbarState::new(max_scroll)
                    .position(state.scroll)
                    .viewport_content_length(content_area.height as usize);
                paragraph
                    .scroll((scroll_offset, 0))
                    .render(content_area, buf);
                if max_scroll > 0 {
                    Scrollbar::new(ScrollbarOrientation::VerticalRight)
                        .track_symbol(Some("┆"))
                        .thumb_symbol("▐")
                        .render(scrollbar_area, buf, &mut scrollbar_state);
                }
            }
            MetadataTab::Usage => self.render_usage(content_area, buf, state),
        }
    }
}

impl MetadataPopup<'_> {
    fn render_usage(&self, area: Rect, buf: &mut Buffer, state: &mut MetadataPopupState) {
        let Some(dependency) = self.dependency else {
            Paragraph::new(" No dependency selected ").render(area, buf);
            return;
        };
        let Some(usages) = self.usages.filter(|usages| !usages.is_empty()) else {
            Paragraph::new(" No incoming dependency declarations in this tree ").render(area, buf);
            return;
        };

        let package_id = dependency.package_id;
        if state.source_package_nodes.is_none() {
            state.source_package_nodes = Some(source_package_nodes(self.source_tree));
        }
        if state
            .usage_tree
            .as_ref()
            .is_none_or(|cached| cached.package_id != package_id)
        {
            let usage_tree = build_usage_tree(
                dependency,
                usages,
                self.source_tree,
                state
                    .source_package_nodes
                    .as_ref()
                    .expect("source package index was initialized"),
            );
            state.usage_tree = Some(usage_tree);
        }
        let usage_tree = state
            .usage_tree
            .as_mut()
            .expect("usage tree was initialized");
        let selected = usage_tree.state.selected_node_id();
        let usage_index = selected
            .and_then(|id| usage_tree.usage_indices.get(id.0))
            .and_then(|indices| indices.first())
            .copied();
        let selected_usage = usage_index.and_then(|index| usages.get(index));
        let declaration =
            selected_usage.map(|usage| format_usage_declaration(usage, &dependency.version));
        let [tree_area, details_column] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(area);
        let declaration_height = declaration
            .as_ref()
            .map(|(section, text)| {
                2 + Line::from(text.as_str())
                    .width()
                    .div_ceil(details_column.width.saturating_sub(2).max(1) as usize)
                    + Line::from(section.as_str())
                        .width()
                        .div_ceil(details_column.width.max(1) as usize)
            })
            .unwrap_or(0)
            .min(details_column.height.saturating_sub(1) as usize)
            as u16;
        let [details_area, declaration_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(declaration_height)])
                .areas(details_column);
        let tree_widget_area = Rect {
            width: tree_area.width.saturating_sub(1),
            ..tree_area
        };
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .track_symbol(Some("┆"))
            .thumb_symbol("▐")
            .begin_symbol(Some("▴"))
            .end_symbol(Some("▾"));
        let widget = TreeWidget::new(&usage_tree.tree)
            .prefix(self.prefix)
            .charset(self.charset)
            .format(self.format)
            .scrollbar(scrollbar);
        widget.render(tree_widget_area, buf, &mut usage_tree.state);

        if let Some(usage) = selected_usage {
            let mut lines = Vec::with_capacity(9);
            lines.push(Line::from(Span::styled(
                "Dependency declaration",
                Style::from(HEADER).add_modifier(Modifier::BOLD),
            )));
            if let Some(parent) = usage_tree
                .parent_nodes
                .get(&usage.parent)
                .and_then(|id| usage_tree.tree.node(*id))
                .and_then(DependencyNode::as_dependency)
            {
                push_compact_field(
                    &mut lines,
                    "From",
                    format!("{} v{}", parent.name, parent.version),
                );
            }
            push_compact_field(
                &mut lines,
                "To",
                format!("{} v{}", dependency.name, dependency.version),
            );
            push_compact_field(&mut lines, "Manifest section", usage.kind.label());
            push_compact_field(
                &mut lines,
                "Declared as",
                usage.dependency.name_in_toml().as_str(),
            );
            push_compact_field(
                &mut lines,
                "Optional",
                if usage.dependency.is_optional() {
                    "yes"
                } else {
                    "no"
                },
            );
            push_compact_field(
                &mut lines,
                "Default features",
                if usage.dependency.uses_default_features() {
                    "enabled"
                } else {
                    "disabled"
                },
            );
            push_compact_field(
                &mut lines,
                "Requested features",
                if usage.dependency.features().is_empty() {
                    "none".to_owned()
                } else {
                    usage
                        .dependency
                        .features()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            );
            if let Some(target) = usage.dependency.platform() {
                push_compact_field(&mut lines, "Target condition", target.to_string());
            }
            Paragraph::new(Text::from(lines))
                .style(Style::from(NOP))
                .wrap(Wrap { trim: true })
                .render(details_area, buf);
        }

        if let Some(declaration) = declaration {
            let lines = Text::from(vec![
                Line::styled(
                    "Cargo.toml (simplified)",
                    Style::from(HEADER).add_modifier(Modifier::BOLD),
                ),
                Line::from(declaration.0),
                Line::from(declaration.1),
            ]);
            Paragraph::new(lines)
                .style(Style::from(NOP))
                .wrap(Wrap { trim: false })
                .render(declaration_area, buf);
        }
    }
}

fn push_compact_field<'a>(
    lines: &mut Vec<Line<'a>>,
    label: &'static str,
    value: impl Into<Cow<'a, str>>,
) {
    lines.push(Line::from(vec![
        Span::styled(label, Style::from(VALID)),
        Span::raw(": "),
        Span::raw(value),
    ]));
}

fn format_usage_declaration(usage: &DependencyUsage, version: &str) -> (String, String) {
    let dependency = &usage.dependency;
    let section = match usage.kind {
        DependencyType::Normal => "dependencies",
        DependencyType::Dev => "dev-dependencies",
        DependencyType::Build => "build-dependencies",
    };
    let section = match dependency.platform() {
        Some(target) => format!("[target.'{target}'.{section}]"),
        None => format!("[{section}]"),
    };
    let mut fields = Vec::with_capacity(5);
    fields.push(format!("version = {version:?}"));
    if dependency.package_name() != dependency.name_in_toml() {
        fields.push(format!(
            "package = {:?}",
            dependency.package_name().as_str()
        ));
    }
    if !dependency.features().is_empty() {
        let features = dependency
            .features()
            .iter()
            .map(|feature| format!("{:?}", feature.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        fields.push(format!("features = [{features}]"));
    }
    if dependency.is_optional() {
        fields.push("optional = true".to_owned());
    }
    if !dependency.uses_default_features() {
        fields.push("default-features = false".to_owned());
    }
    let declaration = format!(
        "{} = {{ {} }}",
        dependency.name_in_toml(),
        fields.join(", ")
    );
    (section, declaration)
}

fn build_usage_tree(
    dependency: &Dependency,
    usages: &[DependencyUsage],
    source_tree: &TuiTree,
    source_package_nodes: &FxHashMap<PackageId, NodeId>,
) -> UsageTree {
    let mut root = dependency.clone();
    root.children.clear();
    let mut nodes = vec![DependencyNode::Crate(root)];
    let mut usage_indices = vec![Vec::new()];
    let mut parent_nodes = FxHashMap::<PackageId, NodeId>::default();
    let mut selected_node = None;

    for (index, usage) in usages.iter().enumerate() {
        let Some(parent_node_id) = source_package_nodes.get(&usage.parent) else {
            continue;
        };
        let Some(parent_dependency) = source_tree
            .node(*parent_node_id)
            .and_then(DependencyNode::as_dependency)
        else {
            continue;
        };
        let parent_id = *parent_nodes.entry(usage.parent).or_insert_with(|| {
            let mut parent = parent_dependency.clone();
            parent.children.clear();
            let id = NodeId(nodes.len());
            nodes.push(DependencyNode::Crate(parent));
            usage_indices.push(Vec::new());
            if let DependencyNode::Crate(root) = &mut nodes[0] {
                root.children.push(id);
            }
            id
        });
        usage_indices[parent_id.0].push(index);

        selected_node.get_or_insert(parent_id);
    }

    let mut parents = vec![Vec::new(); nodes.len()];
    for (parent, node) in nodes.iter().enumerate() {
        for &child in node.children() {
            parents[child.0].push(NodeId(parent));
        }
    }
    let tree = DependencyTree {
        workspace_name: dependency.name.clone(),
        nodes,
        parents,
        dependency_usages: FxHashMap::default(),
        roots: vec![NodeId(0)],
    };
    let mut state = TreeWidgetState::default();
    state.expand_all(&tree);
    if let Some(selected_node) = selected_node {
        state.set_selected_node_id(&tree, selected_node);
    }
    UsageTree {
        package_id: dependency.package_id,
        tree: TuiTree::new(tree, true),
        state,
        usage_indices,
        parent_nodes,
    }
}

fn source_package_nodes(source_tree: &TuiTree) -> FxHashMap<PackageId, NodeId> {
    let mut nodes = FxHashMap::default();
    for (index, node) in source_tree.nodes.iter().enumerate() {
        if let Some(dependency) = node.as_dependency()
            && let Some(package_id) = dependency.package_id
        {
            nodes.entry(package_id).or_insert(NodeId(index));
        }
    }
    nodes
}

fn wrapped_line_count(text: &Text<'_>, width: usize) -> usize {
    text.lines
        .iter()
        .map(|line| line.width().max(1).div_ceil(width.max(1)))
        .sum()
}

fn push_field<'a>(lines: &mut Vec<Line<'a>>, label: &'static str, value: impl Into<Cow<'a, str>>) {
    push_field_styled(lines, label, value, Style::default());
}

fn push_field_styled<'a>(
    lines: &mut Vec<Line<'a>>,
    label: &'static str,
    value: impl Into<Cow<'a, str>>,
    value_style: Style,
) {
    const LABEL_COLUMN_WIDTH: usize = 18;
    let padding = LABEL_COLUMN_WIDTH.saturating_sub(label.len()) + 1;
    lines.push(Line::from(vec![
        Span::styled(label, Style::from(VALID)),
        Span::raw(" ".repeat(padding)),
        Span::raw(value).style(value_style),
    ]));
}

fn push_link_field<'a>(lines: &mut Vec<Line<'a>>, label: &'static str, value: Option<&'a str>) {
    push_field(lines, label, value.unwrap_or("Not specified"));
    if let Some(value) = lines.last_mut().and_then(|line| line.spans.last_mut()) {
        value.style = value.style.add_modifier(Modifier::ITALIC);
    }
}
