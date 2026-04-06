use crate::core::{DependencyTree, NodeId};

use super::state::{VirtualPos, VisIdx, VisibleNode};

/// Cached materialization state for a single view (normal or search-filtered).
#[derive(Debug, Default)]
pub(super) struct ViewCache {
    /// Materialized window of visible nodes around the viewport.
    pub(super) nodes: Vec<VisibleNode>,
    /// Last visible non-group child per parent, indexed by parent visible index.
    pub(super) last_non_group_child: Vec<Option<VisIdx>>,
    /// Memoized visible-subtree sizes indexed by NodeId.
    pub(super) subtree_sizes: Vec<usize>,
    /// Total number of virtual lines (sum of root subtree sizes).
    pub(super) total_virtual_lines: usize,
    /// Whether sibling links / last-child map need recomputation.
    pub(super) metadata_dirty: bool,
}

impl ViewCache {
    /// Clears all cached data, resetting to empty state.
    pub(super) fn clear(&mut self) {
        self.nodes.clear();
        self.last_non_group_child.fill(None);
        self.subtree_sizes.clear();
        self.total_virtual_lines = 0;
        self.metadata_dirty = false;
    }

    /// Applies a materialization result to this cache.
    pub(super) fn apply_materialization(&mut self, result: MaterializeResult) {
        self.nodes = result.nodes;
        self.metadata_dirty = true;
    }

    /// Recomputes subtree sizes for the given filter.
    pub(super) fn recompute_subtree_sizes(
        &mut self,
        tree: &DependencyTree,
        open: &[bool],
        filter: Option<&[bool]>,
    ) {
        self.total_virtual_lines =
            compute_subtree_sizes(tree, open, filter, &mut self.subtree_sizes);
    }

    /// Ensures sibling links and last-child map are up to date.
    pub(super) fn ensure_metadata(&mut self, tree: &DependencyTree) {
        if !self.metadata_dirty || self.nodes.is_empty() {
            return;
        }
        Self::populate_sibling_links(&mut self.nodes);
        Self::populate_last_non_group_child_map(tree, &self.nodes, &mut self.last_non_group_child);
        self.metadata_dirty = false;
    }

    /// Records, for each parent visible position, the last visible non-group child position.
    fn populate_last_non_group_child_map(
        tree: &DependencyTree,
        visible_nodes: &[VisibleNode],
        target: &mut Vec<Option<VisIdx>>,
    ) {
        target.clear();
        target.resize(visible_nodes.len(), None);

        for (vis_idx, node) in visible_nodes.iter().enumerate() {
            let Some(tree_node) = tree.node(node.id) else {
                continue;
            };

            if tree_node.is_group() {
                continue;
            }

            if let Some(parent_vis_idx) = node.parent_vis_idx {
                target[parent_vis_idx.0] = Some(VisIdx(vis_idx));
            }
        }
    }

    /// Links each visible node to its next and previous sibling (same parent).
    fn populate_sibling_links(visible_nodes: &mut [VisibleNode]) {
        let mut last_child_of: Vec<Option<VisIdx>> = vec![None; visible_nodes.len()];

        for i in 0..visible_nodes.len() {
            let Some(parent) = visible_nodes[i].parent_vis_idx else {
                continue;
            };

            let current = VisIdx(i);
            if let Some(prev) = last_child_of[parent.0] {
                visible_nodes[prev.0].next_sibling = Some(current);
                visible_nodes[i].prev_sibling = Some(prev);
            }
            last_child_of[parent.0] = Some(current);
        }
    }
}

// ── Materialization ─────────────────────────────────────────────────

/// Output of windowed materialization.
pub(super) struct MaterializeResult {
    /// Materialized nodes: ancestor prefix + viewport window.
    pub(super) nodes: Vec<VisibleNode>,
}

