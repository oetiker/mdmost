//! `erDiagram` layout (design spec §6.4).
//!
//! Placeholder owned by the entity-relationship workstream. The entity attribute table
//! is a [`NodeArt`](super::graph::NodeArt); crow's-foot cardinalities map onto
//! [`Terminator`](super::graph::Terminator) and are routed by [`graph::draw`].
//!
//! [`graph::draw`]: super::graph::draw

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::ErDiagram;
use crate::theme::Theme;

/// Draws an ER diagram into a canvas exactly `width` columns wide.
///
/// # Errors
///
/// Returns [`MermaidError::Unsupported`] until the family is implemented.
pub fn draw(diagram: &ErDiagram, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    let _ = (diagram, width, theme);
    Err(MermaidError::Unsupported {
        line: 0,
        message: "erDiagram rendering is not implemented yet".to_string(),
    })
}
