//! The mapping between flow space and canvas cells.
//!
//! Layout and routing are written once, for a graph that flows in the direction of
//! increasing *flow* with boxes spread along the *cross* axis. [`Frame`] turns those
//! coordinates into canvas rows and columns, which is all that separates `TD` from
//! `BT`, `LR` and `RL` (design spec §6.1).

use crate::canvas::Canvas;
use crate::mermaid::ast::Direction;
use crate::mermaid::chrome;
use crate::theme::Style;

use super::glyph::{Dir, Stroke};
use super::ink::Ink;
use super::spec::DrawnLabel;

/// Maps flow-space coordinates onto a canvas of a known size.
#[derive(Debug, Clone, Copy)]
pub(super) struct Frame {
    direction: Direction,
    total_flow: usize,
    total_cross: usize,
}

impl Frame {
    /// Builds a frame for a graph of `total_flow` × `total_cross` cells.
    pub(super) fn new(direction: Direction, total_flow: usize, total_cross: usize) -> Self {
        Self {
            direction,
            total_flow,
            total_cross,
        }
    }

    /// True when a direction's flow axis runs down or up the canvas.
    pub(super) fn vertical(direction: Direction) -> bool {
        matches!(direction, Direction::TopToBottom | Direction::BottomToTop)
    }

    /// True when this frame's flow axis runs vertically.
    pub(super) fn is_vertical(&self) -> bool {
        Self::vertical(self.direction)
    }

    /// The canvas size as `(rows, cols)`.
    pub(super) fn size(&self) -> (usize, usize) {
        if Self::vertical(self.direction) {
            (self.total_flow, self.total_cross)
        } else {
            (self.total_cross, self.total_flow)
        }
    }

    /// The canvas cell a single flow-space cell maps to.
    pub(super) fn cell(&self, flow: usize, cross: usize) -> (usize, usize) {
        match self.direction {
            Direction::TopToBottom => (flow, cross),
            Direction::BottomToTop => (self.total_flow.saturating_sub(flow + 1), cross),
            Direction::LeftToRight => (cross, flow),
            Direction::RightToLeft => (cross, self.total_flow.saturating_sub(flow + 1)),
        }
    }

    /// The top-left canvas cell of a flow-space block.
    pub(super) fn origin(
        &self,
        flow: usize,
        cross: usize,
        flow_size: usize,
        cross_size: usize,
    ) -> (usize, usize) {
        let _ = cross_size;
        let far = flow + flow_size.saturating_sub(1);
        match self.direction {
            Direction::TopToBottom | Direction::LeftToRight => self.cell(flow, cross),
            Direction::BottomToTop | Direction::RightToLeft => self.cell(far, cross),
        }
    }

    /// The canvas direction of increasing flow.
    pub(super) fn forward(&self) -> Dir {
        match self.direction {
            Direction::TopToBottom => Dir::Down,
            Direction::BottomToTop => Dir::Up,
            Direction::LeftToRight => Dir::Right,
            Direction::RightToLeft => Dir::Left,
        }
    }

    /// The canvas direction of increasing cross position.
    pub(super) fn across(&self) -> Dir {
        if Self::vertical(self.direction) {
            Dir::Right
        } else {
            Dir::Down
        }
    }
}

/// Draws into a canvas and its edge overlay using flow-space coordinates.
pub(super) struct Pen {
    /// The canvas being built.
    pub canvas: Canvas,
    /// The edge overlay, merged onto the canvas at the end.
    pub ink: Ink,
    /// The coordinate mapping.
    pub frame: Frame,
    /// Style for edge labels.
    pub label_style: Style,
}

impl Pen {
    /// Creates a pen over a blank canvas of the frame's size.
    pub(super) fn new(frame: Frame, base: Style, label_style: Style) -> Self {
        let (rows, cols) = frame.size();
        Self {
            canvas: Canvas::new(cols as u16, rows, base),
            ink: Ink::new(rows, cols),
            frame,
            label_style,
        }
    }

