//! `sequenceDiagram` — participants, lifelines, arrows, notes and frames
//! (design spec §6.2).
//!
//! Rendering happens in three separate steps, which is what keeps the code honest:
//!
//! 1. [`columns`] solves the *horizontal* layout, turning header widths, label widths,
//!    note widths and frame nesting into one column position per lifeline.
//! 2. [`plan`] walks the diagram body and assigns every arrow, note, activation bar
//!    and block frame the *rows* it occupies.
//! 3. [`paint`](fn@paint) draws the plan, bottom layer first: lifelines, then frames,
//!    then activation bars, then arrows, and finally notes, which sit on top of
//!    everything they cover.
//!
//! ```text
//!  ╭───────╮        ╭─────────╮
//!  │ Alice │        │ Bob     │
//!  ╰───────╯        ╰─────────╯
//!      ┆                 ┆
//!      ┆   hello         ┆
//!      ┆────────────────▶┆
//!      ┆      hi         ┆
//!      ┆◀╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┆
//! ```

mod columns;
mod plan;

use crate::canvas::{BorderSet, Canvas};
use crate::error::MermaidError;
use crate::mermaid::ast::{BlockKind, MessageHead, MessageLine, ParticipantKind, SequenceDiagram};
use crate::mermaid::chrome;
use crate::text::{Align, display_width};
use crate::theme::Theme;

use columns::{Columns, HOOK, Header};
use plan::{Arrow, Frame, NoteBox, Plan};

/// The lifeline glyph: a dashed vertical, as UML draws it.
const LIFELINE: &str = "┆";
/// The activation-bar glyph, deliberately heavier than the lifeline.
const ACTIVATION: &str = "┃";
/// A solid message shaft.
const SOLID: &str = "─";
/// A dotted message shaft.
const DOTTED: &str = "╌";
/// The arrowhead of a left-to-right message.
const HEAD_RIGHT: &str = "▶";
/// The arrowhead of a right-to-left message.
const HEAD_LEFT: &str = "◀";
/// The terminator of a `-x` / `--x` message.
const HEAD_CROSS: &str = "✗";
/// The head of an actor stick figure.
const ACTOR_HEAD: &str = "◯";
/// The body of an actor stick figure.
const ACTOR_BODY: &str = "╱│╲";
/// The text shown for a diagram with no participants at all.
const EMPTY_TEXT: &str = "(empty sequence diagram)";

/// Renders a sequence diagram into a canvas exactly `width` columns wide.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when the participants, their labels and the
/// block frames cannot be squeezed into `width` even at the tightest layout profile.
pub fn draw(diagram: &SequenceDiagram, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    if diagram.participants.is_empty() {
        let text = chrome::fit(EMPTY_TEXT, usize::from(width));
        let cols = u16::try_from(display_width(&text)).unwrap_or(0);
        let mut body = Canvas::new(cols, 0, theme.base());
        body.push_text(&text, Align::Left, theme.text.dim);
        return chrome::compose(diagram.title.as_deref(), &body, width, theme);
    }
    let columns = columns::solve(diagram, width)?;
    let plan = plan::build(diagram, &columns);
    let body = paint(&columns, &plan, theme);
    chrome::compose(diagram.title.as_deref(), &body, width, theme)
}

/// Draws a solved layout and its plan onto a canvas of the content width.
fn paint(columns: &Columns, plan: &Plan, theme: &Theme) -> Canvas {
    let head_rows = columns.header_height;
    // One blank row of breathing space above and below the body.
    let body_top = head_rows + 1;
    let footer_top = body_top + plan.rows + 1;
    let height = footer_top + head_rows;
    let content = u16::try_from(columns.width).unwrap_or(u16::MAX);
    let mut canvas = Canvas::new(content, height, theme.base());

    for (index, header) in columns.headers.iter().enumerate() {
        let center = columns.centers[index];
        draw_head(&mut canvas, 0, head_rows, center, header, theme);
        draw_head(&mut canvas, footer_top, head_rows, center, header, theme);
        // The lifeline runs from just under the head to just above the footer.
        canvas.vline(
            head_rows,
            center,
            footer_top - head_rows,
            LIFELINE,
            theme.diagram.lifeline,
        );
    }

    for frame in &plan.frames {
        draw_frame(&mut canvas, columns, frame, body_top, theme);
    }
    for bar in &plan.bars {
        let column = columns.centers[bar.participant] + bar.depth;
        canvas.vline(
            body_top + bar.top,
            column,
            bar.bottom - bar.top + 1,
            ACTIVATION,
            theme.diagram.activation,
        );
    }
    for arrow in &plan.arrows {
        draw_arrow(&mut canvas, columns, arrow, body_top, theme);
    }
    for note in &plan.notes {
        draw_note(&mut canvas, note, body_top, theme);
    }
    canvas
}

