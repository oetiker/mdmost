// SPDX-License-Identifier: MIT
//! The document renderer: the one entry point every user-facing path goes through.
//!
//! The pager and `--render-once` both call [`render_document`], so what is piped is what
//! is paged. There used to be a second document renderer next to this one — the flat,
//! exactly-`width` primitive that is now [`super::render_flat`] — and having two of them
//! cost a release's worth of quiet bugs: `--body-width` accepted and silently dropped on
//! the pipe path, and a golden suite pinning layout no user could see. The primitive is
//! crate-private now, and this is the only thing that calls it.
//!
//! Design spec §7.3 and §8 both promise the same thing: a table whose minimum widths
//! exceed the terminal, and a code line longer than the terminal, become *horizontally
//! scrollable* rather than mangled. That promise can only be kept if the document
//! canvas is allowed to be wider than the viewport — a canvas rendered at viewport
//! width has nothing beyond the right edge to scroll to.
//!
//! Widening the *whole* document would be the easy answer and the wrong one: prose
//! reflowed to the width of one long code line would need horizontal scrolling to read
//! a paragraph. So the widening is per block. Every top-level block is rendered at the
//! viewport width; the ones that come back clipped are re-rendered at the width they
//! actually want. [`Canvas::append`] then stacks the parts, padding the narrow ones, and
//! translates their anchors and search spans, so the result obeys the canvas contract
//! exactly as a flat [`super::render_flat`] would.
//!
//! A Mermaid fence is the exception to "clipped" being the signal, and to matching
//! [`super::render_flat`] block for block. It never comes back clipped — a diagram that
//! does not fit comes back as a *dump of its own source*, which fits fine — so it is
//! asked first, through [`crate::render::diagram`], and it is asked under a policy that
//! refuses to mince labels ([`Fit::ROOMY`](crate::mermaid::Fit::ROOMY)). A fence the
//! flat renderer squeezes into the viewport may therefore be drawn wide here instead.
//! That is the one deliberate difference; everything else in a document with nothing
//! over-wide comes out byte-identical to [`super::render_flat`] — including its side
//! margins, which are applied here for the same reason and in the same place. That claim
//! used to be false in exactly one other respect, and being false only about the margins
//! is what made it survive: the pager looked right until you noticed nothing had a
//! gutter.

use crate::canvas::{Canvas, Cell};
use crate::doc::{Doc, Node, NodeKind};
use crate::numbering::Numbering;
use crate::render::{Ctx, Limits, RenderOptions, margins, render_flat};
use crate::theme::Theme;

use super::block;
use super::macros;

/// The glyph the renderers paint in a row's last column when content is cut off.
///
/// This used to be a copy, kept because the module lived under `tui` and reaching into
/// the renderer would have coupled the pager to its internals — with a tripwire test
/// pinning the copy to the original. Living in `render` now, it is the original.
pub(crate) use super::code::OVERFLOW_MARKER;

/// The bar a block quote paints down its left edge, on every row it owns.
///
/// The other former copy; see [`OVERFLOW_MARKER`].
pub(crate) use super::block::QUOTE_BAR;

/// The whole-document inputs every block is rendered against, bundled so the functions
/// that thread them down stay under clippy's argument limit.
///
/// `Copy` for the same reason [`super::Ctx`] is: every block in the loop takes its own
/// copy rather than sharing a borrow that would have to outlive the loop body.
///
/// Blocks are rendered through [`block::render_block_ctx`] with the [`Ctx`] this builds,
/// not through the public [`super::render_block_numbered`]: that signature carries
/// numbers and source but not the macro definitions, and widening a `pub` function
/// already at clippy's argument limit for the one caller that lives beside it is the
/// wrong trade. Everything a block needs from the whole document is here, once.
#[derive(Debug, Clone, Copy)]
struct DocCtx<'a> {
    theme: &'a Theme,
    options: &'a RenderOptions,
    numbers: &'a Numbering,
    /// The whole document text, for a formula that will not draw (design spec §5.3).
    source: &'a str,
    /// The display blocks that define macros and draw nothing (design spec §16).
    macros: &'a [macros::Definition],
}

