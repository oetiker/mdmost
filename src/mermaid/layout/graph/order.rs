//! Crossing reduction: the median heuristic followed by adjacent-transpose passes.
//!
//! Four down/up sweeps are enough in practice and keep the 100-node case fast
//! (design spec §13). Every tie is broken by virtual-node id, so the result is a pure
//! function of the layered graph.

use super::rank::Layered;

/// How many median + transpose sweeps to run.
const SWEEPS: usize = 4;

/// Reorders the ranks of `layered` in place to reduce edge crossings.
pub(super) fn reduce(layered: &mut Layered) {
    if layered.ranks.len() < 2 {
        return;
    }
    let adjacency = Adjacency::new(layered);
    let mut best = layered.ranks.clone();
    let mut best_score = crossings(layered, &best);
    for sweep in 0..SWEEPS {
        let downwards = sweep % 2 == 0;
        median_pass(layered, &adjacency, downwards);
        transpose_pass(layered, &adjacency);
        let score = crossings(layered, &layered.ranks);
        if score < best_score {
            best_score = score;
            best = layered.ranks.clone();
        }
    }
    layered.ranks = best;
}

/// Neighbour lists of the layered graph, by virtual node.
struct Adjacency {
    up: Vec<Vec<usize>>,
    down: Vec<Vec<usize>>,
}

impl Adjacency {
    fn new(layered: &Layered) -> Self {
        let mut up = vec![Vec::new(); layered.vnodes.len()];
        let mut down = vec![Vec::new(); layered.vnodes.len()];
        for seg in &layered.segs {
            down[seg.a].push(seg.b);
            up[seg.b].push(seg.a);
        }
        for list in up.iter_mut().chain(down.iter_mut()) {
            list.sort_unstable();
        }
        Self { up, down }
    }

    fn side(&self, downwards: bool) -> &[Vec<usize>] {
        if downwards { &self.up } else { &self.down }
    }
}

/// The position of every virtual node within its rank.
fn positions(layered: &Layered) -> Vec<usize> {
    let mut pos = vec![0usize; layered.vnodes.len()];
    for rank in &layered.ranks {
        for (at, &vnode) in rank.iter().enumerate() {
            pos[vnode] = at;
        }
    }
    pos
}

/// Sorts each rank by the median position of its neighbours in the previous rank.
fn median_pass(layered: &mut Layered, adjacency: &Adjacency, downwards: bool) {
    let pos = positions(layered);
    let neighbours = adjacency.side(downwards);
    let order: Vec<usize> = if downwards {
        (1..layered.ranks.len()).collect()
    } else {
        (0..layered.ranks.len() - 1).rev().collect()
    };
    for rank in order {
        let mut keyed: Vec<(Option<usize>, usize, usize)> = layered.ranks[rank]
            .iter()
            .enumerate()
            .map(|(at, &vnode)| {
                let mut fixed: Vec<usize> = neighbours[vnode].iter().map(|&n| pos[n]).collect();
                fixed.sort_unstable();
                let median = match fixed.len() {
                    0 => None,
                    n => Some(fixed[n / 2] * 2 + usize::from(n % 2 == 0)),
                };
                (median, at, vnode)
            })
            .collect();
        // Nodes without neighbours keep their slot; the rest sort by median.
        let anchored: Vec<(usize, usize)> = keyed
            .iter()
            .filter(|(median, _, _)| median.is_none())
            .map(|&(_, at, vnode)| (at, vnode))
            .collect();
        keyed.retain(|(median, _, _)| median.is_some());
        keyed.sort_by_key(|&(median, at, _)| (median, at));
        let mut moving = keyed.into_iter().map(|(_, _, vnode)| vnode);
        let mut fresh = Vec::with_capacity(layered.ranks[rank].len());
        for slot in 0..layered.ranks[rank].len() {
            match anchored.iter().find(|&&(at, _)| at == slot) {
                Some(&(_, vnode)) => fresh.push(vnode),
                None => {
                    if let Some(vnode) = moving.next() {
                        fresh.push(vnode);
                    }
                }
            }
        }
        fresh.extend(moving);
        layered.ranks[rank] = fresh;
    }
}

/// Swaps adjacent pairs while that reduces the number of crossings.
fn transpose_pass(layered: &mut Layered, adjacency: &Adjacency) {
    let mut improved = true;
    let mut guard = 0;
    while improved && guard < 8 {
        improved = false;
        guard += 1;
        for rank in 0..layered.ranks.len() {
            for at in 0..layered.ranks[rank].len().saturating_sub(1) {
                let left = layered.ranks[rank][at];
                let right = layered.ranks[rank][at + 1];
                let before = pair_crossings(layered, adjacency, rank, left, right);
                let after = pair_crossings(layered, adjacency, rank, right, left);
                if after < before {
                    layered.ranks[rank].swap(at, at + 1);
                    improved = true;
                }
            }
        }
    }
}

/// Crossings contributed by two neighbouring nodes drawn in the order `left`, `right`.
fn pair_crossings(
    layered: &Layered,
    adjacency: &Adjacency,
    rank: usize,
    left: usize,
    right: usize,
) -> usize {
    let pos = positions(layered);
    let mut total = 0;
    for (side, enabled) in [(&adjacency.up, rank > 0), (&adjacency.down, true)] {
        if !enabled {
            continue;
        }
        for &l in &side[left] {
            for &r in &side[right] {
                if pos[r] < pos[l] {
                    total += 1;
                }
            }
        }
    }
    total
}

/// The total number of edge crossings in `ranks`.
fn crossings(layered: &Layered, ranks: &[Vec<usize>]) -> usize {
    let mut pos = vec![0usize; layered.vnodes.len()];
    for rank in ranks {
        for (at, &vnode) in rank.iter().enumerate() {
            pos[vnode] = at;
        }
    }
    let mut by_rank: Vec<Vec<(usize, usize)>> = vec![Vec::new(); ranks.len()];
    for seg in &layered.segs {
        let rank = layered.vnodes[seg.a].rank;
        if rank + 1 < ranks.len() {
            by_rank[rank].push((pos[seg.a], pos[seg.b]));
        }
    }
    let mut total = 0;
    for pairs in &mut by_rank {
        pairs.sort_unstable();
        for (i, &(_, bi)) in pairs.iter().enumerate() {
            for &(_, bj) in &pairs[i + 1..] {
                if bj < bi {
                    total += 1;
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::layout::graph::rank::{RawEdge, build};

    #[test]
    fn a_simple_crossing_is_removed() {
        // 0 -> 3, 1 -> 2 drawn in the order 0,1 / 2,3 crosses; swapping does not.
        let edges = [RawEdge { from: 0, to: 3 }, RawEdge { from: 1, to: 2 }];
        let mut layered = build(4, &edges);
        layered.ranks[1] = vec![2, 3];
        reduce(&mut layered);
        assert_eq!(crossings(&layered, &layered.ranks), 0);
    }

    #[test]
    fn reduction_is_deterministic() {
        let edges: Vec<RawEdge> = (0..6)
            .map(|i| RawEdge {
                from: i % 3,
                to: 3 + (i * 2) % 3,
            })
            .collect();
        let mut a = build(6, &edges);
        let mut b = build(6, &edges);
        reduce(&mut a);
        reduce(&mut b);
        assert_eq!(a.ranks, b.ranks);
    }
}