/// Draws one participant head, bottom-aligned inside `rows` rows starting at `top`.
fn draw_head(
    canvas: &mut Canvas,
    top: usize,
    rows: usize,
    center: usize,
    header: &Header,
    theme: &Theme,
) {
    let start = top + rows.saturating_sub(header.height());
    let left = center.saturating_sub((header.width - 1) / 2);
    match header.kind {
        ParticipantKind::Participant => {
            let inner = header.width - 2;
            let mut label = Canvas::new(
                u16::try_from(inner).unwrap_or(0),
                header.lines.len(),
                theme.base(),
            );
            for (row, line) in header.lines.iter().enumerate() {
                label.write_field(row, 0, inner, line, Align::Center, theme.diagram.node_text);
            }
            let boxed = label.framed(
                BorderSet::ROUNDED,
                theme.diagram.node_border,
                None,
                theme.base(),
            );
            canvas.blit(start, left, &boxed, theme.base());
        }
        ParticipantKind::Actor => {
            canvas.write_field(
                start,
                left,
                header.width,
                ACTOR_HEAD,
                Align::Center,
                theme.diagram.node_border,
            );
            canvas.write_field(
                start + 1,
                left,
                header.width,
                ACTOR_BODY,
                Align::Center,
                theme.diagram.node_border,
            );
            for (row, line) in header.lines.iter().enumerate() {
                canvas.write_field(
                    start + 2 + row,
                    left,
                    header.width,
                    line,
                    Align::Center,
                    theme.diagram.node_text,
                );
            }
        }
    }
}

/// Draws one message arrow, self-messages included.
fn draw_arrow(canvas: &mut Canvas, columns: &Columns, arrow: &Arrow, top: usize, theme: &Theme) {
    let shaft = match arrow.line {
        MessageLine::Solid => SOLID,
        MessageLine::Dotted => DOTTED,
    };
    let line_style = theme.diagram.line;
    let from = columns.centers[arrow.from];
    let to = columns.centers[arrow.to];

    if arrow.from == arrow.to {
        let row = top + arrow.row;
        let corner = from + HOOK;
        canvas.fill(row, from + 1, HOOK - 1, shaft, line_style);
        canvas.write_str(row, corner, "╮", line_style);
        canvas.write_str(row + 1, corner, "│", line_style);
        canvas.fill(row + 2, from + 2, HOOK - 2, shaft, line_style);
        canvas.write_str(row + 2, corner, "╯", line_style);
        canvas.write_str(
            row + 2,
            from + 1,
            head_glyph(arrow, false),
            theme.diagram.arrow,
        );
        if !arrow.label.is_empty() {
            let at = corner + 2;
            let room = columns.width.saturating_sub(at);
            canvas.write_str(
                row + 1,
                at,
                &chrome::fit(&arrow.label, room),
                theme.diagram.edge_label,
            );
        }
        return;
    }

    let row = top + arrow.row;
    let (low, high) = (from.min(to), from.max(to));
    let rightwards = to > from;
    canvas.fill(row, low + 1, high - low - 1, shaft, line_style);
    let tip = if rightwards { high } else { low };
    canvas.write_str(row, tip, head_glyph(arrow, rightwards), theme.diagram.arrow);

    if !arrow.label.is_empty() {
        let room = high - low - 1;
        let text = chrome::fit(&arrow.label, room);
        let left = low + 1 + (room - display_width(&text)) / 2;
        canvas.write_str(row - 1, left, &text, theme.diagram.edge_label);
    }
}

/// The glyph drawn where a message meets its receiver.
fn head_glyph(arrow: &Arrow, rightwards: bool) -> &'static str {
    match arrow.head {
        MessageHead::Arrow => {
            if rightwards {
                HEAD_RIGHT
            } else {
                HEAD_LEFT
            }
        }
        MessageHead::Cross => HEAD_CROSS,
        MessageHead::None => match arrow.line {
            MessageLine::Solid => SOLID,
            MessageLine::Dotted => DOTTED,
        },
    }
}

