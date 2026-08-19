// SPDX-License-Identifier: MIT
//! One formula, on one row.
//!
//! Design spec §5. Inline math must not give a paragraph line a variable height, which
//! is the constraint the whole renderer's reflow rests on — so this walks the event
//! stream and writes text, and any construct that genuinely needs a second row is
//! reported as [`MathError::NotInline`] rather than being approximated into nonsense.

use pulldown_latex::event::{Content, Event};
use pulldown_latex::{Parser, Storage};

use crate::error::MathError;

/// Draws `src` as a single row of text.
///
/// # Errors
///
/// [`MathError::Parse`] if the LaTeX does not parse, [`MathError::NotInline`] if it
/// parses but cannot be written on one row.
pub fn render_inline(src: &str) -> Result<String, MathError> {
    let storage = Storage::new();
    let parser = Parser::new(src, &storage);
    let mut events = Vec::new();
    for event in parser {
        events.push(event.map_err(|err| {
            MathError::Parse {
                // Only the first line. `ParserError`'s `Display` is four lines: the message,
                // then a `╭─►` context box quoting the input. That box is unreadable in a
                // one-row caption and would put box drawing the manual does not claim into
                // `tests/glyph_inventory.rs`.
                message: err
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            }
        })?);
    }
    write_events(&events, Spacing::Normal)
}

/// Whether the walk writes the spaces of the spacing rule above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spacing {
    /// Spaces are written: `1 + 2`, `E = mc²`, `∑ᵢ₌₁ⁿ i`.
    Normal,
    /// Spaces are not written, because the caller is about to raise or lower this text
    /// and the Unicode script tables of §5.1 have no space — one there would make the
    /// all-or-nothing rule decline the whole group, turning `xⁿ⁺¹` into `x^{n + 1}`.
    ///
    /// Only script operands suppress. A fraction or radical operand is written at full
    /// size and keeps its spaces.
    Suppressed,
}

/// Writes an already-collected event stream.
///
/// Split from [`render_inline`] so that later tasks can test the walk without going
/// through the parser, and so the error path above stays one statement.
fn write_events(events: &[Event<'_>], spacing: Spacing) -> Result<String, MathError> {
    let mut out = String::new();
    for event in events {
        match event {
            Event::Content(content) => write_content(content, &mut out, spacing),
            // A group boundary draws nothing of itself. What a group *does* — a
            // fraction, a root, a matrix — arrives as `Visual` or as an environment,
            // and those are Task 5's.
            Event::Begin(_) | Event::End => {}
            // Only a *horizontal* spacing command draws a column. `\mathstrut` and
            // `\strut` are `Space { width: None, height: Some(…) }` — vertical struts
            // that occupy no columns at all.
            Event::Space { width, .. } if width.is_some() => out.push(' '),
            Event::Space { .. } | Event::StateChange(_) => {}
            _ => return Err(MathError::NotInline("this construct")),
        }
    }
    // A formula ending in an operator would otherwise keep the space written after it.
    Ok(out.trim_end().to_string())
}

/// Writes one piece of content.
///
/// Symbols arrive from `pulldown-latex` already resolved to a `char`. There is
/// deliberately no symbol table in this crate: a second table would drift from the one
/// upstream maintains, and drift here means a formula that renders differently here
/// than everywhere else.
fn write_content(content: &Content<'_>, out: &mut String, spacing: Spacing) {
    match content {
        Content::Text(text) | Content::Number(text) | Content::Function(text) => {
            out.push_str(text);
        }
        Content::BinaryOp { content, .. } => {
            spaced(out, content.encode_utf8(&mut [0u8; 4]), spacing);
        }
        // A relation is the one content that is not a `char`: `RelationContent` may hold
        // two characters (`\shortparallel` and friends), and its only public accessor
        // writes them into a caller's buffer, which the upstream doc comment requires be
        // at least eight bytes. It therefore cannot share an or-pattern with the arm
        // below, whose alternatives all bind a `char`.
        Content::Relation { content, .. } => {
            let mut buf = [0u8; 8];
            let text =
                std::str::from_utf8(content.encode_utf8_to_buf(&mut buf)).unwrap_or_default();
            spaced(out, text, spacing);
        }
        Content::Ordinary { content, .. }
        | Content::LargeOp { content, .. }
        | Content::Delimiter { content, .. } => out.push(*content),
        Content::Punctuation(ch) => out.push(*ch),
    }
}

/// Writes `text` with one space either side, unless spacing is suppressed.
///
/// Never a leading space at the head of a run and never two in a row, so a unary `−x`
/// reads `−x` while `a − b` reads `a − b`. `pulldown-latex` does not mark a leading `-`
/// as unary the way TeX's own bin/ord reclassification would — it is `Content::BinaryOp`
/// exactly like the one in `a-b` — so the head-of-run position (`out` still empty) is
/// the only signal this walk has, and it is sufficient: nothing upstream of the first
/// token can make this use binary. When that is where we are, the operator is glued to
/// what follows with no space on either side, the same as `Spacing::Suppressed`.
fn spaced(out: &mut String, text: &str, spacing: Spacing) {
    if spacing == Spacing::Suppressed || out.is_empty() {
        out.push_str(text);
        return;
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out.push_str(text);
    out.push(' ');
}
