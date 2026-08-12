//! Chrome shared by the three purpose-built Mermaid chart renderers.
//!
//! [`sequence`](crate::mermaid::sequence), [`pie`](crate::mermaid::pie) and
//! [`gantt`](crate::mermaid::gantt) do not use the graph layout engine, but they do
//! share the furniture around their plots: a centred title, horizontal placement of a
//! finished plot inside the width budget, sub-cell bar fills and label fitting. That
//! furniture lives here so none of it is written twice (design spec §14).

use crate::canvas::{Canvas, SearchSpan};
use crate::error::MermaidError;
use crate::mermaid::ast::Label;
use crate::text::{Align, display_width, ellipsize, wrap_plain};
use crate::theme::Theme;

/// Left-growing block elements, indexed by how many eighths of a cell are filled.
///
/// Index `0` is the empty string, index `8` is a full block. This is what gives pie
/// bars their sub-cell precision (design spec §6.5). Gantt bars stay on whole cells:
/// a bar there is a span of dates, and half a cell of one is not a fact the chart
/// knows. (It used to be said they did so to leave room for a fill texture; the
/// textures went when state moved to colour alone — see `gantt::task_glyph`.)
pub const EIGHTH_BLOCKS: [&str; 9] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

/// Composes a finished plot into a canvas exactly `width` columns wide.
///
/// The optional `title` is centred above the plot and followed by a blank row. This is
/// the last step of every `draw` function, which is what makes "every row is exactly
/// `width` columns" structurally true rather than a property each renderer has to
/// remember.
///
/// # Where the drawing sits
///
/// **Against the left edge, with the unused columns on the right.** It used to be centred
/// in the width it was handed, which set it well to the right of the prose above it —
/// every block in a document now starts at the same left margin, and a diagram is no
/// exception (see `render::document::placed`).
///
/// The title is centred over *the drawing*, not over the budget: the two are one piece of
/// art, and how a diagram composes itself is the diagram's own business. So the pair is
/// laid out in the width the content needs and the surplus is added afterwards. A title
/// wider than its plot widens that inner measure rather than being cut down to the plot,
/// which is why the two are centred against each other rather than both set flush left.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when `body` is wider than `width`; renderers
/// are expected to have degraded their layout before they get here.
pub fn compose(
    title: Option<&str>,
    body: &Canvas,
    width: u16,
    theme: &Theme,
) -> Result<Canvas, MermaidError> {
    if body.width() > width {
        return Err(MermaidError::TooNarrow {
            width,
            needed: Some(body.width()),
        });
    }
    let base = theme.base();
    let title = title.map(str::trim).filter(|text| !text.is_empty());
    // The measure the art composes itself in: wide enough for the plot and for the title,
    // and never wider than the budget, which is what the title is ellipsized against.
    let inner = title
        .map_or(0, |title| u16::try_from(display_width(title)).unwrap_or(0))
        .max(body.width())
        .min(width);
    let mut out = Canvas::empty(inner);
    if let Some(title) = title {
        out.push_text_ellipsized(title, Align::Center, theme.diagram.title);
        out.push_blank_row(base);
    }
    // `align_offset` saturates; the copy this replaced used a bare `width - body.width()`,
    // which panics whenever a body is wider than the frame it is being centred in.
    let left =
        crate::canvas::align_offset(usize::from(inner), usize::from(body.width()), Align::Center);
    let top = out.height();
    out.blit(top, left, body, base);
    out.resize_width(width, base);
    Ok(out)
}

/// Renders a horizontal bar `eighths` eighths of a cell long.
///
/// The result is never wider than `max_cells` columns. A non-zero length always
/// produces at least one visible glyph, so a tiny-but-present value never disappears.
pub fn eighth_bar(eighths: usize, max_cells: usize) -> String {
    let clamped = eighths.min(max_cells.saturating_mul(8));
    let full = clamped / 8;
    let rest = clamped % 8;
    let mut out = "█".repeat(full);
    if rest > 0 && full < max_cells {
        out.push_str(EIGHTH_BLOCKS[rest]);
    } else if clamped == 0 && eighths > 0 && max_cells > 0 {
        out.push_str(EIGHTH_BLOCKS[1]);
    }
    out
}

/// Converts a `0.0..=1.0` fraction of `cells` columns into eighths of a cell.
///
/// Values outside the range and non-finite values are clamped, so a degenerate chart
/// cannot produce a bar of nonsensical length.
pub fn eighths_of(fraction: f64, cells: usize) -> usize {
    if !fraction.is_finite() || fraction <= 0.0 {
        return 0;
    }
    let eighths = (fraction.min(1.0) * (cells as f64) * 8.0).round();
    // `round` on a clamped, finite product cannot exceed `cells * 8`.
    eighths.max(0.0) as usize
}

/// One drawn piece of a wrapped [`Label`], and where it came from inside the label.
///
/// A plain `Vec<String>` of wrapped lines answers "what do I draw" and throws away
/// "which bytes drew it", which is exactly what [`Label::spans_for`] needs. This is the
/// wrap every family goes through, so no family can lose that answer by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// The text to draw. A caller that shortens it before drawing — with
    /// [`ellipsize`](crate::text::ellipsize), say — must put the shortened text here,
    /// because a span has to name the bytes behind the cells that were really painted.
    pub text: String,
    /// Which line of [`Label::lines`] this is a piece of.
    pub index: usize,
    /// Where it starts in that line, in bytes, or `None` when it could not be located.
    pub at: Option<usize>,
}

