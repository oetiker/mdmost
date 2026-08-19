// SPDX-License-Identifier: MIT
//! LaTeX math, laid out for a terminal.
//!
//! Design spec `docs/superpowers/specs/2026-08-19-math-design.md`. A sibling of
//! `crate::mermaid`: source text in, drawn output out, and no knowledge of `render` or
//! `tui`.

pub(crate) mod scripts;