impl<'a> DocCtx<'a> {
    /// The rendering context every block of this document starts from.
    fn ctx(self) -> Ctx<'a> {
        Ctx::new(self.theme, self.options)
            .numbered(self.numbers)
            .with_source(self.source)
            .with_macros(self.macros)
    }
}

/// The widest a single block is ever grown to.
///
/// A bound is needed because a pathological document — one enormous minified line —
/// would otherwise allocate a canvas proportional to its longest line. Content beyond
/// it stays clipped, which is the same outcome as today, only much further out.
const MAX_BLOCK_WIDTH: u16 = 2048;

/// Renders `doc` at `width`, widening any block that would otherwise be clipped.
///
/// The returned canvas is at least `width` columns wide and may be wider; the surplus
/// is what the horizontal scroll keys reach, as far as it is drawn in — see
/// [`scroll_reach`], which measures that and is what moves each row.
pub fn render_document(
    doc: &Doc,
    width: u16,
    body_width: Option<u16>,
    theme: &Theme,
    options: &RenderOptions,
) -> Canvas {
    let clipped = ClipTest::new(theme);
    let blocks = &doc.root().children;
    // The top level of a document is a sequence of blocks. Should a parser change ever
    // put bare inline content there, `render_sequence` groups it into one wrapped run
    // and this per-block assembly would not — so hand those documents back to the
    // renderer whole rather than laying them out differently here.
    if blocks.iter().any(block::is_inline) {
        return render_flat(doc, width, theme, options);
    }

    let fill = theme.base();
    // The margin is applied here, over the assembled body, exactly as `render_flat`
    // applies it over `render_sequence` — assembling blocks ourselves means inheriting
    // that job too, and for a long time we did not: every line in the pager sat welded
    // to the scrollbar while the piped renderer was correctly inset.
    let margin = margins(width);
    let full = width - 2 * margin;
    let measure = Measure::new(full, body_width);
    let mut out = Canvas::empty(full);
    // The lone-`#` title banner is a whole-document decision, so it is taken by the
    // renderer and only *placed* here; assembling the top level ourselves means we
    // would otherwise not have the feature at all in the pager. It is placed at the
    // body measure like any other block, so a capped body centres it with the prose
    // rather than letting the art run to the terminal edge on its own.
    let mut banner = crate::render::title_banner(doc, measure.prose, theme, options);
    // Section numbering is the other whole-document decision (design spec §9.3), and
    // for the same reason: a block renderer cannot see whether the document nests
    // deeply enough to want numbers. Computed once here and handed to every block, so
    // the pager and the piped renderer number identically.
    let numbers = Numbering::enabled(doc, options.section_numbers);
    // And the macro definitions (design spec §16) are the third: which candidate blocks
    // draw nothing is decided here, once, so a formula never re-renders a block to find
    // out and a table cell is measured with the same definitions it is drawn with.
    let macros = macros::definitions(doc, theme);
    let doc_ctx = DocCtx {
        theme,
        options,
        numbers: &numbers,
        source: doc.source(),
        macros: &macros,
    };
    for (index, node) in blocks.iter().enumerate() {
        let part = match banner.take() {
            Some(banner) if index == 0 => placed(banner, measure, fill),
            _ => render_placed(node, measure, doc_ctx, &clipped, fill),
        };
        if part.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_blank_row(fill);
        }
        out.append(&part, fill);
    }
    // `append` widens `out` to the widest part, but a document of nothing but empty
    // blocks never appends at all.
    out.resize_width(out.width().max(full), fill);
    // A widened block keeps its surplus past the right margin; that surplus is what the
    // horizontal scroll reaches, and a gutter drawn at its far edge would be off-screen
    // anyway.
    let mut out = out.indent(margin, margin, fill);
    out.resize_width(out.width().max(width), fill);
    out
}

