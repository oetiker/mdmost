//! The shared layered graph layout engine.
//!
//! Flowchart, class, ER and state diagrams are all "boxes joined by edges", so they all
//! come through here (design spec §6.1, §6.3, §6.4, §6.7). A caller describes the
//! topology as a [`GraphSpec`] and supplies a [`NodeArt`] that knows how to draw one
//! node; the engine does cycle breaking, layer assignment, crossing reduction,
//! coordinate assignment on the character grid, orthogonal edge routing with junction
//! glyphs, container frames and the fit-to-width degradation ladder.
//!
//! ```no_run
//! use mdless::mermaid::ast::Direction;
//! use mdless::mermaid::layout::graph::{self, EdgeSpec, Fit, GraphSpec, NodeIdx};
//! use mdless::theme::Theme;
//! use mdless::canvas::Canvas;
//!
//! let theme = Theme::default_dark();
//! let mut spec = GraphSpec::new(Direction::TopToBottom);
//! spec.node_count = 2;
//! spec.root.nodes = vec![NodeIdx(0), NodeIdx(1)];
//! spec.edges.push(EdgeSpec::arrow(NodeIdx(0), NodeIdx(1)));
//! let art = |node: NodeIdx, _budget: u16, theme: &Theme| {
//!     Canvas::from_text(5, "  A  ", theme.base())
//!         .framed(Default::default(), theme.diagram.node_border, None, theme.base())
//! };
//! let canvas = graph::draw(&spec, &art, 40, &theme, Fit::COMPACT).expect("fits");
//! ```
//!
//! # Determinism
//!
//! Every stage is index-based and every tie is broken by index, so the same
//! `(GraphSpec, NodeArt, width, theme)` always produces exactly the same canvas
//! (design spec §13).

mod frame;
mod glyph;
mod ink;
mod order;
mod place;
mod rank;
mod route;
mod spec;

#[cfg(test)]
mod tests;

pub use glyph::{Dir, Stroke};
pub use spec::{EdgeSpec, GraphSpec, GroupSpec, NodeArt, NodeIdx, PortPolicy, Terminator};

use crate::canvas::{BorderSet, Canvas};
use crate::error::MermaidError;
use crate::mermaid::ast::Direction;
use crate::text::{Align, Line, Span};
use crate::theme::Theme;

use frame::{Frame, Pen};
use rank::RawEdge;
use route::{Input, LevelEdge, Reach, Routing};

/// Spacing tried in turn until the drawing fits the width budget.
///
/// Each step is `(cross gap, share of the width a node label may use)`; later steps
/// trade beauty for fit, and the last rung is the tightest *derived from the width*.
/// Exhausting the ladder is not the end of the search: [`draw`] then bisects the label
/// budget below the last rung's, so the ladder is a fast path rather than the floor.
///
/// The share caps one node, so it only bounds the whole drawing when the nodes stack
/// across the width — a `TD` chart. Laid out `LR` the boxes sit side by side and their
/// widths *add*, so a six-rank chart of ordinary labels can overrun 80 columns while
/// every single node is comfortably inside a quarter of it. The last two rungs are for
/// that case: they are tight enough to wrap a short label onto a second line, which is
/// the only lever this engine has to shorten a row of boxes. Nothing that fits at an
/// earlier rung ever reaches them, because the first fit wins.
const LADDER: &[(usize, u16)] = &[
    (3, 1),
    (3, 2),
    (2, 2),
    (2, 3),
    (1, 3),
    (1, 4),
    (1, 6),
    (1, 8),
];

/// How many rungs of [`LADDER`] a caller with somewhere to scroll may use.
///
/// The last two rungs are the word-breaking ones: they are tight enough to cut `Start`
/// into `Star`/`t`. See [`Fit::ROOMY`].
const ROOMY_RUNGS: usize = 6;

/// The narrowest label budget the engine will hand a node, at any width.
///
/// Every rung floors at this value, so a layout at `(1, MIN_BUDGET)` is the smallest
/// drawing the engine can produce for a chart — and, crucially, it does not depend on
/// `width` at all. That is what makes fit monotone: see [`draw`].
const MIN_BUDGET: u16 = 6;

