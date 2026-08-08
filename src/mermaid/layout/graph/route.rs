//! Orthogonal edge routing on the character grid.
//!
//! Routing happens in *flow space*: one axis runs along the graph direction ("flow"),
//! the other across it ("cross"). Every direction the engine supports is a rotation or
//! a mirror of that space, so there is exactly one router (design spec §6.1: all of
//! `TD`, `TB`, `LR`, `RL`, `BT`).
//!
//! An edge leaves its source box through a *port* on the border, runs along the flow
//! axis into the gap between two ranks, jogs sideways along a reserved *channel*, and
//! runs on into the target's port. Channels are allocated by interval colouring, so two
//! edges share a channel only when their sideways runs cannot touch, and a gap is
//! exactly as deep as its channels, labels and terminators need it to be.

mod plan;

use super::frame::Pen;
use super::glyph::Stroke;
use super::rank::{Layered, VKind};
use super::spec::{PortPolicy, Terminator};

use plan::{assign_ports, plan_gap, stack_ranks};

/// An edge as the router sees it: already resolved to items of this level.
#[derive(Debug, Clone)]
pub(super) struct LevelEdge {
    /// Stroke to draw the line in.
    pub stroke: Stroke,
    /// Terminator at the source end.
    pub tail: Terminator,
    /// Terminator at the target end.
    pub head: Terminator,
    /// The mid label, already split into lines.
    pub label: Vec<String>,
    /// A short note drawn beside the source end.
    pub tail_label: Option<String>,
    /// A short note drawn beside the target end.
    pub head_label: Option<String>,
    /// Preferred cross offset of the source port, relative to the source item.
    pub from_hint: usize,
    /// Preferred cross offset of the target port, relative to the target item.
    pub to_hint: usize,
}

/// Everything the router needs to know about the placed graph.
pub(super) struct Input<'a> {
    /// The layered graph.
    pub layered: &'a Layered,
    /// Cross offset of every virtual node.
    pub cross: &'a [usize],
    /// Cross extent of every virtual node's box.
    pub cross_size: &'a [usize],
    /// Flow extent of every virtual node's box; dummies are zero.
    pub flow_size: &'a [usize],
    /// Port policy of every virtual node.
    pub ports: &'a [PortPolicy],
    /// The level's edges, indexed by [`Seg::edge`](super::rank::Seg::edge).
    pub edges: &'a [LevelEdge],
    /// Self edges, as `(item, edge)` pairs.
    pub loops: &'a [(usize, usize)],
    /// Extra flow cells reserved below a virtual node for its self loop.
    pub loop_pad: &'a [usize],
    /// Smallest allowed gap between two ranks.
    pub min_gap: usize,
    /// True when the flow axis runs vertically, which decides whether a label is
    /// wide-and-short or tall-and-narrow in flow space.
    pub vertical: bool,
}

/// The routed layout: where everything sits in flow space.
pub(super) struct Routing {
    /// Flow offset of every virtual node's box.
    pub flow: Vec<usize>,
    /// Total extent along the flow axis.
    pub total_flow: usize,
    /// How far across the flow axis the edge labels and end notes reach, so the caller
    /// can widen the canvas enough that no label is ever clipped.
    pub cross_extent: usize,
    /// First flow cell after each rank's band, i.e. where its gap starts.
    band_end: Vec<usize>,
    gaps: Vec<Gap>,
    routes: Vec<Route>,
}

/// The reserved space between two consecutive ranks.
#[derive(Debug, Clone, Default)]
pub(super) struct Gap {
    pub size: usize,
    pub tail_len: usize,
    pub head_len: usize,
    /// Flow cells reserved for end notes at the source and target ends.
    pub tail_note: usize,
    pub head_note: usize,
    pub channels: usize,
    pub label_base: usize,
    pub label_size: usize,
}

/// One routed segment.
#[derive(Debug, Clone)]
pub(super) struct Route {
    pub src: usize,
    pub dst: usize,
    pub channel: Option<usize>,
    pub label: Option<(usize, usize)>,
}

impl Routing {
    /// Plans ports, channels, labels and rank offsets.
    pub(super) fn compute(input: &Input<'_>) -> Self {
        let ports = assign_ports(input);
        let mut routes: Vec<Route> = (0..input.layered.segs.len())
            .map(|index| Route {
                src: ports.out_port[index],
                dst: ports.in_port[index],
                channel: None,
                label: None,
            })
            .collect();
        let rank_count = input.layered.ranks.len();
        let mut gaps = vec![Gap::default(); rank_count.saturating_sub(1)];
        for (rank, gap) in gaps.iter_mut().enumerate() {
            let members: Vec<usize> = input
                .layered
                .segs
                .iter()
                .enumerate()
                .filter(|(_, seg)| input.layered.vnodes[seg.a].rank == rank)
                .map(|(index, _)| index)
                .collect();
            plan_gap(input, &members, &mut routes, gap);
        }
        let (flow, band_end, total_flow) = stack_ranks(input, &gaps);
        let cross_extent = label_extent(input, &routes);
        Self {
            flow,
            total_flow,
            cross_extent,
            band_end,
            gaps,
            routes,
        }
    }

    /// Draws every edge, dummy run and self loop with `pen`.
    pub(super) fn paint(&self, input: &Input<'_>, pen: &mut Pen) {
        for (index, seg) in input.layered.segs.iter().enumerate() {
            self.paint_segment(input, pen, index, seg.a, seg.b);
        }
        for (id, vnode) in input.layered.vnodes.iter().enumerate() {
            let VKind::Dummy(edge) = vnode.kind else {
                continue;
            };
            let band = band_size(input, vnode.rank);
            pen.run_flow(
                self.flow[id],
                input.cross[id],
                band,
                input.edges[edge].stroke,
            );
        }
        for &(item, edge) in input.loops {
            self.paint_loop(input, pen, item, edge);
        }
    }

