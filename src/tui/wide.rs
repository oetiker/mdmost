//! Rendering the document so that over-wide blocks stay reachable.
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
//! exactly as a plain [`render_document`] would.
//!
//! A Mermaid fence is the exception to "clipped" being the signal, and to matching
//! [`render_document`] block for block. It never comes back clipped — a diagram that
//! does not fit comes back as a *dump of its own source*, which fits fine — so it is
//! asked first, through [`crate::render::diagram`], and it is asked under a policy that
//! refuses to mince labels ([`Fit::ROOMY`](crate::mermaid::Fit::ROOMY)). A fence the
//! piped renderer squeezes into the viewport may therefore be drawn wide here instead.
//! That is the one deliberate difference; everything else in a document with nothing
//! over-wide comes out byte-identical to [`render_document`] — including its side
//! margins, which are applied here for the same reason and in the same place. That claim
//! used to be false in exactly one other respect, and being false only about the margins
//! is what made it survive: the pager looked right until you noticed nothing had a
//! gutter.

use crate::canvas::{Canvas, Cell};
use crate::doc::{Doc, Node, NodeKind};
use crate::render::{Limits, RenderOptions, margins, render_block, render_document};
use crate::theme::Theme;

/// The glyph the renderers paint in a row's last column when content is cut off.
///
/// Duplicated from `render::code` on purpose: that constant is private to a private
/// module, and reaching into the renderer would couple the pager to its internals.
/// [`super::tests::overflow_marker_matches_the_renderer`] pins the two together.
pub const OVERFLOW_MARKER: &str = "\u{203a}";

/// The bar a block quote paints down its left edge, on every row it owns.
///
/// Duplicated from `render::block` for the same reason as [`OVERFLOW_MARKER`], and
/// pinned to it by [`super::tests::the_quote_bar_matches_the_renderer`].
pub const QUOTE_BAR: &str = "\u{258c}";