/// The narrowest label budget a caller with somewhere to scroll will accept.
///
/// A node box spends four columns on its outline and padding, so this leaves ten for
/// the label text — enough for `Markdown`, `viewport` or `anchors` to survive whole.
/// It is a heuristic and openly a blunt one: a single word longer than ten columns is
/// still broken, and a chart of two-letter labels is widened long before it needs to be.
/// What it buys is that the *usual* label is not minced. See [`Fit::ROOMY`].
const ROOMY_BUDGET: u16 = 14;

/// How hard a caller is willing to let the engine degrade a drawing to make it fit.
///
/// The engine has always had one answer to "it does not fit": squeeze. That is right
/// when the alternative is a dump of Mermaid source — a pipe has nowhere to scroll —
/// and wrong when the caller can instead lay the diagram out wide and let the reader
/// scroll to it, because a diagram whose labels have been minced *looks like the
/// diagram is the information* while telling the reader nothing.
///
/// Both policies keep fit monotone in width, because both floor every rung's budget at
/// [`Fit::floor`] and probe that same width-independent floor before giving up. See
/// [`draw`] for why that is what monotonicity rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    /// The rungs of [`LADDER`] this policy may use, tightest last.
    ladder: &'static [(usize, u16)],
    /// The narrowest label budget the engine may hand a node under this policy.
    floor: u16,
}

impl Fit {
    /// Squeeze as hard as the engine can: the whole ladder, down to [`MIN_BUDGET`].
    ///
    /// For a caller whose only other option is dumping the source — `--render-once` and
    /// every diagram nested where the pager cannot widen it.
    pub const COMPACT: Self = Self {
        ladder: LADDER,
        floor: MIN_BUDGET,
    };

    /// Degrade only as far as a drawing stays worth looking at; be wide instead.
    ///
    /// For the pager's top-level fences, which can be laid out wider than the viewport
    /// and scrolled to. Two things are given up relative to [`Fit::COMPACT`], and both
    /// cost something:
    ///
    /// * the word-breaking rungs `(1, 6)` and `(1, 8)`, and
    /// * label budgets below [`ROOMY_BUDGET`].
    ///
    /// **The price:** a chart whose labels are all short enough that `(1, 6)` would have
    /// fitted it into the viewport *without breaking a single word* is now widened and
    /// scrolled instead. The engine is not told which labels broke — that would need
    /// information the [`NodeArt`] seam deliberately does not carry — so the policy is
    /// stated in columns rather than in words, and columns cannot tell the two cases
    /// apart.
    ///
    /// **Why the rungs alone would not do it:** dropping them changes nothing on their
    /// own. The bisection below the tightest rung runs from the floor upward and finds
    /// the *largest* budget that fits, so it recovers everything a dropped rung would
    /// have found. The floor is the part of this policy that has teeth; the rungs are
    /// dropped because leaving them in a policy that then refuses their budgets would be
    /// a lie about what the ladder is for.
    pub const ROOMY: Self = Self {
        ladder: LADDER.split_at(ROOMY_RUNGS).0,
        floor: ROOMY_BUDGET,
    };
}

