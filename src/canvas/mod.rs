//! The `Canvas`: the single currency between renderers and the viewport.
//!
//! Every renderer — block, table, code, Mermaid, image placeholder — returns a
//! [`Canvas`]. The viewport does nothing but blit a vertical slice of the document
//! canvas onto the terminal.
//!
//! Beside the cells a canvas carries three metadata channels, each answering something
//! about a row that only the renderer which drew it can know: [`Anchor`]s for the table of
//! contents, [`SearchSpan`]s mapping source bytes to cells, and [`Pin`]s marking the
//! leading columns that are a block's own chrome. They are how `render` tells `tui` things
//! without `render` depending on `tui`, and adding one is cheaper than teaching the pager
//! to infer the same fact from the drawn cells — which it did for the pin, wrongly.
//!
//! # The contract
//!
//! A canvas is a rectangle of [`Cell`]s that is **exactly [`Canvas::width`] display
//! columns wide on every row**. This is not a convention, it is an invariant: every
//! mutating operation in this module preserves it, and the tests assert it after each
//! one. Double-width clusters occupy a lead cell with `width == 2` followed by a
//! continuation cell with `width == 0`; zero-width clusters are merged into the cell
//! they modify.
//!
//! ```
//! use mdmost::canvas::Canvas;
//! use mdmost::theme::Theme;
//!
//! let theme = Theme::default_dark();
//! let mut canvas = Canvas::new(10, 1, theme.base());
//! canvas.write_str(0, 0, "日本", theme.text.body);
//! assert_eq!(canvas.row_text(0), "日本      ");
//! assert!(canvas.check_invariants().is_ok());
//! ```

mod border;
mod cell;
pub mod meter;
mod ops;

#[cfg(test)]
mod tests;

pub use border::{BorderSet, Rule, Side};
pub use cell::Cell;
pub use ops::CutMark;

use crate::text::{Align, Line, cell_clusters, display_width, grapheme_width, pad_to_width};
use crate::theme::Style;

/// A named position in a canvas, used by the table of contents to jump to a heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    /// The heading's stable id, as assigned by [`Doc`](crate::doc::Doc).
    pub id: String,
    /// The heading level, `1..=6`.
    pub level: u8,
    /// The row the heading starts on.
    pub row: usize,
}

/// The mapping from a source byte range to the cells it was rendered into.
///
/// Search works on the document source and then translates its hits into canvas
/// positions through these spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchSpan {
    /// Start byte offset in the document source.
    pub source_start: usize,
    /// End byte offset (exclusive) in the document source.
    pub source_end: usize,
    /// The atomic unit this span is one piece of, as `(start, end)` source offsets, or
    /// `None` when the span stands for itself.
    ///
    /// A diagram label is the only thing that has one, and it exists because a label is
    /// no longer one span: it wraps onto several rows, and a decoded entity inside it is
    /// cut out into a run of its own, so that every span's source stays a copy of the
    /// cells it names (`mermaid::ast::Label::spans_for`). `select::resolve` still has to
    /// ask "did this drag stay inside one label?" (design spec §2.2), and it cannot ask
    /// that of the pieces — so each piece names the whole it belongs to.
    ///
    /// It is not a second source range: nothing is ever copied or washed from it. It is
    /// a boundary the selection compares its hull against, and the answer stays the hull.
    pub unit: Option<(usize, usize)>,
    /// The row the text was rendered on.
    pub row: usize,
    /// The first column the text occupies.
    pub col: u16,
    /// How many display columns the text occupies.
    pub cols: u16,
}