/// The glyph a numbered code block draws between its line numbers and its code.
///
/// Duplicated from `render::code` for the same reason as [`OVERFLOW_MARKER`], and pinned
/// to it — glyph, style and layout together — by
/// [`super::tests::the_gutter_rule_matches_the_renderer`].
pub const GUTTER_RULE: &str = "\u{2502}";

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
pub fn render_scrollable(doc: &Doc, width: u16, theme: &Theme, options: &RenderOptions) -> Canvas {
    let clipped = ClipTest::new(theme);
    let blocks = &doc.root().children;
    // The top level of a document is a sequence of blocks. Should a parser change ever
    // put bare inline content there, `render_sequence` groups it into one wrapped run
    // and this per-block assembly would not — so hand those documents back to the
    // renderer whole rather than laying them out differently here.
    if blocks.iter().any(is_inline) {
        return render_document(doc, width, theme, options);
    }

    let fill = theme.base();
    // The margin is applied here, over the assembled body, exactly as `render_document`
    // applies it over `render_sequence` — assembling blocks ourselves means inheriting
    // that job too, and for a long time we did not: every line in the pager sat welded
    // to the scrollbar while the piped renderer was correctly inset.
    let margin = margins(width);
    let body_width = width - 2 * margin;
    let mut out = Canvas::empty(body_width);
    for node in blocks {
        let part = render_widened(node, body_width, theme, options, &clipped);
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
    out.resize_width(out.width().max(body_width), fill);
    // A widened block keeps its surplus past the right margin; that surplus is what the
    // horizontal scroll reaches, and a gutter drawn at its far edge would be off-screen
    // anyway.
    let mut out = out.indent(margin, margin, fill);
    out.resize_width(out.width().max(width), fill);
    out
}

/// How much wider than the viewport a diagram must want before it is granted the width.
///
/// A diagram needing one more column than it has is not worth giving the whole document
/// a horizontal scrollbar, a chevron on every row and an `↔ 1/1` readout for. Below this
/// it takes the fit ladder's answer instead, squeezed or dumped — the same thing it got
/// before diagrams could scroll at all.
const MIN_SURPLUS: u16 = 8;

/// How many viewports wide a diagram may be laid out.
///
/// Past this the source dump is the better answer: crossing a diagram three screens wide
/// is already 160 arrow presses, and [`Canvas::append`] pads every row of the document to
/// the widest part, so the whole canvas pays for it. Revisit if a page-left/right or
/// jump-to-edge binding ever lands.
const VIEWPORTS: u16 = 3;

/// The most layouts the width search may spend on one diagram.
const DIAGRAM_PROBES: u8 = 8;

/// Renders one block, at its own width if `width` would clip it.
///
/// A Mermaid fence is asked first, because "this does not fit" reaches the clip hunt as
/// a *dump of Mermaid source* — a block that is not clipped at all and would never be
/// widened, only side-scrolled as raw text if some other line in it happened to be long.
/// [`crate::render::diagram`] answers with the diagram itself, at the narrowest width
/// that draws it, and that width is what the block is laid out at.
fn render_widened(
    node: &Node,
    width: u16,
    theme: &Theme,
    options: &RenderOptions,
    clip: &ClipTest,
) -> Canvas {
    let limits = Limits::new(
        MAX_BLOCK_WIDTH.min(width.saturating_mul(VIEWPORTS)),
        DIAGRAM_PROBES,
    );
    if let Some((at, canvas)) = crate::render::diagram(node, width, limits, theme, options)
        && (at == width || at - width >= MIN_SURPLUS)
    {
        return canvas;
    }
    let narrow = render_block(node, width, theme, options);
    let is_clipped = |canvas: &Canvas| clip.is_clipped(canvas);
    if !is_clipped(&narrow) || width >= MAX_BLOCK_WIDTH {
        return narrow;
    }
    let render = |at: u16| render_block(node, at, theme, options);

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
///   [`render_scrollable`] separates top-level blocks with a blank row, so a run is
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
/// Where the gutter ends is read off the drawn canvas rather than recomputed from the
/// renderer's arithmetic, the way [`scroll_reach`] reads extents and the way
/// `mermaid::layout::graph::ruled_offsets` reads a node's rules. The signal is the
/// *style*: the gutter's digits are the only cells painted in `theme.code.line_number`,
/// which is far more reliable than counting digits or matching `│` — a glyph this
/// project's own documents are full of. The rule that closes the gutter is then the next
/// [`GUTTER_RULE`] in `theme.code.frame`, and the pinned prefix runs one column past it,
/// so the blank column separating gutter from code is kept and offset zero stays
/// byte-identical to no pinning at all.
///
/// A fence's language label is pinned on top of that, by the third style —
/// `theme.code.language`, which nothing else is painted in. It is chrome exactly as the
/// numbers are, and a prefix that stopped short of it would leave half a word standing in
/// a box rule. That extension is per row and only applies where the run is pinned
/// already; see the loop.
///
/// The prefix is spread over each *contiguous* non-blank run — the same run rule
/// [`scroll_reach`] starts from, deliberately not its merged one. Within a run it keeps a
/// fence's `╭`, `┬` and `╰` aligned with the rule below them, so the box does not open up
/// as the code slides under it; using the merged runs instead would pin the first columns
/// of a wide *table* that happens to sit next to a numbered fence, which is a design
/// question nobody has asked.
///
/// One consequence worth naming, in the same family as the `‹` that ends up pointing at
/// nothing: when a merged run drags a numbered block far past its own content, the
/// numbers stay while the code they belong to has scrolled entirely behind them. They are
/// still the right numbers for the rows they sit on.
///
/// The returned vector has one entry per canvas row.
pub fn pinned_prefix(canvas: &Canvas, theme: &Theme) -> Vec<u16> {
    let mut pinned: Vec<u16> = canvas
        .rows()
        .iter()
        .map(|row| gutter_end(row, theme))
        .collect();
    let mut start = 0;
    while start < pinned.len() {
        if row_extent(&canvas.rows()[start]) == 0 {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < pinned.len() && row_extent(&canvas.rows()[end]) > 0 {
            end += 1;
        }
        let widest = pinned[start..end].iter().copied().max().unwrap_or(0);
        for (row, cells) in pinned[start..end]
            .iter_mut()
            .zip(&canvas.rows()[start..end])
        {
            // The label a fence writes into its top rule is chrome for the same reason
            // the numbers are, and cut in the middle it leaves a fragment of a word
            // sitting in a box rule — `╭  ru────╮`. It is extended per row and only where
            // the run is pinned already: an unnumbered fence pinning its label alone
            // would hold its top-left corner still while its code slid out from under it.
            *row = widest.max(if widest > 0 {
                title_end(cells, theme)
            } else {
                0
            });
        }
        start = end;
    }
    pinned
}

/// The first column of code on a numbered code row, or zero when the row has no gutter.
fn gutter_end(cells: &[Cell], theme: &Theme) -> u16 {
    let number = cells
        .iter()
        .position(|cell| !cell.is_blank() && cell.style() == theme.code.line_number);
    let Some(number) = number else { return 0 };
    cells
        .iter()
        .enumerate()
        .skip(number)
        .find(|(_, cell)| cell.text() == GUTTER_RULE && cell.style() == theme.code.frame)
        .map_or(0, |(index, _)| {
            u16::try_from(index).unwrap_or(u16::MAX).saturating_add(2)
        })
}

/// The column one past a fence's language label, blank separator included.
///
/// Zero on every row that carries no label, which is every row but a fence's top rule:
/// `theme.code.language` is painted nowhere else, icon included.
fn title_end(cells: &[Cell], theme: &Theme) -> u16 {
    cells
        .iter()
        .enumerate()
        .rev()
        .find(|(_, cell)| !cell.is_blank() && cell.style() == theme.code.language)
        .map_or(0, |(index, cell)| {
            u16::try_from(index)
                .unwrap_or(u16::MAX)
                .saturating_add(u16::from(cell.width()))
                .saturating_add(1)
        })
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

/// Whether a node is inline content rather than a block of its own.
///
/// Mirrors `render::block::is_inline`, which is private; only the top level of a
/// document is tested against it, and only to decline the per-block path.
fn is_inline(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::Text(_)
            | NodeKind::SoftBreak
            | NodeKind::LineBreak
            | NodeKind::Code { .. }
            | NodeKind::Emph
            | NodeKind::Strong
            | NodeKind::Strikethrough
            | NodeKind::Link { .. }
            | NodeKind::FootnoteReference { .. }
            | NodeKind::SkippedHtml { block: false, .. }
    )
}
