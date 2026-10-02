// SPDX-License-Identifier: MIT
//! Planning half of the router: ports, channels, label bands and rank offsets.
//!
//! Nothing here draws; it only decides *where* things go, which keeps the painting
//! half in [`super::route`] short enough to read in one go.

use super::super::glyph::{Dir, Stroke};
use super::super::spec::{PortPolicy, Terminator};
use super::{Gap, Input, Route, SideCell, band_size, is_item};

/// Port cross coordinates, by segment.
pub(super) struct Ports {
    pub out_port: Vec<usize>,
    pub in_port: Vec<usize>,
}

/// Distributes ports over the exit and entry sides of every virtual node.
pub(super) fn assign_ports(input: &Input<'_>) -> Ports {
    let count = input.layered.segs.len();
    let mut ports = Ports {
        out_port: vec![0; count],
        in_port: vec![0; count],
    };
    for id in 0..input.layered.vnodes.len() {
        for outgoing in [true, false] {
            let members: Vec<usize> = input
                .layered
                .segs
                .iter()
                .enumerate()
                .filter(|(_, seg)| if outgoing { seg.a == id } else { seg.b == id })
                .map(|(index, _)| index)
                .collect();
            if members.is_empty() {
                continue;
            }
            let wanted: Vec<usize> = members
                .iter()
                .map(|&index| {
                    let seg = &input.layered.segs[index];
                    let edge = &input.edges[seg.edge];
                    let other = if outgoing { seg.b } else { seg.a };
                    let reach = if outgoing {
                        edge.from_reach
                    } else {
                        edge.to_reach
                    };
                    // A container aims at the real endpoint inside it, which is what
                    // lets the line carry on through the frame and reach the node.
                    if reach.is_some() {
                        let hint = if outgoing {
                            edge.from_hint
                        } else {
                            edge.to_hint
                        };
                        return input.cross[id] + hint;
                    }
                    // Where the two boxes overlap across the flow, both ends aim at the
                    // middle of the overlap and the edge becomes a straight run with no
                    // jog at all. Otherwise the port stays centred on its own box,
                    // which is where an arrowhead reads best.
                    let lo = input.cross[id].max(input.cross[other]);
                    let hi = (input.cross[id] + input.cross_size[id])
                        .min(input.cross[other] + input.cross_size[other]);
                    if lo < hi {
                        (lo + hi) / 2
                    } else {
                        input.cross[id] + input.cross_size[id] / 2
                    }
                })
                .collect();
            let placed = spread(&members, &wanted, input, id, outgoing);
            for (&index, &at) in members.iter().zip(&placed) {
                if outgoing {
                    ports.out_port[index] = at;
                } else {
                    ports.in_port[index] = at;
                }
            }
        }
    }
    ports
}

/// How many different terminators the edges on one side draw where they meet the node.
///
/// `None` counts as one of them: a plain line meeting the border is as much a piece of
/// notation as a diamond is, and merging it with a diamond loses it just the same.
fn distinct_terminators(members: &[usize], input: &Input<'_>, outgoing: bool) -> usize {
    let mut seen: Vec<Terminator> = Vec::new();
    for &index in members {
        let edge = &input.edges[input.layered.segs[index].edge];
        let terminator = if outgoing { edge.tail } else { edge.head };
        if !seen.contains(&terminator) {
            seen.push(terminator);
        }
    }
    seen.len()
}

