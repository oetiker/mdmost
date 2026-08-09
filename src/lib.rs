//! `mdless` — a full-screen terminal pager for a single Markdown document.
//!
//! # Architecture
//!
//! The central rule of the design (spec §3) is:
//!
//! > Rendering is a pure function of `(AST, width, theme, options)`.
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
//! | [`numbering`] | Section numbers for a deeply nested document |
//! | [`toc`] | Heading tree and current-position tracking |
//! | [`search`] | Source-text search mapped onto canvas positions |
//! | [`config`] | TOML configuration, themes, key bindings |
//! | [`nerdfont`] | Whether this terminal can draw Nerd Font glyphs |
//! | [`tui`] | The ratatui application |
//!
//! # The shared layer
//!
//! [`text`] and [`canvas`] are the single home for grapheme-safe width arithmetic and
//! for composition. No other module may re-implement wrapping, truncation, padding or
//! box drawing: if a renderer needs such an operation and it is missing, the operation
//! belongs in the shared layer rather than in the caller. Duplication between the table
//! renderer, the code renderer and the diagram renderers is treated as a defect (spec
//! §14), and in practice every instance of it has turned out to be a missing shared
//! operation rather than a careless caller.

#![warn(missing_docs)]
#![warn(clippy::doc_markdown)]
#![forbid(unsafe_code)]

pub mod canvas;
pub mod config;
pub mod doc;
pub mod error;
pub mod highlight;
pub mod mermaid;
pub mod nerdfont;
pub mod numbering;
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
