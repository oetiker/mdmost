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
//! use mdmost::doc::Doc;
//! use mdmost::render::{RenderOptions, render_document};
//! use mdmost::theme::Theme;
//!
//! let doc = Doc::parse("# Title\n\nHello.\n");
//! let options = RenderOptions::default();
//! // `None` for the body cap: no ceiling on the prose measure beyond the width itself.
//! let canvas = render_document(&doc, 20, None, &Theme::default_dark(), &options);
//! assert_eq!(canvas.width(), 20);
//! assert_eq!(canvas.anchors()[0].id, "title");
//! // Column 0 is the document gutter, so nothing is ever written there.
//! assert!(canvas.row_text(0).starts_with(' '));
//! ```

pub(crate) mod banner;
pub mod block;
pub(crate) mod bridge;
pub(crate) mod code;
mod diagram;
pub mod document;
pub(crate) mod glyphs;
pub mod inline;
pub mod table;

#[cfg(test)]
mod tests;

use crate::canvas::Canvas;
use crate::doc::Doc;
use crate::numbering::Numbering;
use crate::theme::{Style, Theme};

use glyphs::Glyphs;

pub use block::{render_block, render_block_numbered, render_blocks};
pub(crate) use diagram::{Limits, diagram};
pub use document::render_document;
pub use inline::wrap;
pub use table::render_table;

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
    /// the same display width, so the layout is identical either way. Bullets and task
    /// boxes are ASCII in both sets, so the only thing this setting now changes in the
    /// document body is the code-fence language icons.
    pub icons: bool,
    /// Whether fenced code blocks get a line-number gutter.
    pub line_numbers: bool,
    /// Whether a document titled by a lone `#` heading gets a `FIGlet` banner (§9).
    ///
    /// **Off by default**: art in place of somebody else's title is a decoration the
    /// reader has to opt into (`title_banner = true`). Turned on, it costs nothing on a
    /// document that does not qualify — the condition is "exactly one level-1 heading,
    /// and it is the first block" — and a title too wide for the measure is wrapped
    /// between words rather than declined.
    pub title_banner: bool,
    /// Whether a deeply nested document gets section numbers in front of its headings
    /// (§9.3).
    ///
    /// On by default, and it costs nothing on a document that does not qualify: the
    /// rule is "three or more distinct numbered levels", so a flat document is
    /// unnumbered whatever this says. See [`crate::numbering`].
    pub section_numbers: bool,
}

impl RenderOptions {
    /// Creates options from the two flags, with the title banner and section numbers on.
    pub const fn new(icons: bool, line_numbers: bool) -> Self {
        Self {
            icons,
            line_numbers,
            title_banner: false,
            section_numbers: true,
        }
    }

    /// The same options with the title banner turned on or off.
    #[must_use]
    pub const fn with_title_banner(self, title_banner: bool) -> Self {
        Self {
            title_banner,
            ..self
        }
    }

    /// The same options with section numbering turned on or off.
    #[must_use]
    pub const fn with_section_numbers(self, section_numbers: bool) -> Self {
        Self {
            section_numbers,
            ..self
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
    /// The section numbers of the document being rendered, if it has any.
    ///
    /// `None` for a render that is not a whole document — a table cell rendered on its
    /// own, a block handed to [`render_block`] — because "is this the only `#`" is a
    /// question only the whole document can answer (design spec §9.3).
    pub numbers: Option<&'a Numbering>,
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
            numbers: None,
        }
    }

