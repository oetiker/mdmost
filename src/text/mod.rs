//! Grapheme-cluster-safe, display-width-aware text primitives.
//!
//! This module is the *single* home of width and grapheme logic in `mdless`. The
//! inline renderer, the table column negotiator, the code highlighter and every
//! Mermaid engine reuse these functions; duplicating any of this logic elsewhere is a
//! defect.
//!
//! Two rules hold everywhere:
//!
//! * text is measured in **display columns** via `unicode-width`, never in bytes or
//!   `char`s;
//! * text is split only on **grapheme cluster** boundaries via `unicode-segmentation`,
//!   so combining marks, ZWJ emoji sequences and regional-indicator flags stay intact.

mod span;
mod wrap;

#[cfg(test)]
mod tests;

pub use span::{Line, Span, spans_min_width, spans_width};
pub use wrap::{wrap_plain, wrap_spans};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Horizontal alignment, used by tables, captions and diagram labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    /// Flush left.
    #[default]
    Left,
    /// Centred; a leftover odd column goes to the right.
    Center,
    /// Flush right.
    Right,
}

/// Iterates the grapheme clusters of `text`.
///
/// This is the only permitted way to walk text character-by-character.
pub fn graphemes(text: &str) -> impl Iterator<Item = &str> {
    text.graphemes(true)
}

/// The display width of `text` in terminal columns.
pub fn display_width(text: &str) -> usize {
    text.width()
}

/// The display width of a **one-cell piece** of text, in `0..=2`.
///
/// A terminal cell holds at most a double-width cluster, so this returns at most `2`.
/// That ceiling is a *backstop*, not a guarantee about the input: a grapheme cluster
/// can genuinely be wider than two columns — a wide base character followed by a
/// spacing combining mark (Unicode category `Mc`) advances the cursor by three — and
/// clamping such a cluster to `2` makes the cell claim a width its own text does not
/// have. That mismatch was a live canvas-contract violation (design spec §4) for the
/// whole of the project's early life.
///
/// Callers that are filling cells must therefore split their text with
/// [`cell_clusters`] first, which never yields a piece wider than two columns; the
/// clamp here then never fires. A width of `0` (a lone combining mark, a zero-width
/// joiner) is legal and is merged into the preceding cell by the canvas.
///
/// ZWJ emoji sequences and regional-indicator flags are *not* affected: `unicode-width`
/// already measures those clusters as two columns, which is what terminals draw.
pub fn grapheme_width(cluster: &str) -> u8 {
    match cluster.width() {
        0 => 0,
        1 => 1,
        _ => 2,
    }
}

/// Splits `text` into pieces that each occupy exactly one terminal cell.
///
/// This is [`graphemes`] with one extra rule: a cluster too wide for a single cell is
/// broken into a piece per advancing character, each carrying the zero-width marks that
/// follow it. Only clusters of three columns or more are ever broken — in practice a
/// wide base plus a spacing mark — so combining marks, ZWJ emoji sequences and flags
/// are untouched, because `unicode-width` already measures those at two columns or
/// fewer.
///
/// The pieces concatenate back to `text` exactly, so nothing the author wrote is lost;
/// only the column accounting changes, from a lie to the truth.
pub fn cell_clusters(text: &str) -> CellClusters<'_> {
    CellClusters { rest: text }
}

/// The iterator returned by [`cell_clusters`].
#[derive(Debug, Clone)]
pub struct CellClusters<'a> {
    rest: &'a str,
}

impl<'a> Iterator for CellClusters<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let cluster = graphemes(self.rest).next()?;
        let piece = if display_width(cluster) <= 2 {
            cluster
        } else {
            leading_cell(cluster)
        };
        // Every branch consumes at least one character, so this cannot spin.
        debug_assert!(!piece.is_empty(), "cell piece must make progress");
        self.rest = &self.rest[piece.len()..];
        Some(piece)
    }
}

/// The leading one-cell piece of an over-wide cluster.
///
/// That is its first character together with any zero-width characters trailing it;
/// the next advancing character starts the following piece.
fn leading_cell(cluster: &str) -> &str {
    let mut end = cluster.len();
    for (at, ch) in cluster.char_indices() {
        if at == 0 {
            end = ch.len_utf8();
            continue;
        }
        if display_width(ch.encode_utf8(&mut [0u8; 4])) == 0 {
            end = at + ch.len_utf8();
        } else {
            end = at;
            break;
        }
    }
    &cluster[..end]
}

