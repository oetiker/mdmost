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
//! Each half has one entry point. [`render_inline`] draws a formula onto the prose row;
//! [`render_display_natural`] draws one as a block, at the width the formula itself drew
//! at, which is what a caller centring it or measuring a table column needs.
//!
//! There used to be a second display entry point, `render_display`, which padded the canvas
//! out to a column width the caller named. It was retired: nothing outside its own tests
//! had called it since Task 11, because `render` wants the natural width throughout — see
//! `render::bridge` — and applies its own padding where it decides the centring, in
//! `render::math::centred`. Six doc comments were being maintained to explain why nothing
//! called it. The padding itself is [`Canvas::resize_width`](crate::canvas::Canvas), which
//! has its own tests and a dozen other callers.

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
/// [`MathError::Parse`] if the LaTeX does not parse, [`MathError::NotDrawable`] if it parses
/// but this engine will not draw it on the prose row: a construct with no honest one-row
/// form, or a source `build` refuses whatever the mode.
pub fn render_inline(src: &str) -> Result<String, MathError> {
    let storage = Storage::new();
    let events = build::parse(src, &storage)?;
    let laid_out = build::build(&events, build::Mode::Inline)?;
    draw::to_row(&laid_out)
}

/// Draws `src` as a block of box art at the width the formula itself draws at.
///
/// Design spec §6. `width` is the cap the caller can show, not the width of the answer: a
/// formula wider than it does not come back as a canvas at all, and a narrower one comes
/// back at its own width with no padding.
///
/// **The canvas is not padded out to `width`, and that is the point.** Two callers need the
/// width the formula asked for, and neither can recover it from a padded canvas, because
/// the padding and the formula are then the same cells:
///
/// * **Centring** (design spec §7) needs something to centre. A canvas that already fills
///   the measure has no slack in it, so centring it is a no-op.
/// * **Measuring a table cell** needs the column width a formula asks for. A drawn formula
///   cannot be squeezed, so this one number is both its minimum and its natural width.
///
/// A caller that does want a canvas filling a fixed measure pads it itself, with
/// [`Canvas::resize_width`](crate::canvas::Canvas) — which is all the retired
/// `render_display` did, and is what `render::block` and `render::code` do for every other
/// kind of block.
///
/// Pass `u16::MAX` for `width` to ask with no cap at all, which is what a measurement
/// does: there is no column yet to be too wide for.
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
/// (design spec §7), so `needed` is the answer and not a hint to search from. `width` is
/// compared against the laid-out width before anything is drawn, so a formula past the cap
/// costs a layout and not a canvas.
///
/// [`MathError::NotDrawable`] if this engine will not build the formula at all: a source
/// past one of `build`'s caps, or a construct no mode draws yet — a grid, which is stage 3.
/// All three take the caller to design spec §9's framed source, which is why one return
/// type covers them.
pub fn render_display_natural(src: &str, width: u16, theme: &Theme) -> Result<Canvas, MathError> {
    let storage = Storage::new();
    let events = build::parse(src, &storage)?;
    let laid_out = build::build(&events, build::Mode::Display)?;
    if laid_out.is_empty() {
        // Width 0, not `width`: a formula that draws no cells has no width of its own,
        // and a caller that needs one filling a measure pads it with `Canvas::resize_width`
        // like any other block. Design spec §16.3 makes it contribute no rows either way.
        return Ok(Canvas::empty(0));
    }
    if laid_out.width > width {
        return Err(MathError::TooWide {
            needed: laid_out.width,
        });
    }
    Ok(draw::to_canvas(&laid_out, laid_out.width, theme))
}

/// The characters `src`'s own commands resolved to.
///
/// Design spec §13: `pulldown-latex` resolves `\alpha` to `α`, and that character is the
/// *document's* — asked for by name — while the radical sign, the slash and the script
/// forms this crate puts around it are mdmost's. `tests/glyph_inventory.rs` subtracts this
/// from what the renderer drew and claims the rest.
///
/// A font command is the same case: an author who writes `\mathbb{R}` asked for `ℝ`, so
/// that is what comes back — not the `R` they typed, which is never drawn. The walk is
/// `build::content_text`, which tracks the font state, and it is deliberately not the
/// layout walk: a construct this engine refuses to draw is still made of its author's
/// characters, and only a parse failure is an error here.
///
/// # Errors
///
/// [`MathError::Parse`] if the LaTeX does not parse.
pub fn symbols(src: &str) -> Result<String, MathError> {
    let storage = Storage::new();
    let events = build::parse(src, &storage)?;
    Ok(build::content_text(&events))
}
