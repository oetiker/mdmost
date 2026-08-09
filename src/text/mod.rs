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
//!   The one exception is [`cell_clusters`], which fills terminal cells and must go
//!   inside a cluster that no cell can hold — and even there the cut is chosen by
//!   measurement, and a cluster with no honest cut is replaced rather than mangled.

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
/// [`cell_clusters`] first, which never yields a piece wider than two columns — it
/// divides what can be divided and replaces what cannot — so the clamp here never
/// fires. A width of `0` (a lone combining mark, a zero-width joiner) is legal and is
/// merged into the preceding cell by the canvas.
///
/// Whether a given cluster is affected is a question of measurement, never of which
/// script or sequence it belongs to. A ZWJ emoji sequence and a regional-indicator flag
/// each *measure* two columns and so pass through untouched; the same sequence carrying
/// a spacing mark measures three and does not.
pub fn grapheme_width(cluster: &str) -> u8 {
    match cluster.width() {
        0 => 0,
        1 => 1,
        _ => 2,
    }
}

/// The glyph standing in for a cluster no arrangement of cells can hold.
///
/// One column wide, which is what makes it usable as the head of a padded run.
pub const UNPLACEABLE: &str = "\u{FFFD}";

/// The one-column text a control character is drawn as, or `None` for ordinary text.
///
/// A control character is not text: it is an instruction to the terminal, and writing
/// one into a cell hands that instruction straight through to the screen. `unicode-width`
/// prices every character in Unicode category `Cc` — C0, `DEL`, C1 — at one column, so
/// the whole program's arithmetic (wrapping, table negotiation,
/// [`Canvas::check_invariants`](crate::canvas::Canvas::check_invariants)) has already
/// counted such a character as occupying exactly one column by the time it reaches a
/// cell. The terminal then disagrees: a `TAB` jumps to the next tab stop, so the row is
/// drawn some six columns wider than the width it was laid out at; an `ESC` opens an
/// escape sequence and a document can repaint the screen from inside a paragraph.
///
/// Both are the same defect — the canvas measured one thing and the terminal drew
/// another — and both are answered here, by substituting **one column of real text**.
/// Same width in, same width out, so nothing measured upstream moves:
///
/// * whitespace controls (`\t`, `\n`, `\v`, `\f`, `\r`) become a space, which is what
///   they mean in prose. A tab is *not* expanded to a tab stop here: its column was
///   negotiated as one, long before this point, and inside a code block — the one place
///   a tab's alignment carries information — `highlight::expand_tabs` has already
///   expanded it against the real column, before anything measured the line.
/// * every other control character becomes [`UNPLACEABLE`], because it has no textual
///   meaning at all and silently dropping it would move every column after it.
///
/// This is stated as a *category* — `char::is_control` is exactly Unicode `Cc` — rather
/// than as a list of the characters somebody has complained about, so the class is
/// closed rather than the instance.
pub fn control_substitute(ch: char) -> Option<&'static str> {
    if !ch.is_control() {
        return None;
    }
    Some(if ch.is_whitespace() { " " } else { UNPLACEABLE })
}

/// Splits `text` into pieces that each occupy exactly one terminal cell.
///
/// This is [`graphemes`] with three extra rules, all stated as measurements:
///
/// * a control character — Unicode `Cc`, which is `TAB` and `ESC` as much as `NUL` — is
///   replaced by one column of real text (see [`control_substitute`]), because it is
///   priced at one column by every measurement in the program and drawn at some other
///   width, or not drawn at all, by the terminal;
///
/// * a cluster too wide for a single cell is broken at the widest point that *preserves
///   its total width* (see `leading_cell`) — in practice a spacing mark (category `Mc`)
///   attached to something already two columns wide;
/// * a cluster too wide for a single cell that admits **no** width-preserving break is
///   replaced by [`UNPLACEABLE`] followed by blanks, together exactly as wide as the
///   cluster it stands in for.
///
/// A ZWJ emoji sequence or a flag is never broken *on its own*, because `unicode-width`
/// measures those at two columns and they never enter the splitting path. But do not
/// read that as "sequences are untouched": a ZWJ sequence carrying a spacing mark
/// measures three and does enter it, and the cut must then fall after the joined run
/// rather than inside it. Splitting inside a join is one of the two failures this
/// function is written to avoid, so neither rule is stated as a list of characters.
///
/// The guarantee that holds for every input is about *columns*: the widths of the pieces
/// sum to exactly `display_width(text)`, and no piece is wider than two columns. A row
/// filled from them is therefore as wide as the cells claim, which is what design spec
/// §4 requires and what [`Cell::new`](crate::canvas::Cell::new) asserts.
///
/// The pieces also concatenate back to `text` exactly — *except* for a replaced cluster
/// and for a substituted control character, where the author's character is gone. That
/// is not a lapse: a cluster of three columns
/// with nothing inside it to divide (U+17D8 KHMER SIGN BEYYAL is the only such scalar
/// under `unicode-width` 0.2, but composed clusters reach the same state) cannot be put
/// into cells at all. The alternatives were to let a cell claim two columns and draw
/// three, which is the canvas-contract violation this whole area exists to prevent, or
/// to widen the contract so a cell may be three columns wide, which changes every
/// consumer of the grid for one sign. Standing a same-width marker in its place keeps
/// the grid honest, keeps the column arithmetic callers already did with
/// [`display_width`] exact — so everything after it stays in the column it was laid out
/// in — and leaves a visible sign that something was there, rather than deleting it
/// silently.
pub fn cell_clusters(text: &str) -> CellClusters<'_> {
    CellClusters {
        rest: text,
        blanks: 0,
    }
}

