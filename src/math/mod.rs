// SPDX-License-Identifier: MIT
//! LaTeX math, laid out for a terminal.
//!
//! Design spec `docs/superpowers/specs/2026-08-19-math-design.md`. A sibling of
//! `crate::mermaid`: source text in, drawn output out, and no knowledge of `render` or
//! `tui`.
//!
//! Stage 1 draws inline math only. Display math is laid out in two dimensions by a
//! later stage; until then `render::document` shows it as its own source.

mod boxes;
mod build;
mod draw;
mod inline;
pub(crate) mod scripts;
mod spacing;

#[cfg(test)]
mod tests;

pub use inline::{render_inline, symbols};
