//! Fenced and indented code blocks, and the Mermaid fences routed out of them.
//!
//! Code never wraps (design spec §8): a line wider than the frame is clipped and the
//! last column carries an overflow marker, the same way an over-wide table is clipped.
//! A ```` ```mermaid ```` fence goes to the diagram renderer instead; when that fails
//! the block degrades to a syntax-highlighted code block with a dim caption naming the
//! reason (design spec §6).

use crate::canvas::{BorderSet, Canvas};
use crate::text::{Align, Line, Span};

use super::{Ctx, bridge};

/// The info-string language that routes a fence to the Mermaid renderer.
const MERMAID: &str = "mermaid";

/// The marker shown at the right edge of a clipped code line.
pub(crate) const OVERFLOW_MARKER: &str = "›";

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
        return clipped(&lines, width, ctx);
    }
    let inner = clipped(&lines, width - 2, ctx);
    let title = fenced
        .then_some(language)
        .flatten()
        .map(|name| Line::styled(name, theme.code.language));
    inner.framed(
        BorderSet::ROUNDED,
        theme.code.frame,
        title.as_ref(),
        theme.code.background,
    )
}

/// Writes code lines at `width` columns, clipping rather than wrapping.
fn clipped(lines: &[Line], width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let budget = usize::from(width);
    let mut out = Canvas::new(width, lines.len(), theme.code.background);
    for (row, line) in lines.iter().enumerate() {
        if line.width() <= budget {
            out.write_line(row, 0, line, theme.code.background);
            continue;
        }
        // One column is reserved for the marker that says the line goes on.
        let head = line.truncated(budget.saturating_sub(1));
        out.write_line(row, 0, &head, theme.code.background);
        out.write_str(row, budget - 1, OVERFLOW_MARKER, theme.code.overflow_marker);
    }
    out
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
