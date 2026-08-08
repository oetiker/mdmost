//! Fenced and indented code blocks, and the Mermaid fences routed out of them.
//!
//! Code never wraps (design spec §8): a line wider than the frame is clipped and the
//! last column carries an overflow marker, the same way an over-wide table is clipped.
//! A ```` ```mermaid ```` fence goes to the diagram renderer instead; when that fails
//! the block degrades to a syntax-highlighted code block with a dim caption naming the
//! reason (design spec §6).
//!
//! With [`RenderOptions::line_numbers`](super::RenderOptions::line_numbers) on, a
//! themed gutter is drawn to the left of the code. The gutter is *outside* the
//! clipped region: it is written at a fixed position and the code area shrinks by its
//! width, so scrolling a long line horizontally never scrolls the numbers away.

use crate::canvas::{BorderSet, Canvas};
use crate::text::{Align, Line, Span, display_width};

use super::{Ctx, bridge};

/// The info-string language that routes a fence to the Mermaid renderer.
const MERMAID: &str = "mermaid";

/// The marker shown at the right edge of a clipped code line.
pub(crate) const OVERFLOW_MARKER: &str = "›";

/// The glyph separating the line-number gutter from the code.
const GUTTER_RULE: &str = "│";

/// Renders a code block, routing Mermaid fences to the diagram renderer.
pub(crate) fn render_code_block(
    language: Option<&str>,
    literal: &str,
    fenced: bool,
    width: u16,
    ctx: Ctx<'_>,
) -> Canvas {
    if language == Some(MERMAID) {
        return match bridge::render_mermaid(literal, width, ctx.theme) {
            Ok(mut canvas) => {
                canvas.resize_width(width, ctx.base);
                canvas
            }
            Err(error) => {
                let mut out = framed_code(language, literal, fenced, width, ctx);
                out.append(&caption(&error.reason(), width, ctx), ctx.base);
                out
            }
        };
    }
    framed_code(language, literal, fenced, width, ctx)
}

/// Draws the framed, highlighted code block.
fn framed_code(
    language: Option<&str>,
    literal: &str,
    fenced: bool,
    width: u16,
    ctx: Ctx<'_>,
) -> Canvas {
    let theme = ctx.theme;
    let lines = bridge::highlight(language, literal, theme);
    // Below four columns there is no room for a frame plus content; the code is shown
    // bare rather than as a box with nothing inside it.
    if width < 4 {
        return code_area(&lines, width, false, ctx);
    }
    let inner = code_area(&lines, width - 2, ctx.options.line_numbers, ctx);
    let title = fenced
        .then_some(language)
        .flatten()
        .map(|name| title(name, ctx));
    inner.framed(
        BorderSet::ROUNDED,
        theme.code.frame,
        title.as_ref(),
        theme.code.background,
    )
}

/// The label drawn into the frame's top edge: the language, with its icon if enabled.
fn title(language: &str, ctx: Ctx<'_>) -> Line {
    let theme = ctx.theme;
    let mut line = Line::empty();
    if let Some(icon) = ctx.glyphs.language(Some(language)) {
        line.push(Span::new(format!("{icon} "), theme.code.language));
    }
    line.push(Span::new(language, theme.code.language));
    line
}

/// Writes code lines at `width` columns, clipping rather than wrapping.
///
/// When `numbered` is set and there is room for it, a gutter of right-aligned line
/// numbers is drawn first and the code is clipped to what remains.
fn code_area(lines: &[Line], width: u16, numbered: bool, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let budget = usize::from(width);
    let digits = if numbered {
        digit_count(lines.len())
    } else {
        0
    };
    // The gutter is `NNN │ `. It is dropped entirely rather than squeezing the code
    // into nothing when the block is too narrow to carry both.
    let gutter = if digits == 0 || digits + 4 > budget {
        0
    } else {
        digits + 3
    };
    let code_width = budget - gutter;

    let mut out = Canvas::new(width, lines.len(), theme.code.background);
    for (row, line) in lines.iter().enumerate() {
        if gutter > 0 {
            let number = format!("{:>digits$} ", row + 1, digits = digits);
            out.write_str(row, 0, &number, theme.code.line_number);
            out.write_str(row, digits + 1, GUTTER_RULE, theme.code.frame);
        }
        if line.width() <= code_width {
            out.write_line(row, gutter, line, theme.code.background);
            continue;
        }
        // One column is reserved for the marker that says the line goes on.
        let head = line.truncated(code_width.saturating_sub(1));
        out.write_line(row, gutter, &head, theme.code.background);
        out.write_str(row, budget - 1, OVERFLOW_MARKER, theme.code.overflow_marker);
    }
    out
}

/// How many columns the largest line number needs.
fn digit_count(lines: usize) -> usize {
    let mut digits = 1usize;
    let mut value = lines;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

/// The dim caption drawn under a block that could not be rendered as intended.
fn caption(reason: &str, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let text = format!("unsupported mermaid syntax: {reason}");
    let mut out = Canvas::empty(width);
    for line in super::inline::wrap(&[Span::new(text, theme.block.caption)], usize::from(width)) {
        out.push_line(&line, Align::Left, ctx.base);
    }
    out
}

/// The natural width of a code block at these options, frame and gutter included.
///
/// The table column negotiator needs this so a code block in a cell asks for the
/// right amount of room.
pub(crate) fn natural_width(literal: &str, ctx: Ctx<'_>) -> usize {
    let longest = literal.lines().map(display_width).max().unwrap_or(0);
    let gutter = if ctx.options.line_numbers {
        digit_count(literal.lines().count()) + 3
    } else {
        0
    };
    longest + gutter + 2
}