/// Places `members` along one side of virtual node `id`.
///
/// Each port is first aimed straight at what it connects to, so an edge that could be
/// a straight run becomes one instead of jogging a cell or two; ports are then nudged
/// apart in order. When the side is too narrow to hold them with a cell of air between
/// them the edges share one port and merge into a single stem, which reads far better
/// than a row of touching junctions.
///
/// Merging is only ever a matter of style, though, and only while the edges agree
/// about what they draw where they meet the node. A flowchart fan is a bus because
/// every edge ends in the same arrowhead; a class node's relations end in a triangle,
/// a filled diamond, a hollow diamond and a plain line, and those glyphs *are* the
/// meaning (design spec §6.3, §6.4). So when the edges on one side carry different
/// terminators they are given distinct ports as long as any remain, and only share
/// one when the side genuinely runs out of cells.
///
/// A container is different: its edges really end at nodes inside it, and an edge only
/// reaches its node if its port sits over that node. So the edges are grouped by the
/// node they aim at and each group is spread over that node's own span, clear of the
/// title letters in the frame.
fn spread(
    members: &[usize],
    wanted: &[usize],
    input: &Input<'_>,
    id: usize,
    outgoing: bool,
) -> Vec<usize> {
    let whole = (input.cross[id], input.cross_size[id].max(1));
    let span_of = |index: usize| -> Option<(usize, usize)> {
        let edge = &input.edges[input.layered.segs[index].edge];
        let reach = if outgoing {
            edge.from_reach
        } else {
            edge.to_reach
        };
        reach.map(|reach| (reach.lo, reach.hi))
    };
    let spans: Vec<Option<(usize, usize)>> = members.iter().map(|&m| span_of(m)).collect();
    if !is_item(input, id) || spans.iter().all(Option::is_none) {
        return spread_over(members, wanted, input, id, outgoing, whole, true);
    }
    let mut out = vec![0; members.len()];
    let mut done = vec![false; members.len()];
    for first in 0..members.len() {
        if done[first] {
            continue;
        }
        let span = spans[first];
        let group: Vec<usize> = (first..members.len())
            .filter(|&i| !done[i] && spans[i] == span)
            .collect();
        let sub_members: Vec<usize> = group.iter().map(|&i| members[i]).collect();
        let sub_wanted: Vec<usize> = group.iter().map(|&i| wanted[i]).collect();
        let placed = match span {
            None => spread_over(&sub_members, &sub_wanted, input, id, outgoing, whole, true),
            Some((lo, hi)) => {
                let side = (input.cross[id] + lo, hi.saturating_sub(lo).max(1));
                let clear = clear_stretch(input, id, outgoing, side);
                let mut placed =
                    spread_over(&sub_members, &sub_wanted, input, id, outgoing, clear, false);
                nudge_off_title(&mut placed, input, id, outgoing, side);
                placed
            }
        };
        for (&i, at) in group.iter().zip(placed) {
            done[i] = true;
            out[i] = at;
        }
    }
    out
}

/// Places `members` along the stretch `(start, size)` of one side of `id`.
///
/// `whole` says the stretch is the entire side, which is when a self loop's cell and
/// the box's internal rules have to be kept clear.
fn spread_over(
    members: &[usize],
    wanted: &[usize],
    input: &Input<'_>,
    id: usize,
    outgoing: bool,
    (start, size): (usize, usize),
    whole: bool,
) -> Vec<usize> {
    let centre = start + size / 2;
    let count = members.len();
    // A self loop owns the last cell of the side; forward ports keep off it.
    let looped = whole && input.loops.iter().any(|&(item, _)| item == id);
    let size = size - usize::from(looped && size > 3);
    let usable = size.saturating_sub(2);
    if input.ports[id] == PortPolicy::Center || size < 3 || count == 0 {
        return vec![centre; count];
    }
    // Merging is a style choice while the edges agree about their terminator: one
    // stem reads better than a row of touching junctions, so it is taken as soon as
    // the ports would lose their cell of air. When the terminators differ, merging
    // destroys meaning instead of tidying it, so the ports spread as far as the side
    // allows and only the overflow — if any — shares a cell.
    let varied = distinct_terminators(members, input, outgoing) > 1;
    if !varied && count * 2 > usable {
        return vec![centre; count];
    }
    let (first, last) = (start + 1, start + size - 2);
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by_key(|&i| (wanted[i], members[i]));
    let mut out = vec![centre; count];
    // A lone edge takes the aim it asked for, which is what turns a near-miss into a
    // straight run. Several edges on one side are spread evenly instead: aiming them
    // individually can bunch two arrowheads together, which reads far worse than a
    // regular fan.
    let aimed = count == 1;
    if aimed {
        for &i in &order {
            out[i] = wanted[i].clamp(first, last);
        }
    } else {
        for (slot, &i) in order.iter().enumerate() {
            out[i] = start + 1 + (usable * (2 * slot + 1)) / (2 * count);
        }
        let mut previous: Option<usize> = None;
        for &i in &order {
            let floor = previous.map_or(first, |p: usize| (p + 1).min(last));
            out[i] = out[i].clamp(floor, last);
            previous = Some(out[i]);
        }
    }
    if whole {
        nudge_off_rules(&mut out, &order, input, id, first, last);
    }
    out
}

