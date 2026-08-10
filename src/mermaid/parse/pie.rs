//! `pie` parsing (design spec §6.5).
//!
//! Supported: `pie`, `pie showData`, `pie title X`, a separate `title X` line, and
//! `"label" : value` slices. Values must be finite and non-negative; the renderer
//! turns them into a sorted bar chart.

use crate::error::MermaidError;
use crate::mermaid::ast::{PieChart, PieSlice};
use crate::mermaid::entity;

use super::lex::{self, Nesting, SrcLine};

/// Parses a whole `pie` chart.
pub fn parse(lines: &[SrcLine<'_>]) -> Result<PieChart, MermaidError> {
    let Some((header, body)) = lines.split_first() else {
        return Err(lex::syntax(1, "empty diagram"));
    };
    let mut chart = PieChart {
        title: None,
        show_data: false,
        slices: Vec::new(),
    };

    let (_, mut rest) = lex::split_word(header.text);
    if let Some(after) = lex::strip_keyword(rest, "showData") {
        chart.show_data = true;
        rest = after;
    }
    if !rest.is_empty() {
        statement(&mut chart, rest, header.number)?;
    }
    for line in body {
        statement(&mut chart, line.text, line.number)?;
    }
    if chart.slices.is_empty() {
        return Err(lex::syntax(
            lines.last().map_or(1, |line| line.number),
            "pie chart without any slices",
        ));
    }
    Ok(chart)
}

/// Handles one statement of a pie chart.
fn statement(chart: &mut PieChart, text: &str, line: usize) -> Result<(), MermaidError> {
    if let Some(rest) = lex::strip_keyword(text, "showData") {
        chart.show_data = true;
        if rest.is_empty() {
            return Ok(());
        }
        return statement(chart, rest, line);
    }
    if let Some(rest) = lex::strip_keyword(text, "title") {
        chart.title = Some(entity::decode(lex::unquote(rest)).into_owned());
        return Ok(());
    }
    let Some((label, value)) = lex::split_once_top_level(text, ':', Nesting::Ignore) else {
        return Err(lex::syntax(
            line,
            format!("cannot read a slice from `{text}`"),
        ));
    };
    let value: f64 = value
        .parse()
        .map_err(|_| lex::syntax(line, format!("`{value}` is not a number")))?;
    if !value.is_finite() || value < 0.0 {
        return Err(lex::syntax(
            line,
            format!("slice value `{value}` must be finite and not negative"),
        ));
    }
    chart.slices.push(PieSlice {
        label: entity::decode(lex::unquote(label)).into_owned(),
        value,
    });
    Ok(())
}
