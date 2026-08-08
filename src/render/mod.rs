//! Renderers: AST plus width budget plus theme in, [`Canvas`](crate::canvas::Canvas) out.
//!
//! Rendering is recursive over a *width budget*. A renderer is handed the number of
//! columns it may occupy and must return a canvas exactly that wide. A table cell is a
//! nested document rendered at its column's budget — that is what makes Markdown
//! inside table cells work without special-casing.
//!
//! # Entry points
//!
//! * [`render_document`] — the whole document; the viewport blits slices of its canvas.
//! * [`block::render_block`] / [`block::render_blocks`] — one block, or a sequence.
//! * [`table::render_table`] — a GFM table (design spec §7).
//! * [`inline::wrap`] — styled runs to wrapped lines, delegating to
//!   [`crate::text::wrap_spans`].
//!
//! # Purity
//!
//! Nothing here retains state between calls, and no output depends on anything but
//! `(AST, width, theme)` (design spec §3). Rendering the same document twice at the
//! same width produces byte-identical canvases.
//!
//! ```
//! use mdless::doc::Doc;
//! use mdless::render::render_document;
//! use mdless::theme::Theme;
//!
//! let doc = Doc::parse("# Title\n\nHello.\n");
//! let canvas = render_document(&doc, 20, &Theme::default_dark());
//! assert_eq!(canvas.width(), 20);
//! assert_eq!(canvas.anchors()[0].id, "title");
//! ```

pub mod block;
mod bridge;
mod code;
pub mod inline;
pub mod table;

#[cfg(test)]
mod tests;

use crate::canvas::Canvas;
use crate::doc::Doc;
use crate::theme::{Style, Theme};

pub use block::{render_block, render_blocks};
pub use inline::wrap;
pub use table::{render_table, render_table_full};

/// The immutable context threaded through the recursive renderers.
///
/// It carries the theme plus the nesting depths that change how a block looks — the
/// bullet glyph of a list, the accent colour of a quote bar. It is `Copy`, so a nested
/// renderer takes a modified copy rather than mutating shared state, which is what
/// keeps rendering a pure function.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx<'a> {
    /// The active theme.
    pub theme: &'a Theme,
    /// The style canvases are filled with and inline text inherits.
    ///
    /// Body text at the top level, quote text inside a block quote; a span that sets
    /// no colour of its own is drawn in this style.
    pub base: Style,
    /// How many lists enclose the current node.
    pub list_depth: usize,
    /// How many block quotes enclose the current node.
    pub quote_depth: usize,
    /// How many tables enclose the current node, to stop pathological recursion.
    pub table_depth: usize,
}

/// The deepest table nesting that is rendered; deeper tables degrade to their text.
///
/// Tables nested inside tables are legal and supported (design spec §7.4); this bound
/// only guards against a document that nests them without end.
pub(crate) const MAX_TABLE_DEPTH: usize = 4;

impl<'a> Ctx<'a> {
    /// The context a top-level document render starts from.
    pub(crate) fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            base: theme.base(),
            list_depth: 0,
            quote_depth: 0,
            table_depth: 0,
        }
    }

    /// The context for content one list level deeper.
    pub(crate) fn in_list(self) -> Self {
        Self {
            list_depth: self.list_depth + 1,
            ..self
        }
    }

    /// The context for content one block quote deeper.
    pub(crate) fn in_quote(self) -> Self {
        Self {
            quote_depth: self.quote_depth + 1,
            base: self.base.patch(self.theme.block.quote_text),
            ..self
        }
    }

    /// The context for the content of a table cell, drawn in the cell style.
    pub(crate) fn in_cell(self, base: Style) -> Self {
        Self { base, ..self }
    }

    /// The context for content one table deeper.
    pub(crate) fn in_table(self) -> Self {
        Self {
            table_depth: self.table_depth + 1,
            ..self
        }
    }
}

/// Renders a whole document at `width` columns.
///
/// The returned canvas is exactly `width` columns wide on every row and carries:
///
/// * one [`Anchor`](crate::canvas::Anchor) per heading, at the row the heading starts
///   on, so the table of contents can jump to it;
/// * [`SearchSpan`](crate::canvas::SearchSpan)s mapping source byte ranges to canvas
///   positions, so search hits can be highlighted.
pub fn render_document(doc: &Doc, width: u16, theme: &Theme) -> Canvas {
    let mut canvas = block::render_sequence(&doc.root().children, width, Ctx::new(theme), true);
    canvas.resize_width(width, theme.base());
    canvas
}
