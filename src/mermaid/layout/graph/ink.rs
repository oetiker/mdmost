//! The edge overlay: a grid of line masks that is merged onto a [`Canvas`].
//!
//! Routing happens entirely in this grid, which knows nothing about diagram families.
//! Only [`Ink::apply`] touches a canvas, and it is the single place where a mask
//! becomes a character.

use crate::canvas::Canvas;
use crate::theme::Style;

use super::glyph::{Dir, Mask, Stroke, glyph, mask_of, stroke_of};

/// One cell of the overlay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Spot {
    mask: Mask,
    /// `None` until something has been drawn here, so that the first stroke wins
    /// outright instead of being merged with a default.
    stroke: Option<Stroke>,
    /// A literal glyph that replaces the mask entirely (arrowheads, terminators).
    fixed: Option<char>,
    /// True when the cell is an arrowhead or terminator rather than plain line.
    accent: bool,
}

/// A grid of line masks laid over a canvas of the same size.
#[derive(Debug, Clone)]
pub struct Ink {
    cols: usize,
    rows: usize,
    spots: Vec<Spot>,
}

impl Ink {
    /// Creates an empty overlay of `rows` × `cols` cells.
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            cols,
            rows,
            spots: vec![Spot::default(); rows * cols],
        }
    }

    fn index(&self, row: usize, col: usize) -> Option<usize> {
        (row < self.rows && col < self.cols).then(|| row * self.cols + col)
    }

    /// Adds `mask` to a cell, merging with whatever is already there.
    pub fn add(&mut self, row: usize, col: usize, mask: Mask, stroke: Stroke) {
        let Some(at) = self.index(row, col) else {
            return;
        };
        let spot = &mut self.spots[at];
        spot.mask |= mask;
        spot.stroke = Some(spot.stroke.map_or(stroke, |had| had.merge(stroke)));
    }

    /// Draws a straight run of `len` cells starting at `(row, col)` and moving `dir`.
    ///
    /// Both the entry and the exit side of every intermediate cell are marked, so the
    /// run connects to whatever it meets at either end.
    pub fn run(&mut self, row: usize, col: usize, dir: Dir, len: usize, stroke: Stroke) {
        let (mut r, mut c) = (row, col);
        for step in 0..len {
            let mut mask = dir.mask();
            if step > 0 {
                mask |= dir.flip().mask();
            }
            self.add(r, c, mask, stroke);
            (r, c) = dir.step(r, c);
        }
        // Close the far end so the last cell connects backwards.
        self.add(r, c, dir.flip().mask(), stroke);
    }

    /// Places a literal glyph, replacing any mask in that cell.
    pub fn put(&mut self, row: usize, col: usize, ch: char, stroke: Stroke, accent: bool) {
        let Some(at) = self.index(row, col) else {
            return;
        };
        self.spots[at] = Spot {
            mask: Mask::NONE,
            stroke: Some(stroke),
            fixed: Some(ch),
            accent,
        };
    }

    /// Merges the overlay onto `canvas`, which must be at least as large.
    ///
    /// Cells that already hold a box-drawing glyph — a node border — are merged rather
    /// than overwritten, which is how an edge leaving a box turns its border into a
    /// `┬`. Cells holding anything else are left untouched by plain line masks, so a
    /// stray route can never eat a label.
    pub fn apply(&self, canvas: &mut Canvas, line: Style, accent: Style) {
        for row in 0..self.rows.min(canvas.height()) {
            for col in 0..self.cols {
                let spot = self.spots[row * self.cols + col];
                if spot == Spot::default() {
                    continue;
                }
                let under = canvas
                    .row(row)
                    .and_then(|cells| cells.get(col))
                    .map(|cell| cell.text().chars().next().unwrap_or(' '))
                    .unwrap_or(' ');
                if let Some(ch) = spot.fixed {
                    let style = if spot.accent { accent } else { line };
                    let mut buf = [0u8; 4];
                    canvas.write_str(row, col, ch.encode_utf8(&mut buf), style);
                    continue;
                }
                let drawn = spot.stroke.unwrap_or_default();
                let (mask, stroke) = match mask_of(under) {
                    Some(existing) => (spot.mask | existing, drawn.merge(stroke_of(under))),
                    None if under == ' ' => (spot.mask, drawn),
                    // Something non-line already occupies the cell: leave it alone.
                    None => continue,
                };
                let mut buf = [0u8; 4];
                canvas.write_str(row, col, glyph(mask, stroke).encode_utf8(&mut buf), line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn ink_text(ink: &Ink) -> String {
        let theme = Theme::default_dark();
        let mut canvas = Canvas::new(ink.cols as u16, ink.rows, theme.base());
        ink.apply(&mut canvas, theme.base(), theme.base());
        canvas.plain_text()
    }

    #[test]
    fn crossing_runs_merge_into_a_cross() {
        let mut ink = Ink::new(3, 3);
        ink.run(1, 0, Dir::Right, 2, Stroke::Solid);
        ink.run(0, 1, Dir::Down, 2, Stroke::Solid);
        assert_eq!(ink_text(&ink), " │ \n─┼─\n │ ");
    }

    #[test]
    fn edge_merges_into_a_node_border() {
        let theme = Theme::default_dark();
        let mut canvas = Canvas::new(3, 2, theme.base());
        canvas.write_str(0, 0, "╰─╯", theme.base());
        let mut ink = Ink::new(2, 3);
        ink.run(1, 1, Dir::Up, 1, Stroke::Solid);
        ink.apply(&mut canvas, theme.base(), theme.base());
        assert_eq!(canvas.row_text(0), "╰┬╯");
    }
}
