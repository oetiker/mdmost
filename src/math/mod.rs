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
//! Only the inline half is wired to a caller so far. `render::document` still shows a
//! display formula as its own source; the canvas `draw` already draws for it is reached by
//! a later stage.

mod boxes;
mod build;
mod draw;
pub(crate) mod scripts;
mod spacing;

#[cfg(test)]
mod tests;

use pulldown_latex::Storage;

use crate::error::MathError;

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