/// The part of the stretch `(start, size)` that lies under plain frame.
///
/// Spreading a fan over the whole span of a node and then pushing the ports that hit
/// the title aside would bunch them up against its last letter. The fan is spread over
/// the longest run of frame cells instead; the stretch comes back unchanged when it
/// has none, and [`nudge_off_title`] then does what it can.
fn clear_stretch(
    input: &Input<'_>,
    id: usize,
    outgoing: bool,
    (start, size): (usize, usize),
) -> (usize, usize) {
    let side = if outgoing {
        &input.side_out[id]
    } else {
        &input.side_in[id]
    };
    let base = input.cross[id];
    let art = |at: usize| {
        side.get(at - base)
            .is_none_or(|&cell| cell == SideCell::Art)
    };
    let (first, last) = (start + 1, start + size.saturating_sub(2));
    let mut best: Option<(usize, usize)> = None;
    let mut run: Option<usize> = None;
    for at in first..=last + 1 {
        if at <= last && art(at) {
            run.get_or_insert(at);
        } else if let Some(from) = run.take()
            && best.is_none_or(|(a, b)| at - from > b - a + 1)
        {
            best = Some((from, at - 1));
        }
    }
    // `spread_over` keeps one cell off each end, which is the node's corner there; the
    // run's own ends are free to use, so it is handed over with a cell either side.
    best.map_or((start, size), |(a, b)| (a - 1, b - a + 3))
}

/// Moves ports off the letters of a container title, and off its blank margin when a
/// cell of frame is free nearby.
///
/// The search stays within the stretch `(start, size)`, the span of the node the edges
/// aim at. A cell of frame is best, since the crossing then shows as a junction; a
/// blank cell beside the title comes next; a cell no other port holds beats a shared
/// one. A port with nowhere to go stays put and its edge ends at the frame.
fn nudge_off_title(
    out: &mut [usize],
    input: &Input<'_>,
    id: usize,
    outgoing: bool,
    (start, size): (usize, usize),
) {
    let side = if outgoing {
        &input.side_out[id]
    } else {
        &input.side_in[id]
    };
    let base = input.cross[id];
    let cell = |at: usize| side.get(at - base).copied().unwrap_or(SideCell::Art);
    let (first, last) = (start + 1, (start + size).saturating_sub(2).max(start + 1));
    for i in 0..out.len() {
        let at = out[i];
        if cell(at) == SideCell::Art {
            continue;
        }
        let best = (1..size)
            .flat_map(|step| [at + step, at.wrapping_sub(step)])
            .filter(|&candidate| (first..=last).contains(&candidate))
            .filter(|&candidate| cell(candidate) < cell(at))
            .min_by_key(|&candidate| (cell(candidate), out.contains(&candidate)));
        if let Some(candidate) = best {
            out[i] = candidate;
        }
    }
}

/// Moves ports off any border cell that already carries an internal rule.
///
/// Only cells that are free and inside the side are considered, and a port that has
/// nowhere better to go simply stays put: keeping the edge is always worth more than
/// keeping the rule tidy.
fn nudge_off_rules(
    out: &mut [usize],
    order: &[usize],
    input: &Input<'_>,
    id: usize,
    first: usize,
    last: usize,
) {
    let rules = &input.ruled[id];
    if rules.iter().all(|ruled| !ruled) {
        return;
    }
    let start = input.cross[id];
    let ruled_at = |at: usize| rules.get(at - start).copied().unwrap_or(false);
    for &i in order {
        if !ruled_at(out[i]) {
            continue;
        }
        let taken = out.to_vec();
        let better = (1..=2)
            .flat_map(|step| [out[i] + step, out[i].saturating_sub(step)])
            .find(|&candidate| {
                (first..=last).contains(&candidate)
                    && !ruled_at(candidate)
                    && !taken.contains(&candidate)
            });
        if let Some(candidate) = better {
            out[i] = candidate;
        }
    }
}

