//! The caller-supplied seam: what a graph node *is*, and where edges may touch it.
//!
//! The layered graph engine ([`graph`](super)) knows how to break cycles, assign
//! layers, reduce crossings, place boxes on a character grid and route orthogonal
//! edges between them. It deliberately knows *nothing* about what is inside a box.
//! A flowchart node is a shaped outline around a label; a `classDiagram` node is a
//! three-compartment box; an `erDiagram` node is an attribute table. All three are the
//! same problem to the engine, and that is what stops design spec §6.3, §6.4 and §6.7
//! becoming copy-pasted forks of §6.1 (design spec §14).
//!
//! The seam is the [`NodeContent`] trait. A family implements it once per node kind,
//! hands the engine a `Vec<Box<dyn NodeContent>>`, and gets a laid-out canvas back.
//! The trait is object-safe on purpose: no generics, no associated types.
//!
//! # Contract
//!
//! For every implementor and every `size` the engine may pass to [`NodeContent::draw`]:
//!
//! * `minimum().width <= natural().width` and likewise for height.
//! * `fit(w)` returns a size with `width <= w.max(minimum().width)`; the engine never
//!   asks for less than `minimum()`.
//! * `draw(size, theme)` returns a canvas that is *exactly* `size.width` columns and
//!   `size.height` rows. The engine blits it verbatim; it does not pad or crop.
//! * `ports(size)` returns offsets that lie strictly inside the drawn outline for that
//!   same `size`.
//!
//! Implementations must be pure functions of `(self, size, theme)` — the engine calls
//! `measure` and `draw` at different times and assumes they agree (design spec §3).

use crate::canvas::Canvas;
use crate::theme::{Style, Theme};

/// A size in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Size {
    /// Width in display columns.
    pub width: u16,
    /// Height in rows.
    pub height: u16,
}

impl Size {
    /// A size of `width` columns by `height` rows.
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    /// The component-wise maximum of two sizes.
    pub fn max(self, other: Self) -> Self {
        Self::new(self.width.max(other.width), self.height.max(other.height))
    }
}

/// One of the four sides of a node box.
///
/// Sides are named in canvas terms, not flow terms: [`Side::Top`] is always the box's
/// first row, whatever the diagram's [`Direction`](crate::mermaid::ast::Direction) is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    /// The first row of the box.
    Top,
    /// The last row of the box.
    Bottom,
    /// The first column of the box.
    Left,
    /// The last column of the box.
    Right,
}

impl Side {
    /// The side directly across the box.
    pub fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// Whether the side is a horizontal edge of the box (top or bottom).
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

/// The offsets along each side at which an edge may legally attach.
///
/// Offsets are measured from the box's top-left corner: along [`Side::Top`] and
/// [`Side::Bottom`] they are columns `0..width`, along [`Side::Left`] and
/// [`Side::Right`] they are rows `0..height`.
///
/// These are *candidate* offsets, not assignments. The engine allocates a distinct
/// offset per incident edge — deterministically, centre-outward — which is how
/// multi-edges and self-loops get separated. A shape with a single sensible
/// attachment point (a rhombus apex, a circle's pole) simply offers one offset per
/// side and the engine stacks edges on it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ports {
    /// Columns on the top edge, ascending.
    pub top: Vec<u16>,
    /// Columns on the bottom edge, ascending.
    pub bottom: Vec<u16>,
    /// Rows on the left edge, ascending.
    pub left: Vec<u16>,
    /// Rows on the right edge, ascending.
    pub right: Vec<u16>,
}

