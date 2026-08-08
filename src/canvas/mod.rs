//! The `Canvas`: the single currency between renderers and the viewport.
//!
//! Every renderer — block, table, code, Mermaid, image placeholder — returns a
//! [`Canvas`]. The viewport does nothing but blit a vertical slice of the document
//! canvas onto the terminal.
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
//! use mdless::canvas::Canvas;
//! use mdless::theme::Theme;
//!
//! let theme = Theme::default_dark();
//! let mut canvas = Canvas::new(10, 1, theme.base());
//! canvas.write_str(0, 0, "日本", theme.text.body);
//! assert_eq!(canvas.row_text(0), "日本      ");
//! assert!(canvas.check_invariants().is_ok());
//! ```

mod border;
mod cell;
mod ops;

#[cfg(test)]
mod tests;

pub use border::BorderSet;
pub use cell::Cell;

use crate::text::{Align, Line, display_width, grapheme_width, graphemes, pad_to_width};
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
    /// The row the text was rendered on.
    pub row: usize,
    /// The first column the text occupies.
    pub col: u16,
    /// How many display columns the text occupies.
    pub cols: u16,
}

/// A rectangle of styled cells, exactly [`Canvas::width`] columns wide on every row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Canvas {
    width: u16,
    rows: Vec<Vec<Cell>>,
    anchors: Vec<Anchor>,
    spans: Vec<SearchSpan>,
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
        for cluster in graphemes(text) {
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
        }
        Ok(())
    }
}

/// A row of `width` blank cells.
fn blank_row(width: u16, style: Style) -> Vec<Cell> {
    (0..width).map(|_| Cell::blank(style)).collect()
}

/// The column at which `content_width` starts when aligned in `field_width`.
fn align_offset(field_width: usize, content_width: usize, align: Align) -> usize {
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