/// The two widths a block may be laid out at, and where a capped block is placed.
///
/// Prose past roughly a hundred columns is hard to read: the eye loses the start of the
/// next line on the way back from the end of this one. So the body has a configurable
/// cap ([`crate::config::Config::body_width`]) and, on a terminal wider than the cap,
/// the prose is centred within the body rather than run edge to edge.
///
/// # Which blocks are exempt, and why
///
/// The cap is about *reflowable* content. Content that cannot be reflowed is not made
/// more readable by being given less room — it is made narrower and then cut. So:
///
/// * **Tables and Mermaid diagrams are exempt outright**: they are laid out at the full
///   body width, whatever the cap says. Both renderers stop at their natural width —
///   `render::table::distribute` returns the natural widths when they fit rather than
///   padding the columns out, and `render::diagram` answers with the *narrowest* width
///   that draws — so "the full width" costs nothing when they do not need it, and a
///   table squeezed into a cap would wrap every cell instead.
/// * **Everything else is laid out at the cap and escalates to the full body width the
///   moment the cap would clip it.** That is what a fenced code block gets: its frame
///   fills whatever budget it is handed, so exempting it outright would blow every
///   three-line snippet out to the terminal edge while the prose beside it stayed at
///   the measure. A code block is only *mangled* when a line is cut, and being cut is
///   exactly the escalation trigger. The same rule covers a wide table or fence nested
///   inside a block quote or a list item: the quote is prose and is capped, but the
///   wide thing inside it still reaches the full width.
/// * Image and HTML placeholders are prose furniture and are capped with the prose.
///   (HTML is not rendered at all — design spec §2.)
///
/// Past the full body width nothing changes: the block is widened and reached with the
/// horizontal scroll keys exactly as it was before the cap existed.
///
/// # Placement
///
/// **Everything shares one left edge.** The cap limits a block's *width* and does nothing
/// else: narrow content stops early on the right, wide content runs past the measure.
/// Nothing is centred, and there are no exceptions — not the title banner, not a heading
/// rule, not a diagram. [`placed`] is where that is done, and where the argument for it
/// against the centred layout this replaced is written down.
///
/// A capped block is still cropped to what it occupies, which is what keeps
/// [`scroll_reach`] seeing what it expects: rows that fit stay well inside the render
/// width and get an offset of zero. A pinned prefix needs nothing from the placement —
/// it is published by the renderer and translated by the document margin itself
/// ([`pinned_prefix`], [`Canvas::indent`]).
#[derive(Debug, Clone, Copy)]
struct Measure {
    /// The whole body between the margins.
    full: u16,
    /// The width prose is laid out at; equal to `full` when there is no cap.
    prose: u16,
}

impl Measure {
    /// The measure for a body of `full` columns under `cap`.
    fn new(full: u16, cap: Option<u16>) -> Self {
        Self {
            full,
            prose: cap.map_or(full, |cap| cap.min(full)).max(1),
        }
    }

    /// Whether the cap is doing anything at this width.
    const fn is_capped(&self) -> bool {
        self.prose < self.full
    }
}

/// Whether a block is laid out at the full body width however narrow the cap is.
///
/// See [`Measure`] for the reasoning. The test is the *top-level* block's own kind: a
/// table nested in a quote is not exempt, it escalates instead.
fn is_exempt(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Table(_) => true,
        NodeKind::CodeBlock { language, .. } => {
            crate::render::code::is_mermaid(language.as_deref())
        }
        // A display formula has exactly one width (design spec §7) — the same reason a
        // Mermaid diagram is exempt just above. Without this arm the prose cap squeezes
        // it and the clip test then widens it back, which costs a second layout and
        // reads as a flicker on a narrow terminal.
        //
        // This governs the width the formula is *laid out* at, and nothing else. What it
        // is *centred* in is `measure.prose`, which [`render_widened`] passes on
        // separately — so a 90-column formula draws at 90 instead of being squeezed to
        // the cap, and still shares its left edge with the prose it belongs to.
        NodeKind::Math { display: true, .. } => true,
        _ => false,
    }
}

/// Renders one block at the width its kind earns, and places it in the body.
///
/// `doc` is threaded down as the block's [`Ctx`] so a formula that will not draw can
/// fall back to its own bytes of the source — see [`super::Ctx::source`] — and a formula
/// that uses a macro sees the definitions before it — see [`super::Ctx::macros`].
fn render_placed(
    node: &Node,
    measure: Measure,
    doc: DocCtx<'_>,
    clip: &ClipTest,
    fill: crate::theme::Style,
) -> Canvas {
    // Nothing to place: with no cap the body is the full width, and every block was laid
    // out at it.
    if !measure.is_capped() {
        return render_widened(node, measure, doc, clip);
    }
    let canvas = if is_exempt(node) {
        render_widened(node, measure, doc, clip)
    } else {
        let capped = block::render_block_ctx(node, measure.prose, doc.ctx());
        if clip.is_clipped(&capped) {
            // The cap would cut this block short, so it takes the whole body — and
            // beyond it, if the whole body is not enough either.
            render_widened(node, measure, doc, clip)
        } else {
            capped
        }
    };
    placed(canvas, measure, fill)
}

