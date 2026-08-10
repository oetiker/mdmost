//! Mouse text selection, mapped back onto the document source.
//!
//! With mouse capture on, the terminal's own drag-select is gone (see
//! [`Config::mouse`](crate::config::Config::mouse)), so the pager draws its own. What
//! makes that worth doing rather than merely a replacement: the pager knows which
//! *source* bytes produced each cell, so a drag over a rendered heading `◆ Wide
//! diagram` yields `# Wide diagram` and a drag over **bold** yields `**bold**`. The
//! reader copies the Markdown they came for, not a screenshot of it.
//!
//! # How the mapping works
//!
//! It is an inversion of search, not new plumbing. [`Canvas`] already carries
//! [`SearchSpan`]s — *source byte range → row, col, cols* — recorded by
//! [`render::inline`](crate::render::inline) and translated through every `blit`,
//! `indent`, `append` and `slice_rows` by `Canvas::merge_metadata`. `search` walks them
//! forwards, source → cells; [`extract`] walks them backwards, cells → source.
//!
//! # The four decisions
//!
//! **1. A selection is a hull of source bytes, taken verbatim.** Every span the
//! selected cells touch contributes the byte range it actually covers; the result is
//! `source[lo..hi]` where `lo` is the lowest and `hi` the highest byte covered. Nothing
//! is synthesised and nothing between the ends is dropped. This is what gives
//! multi-row selections *source* line structure rather than the renderer's: a reflowed
//! paragraph has wraps that exist nowhere in the source, and the hull simply does not
//! contain them — it contains the newlines the author typed. It is also why dragging
//! from a paragraph above a code fence to one below it yields the fence, verbatim,
//! including its fence lines: they lie between the ends of the hull.
//!
//! **2. Markup adjacent to a selected edge comes with it.** After the hull is taken,
//! each end is extended outward over bytes that *no span rendered* — `#`, `**`, `- `,
//! `[`, `](url)` — stopping at a newline. The rule is self-limiting and needs no
//! special case for partial selections: if the reader's drag cut into the middle of a
//! rendered run, the byte just outside the hull *is* inside a span, so no extension
//! happens and the covered range is returned verbatim. Dragging half of `**bold**` from
//! its start gives `**bol`; dragging its middle gives `ol`. A pager must not invent
//! syntax, and unbalanced-looking output here is the honest report of an unbalanced
//! drag. What it may do is include the delimiters the reader could not have selected
//! because they were never drawn.
//!
//! **3. Content with no spans falls back to what is on screen.** Spans are recorded by
//! the inline renderer and, per line, by `render::code::code_area` (design spec §3), so
//! a Mermaid diagram and a table's frame carry none, but a fenced or indented code
//! block does (a table *cell* is a nested inline render and does too). When a selection
//! touches no span at all, the rendered text of the selected cells is returned instead,
//! and [`Extract::from_source`] says so, so the status bar can too. For a diagram that
//! fallback is the box art the reader is looking at, which is at least what they
//! pointed at.
//!
//! The known limitation this leaves, stated plainly because a doc comment that hid it
//! would be the defect class this project keeps catching in itself: a drag that starts
//! in prose and *ends inside* spanless content copies only as far as the last byte the
//! renderer mapped — the paragraph, not the half of the diagram below it (a code fence
//! no longer demonstrates this: it carries spans now, so a drag ending inside one hits
//! decision 1 instead). There is no honest way to do better: the hull's far end is a
//! source offset and the cells below it have none, so any guess would either over-copy
//! the rest of the block or invent an offset. Ending the drag past the block, or inside
//! it on both ends, both give the right answer.
//!
//! **4. Coordinates are canvas coordinates, not viewport ones.** The selection is
//! anchored to the document, so scrolling — vertically or horizontally — during a drag
//! moves the viewport over a selection that stays put, which is the only behaviour that
//! makes selecting more than a screenful possible. A *resize* is the opposite case and
//! the selection is dropped: rendering is a pure function of width (design spec §3), so
//! after a reflow those canvas cells hold different text. Re-anchoring through the
//! source range is possible but would silently change what is highlighted, because the
//! hull at 100 columns is not the hull at 40; discarding is the honest answer and the
//! reader has already had the release event that copies.