/// Hands `extra` columns out across `slots`, as evenly as whole columns allow.
///
/// Every slot grows by `extra / slots.len()`, and the `extra % slots.len()` columns
/// that cannot be split go to the leftmost slots — so the result is deterministic and
/// the total handed out is exactly `extra`. Distributing slack is the last step of
/// every column negotiation in the program (table column widths, sequence-diagram
/// participant spacing), and it was written out by hand each time.
///
/// An empty `slots` swallows the slack rather than dividing by zero.
pub fn distribute_evenly(slots: &mut [usize], extra: usize) {
    if slots.is_empty() || extra == 0 {
        return;
    }
    let share = extra / slots.len();
    let leftover = extra % slots.len();
    for (at, slot) in slots.iter_mut().enumerate() {
        *slot += share + usize::from(at < leftover);
    }
}

/// The width of the longest run of text in `text` that cannot be broken by wrapping.
///
/// This is the minimum column budget a piece of text needs before wrapping has to
/// resort to splitting inside a word. Table column negotiation (design spec §7.1) is
/// defined in terms of this value.
pub fn min_unbreakable_width(text: &str) -> usize {
    text.split_whitespace()
        .map(display_width)
        .max()
        .unwrap_or(0)
}

/// Truncates `text` to at most `max_width` display columns.
///
/// The cut never lands inside a grapheme cluster. If a double-width cluster straddles
/// the limit it is dropped entirely, so the result is at most `max_width` columns wide
/// (it may be one column narrower).
pub fn truncate_to_width(text: &str, max_width: usize) -> &str {
    let mut width = 0usize;
    for (offset, cluster) in text.grapheme_indices(true) {
        // The cluster's *true* width, not a cell's capacity: costing a three-column
        // cluster as two would let the result exceed `max_width`, which is the one
        // promise this function makes.
        let w = display_width(cluster);
        if width + w > max_width {
            return &text[..offset];
        }
        width += w;
    }
    text
}

/// The glyph marking text that had to be shortened.
///
/// One idiom for the whole program: the visual review found three different truncation
/// markers in use, which reads as three different features rather than one.
pub const ELLIPSIS: &str = "…";

/// Shortens `text` to at most `width` display columns, marking the cut with `…`.
///
/// Text that already fits is returned unchanged. Cuts land on grapheme cluster
/// boundaries, so combining marks and emoji sequences survive; the result may come out
/// a column narrower than `width` when a double-width cluster would have straddled the
/// limit, but it is never wider.
///
/// This is the single implementation of "shorten and mark it". [`Canvas::push_text_ellipsized`]
/// and the Mermaid chart chrome both call it rather than keeping their own copy.
///
/// [`Canvas::push_text_ellipsized`]: crate::canvas::Canvas::push_text_ellipsized
pub fn ellipsize(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    format!("{}{ELLIPSIS}", truncate_to_width(text, width - 1))
}

/// Splits `text` at `at_width` display columns, returning the part before and after.
///
/// As with [`truncate_to_width`], a cluster that straddles the split point is not
/// divided; it goes to the second half.
pub fn split_at_width(text: &str, at_width: usize) -> (&str, &str) {
    let head = truncate_to_width(text, at_width);
    (head, &text[head.len()..])
}

/// Pads `text` with spaces to exactly `width` display columns using `align`.
///
/// Text wider than `width` is truncated with [`truncate_to_width`], then padded, so
/// the result is always exactly `width` columns wide.
pub fn pad_to_width(text: &str, width: usize, align: Align) -> String {
    let text = truncate_to_width(text, width);
    let slack = width.saturating_sub(display_width(text));
    let (left, right) = match align {
        Align::Left => (0, slack),
        Align::Right => (slack, 0),
        Align::Center => (slack / 2, slack - slack / 2),
    };
    let mut out = String::with_capacity(text.len() + slack);
    out.extend(std::iter::repeat_n(' ', left));
    out.push_str(text);
    out.extend(std::iter::repeat_n(' ', right));
    out
}

/// Repeats `cluster` until the result is `width` display columns wide.
///
/// Used for rules, borders and fills. If `cluster` is double-width and `width` is odd,
/// the last column is filled with a space so the result is exactly `width` columns.
/// A zero-width `cluster` yields `width` spaces rather than looping forever.
pub fn repeat_to_width(cluster: &str, width: usize) -> String {
    let w = usize::from(grapheme_width(cluster));
    if w == 0 {
        return " ".repeat(width);
    }
    let count = width / w;
    let mut out = cluster.repeat(count);
    out.extend(std::iter::repeat_n(' ', width - count * w));
    out
}
