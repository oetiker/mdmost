//! Calls out to the two renderer collaborators owned by other workstreams.
//!
//! The block renderer depends on two functions it does not own:
//!
//! * `crate::highlight::highlight(lang, src, &Theme) -> Vec<Line>`
//! * `crate::mermaid::render_mermaid(src, width, &Theme) -> Result<Canvas, MermaidError>`
//!
//! Routing both through this module keeps the dependency in one place, so a change on
//! either side is a change to one function here rather than to every call site.
//!
//! A Mermaid failure is never fatal: [`render_code_block`](super::code::render_code_block)
//! turns the error into a syntax-highlighted code block with a dim caption naming the
//! reason (design spec §6).

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::text::Line;
use crate::theme::Theme;

/// Turns source code into styled lines.
pub(crate) fn highlight(language: Option<&str>, src: &str, theme: &Theme) -> Vec<Line> {
    crate::highlight::highlight(language, src, theme)
}

/// Draws a Mermaid diagram as Unicode box art.
///
/// Named for what it returns rather than mirroring `mermaid::render_mermaid`, so the
/// two sides of the seam cannot be confused at a call site.
///
/// # Errors
///
/// Propagates the [`MermaidError`] so the caller can degrade gracefully.
pub(crate) fn mermaid(src: &str, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    crate::mermaid::render_mermaid(src, width, theme)
}
