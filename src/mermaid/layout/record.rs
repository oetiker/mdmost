//! The compartment box shared by class and ER nodes.
//!
//! A `classDiagram` node is a name over a field list over a method list (design spec
//! §6.3); an `erDiagram` node is a name over an attribute table (§6.4). Those are the
//! same drawing — a framed box divided by full-width rules — so it is built once here
//! rather than twice in the two family modules (design spec §14).
//!
//! Two rules come from the visual review in `docs/qa/visual-review.md` and are worth
//! stating because they are easy to get wrong:
//!
//! * **Content is body-weight ink; structure is dim.** Member and attribute text is the
//!   *content* of a class or ER diagram, so it is drawn in the node text style while
//!   borders and compartment rules stay quiet. The review's finding M7 was that a
//!   diagram whose labels are fainter than its lines reads backwards.
//! * **Boxes have interior padding.** One blank column on each side of the text, so no
//!   glyph ever touches the border (finding P1: nothing in the program has margins).

use crate::canvas::{BorderSet, Canvas};
use crate::mermaid::chrome;
use crate::text::{Align, display_width};
use crate::theme::{Style, Theme};

/// Blank columns between the text and the box border.
const PAD: usize = 1;
/// Columns taken by the border itself, one on each side.
const BORDER: usize = 2;
/// The narrowest text column worth drawing before elision takes over.
const MIN_TEXT: usize = 3;

/// One line of text inside a compartment.
#[derive(Debug, Clone)]
pub(super) struct Row {
    /// The text to draw. Elided with `…` when it does not fit.
    pub text: String,
    /// How the text sits within the box's text column.
    pub align: Align,
    /// The ink to draw it in.
    pub style: Style,
}

impl Row {
    /// A centred row, used for names and stereotypes.
    pub fn centred(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            align: Align::Center,
            style,
        }
    }

    /// A left-aligned row, used for members and attributes.
    pub fn left(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            align: Align::Left,
            style,
        }
    }
}

/// Draws a compartment box at most `budget` columns wide.
///
/// Empty compartments are dropped rather than drawn as an empty band, so a class with
/// no members is a plain name box and an entity with no attributes is a plain name box.
/// A box with nothing in it at all still draws as a one-row frame rather than
/// collapsing to nothing.
pub(super) fn draw(compartments: &[Vec<Row>], budget: u16, theme: &Theme) -> Canvas {
    let styles = theme.diagram;
    let filled: Vec<&Vec<Row>> = compartments
        .iter()
        .filter(|rows| !rows.is_empty())
        .collect();

    let text_budget = usize::from(budget)
        .saturating_sub(BORDER + 2 * PAD)
        .max(MIN_TEXT);
    let elided: Vec<Vec<Row>> = filled
        .iter()
        .map(|rows| {
            rows.iter()
                .map(|row| Row {
                    text: chrome::fit(&row.text, text_budget),
                    ..row.clone()
                })
                .collect()
        })
        .collect();

    let text_width = elided
        .iter()
        .flatten()
        .map(|row| display_width(&row.text))
        .max()
        .unwrap_or(0)
        .max(1);
    let inner = text_width + 2 * PAD;

    // Lay the compartments out first, remembering where each rule has to go, then
    // frame the result and cut the rules into the side borders.
    let mut body = Canvas::new(inner as u16, 0, theme.base());
    let mut rules: Vec<usize> = Vec::new();
    for (at, rows) in elided.iter().enumerate() {
        if at > 0 {
            rules.push(body.height());
            body.push_blank_row(theme.base());
        }
        for row in rows {
            let index = body.push_blank_row(theme.base());
            body.write_field(index, PAD, text_width, &row.text, row.align, row.style);
        }
    }
    if body.height() == 0 {
        body.push_blank_row(theme.base());
    }

    let mut out = body.framed(BorderSet::PLAIN, styles.node_border, None, theme.base());
    for rule in rules {
        // +1 for the frame's own top edge.
        let row = rule + 1;
        out.write_str(row, 0, "├", styles.node_border);
        out.hline(row, 1, inner, "─", styles.compartment);
        out.write_str(row, inner + 1, "┤", styles.node_border);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::default_dark()
    }

    fn rows(texts: &[&str]) -> Vec<Row> {
        let style = theme().diagram.node_text;
        texts.iter().map(|text| Row::left(*text, style)).collect()
    }

    #[test]
    fn compartments_are_separated_by_a_rule_tied_into_the_border() {
        let theme = theme();
        let canvas = draw(&[rows(&["Name"]), rows(&["+field"])], 40, &theme);
        let text = canvas.plain_text();
        assert!(text.contains("├──"), "{text}");
        assert!(text.contains("──┤"), "{text}");
    }

    #[test]
    fn an_empty_compartment_is_dropped_rather_than_drawn_blank() {
        let theme = theme();
        let with_gap = draw(&[rows(&["Name"]), Vec::new()], 40, &theme);
        let without = draw(&[rows(&["Name"])], 40, &theme);
        assert_eq!(with_gap.plain_text(), without.plain_text());
    }

    #[test]
    fn every_row_is_padded_away_from_the_border() {
        let theme = theme();
        let canvas = draw(&[rows(&["abc"])], 40, &theme);
        let row = canvas.row_text(1);
        assert!(row.starts_with("│ abc "), "{row:?}");
        assert!(row.ends_with(" │"), "{row:?}");
    }

    #[test]
    fn a_row_too_wide_for_the_budget_is_elided() {
        let theme = theme();
        let canvas = draw(&[rows(&["a very long member indeed"])], 14, &theme);
        assert!(canvas.width() <= 14, "{}", canvas.width());
        assert!(canvas.plain_text().contains('…'));
    }

    #[test]
    fn a_tiny_budget_never_panics_and_still_draws_a_box() {
        let theme = theme();
        for budget in 0..12u16 {
            let canvas = draw(&[rows(&["Name"]), rows(&["+f"])], budget, &theme);
            assert!(canvas.height() >= 3, "budget {budget}");
            canvas.check_invariants().expect("canvas contract");
        }
    }

    #[test]
    fn a_box_with_no_content_at_all_is_still_a_frame() {
        let theme = theme();
        let canvas = draw(&[], 20, &theme);
        assert_eq!(canvas.height(), 3);
        canvas.check_invariants().expect("canvas contract");
    }
}
