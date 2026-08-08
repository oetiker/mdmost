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
    render_diagram(&parse::parse(src)?, width, theme)
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
    match diagram {
        ast::Diagram::Sequence(d) => sequence::draw(d, width, theme),
        ast::Diagram::Pie(d) => pie::draw(d, width, theme),
        ast::Diagram::Gantt(d) => gantt::draw(d, width, theme),
        // TODO(dispatch): the graph-engine families are wired up as they land.
        ast::Diagram::Flowchart(_) => Err(unsupported("flowchart")),
        ast::Diagram::Class(_) => Err(unsupported("classDiagram")),
        ast::Diagram::Er(_) => Err(unsupported("erDiagram")),
        ast::Diagram::State(_) => Err(unsupported("stateDiagram")),
    }
}

/// The error reported for a family whose renderer has not landed yet.
fn unsupported(family: &str) -> MermaidError {
    MermaidError::UnsupportedFamily(family.to_string())
}
