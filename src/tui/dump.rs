// SPDX-License-Identifier: MIT
//! Writing a [`Canvas`] to a stream, for `--render-once`.
//!
//! Two flavours, chosen by the caller from whether the destination is a terminal
//! (design spec §11): ANSI truecolour when it is, plain text when it is not, so
//! `mdmost x.md | cat` yields text rather than escape soup. Both are deterministic
//! functions of the canvas, which is what lets the snapshot tests drive the real
//! binary headlessly.

use std::io::{self, Write};

use crate::canvas::Canvas;
use crate::theme::{Attributes, Color, Style};

/// Writes the canvas as plain text, one row per line, without trailing blanks.
///
/// `narrow` is for a terminal that draws an emoji-presentation sequence in one column;
/// see [`row`].
pub fn write_plain(out: &mut impl Write, canvas: &Canvas, narrow: bool) -> io::Result<()> {
    for cells in canvas.rows() {
        writeln!(out, "{}", row(cells, narrow).trim_end())?;
    }
    Ok(())
}

/// One row's text, with the columns it occupies preserved.
///
/// A continuation cell contributes nothing, because the wide glyph before it already
/// fills that column — unless this terminal draws that glyph in one column, in which
/// case the glyph is written without its selector and the column it no longer fills is
/// written as the space it now is. See `crate::text::presentation_base`.
fn row(cells: &[crate::canvas::Cell], narrow: bool) -> String {
    let mut out = String::new();
    let mut disowned = false;
    for cell in cells {
        if cell.is_continuation() {
            if disowned {
                out.push(' ');
            }
            disowned = false;
            continue;
        }
        match crate::text::presentation_base(cell.text()).filter(|_| narrow) {
            Some(base) => {
                out.push_str(base);
                disowned = true;
            }
            None => {
                out.push_str(cell.text());
                disowned = false;
            }
        }
    }
    out
}

/// Writes the canvas with ANSI truecolour escapes.
///
/// Every row ends with a reset, so a truncated dump cannot leave the terminal in a
/// coloured state. `narrow` is for a terminal that draws an emoji-presentation
/// sequence in one column; see [`row`].
pub fn write_ansi(
    out: &mut impl Write,
    canvas: &Canvas,
    base: Style,
    narrow: bool,
) -> io::Result<()> {
    for cells in canvas.rows() {
        let mut current = Style::NONE;
        let mut pending_reset = false;
        let mut disowned = false;
        for cell in cells {
            if cell.is_continuation() {
                if disowned {
                    out.write_all(b" ")?;
                }
                disowned = false;
                continue;
            }
            let style = base.patch(cell.style());
            if style != current {
                out.write_all(sgr(style).as_bytes())?;
                current = style;
                pending_reset = true;
            }
            match crate::text::presentation_base(cell.text()).filter(|_| narrow) {
                Some(text) => {
                    out.write_all(text.as_bytes())?;
                    disowned = true;
                }
                None => {
                    out.write_all(cell.text().as_bytes())?;
                    disowned = false;
                }
            }
        }
        if pending_reset {
            out.write_all(b"\x1b[0m")?;
        }
        out.write_all(b"\n")?;
    }
    Ok(())
}

/// The full SGR sequence for a style, reset first so styles never accumulate.
fn sgr(style: Style) -> String {
    let mut out = String::from("\x1b[0");
    let attrs = style.attrs;
    for (attribute, code) in [
        (Attributes::BOLD, "1"),
        (Attributes::DIM, "2"),
        (Attributes::ITALIC, "3"),
        (Attributes::UNDERLINE, "4"),
        (Attributes::REVERSE, "7"),
        (Attributes::STRIKETHROUGH, "9"),
    ] {
        if attrs.contains(attribute) {
            out.push(';');
            out.push_str(code);
        }
    }
    if let Some(fg) = style.fg {
        out.push_str(&color(38, fg));
    }
    if let Some(bg) = style.bg {
        out.push_str(&color(48, bg));
    }
    out.push('m');
    out
}

/// One truecolour SGR parameter group.
fn color(lead: u8, color: Color) -> String {
    format!(";{lead};2;{};{};{}", color.r, color.g, color.b)
}