/// Lays out `spec` into a canvas exactly `width` columns wide.
///
/// The drawing is centred in the canvas. Nodes are drawn by `art`, which is called
/// once per node per attempt at fitting the width budget.
///
/// The search runs `fit`'s rungs first and takes the first that fits, then — only if
/// every rung overflowed — bisects the label budget below the tightest rung's, down to
/// `fit`'s floor. The second phase exists because a rung's budget is `width / share`,
/// which *grows* with `width`: without it, one more column could hand every node a wider
/// budget, overshoot, and turn a chart that drew at some width into an error one column
/// wider.
///
/// Fit is therefore monotone in `width` — and the reason is the bisection's *first*
/// probe rather than the bisection. That probe is `(tightest gap, fit.floor)`, whose
/// drawing does not depend on `width` at all, so this function succeeds when a rung fits
/// **or** `width` is at least that floor drawing's width. The rung half is still not
/// monotone on its own — the rungs quantise exactly as they always did — but it is
/// absorbed: no rung is tighter than the floor probe in either gap or budget, and the
/// drawing only grows with each, so a rung that fits at `width` already means `width`
/// clears the floor. Success collapses to `width >= floor`, which is monotone whatever
/// the layout does in between. That holds for either [`Fit`], since both floor every
/// rung at their own floor. That the drawing really is nondecreasing in gap and budget
/// is an empirical claim about the layout, checked by
/// `tests/mermaid_layout_monotone.rs`.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when even the smallest drawing this policy
/// allows — the tightest spacing at `fit`'s floor — does not fit into `width`. `needed`
/// is then that drawing's width: a true floor for this policy, not merely the narrowest
/// attempt, and the exact width at which the diagram starts to draw.
pub fn draw(
    spec: &GraphSpec,
    art: &dyn NodeArt,
    width: u16,
    theme: &Theme,
    fit: Fit,
) -> Result<Canvas, MermaidError> {
    validate(spec)?;
    let attempt = |gap: usize, budget: u16| {
        Ctx {
            spec,
            art,
            theme,
            budget,
            gap,
        }
        .group(&spec.root, spec.direction)
        .canvas
    };
    let mut narrowest: Option<u16> = None;

    for &(gap, share) in fit.ladder {
        let canvas = attempt(gap, (width / share).max(fit.floor));
        if canvas.width() <= width {
            return Ok(centred(canvas, width, theme));
        }
        narrowest = narrower(narrowest, &canvas);
    }

    // The ladder is exhausted, so the tightest rung's budget is known not to fit and
    // bounds the search above. Below it sits the floor, which the last rung has already
    // drawn whenever the two coincide — at those widths there is nothing left to try.
    let (tightest_gap, tightest_share) = *fit.ladder.last().expect("the ladder has rungs");
    let mut hi = (width / tightest_share).max(fit.floor);
    if hi == fit.floor {
        return Err(MermaidError::TooNarrow {
            width,
            needed: narrowest,
        });
    }
    let mut best = attempt(tightest_gap, fit.floor);
    narrowest = narrower(narrowest, &best);
    if best.width() > width {
        return Err(MermaidError::TooNarrow {
            width,
            needed: narrowest,
        });
    }

    // A fit exists; spend a handful of layouts finding the most generous one, keeping
    // `MIN_BUDGET..=lo` fitting and `hi` overflowing.
    let mut lo = fit.floor;
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        let canvas = attempt(tightest_gap, mid);
        narrowest = narrower(narrowest, &canvas);
        if canvas.width() <= width {
            lo = mid;
            best = canvas;
        } else {
            hi = mid;
        }
    }
    Ok(centred(best, width, theme))
}

/// The narrower of `at` and `canvas`' width.
fn narrower(at: Option<u16>, canvas: &Canvas) -> Option<u16> {
    Some(at.map_or(canvas.width(), |at| at.min(canvas.width())))
}

/// Rejects a specification the engine cannot draw.
fn validate(spec: &GraphSpec) -> Result<(), MermaidError> {
    let mut seen = vec![false; spec.node_count];
    let mut stack = vec![&spec.root];
    while let Some(group) = stack.pop() {
        for node in &group.nodes {
            match seen.get_mut(node.0) {
                Some(flag) if !*flag => *flag = true,
                _ => {
                    return Err(MermaidError::Internal {
                        message: "node placed in more than one subgraph".to_string(),
                    });
                }
            }
        }
        stack.extend(&group.children);
    }
    if seen.iter().any(|placed| !placed) {
        return Err(MermaidError::Internal {
            message: "node missing from the container tree".to_string(),
        });
    }
    if spec
        .edges
        .iter()
        .any(|edge| edge.from.0 >= spec.node_count || edge.to.0 >= spec.node_count)
    {
        return Err(MermaidError::Internal {
            message: "edge refers to an unknown node".to_string(),
        });
    }
    Ok(())
}