struct MaterializeCtx<'a> {
    tree: &'a DependencyTree,
    open: &'a [bool],
    sizes: &'a [usize],
    filter: Option<&'a [bool]>,
    virtual_pos: usize,
    window_start: usize,
    window_end: usize,
    /// Stack of ancestors on the current DFS path: (NodeId, depth, virtual_pos).
    ancestor_stack: Vec<(NodeId, usize, usize)>,
    /// Cycle guard: nodes currently on the DFS path. Mirrors `in_progress`
    /// in `compute_size_recursive` so back-edges in cyclic dep graphs
    /// (e.g. dev-dep cycles) are emitted as leaves rather than recursed into.
    in_progress: Vec<bool>,
    prefix_emitted: bool,
    output: Vec<VisibleNode>,
}

impl MaterializeCtx<'_> {
    fn materialize_node(&mut self, id: NodeId, depth: usize, parent_ancestor_idx: Option<usize>) {
        let my_vpos = self.virtual_pos;
        let subtree_size = self.sizes[id.0];

        // Entirely before window — skip subtree
        if my_vpos + subtree_size <= self.window_start {
            self.virtual_pos += subtree_size;
            return;
        }

        // Entirely past window — stop
        if my_vpos >= self.window_end {
            return;
        }

        // This node is in or overlaps the window.
        self.virtual_pos += 1;

        let in_window = my_vpos >= self.window_start;

        if in_window {
            // Emit ancestor prefix if this is the first in-window node.
            if !self.prefix_emitted {
                self.emit_ancestor_prefix();
            }

            let parent_vis_idx = if depth == 0 {
                None
            } else {
                // Find the parent in the output buffer.
                // The parent is the last ancestor_stack entry, which was either emitted
                // as prefix or as a previous window node.
                self.find_parent_vis_idx(parent_ancestor_idx)
            };

            self.output.push(VisibleNode {
                id,
                depth,
                virtual_pos: VirtualPos(my_vpos),
                parent_vis_idx,
                next_sibling: None,
                prev_sibling: None,
            });
        }

        // Recurse into children if open. Skip recursion on a back-edge —
        // a node already on the current DFS path is a cycle, and the size
        // accounting in `compute_size_recursive` treats it as a leaf.
        if self.open[id.0]
            && !self.in_progress[id.0]
            && let Some(node) = self.tree.node(id)
        {
            let my_ancestor_idx = self.ancestor_stack.len();
            self.ancestor_stack.push((id, depth, my_vpos));
            self.in_progress[id.0] = true;

            for &child in node.children() {
                if self.filter.is_some_and(|f| !f[child.0]) {
                    continue;
                }
                if self.virtual_pos >= self.window_end {
                    break;
                }
                let child_size = self.sizes[child.0];
                if self.virtual_pos + child_size <= self.window_start {
                    self.virtual_pos += child_size;
                    continue;
                }
                self.materialize_node(child, depth + 1, Some(my_ancestor_idx));
            }

            self.in_progress[id.0] = false;
            self.ancestor_stack.pop();
        }
    }

    /// Emits ancestor prefix nodes for lineage/breadcrumb rendering.
    fn emit_ancestor_prefix(&mut self) {
        self.prefix_emitted = true;

        if self.ancestor_stack.is_empty() {
            return;
        }

        for i in 0..self.ancestor_stack.len() {
            let (anc_id, anc_depth, anc_vpos) = self.ancestor_stack[i];

            let parent_vis_idx = if anc_depth == 0 || i == 0 {
                None
            } else {
                // Parent is the previous ancestor in the prefix.
                Some(VisIdx(self.output.len() - 1))
            };

            self.output.push(VisibleNode {
                id: anc_id,
                depth: anc_depth,
                virtual_pos: VirtualPos(anc_vpos),
                parent_vis_idx,
                next_sibling: None,
                prev_sibling: None,
            });
        }
    }

    /// Finds the VisIdx of the parent node in the output buffer.
    fn find_parent_vis_idx(&self, parent_ancestor_idx: Option<usize>) -> Option<VisIdx> {
        let parent_ancestor_idx = parent_ancestor_idx?;
        let (parent_id, _, parent_vpos) = self.ancestor_stack[parent_ancestor_idx];

        // Search the output buffer for this parent.
        // It's either in the prefix or was emitted as a window node.
        // Search from the end since the parent was recently emitted.
        self.output
            .iter()
            .rposition(|n| n.id == parent_id && n.virtual_pos == VirtualPos(parent_vpos))
            .map(VisIdx)
    }
}

