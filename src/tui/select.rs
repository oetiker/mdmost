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
    ///
    /// This is a *cell* interval — it makes no distinction between text and chrome —
    /// so [`highlighted_columns`] does not call it: the wash is built from
    /// [`source_hull`] instead, which only ever covers spans. What still calls this is
    /// [`rendered_text`], the spanless fallback (design spec §2, decision 3): a
    /// diagram's box art has no source hull to consult, so what the reader dragged
    /// over is answered the only way left — by the cells themselves.
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

/// The source range a selection covers.
///
/// Two endpoints, resolved to source offsets, and everything between them — which is
/// document order, not screen geometry. A wrapped table cell therefore continues into
/// the *next cell* rather than into whatever sits beside it on the same screen row, and
/// a drag whose corners describe a rectangle still selects what a reader would read
/// between them (design spec §2).
///
/// `end`'s column is a mouse cell — inclusive, like every other column this pager
/// hands to `columns_on` — but `offset_at` resolves a column to the byte *at* it, not
/// past it, so the far endpoint is probed one column beyond where the drag actually
/// ended. That is what makes `Bias::End`'s "end of the previous span" fallback land on
/// the end of the clicked word rather than its last-but-one byte (design spec §2.1:
/// "a release past the end of a line takes the end of the last span on that row").
///
/// The two offsets are used exactly as `offset_at` returns them, with no reordering:
/// `Bias::Start`'s and `Bias::End`'s chrome fallbacks are inverted on purpose so that a
/// drag over chrome alone yields `lo >= hi` (design spec §2, "dragging across only
/// chrome selects nothing"; see `Bias`'s doc comment). Sorting the pair back into
/// ascending order — tempting, since `start`/`end` are already in document order —
/// would silently turn that empty signal into a hull spanning from the fallback's `0`
/// to its `len()`, i.e. the whole document, which is the one answer decision 1
/// explicitly rules out.
pub(crate) fn source_hull(
    canvas: &Canvas,
    source: &str,
    selection: Selection,
) -> Option<(usize, usize)> {
    let (start, end) = selection.ordered();
    let far = Pos {
        row: end.row,
        col: end.col.saturating_add(1),
    };
    let lo = offset_at(canvas, source, start, Bias::Start)?;
    let hi = offset_at(canvas, source, far, Bias::End)?;
    (lo < hi).then_some((lo, hi))
}

/// Which way an endpoint resolves when it lands on a cell no span covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bias {
    /// The near end of the range: take the start of the next text.
    Start,
    /// The far end: take the end of the previous text.
    End,
}