/// The leading columns of one row that are chrome rather than content.
///
/// The third metadata channel, beside [`Anchor`] and [`SearchSpan`], and it exists for
/// the same reason they do: the pager needs to know something about a row that only the
/// renderer that drew it can know. Here it is where a block's own chrome stops — a code
/// fence's line-number gutter — so that the horizontal scroll can hold those columns
/// still while the code slides underneath them.
///
/// **A pin is a claim about the whole row**: columns `0..cols` of row `row` belong to a
/// block that owns the row from its first column. That is why it travels through
/// [`Canvas::append`] and [`Canvas::indent`], which stack and inset whole rows, and not
/// through [`Canvas::blit`], which drops any pin the source carried — a canvas placed at
/// an arbitrary column of a row it shares with other content (a table cell) cannot make a
/// claim about that row's first columns.
///
/// This channel replaced a detector that inferred the same seam by matching cell styles
/// against `theme.code.line_number`. That was unsound twice over: the style is not unique
/// (`code.operator` is the same value in both shipped themes, so an *unnumbered* fence
/// tripped it), and the inferred prefix was spread over a contiguous run of non-blank
/// rows, which in a list item is the fence *and the table under it*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    /// The row whose leading columns are chrome.
    pub row: usize,
    /// How many leading columns, counted from column zero of the row.
    pub cols: u16,
}

/// A region of a row that is a control, and what clicking it copies.
///
/// The fourth metadata channel, and it exists for the reason the other three do: the
/// pager needs to know something about a region that only the renderer which drew it can
/// know. Here that these cells are a button, and what it puts on the clipboard.
///
/// The payload is text, not a source byte range, because the two are not the same
/// answer: the source of a fence inside a block quote carries `> ` on every interior
/// line, and copying that is not what the button promises.
///
/// Like [`Pin`], a hotspot is a claim about a region of one row, so it travels through
/// [`Canvas::append`] and [`Canvas::indent`] and is dropped by [`Canvas::blit`] — a
/// canvas placed at an arbitrary column of a row it shares with other content cannot
/// claim that a control lives there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
    /// The row the control is drawn on.
    pub row: usize,
    /// The first column it occupies.
    pub col: u16,
    /// How many display columns it occupies.
    pub cols: u16,
    /// The plain-text payload. Always present: the only thing OSC 52 can carry.
    pub text: String,
    /// A richer flavour offered to a local clipboard only. `None` for a code block.
    pub html: Option<String>,
}

/// A rectangle of cells that is indivisible in a selection, and the source it copies.
///
/// The fifth metadata channel. A diagram records one, and it is the only way the pager
/// can know that a rectangle of cells is *one thing*: a diagram's labels carry
/// [`SearchSpan`]s and the box art between them carries nothing, so a drag from one
/// label to the next would otherwise light two words and put the punctuation between
/// them on the clipboard, truncated wherever the last label ended (design spec §2.2).
///
/// `source_start..source_end` is the **whole** construct — for a Mermaid fence, the
/// fence lines included — not the union of the labels inside it. That is what makes the
/// wider drag copy something that still parses as a diagram.
///
/// Like [`Pin`] and [`Hotspot`], an atom is a claim about rows a block owns outright, so
/// it travels through [`Canvas::append`] and [`Canvas::indent`] and is dropped by
/// [`Canvas::blit`]: a canvas placed at an arbitrary column of a row it shares with other
/// content cannot claim a rectangle of that row. That is an invariant, not a live case —
/// the only sub-canvas the renderer blits is a table cell, and a GFM table cell holds
/// inline content, so no document puts a diagram inside one. Nothing to go looking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
    /// The first row of the rectangle.
    pub row: usize,
    /// How many rows it covers.
    pub rows: usize,
    /// The first column it occupies.
    pub col: u16,
    /// How many display columns it occupies.
    pub cols: u16,
    /// Start byte offset in the document source.
    pub source_start: usize,
    /// End byte offset (exclusive) in the document source.
    pub source_end: usize,
    /// The block's content with its container prefix already off — comrak's `literal`.
    ///
    /// Carried rather than re-derived from `source_start..source_end`, because *what the
    /// prefix was* is not a question the source range can answer. A quoted block's lines
    /// need not carry the same bytes: `>` and `> ` are the same marker, a blank quoted
    /// line is a bare `>` — which is what `CommonMark` itself produces for one — and a
    /// document may mix them freely inside one quote. Sampling one line's prefix and
    /// requiring every other line to start with that exact string renders correctly and
    /// *copies wrong*, putting a stray `>` inside the Mermaid: the see/get divergence an
    /// [`Atom`] exists to remove. comrak already stripped the prefix when it parsed the
    /// block, so its answer is the authority; `select::extract` checks that answer back
    /// against the document line by line rather than trusting it blind, and leaves a line
    /// it cannot locate alone (design spec §2.2).
    ///
    /// Content only: the fence lines are not in it. They are read from the document
    /// instead — the opener because `source_start` already points past the prefix at it,
    /// the closer because the fence character the opener begins with locates it on its
    /// own line.
    pub content: String,
}

