//! Mermaid diagrams rendered as Unicode box art.
//!
//! Placeholder. Parsing produces a typed diagram; layout turns it into a
//! [`Canvas`](crate::canvas::Canvas). Any unsupported construct must fail gracefully
//! with a [`MermaidError`](crate::error::MermaidError), which the block renderer turns
//! into a captioned code block.

pub mod layout;
pub mod parse;
