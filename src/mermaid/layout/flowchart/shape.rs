// SPDX-License-Identifier: MIT
//! Flowchart node boxes, one per shape of design spec §6.1.
//!
//! Every shape is built from the same two steps — wrap the label, then draw an outline
//! around it — so they are guaranteed to agree about padding and about where their
//! border sits. Shapes the parser could not classify already arrive as
//! [`NodeShape::Rect`], so there is no fallback to handle here.

use crate::canvas::{BorderSet, Canvas, SearchSpan, align_offset};
use crate::mermaid::ast::{Label, NodeShape};
use crate::mermaid::chrome;
use crate::mermaid::layout::graph::PortPolicy;
use crate::text::{Align, display_width};
use crate::theme::Theme;

/// Columns of blank space between the label and the outline.
const PAD: usize = 1;

/// Draws one node box at most `budget` columns wide.
pub(super) fn draw(label: &Label, shape: NodeShape, budget: u16, theme: &Theme) -> Canvas {
    let frame = outline_width(shape);
    let inner_budget = usize::from(budget).saturating_sub(frame + 2 * PAD).max(3);
    // `inner_budget` is at least 3, so the wrap is never asked for zero columns; the
    // blank piece below is the only way this can come back empty.
    let lines = chrome::label_pieces_or_blank(label, inner_budget);
    let text = lines
        .iter()
        .map(|line| display_width(&line.text))
        .max()
        .unwrap_or(0)
        .max(1);
    let styles = theme.diagram;
    let field = text + 2 * PAD;
    let mut body = Canvas::new(field as u16, lines.len(), theme.base());
    for (row, line) in lines.iter().enumerate() {
        body.write_field(row, 0, field, &line.text, Align::Center, styles.node_text);
        // Where this drawn line came from in the Mermaid source, run by run: the
        // characters a reader drags over are the characters they get (design spec §2.2),
        // so a wrapped label's rows name their own bytes rather than all naming the whole
        // label. `chrome::label_spans` is that rule for every family at once, including
        // the "no `source` means no span" part: a label built by `Label::line`, or one
        // `lex::label_at` refused to place, must emit nothing rather than a span at byte
        // zero of the document.
        let drawn = display_width(&line.text);
        if drawn == 0 {
            continue;
        }
        // `write_field` centres through `text::pad_to_width`, whose left pad is
        // `slack / 2`; `align_offset` is the same rule, asked rather than re-derived.
        let col = align_offset(field, drawn, Align::Center);
        chrome::label_spans(&mut body, label, line, row, col);
    }
    match shape {
        NodeShape::Rect => body.framed(BorderSet::PLAIN, styles.node_border, None, theme.base()),
        NodeShape::Round => body.framed(BorderSet::ROUNDED, styles.node_border, None, theme.base()),
        NodeShape::Stadium => walls(&body, theme, "(", ")", BorderSet::ROUNDED),
        NodeShape::Circle => {
            let wide = body.indent(1, 1, theme.base());
            walls(&wide, theme, "((", "))", BorderSet::ROUNDED)
        }
        NodeShape::Rhombus => rhombus(&body, theme),
        NodeShape::Subroutine => subroutine(&body, theme),
        NodeShape::Cylinder => cylinder(&body, theme),
    }
}

/// Moves `src`'s spans on `row` onto row `index`, column `left`, of `out`.
///
/// [`Canvas::blit`] translates spans for the shapes built from `framed` and `indent`.
/// [`rhombus`] and [`cylinder`] cannot use it — they interleave rules between the rows
/// they copy — so they copy cell by cell, and a span left behind by that loop is a
/// label that silently loses its provenance in two shapes out of seven. Called from
/// inside each copy loop with the loop's own `index`, so the two can never disagree
/// about where the row landed.
fn carry_spans(out: &mut Canvas, src: &Canvas, row: usize, index: usize, left: u16) {
    let moved: Vec<SearchSpan> = src
        .spans()
        .iter()
        .filter(|span| span.row == row)
        .map(|span| SearchSpan {
            row: index,
            col: span.col.saturating_add(left),
            ..*span
        })
        .collect();
    for span in moved {
        out.add_span(span);
    }
}

/// Where edges may attach to a node of this shape.
pub(super) fn ports(shape: NodeShape) -> PortPolicy {
    match shape {
        NodeShape::Rhombus | NodeShape::Circle => PortPolicy::Center,
        _ => PortPolicy::Spread,
    }
}

/// How many columns the outline itself takes on each side.
fn outline_width(shape: NodeShape) -> usize {
    match shape {
        NodeShape::Circle => 4,
        NodeShape::Subroutine => 4,
        _ => 2,
    }
}

