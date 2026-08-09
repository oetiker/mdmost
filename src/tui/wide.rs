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
//! viewport width; the ones that come back clipped — and only those — are re-rendered
//! at the width they actually want. [`Canvas::append`] then stacks the parts, padding
//! the narrow ones, and translates their anchors and search spans, so the result obeys
//! the canvas contract exactly as a plain [`render_document`] would.
//!
//! The common document has nothing clipped, pays for no extra render, and comes out
//! byte-identical to [`render_document`] — including its side margins, which are
//! applied here for the same reason and in the same place. That claim used to be false
//! in exactly that one respect, and being false only about the margins is what made it
//! survive: the pager looked right until you noticed nothing had a gutter.

use crate::canvas::Canvas;
use crate::doc::{Doc, Node, NodeKind};
use crate::render::{RenderOptions, margins, render_block, render_document};
use crate::theme::Theme;

/// The glyph the renderers paint in a row's last column when content is cut off.
///
/// Duplicated from `render::code` on purpose: that constant is private to a private
/// module, and reaching into the renderer would couple the pager to its internals.
/// [`super::tests::overflow_marker_matches_the_renderer`] pins the two together.
pub const OVERFLOW_MARKER: &str = "\u{203a}";

/// The widest a single block is ever grown to.
///
/// A bound is needed because a pathological document — one enormous minified line —
/// would otherwise allocate a canvas proportional to its longest line. Content beyond
/// it stays clipped, which is the same outcome as today, only much further out.
const MAX_BLOCK_WIDTH: u16 = 2048;

/// Renders `doc` at `width`, widening any block that would otherwise be clipped.
///
/// The returned canvas is at least `width` columns wide and may be wider; the surplus
/// is what the horizontal scroll keys reach.
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

/// Renders one block, at its own width if `width` would clip it.
fn render_widened(
    node: &Node,
    width: u16,
    theme: &Theme,
    options: &RenderOptions,
    clip: &ClipTest,
) -> Canvas {
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

/// Recognises the renderers' "content was cut off here" marker.
///
/// The marker is not always in the last column — a framed code block paints it inside
/// its own border — so the whole row has to be searched. Matching the glyph alone
/// would then mistake a document that merely *contains* the character for a clipped
/// one, so the marker's style has to match too.
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
