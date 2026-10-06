// SPDX-License-Identifier: MIT
//! Entry stubs: how an edge reaches a node inside a container when a straight line
//! from the frame to it would cross another box.
//!
//! An edge that ends inside a container normally carries on through the frame to its
//! node in a straight line. When another box stands in that line, the edge is split
//! at a one-cell stub node placed in the container itself: the outer half ends at the
//! stub, and the container lays out the inner half like any of its own edges, around
//! whatever is in the way. A stub for an edge entering the container sits on its first
//! rank, and one for an edge leaving it on its last rank, so the frame side the outer
//! half crosses is the near one.
//!
//! Which edges are blocked is only known once the container is drawn, so [`super::draw`]
//! draws, splits the blocked edges, and draws again. A split edge can be blocked again
//! one container further in, which the next round splits in turn.

use super::spec::{EdgeSpec, GraphSpec, GroupSpec, NodeIdx, Terminator};

/// A stub node added to a spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Stub {
    /// The node.
    pub node: NodeIdx,
    /// True for the stub of an edge leaving its container, which takes the last rank.
    pub sink: bool,
}

/// An edge whose straight line to its node inside a container is blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Blocked {
    /// Index into the spec's edges.
    pub edge: usize,
    /// True when the blocked end is the edge's target.
    pub inwards: bool,
}

/// `spec` with every `blocked` edge split at a new stub, which is added to `stubs`.
pub(super) fn split(spec: &GraphSpec, blocked: &[Blocked], stubs: &mut Vec<Stub>) -> GraphSpec {
    let mut out = spec.clone();
    let mut seen = Vec::new();
    for &Blocked { edge, inwards } in blocked {
        if seen.contains(&(edge, inwards)) {
            continue;
        }
        seen.push((edge, inwards));
        let Some(whole) = out.edges.get(edge).cloned() else {
            continue;
        };
        let (inner, outer) = if inwards {
            (whole.to, whole.from)
        } else {
            (whole.from, whole.to)
        };
        let (Some(inner_path), Some(outer_path)) =
            (path_to(&out.root, inner), path_to(&out.root, outer))
        else {
            continue;
        };
        let shared = inner_path
            .iter()
            .zip(&outer_path)
            .take_while(|(a, b)| a == b)
            .count();
        if shared >= inner_path.len() {
            continue;
        }
        // The container the edge enters: the one below the closest common ancestor.
        let Some(container) = group_at(&mut out.root, &inner_path[..=shared]) else {
            continue;
        };
        let stub = NodeIdx(out.node_count);
        out.node_count += 1;
        container.nodes.push(stub);
        stubs.push(Stub {
            node: stub,
            sink: !inwards,
        });
        let (outer_half, inner_half) = if inwards {
            (
                EdgeSpec {
                    to: stub,
                    head: Terminator::None,
                    head_label: None,
                    ..whole.clone()
                },
                EdgeSpec {
                    from: stub,
                    tail: Terminator::None,
                    label: Default::default(),
                    tail_label: None,
                    ..whole
                },
            )
        } else {
            (
                EdgeSpec {
                    from: stub,
                    tail: Terminator::None,
                    tail_label: None,
                    ..whole.clone()
                },
                EdgeSpec {
                    to: stub,
                    head: Terminator::None,
                    label: Default::default(),
                    head_label: None,
                    ..whole
                },
            )
        };
        out.edges[edge] = outer_half;
        out.edges.push(inner_half);
    }
    out
}

/// The child indices leading from `group` to the group that holds `node` directly.
fn path_to(group: &GroupSpec, node: NodeIdx) -> Option<Vec<usize>> {
    if group.nodes.contains(&node) {
        return Some(Vec::new());
    }
    group.children.iter().enumerate().find_map(|(at, child)| {
        path_to(child, node).map(|mut path| {
            path.insert(0, at);
            path
        })
    })
}

/// The group `path` leads to from `group`.
fn group_at<'a>(group: &'a mut GroupSpec, path: &[usize]) -> Option<&'a mut GroupSpec> {
    match path.split_first() {
        None => Some(group),
        Some((&first, rest)) => group_at(group.children.get_mut(first)?, rest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::ast::Direction;

    #[test]
    fn a_blocked_edge_is_split_at_a_stub_in_the_container_it_enters() {
        let mut spec = GraphSpec::new(Direction::TopToBottom);
        spec.node_count = 3;
        spec.root.nodes = vec![NodeIdx(0)];
        spec.root.children = vec![GroupSpec {
            nodes: vec![NodeIdx(1), NodeIdx(2)],
            ..GroupSpec::default()
        }];
        spec.edges = vec![
            EdgeSpec::arrow(NodeIdx(1), NodeIdx(2)),
            EdgeSpec::arrow(NodeIdx(0), NodeIdx(2)),
        ];
        let mut stubs = Vec::new();
        let out = split(
            &spec,
            &[Blocked {
                edge: 1,
                inwards: true,
            }],
            &mut stubs,
        );
        assert_eq!(
            stubs,
            [Stub {
                node: NodeIdx(3),
                sink: false
            }]
        );
        assert_eq!(out.node_count, 4);
        assert_eq!(
            out.root.children[0].nodes,
            [NodeIdx(1), NodeIdx(2), NodeIdx(3)]
        );
        assert_eq!(
            (out.edges[1].from, out.edges[1].to),
            (NodeIdx(0), NodeIdx(3))
        );
        assert_eq!(out.edges[1].head, Terminator::None);
        assert_eq!(
            (out.edges[2].from, out.edges[2].to),
            (NodeIdx(3), NodeIdx(2))
        );
        assert_eq!(out.edges[2].head, Terminator::Arrow);
    }
}
