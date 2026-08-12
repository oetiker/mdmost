//! The node bodies of a state diagram (design spec §6.7).
//!
//! Five shapes: the rounded box of an ordinary state, the filled dot that starts a
//! scope, the ringed dot that ends one, the diamond of a `<<choice>>` and the bar of a
//! `<<fork>>`/`<<join>>`. Notes reuse the rounded box in the note ink.

use crate::canvas::{BorderSet, Canvas, align_offset};
use crate::mermaid::ast::Label;
use crate::mermaid::chrome::{self, Piece};
use crate::mermaid::layout::graph::PortPolicy;
use crate::text::{Align, display_width, ellipsize};
use crate::theme::{Style, Theme};

/// Blank columns between a state's label and its border.
const PAD: usize = 1;
/// Columns taken by the border, one on each side.
const BORDER: usize = 2;
/// The narrowest label column worth drawing.
const MIN_TEXT: usize = 3;
/// How wide a fork/join bar is drawn.
const BAR_WIDTH: u16 = 7;

/// The glyph marking the start of a scope.
const START: &str = "●";
/// The glyph marking the end of a scope.
const END: &str = "◉";

/// Draws a rounded state box holding a state's label.
pub(super) fn state(label: &Label, budget: u16, theme: &Theme) -> Canvas {
    box_of(label, budget, usize::MAX, theme, theme.diagram.node_text)
}

/// Draws a note box: the same outline in the note ink, capped at `cap` columns.
pub(super) fn note(label: &Label, budget: u16, cap: usize, theme: &Theme) -> Canvas {
    box_of(label, budget, cap, theme, theme.diagram.note)
}

/// A rounded box wrapping `label` to the budget and drawing it in `ink`.
///
/// The label arrives whole rather than as finished lines, because the wrap is where a
/// drawn row loses track of the bytes it came from: `chrome::label_pieces` keeps that
/// correspondence so every row can name its own bytes (design spec §2.2).
fn box_of(label: &Label, budget: u16, cap: usize, theme: &Theme, ink: Style) -> Canvas {
    let text_budget = usize::from(budget)
        .saturating_sub(BORDER + 2 * PAD)
        .max(MIN_TEXT)
        .min(cap);
    let mut pieces = chrome::label_pieces(label, text_budget);
    for piece in &mut pieces {
        // A shortened piece keeps its own text, so the span it emits — if any — names
        // the cells that were really painted rather than the ones that were not.
        piece.text = ellipsize(&piece.text, text_budget);
    }
    if pieces.is_empty() {
        pieces.push(Piece {
            text: String::new(),
            index: 0,
            at: None,
        });
    }
    let text = pieces
        .iter()
        .map(|piece| display_width(&piece.text))
        .max()
        .unwrap_or(0)
        .max(1);

    let mut body = Canvas::new((text + 2 * PAD) as u16, 0, theme.base());
    for piece in &pieces {
        let row = body.push_blank_row(theme.base());
        body.write_field(row, PAD, text, &piece.text, Align::Center, ink);
        // `write_field` centres through `text::pad_to_width`; `align_offset` is that same
        // rule asked rather than re-derived.
        let col = PAD + align_offset(text, display_width(&piece.text), Align::Center);
        chrome::label_spans(&mut body, label, piece, row, col);
    }
    body.framed(
        BorderSet::ROUNDED,
        theme.diagram.node_border,
        None,
        theme.base(),
    )
}

/// Draws the filled dot that a scope's `[*] -->` transition starts from.
pub(super) fn start(theme: &Theme) -> Canvas {
    marker(START, theme)
}

/// Draws the ringed dot that a scope's `--> [*]` transition ends at.
pub(super) fn end(theme: &Theme) -> Canvas {
    marker(END, theme)
}

/// A one-cell marker glyph.
fn marker(glyph: &str, theme: &Theme) -> Canvas {
    let mut canvas = Canvas::new(1, 0, theme.base());
    let row = canvas.push_blank_row(theme.base());
    canvas.write_str(row, 0, glyph, theme.diagram.node_border);
    canvas
}

/// Draws the diamond of a `<<choice>>` state.
///
/// Kept deliberately small: a choice carries no text of its own, and a large empty
/// diamond reads as a missing label rather than as a decision point.
pub(super) fn choice(theme: &Theme) -> Canvas {
    let ink = theme.diagram.node_border;
    let mut canvas = Canvas::new(4, 0, theme.base());
    for text in [" ╱╲ ", "╱  ╲", "╲  ╱", " ╲╱ "] {
        let row = canvas.push_blank_row(theme.base());
        canvas.write_str(row, 0, text, ink);
    }
    canvas
}

/// Draws the solid bar of a `<<fork>>` or `<<join>>` state.
pub(super) fn bar(theme: &Theme) -> Canvas {
    let mut canvas = Canvas::new(BAR_WIDTH, 0, theme.base());
    let row = canvas.push_blank_row(theme.base());
    canvas.fill(
        row,
        0,
        usize::from(BAR_WIDTH),
        "█",
        theme.diagram.node_border,
    );
    canvas
}

/// Where edges may attach to each shape.
///
/// The diamond and the two markers are only meetable at their middle; everything else
/// has straight sides and can fan its edges out.
pub(super) const fn ports(centred: bool) -> PortPolicy {
    if centred {
        PortPolicy::Center
    } else {
        PortPolicy::Spread
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::default_dark()
    }

    #[test]
    fn a_state_box_is_rounded_and_padded() {
        let canvas = state(&Label::line("Idle"), 40, &theme());
        let text = canvas.plain_text();
        assert!(text.starts_with('╭'), "{text}");
        assert!(canvas.row_text(1).contains(" Idle "), "{text}");
    }

    #[test]
    fn a_state_box_wraps_a_long_label() {
        let canvas = state(&Label::line("a rather long state name"), 16, &theme());
        assert!(canvas.width() <= 16, "{}", canvas.width());
        assert!(
            canvas.height() > 3,
            "expected wrapping: {}",
            canvas.height()
        );
    }

    #[test]
    fn an_empty_label_still_draws_a_box() {
        let canvas = state(&Label::default(), 20, &theme());
        assert_eq!(canvas.height(), 3);
        canvas.check_invariants().expect("canvas contract");
    }

    #[test]
    fn markers_are_one_cell() {
        for canvas in [start(&theme()), end(&theme())] {
            assert_eq!(canvas.width(), 1);
            assert_eq!(canvas.height(), 1);
        }
        assert_eq!(start(&theme()).plain_text().trim(), START);
        assert_eq!(end(&theme()).plain_text().trim(), END);
    }

    #[test]
    fn a_choice_is_a_small_diamond() {
        let canvas = choice(&theme());
        assert_eq!(canvas.width(), 4);
        assert_eq!(canvas.height(), 4);
        canvas.check_invariants().expect("canvas contract");
    }

    #[test]
    fn a_bar_is_one_solid_row() {
        let canvas = bar(&theme());
        assert_eq!(canvas.height(), 1);
        assert_eq!(canvas.row_text(0), "█".repeat(usize::from(BAR_WIDTH)));
    }

    #[test]
    fn a_tiny_budget_never_panics() {
        for budget in 0..12u16 {
            let canvas = state(&Label::line("Something long"), budget, &theme());
            canvas.check_invariants().expect("canvas contract");
        }
    }

    #[test]
    fn ports_are_centred_only_where_the_outline_is_not_straight() {
        assert_eq!(ports(true), PortPolicy::Center);
        assert_eq!(ports(false), PortPolicy::Spread);
    }
}