impl Atom {
    /// Whether the rectangle covers `row`.
    pub fn covers_row(&self, row: usize) -> bool {
        row >= self.row && row < self.row + self.rows
    }

    /// The columns the rectangle occupies, as a half-open interval.
    pub fn columns(&self) -> std::ops::Range<u16> {
        self.col..self.col.saturating_add(self.cols)
    }

    /// Whether `span`'s bytes lie inside the construct this atom copies.
    ///
    /// How the pager tells a diagram's own labels from every other span on the canvas.
    /// Containment in the source, not overlap on screen: the drawn rectangle is padded
    /// out to the block width and a row of it belongs to nothing else, but the source
    /// range is exactly the fence and its contents.
    pub fn contains_span(&self, span: &SearchSpan) -> bool {
        span.source_start >= self.source_start && span.source_end <= self.source_end
    }
}

/// A rectangle of styled cells, exactly [`Canvas::width`] columns wide on every row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Canvas {
    width: u16,
    rows: Vec<Vec<Cell>>,
    anchors: Vec<Anchor>,
    spans: Vec<SearchSpan>,
    pins: Vec<Pin>,
    hotspots: Vec<Hotspot>,
    atoms: Vec<Atom>,
}

impl Canvas {
    /// Creates a canvas of `width` columns and `height` rows, filled with blanks.
    pub fn new(width: u16, height: usize, style: Style) -> Self {
        let mut canvas = Self::empty(width);
        canvas.rows = (0..height).map(|_| blank_row(width, style)).collect();
        canvas
    }

    /// Creates a canvas of `width` columns and no rows.
    pub fn empty(width: u16) -> Self {
        Self {
            width,
            rows: Vec::new(),
            anchors: Vec::new(),
            spans: Vec::new(),
            pins: Vec::new(),
            hotspots: Vec::new(),
            atoms: Vec::new(),
        }
    }

    /// Renders wrapped [`Line`]s into a canvas of `width` columns.
    ///
    /// Lines are neither wrapped nor re-flowed here; pass them through
    /// [`wrap_spans`](crate::text::wrap_spans) first. Content wider than `width` is
    /// clipped, narrower content is padded with blanks in `base`.
    pub fn from_lines(width: u16, lines: &[Line], base: Style) -> Self {
        let mut canvas = Self::new(width, lines.len(), base);
        for (row, line) in lines.iter().enumerate() {
            canvas.write_line(row, 0, line, base);
        }
        canvas
    }

    /// Renders a single text line into a canvas of `width` columns.
    pub fn from_text(width: u16, text: &str, style: Style) -> Self {
        let mut canvas = Self::new(width, 1, style);
        canvas.write_str(0, 0, text, style);
        canvas
    }

    /// The canvas width in display columns.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// The number of rows.
    pub fn height(&self) -> usize {
        self.rows.len()
    }

    /// Whether the canvas has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// All rows.
    pub fn rows(&self) -> &[Vec<Cell>] {
        &self.rows
    }