/// Sizes one gap and assigns its channels and label bands.
pub(super) fn plan_gap(input: &Input<'_>, members: &[usize], routes: &mut [Route], gap: &mut Gap) {
    for &index in members {
        let seg = &input.layered.segs[index];
        let edge = &input.edges[seg.edge];
        if is_item(input, seg.a) {
            gap.tail_len = gap.tail_len.max(edge.tail.len(Dir::Down));
        }
        if is_item(input, seg.b) {
            gap.head_len = gap.head_len.max(edge.head.len(Dir::Down));
            gap.head_note = gap
                .head_note
                .max(note_extent(input, edge.head_label.as_deref()));
        }
        if is_item(input, seg.a) {
            gap.tail_note = gap
                .tail_note
                .max(note_extent(input, edge.tail_label.as_deref()));
        }
    }
    untangle_swaps(input, members, routes);
    let jogs: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&index| routes[index].src != routes[index].dst)
        .collect();
    let ends: Vec<(usize, usize)> = jogs
        .iter()
        .map(|&index| (routes[index].src, routes[index].dst))
        .collect();
    let (colours, channels) = stack_channels(&ends);
    for (&index, &channel) in jogs.iter().zip(&colours) {
        routes[index].channel = Some(channel);
    }
    gap.channels = channels;
    gap.label_base = gap.tail_len + gap.tail_note + channels;
    // A label belongs to the edge, not to the piece of it that happens to cross this
    // gap. An edge spanning several ranks is cut into one segment per gap, and every
    // one of them sees the same non-empty label — so the carrier has to be picked, or
    // the label is drawn once per rank crossed. It is the segment leaving the real
    // node, which is also the only segment a single-rank edge has: the label then sits
    // beside its source in every case, wherever the edge ends up going.
    let labelled: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&index| {
            let seg = &input.layered.segs[index];
            is_item(input, seg.a) && !input.edges[seg.edge].label.is_empty()
        })
        .collect();
    let mut spans = Vec::with_capacity(labelled.len());
    let mut extents = Vec::with_capacity(labelled.len());
    for &index in &labelled {
        let edge = &input.edges[input.layered.segs[index].edge];
        let text = edge.label.width();
        let (across, along) = if input.vertical {
            (text, edge.label.height())
        } else {
            (edge.label.height(), text)
        };
        let at = label_side(members, routes, index, across);
        spans.push((at, at + across));
        extents.push(along);
    }
    let (bands, band_count) = colour(&spans);
    let mut band_flow = vec![0usize; band_count];
    for (slot, &band) in bands.iter().enumerate() {
        band_flow[band] = band_flow[band].max(extents[slot]);
    }
    let mut offsets = Vec::with_capacity(band_count);
    let mut cursor = 0usize;
    for size in &band_flow {
        offsets.push(cursor);
        cursor += size;
    }
    gap.label_size = cursor;
    for (slot, &index) in labelled.iter().enumerate() {
        routes[index].label = Some((gap.label_base + offsets[bands[slot]], spans[slot].0));
    }
    let needed =
        gap.tail_len + gap.tail_note + gap.channels + gap.label_size + gap.head_note + gap.head_len;
    // A dashed or heavy edge needs at least one plain cell of line, or its stroke would
    // be hidden entirely behind the terminator.
    let styled = members
        .iter()
        .any(|&index| input.edges[input.layered.segs[index].edge].stroke != Stroke::Solid);
    gap.size = needed.max(input.min_gap) + usize::from(styled);
}

/// The first cross cell of the label carried by segment `index`, `across` cells wide.
///
/// The label rows lie below the channels, where every edge crossing the gap runs
/// straight along the flow at its target port. So any other edge whose port falls
/// within the label's columns, or in the cell just past them, would cut through the
/// text or run flush against it. The label sits after its own line unless such a
/// line is in the way there and not before the line, in which case it sits before it.
/// When both sides are blocked it stays after the line.
fn label_side(members: &[usize], routes: &[Route], index: usize, across: usize) -> usize {
    let own = routes[index].dst;
    let blocked = |lo: usize, hi: usize| {
        members.iter().any(|&other| {
            let at = routes[other].dst;
            other != index && at != own && (lo..=hi).contains(&at)
        })
    };
    let after = own + 1;
    if !blocked(after, after + across) {
        return after;
    }
    match own.checked_sub(across) {
        Some(before) if !blocked(before.saturating_sub(1), own.saturating_sub(1)) => before,
        _ => after,
    }
}