/// Places an already-drawn block in the body: at the left margin, cropped to what it drew.
///
/// Split out of [`render_placed`] so the title banner — which is drawn by the renderer
/// rather than here — is placed by exactly the same arithmetic as every other block.
fn placed(mut canvas: Canvas, measure: Measure, fill: crate::theme::Style) -> Canvas {
    if !measure.is_capped() {
        return canvas;
    }
    let extent = canvas
        .rows()
        .iter()
        .map(|row| row_extent(row))
        .max()
        .unwrap_or(0);
    // How much of the body this block occupies. A block laid out at the cap is credited
    // with the cap whatever it happens to draw, so a two-word paragraph does not report
    // itself as two words wide; a block that took the full width is credited with what
    // it actually drew.
    let occupied = if extent > measure.prose {
        extent
    } else {
        measure.prose
    };
    if occupied >= measure.full {
        return canvas;
    }
    // And that is the whole of the placement: **every block starts at the same left
    // margin**, so what the cap does is shorten a line, never move it. This used to
    // centre the block in the body, which reads well in a document that is nothing but
    // prose and falls apart in any other: each block is centred against its own width,
    // so a heading, a table, a fence and a diagram on one page came to rest on four
    // different left edges (19, 8, 1 and 37 columns, measured at a width of 110) and the
    // margin staircased as the reader scrolled. A single left edge is what makes a mixed
    // document scannable — the eye returns to one place — and the content that will not
    // fit sticks out to the right, where nobody is looking for the start of a line.
    //
    // Cropping is still needed, and for the reason it always was: without it a block
    // carries the columns of padding it was laid out with past the right margin, which
    // puts every row's extent past the render width and hands the whole document to
    // `scroll_reach` as one over-wide run.
    canvas.truncate_width(occupied, fill);
    canvas
}

/// How much wider than the viewport a diagram must want before it is granted the width.
///
/// A diagram needing one more column than it has is not worth giving the whole document
/// a horizontal scrollbar, a chevron on every row and an `↔ 1/1` readout for. Below this
/// it takes the fit ladder's answer instead, squeezed or dumped — the same thing it got
/// before diagrams could scroll at all.
///
/// The rule is the same for a formula, but it is applied in a different place. A diagram
/// answers `None` when it does not want the room, so the guard below can hold it; a
/// formula that does not want the room must still answer, or the clip search widens it
/// regardless — so [`super::math::formula`] applies this itself and is why it is visible
/// to a sibling module rather than private here.
pub(super) const MIN_SURPLUS: u16 = 8;

/// How many viewports wide a diagram may be laid out.
///
/// Past this the source dump is the better answer: crossing a diagram three screens wide
/// is already 160 arrow presses, and [`Canvas::append`] pads every row of the document to
/// the widest part, so the whole canvas pays for it. Revisit if a page-left/right or
/// jump-to-edge binding ever lands.
const VIEWPORTS: u16 = 3;

/// The most layouts the width search may spend on one diagram.
const DIAGRAM_PROBES: u8 = 8;

