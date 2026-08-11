//! Flowchart node boxes, one per shape of design spec §6.1.
//!
//! Every shape is built from the same two steps — wrap the label, then draw an outline
//! around it — so they are guaranteed to agree about padding and about where their
//! border sits. Shapes the parser could not classify already arrive as
//! [`NodeShape::Rect`], so there is no fallback to handle here.

use crate::canvas::{BorderSet, Canvas, SearchSpan, align_offset};
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
        .map(|line| display_width(&line.text))
        .max()
        .unwrap_or(0)
        .max(1);
    let styles = theme.diagram;
    let field = text + 2 * PAD;
    let mut body = Canvas::new(field as u16, lines.len(), theme.base());
    let unit = (!label.source.is_empty()).then_some((label.source.start, label.source.end));
    for (row, line) in lines.iter().enumerate() {
        body.write_field(row, 0, field, &line.text, Align::Center, styles.node_text);
        // Where this drawn line came from in the Mermaid source, run by run: the
        // characters a reader drags over are the characters they get (design spec §2.2),
        // so a wrapped label's rows name their own bytes rather than all naming the whole
        // label. They still carry the whole label as their `unit`, because "did the drag
        // stay inside one label?" is a question about the label and not about a row of it.
        //
        // An empty `source` is the contract's "synthesised, not from the source" — a
        // label built by `Label::line` or one `lex::label_at` refused to place — and it
        // must emit nothing rather than a span at byte zero of the document.
        let drawn = display_width(&line.text);
        let (Some(unit), Some(at)) = (unit, line.at) else {
            continue;
        };
        if drawn == 0 {
            continue;
        }
        // `write_field` centres through `text::pad_to_width`, whose left pad is
        // `slack / 2`; `align_offset` is the same rule, asked rather than re-derived.
        let col = align_offset(field, drawn, Align::Center);
        for span in label.spans_for(line.index, at, &line.text) {
            body.add_span(SearchSpan {
                source_start: span.source.start,
                source_end: span.source.end,
                unit: Some(unit),
                row,
                col: u16::try_from(col + span.col).unwrap_or(u16::MAX),
                cols: u16::try_from(span.cols).unwrap_or(u16::MAX),
            });
        }
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

/// One drawn line of a wrapped label, and where it came from inside the label.
struct Drawn {
    /// The text to draw.
    text: String,
    /// Which line of [`Label::lines`] it is a piece of.
    index: usize,
    /// Where it starts in that line, in bytes, or `None` if it could not be located.
    at: Option<usize>,
}

/// Wraps a label to `budget` columns, keeping its explicit `<br>` breaks.
///
/// Each piece is located back in the line it was wrapped from, because a span has to
/// name the bytes that drew it and wrapping is where the correspondence is lost. The
/// search is a forward scan rather than arithmetic: `wrap_spans` drops the whitespace at
/// a break, and drops a grapheme cluster wider than the whole budget, so the pieces of a
/// line are in order but not adjacent. A piece the scan cannot find — which that dropped
/// cluster can produce — is left unlocated and draws without provenance, rather than
/// claiming bytes chosen by a guess.
fn wrap(label: &Label, budget: usize) -> Vec<Drawn> {
    let mut out: Vec<Drawn> = Vec::new();
    for (index, line) in label.lines.iter().enumerate() {
        let mut cursor = 0usize;
        for text in wrap_plain(line, budget.max(1)) {
            let at = line[cursor..].find(&text).map(|found| cursor + found);
            cursor = at.map_or(cursor, |at| at + text.len());
            out.push(Drawn { text, index, at });
        }
    }
    if out.is_empty() {
        out.push(Drawn {
            text: String::new(),
            index: 0,
            at: Some(0),
        });
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