/// The source byte a cell points at.
///
/// A cell inside a span is exact. A cell on chrome — a border, the gutter, padding,
/// the blank tail of a row — has no span to ask, so it resolves to the nearest text in
/// document order in the direction `bias` names. This is the only coordinate in the
/// selection that is interpreted rather than looked up (design spec §2.1).
pub(crate) fn offset_at(canvas: &Canvas, source: &str, pos: Pos, bias: Bias) -> Option<usize> {
    // Exact hit first: the cell is inside some span's drawn columns.
    for span in canvas.spans() {
        let end = span.col.saturating_add(span.cols);
        if span.row == pos.row && pos.col >= span.col && pos.col < end {
            let body = source
                .get(span.source_start..span.source_end)
                .unwrap_or_default();
            return Some(span.source_start + byte_at_column(body, pos.col - span.col));
        }
    }
    // Chrome. Search in READING ORDER — (row, col) across the whole canvas, not just
    // this row — because "document order" is the whole point and a drag inside a
    // diagram's blank interior has no span on its own rows at all.
    let key = (pos.row, pos.col);
    match bias {
        // The near end takes the first text at or after the cell.
        Bias::Start => canvas
            .spans()
            .iter()
            .filter(|s| (s.row, s.col) >= key)
            .min_by_key(|s| (s.row, s.col))
            .map(|s| s.source_start)
            .or(Some(source.len())),
        // The far end takes the last text at or before it.
        Bias::End => canvas
            .spans()
            .iter()
            .filter(|s| (s.row, s.col) <= key)
            .max_by_key(|s| (s.row, s.col))
            .map(|s| s.source_end)
            .or(Some(0)),
    }
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

/// The display column that `byte` bytes into `text` land on.
///
/// `byte_at_column`'s inverse, grapheme-wise for the same reason: only whole clusters
/// consumed *before* `byte` count, so a byte offset that lands mid-cluster (which
/// should not happen for a boundary this module produces, but a defensive read is
/// cheaper than a panic) still yields a sane column rather than an inflated one.
fn column_at_byte(text: &str, byte: usize) -> u16 {
    let mut used = 0u16;
    let mut offset = 0usize;
    for cluster in graphemes(text) {
        if offset >= byte {
            break;
        }
        offset += cluster.len();
        used = used.saturating_add(u16::try_from(display_width(cluster)).unwrap_or(u16::MAX));
    }
    used
}

/// The column ranges of `row` that a selection washes.
///
/// Every span the hull covers, clipped to the covered part. Chrome carries no spans, so
/// borders, the line-number gutter, cell padding and the blank tail of a row are not in
/// the answer and no rule had to say so (design spec §2). Consumes [`source_hull`]'s
/// endpoints directly rather than re-resolving them from `canvas` and `selection`: a
/// second `offset_at` walk would have to re-derive the far endpoint's inclusive-column
/// convention, and a highlight that disagreed with the hull by one boundary would be far
/// more visible than the same slip on a clipboard payload.
pub(crate) fn highlighted_columns(
    canvas: &Canvas,
    source: &str,
    selection: Selection,
    row: usize,
) -> Vec<Range<u16>> {
    let Some((lo, hi)) = source_hull(canvas, source, selection) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for span in canvas.spans() {
        if span.row != row || span.source_end <= lo || span.source_start >= hi {
            continue;
        }
        let body = source
            .get(span.source_start..span.source_end)
            .unwrap_or_default();
        let from = column_at_byte(body, lo.saturating_sub(span.source_start));
        let to = if hi >= span.source_end {
            span.cols
        } else {
            column_at_byte(body, hi - span.source_start)
        };
        let (a, b) = (span.col + from, span.col + to);
        if a < b {
            out.push(a..b);
        }
    }
    out.sort_by_key(|r| r.start);
    out
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

#[cfg(test)]
mod tests {
    use super::column_at_byte;

    #[test]
    fn column_at_byte_counts_display_width_not_bytes() {
        assert_eq!(column_at_byte("abc", 0), 0);
        assert_eq!(column_at_byte("abc", 2), 2);
        assert_eq!(column_at_byte("abc", 3), 3);
    }

    #[test]
    fn column_at_byte_handles_a_multi_byte_grapheme() {
        // 'é' is two bytes and one display column.
        let text = "café";
        assert_eq!(text.len(), 5, "the fixture must actually be multi-byte");
        assert_eq!(column_at_byte(text, 0), 0);
        assert_eq!(column_at_byte(text, 3), 3, "just before the 'é'");
        assert_eq!(
            column_at_byte(text, 5),
            4,
            "past the 'é', which is two bytes but one column"
        );
    }

    #[test]
    fn column_at_byte_handles_a_wide_grapheme() {
        // U+3000 IDEOGRAPHIC SPACE is three bytes and two display columns.
        let text = "a\u{3000}b";
        assert_eq!(text.len(), 5, "the fixture must actually be wide");
        assert_eq!(column_at_byte(text, 0), 0);
        assert_eq!(column_at_byte(text, 1), 1, "just before the wide space");
        assert_eq!(
            column_at_byte(text, 4),
            3,
            "past the wide space, which is three bytes but two columns"
        );
        assert_eq!(column_at_byte(text, 5), 4, "past the trailing 'b' too");
    }
}