    /// The cells of one row, or `None` if the row is out of range.
    pub fn row(&self, row: usize) -> Option<&[Cell]> {
        self.rows.get(row).map(Vec::as_slice)
    }

    /// The plain text of one row, including the padding blanks.
    ///
    /// Returns an empty string for an out-of-range row. Intended for tests and for
    /// the `--render-once` plain dump.
    pub fn row_text(&self, row: usize) -> String {
        self.rows
            .get(row)
            .map(|cells| cells.iter().map(Cell::text).collect())
            .unwrap_or_default()
    }

    /// The plain text of the whole canvas, rows joined with `\n`.
    pub fn plain_text(&self) -> String {
        (0..self.height())
            .map(|row| self.row_text(row))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The heading anchors recorded in this canvas.
    pub fn anchors(&self) -> &[Anchor] {
        &self.anchors
    }

    /// The source-to-canvas mappings recorded in this canvas.
    pub fn spans(&self) -> &[SearchSpan] {
        &self.spans
    }

    /// Records a heading anchor.
    pub fn add_anchor(&mut self, anchor: Anchor) {
        self.anchors.push(anchor);
    }

    /// Records a source-to-canvas mapping.
    pub fn add_span(&mut self, span: SearchSpan) {
        self.spans.push(span);
    }

    /// Replaces every span with what `map` returns for it, dropping the ones it maps to
    /// `None`.
    ///
    /// The one channel a canvas can carry that is not a claim about its own cells: a
    /// span's byte offsets belong to the document, and a canvas assembled from a
    /// fragment of it — a Mermaid block's own text — has to have them rebased before it
    /// joins the document (`render::code::diagram_block`). Dropping is part of the
    /// contract, not a convenience: a span whose offsets cannot be rebased must leave,
    /// because a *wrong* offset copies bytes from elsewhere in the document while a
    /// missing one merely falls back to the drawn cells.
    pub fn map_spans(&mut self, map: impl FnMut(&SearchSpan) -> Option<SearchSpan>) {
        let spans = std::mem::take(&mut self.spans);
        self.spans = spans.iter().filter_map(map).collect();
    }

    /// The pinned chrome prefixes recorded in this canvas.
    pub fn pins(&self) -> &[Pin] {
        &self.pins
    }

    /// Records that columns `0..cols` of `row` are the row's own chrome.
    ///
    /// A `cols` of zero is not recorded: no pin and a pin of nothing are the same claim,
    /// and only one of them costs an entry.
    pub fn add_pin(&mut self, row: usize, cols: u16) {
        if cols > 0 {
            self.pins.push(Pin { row, cols });
        }
    }

    /// The controls recorded in this canvas.
    pub fn hotspots(&self) -> &[Hotspot] {
        &self.hotspots
    }

    /// Records a control.
    pub fn add_hotspot(&mut self, hotspot: Hotspot) {
        self.hotspots.push(hotspot);
    }

    /// The indivisible regions recorded in this canvas.
    pub fn atoms(&self) -> &[Atom] {
        &self.atoms
    }

    /// Records that a rectangle of cells is one thing, copied as `source_start..end`.
    ///
    /// A rectangle with no rows or no columns is not recorded: nothing can be dragged
    /// over it, so an entry for it could only ever be a way to get the wrong answer.
    pub fn add_atom(&mut self, atom: Atom) {
        if atom.rows > 0 && atom.cols > 0 && atom.source_end > atom.source_start {
            self.atoms.push(atom);
        }
    }

    /// How many leading columns of each row are chrome; one entry per row.
    ///
    /// The [`Pin`] channel, expanded to the shape the viewport wants. Rows with no pin
    /// get zero, and a row with more than one pin — which nothing records today, but
    /// which the channel does not forbid — gets the widest, because a prefix has to cover
    /// every claim made about the row for the chrome to stay whole.
    pub fn pinned_prefix(&self) -> Vec<u16> {
        let mut pinned = vec![0u16; self.rows.len()];
        for pin in &self.pins {
            if let Some(slot) = pinned.get_mut(pin.row) {
                *slot = (*slot).max(pin.cols);
            }
        }
        pinned
    }

    /// Appends a blank row and returns its index.
    pub fn push_blank_row(&mut self, style: Style) -> usize {
        self.rows.push(blank_row(self.width, style));
        self.rows.len() - 1
    }

    /// Appends `count` blank rows.
    pub fn push_blank_rows(&mut self, count: usize, style: Style) {
        for _ in 0..count {
            self.push_blank_row(style);
        }
    }

    /// Appends a row holding `line`, aligned within the canvas width.
    ///
    /// Returns the index of the new row.
    pub fn push_line(&mut self, line: &Line, align: Align, base: Style) -> usize {
        let row = self.push_blank_row(base);
        let col = align_offset(usize::from(self.width), line.width(), align);
        self.write_line(row, col, line, base);
        row
    }

    /// Appends a row holding `text`, aligned within the canvas width.
    ///
    /// Returns the index of the new row.
    pub fn push_text(&mut self, text: &str, align: Align, style: Style) -> usize {
        let row = self.push_blank_row(style);
        let col = align_offset(usize::from(self.width), display_width(text), align);
        self.write_str(row, col, text, style);
        row
    }

    /// Grows the canvas to at least `height` rows by appending blank rows.
    pub fn pad_to_height(&mut self, height: usize, style: Style) {
        while self.rows.len() < height {
            self.push_blank_row(style);
        }
    }

    /// Writes `text` at `(row, col)` and returns how many columns were written.
    ///
    /// * Out-of-range rows and columns are ignored (nothing is written, `0` returned).
    /// * Writing stops at the right edge; a double-width cluster that would straddle
    ///   the edge is dropped rather than split.
    /// * Zero-width clusters (combining marks, joiners) are appended to the previous
    ///   cell's text instead of consuming a column. If there is no previous cell in
    ///   this write and the cell to the left is blank, they are dropped.
    /// * Overwriting either half of an existing double-width cell replaces the orphaned
    ///   half with a blank, so the row stays exactly [`Canvas::width`] columns wide.
    /// * A cluster that no cell can hold and that cannot be divided without changing its
    ///   width — U+17D8, a wide sign carrying a spacing mark — is drawn as
    ///   [`text::UNPLACEABLE`](crate::text::UNPLACEABLE) padded to the cluster's own
    ///   width, so the columns after it are unmoved. At the right edge such a run is cut
    ///   like any other text rather than dropped whole, which leaves the marker visible
    ///   in the last column; the row still measures exactly [`Canvas::width`].
    pub fn write_str(&mut self, row: usize, col: usize, text: &str, style: Style) -> usize {
        let width = usize::from(self.width);
        let Some(cells) = self.rows.get_mut(row) else {
            return 0;
        };
        if col >= width {
            return 0;
        }
        let mut cursor = col;
        let mut last_written: Option<usize> = None;
        // `cell_clusters`, not `graphemes`: a cluster wider than two columns (a wide
        // base plus a spacing mark) has to occupy more than one cell, or the cells
        // would claim a width their own text does not have. Where such a cluster cannot
        // be divided at all, what arrives here is the marker run that stands in for it.
        for cluster in cell_clusters(text) {
            let cluster_width = usize::from(grapheme_width(cluster));
            if cluster_width == 0 {
                if let Some(index) = last_written {
                    cells[index].append_zero_width(cluster);
                }
                continue;
            }
            if cursor + cluster_width > width {
                break;
            }
            overwrite(cells, cursor, Cell::new(cluster, style));
            last_written = Some(cursor);
            cursor += cluster_width;
        }
        cursor - col
    }

    /// Writes a styled [`Line`] at `(row, col)` and returns how many columns were
    /// written.
    ///
    /// Each span's style is overlaid on `base`, so `base` supplies the background and
    /// the span supplies the accents. See [`Style::patch`].
    pub fn write_line(&mut self, row: usize, col: usize, line: &Line, base: Style) -> usize {
        let mut cursor = col;
        for span in &line.spans {
            cursor += self.write_str(row, cursor, &span.text, base.patch(span.style));
        }
        cursor - col
    }

    /// Writes `text` into `col..col + field_width`, padded and aligned within it.
    ///
    /// The field is always fully painted in `style`, so this is the right call for a
    /// table cell or a status-bar segment.
    pub fn write_field(
        &mut self,
        row: usize,
        col: usize,
        field_width: usize,
        text: &str,
        align: Align,
        style: Style,
    ) {
        let padded = pad_to_width(text, field_width, align);
        self.write_str(row, col, &padded, style);
    }

    /// Fills `col..col + len` on `row` with `cluster`.
    pub fn fill(&mut self, row: usize, col: usize, len: usize, cluster: &str, style: Style) {
        let filler = crate::text::repeat_to_width(cluster, len);
        self.write_str(row, col, &filler, style);
    }

    /// Draws a horizontal run of `cluster` starting at `(row, col)`.
    ///
    /// A convenience alias of [`Canvas::fill`] that reads better in border code.
    pub fn hline(&mut self, row: usize, col: usize, len: usize, cluster: &str, style: Style) {
        self.fill(row, col, len, cluster, style);
    }

    /// Draws a vertical run of `cluster` downwards from `(row, col)`.
    pub fn vline(&mut self, row: usize, col: usize, len: usize, cluster: &str, style: Style) {
        for offset in 0..len {
            self.write_str(row + offset, col, cluster, style);
        }
    }

    /// Sets the style of every cell in `col..col + len` on `row`.
    pub fn set_style(&mut self, row: usize, col: usize, len: usize, style: Style) {
        self.map_style(row, col, len, |cell| cell.set_style(style));
    }

    /// Overlays `style` on every cell in `col..col + len` on `row`.
    ///
    /// This is how a search match or the current-line highlight is applied without
    /// destroying the syntax colours underneath. See [`Style::patch`].
    pub fn patch_style(&mut self, row: usize, col: usize, len: usize, style: Style) {
        self.map_style(row, col, len, |cell| cell.patch_style(style));
    }

    /// Overlays `style` on every cell of the canvas.
    pub fn patch_style_all(&mut self, style: Style) {
        for cells in &mut self.rows {
            for cell in cells.iter_mut() {
                cell.patch_style(style);
            }
        }
    }

    fn map_style(&mut self, row: usize, col: usize, len: usize, mut f: impl FnMut(&mut Cell)) {
        let Some(cells) = self.rows.get_mut(row) else {
            return;
        };
        let end = (col + len).min(cells.len());
        let start = col.min(end);
        for cell in &mut cells[start..end] {
            f(cell);
        }
    }

    /// Verifies the canvas contract.
    ///
    /// Returns `Err` with a human-readable description of the first violation found.
    /// Renderers should call this in their tests; the cost makes it unsuitable for
    /// production paths.
    pub fn check_invariants(&self) -> Result<(), String> {
        for (index, cells) in self.rows.iter().enumerate() {
            let mut columns = 0usize;
            let mut expect_continuation = false;
            for cell in cells {
                if expect_continuation && !cell.is_continuation() {
                    return Err(format!(
                        "row {index}: double-width cell is not followed by a continuation cell"
                    ));
                }
                expect_continuation = cell.width() == 2;
                if cell.width() == 0 && !cell.is_continuation() {
                    return Err(format!("row {index}: stray zero-width cell"));
                }
                // The cell must not lie about how much room its own text needs. This
                // is the assertion that makes design spec §4 enforceable rather than
                // aspirational: without it a clamped over-wide cluster reports two
                // columns while drawing three, and every row containing one is a
                // column too wide.
                // A control character measures one column and draws something else
                // entirely — a TAB jumps to the next tab stop, an ESC opens a sequence
                // that can repaint the screen — so every check in this function agrees
                // that the row is exact while the terminal draws it wider. The width
                // guarantee is a guarantee about the *terminal*, so a control character
                // in a cell is a violation of it even though the arithmetic adds up.
                // `text::cell_clusters` substitutes a printable column for each one.
                if let Some(ch) = cell.text().chars().find(|ch| ch.is_control()) {
                    return Err(format!(
                        "row {index}: cell {:?} carries control character U+{:04X}, \
                         which the terminal will not draw as one column",
                        cell.text(),
                        u32::from(ch)
                    ));
                }
                let drawn = display_width(cell.text());
                if drawn != usize::from(cell.width()) {
                    return Err(format!(
                        "row {index}: cell {:?} draws {drawn} columns but claims {}",
                        cell.text(),
                        cell.width()
                    ));
                }
                columns += usize::from(cell.width());
            }
            if expect_continuation {
                return Err(format!("row {index}: double-width cell at the row edge"));
            }
            if columns != usize::from(self.width) {
                return Err(format!(
                    "row {index}: {columns} columns, expected {}",
                    self.width
                ));
            }
            // Finally the row is measured *assembled*. Every check above is per cell,
            // and per-cell honesty does not add up to an honest row: two adjacent cells
            // can each measure exactly what they claim and still re-join into a single
            // grapheme cluster when concatenated, drawing narrower together than apart.
            // A ZWJ emoji sequence split across two cells did precisely that, and passed
            // every assertion above while rendering the row two columns short.
            let drawn = display_width(&self.row_text(index));
            if drawn != usize::from(self.width) {
                return Err(format!(
                    "row {index}: cells claim {} columns but the assembled row draws \
                     {drawn} — adjacent cells are re-joining",
                    self.width
                ));
            }
        }
        Ok(())
    }
}

/// A row of `width` blank cells.
fn blank_row(width: u16, style: Style) -> Vec<Cell> {
    (0..width).map(|_| Cell::blank(style)).collect()
}

/// The column at which `content_width` starts when aligned within `field_width`.
///
/// This is the whole of "where do I put this thing" arithmetic, and it belongs in one
/// place: the table renderer, the Mermaid chart chrome, the sequence-diagram label
/// placer and the graph layout engine all need it, and each had grown its own copy
/// because this function used to be private. Reach for it rather than writing
/// `slack / 2` again.
///
/// Content wider than the field yields `0` rather than underflowing — several of the
/// hand-rolled copies used a bare `field - content`, which panics at small widths.
pub(crate) fn align_offset(field_width: usize, content_width: usize, align: Align) -> usize {
    let slack = field_width.saturating_sub(content_width);
    match align {
        Align::Left => 0,
        Align::Center => slack / 2,
        Align::Right => slack,
    }
}

/// Places `cell` at `col`, repairing any double-width cell it cuts in half.
///
/// The caller guarantees `col + cell.width() <= cells.len()` and `cell.width() >= 1`.
fn overwrite(cells: &mut [Cell], col: usize, cell: Cell) {
    let style = cell.style();
    // We are covering the right half of a wide cell: blank out its orphaned left half.
    if cells[col].is_continuation() && col > 0 {
        let orphan_style = cells[col - 1].style();
        cells[col - 1] = Cell::blank(orphan_style);
    }
    let end = col + usize::from(cell.width());
    // We are covering the left half of a wide cell: blank out its orphaned right half.
    if end < cells.len() && cells[end].is_continuation() {
        cells[end] = Cell::blank(style);
    }
    cells[col] = cell;
    for slot in &mut cells[col + 1..end] {
        *slot = Cell::continuation(style);
    }
}
