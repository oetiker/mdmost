//! Turning a parsed table into clipboard payloads.
//!
//! A pure function from AST to string: this module depends on [`crate::doc`] and nothing
//! else — not on `canvas`, not on `theme`, and above all not on `tui`. That is what makes
//! it the easiest thing here to test exhaustively, and it is why the renderer can build a
//! payload at render time without dragging the pager in with it.
//!
//! Two flavours, and they are not equal partners. **TSV is what makes Excel and Google
//! Sheets split a paste into cells** — not HTML — and it is the only thing OSC 52 can
//! carry, so it is what every reader receives. The HTML in [`html`] is an upgrade
//! offered to a local clipboard on top of it.

mod html;
mod tsv;

#[cfg(test)]
mod tests;

pub use html::table_html;
pub use tsv::table_tsv;
