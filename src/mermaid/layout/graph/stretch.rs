// SPDX-License-Identifier: MIT
//! Growing a node box along its sides, so a sideways flow has room for every port.
//!
//! In a left-to-right flow an edge meets a box on a side that is only as many cells
//! long as the box is tall, and a one-line box has one. Edges with the same terminator
//! share it as one stem by design, but edges that end differently — an arrow and a
//! plain dotted tie — would merge into one line and lose what tells them apart. Such a
//! box is grown by blank rows inside its outline until each kind of end has its own
//! cell, with a cell of air between them.

use crate::canvas::Canvas;
use crate::theme::Style;

use super::glyph;

/// `canvas` with `extra` blank rows added inside its outline, half above the first
/// inner row and half below the last, so the text stays centred.
///
/// A filler row copies the inner row next to it with everything between its outline
/// cells blanked in their own styles, which keeps a painted box's tint. Search spans
/// stay with the rows that hold text. A canvas with no inner row is returned as is.
pub(super) fn rows(canvas: &Canvas, extra: usize, fill: Style) -> Canvas {
    let height = canvas.height();
    if extra == 0 || height < 3 {
        return canvas.clone();
    }
    let above = extra / 2;
    let below = extra - above;
    let mut parts = vec![canvas.slice_rows(0, 1)];
    parts.extend((0..above).map(|_| filler(canvas, 1)));
    parts.push(canvas.slice_rows(1, height - 2));
    parts.extend((0..below).map(|_| filler(canvas, height - 2)));
    parts.push(canvas.slice_rows(height - 1, 1));
    Canvas::vconcat(&parts, canvas.width(), fill)
}

/// Row `row` of `canvas` with its inside blanked.
fn filler(canvas: &Canvas, row: usize) -> Canvas {
    let mut out = canvas.slice_rows(row, 1);
    out.map_spans(|_| None);
    let cells: Vec<(String, Style, bool)> = out
        .row(0)
        .unwrap_or_default()
        .iter()
        .map(|cell| {
            (
                cell.text().to_string(),
                cell.style(),
                cell.is_continuation(),
            )
        })
        .collect();
    let outline = |text: &str| {
        text.chars()
            .next()
            .is_some_and(|ch| glyph::mask_of(ch).is_some())
    };
    let first = cells.iter().position(|(text, ..)| outline(text));
    let last = cells.iter().rposition(|(text, ..)| outline(text));
    if let (Some(first), Some(last)) = (first, last) {
        for (col, (_, style, _)) in cells.iter().enumerate().take(last).skip(first + 1) {
            out.write_str(0, col, " ", *style);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn a_box_grows_by_blank_rows_around_its_text() {
        let theme = Theme::default_dark();
        let mut canvas = Canvas::new(8, 3, theme.base());
        canvas.write_str(0, 0, "┌──────┐", theme.base());
        canvas.write_str(1, 0, "│ Idle │", theme.base());
        canvas.write_str(2, 0, "└──────┘", theme.base());
        let grown = rows(&canvas, 2, theme.base());
        let text: Vec<String> = (0..grown.height()).map(|row| grown.row_text(row)).collect();
        assert_eq!(
            text,
            ["┌──────┐", "│      │", "│ Idle │", "│      │", "└──────┘"]
        );
    }
}