/// The cross offsets along `canvas`' sides that a port should keep off.
///
/// A node whose art draws internal rules — a class box's compartments, an entity's
/// attribute table — shows a `├` or `┤` where a rule meets the border. An edge
/// attaching there turns the rule into a line that appears to flow out of the box, so
/// the router avoids those cells when it has a choice. This is read back off the
/// drawn node rather than declared, so it works for any caller without widening the
/// [`NodeArt`] seam.
fn ruled_offsets(canvas: &Canvas, vertical: bool) -> Vec<bool> {
    let rows = canvas.height();
    let cols = usize::from(canvas.width());
    let ruled = |row: usize, col: usize| -> bool {
        canvas
            .row(row)
            .and_then(|cells| cells.get(col))
            .map(|cell| cell.text())
            .is_some_and(|text| matches!(text, "├" | "┤" | "┬" | "┴" | "┼" | "╋" | "┣" | "┫"))
    };
    if vertical {
        (0..cols)
            .map(|col| ruled(0, col) || ruled(rows.saturating_sub(1), col))
            .collect()
    } else {
        (0..rows)
            .map(|row| ruled(row, 0) || ruled(row, cols.saturating_sub(1)))
            .collect()
    }
}

/// Where a node sits inside its container box, measured along the parent's axes.
///
/// `inwards` asks for the distance from the edge an incoming line arrives at; otherwise
/// it is the distance from the edge an outgoing line leaves by.
fn reach_of(canvas: &Canvas, spot: Spot, direction: Direction, inwards: bool) -> Reach {
    let rows = canvas.height();
    let cols = usize::from(canvas.width());
    let (near, far, lo, hi) = match direction {
        Direction::TopToBottom => (spot.row, rows - (spot.row + spot.rows), spot.col, spot.cols),
        Direction::BottomToTop => (rows - (spot.row + spot.rows), spot.row, spot.col, spot.cols),
        Direction::LeftToRight => (spot.col, cols - (spot.col + spot.cols), spot.row, spot.rows),
        Direction::RightToLeft => (cols - (spot.col + spot.cols), spot.col, spot.row, spot.rows),
    };
    Reach {
        depth: if inwards { near } else { far },
        lo,
        hi: lo + hi,
    }
}

/// Centres `canvas` in a canvas exactly `width` columns wide.
fn centred(canvas: Canvas, width: u16, theme: &Theme) -> Canvas {
    let left = crate::canvas::align_offset(
        usize::from(width),
        usize::from(canvas.width()),
        Align::Center,
    );
    let left = u16::try_from(left).unwrap_or(0);
    let mut out = canvas.indent(
        left,
        width.saturating_sub(canvas.width() + left),
        theme.base(),
    );
    out.resize_width(width, theme.base());
    out
}

/// One attempt at drawing the graph, at a fixed spacing and label budget.
struct Ctx<'a> {
    spec: &'a GraphSpec,
    art: &'a dyn NodeArt,
    theme: &'a Theme,
    budget: u16,
    gap: usize,
}

/// A drawn group: its canvas plus where each node it contains ended up.
struct Drawn {
    canvas: Canvas,
    /// Where every node it contains ended up inside `canvas`.
    hints: Vec<(NodeIdx, Spot)>,
}

/// The rectangle one node's box occupies inside a canvas.
#[derive(Debug, Clone, Copy)]
struct Spot {
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
}

impl Spot {
    /// Moves the rectangle by `(rows, cols)`.
    fn shifted(self, rows: usize, cols: usize) -> Self {
        Self {
            row: self.row + rows,
            col: self.col + cols,
            ..self
        }
    }
}

/// One box taking part in a level's layout.
struct Item {
    canvas: Canvas,
    ports: PortPolicy,
    hints: Vec<(NodeIdx, Spot)>,
    members: Vec<NodeIdx>,
    /// True when the box is a container frame rather than a node itself.
    group: bool,
}