/// Draws a `loop` / `alt` / `opt` / `par` / `critical` frame and its branch dividers.
fn draw_frame(canvas: &mut Canvas, columns: &Columns, frame: &Frame, top: usize, theme: &Theme) {
    let border = BorderSet::ROUNDED;
    let style = theme.diagram.group_border;
    let left = columns.frame_left(frame.depth);
    let right = columns.frame_right(frame.depth);
    if right <= left + 1 {
        return;
    }
    let inner = right - left - 1;
    let first = top + frame.top;
    let last = top + frame.bottom;

    canvas.write_str(first, left, &border.top_left.to_string(), style);
    canvas.hline(
        first,
        left + 1,
        inner,
        &border.horizontal.to_string(),
        style,
    );
    canvas.write_str(first, right, &border.top_right.to_string(), style);
    canvas.write_str(last, left, &border.bottom_left.to_string(), style);
    canvas.hline(last, left + 1, inner, &border.horizontal.to_string(), style);
    canvas.write_str(last, right, &border.bottom_right.to_string(), style);
    for row in first + 1..last {
        canvas.write_str(row, left, &border.vertical.to_string(), style);
        canvas.write_str(row, right, &border.vertical.to_string(), style);
    }

    for (index, branch) in frame.branches.iter().enumerate() {
        let row = top + branch.row;
        if index > 0 {
            canvas.write_str(row, left, &border.tee_right.to_string(), style);
            canvas.hline(row, left + 1, inner, DOTTED, style);
            canvas.write_str(row, right, &border.tee_left.to_string(), style);
        }
        let caption = caption(frame.kind, index, branch.label.as_deref());
        canvas.write_str(
            row,
            left + 2,
            &chrome::fit(&caption, inner.saturating_sub(2)),
            theme.diagram.group_title,
        );
    }
}

/// The caption written into a frame's top edge or a branch divider.
fn caption(kind: BlockKind, index: usize, label: Option<&str>) -> String {
    let keyword = match (kind, index) {
        (BlockKind::Loop, _) => "loop",
        (BlockKind::Opt, _) => "opt",
        (BlockKind::Alt, 0) => "alt",
        (BlockKind::Alt, _) => "else",
        (BlockKind::Par, 0) => "par",
        (BlockKind::Par, _) => "and",
        (BlockKind::Critical, 0) => "critical",
        (BlockKind::Critical, _) => "option",
    };
    match label.map(str::trim).filter(|text| !text.is_empty()) {
        Some(label) => format!(" {keyword} [{label}] "),
        None => format!(" {keyword} "),
    }
}

/// Draws a note box over whatever it covers.
fn draw_note(canvas: &mut Canvas, note: &NoteBox, top: usize, theme: &Theme) {
    let inner = note.width.saturating_sub(2);
    let mut text = Canvas::new(
        u16::try_from(inner).unwrap_or(0),
        note.lines.len(),
        theme.diagram.note,
    );
    for (row, line) in note.lines.iter().enumerate() {
        text.write_field(
            row,
            1,
            inner.saturating_sub(2),
            line,
            Align::Left,
            theme.diagram.note,
        );
    }
    let boxed = text.framed(
        BorderSet::ROUNDED,
        theme.diagram.note,
        None,
        theme.diagram.note,
    );
    canvas.blit(top + note.top, note.left, &boxed, theme.base());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::grapheme_width;

    #[test]
    fn every_sequence_glyph_is_one_column_wide() {
        for glyph in [
            LIFELINE, ACTIVATION, SOLID, DOTTED, HEAD_RIGHT, HEAD_LEFT, HEAD_CROSS, ACTOR_HEAD,
        ] {
            assert_eq!(grapheme_width(glyph), 1, "{glyph:?} must be one column");
        }
        assert_eq!(display_width(ACTOR_BODY), 3);
    }

    #[test]
    fn captions_name_their_keyword() {
        assert_eq!(caption(BlockKind::Loop, 0, Some("twice")), " loop [twice] ");
        assert_eq!(caption(BlockKind::Alt, 1, Some("no")), " else [no] ");
        assert_eq!(caption(BlockKind::Opt, 0, None), " opt ");
        assert_eq!(caption(BlockKind::Par, 1, None), " and ");
        assert_eq!(caption(BlockKind::Critical, 1, Some("x")), " option [x] ");
    }
}