    /// The same context, rendering the headings of a numbered document.
    pub(crate) fn numbered(self, numbers: &'a Numbering) -> Self {
        Self {
            numbers: Some(numbers),
            ..self
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

/// Lays the whole document out flat at exactly `width` columns.
///
/// This is the layout primitive, not the renderer users meet: it reflows everything to
/// the width it is given, so a table that cannot fit comes out with its cells wrapped
/// rather than laid out wide and scrolled to. [`render_document`] is the entry point
/// every user-facing path goes through, and the only caller here is its fallback for a
/// document with bare inline content at the top level.
///
/// It stays crate-private on purpose. When it was public, tests reached for it as "the
/// document renderer" and the goldens ended up pinning 375 lines of layout that no user
/// could ever see, because the two renderers had quietly diverged and the suite was
/// standing on the wrong side of the disagreement.
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
///
/// # Margins
///
/// The document body is inset by [`DOCUMENT_MARGIN`] columns on each side, so no
/// block — paragraph, table border or code frame — is ever welded to the viewport
/// edge or to the scrollbar next to it. The inset is applied once, here, rather than
/// by every block renderer: block renderers still receive a plain width budget and
/// still return a canvas exactly that wide.
pub(crate) fn render_flat(doc: &Doc, width: u16, theme: &Theme, options: &RenderOptions) -> Canvas {
    // Computed once, here, from the whole document — never at parse time (design spec
    // §3), and never per block, which could not answer the question anyway.
    let numbers = Numbering::enabled(doc, options.section_numbers);
    let ctx = Ctx::new(theme, options).numbered(&numbers);
    let margin = margins(width);
    let body_width = width - 2 * margin;
    let blocks = &doc.root().children;
    let body = match title_banner(doc, body_width, theme, options) {
        Some(mut banner) => {
            let rest = block::render_sequence(&blocks[1..], body_width, ctx, true);
            if !rest.is_empty() {
                banner.push_blank_row(ctx.base);
                banner.append(&rest, ctx.base);
            }
            banner
        }
        None => block::render_sequence(blocks, body_width, ctx, true),
    };
    let mut canvas = body.indent(margin, margin, theme.base());
    canvas.resize_width(width, theme.base());
    canvas
}

/// The banner for a document that is titled by a lone `#` heading, if it has one.
///
/// Whether the document *has* a title is [`Doc::lone_title`]'s answer, not this
/// function's: section numbering asks the same question (design spec §9.3) and the two
/// must never disagree — a document cannot be banner'd and numbered from the wrong
/// level at the same time. What is decided here is only whether the banner can be
/// *drawn*, which needs a width and a glyph table and is therefore render-time.
///
/// Either way the question needs the whole document in view, which is why it is not
/// asked in [`block::render_block`]: a block renderer can see a level-1 heading but
/// never whether it is the only one, and answering from a block would give a
/// six-chapter manual six banners (design spec §9).
///
/// The rule under the banner is the same one an ordinary H1 draws, so a banner is a
/// change of typeface, not a different kind of thing on the page.
///
/// Both document-level entry points call this — [`render_document`] and the pager's
/// [`crate::render::render_document`], which assembles the top level block by
/// block and would otherwise silently not have the feature.
pub(crate) fn title_banner(
    doc: &Doc,
    width: u16,
    theme: &Theme,
    options: &RenderOptions,
) -> Option<Canvas> {
    if !options.title_banner {
        return None;
    }
    let ctx = Ctx::new(theme, options);
    // The structural half of the question — "is this document titled?" — is
    // `Doc::lone_title`, shared with section numbering so the two can never disagree
    // about which heading is the title. What is left here is the banner's own half:
    // whether the art can actually be drawn at this width.
    let title = doc.lone_title()?;
    let first = doc.root().children.first()?;
    let id = &title.id;
    let mut canvas = banner::render_title(first, id, width, ctx)?;
    if let Some(glyph) = block::heading_rule(1) {
        canvas.push_rule(glyph, ctx.theme.heading_rule(1));
    }
    Some(canvas)
}

/// The blank columns kept clear on each side of the document body (design spec §9).
pub const DOCUMENT_MARGIN: u16 = 1;

/// The margin actually affordable at `width`.
///
/// The gutter is dropped only when paying for it would leave the body no columns at
/// all, which is the degenerate one- and two-column case.
pub(crate) const fn margins(width: u16) -> u16 {
    if width > 2 * DOCUMENT_MARGIN {
        DOCUMENT_MARGIN
    } else {
        0
    }
}