/// Wraps a [`Label`] to `width` columns, keeping each piece's place inside its line.
///
/// The label's own `<br>`-separated lines are honoured first, then each is wrapped; a
/// zero width yields no pieces at all. On top of the wrap sits the forward scan that
/// locates each piece back in the line it came from:
/// [`wrap_plain`] drops the whitespace at a break and can drop a grapheme cluster wider
/// than the whole budget, so the pieces of a line are in order but not adjacent. A piece
/// the scan cannot find is left unlocated and draws without provenance, rather than
/// claiming bytes chosen by a guess.
pub fn label_pieces(label: &Label, width: usize) -> Vec<Piece> {
    if width == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (index, line) in label.lines.iter().enumerate() {
        let mut cursor = 0usize;
        for text in wrap_plain(line, width) {
            let at = line[cursor..].find(&text).map(|found| cursor + found);
            cursor = at.map_or(cursor, |at| at + text.len());
            out.push(Piece { text, index, at });
        }
    }
    out
}

/// The label's own lines as pieces, with nothing wrapped away.
///
/// [`label_pieces`] for a caller that draws each `<br>`-separated line whole rather than
/// wrapping it — a frame title, which is clipped to the top edge it is written into.
/// Every piece starts at byte zero of its line, which is what it is: the whole of it.
pub fn label_rows(label: &Label) -> Vec<Piece> {
    label
        .lines
        .iter()
        .enumerate()
        .map(|(index, line)| Piece {
            text: line.clone(),
            index,
            at: Some(0),
        })
        .collect()
}

/// Adds the search spans behind one drawn piece, whose first column is `(row, col)`.
///
/// **One span per run [`Label::spans_for`] returns, never one span naming the whole
/// label.** A reader dragging inside a box gets the characters they went over (design
/// spec §2.2), which is only possible if each drawn run names its own bytes; the label
/// survives as the [`unit`](SearchSpan::unit) every run of it shares, which is what
/// `select::resolve` asks "did this drag stay inside one label?" of.
///
/// Emits nothing when the label carries no source — `Label::line`, a label
/// `lex::label_at` refused to place, or a `Label::from_lines` hull — because a span at
/// byte zero of the document is worse than no span at all.
pub fn label_spans(canvas: &mut Canvas, label: &Label, piece: &Piece, row: usize, col: usize) {
    let (Some(at), false) = (piece.at, label.source.is_empty()) else {
        return;
    };
    let unit = Some((label.source.start, label.source.end));
    for span in label.spans_for(piece.index, at, &piece.text) {
        canvas.add_span(SearchSpan {
            source_start: span.source.start,
            source_end: span.source.end,
            unit,
            row,
            col: u16::try_from(col + span.col).unwrap_or(u16::MAX),
            cols: u16::try_from(span.cols).unwrap_or(u16::MAX),
        });
    }
}

/// A label flattened onto one row: its lines joined by a space.
///
/// What a chart with one row per item draws — a pie slice, a gantt task, a message
/// arrow. The join is not reversible, which is exactly why [`label_row_span`] hands the
/// flattened text back to `Label::spans_for` rather than assuming it is line zero: a
/// one-line label passes that check and a `<br>`-broken one does not.
pub fn label_one_line(label: &Label) -> String {
    label.lines.join(" ")
}

/// Adds the spans behind a label drawn whole on one row, starting at `(row, col)`.
///
/// `text` is what was really painted, shortening included. Emits nothing when `text` is
/// not the label's first line as written — see [`label_one_line`].
pub fn label_row_span(canvas: &mut Canvas, label: &Label, text: &str, row: usize, col: usize) {
    let piece = Piece {
        text: text.to_string(),
        index: 0,
        at: Some(0),
    };
    label_spans(canvas, label, &piece, row, col);
}

/// The width of the widest line in `lines`.
pub fn lines_width(lines: &[String]) -> usize {
    lines
        .iter()
        .map(String::as_str)
        .map(display_width)
        .max()
        .unwrap_or(0)
}

/// The width of the widest line a [`Label`] needs when nothing wraps.
pub fn label_natural_width(label: &Label) -> usize {
    label
        .lines
        .iter()
        .map(String::as_str)
        .map(display_width)
        .max()
        .unwrap_or(0)
}

/// The one-line body a family draws when it has nothing to draw.
///
/// Pie, gantt and sequence all reported "no slices" / "no tasks" / "no participants"
/// with character-identical code; §14 makes that a defect, so it lives here.
pub fn placeholder(text: &str, width: u16, theme: &Theme) -> Canvas {
    let text = ellipsize(text, usize::from(width));
    let cols = u16::try_from(crate::text::display_width(&text)).unwrap_or(0);
    let mut body = Canvas::new(cols, 0, theme.base());
    body.push_text(&text, crate::text::Align::Left, theme.text.dim);
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{ELLIPSIS, grapheme_width};

    #[test]
    fn every_shared_glyph_is_one_column_wide() {
        for glyph in EIGHTH_BLOCKS
            .iter()
            .skip(1)
            .chain(std::iter::once(&ELLIPSIS))
        {
            assert_eq!(grapheme_width(glyph), 1, "{glyph:?} must be one column");
        }
    }

    #[test]
    fn eighth_bar_respects_its_budget() {
        assert_eq!(eighth_bar(0, 4), "");
        assert_eq!(eighth_bar(8, 4), "█");
        assert_eq!(eighth_bar(12, 4), "█▌");
        assert_eq!(eighth_bar(999, 3), "███");
        // A tiny non-zero value still shows something.
        assert_eq!(eighth_bar(1, 4), "▏");
    }

    #[test]
    fn eighths_of_clamps_degenerate_input() {
        assert_eq!(eighths_of(f64::NAN, 10), 0);
        assert_eq!(eighths_of(-1.0, 10), 0);
        assert_eq!(eighths_of(2.0, 10), 80);
        assert_eq!(eighths_of(0.5, 10), 40);
    }
}