/// Materializes a window of visible nodes around a viewport range.
///
/// The output contains an ancestor prefix (for lineage/breadcrumb rendering)
/// followed by the viewport window nodes. Only nodes within `[window_start, window_end)`
/// are emitted, plus ancestors of the first emitted node.
pub(super) fn materialize_window(
    tree: &DependencyTree,
    open: &[bool],
    sizes: &[usize],
    filter: Option<&[bool]>,
    roots: &[NodeId],
    window_start: usize,
    window_count: usize,
) -> MaterializeResult {
    let mut ctx = MaterializeCtx {
        tree,
        open,
        sizes,
        filter,
        virtual_pos: 0,
        window_start,
        window_end: window_start + window_count,
        ancestor_stack: Vec::with_capacity(64),
        in_progress: vec![false; tree.nodes.len()],
        prefix_emitted: false,
        output: Vec::with_capacity(window_count + 64),
    };

    for &root in roots {
        if filter.is_some_and(|f| !f[root.0]) {
            continue;
        }
        if ctx.virtual_pos >= ctx.window_end {
            break;
        }
        let root_size = sizes[root.0];
        if ctx.virtual_pos + root_size <= ctx.window_start {
            ctx.virtual_pos += root_size;
            continue;
        }
        ctx.materialize_node(root, 0, None);
    }

    MaterializeResult { nodes: ctx.output }
}

// ── Subtree size computation ──��─────────────────────────────────────

/// Computes memoized visible-subtree sizes for all nodes.
///
/// `subtree_size[id] = 1 + (if open[id]: Σ subtree_size[child])`.
/// A `computed` guard prevents recomputing already-visited nodes (DAG memoization).
/// An `in_progress` guard breaks hypothetical cycles by treating in-progress nodes as leaves.
fn compute_subtree_sizes(
    tree: &DependencyTree,
    open: &[bool],
    filter: Option<&[bool]>,
    sizes: &mut Vec<usize>,
) -> usize {
    sizes.clear();
    sizes.resize(tree.nodes.len(), 0);
    let mut computed = vec![false; tree.nodes.len()];
    let mut in_progress = vec![false; tree.nodes.len()];

    let mut total = 0usize;
    for &root in tree.roots() {
        if filter.is_some_and(|f| !f[root.0]) {
            continue;
        }
        total += compute_size_recursive(
            tree,
            open,
            filter,
            root,
            sizes,
            &mut computed,
            &mut in_progress,
        );
    }
    total
}

fn compute_size_recursive(
    tree: &DependencyTree,
    open: &[bool],
    filter: Option<&[bool]>,
    id: NodeId,
    sizes: &mut [usize],
    computed: &mut [bool],
    in_progress: &mut [bool],
) -> usize {
    if in_progress[id.0] {
        return 1; // cycle break
    }
    if computed[id.0] {
        return sizes[id.0];
    }

    in_progress[id.0] = true;

    let mut size: usize = 1;
    if open[id.0]
        && let Some(node) = tree.node(id)
    {
        for &child in node.children() {
            if filter.is_some_and(|f| !f[child.0]) {
                continue;
            }
            size += compute_size_recursive(tree, open, filter, child, sizes, computed, in_progress);
        }
    }

    sizes[id.0] = size;
    computed[id.0] = true;
    in_progress[id.0] = false;
    size
}