/// How many flow cells an end note occupies: one row when the flow runs down the
/// page, its full width when it runs across.
fn note_extent(input: &Input<'_>, note: Option<&str>) -> usize {
    match note {
        None => 0,
        Some(text) if input.vertical => usize::from(!text.is_empty()),
        Some(text) => crate::text::display_width(text),
    }
}

/// Gives every jog `(src, dst)` a channel, so that no two jogs ever share line.
///
/// Jogs whose sideways runs overlap get different channels, as in [`colour`]. On top
/// of that, a jog that ends in the column where another one starts runs below it:
/// the other edge then leaves that column before this one arrives, and the two read
/// as a step. The other way round both would run down the same cells between their
/// two channels, and the eye could no longer tell them apart. A jog that also
/// starts where the other ends cannot be ordered either way; [`untangle_swaps`]
/// moves such a port beforehand, and any longer cycle is cut where it is found.
fn stack_channels(ends: &[(usize, usize)]) -> (Vec<usize>, usize) {
    let count = ends.len();
    let span = |i: usize| (ends[i].0.min(ends[i].1), ends[i].0.max(ends[i].1));
    // `above[i]` lists the jogs that must take a channel before jog `i`.
    let above: Vec<Vec<usize>> = (0..count)
        .map(|i| {
            (0..count)
                .filter(|&j| j != i && ends[j].0 == ends[i].1)
                .collect()
        })
        .collect();
    let mut used: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut out = vec![0usize; count];
    let mut done = vec![false; count];
    for _ in 0..count {
        let pending = || (0..count).filter(|&i| !done[i]);
        let ready = pending()
            .filter(|&i| above[i].iter().all(|&j| done[j]))
            .min_by_key(|&i| (span(i), i));
        let Some(i) = ready.or_else(|| pending().min_by_key(|&i| (span(i), i))) else {
            break;
        };
        let floor = above[i]
            .iter()
            .filter(|&&j| done[j])
            .map(|&j| out[j] + 1)
            .max()
            .unwrap_or(0);
        let (lo, hi) = span(i);
        let fits = |taken: &Vec<(usize, usize)>| taken.iter().all(|&(a, b)| hi < a || lo > b);
        let chosen = (floor..used.len())
            .find(|&c| fits(&used[c]))
            .unwrap_or(used.len().max(floor));
        if chosen >= used.len() {
            used.resize(chosen + 1, Vec::new());
        }
        used[chosen].push((lo, hi));
        out[i] = chosen;
        done[i] = true;
    }
    (out, used.len())
}

/// Moves one port of every pair of jogs that swap columns, one ending where the other
/// starts and the other way round.
///
/// Two fans spread evenly over boxes that line up produce this when their edges
/// cross, and no channel order keeps such a pair apart (see [`stack_channels`]): it
/// is drawn as one loop. Moving the arrival port of either edge by one cell leaves a
/// single crossing. The port moves towards the edge's own source, onto a plain
/// border cell that no other line of the gap starts in and that keeps a cell of air
/// to every other arrival port; when neither edge has such a cell the pair stays as
/// it is.
fn untangle_swaps(input: &Input<'_>, members: &[usize], routes: &mut [Route]) {
    for (pos, &a) in members.iter().enumerate() {
        for &b in &members[pos + 1..] {
            let (ra, rb) = (&routes[a], &routes[b]);
            let swapped = ra.src != ra.dst && ra.dst == rb.src && rb.dst == ra.src;
            if !swapped {
                continue;
            }
            for index in [a, b] {
                if let Some(at) = shifted_port(input, members, routes, index) {
                    routes[index].dst = at;
                    break;
                }
            }
        }
    }
}

