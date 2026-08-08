//! `pie` charts, drawn as a sorted horizontal bar chart (design spec §6.5).
//!
//! A circle drawn in character cells reads badly, so the honest rendering of a pie
//! chart on a terminal is a bar chart: one row per slice, sorted by value, with a
//! colour swatch that doubles as the legend key, the label, a bar with sub-cell
//! precision, the share as a percentage and — when `showData` was given — the raw
//! value. A summary rule and total row close the chart off.
//!
//! ```text
//! ● Cats    ███████████████████████▌      41.2%   245
//! ● Dogs    ████████████████▍             28.6%   170
//! ● Birds   ██████████▏                   17.8%   106
//!   ─────────────────────────────────────────────────
//!   Total                                100.0%   521
//! ```

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::{PieChart, PieSlice};
use crate::mermaid::chrome;
use crate::text::{Align, display_width};
use crate::theme::{Style, Theme};

/// The legend swatch drawn in front of every slice label.
const SWATCH: &str = "●";
/// Columns taken by the swatch and the space after it.
const SWATCH_COLS: usize = 2;
/// Blank columns between the label column and the bar area.
const LABEL_GAP: usize = 2;
/// Blank columns between the bar area and the percentage column.
const BAR_GAP: usize = 2;
/// Columns reserved for a percentage such as `100.0%`.
const PERCENT_COLS: usize = 6;
/// Blank columns between the percentage column and the value column.
const VALUE_GAP: usize = 3;
/// The narrowest bar area worth drawing.
const MIN_BAR_COLS: usize = 4;
/// The narrowest label column worth drawing.
const MIN_LABEL_COLS: usize = 4;
/// The text shown for a chart that declares no slices.
const EMPTY_TEXT: &str = "(no data)";

/// Renders a pie chart into a canvas exactly `width` columns wide.
///
/// Slices are sorted by descending value; ties keep declaration order, so the output
/// is a deterministic function of the input.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when `width` leaves no room for a legible
/// chart, which happens below roughly twenty columns.
pub fn draw(chart: &PieChart, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    let body = if chart.slices.is_empty() {
        empty_body(width, theme)
    } else {
        plot(chart, width, theme)?
    };
    chrome::compose(chart.title.as_deref(), &body, width, theme)
}

/// The placeholder plot used when a chart declares no slices at all.
fn empty_body(width: u16, theme: &Theme) -> Canvas {
    let text = chrome::fit(EMPTY_TEXT, usize::from(width));
    let cols = u16::try_from(display_width(&text)).unwrap_or(0);
    let mut body = Canvas::new(cols, 0, theme.base());
    body.push_text(&text, Align::Left, theme.text.dim);
    body
}

/// The column budget of every part of one chart row.
#[derive(Debug, Clone, Copy)]
struct Columns {
    label: usize,
    bar: usize,
    value: usize,
}

impl Columns {
    /// The total content width of a chart row.
    fn total(self) -> usize {
        let mut total = SWATCH_COLS + self.label + LABEL_GAP + self.bar + BAR_GAP + PERCENT_COLS;
        if self.value > 0 {
            total += VALUE_GAP + self.value;
        }
        total
    }

    /// The column the bar area starts at.
    fn bar_start(self) -> usize {
        SWATCH_COLS + self.label + LABEL_GAP
    }

    /// The column the percentage column starts at.
    fn percent_start(self) -> usize {
        self.bar_start() + self.bar + BAR_GAP
    }

    /// The column the value column starts at.
    fn value_start(self) -> usize {
        self.percent_start() + PERCENT_COLS + VALUE_GAP
    }
}

/// Chooses how many columns each part of a row gets, degrading as the budget shrinks.
///
/// The order of sacrifice is: give the bar its share, then squeeze the label column,
/// then drop the raw value column, and only then give up.
fn negotiate(chart: &PieChart, width: u16, values: &[String]) -> Result<Columns, MermaidError> {
    let budget = usize::from(width);
    let natural_label = chart
        .slices
        .iter()
        .map(|slice| display_width(&slice.label))
        .max()
        .unwrap_or(0)
        .max(display_width("Total"));
    let natural_value = if chart.show_data {
        values
            .iter()
            .map(String::as_str)
            .map(display_width)
            .max()
            .unwrap_or(0)
    } else {
        0
    };

    // A label column never takes more than two fifths of the width.
    let label_cap = (budget * 2 / 5).max(MIN_LABEL_COLS);
    for value in [natural_value, 0] {
        for label in (MIN_LABEL_COLS..=natural_label.min(label_cap)).rev() {
            let columns = Columns {
                label,
                bar: MIN_BAR_COLS,
                value,
            };
            if let Some(slack) = budget.checked_sub(columns.total()) {
                return Ok(Columns {
                    bar: MIN_BAR_COLS + slack,
                    ..columns
                });
            }
        }
        if natural_label < MIN_LABEL_COLS {
            let columns = Columns {
                label: natural_label,
                bar: MIN_BAR_COLS,
                value,
            };
            if let Some(slack) = budget.checked_sub(columns.total()) {
                return Ok(Columns {
                    bar: MIN_BAR_COLS + slack,
                    ..columns
                });
            }
        }
    }
    Err(MermaidError::TooNarrow { width })
}