use std::ops::Range;

use crate::canvas::Canvas;
use crate::text::{display_width, graphemes};

/// A position in document-canvas coordinates.
///
/// Canvas, not viewport: see the module's fourth decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    /// The canvas row.
    pub row: usize,
    /// The canvas column.
    pub col: u16,
}

impl Pos {
    /// A position at `row`, `col`.
    pub fn new(row: usize, col: u16) -> Self {
        Self { row, col }
    }
}

/// A drag in progress, or one the reader has finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Where the button went down.
    anchor: Pos,
    /// Where the pointer is now, inclusive.
    head: Pos,
    /// Whether the button is still down.
    dragging: bool,
}

impl Selection {
    /// Starts a drag at `anchor`.
    pub fn started(anchor: Pos) -> Self {
        Self {
            anchor,
            head: anchor,
            dragging: true,
        }
    }

    /// Moves the loose end.
    pub fn drag_to(&mut self, head: Pos) {
        self.head = head;
    }

    /// Ends the drag, leaving the highlight up.
    pub fn finish(&mut self) {
        self.dragging = false;
    }

    /// Whether the button is still down.
    pub fn is_dragging(self) -> bool {
        self.dragging
    }

    /// Whether the drag never moved off the cell it started on.
    ///
    /// A click is not a selection: it would copy one character, which nobody meant.
    pub fn is_click(self) -> bool {
        self.anchor == self.head
    }

    /// The two ends in document order.
    fn ordered(self) -> (Pos, Pos) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// The rows the selection touches.
    pub fn rows(self) -> std::ops::RangeInclusive<usize> {
        let (start, end) = self.ordered();
        start.row..=end.row
    }

    /// The columns of `row` that are selected, as a half-open interval.
    ///
    /// The selection flows like text rather than covering a rectangle: the first row
    /// runs from the anchor to the end of the row, the last row from its start to the
    /// pointer, and every row between them entirely. A block selection would be the
    /// wrong shape for prose, which is what this pager is mostly showing.
    pub fn columns_on(self, row: usize, width: u16) -> Option<Range<u16>> {
        let (start, end) = self.ordered();
        if row < start.row || row > end.row {
            return None;
        }
        let last = end.col.saturating_add(1).min(width);
        let first = start.col.min(width);
        let range = match (row == start.row, row == end.row) {
            (true, true) => first..last.max(first),
            (true, false) => first..width,
            (false, true) => 0..last,
            (false, false) => 0..width,
        };
        (range.start < range.end).then_some(range)
    }
}

/// The text a selection yielded, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extract {
    /// The text to put on the clipboard.
    pub text: String,
    /// Whether it is document source (`true`) or the rendered cells (`false`).
    ///
    /// The status bar reports the difference: telling a reader they copied Markdown
    /// when they copied box art would be a lie of exactly the kind this project keeps
    /// finding in its own doc comments.
    pub from_source: bool,
}

/// Extracts the source behind a selection, falling back to the rendered cells.
///
/// Returns `None` when the selection covers nothing at all — an empty region, or one
/// entirely off the bottom of the canvas.
pub fn extract(canvas: &Canvas, source: &str, selection: Selection) -> Option<Extract> {
    if let Some((lo, hi)) = source_hull(canvas, source, selection) {
        let (lo, hi) = extend_over_markup(canvas, source, lo, hi);
        let text = source.get(lo..hi).unwrap_or_default();
        if !text.is_empty() {
            return Some(Extract {
                text: text.to_string(),
                from_source: true,
            });
        }
    }
    let text = rendered_text(canvas, selection);
    (!text.is_empty()).then_some(Extract {
        text,
        from_source: false,
    })
}