impl Ctx<'_> {
    /// Lays out one container and everything below it.
    fn group(&self, group: &GroupSpec, inherited: Direction) -> Drawn {
        let direction = group.direction.unwrap_or(inherited);
        let mut items = Vec::new();
        for &node in &group.nodes {
            let canvas = self.art.render(node, self.budget, self.theme);
            let whole = Spot {
                row: 0,
                col: 0,
                rows: canvas.height(),
                cols: usize::from(canvas.width()),
            };
            items.push(Item {
                canvas,
                ports: self.art.ports(node),
                hints: vec![(node, whole)],
                members: vec![node],
                group: false,
            });
        }
        for child in &group.children {
            let drawn = self.group(child, direction);
            let members = drawn.hints.iter().map(|&(node, _)| node).collect();
            items.push(Item {
                canvas: drawn.canvas,
                ports: PortPolicy::Spread,
                hints: drawn.hints,
                members,
                group: true,
            });
        }
        let inner = self.level(&items, direction);
        match &group.title {
            None => inner,
            Some(title) => self.frame(inner, title),
        }
    }

    /// Wraps a drawn container in its titled frame.
    fn frame(&self, drawn: Drawn, title: &[String]) -> Drawn {
        let styles = self.theme.diagram;
        let mut padded = Canvas::new(drawn.canvas.width(), 1, self.theme.base());
        padded.append(&drawn.canvas, self.theme.base());
        padded.push_blank_row(self.theme.base());
        let padded = padded.indent(1, 1, self.theme.base());
        let heading = title
            .first()
            .map(|text| Line::new(vec![Span::new(text.clone(), styles.group_title)]));
        let canvas = padded.framed(
            BorderSet::DASHED,
            styles.group_border,
            heading.as_ref(),
            self.theme.base(),
        );
        // The frame adds one row and column of border plus one of padding.
        let hints = drawn
            .hints
            .into_iter()
            .map(|(node, spot)| (node, spot.shifted(2, 2)))
            .collect();
        Drawn { canvas, hints }
    }

    /// Lays out one level: the boxes of a container and the edges between them.
    fn level(&self, items: &[Item], direction: Direction) -> Drawn {
        let vertical = Frame::vertical(direction);
        if items.is_empty() {
            return Drawn {
                canvas: Canvas::empty(0),
                hints: Vec::new(),
            };
        }
        let owner = self.owners(items);
        let (raw, mut level_edges, loops) = self.edges(items, &owner, direction);
        let mut layered = rank::build(items.len(), &raw);
        order::reduce(&mut layered);
        // An edge reversed to break a cycle is drawn against the flow, so its
        // terminators swap ends and its arrow still points where the source said.
        for (edge, reversed) in level_edges.iter_mut().zip(&layered.reversed) {
            if *reversed {
                std::mem::swap(&mut edge.tail, &mut edge.head);
                std::mem::swap(&mut edge.from_hint, &mut edge.to_hint);
                std::mem::swap(&mut edge.from_reach, &mut edge.to_reach);
            }
        }

        let count = layered.vnodes.len();
        let mut cross_size = vec![1usize; count];
        let mut flow_size = vec![0usize; count];
        let mut place_size = vec![1usize; count];
        let mut loop_pad = vec![0usize; count];
        let mut ports = vec![PortPolicy::Center; count];
        let mut ruled: Vec<Vec<bool>> = vec![Vec::new(); count];
        for (index, item) in items.iter().enumerate() {
            let (rows, cols) = (item.canvas.height(), usize::from(item.canvas.width()));
            let (cross, flow) = if vertical { (cols, rows) } else { (rows, cols) };
            cross_size[index] = cross;
            flow_size[index] = flow;
            let looped = loops.iter().any(|&(item, _)| item == index);
            // A self loop needs three cells beside the box and two rows below it.
            place_size[index] = cross + if looped { 3 } else { 0 };
            loop_pad[index] = if looped { 2 } else { 0 };
            ports[index] = item.ports;
            ruled[index] = ruled_offsets(&item.canvas, vertical);
        }
        let cross_gap = if vertical {
            self.gap
        } else {
            self.gap.saturating_sub(2).max(1)
        };
        let cross = place::assign(&layered, &place_size, cross_gap);
        let input = Input {
            layered: &layered,
            cross: &cross,
            cross_size: &cross_size,
            flow_size: &flow_size,
            ports: &ports,
            edges: &level_edges,
            loops: &loops,
            loop_pad: &loop_pad,
            ruled: &ruled,
            min_gap: if vertical { 1 } else { 3 },
            vertical,
        };
        let routing = Routing::compute(&input);
        let total_cross = cross
            .iter()
            .zip(&place_size)
            .map(|(at, size)| at + size)
            .max()
            .unwrap_or(0)
            .max(routing.cross_extent);
        let frame = Frame::new(direction, routing.total_flow, total_cross);
        let styles = self.theme.diagram;
        let mut pen = Pen::new(frame, self.theme.base(), styles.edge_label);
        let mut hints = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let (row, col) = frame.origin(
                routing.flow[index],
                cross[index],
                flow_size[index],
                cross_size[index],
            );
            pen.canvas.blit(row, col, &item.canvas, self.theme.base());
            for &(node, spot) in &item.hints {
                hints.push((node, spot.shifted(row, col)));
            }
        }
        routing.paint(&input, &mut pen);
        let mut canvas = pen.canvas;
        pen.ink.apply(&mut canvas, styles.line, styles.arrow);
        Drawn { canvas, hints }
    }

    /// Maps every node under this container to the item that holds it.
    fn owners(&self, items: &[Item]) -> Vec<Option<usize>> {
        let mut owner = vec![None; self.spec.node_count];
        for (index, item) in items.iter().enumerate() {
            for node in &item.members {
                owner[node.0] = Some(index);
            }
        }
        owner
    }

    /// Splits this container's edges into layered edges and self loops.
    fn edges(
        &self,
        items: &[Item],
        owner: &[Option<usize>],
        direction: Direction,
    ) -> (Vec<RawEdge>, Vec<LevelEdge>, Vec<(usize, usize)>) {
        let vertical = Frame::vertical(direction);
        let mut raw = Vec::new();
        let mut level = Vec::new();
        let mut pending = Vec::new();
        for edge in &self.spec.edges {
            let (Some(from), Some(to)) = (owner[edge.from.0], owner[edge.to.0]) else {
                continue;
            };
            let spot = |item: usize, node: NodeIdx| -> Option<Spot> {
                items[item]
                    .hints
                    .iter()
                    .find(|&&(other, _)| other == node)
                    .map(|&(_, spot)| spot)
            };
            let hint = |item: usize, node: NodeIdx| -> usize {
                spot(item, node).map_or(0, |spot| {
                    if vertical {
                        spot.col + spot.cols / 2
                    } else {
                        spot.row + spot.rows / 2
                    }
                })
            };
            let reach = |item: usize, node: NodeIdx, inwards: bool| -> Option<Reach> {
                if !items[item].group {
                    return None;
                }
                let spot = spot(item, node)?;
                Some(reach_of(&items[item].canvas, spot, direction, inwards))
            };
            let described = LevelEdge {
                stroke: edge.stroke,
                tail: edge.tail,
                head: edge.head,
                label: edge.label.clone(),
                tail_label: edge.tail_label.clone(),
                head_label: edge.head_label.clone(),
                from_hint: hint(from, edge.from),
                to_hint: hint(to, edge.to),
                from_reach: reach(from, edge.from, false),
                to_reach: reach(to, edge.to, true),
            };
            if from == to {
                // Both ends inside the same nested container: drawn one level down.
                if edge.from == edge.to {
                    pending.push((from, described));
                }
                continue;
            }
            raw.push(RawEdge { from, to });
            level.push(described);
        }
        // Self loops take no part in layering, so they are indexed after the rest.
        let mut loops = Vec::with_capacity(pending.len());
        for (item, described) in pending {
            loops.push((item, level.len()));
            level.push(described);
        }
        (raw, level, loops)
    }
}
