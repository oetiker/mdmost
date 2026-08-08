//! Renderers: AST plus width budget plus theme in, [`Canvas`](crate::canvas::Canvas) out.
//!
//! Rendering is recursive over a *width budget*. A renderer is handed the number of
//! columns it may occupy and must return a canvas exactly that wide. A table cell is a
//! nested document rendered at its column's budget — that is what makes Markdown
//! inside table cells work without special-casing.
//!
//! # Status
//!
//! This module and its children are placeholders. The interfaces named in design spec
//! §5 are:
//!
//! * `inline::wrap(&[Span], width) -> Vec<Line>` — delegate to
//!   [`crate::text::wrap_spans`], which is the single wrapping implementation.
//! * `block::render_block(&Node, width, &Theme) -> Canvas`
//! * `table::render_table(&Node, width, &Theme) -> Canvas`

pub mod block;
pub mod inline;
pub mod table;