/// The lowest and highest source byte the selected cells cover.
fn source_hull(canvas: &Canvas, source: &str, selection: Selection) -> Option<(usize, usize)> {
    let width = canvas.width();
    let mut lo = usize::MAX;
    let mut hi = 0usize;
    for span in canvas.spans() {
        let Some(wanted) = selection.columns_on(span.row, width) else {
            continue;
        };
        let first = wanted.start.max(span.col);
        let last = wanted.end.min(span.col.saturating_add(span.cols));
        if first >= last {
            continue;
        }
        let body = source
            .get(span.source_start..span.source_end)
            .unwrap_or_default();
        let start = span.source_start + byte_at_column(body, first - span.col);
        let end = span.source_start + byte_at_column(body, last - span.col);
        if start >= end {
            continue;
        }
        lo = lo.min(start);
        hi = hi.max(end);
    }
    (lo < hi).then_some((lo, hi))
}

/// The byte offset in `text` that `columns` display columns land on.
///
/// The exact inverse of the column arithmetic `search::segments_for` does in the other
/// direction, and grapheme-wise for the same reason: a double-width cluster is two
/// columns and one boundary, so counting bytes or `char`s would land inside it.
fn byte_at_column(text: &str, columns: u16) -> usize {
    let wanted = usize::from(columns);
    let mut used = 0usize;
    let mut offset = 0usize;
    for cluster in graphemes(text) {
        if used >= wanted {
            break;
        }
        used += display_width(cluster);
        offset += cluster.len();
    }
    offset
}

/// Widens `lo..hi` over source bytes the renderer never drew.
///
/// `#`, `**`, `- `, `[`, `](url)`, a fence's info string: the reader could not have
/// dragged over them, so a selection that reaches the edge of what *was* drawn is
/// taken to include the markup that made it. The walk stops at a newline, so one word
/// can never swallow the line above it, and it stops the moment it meets a byte that a
/// span does render — which is what makes a partial selection come back verbatim
/// without a special case for it.
fn extend_over_markup(
    canvas: &Canvas,
    source: &str,
    mut lo: usize,
    mut hi: usize,
) -> (usize, usize) {
    let mut covered: Vec<(usize, usize)> = canvas
        .spans()
        .iter()
        .map(|span| (span.source_start, span.source_end))
        .collect();
    covered.sort_unstable();
    let rendered = |offset: usize| {
        covered
            .iter()
            .any(|&(start, end)| (start..end).contains(&offset))
    };
    while lo > 0 {
        let Some(previous) = source[..lo].char_indices().next_back().map(|(at, _)| at) else {
            break;
        };
        if source[previous..].starts_with('\n') || rendered(previous) {
            break;
        }
        lo = previous;
    }
    while hi < source.len() {
        if source[hi..].starts_with('\n') || rendered(hi) {
            break;
        }
        let step = source[hi..].chars().next().map_or(1, char::len_utf8);
        hi += step;
    }
    (lo, hi)
}

/// The plain text of the selected cells, for content that carries no spans.
///
/// Rows are right-trimmed and joined with newlines: the canvas pads every row out to
/// its full width, and copying that padding would put a rectangle of spaces on the
/// clipboard.
fn rendered_text(canvas: &Canvas, selection: Selection) -> String {
    let width = canvas.width();
    let mut rows: Vec<String> = Vec::new();
    for row in selection.rows() {
        let Some(cells) = canvas.row(row) else { break };
        let Some(wanted) = selection.columns_on(row, width) else {
            continue;
        };
        let text: String = cells
            .iter()
            .skip(usize::from(wanted.start))
            .take(usize::from(wanted.end - wanted.start))
            .map(crate::canvas::Cell::text)
            .collect();
        rows.push(text.trim_end().to_string());
    }
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows.join("\n")
}