/// A framed box whose vertical edges are replaced by `left` and `right`.
fn walls(body: &Canvas, theme: &Theme, left: &str, right: &str, border: BorderSet) -> Canvas {
    let styles = theme.diagram;
    let pad = left.chars().count().saturating_sub(1);
    let body = if pad > 0 {
        body.indent(pad as u16, pad as u16, theme.base())
    } else {
        body.clone()
    };
    let mut out = body.framed(border, styles.node_border, None, theme.base());
    let last = usize::from(out.width()) - left.chars().count();
    for row in 1..out.height() - 1 {
        out.write_str(row, 0, left, styles.node_border);
        out.write_str(row, last, right, styles.node_border);
    }
    out
}

/// A rhombus: slanted top and bottom edges over straight sides.
fn rhombus(body: &Canvas, theme: &Theme) -> Canvas {
    let styles = theme.diagram;
    let width = usize::from(body.width()) + 2;
    let mut out = Canvas::new(width as u16, 0, theme.base());
    let span = width.saturating_sub(4);
    let cap = |lead: char, tail: char| {
        let mut text = String::from(" ");
        text.push(lead);
        text.push_str(&"─".repeat(span));
        text.push(tail);
        text
    };
    out.push_text(&cap('╱', '╲'), Align::Left, styles.node_border);
    for row in 0..body.height() {
        let index = out.push_blank_row(theme.base());
        out.write_str(index, 0, "│", styles.node_border);
        if let Some(cells) = body.row(row) {
            for (col, cell) in cells.iter().enumerate() {
                out.write_str(index, col + 1, cell.text(), cell.style());
            }
        }
        carry_spans(&mut out, body, row, index, 1);
        out.write_str(index, width - 1, "│", styles.node_border);
    }
    out.push_text(&cap('╲', '╱'), Align::Left, styles.node_border);
    out
}

/// A subroutine box: a rectangle with an inner rule down each side.
fn subroutine(body: &Canvas, theme: &Theme) -> Canvas {
    let styles = theme.diagram;
    let wide = body.indent(2, 2, theme.base());
    let mut out = wide.framed(BorderSet::PLAIN, styles.node_border, None, theme.base());
    let last = usize::from(out.width()) - 1;
    let height = out.height();
    out.write_str(0, 2, "┬", styles.node_border);
    out.write_str(height - 1, 2, "┴", styles.node_border);
    out.write_str(0, last - 2, "┬", styles.node_border);
    out.write_str(height - 1, last - 2, "┴", styles.node_border);
    for row in 1..height - 1 {
        out.write_str(row, 2, "│", styles.node_border);
        out.write_str(row, last - 2, "│", styles.node_border);
    }
    out
}

/// A cylinder: a rounded box with an ellipse rule under its lid and above its base.
///
/// Both rules are drawn, so the shape reads as a drum seen slightly from above rather
/// than as a rectangle with one stray line across it.
fn cylinder(body: &Canvas, theme: &Theme) -> Canvas {
    let styles = theme.diagram;
    let framed = body.framed(BorderSet::ROUNDED, styles.node_border, None, theme.base());
    let width = usize::from(framed.width());
    let mut rule = String::from("├");
    rule.push_str(&"─".repeat(width.saturating_sub(2)));
    rule.push('┤');
    let last = framed.height().saturating_sub(1);
    let mut out = Canvas::new(framed.width(), 0, theme.base());
    for row in 0..framed.height() {
        if row == last {
            let index = out.push_blank_row(theme.base());
            out.write_str(index, 0, &rule, styles.node_border);
        }
        let index = out.push_blank_row(theme.base());
        if let Some(cells) = framed.row(row) {
            for (col, cell) in cells.iter().enumerate() {
                out.write_str(index, col, cell.text(), cell.style());
            }
        }
        carry_spans(&mut out, &framed, row, index, 0);
        if row == 0 {
            let index = out.push_blank_row(theme.base());
            out.write_str(index, 0, &rule, styles.node_border);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty label still draws a row, so the node is a box and not two rules.
    ///
    /// The blank piece `chrome::label_pieces_or_blank` supplies is the only thing
    /// standing between `A[]` and a node with no inside. Only the state diagram guarded
    /// that before; the flowchart shared the behaviour and none of the test.
    #[test]
    fn an_empty_label_still_draws_a_box() {
        let theme = Theme::default_dark();
        for shape in [
            NodeShape::Rect,
            NodeShape::Round,
            NodeShape::Stadium,
            NodeShape::Circle,
            NodeShape::Rhombus,
            NodeShape::Subroutine,
            NodeShape::Cylinder,
        ] {
            let canvas = draw(&Label::default(), shape, 20, &theme);
            assert!(canvas.height() >= 3, "{shape:?}: {}", canvas.height());
            canvas.check_invariants().expect("canvas contract");
        }
    }
}
