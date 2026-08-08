//! Chrome shared by the three purpose-built Mermaid chart renderers.
//!
//! [`sequence`](crate::mermaid::sequence), [`pie`](crate::mermaid::pie) and
//! [`gantt`](crate::mermaid::gantt) do not use the graph layout engine, but they do
//! share the furniture around their plots: a centred title, horizontal placement of a
//! finished plot inside the width budget, sub-cell bar fills and label fitting. That
//! furniture lives here so none of it is written twice (design spec §14).

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::Label;
use crate::text::{Align, display_width, truncate_to_width, wrap_plain};
use crate::theme::Theme;

/// Left-growing block elements, indexed by how many eighths of a cell are filled.
///
/// Index `0` is the empty string, index `8` is a full block. This is what gives pie
/// bars their sub-cell precision (design spec §6.5). Gantt bars deliberately stay on
/// whole cells so their texture can carry the task state.
pub const EIGHTH_BLOCKS: [&str; 9] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

/// The ellipsis appended to a label that had to be shortened.
pub const ELLIPSIS: &str = "…";

/// Composes a finished plot into a canvas exactly `width` columns wide.
///
/// The optional `title` is centred above the plot and followed by a blank row; the
/// plot itself is centred horizontally. This is the last step of every `draw`
/// function, which is what makes "every row is exactly `width` columns" structurally
/// true rather than a property each renderer has to remember.
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
        return Err(MermaidError::TooNarrow { width });
    }
    let base = theme.base();
    let mut out = Canvas::empty(width);
    if let Some(title) = title.map(str::trim).filter(|text| !text.is_empty()) {
        out.push_text_ellipsized(title, Align::Center, theme.diagram.title);
        out.push_blank_row(base);
    }
    let left = usize::from(width - body.width()) / 2;
    let top = out.height();
    out.blit(top, left, body, base);
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

/// Shortens `text` to at most `width` display columns, marking the cut with an ellipsis.
///
/// Text that already fits is returned unchanged. Cuts land on grapheme cluster
/// boundaries, so combining marks and emoji sequences survive intact.
pub fn fit(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    format!("{}{ELLIPSIS}", truncate_to_width(text, width - 1))
}

/// Wraps a [`Label`] into plain lines of at most `width` display columns.
///
/// The label's own `<br>`-separated lines are honoured first, then each is wrapped.
/// A zero width yields no lines at all.
pub fn label_lines(label: &Label, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    label
        .lines
        .iter()
        .flat_map(|line| wrap_plain(line, width))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::grapheme_width;

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

    #[test]
    fn fit_marks_the_cut() {
        assert_eq!(fit("hello", 10), "hello");
        assert_eq!(fit("hello", 5), "hello");
        assert_eq!(fit("hello", 4), "hel…");
        assert_eq!(fit("hello", 0), "");
        // A double-width cluster that would straddle the limit is dropped, not split.
        assert_eq!(fit("日本語", 4), "日…");
    }
}
