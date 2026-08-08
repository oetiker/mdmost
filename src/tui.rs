//! The ratatui application: panes, scrolling, key dispatch, help overlay, status bar.
//!
//! This is the only module that depends on `ratatui` and `crossterm`; it converts
//! [`crate::theme::Style`] into the TUI crate's own style type at its edge, and
//! `crossterm` key events into [`crate::config::Key`] at the other.
//!
//! # Shape
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`app`] | The state machine — no terminal, fully unit-testable |
//! | [`cache`] | The `(document, width, theme)` render cache |
//! | [`draw`] | Painting the document, the scrollbar and the search highlights |
//! | [`chrome`] | Table-of-contents pane, status bar, help overlay |
//! | [`help`] | Help content generated from the live key table |
//! | [`icons`] | Nerd Font glyphs and their plain-Unicode fallbacks |
//! | [`dump`] | `--render-once` output, ANSI or plain |
//! | `term` | Terminal lifecycle, signal safety and the event loop |
//!
//! The split exists because design spec §13 requires application state to be testable
//! without a terminal: [`app::App`] never touches one.

pub mod app;
pub mod cache;
pub mod chrome;
pub mod draw;
pub mod dump;
pub mod help;
pub mod icons;
mod term;

#[cfg(test)]
mod tests;

pub use app::{App, AppOptions, Focus, Overlay, PromptKind};

/// Runs the pager to completion.
///
/// The terminal is restored on every exit path, including panics and `SIGTERM`.
///
/// # Errors
///
/// Returns any I/O failure raised by the terminal.
pub fn run(app: &mut App) -> std::io::Result<()> {
    term::run(app)
}

/// Restores the terminal, for callers that need to bail out mid-flight.
pub fn restore_terminal() {
    term::restore();
}

/// The terminal's width in columns, if there is a terminal to ask.
///
/// Exists so that the binary need not depend on `crossterm` itself: design spec §5
/// makes this module the only one that may.
pub fn terminal_width() -> Option<u16> {
    crossterm::terminal::size().ok().map(|(columns, _)| columns)
}
