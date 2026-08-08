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

/// The display width of a single grapheme cluster, clamped to `0..=2`.
///
/// Terminal cells hold at most a double-width cluster; anything `unicode-width`
/// reports as wider is clamped so that the canvas contract cannot be violated.
/// A cluster of width `0` (a lone combining mark, a zero-width joiner) is legal here
/// and is merged into the preceding cell by the canvas.
pub fn grapheme_width(cluster: &str) -> u8 {
    match cluster.width() {
        0 => 0,
        1 => 1,
        _ => 2,
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
        let w = usize::from(grapheme_width(cluster));
        if width + w > max_width {
            return &text[..offset];
        }
        width += w;
    }
    text
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
