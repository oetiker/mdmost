//! Composition operations on canvases.
//!
//! These are the operations block, table and diagram renderers assemble their output
//! from. Every one of them preserves the canvas contract described in
//! [`crate::canvas`].

use super::{Anchor, BorderSet, Canvas, Cell, SearchSpan};
use crate::error::CanvasError;
use crate::text::{Align, Line, display_width};
use crate::theme::Style;

impl Canvas {
    /// Grows the canvas to `width` columns by padding every row on the right.
    ///
    /// # Errors
    ///
    /// Returns [`CanvasError::Narrowing`] if `width` is smaller than the current
    /// width; use [`Canvas::truncate_width`] when narrowing is intended.
    pub fn pad_to_width(&mut self, width: u16, style: Style) -> Result<(), CanvasError> {
        if width < self.width {
            return Err(CanvasError::Narrowing {
                current: self.width,
                requested: width,
            });
        }
        let extra = usize::from(width - self.width);
        for cells in &mut self.rows {
            cells.extend((0..extra).map(|_| Cell::blank(style)));
        }
        self.width = width;
        Ok(())
    }

    /// Shrinks the canvas to `width` columns, clipping on the right.
    ///
    /// A double-width cell straddling the new edge is replaced by a blank, so the
    /// result is still exactly `width` columns wide. Widening is a no-op.
    pub fn truncate_width(&mut self, width: u16, style: Style) {
        if width >= self.width {
            return;
        }
        let keep = usize::from(width);
        for cells in &mut self.rows {
            cells.truncate(keep);
            if cells.last().is_some_and(|c| c.width() == 2) {
                let last = cells.len() - 1;
                cells[last] = Cell::blank(style);
            }
        }
        self.width = width;
    }

    /// Sets the canvas width, padding or clipping as needed.
    pub fn resize_width(&mut self, width: u16, style: Style) {
        if width >= self.width {
            // Padding cannot fail once we know we are not narrowing.
            let _ = self.pad_to_width(width, style);
        } else {
            self.truncate_width(width, style);
        }
    }

    /// Copies `src` onto `self` with its top-left corner at `(top, left)`.
    ///
    /// * `self` grows downwards with blank rows if `src` does not fit vertically.
    /// * Content that would fall off the right edge is clipped.
    /// * `src`'s anchors and spans are translated by the offset and merged into
    ///   `self`'s.
    /// * Search spans are translated verbatim, including spans whose cells were
    ///   clipped at the right edge; consumers must clamp a span to the canvas width
    ///   before highlighting it.
    /// * Cells of `src` that are blank *and* carry no style still overwrite the
    ///   destination; use [`Canvas::blit_opaque`] semantics deliberately — a canvas is
    ///   a rectangle, not a sprite with transparency.
    pub fn blit(&mut self, top: usize, left: usize, src: &Canvas, fill: Style) {
        self.pad_to_height(top + src.height(), fill);
        let width = usize::from(self.width);
        for (offset, cells) in src.rows.iter().enumerate() {
            let row = top + offset;
            let mut col = left;
            for cell in cells {
                if col >= width {
                    break;
                }
                match cell.width() {
                    0 => {}
                    w if col + usize::from(w) <= width => {
                        // Re-drawing through `write_str` keeps every repair rule in one
                        // place, including the double-width straddle handling.
                        self.write_str(row, col, cell.text(), cell.style());
                        col += usize::from(w);
                    }
                    _ => {
                        // A double-width cell straddling the right edge: blank it.
                        self.write_str(row, col, " ", cell.style());
                        col += 1;
                    }
                }
            }
        }
        self.merge_metadata(src, top, left);
    }

    /// Translates and merges `src`'s anchors and spans into `self`.
    fn merge_metadata(&mut self, src: &Canvas, top: usize, left: usize) {
        self.anchors.extend(src.anchors.iter().map(|a| Anchor {
            id: a.id.clone(),
            level: a.level,
            row: a.row + top,
        }));
        let left16 = u16::try_from(left).unwrap_or(u16::MAX);
        self.spans.extend(src.spans.iter().map(|s| SearchSpan {
            row: s.row + top,
            col: s.col.saturating_add(left16),
            ..*s
        }));
    }

    /// Appends `other` below `self`.
    ///
    /// The result is as wide as the wider of the two; the narrower one is padded on
    /// the right with blanks in `fill`.
    pub fn append(&mut self, other: &Canvas, fill: Style) {
        let width = self.width.max(other.width);
        let _ = self.pad_to_width(width, fill);
        let top = self.height();
        self.rows
            .extend(other.rows.iter().map(|cells| pad_cells(cells, width, fill)));
        self.merge_metadata(other, top, 0);
    }

    /// Stacks canvases vertically, in order.
    ///
    /// The result is as wide as the widest input, and as wide as `min_width` at
    /// minimum.
    pub fn vconcat(parts: &[Canvas], min_width: u16, fill: Style) -> Canvas {
        let width = parts
            .iter()
            .map(Canvas::width)
            .max()
            .unwrap_or(0)
            .max(min_width);
        let mut out = Canvas::empty(width);
        for part in parts {
            out.append(part, fill);
        }
        out
    }

