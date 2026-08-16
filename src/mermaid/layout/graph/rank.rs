// SPDX-License-Identifier: MIT
//! Layer assignment: cycle breaking, longest-path ranking and dummy-node insertion.
//!
//! The result is the *layered graph* every later stage works on: a list of virtual
//! nodes — real items plus one-cell routing dummies — grouped into ranks, and one
//! segment per pair of adjacent ranks an edge passes between. Long edges therefore
//! never have to be reasoned about again: they are a chain of unit-length segments,
//! and their dummies occupy real space, so a long edge can never cut through a box.
//!
//! Everything here is index-based and order-preserving, so the output is a pure
//! function of the input order (design spec §13: deterministic layout).

/// What a virtual node stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VKind {
    /// A real item: a node box or a nested group's frame.
    Item(usize),
    /// A routing dummy that reserves a cell for a long edge.
    Dummy(usize),
}

/// A node of the layered graph.
#[derive(Debug, Clone, Copy)]
pub(super) struct VNode {
    /// What the node stands for.
    pub kind: VKind,
    /// Its layer, counted along the flow direction.
    pub rank: usize,
}

/// One edge piece running from rank `r` to rank `r + 1`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Seg {
    /// The level edge this piece belongs to.
    pub edge: usize,
    /// The virtual node in the lower rank.
    pub a: usize,
    /// The virtual node in the higher rank.
    pub b: usize,
}

/// The layered graph.
#[derive(Debug, Clone, Default)]
pub(super) struct Layered {
    /// Every virtual node.
    pub vnodes: Vec<VNode>,
    /// Every unit-length edge piece.
    pub segs: Vec<Seg>,
    /// Virtual node ids per rank, in drawing order.
    pub ranks: Vec<Vec<usize>>,
    /// Per level edge: `true` when the edge was reversed to break a cycle.
    pub reversed: Vec<bool>,
}

/// An edge as the layering stage sees it.
#[derive(Debug, Clone, Copy)]
pub(super) struct RawEdge {
    /// Source item.
    pub from: usize,
    /// Target item.
    pub to: usize,
}

/// Builds the layered graph for `items` connected by `edges`.
///
/// Self edges (`from == to`) must have been removed by the caller; they are drawn as
/// loops beside their item and take no part in layering.
pub(super) fn build(item_count: usize, edges: &[RawEdge]) -> Layered {
    let reversed = break_cycles(item_count, edges);
    let oriented: Vec<RawEdge> = edges
        .iter()
        .zip(&reversed)
        .map(|(e, &rev)| {
            if rev {
                RawEdge {
                    from: e.to,
                    to: e.from,
                }
            } else {
                *e
            }
        })
        .collect();
    let ranks_of = longest_path(item_count, &oriented);
    let mut layered = Layered {
        reversed,
        ..Layered::default()
    };
    for (item, &rank) in ranks_of.iter().enumerate() {
        layered.vnodes.push(VNode {
            kind: VKind::Item(item),
            rank,
        });
    }
    for (edge, e) in oriented.iter().enumerate() {
        let (lo, hi) = (ranks_of[e.from], ranks_of[e.to]);
        let mut previous = e.from;
        for rank in lo + 1..hi {
            let dummy = layered.vnodes.len();
            layered.vnodes.push(VNode {
                kind: VKind::Dummy(edge),
                rank,
            });
            layered.segs.push(Seg {
                edge,
                a: previous,
                b: dummy,
            });
            previous = dummy;
        }
        layered.segs.push(Seg {
            edge,
            a: previous,
            b: e.to,
        });
    }
    layered.ranks = initial_order(&layered, &oriented, item_count);
    layered
}

/// Marks the edges that must be reversed to make the graph acyclic.
///
/// A depth-first sweep in declaration order; any edge pointing back at a node still on
/// the stack is a back edge. Declaration order makes the choice deterministic and
/// keeps the "first path wins" reading that matches how the source was written.
fn break_cycles(item_count: usize, edges: &[RawEdge]) -> Vec<bool> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Fresh,
        Open,
        Done,
    }
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); item_count];
    for (index, edge) in edges.iter().enumerate() {
        if edge.from < item_count {
            out[edge.from].push(index);
        }
    }
    let mut state = vec![State::Fresh; item_count];
    let mut reversed = vec![false; edges.len()];
    // Iterative DFS: (item, next outgoing edge to consider).
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for root in 0..item_count {
        if state[root] != State::Fresh {
            continue;
        }
        state[root] = State::Open;
        stack.push((root, 0));
        while let Some((item, cursor)) = stack.pop() {
            if cursor == out[item].len() {
                state[item] = State::Done;
                continue;
            }
            stack.push((item, cursor + 1));
            let edge = out[item][cursor];
            let target = edges[edge].to;
            match state.get(target) {
                Some(State::Fresh) => {
                    state[target] = State::Open;
                    stack.push((target, 0));
                }
                Some(State::Open) => reversed[edge] = true,
                _ => {}
            }
        }
    }
    reversed
}

