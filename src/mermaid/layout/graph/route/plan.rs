//! Planning half of the router: ports, channels, label bands and rank offsets.
//!
//! Nothing here draws; it only decides *where* things go, which keeps the painting
//! half in [`super::route`] short enough to read in one go.

use super::super::glyph::{Dir, Stroke};
use super::super::spec::PortPolicy;
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
                    let hint = if outgoing {
                        edge.from_hint
                    } else {
                        edge.to_hint
                    };
                    // Aim at the far end, but prefer the caller's hint when the item is
                    // a frame around the real endpoint.
                    let far = input.cross[other] + input.cross_size[other] / 2;
                    (input.cross[id] + hint + far) / 2
                })
                .collect();
            let placed = spread(&members, &wanted, input, id);
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

/// Places `members` along one side of virtual node `id`.
///
/// Ports keep the order of their wanted positions and are spread evenly when they do
/// not fit distinctly, so an edge fan looks regular rather than merely legal.
fn spread(members: &[usize], wanted: &[usize], input: &Input<'_>, id: usize) -> Vec<usize> {
    let start = input.cross[id];
    let size = input.cross_size[id].max(1);
    let centre = start + size / 2;
    let count = members.len();
    let usable = size.saturating_sub(2);
    // Ports need a cell of air between them; when the side is too narrow for that the
    // edges share one port and merge into a single stem, which reads far better than a
    // row of touching junctions.
    if input.ports[id] == PortPolicy::Center || size < 3 || count == 0 || count * 2 > usable {
        return vec![centre; count];
    }
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by_key(|&i| (wanted[i], members[i]));
    let mut out = vec![centre; count];
    for (slot, &i) in order.iter().enumerate() {
        out[i] = start + 1 + (usable * (2 * slot + 1)) / (2 * count);
    }
    // Keep the ports ordered and inside the side. When there are more edges than
    // cells they share the last port, which merges into a single junction rather than
    // spilling onto a corner glyph.
    let first = start + 1;
    let last = start + size - 2;
    let mut previous: Option<usize> = None;
    for &i in &order {
        let floor = previous.map_or(first, |p| (p + 1).min(last));
        out[i] = out[i].clamp(floor, last);
        previous = Some(out[i]);
    }
    out
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
    let labelled: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&index| !input.edges[input.layered.segs[index].edge].label.is_empty())
        .collect();
    let mut spans = Vec::with_capacity(labelled.len());
    let mut extents = Vec::with_capacity(labelled.len());
    for &index in &labelled {
        let edge = &input.edges[input.layered.segs[index].edge];
        let text = edge
            .label
            .iter()
            .map(|line| crate::text::display_width(line))
            .max()
            .unwrap_or(0);
        let (across, along) = if input.vertical {
            (text, edge.label.len())
        } else {
            (edge.label.len(), text)
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