    /// Places canvases side by side, separated by `gap` blank columns.
    ///
    /// Shorter canvases are top-aligned and padded with blank rows, matching the
    /// table renderer's row rule (design spec §7.6).
    pub fn hconcat(parts: &[Canvas], gap: u16, fill: Style) -> Canvas {
        if parts.is_empty() {
            return Canvas::empty(0);
        }
        let height = parts.iter().map(Canvas::height).max().unwrap_or(0);
        let total: u16 = parts.iter().map(Canvas::width).sum::<u16>()
            + gap * u16::try_from(parts.len().saturating_sub(1)).unwrap_or(0);
        let mut out = Canvas::new(total, height, fill);
        let mut col = 0usize;
        for part in parts {
            out.blit(0, col, part, fill);
            col += usize::from(part.width) + usize::from(gap);
        }
        out
    }

    /// Returns a copy of `self` inset by `left` and `right` blank columns.
    ///
    /// Used for list indentation and block quote gutters.
    pub fn indent(&self, left: u16, right: u16, fill: Style) -> Canvas {
        let mut out = Canvas::new(self.width + left + right, self.height(), fill);
        out.blit(0, usize::from(left), self, fill);
        out
    }

    /// Returns rows `start..start + len` as a canvas of the same width.
    ///
    /// This is what the viewport uses to show a slice of the document. Anchors and
    /// spans falling inside the slice are translated; the rest are dropped.
    pub fn slice_rows(&self, start: usize, len: usize) -> Canvas {
        let end = (start + len).min(self.height());
        let start = start.min(end);
        let mut out = Canvas::empty(self.width);
        out.rows = self.rows[start..end].to_vec();
        out.anchors = self
            .anchors
            .iter()
            .filter(|a| (start..end).contains(&a.row))
            .map(|a| Anchor {
                id: a.id.clone(),
                level: a.level,
                row: a.row - start,
            })
            .collect();
        out.spans = self
            .spans
            .iter()
            .filter(|s| (start..end).contains(&s.row))
            .map(|s| SearchSpan {
                row: s.row - start,
                ..*s
            })
            .collect();
        out
    }

    /// Draws a frame around `self`, returning a canvas two columns wider and two rows
    /// taller.
    ///
    /// `title`, when given, is written into the top edge one column in from the left
    /// corner, surrounded by a space on each side. A title longer than the top edge is
    /// clipped, so the corner glyphs always survive; a title is only drawn at all when
    /// the content is at least four columns wide.
    pub fn framed(
        &self,
        border: BorderSet,
        border_style: Style,
        title: Option<&Line>,
        fill: Style,
    ) -> Canvas {
        let width = self.width + 2;
        let height = self.height() + 2;
        let mut out = Canvas::new(width, height, fill);
        let inner = usize::from(self.width);

        out.write_str(0, 0, &border.top_left.to_string(), border_style);
        out.hline(0, 1, inner, &border.horizontal.to_string(), border_style);
        out.write_str(0, inner + 1, &border.top_right.to_string(), border_style);

        out.write_str(height - 1, 0, &border.bottom_left.to_string(), border_style);
        out.hline(
            height - 1,
            1,
            inner,
            &border.horizontal.to_string(),
            border_style,
        );
        out.write_str(
            height - 1,
            inner + 1,
            &border.bottom_right.to_string(),
            border_style,
        );

        for row in 1..height - 1 {
            out.write_str(row, 0, &border.vertical.to_string(), border_style);
            out.write_str(row, inner + 1, &border.vertical.to_string(), border_style);
        }

        if let Some(title) = title
            && inner >= 4
        {
            let mut label = Line::empty();
            label.push(crate::text::Span::new(" ", border_style));
            for span in &title.spans {
                label.push(span.clone());
            }
            label.push(crate::text::Span::new(" ", border_style));
            // Clip to the inner width so an over-long title can never overwrite the
            // top-right corner glyph.
            out.write_line(0, 1, &label.truncated(inner), border_style);
        }

        out.blit(1, 1, self, fill);
        out
    }

    /// Draws a full-width horizontal rule as a new row.
    ///
    /// Returns the index of the new row.
    pub fn push_rule(&mut self, cluster: &str, style: Style) -> usize {
        let row = self.push_blank_row(style);
        self.hline(row, 0, usize::from(self.width), cluster, style);
        row
    }

    /// Appends a row holding `text` aligned within the canvas, truncating with an
    /// ellipsis if it does not fit.
    ///
    /// Returns the index of the new row.
    pub fn push_text_ellipsized(&mut self, text: &str, align: Align, style: Style) -> usize {
        let width = usize::from(self.width);
        if display_width(text) <= width {
            return self.push_text(text, align, style);
        }
        let head = crate::text::truncate_to_width(text, width.saturating_sub(1));
        let shortened = format!("{head}…");
        self.push_text(&shortened, align, style)
    }
}

/// Copies `cells` into a row of exactly `width` columns.
fn pad_cells(cells: &[Cell], width: u16, fill: Style) -> Vec<Cell> {
    let mut out = cells.to_vec();
    let current: usize = out.iter().map(|c| usize::from(c.width())).sum();
    let target = usize::from(width);
    if current < target {
        out.extend((current..target).map(|_| Cell::blank(fill)));
    } else if current > target {
        out.truncate(target);
        if out.last().is_some_and(|c| c.width() == 2) {
            let last = out.len() - 1;
            out[last] = Cell::blank(fill);
        }
    }
    out
}