/// A free cell one step from the arrival port of segment `index`, nearer its source
/// first.
fn shifted_port(
    input: &Input<'_>,
    members: &[usize],
    routes: &[Route],
    index: usize,
) -> Option<usize> {
    let id = input.layered.segs[index].b;
    if !is_item(input, id) || input.ports[id] == PortPolicy::Center {
        return None;
    }
    let (src, dst) = (routes[index].src, routes[index].dst);
    let toward = if src < dst { dst - 1 } else { dst + 1 };
    let away = if src < dst {
        dst + 1
    } else {
        dst.checked_sub(1)?
    };
    let base = input.cross[id];
    let (first, last) = (base + 1, (base + input.cross_size[id]).checked_sub(2)?);
    let looped = input.loops.iter().any(|&(item, _)| item == id);
    let plain = |at: usize| {
        input.side_in[id]
            .get(at - base)
            .is_none_or(|&cell| cell == SideCell::Art)
            && !input.ruled[id].get(at - base).copied().unwrap_or(false)
    };
    let clear = |at: usize| {
        members.iter().all(|&other| {
            other == index || {
                let route = &routes[other];
                route.src != at && route.dst.abs_diff(at) > 1
            }
        })
    };
    [toward, away].into_iter().find(|&at| {
        (first..=last).contains(&at) && !(looped && at == last) && plain(at) && clear(at)
    })
}

/// Greedy interval colouring: intervals sharing a colour never overlap.
fn colour(spans: &[(usize, usize)]) -> (Vec<usize>, usize) {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&i| (spans[i].0, spans[i].1, i));
    let mut used: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut out = vec![0usize; spans.len()];
    for &i in &order {
        let (lo, hi) = spans[i];
        let mut chosen = None;
        for (colour, taken) in used.iter().enumerate() {
            if taken.iter().all(|&(a, b)| hi < a || lo > b) {
                chosen = Some(colour);
                break;
            }
        }
        let colour = chosen.unwrap_or_else(|| {
            used.push(Vec::new());
            used.len() - 1
        });
        used[colour].push((lo, hi));
        out[i] = colour;
    }
    (out, used.len())
}

/// Stacks the ranks along the flow axis, centring every box inside its band.
pub(super) fn stack_ranks(input: &Input<'_>, gaps: &[Gap]) -> (Vec<usize>, Vec<usize>, usize) {
    let mut flow = vec![0usize; input.layered.vnodes.len()];
    let mut band_end = Vec::with_capacity(input.layered.ranks.len());
    let mut cursor = 0usize;
    for (rank, members) in input.layered.ranks.iter().enumerate() {
        let band = band_size(input, rank);
        for &id in members {
            let size = input.flow_size[id];
            flow[id] = if size == 0 {
                cursor
            } else {
                // Centre the box in its band, but keep any self-loop reservation
                // directly below it.
                cursor + (band - size - input.loop_pad[id]) / 2
            };
        }
        cursor += band;
        band_end.push(cursor);
        cursor += gaps.get(rank).map_or(0, |gap| gap.size);
    }
    (flow, band_end, cursor)
}

#[cfg(test)]
mod tests {
    use super::stack_channels;

    #[test]
    fn a_jog_ending_where_another_starts_runs_below_it() {
        // Airlock 25 -> Web UI 46, hin-cred-mgr 46 -> HIN LDAP 76: drawn the other
        // way round, both run down column 46 between their channels.
        for ends in [[(25, 46), (46, 76)], [(46, 76), (25, 46)]] {
            let (channels, count) = stack_channels(&ends);
            let (arriving, leaving) = if ends[0].1 == 46 { (0, 1) } else { (1, 0) };
            assert!(
                channels[arriving] > channels[leaving],
                "{ends:?} -> {channels:?}"
            );
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn the_order_carries_along_a_chain() {
        let ends = [(5, 10), (10, 20), (20, 30)];
        let (channels, _) = stack_channels(&ends);
        assert!(
            channels[0] > channels[1] && channels[1] > channels[2],
            "{channels:?}"
        );
    }

    #[test]
    fn jogs_that_never_meet_share_a_channel() {
        let (channels, count) = stack_channels(&[(1, 5), (8, 12), (20, 14)]);
        assert_eq!(channels, vec![0, 0, 0]);
        assert_eq!(count, 1);
    }

    #[test]
    fn a_swapped_pair_still_gets_two_channels() {
        let (channels, count) = stack_channels(&[(15, 27), (27, 15)]);
        assert_ne!(channels[0], channels[1]);
        assert_eq!(count, 2);
    }
}
