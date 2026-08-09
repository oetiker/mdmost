//! Mermaid diagrams rendered as Unicode box art.
//!
//! Parsing produces a typed [`Diagram`](ast::Diagram); layout turns it into a
//! [`Canvas`](crate::canvas::Canvas). Any unsupported construct fails gracefully with a
//! [`MermaidError`](crate::error::MermaidError), which the block renderer turns into a
//! captioned code block.

pub mod ast;
pub(crate) mod chrome;
pub mod gantt;
pub mod layout;
pub mod parse;
pub mod pie;
pub mod sequence;

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::theme::Theme;

pub use layout::graph::Fit;

/// Renders a mermaid source block into a canvas exactly `width` columns wide.
///
/// This is the single entry point the block renderer calls for a ```` ```mermaid ````
/// fence.
///
/// # Errors
///
/// Returns a [`MermaidError`] when the source is malformed or uses a construct outside
/// the supported subset. The caller is expected to fall back to a captioned code block
/// rather than treating this as fatal.
pub fn render_mermaid(src: &str, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    render_mermaid_with(src, width, theme, Fit::COMPACT)
}

/// Renders a mermaid source block under the given fit policy.
///
/// A caller that can lay a diagram out wider than the viewport and let the reader
/// scroll to it passes [`Fit::ROOMY`]; one whose only alternative is dumping the source
/// passes [`Fit::COMPACT`]. See [`Fit`].
///
/// # Errors
///
/// As [`render_mermaid`], with "cannot be drawn" meaning "not within what `fit`
/// allows".
pub fn render_mermaid_with(
    src: &str,
    width: u16,
    theme: &Theme,
    fit: Fit,
) -> Result<Canvas, MermaidError> {
    render_diagram_with(&parse::parse(src)?, width, theme, fit)
}

/// Renders an already-parsed diagram into a canvas exactly `width` columns wide.
///
/// # Errors
///
/// Returns a [`MermaidError`] when the diagram cannot be laid out within `width`.
pub fn render_diagram(
    diagram: &ast::Diagram,
    width: u16,
    theme: &Theme,
) -> Result<Canvas, MermaidError> {
    render_diagram_with(diagram, width, theme, Fit::COMPACT)
}

/// Renders an already-parsed diagram under the given fit policy.
///
/// Only the four families built on the shared graph engine have a degradation ladder to
/// choose between; `fit` is inert for sequence, pie and gantt diagrams, whose layouts
/// degrade through their own fixed profiles.
///
/// # Errors
///
/// Returns a [`MermaidError`] when the diagram cannot be laid out within `width` under
/// `fit`.
pub fn render_diagram_with(
    diagram: &ast::Diagram,
    width: u16,
    theme: &Theme,
    fit: Fit,
) -> Result<Canvas, MermaidError> {
    match diagram {
        ast::Diagram::Sequence(d) => sequence::draw(d, width, theme),
        ast::Diagram::Pie(d) => pie::draw(d, width, theme),
        ast::Diagram::Gantt(d) => gantt::draw(d, width, theme),
        ast::Diagram::Flowchart(d) => layout::flowchart::draw_with(d, width, theme, fit),
        ast::Diagram::Class(d) => layout::class::draw_with(d, width, theme, fit),
        ast::Diagram::Er(d) => layout::er::draw_with(d, width, theme, fit),
        ast::Diagram::State(d) => layout::state::draw_with(d, width, theme, fit),
    }
}
