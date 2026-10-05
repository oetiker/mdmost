// SPDX-License-Identifier: MIT
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
//! use mdmost::mermaid::ast::Direction;
//! use mdmost::mermaid::layout::graph::{self, EdgeSpec, Fit, GraphSpec, NodeIdx};
//! use mdmost::theme::Theme;
//! use mdmost::canvas::Canvas;
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
mod stretch;
mod stub;

#[cfg(test)]
mod tests;

pub use glyph::{Dir, Stroke};
pub use spec::{
    Beside, DrawnLabel, EdgeSpec, FrameStyle, GraphSpec, GroupSpec, NodeArt, NodeIdx, PortPolicy,
    Terminator,
};

use std::borrow::Cow;
use std::cell::RefCell;

use crate::canvas::{BorderSet, Canvas};
use crate::error::MermaidError;
use crate::mermaid::ast::Direction;
use crate::mermaid::chrome::{self, Piece};
use crate::text::{Line, Span};
use crate::theme::{Color, Style, Theme};

use frame::{Frame, Pen};
use rank::RawEdge;
use route::{Input, LevelEdge, Reach, Routing, SideCell};
use stub::{Blocked, Stub};

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
/// The drawing is anchored at the left of the canvas, with any unused columns on the
/// right ([`anchored`]). Nodes are drawn by `art`, which is called once per node per
/// attempt at fitting the width budget.
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
        let mut spec = Cow::Borrowed(spec);
        let mut stubs = Vec::new();
        let mut round = 0;
        loop {
            let ctx = Ctx {
                spec: &spec,
                art,
                theme,
                budget,
                gap,
                stubs: &stubs,
                blocked: RefCell::default(),
            };
            let drawn = ctx.group(&spec.root, spec.direction);
            let blocked = ctx.blocked.take();
            if blocked.is_empty() || round == STUB_ROUNDS {
                return ctx.washed(drawn);
            }
            let split = stub::split(&spec, &blocked, &mut stubs);
            spec = Cow::Owned(split);
            round += 1;
        }
    };
    let mut narrowest: Option<u16> = None;

    for &(gap, share) in fit.ladder {
        let canvas = attempt(gap, (width / share).max(fit.floor));
        if canvas.width() <= width {
            return Ok(anchored(canvas, width, theme));
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
    Ok(anchored(best, width, theme))
}

/// Moves item `node`, which has no edges, into the rank of item `anchor`, right
/// beside it.
fn share_rank(
    layered: &mut rank::Layered,
    node: Option<usize>,
    anchor: Option<usize>,
    right: bool,
) {
    let (Some(node), Some(anchor)) = (node, anchor) else {
        return;
    };
    let old = layered.vnodes[node].rank;
    let rank = layered.vnodes[anchor].rank;
    layered.ranks[old].retain(|&id| id != node);
    layered.vnodes[node].rank = rank;
    let at = layered.ranks[rank]
        .iter()
        .position(|&id| id == anchor)
        .map_or(0, |at| if right { at + 1 } else { at });
    layered.ranks[rank].insert(at, node);
    debug_assert!(
        layered.ranks.iter().all(|ids| !ids.is_empty()),
        "an anchor's rank always holds a source besides the note"
    );
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
/// the router avoids those cells when it has a choice. A heavy box draws its rules
/// with `┠ ┨ ┯ ┷` instead (colour spec §6.2). This is read back off the
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
            .is_some_and(|text| {
                matches!(
                    text,
                    "├" | "┤"
                        | "┬"
                        | "┴"
                        | "┼"
                        | "╋"
                        | "┣"
                        | "┫"
                        | "┠"
                        | "┨"
                        | "┯"
                        | "┷"
                )
            })
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

/// Where a frame's title goes and how wide the frame becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TitlePlace {
    /// Columns between the two side borders.
    inner: usize,
    /// How far the content moves right inside them.
    shift: usize,
    /// The frame column the space before the title is written to.
    at: usize,
}

/// Places a title `title` columns wide over content `content` columns wide.
///
/// A frame narrower than its title cut it off: `subgraph s["A rather long title"]`
/// around one small node drew `╭ A rath╮`. So the frame grows to hold the title and a
/// space either side, though never past `cap`, the width a node's own label may take,
/// so that one long title cannot push the whole diagram off the page.
///
/// A title written over a node an outside edge must reach also stops that edge at the
/// border, because no line crosses a letter. `crossed` holds those nodes as
/// `(column, width)` inside the content, and every one of them keeps at least one cell
/// of plain frame above its interior. The first fit wins, in this order: the title at
/// the left with the content centred; the title moved right; the frame widened one
/// column at a time. When nothing fits within `cap`, the plain layout is used and the
/// edge ends at the frame.
fn place_title(content: usize, title: usize, crossed: &[(usize, usize)], cap: usize) -> TitlePlace {
    let block = if title == 0 { 0 } else { title + 2 };
    let narrowest = content.max(block.min(cap.max(content)));
    let fits = |place: TitlePlace| {
        crossed.iter().all(|&(col, cols)| {
            // Frame columns: the border, the padding, the shift.
            let lo = col + 2 + place.shift + 1;
            let hi = (col + 2 + place.shift + cols).saturating_sub(2);
            (lo..=hi).any(|at| at < place.at || at >= place.at + block)
        })
    };
    let plain = TitlePlace {
        inner: narrowest,
        shift: (narrowest - content) / 2,
        at: 1,
    };
    if block == 0 || crossed.is_empty() || fits(plain) {
        return plain;
    }
    for inner in narrowest..=cap.max(narrowest) {
        let centred = (inner - content) / 2;
        let shifts =
            std::iter::once(centred).chain((0..=inner - content).filter(|&s| s != centred));
        for shift in shifts {
            for at in 1..=(inner + 1).saturating_sub(block).max(1) {
                let place = TitlePlace { inner, shift, at };
                if fits(place) {
                    return place;
                }
            }
        }
    }
    plain
}

/// What each cell along one side of `canvas` holds.
///
/// `inwards` picks the side an incoming edge arrives at; otherwise it is the side an
/// outgoing edge leaves by. A container frame writes its title into its top border,
/// and an edge carried through the frame to a node inside cannot cross those letters:
/// a port landing on one stops at the border. The router keeps such ports off them.
fn side_cells(canvas: &Canvas, direction: Direction, inwards: bool) -> Vec<SideCell> {
    let rows = canvas.height();
    let cols = usize::from(canvas.width());
    let cell = |row: usize, col: usize| -> SideCell {
        let Some(drawn) = canvas.row(row).and_then(|cells| cells.get(col)) else {
            return SideCell::Blank;
        };
        // The right half of a wide letter has no text of its own, but is still a letter.
        if drawn.is_continuation() {
            return SideCell::Text;
        }
        match drawn.text().chars().next().unwrap_or(' ') {
            ' ' => SideCell::Blank,
            ch if glyph::mask_of(ch).is_some() => SideCell::Art,
            _ => SideCell::Text,
        }
    };
    // The side an edge enters by is the near one for a top-down or left-to-right flow.
    let near = matches!(direction, Direction::TopToBottom | Direction::LeftToRight) == inwards;
    let mut side: Vec<SideCell> = if Frame::vertical(direction) {
        let row = if near { 0 } else { rows.saturating_sub(1) };
        (0..cols).map(|col| cell(row, col)).collect()
    } else {
        let col = if near { 0 } else { cols.saturating_sub(1) };
        (0..rows).map(|row| cell(row, col)).collect()
    };
    // A space between two words of a title is part of the title, not a gap in the
    // frame: an edge let through there drew `╭ A rather│long title`. Only the margin
    // spaces between the title and the frame line stay blank.
    let mut index = 0;
    while index < side.len() {
        if side[index] != SideCell::Blank {
            index += 1;
            continue;
        }
        let end = (index..side.len())
            .find(|&at| side[at] != SideCell::Blank)
            .unwrap_or(side.len());
        let before = index.checked_sub(1).map(|at| side[at]);
        let after = side.get(end).copied();
        if before == Some(SideCell::Text) && after == Some(SideCell::Text) {
            side[index..end].fill(SideCell::Text);
        }
        index = end;
    }
    side
}

/// The widest run of `spot`'s cross span that a straight line from the container's
/// side reaches without crossing the box of another node in `hints`, or `None` when
/// every column of it is blocked. `inwards` and `direction` pick the side as in
/// [`reach_of`].
fn clear_span(
    hints: &[(NodeIdx, Spot)],
    node: NodeIdx,
    spot: Spot,
    canvas: &Canvas,
    direction: Direction,
    inwards: bool,
) -> Option<(usize, usize)> {
    let vertical = Frame::vertical(direction);
    // `(flow start, flow end, cross start, cross end)` of a box.
    let axes = |at: Spot| {
        if vertical {
            (at.row, at.row + at.rows, at.col, at.col + at.cols)
        } else {
            (at.col, at.col + at.cols, at.row, at.row + at.rows)
        }
    };
    let total = if vertical {
        canvas.height()
    } else {
        usize::from(canvas.width())
    };
    let (flow_lo, flow_hi, lo, hi) = axes(spot);
    let from_start =
        matches!(direction, Direction::TopToBottom | Direction::LeftToRight) == inwards;
    let (way_lo, way_hi) = if from_start {
        (0, flow_lo)
    } else {
        (flow_hi, total)
    };
    let mut free = vec![true; hi - lo];
    for &(other, at) in hints {
        if other == node {
            continue;
        }
        let (a, b, c, d) = axes(at);
        if a >= way_hi || b <= way_lo {
            continue;
        }
        for cross in c.max(lo)..d.min(hi) {
            free[cross - lo] = false;
        }
    }
    let mut best: Option<(usize, usize)> = None;
    let mut start = None;
    for at in 0..=free.len() {
        match (free.get(at).copied().unwrap_or(false), start) {
            (true, None) => start = Some(at),
            (false, Some(first)) => {
                if best.is_none_or(|(a, b)| at - first > b - a) {
                    best = Some((first, at));
                }
                start = None;
            }
            _ => {}
        }
    }
    best.map(|(a, b)| (lo + a, lo + b))
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

/// Washes the inside of each frame, in order, over cells still on the page.
///
/// `frames` is innermost first. A cell once visited is claimed, so an outer wash never
/// reaches into an inner frame; cells of a node that `keep`s the page are skipped; a
/// node with its own fill is no longer on the page and keeps it.
fn tint_frames(canvas: &mut Canvas, frames: &[Washed], keep: &[Spot], page: Color) {
    let cols = usize::from(canvas.width());
    let mut claimed = vec![false; canvas.height() * cols];
    let kept = |row: usize, col: usize| {
        keep.iter().any(|spot| {
            (spot.row..spot.row + spot.rows).contains(&row)
                && (spot.col..spot.col + spot.cols).contains(&col)
        })
    };
    for washed in frames {
        let Spot {
            row,
            col,
            rows,
            cols: width,
        } = washed.at;
        for r in row + 1..(row + rows).saturating_sub(1) {
            for c in col + 1..(col + width).saturating_sub(1).min(cols) {
                let Some(flag) = claimed.get_mut(r * cols + c) else {
                    continue;
                };
                if std::mem::replace(flag, true) || kept(r, c) {
                    continue;
                }
                let on_page = canvas
                    .row(r)
                    .and_then(|cells| cells.get(c))
                    .is_some_and(|cell| cell.style().bg == Some(page));
                if on_page {
                    canvas.patch_style(r, c, 1, Style::new().bg(washed.tint));
                }
            }
        }
    }
}

/// Anchors `canvas` at the left of a canvas exactly `width` columns wide.
///
/// The drawing used to be centred in the budget it was given. It is not any more: every
/// block in a document starts at the same left margin (see `render::document::placed`),
/// and a diagram centred in its budget came to rest well to the right of the prose above
/// it — a third left edge on one page. The unused columns are kept on the right, so the
/// canvas is still exactly `width` wide, which is what every caller was promised.
fn anchored(canvas: Canvas, width: u16, theme: &Theme) -> Canvas {
    let mut out = canvas.indent(0, width.saturating_sub(canvas.width()), theme.base());
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
    /// The entry stubs added to `spec`, which `art` knows nothing about.
    stubs: &'a [Stub],
    /// Edges whose straight line into a container met another box.
    blocked: RefCell<Vec<Blocked>>,
}

/// How many times a drawing is split at entry stubs and drawn again: one round per
/// level of nesting a blocked edge passes into.
const STUB_ROUNDS: usize = 4;

/// A drawn group: its canvas plus where each node it contains ended up.
struct Drawn {
    canvas: Canvas,
    /// Where every node it contains ended up inside `canvas`.
    hints: Vec<(NodeIdx, Spot)>,
    /// Every washed frame inside `canvas`, innermost first.
    frames: Vec<Washed>,
}

/// The rectangle one node's box occupies inside a canvas.
#[derive(Debug, Clone, Copy)]
struct Spot {
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
}

/// A washed frame's rectangle inside a canvas, and its wash.
#[derive(Debug, Clone, Copy)]
struct Washed {
    at: Spot,
    tint: Color,
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
    frames: Vec<Washed>,
    members: Vec<NodeIdx>,
    /// True when the box is a container frame rather than a node itself.
    group: bool,
}

impl Ctx<'_> {
    /// The finished drawing with every washed frame applied, innermost first.
    ///
    /// Done once on the whole canvas rather than per frame, because the level above a
    /// frame draws the edges and labels that enter it after the frame is finished, on
    /// the page background (colour spec §6.3).
    fn washed(&self, mut drawn: Drawn) -> Canvas {
        if drawn.frames.is_empty() {
            return drawn.canvas;
        }
        let keep: Vec<Spot> = drawn
            .hints
            .iter()
            .filter(|&&(node, _)| self.stub(node).is_none() && self.art.keeps_page(node))
            .map(|&(_, spot)| spot)
            .collect();
        tint_frames(
            &mut drawn.canvas,
            &drawn.frames,
            &keep,
            self.theme.palette.bg,
        );
        drawn.canvas
    }

    /// The entry stub `node` is, if it is one.
    fn stub(&self, node: NodeIdx) -> Option<Stub> {
        self.stubs.iter().find(|stub| stub.node == node).copied()
    }

    /// Lays out one container and everything below it.
    fn group(&self, group: &GroupSpec, inherited: Direction) -> Drawn {
        let direction = group.direction.unwrap_or(inherited);
        let mut items = Vec::new();
        for &node in &group.nodes {
            let canvas = match self.stub(node) {
                // A stub is one cell of the line passing through it.
                Some(_) => {
                    let mut canvas = Canvas::new(1, 1, self.theme.base());
                    let line = if Frame::vertical(direction) {
                        "│"
                    } else {
                        "─"
                    };
                    canvas.write_str(0, 0, line, self.theme.diagram.line);
                    canvas
                }
                None => self.art.render(node, self.budget, self.theme),
            };
            let whole = Spot {
                row: 0,
                col: 0,
                rows: canvas.height(),
                cols: usize::from(canvas.width()),
            };
            items.push(Item {
                canvas,
                ports: match self.stub(node) {
                    Some(_) => PortPolicy::Center,
                    None => self.art.ports(node),
                },
                hints: vec![(node, whole)],
                frames: Vec::new(),
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
                frames: drawn.frames,
                members,
                group: true,
            });
        }
        let inner = self.level(&items, direction);
        match &group.title {
            None => inner,
            Some(title) => {
                let crossed = self.crossed(group, &inner, inherited);
                self.frame(inner, title, &crossed, &group.style)
            }
        }
    }

    /// The nodes of `drawn` that an edge from outside `group` meets through the frame's
    /// top edge, as their columns inside `drawn`.
    ///
    /// Only a vertical parent flow sends edges through the top edge, which is the edge
    /// the title is written into. Which end of the edge is outside does not matter: an
    /// edge reversed to break a cycle crosses the other way.
    fn crossed(&self, group: &GroupSpec, drawn: &Drawn, parent: Direction) -> Vec<(usize, usize)> {
        if !Frame::vertical(parent) || group.title.is_none() {
            return Vec::new();
        }
        let inside = |node: NodeIdx| drawn.hints.iter().any(|&(other, _)| other == node);
        drawn
            .hints
            .iter()
            .filter(|&&(node, _)| {
                self.spec.edges.iter().any(|edge| {
                    (edge.from == node && !inside(edge.to))
                        || (edge.to == node && !inside(edge.from))
                })
            })
            .map(|&(_, spot)| (spot.col, spot.cols))
            .collect()
    }

    /// Wraps a drawn container in its titled frame.
    ///
    /// A frame has one top edge, so only the title's first row is drawn, and it is
    /// clipped to the width of that edge. The clip is made *here* rather than left to
    /// [`Canvas::framed`], because the span that maps the title back to the document has
    /// to name the bytes behind the cells that were really painted: computing the drawn
    /// text once and both drawing and mapping it is the only way the two cannot disagree.
    ///
    /// `crossed` lists, in the columns of `drawn`, the nodes an edge from outside
    /// reaches through the top edge. The title is placed so that it leaves each of them
    /// at least one cell of plain frame to cross; see [`place_title`].
    ///
    /// `style` colours the border and title and picks the dash weight; a wash is only
    /// recorded here and laid on the finished canvas by [`Ctx::washed`].
    fn frame(
        &self,
        drawn: Drawn,
        title: &DrawnLabel,
        crossed: &[(usize, usize)],
        style: &FrameStyle,
    ) -> Drawn {
        let styles = self.theme.diagram;
        let border = style
            .ink
            .map_or(styles.group_border, |ink| styles.group_border.fg(ink));
        let title_style = style
            .ink
            .map_or(styles.group_title, |ink| styles.group_title.fg(ink));
        let set = if style.heavy {
            BorderSet::DASHED_HEAVY
        } else {
            BorderSet::DASHED
        };
        let base = self.theme.base();
        let mut padded = Canvas::new(drawn.canvas.width(), 1, base);
        padded.append(&drawn.canvas, base);
        padded.push_blank_row(base);
        let padded = padded.indent(1, 1, base);
        let content = usize::from(padded.width());
        let title_cols = title
            .rows
            .first()
            .map_or(0, |row| crate::text::display_width(&row.text));
        let place = place_title(content, title_cols, crossed, usize::from(self.budget));
        let extra = u16::try_from(place.inner - content).unwrap_or(0);
        let left = u16::try_from(place.shift).unwrap_or(0);
        let padded = padded.indent(left, extra.saturating_sub(left), base);
        let mut canvas = padded.framed(set, border, None, base);
        // A space, the title and a space, clipped so they never reach the corner.
        let room = (place.inner + 1).saturating_sub(place.at + 2);
        let head = title
            .rows
            .first()
            .filter(|_| place.inner >= 4)
            .map(|row| Piece {
                text: crate::text::truncate_to_width(&row.text, room).to_string(),
                index: row.index,
                at: row.at,
            })
            .filter(|row| !row.text.is_empty());
        if let Some(head) = &head {
            let spaced = Line::new(vec![
                Span::new(" ", border),
                Span::new(head.text.clone(), title_style),
                Span::new(" ", border),
            ]);
            canvas.write_line(0, place.at, &spaced, border);
            chrome::label_spans(&mut canvas, &title.label, head, 0, place.at + 1);
        }
        // The frame adds one row and column of border plus one of padding, and the
        // content moved right by `shift` within a frame widened for its title.
        let hints = drawn
            .hints
            .into_iter()
            .map(|(node, spot)| (node, spot.shifted(2, 2 + place.shift)))
            .collect();
        // Recorded after everything inside it, which keeps the list innermost first.
        let mut frames: Vec<Washed> = drawn
            .frames
            .into_iter()
            .map(|washed| Washed {
                at: washed.at.shifted(2, 2 + place.shift),
                ..washed
            })
            .collect();
        if let Some(tint) = style.tint {
            frames.push(Washed {
                at: Spot {
                    row: 0,
                    col: 0,
                    rows: canvas.height(),
                    cols: usize::from(canvas.width()),
                },
                tint,
            });
        }
        Drawn {
            canvas,
            hints,
            frames,
        }
    }

    /// Lays out one level: the boxes of a container and the edges between them.
    fn level(&self, items: &[Item], direction: Direction) -> Drawn {
        let vertical = Frame::vertical(direction);
        if items.is_empty() {
            return Drawn {
                canvas: Canvas::empty(0),
                hints: Vec::new(),
                frames: Vec::new(),
            };
        }
        let owner = self.owners(items);
        let beside = self.beside(items, &owner);
        // Sideways, beside is along the flow: the tie is an ordinary edge into the rank
        // before or after the anchor. `RL` counts its ranks leftwards.
        let ties: Vec<EdgeSpec> = if vertical {
            Vec::new()
        } else {
            let leftwards = direction == Direction::RightToLeft;
            beside
                .iter()
                .map(|tie| {
                    let (from, to) = if tie.right != leftwards {
                        (tie.anchor, tie.node)
                    } else {
                        (tie.node, tie.anchor)
                    };
                    EdgeSpec {
                        stroke: Stroke::Dotted,
                        head: Terminator::None,
                        ..EdgeSpec::arrow(from, to)
                    }
                })
                .collect()
        };
        let (raw, mut level_edges, loops) = self.edges(items, &owner, direction, &ties);
        // The stub of an edge leaving this container takes the last rank, so the line
        // leaves through the far side with nothing below it.
        let last: Vec<bool> = items
            .iter()
            .map(|item| {
                !item.group
                    && item
                        .members
                        .iter()
                        .any(|&node| self.stub(node).is_some_and(|stub| stub.sink))
            })
            .collect();
        let mut layered = rank::build(items.len(), &raw, &last);
        order::reduce(&mut layered);
        if vertical {
            for tie in &beside {
                share_rank(
                    &mut layered,
                    owner[tie.node.0],
                    owner[tie.anchor.0],
                    tie.right,
                );
            }
        }
        // An edge reversed to break a cycle is drawn against the flow, so its
        // terminators swap ends and its arrow still points where the source said.
        for (edge, reversed) in level_edges.iter_mut().zip(&layered.reversed) {
            if *reversed {
                std::mem::swap(&mut edge.tail, &mut edge.head);
                std::mem::swap(&mut edge.from_hint, &mut edge.to_hint);
                std::mem::swap(&mut edge.from_reach, &mut edge.to_reach);
            }
        }
        let grown = self.grown(items, &layered, &level_edges, vertical);
        let canvas_of = |index: usize| grown[index].as_ref().unwrap_or(&items[index].canvas);

        let count = layered.vnodes.len();
        let mut cross_size = vec![1usize; count];
        let mut flow_size = vec![0usize; count];
        let mut place_size = vec![1usize; count];
        let mut loop_pad = vec![0usize; count];
        let mut ports = vec![PortPolicy::Center; count];
        let mut ruled: Vec<Vec<bool>> = vec![Vec::new(); count];
        let mut side_in: Vec<Vec<SideCell>> = vec![Vec::new(); count];
        let mut side_out: Vec<Vec<SideCell>> = vec![Vec::new(); count];
        for (index, item) in items.iter().enumerate() {
            let canvas = canvas_of(index);
            let (rows, cols) = (canvas.height(), usize::from(canvas.width()));
            let (cross, flow) = if vertical { (cols, rows) } else { (rows, cols) };
            cross_size[index] = cross;
            flow_size[index] = flow;
            let looped = loops.iter().any(|&(item, _)| item == index);
            // A self loop needs three cells beside the box and two rows below it.
            place_size[index] = cross + if looped { 3 } else { 0 };
            loop_pad[index] = if looped { 2 } else { 0 };
            ports[index] = item.ports;
            ruled[index] = ruled_offsets(canvas, vertical);
            side_in[index] = side_cells(canvas, direction, true);
            side_out[index] = side_cells(canvas, direction, false);
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
            side_in: &side_in,
            side_out: &side_out,
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
        let mut frames = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let (row, col) = frame.origin(
                routing.flow[index],
                cross[index],
                flow_size[index],
                cross_size[index],
            );
            let canvas = canvas_of(index);
            pen.canvas.blit(row, col, canvas, self.theme.base());
            for &(node, spot) in &item.hints {
                let spot = if grown[index].is_some() {
                    Spot {
                        rows: canvas.height(),
                        ..spot
                    }
                } else {
                    spot
                };
                hints.push((node, spot.shifted(row, col)));
            }
            for washed in &item.frames {
                frames.push(Washed {
                    at: washed.at.shifted(row, col),
                    ..*washed
                });
            }
        }
        routing.paint(&input, &mut pen);
        if vertical {
            for tie in &beside {
                let (Some(node), Some(anchor)) = (owner[tie.node.0], owner[tie.anchor.0]) else {
                    continue;
                };
                let rect = |item: usize| {
                    let (row, col) = frame.origin(
                        routing.flow[item],
                        cross[item],
                        flow_size[item],
                        cross_size[item],
                    );
                    (row, col, items[item].canvas.height(), cross_size[item])
                };
                let (left, right) = if tie.right {
                    (rect(anchor), rect(node))
                } else {
                    (rect(node), rect(anchor))
                };
                // Mid-way down the rows both boxes share, from one facing side to the other.
                let top = left.0.max(right.0);
                let bottom = (left.0 + left.2).min(right.0 + right.2);
                let start = left.1 + left.3 - 1;
                if bottom > top && right.1 > start {
                    let row = top + (bottom - top - 1) / 2;
                    pen.ink
                        .run(row, start, Dir::Right, right.1 - start, Stroke::Dotted);
                }
            }
        }
        let mut canvas = pen.canvas;
        pen.ink.apply(&mut canvas, styles.line, styles.arrow);
        Drawn {
            canvas,
            hints,
            frames,
        }
    }

    /// Per item, its box grown by blank rows when a sideways flow brings more kinds of
    /// edge end to one of its sides than the side has cells for ([`stretch`]).
    fn grown(
        &self,
        items: &[Item],
        layered: &rank::Layered,
        edges: &[LevelEdge],
        vertical: bool,
    ) -> Vec<Option<Canvas>> {
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                if vertical || item.group || item.ports != PortPolicy::Spread {
                    return None;
                }
                let kinds = |incoming: bool| {
                    let mut seen: Vec<Terminator> = Vec::new();
                    for seg in &layered.segs {
                        let end = if incoming { seg.b } else { seg.a };
                        if end != index {
                            continue;
                        }
                        let edge = &edges[seg.edge];
                        let terminator = if incoming { edge.head } else { edge.tail };
                        if !seen.contains(&terminator) {
                            seen.push(terminator);
                        }
                    }
                    seen.len()
                };
                // One cell per kind, with a cell of air between neighbours.
                let wanted = (2 * kinds(true).max(kinds(false))).saturating_sub(1);
                let inner = item.canvas.height().saturating_sub(2);
                (wanted > inner)
                    .then(|| stretch::rows(&item.canvas, wanted - inner, self.theme.base()))
            })
            .collect()
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

    /// The ties of this level whose node and anchor are two different items here,
    /// the node a plain box.
    fn beside(&self, items: &[Item], owner: &[Option<usize>]) -> Vec<Beside> {
        self.spec
            .beside
            .iter()
            .filter(|tie| match (owner[tie.node.0], owner[tie.anchor.0]) {
                (Some(node), Some(anchor)) => node != anchor && !items[node].group,
                _ => false,
            })
            .copied()
            .collect()
    }

    /// Splits this container's edges, followed by `ties`, into layered edges and self
    /// loops.
    fn edges(
        &self,
        items: &[Item],
        owner: &[Option<usize>],
        direction: Direction,
        ties: &[EdgeSpec],
    ) -> (Vec<RawEdge>, Vec<LevelEdge>, Vec<(usize, usize)>) {
        let vertical = Frame::vertical(direction);
        let mut raw = Vec::new();
        let mut level = Vec::new();
        let mut pending = Vec::new();
        for (index, edge) in self.spec.edges.iter().chain(ties).enumerate() {
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
                let reach = reach_of(&items[item].canvas, spot, direction, inwards);
                let clear = clear_span(
                    &items[item].hints,
                    node,
                    spot,
                    &items[item].canvas,
                    direction,
                    inwards,
                );
                match clear {
                    Some((lo, hi)) => Some(Reach { lo, hi, ..reach }),
                    None => {
                        // Only an edge crossing this level's frames enters a container.
                        if from != to && index < self.spec.edges.len() {
                            self.blocked.borrow_mut().push(Blocked {
                                edge: index,
                                inwards,
                            });
                        }
                        None
                    }
                }
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
