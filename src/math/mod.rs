// SPDX-License-Identifier: MIT
//! LaTeX math, laid out for a terminal.
//!
//! Design spec `docs/superpowers/specs/2026-08-19-math-design.md`. A sibling of
//! `crate::mermaid`: source text in, drawn output out, and no knowledge of `render` or
//! `tui`.
//!
//! There is one engine. [`build`] turns the event stream into the box tree of design spec
//! §4 and [`draw`] puts that tree onto cells; inline is not a second walk but the same
//! tree under the constraint `above == 0 && below == 0`, and `build::Mode` is the one flag
//! that says so. A construct that cannot meet the constraint rewrites itself onto the row
//! — a fraction becomes `a/b` — or fails by name so the caller can show the source.
//!
//! Both halves have an entry point — [`render_inline`] and [`render_display`] — but only
//! the inline one has a caller in `render`. `render::document` still shows a display
//! formula as its own source; wiring it to [`render_display`] is a later task.

mod boxes;
mod build;
mod delim;
mod draw;
pub(crate) mod scripts;
mod spacing;

#[cfg(test)]
mod tests;

use pulldown_latex::Storage;

use crate::canvas::Canvas;
use crate::error::MathError;
use crate::theme::Theme;

/// Draws `src` as one row of text.
///
/// Design spec §5. The formula is built as a box tree and constrained to the row the prose
/// sits on; a construct that needs a second row rewrites itself where an honest one-row
/// form exists and fails by name where it does not.
///
/// The row comes back as the engine built it, leading and trailing columns included. A
/// formula may honestly begin or end with one: `$\,x$` asked for a thin space, and `${}-x$`
/// is the idiom that keeps a minus binary, so it sets ` − x` where `$-x$` sets `−x`.
/// Trimming here would be a second place where spacing is decided, and `spacing.rs` is
/// meant to be the only one; it would also make this function disagree with `draw::to_row`
/// over the same box, which is the one engine splitting in two again by the back door.
///
/// # Errors
///
/// [`MathError::Parse`] if the LaTeX does not parse, [`MathError::NotInline`] if it parses
/// but cannot be written on one row.
pub fn render_inline(src: &str) -> Result<String, MathError> {
    let storage = Storage::new();
    let events = build::parse(src, &storage)?;
    let laid_out = build::build(&events, build::Mode::Inline)?;
    draw::to_row(&laid_out)
}

/// Draws `src` as a block of box art at least `width` columns wide.
///
/// Design spec §6. The canvas is exactly `width` columns on every row: a narrower formula
/// is padded out to the measure, and a wider one does not come back as a canvas at all.
/// `draw::to_canvas` treats `width` as a floor rather than a cap, but the comparison here
/// happens first, so nothing wider than `width` ever reaches it by this route.
///
/// A formula that draws no cells returns an empty canvas rather than an error: design spec
/// §16.3 makes a definition-only block contribute no rows, and stating the rule over the
/// *result* is what keeps `\newcommand`, `\def`, a comment and whitespace behaving alike
/// with no list to maintain.
///
/// # Errors
///
/// [`MathError::Parse`] if the LaTeX does not parse. [`MathError::TooWide`] if it draws
/// wider than `width`, carrying the width it needs — a formula has exactly one width
/// (design spec §7), so `needed` is the answer and not a hint to search from.
///
/// [`MathError::NotInline`] as well, despite the name, and it is not a bug: `build::parse`
/// refuses a pathological source before the mode is consulted at all, and a construct this
/// engine cannot build in *either* mode — a grid — reports itself the same way. Every one
/// of the three takes the caller to design spec §9's framed source, which is why one
/// return type covers them.
pub fn render_display(src: &str, width: u16, theme: &Theme) -> Result<Canvas, MathError> {
    let storage = Storage::new();
    let events = build::parse(src, &storage)?;
    let laid_out = build::build(&events, build::Mode::Display)?;
    if laid_out.is_empty() {
        return Ok(Canvas::empty(width));
    }
    if laid_out.width > width {
        return Err(MathError::TooWide {
            needed: laid_out.width,
        });
    }
    Ok(draw::to_canvas(&laid_out, width, theme))
}

/// The characters `src`'s own commands resolved to.
///
/// Design spec §13: `pulldown-latex` resolves `\alpha` to `α`, and that character is the
/// *document's* — asked for by name — while the radical sign, the slash and the script
/// forms this crate puts around it are mdmost's. `tests/glyph_inventory.rs` subtracts this
/// from what the renderer drew and claims the rest.
///
/// # Errors
///
/// [`MathError::Parse`] if the LaTeX does not parse.
pub fn symbols(src: &str) -> Result<String, MathError> {
    let storage = Storage::new();
    let events = build::parse(src, &storage)?;
    let mut out = String::new();
    for event in &events {
        if let pulldown_latex::event::Event::Content(content) = event {
            out.push_str(&build::atom(content).1.plain_text());
        }
    }
    Ok(out)
}