/// Renders one block, at its own width if `measure.full` would clip it.
///
/// A Mermaid fence is asked first, because "this does not fit" reaches the clip hunt as
/// a *dump of Mermaid source* — a block that is not clipped at all and would never be
/// widened, only side-scrolled as raw text if some other line in it happened to be long.
/// [`crate::render::diagram`] answers with the diagram itself, at the narrowest width
/// that draws it, and that width is what the block is laid out at.
///
/// A display formula is asked next and for the same reason — "this does not fit" reaches
/// the clip hunt as a *dump of LaTeX*, and the search would then widen the block until the
/// formula drew, at any surplus at all. [`super::math::formula`] answers for every formula
/// that builds, so a formula never reaches the search; it is the one that applies
/// [`MIN_SURPLUS`], and it is handed `measure.prose` because a formula is centred in the
/// column of prose it belongs to rather than in the body it is laid out across.
fn render_widened(node: &Node, measure: Measure, doc: DocCtx<'_>, clip: &ClipTest) -> Canvas {
    let width = measure.full;
    let limits = Limits::new(
        MAX_BLOCK_WIDTH.min(width.saturating_mul(VIEWPORTS)),
        DIAGRAM_PROBES,
    );
    if let Some((at, canvas)) = crate::render::diagram(node, width, limits, doc.theme, doc.options)
        && (at == width || at - width >= MIN_SURPLUS)
    {
        return canvas;
    }
    if let Some((_, canvas)) = super::math::formula(node, width, measure.prose, limits, doc.ctx()) {
        // No guard: `formula` has already applied `MIN_SURPLUS` and answers with the
        // block to draw either way. Re-testing `at` here would be a second rule deciding
        // one width, and the range the two disagreed in is where the defect lived.
        return canvas;
    }
    let narrow = block::render_block_ctx(node, width, doc.ctx());
    let is_clipped = |canvas: &Canvas| clip.is_clipped(canvas);
    if !is_clipped(&narrow) || width >= MAX_BLOCK_WIDTH {
        return narrow;
    }
    let render = |at: u16| block::render_block_ctx(node, at, doc.ctx());

    // Double until the block fits, which bounds the search; then bisect for the
    // narrowest width that still fits, so the scrollable extent matches the content
    // rather than the probe that happened to find it.
    let mut clipped = width;
    let mut fits = width;
    let mut fitted = loop {
        fits = fits.saturating_mul(2).min(MAX_BLOCK_WIDTH);
        let canvas = render(fits);
        if !is_clipped(&canvas) || fits == MAX_BLOCK_WIDTH {
            break canvas;
        }
        clipped = fits;
    };
    while fits - clipped > 1 {
        let mid = clipped + (fits - clipped) / 2;
        let canvas = render(mid);
        if is_clipped(&canvas) {
            clipped = mid;
        } else {
            fits = mid;
            fitted = canvas;
        }
    }
    fitted
}