    /// Draws one unit-length edge piece from `a` down into `b`.
    fn paint_segment(&self, input: &Input<'_>, pen: &mut Pen, index: usize, a: usize, b: usize) {
        let route = &self.routes[index];
        let edge = &input.edges[input.layered.segs[index].edge];
        let rank = input.layered.vnodes[a].rank;
        let gap = &self.gaps[rank];
        let forward = pen.frame.forward();
        let is_source = is_item(input, a);
        let exit = if is_source {
            self.flow[a] + input.flow_size[a].max(1) - 1
        } else {
            self.band_end[rank] - 1
        };
        let tail = if is_source {
            edge.tail
        } else {
            Terminator::None
        };
        let head = if is_item(input, b) {
            edge.head
        } else {
            Terminator::None
        };
        let head_len = head.len(forward);
        // Reach out of the source border so the junction glyph merges into it.
        pen.run_flow(exit, route.src, 1, edge.stroke);
        if !tail.is_none() {
            pen.terminator(
                exit + 1,
                route.src,
                tail.glyphs(forward.flip()),
                edge.stroke,
                false,
            );
        }
        // The run starts at the terminator's own cell: its glyph replaces the mask
        // there, but the connection onwards is kept, so no corner is ever left open.
        let body = exit + 1;
        // The target's first cell is its border; a terminator sits just before it.
        let entry = self.flow[b];
        let head_at = entry.saturating_sub(head_len);
        match route.channel {
            None => pen.run_flow(body, route.src, head_at.saturating_sub(body), edge.stroke),
            Some(channel) => {
                let at = self.band_end[rank] + gap.tail_len + gap.tail_note + channel;
                pen.run_flow(body, route.src, at.saturating_sub(body), edge.stroke);
                let (lo, hi) = (route.src.min(route.dst), route.src.max(route.dst));
                pen.run_cross(at, lo, hi - lo, edge.stroke);
                pen.run_flow(at, route.dst, head_at.saturating_sub(at), edge.stroke);
            }
        }
        if head_len > 0 {
            pen.terminator(head_at, route.dst, head.glyphs(forward), edge.stroke, true);
        }
        if let Some((flow, cross)) = route.label {
            pen.label(self.band_end[rank] + flow, cross, &edge.label);
        }
        // End notes — class-diagram cardinalities and the like — sit just outside the
        // terminator they belong to.
        if let Some(note) = edge.tail_label.as_ref().filter(|_| is_source) {
            let at = self.band_end[rank] + gap.tail_len;
            pen.label(at, route.src + 1, std::slice::from_ref(note));
        }
        if let Some(note) = edge
            .head_label
            .as_ref()
            .filter(|_| head_len > 0 || is_item(input, b))
        {
            let at = head_at.saturating_sub(gap.head_note);
            pen.label(at, route.dst + 1, std::slice::from_ref(note));
        }
    }

    /// Draws a self edge as a loop hanging off the item's cross-positive side.
    fn paint_loop(&self, input: &Input<'_>, pen: &mut Pen, item: usize, edge: usize) {
        let spec = &input.edges[edge];
        let top = self.flow[item];
        let height = input.flow_size[item].max(1);
        let right = input.cross[item] + input.cross_size[item] - 1;
        // Re-enter right of centre, clear of the port a forward edge would use, so the
        // return rail never has to cross an outgoing line.
        let centre = input.cross[item] + 1 + 3 * input.cross_size[item].saturating_sub(2) / 4;
        let mid = top + height / 2;
        let arrow = top + height;
        let rail = arrow + 1;
        pen.run_cross(mid, right, 3, spec.stroke);
        pen.run_flow(mid, right + 3, rail - mid, spec.stroke);
        pen.run_cross(rail, centre, right + 3 - centre, spec.stroke);
        pen.run_flow(arrow, centre, 1, spec.stroke);
        if !spec.head.is_none() {
            let back = pen.frame.forward().flip();
            pen.terminator(arrow, centre, spec.head.glyphs(back), spec.stroke, false);
        }
    }
}

/// How far across the flow axis the labels and end notes reach.
fn label_extent(input: &Input<'_>, routes: &[Route]) -> usize {
    let across = |text: &str| {
        if input.vertical {
            crate::text::display_width(text)
        } else {
            1
        }
    };
    let mut extent = 0usize;
    for (index, seg) in input.layered.segs.iter().enumerate() {
        let edge = &input.edges[seg.edge];
        let route = &routes[index];
        // Across the flow axis a label is as wide as its widest line when the graph
        // runs down the page, and as tall as its line count when it runs across.
        let widest = if input.vertical {
            edge.label
                .iter()
                .map(|line| crate::text::display_width(line))
                .max()
                .unwrap_or(0)
        } else {
            edge.label.len()
        };
        if widest > 0 {
            extent = extent.max(route.dst + 1 + widest);
        }
        if let Some(note) = &edge.tail_label {
            extent = extent.max(route.src + 1 + across(note));
        }
        if let Some(note) = &edge.head_label {
            extent = extent.max(route.dst + 1 + across(note));
        }
    }
    extent
}

/// True when `vnode` is a real item rather than a routing dummy.
pub(super) fn is_item(input: &Input<'_>, vnode: usize) -> bool {
    matches!(input.layered.vnodes[vnode].kind, VKind::Item(_))
}

/// The flow extent of a whole rank, including any self-loop reservation.
pub(super) fn band_size(input: &Input<'_>, rank: usize) -> usize {
    input.layered.ranks[rank]
        .iter()
        .map(|&id| input.flow_size[id] + input.loop_pad[id])
        .max()
        .unwrap_or(1)
        .max(1)
}