/// The iterator returned by [`cell_clusters`].
#[derive(Debug, Clone)]
pub struct CellClusters<'a> {
    rest: &'a str,
    /// Blank columns still owed for a cluster that was replaced by a marker.
    blanks: usize,
}

impl<'a> Iterator for CellClusters<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.blanks > 0 {
            self.blanks -= 1;
            return Some(" ");
        }
        let cluster = graphemes(self.rest).next()?;
        // A control character is stopped here, at the last point before text becomes
        // cells, so that no renderer has to remember to do it — see
        // [`control_substitute`]. It is taken off the *character*, not the cluster: a
        // `\r\n` is one cluster of two controls and each owes its own column, and a
        // control carrying a combining mark leaves the mark to be merged into the
        // preceding cell as usual.
        if let Some(ch) = self.rest.chars().next()
            && let Some(substitute) = control_substitute(ch)
        {
            debug_assert_eq!(display_width(&ch.to_string()), display_width(substitute));
            self.rest = &self.rest[ch.len_utf8()..];
            return Some(substitute);
        }
        let piece = if display_width(cluster) <= 2 {
            cluster
        } else {
            match leading_cell(cluster) {
                Some(head) => head,
                None => {
                    // Nothing can be cut off this cluster without changing how many
                    // columns it draws, and it does not fit a cell whole. Replace it.
                    self.rest = &self.rest[cluster.len()..];
                    self.blanks = display_width(cluster) - 1;
                    debug_assert_eq!(display_width(UNPLACEABLE), 1);
                    return Some(UNPLACEABLE);
                }
            }
        };
        // Every branch consumes at least one character, so this cannot spin.
        debug_assert!(!piece.is_empty(), "cell piece must make progress");
        self.rest = &self.rest[piece.len()..];
        Some(piece)
    }
}

/// The leading one-cell piece of an over-wide cluster, if the cluster can be divided at
/// all.
///
/// The split point is chosen by *measurement*, not by character category: the longest
/// prefix that still fits a cell (at most two columns) and whose width, plus the width
/// of the remainder, is exactly the width of the whole cluster.
///
/// That last condition is the one that matters, and it is not implied by the first.
/// A prefix can measure two columns on its own and still be a ruinous split, because
/// characters that were *joined* inside the cluster draw as one unit only while they
/// stay together: cutting a ZWJ emoji sequence in half yields two pieces of two columns
/// each, but writing them into adjacent cells re-joins them on screen, where they draw
/// two columns in total rather than four. The cells are then each honest about their own
/// text and the row is still short — which is exactly how this survived a per-cell
/// invariant check. Measuring the remainder catches every such join without having to
/// enumerate which characters join.
///
/// `None` means no such prefix exists — either the cluster is a single character, or
/// every boundary in it would change the total. There is then no honest way to fill
/// cells from it, and [`cell_clusters`] replaces it rather than cutting it somewhere
/// that lies about its width.
fn leading_cell(cluster: &str) -> Option<&str> {
    let whole = display_width(cluster);
    let mut split = None;
    for (at, _) in cluster.char_indices().skip(1) {
        let head = display_width(&cluster[..at]);
        if head <= 2 && head + display_width(&cluster[at..]) == whole {
            split = Some(at);
        }
    }
    split.map(|end| &cluster[..end])
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
