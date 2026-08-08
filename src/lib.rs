//! `mdless` — a full-screen terminal pager for a single Markdown document.
//!
//! # Architecture
//!
//! The central rule of the design (spec §3) is:
//!
//! > Rendering is a pure function of `(AST, width, theme)`.
//!
//! The document is parsed once into [`doc::Doc`]. Nothing about layout is decided at
//! parse time. Every renderer receives a *width budget* and returns a
//! [`canvas::Canvas`], which is the only type shared between modules. A resize simply
//! throws the canvas away and renders again.
//!
//! # Module map
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`doc`] | Owned AST, heading ids, source offsets, skipped HTML |
//! | [`text`] | Grapheme-safe, width-aware text primitives and wrapping |
//! | [`canvas`] | The `Canvas` contract: cells, composition, framing |
//! | [`theme`] | Palette and semantic style lookup, built-in themes |
//! | [`render`] | Inline, block and table renderers |
//! | [`highlight`] | Fenced code highlighting |
//! | [`mermaid`] | Mermaid parsing and Unicode-art layout |
//! | [`toc`] | Heading tree and current-position tracking |
//! | [`search`] | Source-text search mapped onto canvas positions |
//! | [`config`] | TOML configuration, themes, key bindings |
//! | [`tui`] | The ratatui application |
//!
//! # Foundation vs. work in progress
//!
//! [`doc`], [`text`], [`canvas`], [`theme`] and [`error`] are complete. The remaining
//! modules are placeholders owned by other workstreams; they exist so the module map
//! is visible and so their interfaces can be filled in without moving files.

#![warn(missing_docs)]
#![warn(clippy::doc_markdown)]
#![forbid(unsafe_code)]

pub mod canvas;
pub mod config;
pub mod doc;
pub mod error;
pub mod highlight;
pub mod mermaid;
pub mod render;
pub mod search;
pub mod text;
pub mod theme;
pub mod toc;
pub mod tui;

pub use canvas::{Anchor, Canvas, Cell, SearchSpan};
pub use doc::Doc;
pub use error::{Error, Result};
pub use text::{Line, Span};
pub use theme::{Style, Theme};
