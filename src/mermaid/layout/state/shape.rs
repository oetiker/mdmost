//! The node bodies of a state diagram (design spec §6.7).
//!
//! Five shapes: the rounded box of an ordinary state, the filled dot that starts a
//! scope, the ringed dot that ends one, the diamond of a `<<choice>>` and the bar of a
//! `<<fork>>`/`<<join>>`. Notes reuse the rounded box in the note ink.

use crate::canvas::{BorderSet, Canvas};
use crate::mermaid::chrome;
use crate::mermaid::layout::graph::PortPolicy;
use crate::text::{Align, display_width, wrap_plain};
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

/// Draws a rounded state box holding `lines` of label text.
pub(super) fn state(label: &[String], budget: u16, theme: &Theme) -> Canvas {
    box_of(label, budget, theme, theme.diagram.node_text)
}

/// Draws a note box: the same outline in the note ink.
pub(super) fn note(label: &[String], budget: u16, theme: &Theme) -> Canvas {
    box_of(label, budget, theme, theme.diagram.note)
}

/// A rounded box wrapping `label` to the budget and drawing it in `ink`.
fn box_of(label: &[String], budget: u16, theme: &Theme, ink: Style) -> Canvas {
    let text_budget = usize::from(budget)
        .saturating_sub(BORDER + 2 * PAD)
        .max(MIN_TEXT);
    let mut lines: Vec<String> = label
        .iter()
        .flat_map(|line| wrap_plain(line, text_budget))
        .map(|line| chrome::fit(&line, text_budget))
        .collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    let text = lines
        .iter()
        .map(|line| display_width(line))
        .max()
        .unwrap_or(0)
        .max(1);

    let mut body = Canvas::new((text + 2 * PAD) as u16, 0, theme.base());
    for line in &lines {
        let row = body.push_blank_row(theme.base());
        body.write_field(row, PAD, text, line, Align::Center, ink);
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
        let canvas = state(&["Idle".to_string()], 40, &theme());
        let text = canvas.plain_text();
        assert!(text.starts_with('╭'), "{text}");
        assert!(canvas.row_text(1).contains(" Idle "), "{text}");
    }

    #[test]
    fn a_state_box_wraps_a_long_label() {
        let canvas = state(&["a rather long state name".to_string()], 16, &theme());
        assert!(canvas.width() <= 16, "{}", canvas.width());
        assert!(
            canvas.height() > 3,
            "expected wrapping: {}",
            canvas.height()
        );
    }

    #[test]
    fn an_empty_label_still_draws_a_box() {
        let canvas = state(&[], 20, &theme());
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
            let canvas = state(&["Something long".to_string()], budget, &theme());
            canvas.check_invariants().expect("canvas contract");
        }
    }

    #[test]
    fn ports_are_centred_only_where_the_outline_is_not_straight() {
        assert_eq!(ports(true), PortPolicy::Center);
        assert_eq!(ports(false), PortPolicy::Spread);
    }
}