/// How far each row of `canvas` may usefully be scrolled sideways, in columns.
///
/// Horizontal scrolling exists for the handful of blocks that did not fit. Applying one
/// offset to every row drags the whole page: the heading disappears, paragraphs are
/// decapitated mid-word, and the columns the prose used to occupy go blank — for the
/// sake of one wide table further down. The viewport therefore offsets each row by
/// `min(offset, reach - viewport)`, and this is where `reach` comes from.
///
/// A row's own content extent would be the naive answer and shears blocks apart: a
/// diagram whose rows have ragged right edges would have every row stop at its own
/// extent, sliding arrows off the boxes they attach to. The unit is instead a *run* of
/// rows that scroll together, read off the drawn canvas the way `ruled_offsets` in the
/// graph engine reads a node's rules rather than being told about them:
///
/// * Consecutive non-blank rows belong to one run and share the run's widest extent.
///   [`render_document`] separates top-level blocks with a blank row, so a run is
///   normally exactly one block. A row drawing nothing but block-prefix decoration
///   counts as blank — see [`row_extent`], and without that a block quote is a single
///   run and one wide fence inside it drags every quoted sentence off the screen.
/// * Runs on both sides of a gap are merged when *both* reach past `width`, the width
///   the document was laid out at. Any number of blank rows is bridged, not just one, so
///   a block with a gap inside it — a loose list, a diagram with a gap between ranks — is
///   kept in one piece whenever the gap separates two over-wide parts. Two adjacent
///   over-wide blocks are merged too, and then scroll together; that is the price of not
///   being told where the block boundaries are, and the narrower of the two pays it. It
///   is not merely some blank space on the right: the narrower block travels the wider
///   one's distance, so at the far end of a 137-column table a merged 86-column table is
///   off-screen altogether, leaving its `‹` markers pointing at nothing.
/// * A run that fits within the viewport gets an offset of zero from the `min` above,
///   which is the whole point: prose, headings and narrow blocks stay at column 0.
///
/// The returned vector has one entry per canvas row.
pub fn scroll_reach(canvas: &Canvas, width: u16) -> Vec<u16> {
    let mut reach: Vec<u16> = canvas.rows().iter().map(|row| row_extent(row)).collect();
    let mut start = 0;
    let mut previous: Option<std::ops::Range<usize>> = None;
    while start < reach.len() {
        if reach[start] == 0 {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < reach.len() && reach[end] > 0 {
            end += 1;
        }
        let widest = reach[start..end].iter().copied().max().unwrap_or(0);
        let run = match previous {
            // Both sides over-wide: one block interrupted by a blank row, most likely.
            Some(before) if widest > width && reach[before.start] > width => before.start..end,
            _ => start..end,
        };
        let widest = reach[run.clone()].iter().copied().max().unwrap_or(0);
        for row in &mut reach[run.clone()] {
            *row = widest;
        }
        previous = Some(run);
        start = end;
    }
    reach
}

/// How many leading columns of each row stay put while the rest scrolls sideways.
///
/// `render::code` keeps the line-number gutter out of its *own* clip, so a long line is
/// cut to the right of the numbers rather than over them. That says nothing about the
/// pager: the horizontal offset moved every column of the row alike, so the numbers slid
/// off to the left exactly when a line long enough to scroll made them useful. A pinned
/// prefix is the fix — [`super::draw::blit`] draws these columns unscrolled and starts
/// the offset after them.
///
/// Unlike [`scroll_reach`], this is a *lookup*, not a measurement: the renderer that drew
/// the gutter records where it ends as a [`Pin`](crate::canvas::Pin) on the canvas, the
/// third metadata channel beside anchors and search spans, and this function only expands
/// that channel to one entry per row. See [`crate::render::code`] for what is published
/// and [`Canvas::pinned_prefix`] for how a pin travels through `append` and `indent`.
///
/// It is worth knowing why it is not measured. It was, once — by matching cell styles,
/// the way `mermaid::layout::graph::ruled_offsets` reads a node's rules — and that seam
/// was unsound twice over. The style it keyed on is not unique (`theme.code.line_number`
/// and `theme.code.operator` are one value in both shipped themes, so an *unnumbered*
/// fence containing an `=` was read as having a gutter), and the prefix it found was
/// spread over a contiguous run of non-blank rows on the argument that a run is "normally
/// exactly one block". Markdown containers emit blocks with no blank row between them, so
/// a numbered fence and a wide table in one list item are one run: the fence's gutter was
/// frozen over every row of the table, which read `aaaa‹bbbb…` — text in no document —
/// with its second column unreachable. Reading chrome off the canvas cannot tell which
/// block a row belongs to; only the renderer knows that, so the renderer says.
///
/// The returned vector has one entry per canvas row.
pub fn pinned_prefix(canvas: &Canvas) -> Vec<u16> {
    canvas.pinned_prefix()
}

/// The column one past the last thing a row actually draws.
///
/// Block-prefix decoration is not content. A block quote paints its bar on *every* row
/// it owns, including the rows that separate its parts, so a row whose only content is
/// that bar is a blank row in a costume — and counting it as content welded a whole
/// quote into one run, handing the quoted prose the reach of whatever wide fence or
/// table sat between the sentences. That is the whole-page scroll drag the per-run
/// offsets were introduced to remove, one nesting level down.
///
/// Splitting on such a row is safe for the ragged blocks runs exist to protect: box art
/// never draws a row of bars and nothing else, and if a widened block ever did, both
/// halves would reach past the render width and [`scroll_reach`]'s merge rule would put
/// it back together.
fn row_extent(cells: &[Cell]) -> u16 {
    let drawn = |cell: &Cell| !cell.is_blank() && !cell.is_continuation();
    if !cells
        .iter()
        .filter(|cell| drawn(cell))
        .any(|cell| cell.text() != QUOTE_BAR)
    {
        return 0;
    }
    cells
        .iter()
        .enumerate()
        .rev()
        .find(|(_, cell)| drawn(cell))
        .map_or(0, |(index, cell)| {
            u16::try_from(index)
                .unwrap_or(u16::MAX)
                .saturating_add(u16::from(cell.width()))
        })
}

/// Recognises the renderers' "content was cut off here" marker.
///
/// The marker is not always in the last column — a framed code block paints it inside
/// its own border — so the whole row has to be searched. Matching the glyph alone
/// would then mistake a document that merely *contains* the character for a clipped
/// one, so the marker's style has to match too.
///
/// Not every row of a clipped block carries the marker: a table closes its cut rules
/// with `╮ ┤ ╯` rather than marking them, so that the frame reads as a box that
/// continues rather than a box that broke. Detection therefore rests on the *content*
/// rows, which are cut by exactly the same amount and always exist — a box with no rows
/// between its rules is not a table. A renderer that ever marked no row at all would
/// switch horizontal scrolling off for its blocks in silence;
/// [`super::tests::a_clipped_table_is_still_detected_and_widened`] is the tripwire.
struct ClipTest {
    /// The styles the renderers paint the marker in.
    styles: [crate::theme::Style; 2],
}

impl ClipTest {
    /// Builds the test for one theme.
    fn new(theme: &Theme) -> Self {
        Self {
            styles: [theme.code.overflow_marker, theme.table.overflow_marker],
        }
    }

    /// Whether `canvas` carries a cut-off marker anywhere.
    fn is_clipped(&self, canvas: &Canvas) -> bool {
        canvas
            .rows()
            .iter()
            .flatten()
            .any(|cell| cell.text() == OVERFLOW_MARKER && self.styles.contains(&cell.style()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stage-1 insurance: without an arm in [`is_exempt`] a wide formula is squeezed
    /// to the body-width cap and clipped, exactly as a Mermaid diagram would be. This is
    /// the fourth hand-written `NodeKind` list on this codebase to go stale, and the
    /// previous three each shipped a visible defect.
    #[test]
    fn a_display_formula_is_exempt_from_the_prose_cap() {
        let doc = Doc::parse("$$\\frac{a + b + c + d + e + f}{2}$$");
        assert!(is_exempt(&doc.root().children[0]));
    }

    #[test]
    fn an_inline_formula_is_not_exempt() {
        // The exemption is about a block that has exactly one width. An inline formula is
        // part of a sentence and reflows with it, so it is capped like the prose it sits
        // in — and the node here is the paragraph, not the formula.
        let doc = Doc::parse("Some $x^2$ prose.");
        assert!(!is_exempt(&doc.root().children[0]));
    }

    /// A guard on the decoupling the owner ruled: exemption governs the width a formula is
    /// *laid out* at, and says nothing about what it is *centred* in. If this pair ever
    /// disagrees, one of the two rules has absorbed the other.
    #[test]
    fn a_wide_body_lays_a_formula_out_past_the_prose_cap_and_still_centres_it_in_the_cap() {
        let doc = Doc::parse("$$\\frac{a}{b}$$");
        let theme = Theme::default();
        let canvas = render_document(&doc, 120, Some(72), &theme, &RenderOptions::default());
        let rule = (0..canvas.height())
            .find(|row| canvas.row_text(*row).contains('─'))
            .expect("a rule row");
        let text = canvas.row_text(rule);
        let left = text.len() - text.trim_start().len();
        // 118 columns of body inside a 120-column terminal, one column of margin each
        // side; the formula is one column wide and centred in the 72-column text column,
        // so 1 + (72 - 1) / 2 = 36. Centred in the body it would be 1 + 58 = 59.
        assert_eq!(left, 36, "centred in the prose cap, not in the body");
    }

    /// The other half of the decoupling, and the half `is_exempt` is actually for: a
    /// formula wider than the cap is *drawn* at the body width instead of being squeezed
    /// into the cap and dumped as its own source.
    ///
    /// The macro is what makes this observable. A formula's drawn width and its source
    /// length are unrelated, and the cap only dumps a formula it cannot draw — so with a
    /// long source the dump is itself clipped, the clip test widens the block, and the
    /// formula comes back drawn anyway with the missing arm invisible. Forty characters
    /// of source drawing seventy-four columns is a dump that fits at the cap, which is
    /// the only shape in which the defect reaches a reader.
    #[test]
    fn a_formula_wider_than_the_cap_is_drawn_at_the_body_width_not_dumped_at_the_cap() {
        let doc = Doc::parse("$$\\newcommand{\\q}{a+b+c+d+e+f+g+h+i+j}\\q\\q$$");
        let theme = Theme::default();
        let canvas = render_document(&doc, 120, Some(72), &theme, &RenderOptions::default());
        let text = canvas.plain_text();
        assert!(
            !text.contains("newcommand"),
            "drawn, not dumped as its own source: {text}"
        );
    }
}
