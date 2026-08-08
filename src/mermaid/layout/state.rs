//! `stateDiagram-v2` layout (design spec §6.7).
//!
//! Placeholder owned by the state-diagram workstream. The rounded state box
//! is a [`NodeArt`](super::graph::NodeArt); the start and end markers map onto
//! [`Terminator`](super::graph::Terminator) and are routed by [`graph::draw`].
//!
//! [`graph::draw`]: super::graph::draw

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::StateDiagram;
use crate::theme::Theme;

/// Draws a state diagram into a canvas exactly `width` columns wide.
///
/// # Errors
///
/// Returns [`MermaidError::Unsupported`] until the family is implemented.
pub fn draw(diagram: &StateDiagram, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    let _ = (diagram, width, theme);
    Err(MermaidError::Unsupported {
        line: 0,
        message: "stateDiagram-v2 rendering is not implemented yet".to_string(),
    })
}