    /// Draws `len` cells of line along the flow direction, starting at `(flow, cross)`.
    pub(super) fn run_flow(&mut self, flow: usize, cross: usize, len: usize, stroke: Stroke) {
        if len == 0 {
            return;
        }
        let (row, col) = self.frame.cell(flow, cross);
        let dir = self.frame.forward();
        self.ink.run(row, col, dir, len, stroke);
    }

    /// Draws `len` cells of line across the flow direction, starting at `(flow, cross)`.
    pub(super) fn run_cross(&mut self, flow: usize, cross: usize, len: usize, stroke: Stroke) {
        if len == 0 {
            return;
        }
        let (row, col) = self.frame.cell(flow, cross);
        let dir = self.frame.across();
        self.ink.run(row, col, dir, len, stroke);
    }

    /// Places a terminator's glyphs along the flow axis.
    ///
    /// With `forwards` the first glyph sits at `flow` and the last one furthest along;
    /// otherwise the sequence is laid out backwards, which is how a tail terminator
    /// keeps its innermost glyph against its node.
    pub(super) fn terminator(
        &mut self,
        flow: usize,
        cross: usize,
        glyphs: &str,
        stroke: Stroke,
        forwards: bool,
    ) {
        let count = glyphs.chars().count();
        for (index, ch) in glyphs.chars().enumerate() {
            let at = if forwards {
                flow + index
            } else {
                flow + count - 1 - index
            };
            let (row, col) = self.frame.cell(at, cross);
            self.ink.put(row, col, ch, stroke, true);
        }
    }

    /// True when a line may pass through this flow-space cell.
    ///
    /// Blank cells and existing box art qualify — box art merges into a junction. Any
    /// other content (a label, a node's text) does not, so a route can never run
    /// through something a caller has drawn.
    pub(super) fn passable(&self, flow: usize, cross: usize) -> bool {
        let (row, col) = self.frame.cell(flow, cross);
        match self.canvas.row(row).and_then(|cells| cells.get(col)) {
            None => false,
            Some(cell) => {
                let ch = cell.text().chars().next().unwrap_or(' ');
                ch == ' ' || super::glyph::mask_of(ch).is_some()
            }
        }
    }

    /// Writes an edge label whose block starts at `(flow, cross)` in flow space, and
    /// maps every drawn row back to the document bytes that drew it.
    ///
    /// The spans come from [`chrome::label_spans`], run by run, so a reader dragging
    /// across a wrapped edge label gets the characters they went over rather than the
    /// whole label (design spec §2.2). A label with no source — a synthesised one, or a
    /// container key standing in for a title — emits nothing, which is that helper's own
    /// rule and not a second one written here.
    pub(super) fn drawn_label(&mut self, flow: usize, cross: usize, label: &DrawnLabel) {
        let (row, col) = self.write_lines(flow, cross, &label.lines());
        for (index, piece) in label.rows.iter().enumerate() {
            chrome::label_spans(&mut self.canvas, &label.label, piece, row + index, col);
        }
    }

    /// Writes an end note — a class cardinality — whose block starts at `(flow, cross)`.
    ///
    /// A bare string, because that is all a cardinality ever is: the parser reads it as a
    /// quoted word and never builds a [`Label`](crate::mermaid::ast::Label) for it. So it
    /// draws without a span rather than being handed an empty label to fit
    /// [`drawn_label`](Pen::drawn_label) — an empty `Label::source` emits nothing anyway,
    /// and a span at byte zero of the document would be worse than none at all.
    pub(super) fn note(&mut self, flow: usize, cross: usize, text: &str) {
        self.write_lines(flow, cross, &[text]);
    }

    /// Paints `lines` as a block starting at `(flow, cross)`, returning its top-left cell.
    fn write_lines(&mut self, flow: usize, cross: usize, lines: &[&str]) -> (usize, usize) {
        let width = lines
            .iter()
            .map(|line| crate::text::display_width(line))
            .max()
            .unwrap_or(0);
        let (flow_size, cross_size) = if self.frame.is_vertical() {
            (lines.len(), width)
        } else {
            (width, lines.len())
        };
        let (row, col) = self.frame.origin(flow, cross, flow_size, cross_size);
        let style = self.label_style;
        for (index, line) in lines.iter().enumerate() {
            self.canvas.write_str(row + index, col, line, style);
        }
        (row, col)
    }
}
