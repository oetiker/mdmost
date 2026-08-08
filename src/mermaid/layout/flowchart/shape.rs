//! Flowchart node boxes, one per shape of design spec §6.1.
//!
//! Every shape is built from the same two steps — wrap the label, then draw an outline
//! around it — so they are guaranteed to agree about padding and about where their
//! border sits. Shapes the parser could not classify already arrive as
//! [`NodeShape::Rect`], so there is no fallback to handle here.

use crate::canvas::{BorderSet, Canvas};
use crate::mermaid::ast::{Label, NodeShape};
use crate::mermaid::layout::graph::PortPolicy;
use crate::text::{Align, display_width, wrap_plain};
use crate::theme::Theme;

/// Columns of blank space between the label and the outline.
const PAD: usize = 1;

/// Draws one node box at most `budget` columns wide.
pub(super) fn draw(label: &Label, shape: NodeShape, budget: u16, theme: &Theme) -> Canvas {
    let frame = outline_width(shape);
    let inner_budget = usize::from(budget).saturating_sub(frame + 2 * PAD).max(3);
    let lines = wrap(label, inner_budget);
    let text = lines
        .iter()
        .map(|line| display_width(line))
        .max()
        .unwrap_or(0)
        .max(1);
    let styles = theme.diagram;
    let mut body = Canvas::new((text + 2 * PAD) as u16, lines.len(), theme.base());
    for (row, line) in lines.iter().enumerate() {
        body.write_field(
            row,
            0,
            text + 2 * PAD,
            line,
            Align::Center,
            styles.node_text,
        );
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

/// Wraps a label to `budget` columns, keeping its explicit `<br>` breaks.
fn wrap(label: &Label, budget: usize) -> Vec<String> {
    let mut out: Vec<String> = label
        .lines
        .iter()
        .flat_map(|line| wrap_plain(line, budget.max(1)))
        .collect();
    if out.is_empty() {
        out.push(String::new());
    }
    out
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
        if row == 0 {
            let index = out.push_blank_row(theme.base());
            out.write_str(index, 0, &rule, styles.node_border);
        }
    }
    out
}
