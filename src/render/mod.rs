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
//! use mdless::render::{RenderOptions, render_document};
//! use mdless::theme::Theme;
//!
//! let doc = Doc::parse("# Title\n\nHello.\n");
//! let options = RenderOptions::default();
//! let canvas = render_document(&doc, 20, &Theme::default_dark(), &options);
//! assert_eq!(canvas.width(), 20);
//! assert_eq!(canvas.anchors()[0].id, "title");
//! ```

pub mod block;
mod bridge;
mod code;
mod glyphs;
pub mod inline;
pub mod table;

#[cfg(test)]
mod tests;

use crate::canvas::Canvas;
use crate::doc::Doc;
use crate::theme::{Style, Theme};

use glyphs::Glyphs;

pub use block::{render_block, render_blocks};
pub use inline::wrap;
pub use table::{render_table, render_table_full};

/// Render-time capabilities that come from CLI flags and configuration rather than
/// from the theme.
///
/// The theme decides what things *look* like; these decide what the renderer is
/// *allowed to draw*. They are a separate input on purpose: two documents rendered
/// with the same theme but different options are different renders.
///
/// # Cache key
///
/// Rendering is a pure function of `(AST, width, theme, options)` — a render cache
/// must therefore include these in its key, alongside the document version, the width
/// and the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptions {
    /// Whether Nerd Font glyphs may be used.
    ///
    /// `false` (from `--no-icons` or `icons = false`) substitutes plain Unicode of
    /// the same display width, so the layout is identical either way.
    pub icons: bool,
    /// Whether fenced code blocks get a line-number gutter.
    pub line_numbers: bool,
}

impl RenderOptions {
    /// Creates options from the two flags.
    pub const fn new(icons: bool, line_numbers: bool) -> Self {
        Self {
            icons,
            line_numbers,
        }
    }

    /// The glyph set these options select.
    pub(crate) const fn glyphs(&self) -> Glyphs {
        Glyphs::new(self.icons)
    }
}

impl Default for RenderOptions {
    /// Nerd Font glyphs on, line numbers off — the defaults of design spec §9 and §8.
    fn default() -> Self {
        Self::new(true, false)
    }
}

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
    /// The active render options.
    pub options: RenderOptions,
    /// The glyph set the options select, resolved once.
    pub glyphs: Glyphs,
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
    pub(crate) fn new(theme: &'a Theme, options: &RenderOptions) -> Self {
        Self {
            theme,
            options: *options,
            glyphs: options.glyphs(),
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
///
/// `options` carries the settings that are not the theme's business — Nerd Font
/// glyphs and code line numbers — and belongs in any cache key alongside the document
/// version, the width and the theme.
pub fn render_document(doc: &Doc, width: u16, theme: &Theme, options: &RenderOptions) -> Canvas {
    let ctx = Ctx::new(theme, options);
    let mut canvas = block::render_sequence(&doc.root().children, width, ctx, true);
    canvas.resize_width(width, theme.base());
    canvas
}
