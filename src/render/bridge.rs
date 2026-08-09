//! Calls out to the two renderer collaborators owned by other workstreams.
//!
//! The block renderer depends on two functions it does not own:
//!
//! * `crate::highlight::highlight(lang, src, &Theme) -> Vec<Line>`
//! * `crate::mermaid::render_mermaid_with(src, width, &Theme, Fit) -> Result<Canvas, MermaidError>`
//!
//! Routing both through this module keeps the dependency in one place, so a change on
//! either side is a change to one function here rather than to every call site.
//!
//! A Mermaid failure is never fatal: [`render_code_block`](super::code::render_code_block)
//! turns the error into a syntax-highlighted code block with a dim caption naming the
//! reason (design spec §6).

use crate::canvas::Canvas;
use crate::error::MermaidError;
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