/// Assigns each item the longest distance from any source (design spec §6.1).
fn longest_path(item_count: usize, edges: &[RawEdge]) -> Vec<usize> {
    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); item_count];
    let mut indegree = vec![0usize; item_count];
    for edge in edges {
        if edge.from == edge.to {
            continue;
        }
        incoming[edge.to].push(edge.from);
        indegree[edge.to] += 1;
    }
    let mut rank = vec![0usize; item_count];
    let mut ready: Vec<usize> = (0..item_count).filter(|&i| indegree[i] == 0).collect();
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); item_count];
    for edge in edges {
        if edge.from != edge.to {
            out[edge.from].push(edge.to);
        }
    }
    let mut settled = 0usize;
    while let Some(item) = ready.pop() {
        settled += 1;
        for &next in &out[item] {
            rank[next] = rank[next].max(rank[item] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.push(next);
            }
        }
    }
    debug_assert_eq!(settled, item_count, "cycle survived cycle breaking");
    rank
}

/// The starting order within each rank: breadth-first from the sources, in item order.
fn initial_order(layered: &Layered, edges: &[RawEdge], item_count: usize) -> Vec<Vec<usize>> {
    let depth = layered.vnodes.iter().map(|v| v.rank).max().unwrap_or(0);
    let mut ranks: Vec<Vec<usize>> = vec![Vec::new(); depth + 1];
    let mut placed = vec![false; layered.vnodes.len()];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); item_count];
    for edge in edges {
        out[edge.from].push(edge.to);
    }
    // Items first, breadth-first so connected items end up near each other.
    let mut queue: std::collections::VecDeque<usize> = (0..item_count)
        .filter(|&i| layered.vnodes[i].rank == 0)
        .collect();
    while let Some(item) = queue.pop_front() {
        if std::mem::replace(&mut placed[item], true) {
            continue;
        }
        ranks[layered.vnodes[item].rank].push(item);
        for &next in &out[item] {
            if !placed[next] {
                queue.push_back(next);
            }
        }
    }
    for item in 0..item_count {
        if !placed[item] {
            placed[item] = true;
            ranks[layered.vnodes[item].rank].push(item);
        }
    }
    // Dummies land next to the segment they continue, which keeps long edges straight.
    for (id, vnode) in layered.vnodes.iter().enumerate() {
        if matches!(vnode.kind, VKind::Dummy(_)) {
            ranks[vnode.rank].push(id);
        }
    }
    ranks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(from: usize, to: usize) -> RawEdge {
        RawEdge { from, to }
    }

    #[test]
    fn chain_ranks_increase() {
        let layered = build(3, &[edge(0, 1), edge(1, 2)]);
        assert_eq!(layered.vnodes[0].rank, 0);
        assert_eq!(layered.vnodes[1].rank, 1);
        assert_eq!(layered.vnodes[2].rank, 2);
        assert_eq!(layered.segs.len(), 2);
    }

    #[test]
    fn cycles_are_broken_at_the_back_edge() {
        let layered = build(3, &[edge(0, 1), edge(1, 2), edge(2, 0)]);
        assert_eq!(layered.reversed, vec![false, false, true]);
        assert_eq!(layered.ranks.len(), 3);
    }

    #[test]
    fn long_edges_get_dummies() {
        let layered = build(3, &[edge(0, 1), edge(1, 2), edge(0, 2)]);
        let dummies = layered
            .vnodes
            .iter()
            .filter(|v| matches!(v.kind, VKind::Dummy(_)))
            .count();
        assert_eq!(dummies, 1);
        assert_eq!(layered.segs.len(), 4);
    }

    #[test]
    fn layering_is_deterministic() {
        let edges = [edge(0, 2), edge(1, 2), edge(2, 3), edge(3, 1)];
        let a = build(4, &edges);
        let b = build(4, &edges);
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }
}
