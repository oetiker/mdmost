// SPDX-License-Identifier: MIT
//! Calls out to the renderer collaborators owned by other workstreams.
//!
//! The renderer depends on functions it does not own:
//!
//! * `crate::highlight::highlight(lang, src, &Theme) -> Vec<Line>`
//! * `crate::mermaid::render_mermaid_with(src, width, &Theme, Fit) -> Result<Canvas, MermaidError>`
//! * `crate::math::render_inline(src) -> Result<String, MathError>`
//! * `crate::math::render_display_natural(src, width, &Theme) -> Result<Canvas, MathError>`
//!
//! Routing all four through this module keeps the dependency in one place, so a change
//! on any side is a change to one function here rather than to every call site.
//!
//! The two math bridges take the document's macro preamble (design spec §16) and do the
//! concatenation here, on this side of the seam: `src/math/` knows nothing of `doc` and
//! must not learn, and the four callers must not each spell the join out.
//!
//! Every entry point of `math` that the renderer uses is routed here. There used to be a
//! second display one, `math::render_display`, deliberately not routed because nothing in
//! the renderer called it; it has since been retired — see [`math_natural`] for what the
//! renderer asks for instead, and why.
//!
//! A Mermaid failure is never fatal: [`render_code_block`](super::code::render_code_block)
//! turns the error into a syntax-highlighted code block with a dim caption naming the
//! reason (design spec §6).

use std::borrow::Cow;

use crate::canvas::Canvas;
use crate::error::{MathError, MermaidError};
use crate::mermaid::Fit;
use crate::text::Line;
use crate::theme::Theme;

/// Turns source code into styled lines.
pub(crate) fn highlight(language: Option<&str>, src: &str, theme: &Theme) -> Vec<Line> {
    crate::highlight::highlight(language, src, theme)
}

/// Draws a Mermaid diagram as Unicode box art, degrading as far as `fit` allows.
///
/// Named for what it returns rather than mirroring `mermaid::render_mermaid`, so the
/// two sides of the seam cannot be confused at a call site.
///
/// Every diagram the renderer draws comes through here, which is what lets a test count
/// layouts: laying a diagram out is by far the most expensive thing this renderer does,
/// and the width search added for scrollable diagrams could quietly double the work on
/// documents that never scroll. See [`MERMAID_LAYOUTS`].
///
/// # Errors
///
/// Propagates the [`MermaidError`] so the caller can degrade gracefully.
pub(crate) fn mermaid(
    src: &str,
    width: u16,
    theme: &Theme,
    fit: Fit,
) -> Result<Canvas, MermaidError> {
    #[cfg(test)]
    MERMAID_LAYOUTS.with(|count| count.set(count.get() + 1));
    crate::mermaid::render_mermaid_with(src, width, theme, fit)
}

/// `src` under `preamble`: the source the engine is handed.
///
/// Borrowed when there is no preamble, which is every formula of every document that
/// defines no macros, so those pay nothing. The join is a newline: a definition-only
/// block draws nothing (design spec §16.3), so nothing of it lands in the formula's row,
/// and a newline is what separates one block's LaTeX from the next in the source anyway.
fn with_preamble<'a>(preamble: &str, src: &'a str) -> Cow<'a, str> {
    if preamble.is_empty() {
        Cow::Borrowed(src)
    } else {
        Cow::Owned(format!("{preamble}\n{src}"))
    }
}

/// Draws a formula as one row of text, under the document's macro `preamble`.
///
/// Named for what it returns, like [`mermaid`] beside it. Routing it through this
/// module is what keeps `render`'s dependency on `math` in one place.
///
/// # Errors
///
/// Propagates the [`MathError`] so the caller can degrade to the source (design spec §9).
pub(crate) fn math_inline(preamble: &str, src: &str) -> Result<String, MathError> {
    crate::math::render_inline(&with_preamble(preamble, src))
}

/// Draws a formula as a block of box art, as wide as the formula and no wider, refusing
/// above `width`, under the document's macro `preamble`.
///
/// Named for what it returns, like [`mermaid`] and [`math_inline`] beside it.
///
/// **The renderer asks for the natural width, never a padded one.** A canvas padded out to
/// the column is the right answer for a caller laying a formula into a fixed measure and
/// the wrong one for every caller here: once it is padded, the formula and the padding are
/// the same cells, so there is nothing left to centre (design spec §7) and nothing to
/// measure a table column by. The padding this module does want is applied where the
/// centring is decided — [`super::math::centred`]. `math::render_display` existed to do it
/// the other way and was retired for want of a caller.
///
/// There is no layout counter here. A diagram is laid out repeatedly while the width
/// search hunts for a fit, which is why [`MERMAID_LAYOUTS`] exists to keep that cost
/// visible; a formula has exactly one width and is laid out once (design spec §7).
///
/// # Errors
///
/// Propagates the [`MathError`] so the caller can degrade to the framed source
/// (design spec §9).
pub(crate) fn math_natural(
    preamble: &str,
    src: &str,
    width: u16,
    theme: &Theme,
) -> Result<Canvas, MathError> {
    crate::math::render_display_natural(&with_preamble(preamble, src), width, theme)
}

// How many diagram layouts this thread has asked for. A counter rather than an
// assertion because the interesting number differs per case: one for a fence that fits,
// two for one that has to be widened, and *not* "one plus however many probes the clip
// hunt happened to take".
#[cfg(test)]
thread_local! {
    pub(crate) static MERMAID_LAYOUTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Runs `body`, returning what it produced and how many diagram layouts it cost.
#[cfg(test)]
pub(crate) fn counting_layouts<T>(body: impl FnOnce() -> T) -> (T, usize) {
    MERMAID_LAYOUTS.with(|count| count.set(0));
    let out = body();
    (out, MERMAID_LAYOUTS.with(std::cell::Cell::get))
}