/// Draws the slice rows, the summary rule and the total row.
fn plot(chart: &PieChart, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    let slices = sorted_slices(chart);
    let total: f64 = slices.iter().map(|slice| slice.value).sum();
    let largest = slices.first().map_or(0.0, |slice| slice.value);

    let values: Vec<String> = slices
        .iter()
        .map(|slice| format_value(slice.value))
        .collect();
    let total_value = format_value(total);
    let mut widest = values.clone();
    widest.push(total_value.clone());
    let columns = negotiate(chart, width, &widest)?;

    let content = u16::try_from(columns.total()).unwrap_or(u16::MAX);
    let base = theme.base();
    let mut body = Canvas::new(content, slices.len(), base);

    for (index, slice) in slices.iter().enumerate() {
        let accent = theme.accent(index);
        body.write_str(index, 0, SWATCH, accent);
        body.write_field(
            index,
            SWATCH_COLS,
            columns.label,
            &chrome::fit(&slice.label, columns.label),
            Align::Left,
            theme.diagram.legend,
        );
        // Bars are scaled against the largest slice, so the biggest one always fills
        // the plot area and the shape of the distribution is readable at any width.
        let fraction = if largest > 0.0 {
            slice.value / largest
        } else {
            0.0
        };
        let bar = chrome::eighth_bar(chrome::eighths_of(fraction, columns.bar), columns.bar);
        body.write_str(index, columns.bar_start(), &bar, accent);
        write_share(
            &mut body,
            index,
            &columns,
            slice.value,
            total,
            theme.diagram.axis,
        );
        if columns.value > 0 {
            body.write_field(
                index,
                columns.value_start(),
                columns.value,
                &values[index],
                Align::Right,
                theme.text.dim,
            );
        }
    }

    if slices.len() > 1 {
        let rule = body.push_blank_row(base);
        body.hline(
            rule,
            SWATCH_COLS,
            usize::from(content) - SWATCH_COLS,
            "─",
            theme.diagram.axis,
        );
        let row = body.push_blank_row(base);
        body.write_field(
            row,
            SWATCH_COLS,
            columns.label,
            &chrome::fit("Total", columns.label),
            Align::Left,
            theme.diagram.legend,
        );
        write_share(&mut body, row, &columns, total, total, theme.diagram.axis);
        if columns.value > 0 {
            body.write_field(
                row,
                columns.value_start(),
                columns.value,
                &total_value,
                Align::Right,
                theme.text.dim,
            );
        }
    }

    Ok(body)
}

/// Writes the percentage cell of one row.
fn write_share(
    body: &mut Canvas,
    row: usize,
    columns: &Columns,
    value: f64,
    total: f64,
    style: Style,
) {
    body.write_field(
        row,
        columns.percent_start(),
        PERCENT_COLS,
        &format_percent(value, total),
        Align::Right,
        style,
    );
}

/// Slices sorted by descending value, ties keeping declaration order.
fn sorted_slices(chart: &PieChart) -> Vec<&PieSlice> {
    let mut slices: Vec<&PieSlice> = chart.slices.iter().collect();
    // `sort_by` is stable, and `total_cmp` is a total order even for NaN, so equal
    // values keep their declaration order and the result is fully deterministic.
    slices.sort_by(|a, b| b.value.total_cmp(&a.value));
    slices
}

/// Formats a slice value, dropping a pointless fractional part.
fn format_value(value: f64) -> String {
    if !value.is_finite() {
        return "—".to_string();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{value:.0}");
    }
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Formats `value` as a percentage of `total`, guarding against an empty chart.
fn format_percent(value: f64, total: f64) -> String {
    if total <= 0.0 || !total.is_finite() || !value.is_finite() {
        return "0.0%".to_string();
    }
    format!("{:.1}%", value / total * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::grapheme_width;

    #[test]
    fn swatch_is_one_column_wide() {
        assert_eq!(grapheme_width(SWATCH), 1);
    }

    #[test]
    fn values_are_formatted_without_noise() {
        assert_eq!(format_value(245.0), "245");
        assert_eq!(format_value(2.5), "2.5");
        assert_eq!(format_value(2.125), "2.13");
        assert_eq!(format_value(f64::INFINITY), "—");
    }

    #[test]
    fn percentages_survive_an_empty_total() {
        assert_eq!(format_percent(1.0, 0.0), "0.0%");
        assert_eq!(format_percent(1.0, 4.0), "25.0%");
    }
}
