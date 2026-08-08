//! Shims for the two renderer collaborators that are being built in parallel.
//!
//! The block renderer depends on two functions owned by other workstreams:
//!
//! * `crate::highlight::highlight(lang, src, &Theme) -> Vec<Line>` — landed, and
//!   called directly below;
//! * `crate::mermaid::render_mermaid(src, width, &Theme) -> Result<Canvas, MermaidError>`
//!   — not yet available, so a stand-in with exactly that signature stands in for it.
//!
//! Every call site in `render` goes through here, so integration is a one-line change
//! per function, with no other file touched.
//!
//! The Mermaid stub deliberately returns an error rather than a placeholder canvas:
//! the graceful-degradation path (design spec §6) is owned by this workstream, so it
//! must be live code exercised by the tests rather than a branch nobody runs.

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
/// TODO(integration): replace the body with
/// `crate::mermaid::render_mermaid(src, width, theme)` once that module lands.
pub(crate) fn render_mermaid(
    src: &str,
    _width: u16,
    _theme: &Theme,
) -> Result<Canvas, MermaidError> {
    let family = src
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("")
        .to_string();
    Err(MermaidError::UnsupportedFamily(family))
}
