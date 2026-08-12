//! Planning half of the router: ports, channels, label bands and rank offsets.
//!
//! Nothing here draws; it only decides *where* things go, which keeps the painting
//! half in [`super::route`] short enough to read in one go.

use super::super::glyph::{Dir, Stroke};
use super::super::spec::{PortPolicy, Terminator};
use super::{Gap, Input, Route, band_size, is_item};

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
fn spread(
    members: &[usize],
    wanted: &[usize],
    input: &Input<'_>,
    id: usize,
    outgoing: bool,
) -> Vec<usize> {
    let start = input.cross[id];
    let size = input.cross_size[id].max(1);
    let centre = start + size / 2;
    let count = members.len();
    // A self loop owns the last cell of the side; forward ports keep off it.
    let looped = input.loops.iter().any(|&(item, _)| item == id);
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
    nudge_off_rules(&mut out, &order, input, id, first, last);
    out
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
    let jogs: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&index| routes[index].src != routes[index].dst)
        .collect();
    let spans: Vec<(usize, usize)> = jogs
        .iter()
        .map(|&index| {
            let route = &routes[index];
            (route.src.min(route.dst), route.src.max(route.dst))
        })
        .collect();
    let (colours, channels) = colour(&spans);
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
        let at = routes[index].dst + 1;
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
        routes[index].label = Some((gap.label_base + offsets[bands[slot]], routes[index].dst + 1));
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

/// How many flow cells an end note occupies: one row when the flow runs down the
/// page, its full width when it runs across.
fn note_extent(input: &Input<'_>, note: Option<&str>) -> usize {
    match note {
        None => 0,
        Some(text) if input.vertical => usize::from(!text.is_empty()),
        Some(text) => crate::text::display_width(text),
    }
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
