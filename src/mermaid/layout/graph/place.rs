//! Coordinate assignment across the flow direction, on the character grid.
//!
//! Positions are integer cell offsets from the start of the cross axis — columns for a
//! top-down graph, rows for a left-right one. No floating point is used anywhere, so
//! there is nothing to round and nothing to drift.
//!
//! The method is the classic priority sweep: pack each rank tight, then repeatedly pull
//! every node towards the median of its neighbours, letting high-priority nodes push
//! low-priority ones aside. Routing dummies get the highest priority, which is what
//! keeps long edges straight instead of zig-zagging around boxes.

use super::rank::{Layered, VKind};

/// How many alignment sweeps to run.
const PASSES: usize = 4;

/// Assigns every virtual node its offset along the cross axis.
///
/// `sizes` is the extent of each virtual node across the flow direction, and `gap` the
/// minimum number of blank cells between two neighbours in the same rank. The returned
/// vector is indexed by virtual node id and is normalised so the leftmost node sits at
/// zero.
pub(super) fn assign(layered: &Layered, sizes: &[usize], gap: usize) -> Vec<usize> {
    let mut pos = vec![0usize; layered.vnodes.len()];
    for rank in &layered.ranks {
        let mut cursor = 0usize;
        for &vnode in rank {
            pos[vnode] = cursor;
            cursor += sizes[vnode] + gap;
        }
    }
    let priority = priorities(layered);
    let (up, down) = neighbours(layered);
    for pass in 0..PASSES {
        let downwards = pass % 2 == 0;
        let ranks: Vec<usize> = if downwards {
            (1..layered.ranks.len()).collect()
        } else {
            (0..layered.ranks.len().saturating_sub(1)).rev().collect()
        };
        let side = if downwards { &up } else { &down };
        for rank in ranks {
            align_rank(layered, &mut pos, sizes, gap, &priority, side, rank);
        }
    }
    normalise(&mut pos);
    pos
}

/// Neighbour lists towards lower and higher ranks.
fn neighbours(layered: &Layered) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let mut up = vec![Vec::new(); layered.vnodes.len()];
    let mut down = vec![Vec::new(); layered.vnodes.len()];
    for seg in &layered.segs {
        down[seg.a].push(seg.b);
        up[seg.b].push(seg.a);
    }
    (up, down)
}

/// Straightening priority: dummies outrank real items, then higher degree wins.
fn priorities(layered: &Layered) -> Vec<usize> {
    let mut degree = vec![0usize; layered.vnodes.len()];
    for seg in &layered.segs {
        degree[seg.a] += 1;
        degree[seg.b] += 1;
    }
    layered
        .vnodes
        .iter()
        .enumerate()
        .map(|(id, vnode)| match vnode.kind {
            VKind::Dummy(_) => usize::MAX / 2 + degree[id],
            VKind::Item(_) => degree[id],
        })
        .collect()
}

/// Pulls one rank towards the medians of its neighbours in `side`.
fn align_rank(
    layered: &Layered,
    pos: &mut [usize],
    sizes: &[usize],
    gap: usize,
    priority: &[usize],
    side: &[Vec<usize>],
    rank: usize,
) {
    let row = layered.ranks[rank].clone();
    let mut order: Vec<usize> = (0..row.len()).collect();
    order.sort_by_key(|&at| (std::cmp::Reverse(priority[row[at]]), at));
    for at in order {
        let vnode = row[at];
        let mut centres: Vec<usize> = side[vnode]
            .iter()
            .map(|&other| pos[other] + sizes[other] / 2)
            .collect();
        if centres.is_empty() {
            continue;
        }
        centres.sort_unstable();
        let centre = centres[centres.len() / 2];
        let want = centre.saturating_sub(sizes[vnode] / 2);
        match want.cmp(&pos[vnode]) {
            std::cmp::Ordering::Greater => {
                push_right(&row, pos, sizes, gap, priority, at, want);
            }
            std::cmp::Ordering::Less => {
                push_left(&row, pos, sizes, gap, priority, at, want);
            }
            std::cmp::Ordering::Equal => {}
        }
    }
}

