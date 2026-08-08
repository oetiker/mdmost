//! The `mdless` binary.
//!
//! This is the CLI edge of the program: argument parsing, stdin handling and the
//! `--render-once` dump mode live here, and this is the only place `anyhow` is used.
//!
//! The command line is not implemented yet; the entry point currently reports the
//! foundation's state so the crate has a runnable binary target.

use anyhow::Result;

fn main() -> Result<()> {
    let theme = mdless::Theme::default_dark();
    eprintln!(
        "mdless {} — foundation only; the pager is not wired up yet (theme: {})",
        env!("CARGO_PKG_VERSION"),
        theme.name
    );
    Ok(())
}