impl Ports {
    /// The candidate offsets on one side.
    pub fn side(&self, side: Side) -> &[u16] {
        match side {
            Side::Top => &self.top,
            Side::Bottom => &self.bottom,
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    /// Ports for a plain rectangular outline: every cell except the corners.
    ///
    /// This is the right answer for every box-shaped node — flowchart rect, stadium,
    /// subroutine, cylinder, class compartment box, ER attribute table.
    pub fn rectangular(size: Size) -> Self {
        let cols: Vec<u16> = (1..size.width.saturating_sub(1)).collect();
        let rows: Vec<u16> = (1..size.height.saturating_sub(1)).collect();
        Self {
            top: cols.clone(),
            bottom: cols,
            left: rows.clone(),
            right: rows,
        }
    }

    /// Ports for a shape that can only be met at the middle of each side.
    ///
    /// The right answer for a rhombus (its apexes) and a circle (its poles).
    pub fn centred(size: Size) -> Self {
        let col = vec![size.width / 2];
        let row = vec![size.height / 2];
        Self {
            top: col.clone(),
            bottom: col,
            left: row.clone(),
            right: row,
        }
    }
}

/// A node's drawn body, measured and painted by the family that owns it.
///
/// See the [module documentation](self) for the contract implementors must honour.
pub trait NodeContent {
    /// The size the node wants when nothing has to wrap.
    fn natural(&self) -> Size;

    /// The smallest size the node can still be drawn at legibly.
    ///
    /// The engine will never call [`draw`](NodeContent::draw) with less than this; if
    /// even the minimum does not fit the width budget it reports
    /// [`MermaidError::TooNarrow`](crate::error::MermaidError::TooNarrow).
    fn minimum(&self) -> Size;

    /// The size the node takes when its width is capped at `max_width`.
    ///
    /// Typically taller than [`natural`](NodeContent::natural) because the label wraps.
    /// Returning a width above `max_width` is allowed only when
    /// `minimum().width > max_width`.
    fn fit(&self, max_width: u16) -> Size;

    /// Paints the node at exactly `size`.
    ///
    /// The returned canvas must be `size.width` columns by `size.height` rows.
    fn draw(&self, size: Size, theme: &Theme) -> Canvas;

    /// The offsets at which an edge may attach, for a node drawn at `size`.
    fn ports(&self, size: Size) -> Ports;

    /// Stamps the node's outline where an edge meets it.
    ///
    /// `canvas` is the node's own canvas, already drawn at `size`, and `offset` is one
    /// of the values [`ports`](NodeContent::ports) offered for `side`. The default
    /// replaces the border glyph with the matching tee (`┬ ┴ ├ ┤`), which is right for
    /// any box-shaped outline. Shapes whose outline is not a straight border at the
    /// attachment point — rhombus, circle — override this with a no-op so their apex
    /// glyph survives.
    fn mark_port(&self, canvas: &mut Canvas, size: Size, side: Side, offset: u16, style: Style) {
        stamp_tee(canvas, size, side, offset, style);
    }
}

/// The default [`NodeContent::mark_port`] behaviour: a tee glyph on a box border.
///
/// Exposed so an implementor that overrides `mark_port` for *some* sides can still
/// delegate the rest.
pub fn stamp_tee(canvas: &mut Canvas, size: Size, side: Side, offset: u16, style: Style) {
    let (row, col, glyph) = match side {
        Side::Top => (0, usize::from(offset), "┬"),
        Side::Bottom => (usize::from(size.height.saturating_sub(1)), offset.into(), "┴"),
        Side::Left => (usize::from(offset), 0, "├"),
        Side::Right => (
            usize::from(offset),
            usize::from(size.width.saturating_sub(1)),
            "┤",
        ),
    };
    canvas.write_str(row, col, glyph, style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_ports_exclude_the_corners() {
        let ports = Ports::rectangular(Size::new(5, 3));
        assert_eq!(ports.top, vec![1, 2, 3]);
        assert_eq!(ports.left, vec![1]);
    }

    #[test]
    fn degenerate_sizes_produce_no_ports_rather_than_panicking() {
        for (w, h) in [(0, 0), (1, 1), (2, 2)] {
            let ports = Ports::rectangular(Size::new(w, h));
            assert!(ports.top.is_empty() || w > 2);
            assert!(ports.left.is_empty() || h > 2);
        }
    }

    #[test]
    fn centred_ports_are_the_middle_of_each_side() {
        let ports = Ports::centred(Size::new(7, 3));
        assert_eq!(ports.top, vec![3]);
        assert_eq!(ports.right, vec![1]);
    }

    #[test]
    fn sides_are_symmetric() {
        for side in [Side::Top, Side::Bottom, Side::Left, Side::Right] {
            assert_eq!(side.opposite().opposite(), side);
            assert_eq!(side.is_horizontal(), side.opposite().is_horizontal());
        }
    }
}