/// Moves `row[at]` right to `want`, pushing lower-priority neighbours out of the way.
fn push_right(
    row: &[usize],
    pos: &mut [usize],
    sizes: &[usize],
    gap: usize,
    priority: &[usize],
    at: usize,
    want: usize,
) {
    let me = row[at];
    let mut wall = row.len();
    for (index, &other) in row.iter().enumerate().skip(at + 1) {
        if priority[other] >= priority[me] {
            wall = index;
            break;
        }
    }
    let mut span = sizes[me];
    for &other in &row[at + 1..wall] {
        span += gap + sizes[other];
    }
    let limit = if wall < row.len() {
        match pos[row[wall]].checked_sub(gap + span) {
            Some(limit) => limit,
            None => return,
        }
    } else {
        want
    };
    let target = want.min(limit);
    if target <= pos[me] {
        return;
    }
    pos[me] = target;
    let mut cursor = target + sizes[me] + gap;
    for &other in &row[at + 1..wall] {
        pos[other] = pos[other].max(cursor);
        cursor = pos[other] + sizes[other] + gap;
    }
}

/// Moves `row[at]` left to `want`, pushing lower-priority neighbours out of the way.
fn push_left(
    row: &[usize],
    pos: &mut [usize],
    sizes: &[usize],
    gap: usize,
    priority: &[usize],
    at: usize,
    want: usize,
) {
    let me = row[at];
    let mut wall = None;
    for index in (0..at).rev() {
        if priority[row[index]] >= priority[me] {
            wall = Some(index);
            break;
        }
    }
    let first = wall.map_or(0, |index| index + 1);
    let mut span = 0usize;
    for &other in &row[first..at] {
        span += sizes[other] + gap;
    }
    let floor = match wall {
        Some(index) => pos[row[index]] + sizes[row[index]] + gap + span,
        None => span,
    };
    let target = want.max(floor);
    if target >= pos[me] {
        return;
    }
    pos[me] = target;
    let mut cursor = target;
    for &other in row[first..at].iter().rev() {
        let want_left = cursor.saturating_sub(gap + sizes[other]);
        pos[other] = pos[other].min(want_left);
        cursor = pos[other];
    }
}

/// Shifts every position so the smallest one is zero.
fn normalise(pos: &mut [usize]) {
    let Some(&min) = pos.iter().min() else {
        return;
    };
    if min > 0 {
        for value in pos.iter_mut() {
            *value -= min;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::layout::graph::order;
    use crate::mermaid::layout::graph::rank::{RawEdge, build};

    #[test]
    fn a_chain_is_straight() {
        let mut layered = build(3, &[RawEdge { from: 0, to: 1 }, RawEdge { from: 1, to: 2 }]);
        order::reduce(&mut layered);
        let sizes = vec![7, 9, 5];
        let pos = assign(&layered, &sizes, 3);
        let centre = |id: usize| pos[id] + sizes[id] / 2;
        assert_eq!(centre(0), centre(1));
        assert_eq!(centre(1), centre(2));
    }

    #[test]
    fn siblings_do_not_overlap() {
        let edges = [
            RawEdge { from: 0, to: 1 },
            RawEdge { from: 0, to: 2 },
            RawEdge { from: 0, to: 3 },
        ];
        let mut layered = build(4, &edges);
        order::reduce(&mut layered);
        let sizes = vec![9, 7, 7, 7];
        let pos = assign(&layered, &sizes, 3);
        let mut spans: Vec<(usize, usize)> = layered.ranks[1]
            .iter()
            .map(|&id| (pos[id], pos[id] + sizes[id]))
            .collect();
        spans.sort_unstable();
        for pair in spans.windows(2) {
            assert!(pair[0].1 + 3 <= pair[1].0, "{spans:?}");
        }
    }
}
