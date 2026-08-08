//! `classDiagram` layout (design spec §6.3).
//!
//! Placeholder owned by the class-diagram workstream. The three-compartment class box
//! is a [`NodeArt`](super::graph::NodeArt); the relations map onto
//! [`Terminator`](super::graph::Terminator) and are routed by [`graph::draw`].
//!
//! [`graph::draw`]: super::graph::draw

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::ClassDiagram;
use crate::theme::Theme;

/// Draws a class diagram into a canvas exactly `width` columns wide.
///
/// # Errors
///
/// Returns [`MermaidError::Unsupported`] until the family is implemented.
pub fn draw(diagram: &ClassDiagram, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    let _ = (diagram, width, theme);
    Err(MermaidError::Unsupported {
        line: 0,
        message: "classDiagram rendering is not implemented yet".to_string(),
    })
}
