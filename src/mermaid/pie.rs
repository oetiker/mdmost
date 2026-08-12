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
use crate::text::{Align, display_width, ellipsize};
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
        chrome::placeholder(EMPTY_TEXT, width, theme)
    } else {
        plot(chart, width, theme)?
    };
    chrome::compose(chart.title.as_deref(), &body, width, theme)
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
        .map(|slice| display_width(&chrome::label_one_line(&slice.label)))
        .max()
        .unwrap_or(0)
        .max(display_width("Total"));
    let natural_value = if chart.show_data {
        chrome::lines_width(values)
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
    Err(MermaidError::TooNarrow {
        width,
        needed: None,
    })
}

/// Draws the slice rows, the summary rule and the total row.
fn plot(chart: &PieChart, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    let slices = sorted_slices(chart);
    let total: f64 = slices.iter().map(|slice| slice.value).sum();
    let largest = slices.first().map_or(0.0, |slice| slice.value);

    let shares = apportion(&slices);
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
        // On the canvas's own surface, not on nothing. `Theme::accent` is a bare
        // foreground, and `Canvas::write_str` *replaces* a cell's style rather than
        // patching it, so an accent used raw hands the bar cells `bg: None` — and a
        // bar's last cell is an eighth block, which inks only part of its cell and
        // shows that missing background through the rest. It happens to look right
        // today because the TUI blit patches `theme.base()` in behind every cell, but
        // that is a safety net elsewhere, not a property of this bar: draw the same
        // chart onto a striped table row and the seam appears.
        let accent = base.patch(theme.accent(index));
        body.write_str(index, 0, SWATCH, accent);
        let drawn = ellipsize(&chrome::label_one_line(&slice.label), columns.label);
        body.write_field(
            index,
            SWATCH_COLS,
            columns.label,
            &drawn,
            Align::Left,
            theme.diagram.legend,
        );
        chrome::label_row_span(&mut body, &slice.label, &drawn, index, SWATCH_COLS);
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
            shares[index],
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

    if !slices.is_empty() {
        let rule = body.push_blank_row(base);
        // Inset at both ends. It used to start at the label column and run to the
        // very edge, which read as a rule that had overshot rather than one that
        // belonged to the columns above it.
        body.hline(
            rule,
            SWATCH_COLS,
            usize::from(content).saturating_sub(SWATCH_COLS * 2),
            "─",
            theme.diagram.axis,
        );
        let row = body.push_blank_row(base);
        body.write_field(
            row,
            SWATCH_COLS,
            columns.label,
            &ellipsize("Total", columns.label),
            Align::Left,
            theme.diagram.legend,
        );
        // The apportioned shares are exact by construction, so the total row is the
        // real sum of the column above it, not an assumed `100.0%`.
        write_share(
            &mut body,
            row,
            &columns,
            shares.iter().sum::<u32>(),
            theme.diagram.axis,
        );
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

/// Writes the percentage cell of one row from a share in tenths of a percent.
fn write_share(body: &mut Canvas, row: usize, columns: &Columns, tenths: u32, style: Style) {
    body.write_field(
        row,
        columns.percent_start(),
        PERCENT_COLS,
        &format_percent(tenths),
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

/// Rounds `value` to `decimals` places, half away from zero.
///
/// This is the rule a reader expects — `0.125` shows as `0.13`, not `0.12` — and it is
/// chosen deliberately rather than inherited from the round-half-to-even that `{:.2}`
/// applies to whichever binary value happens to be nearest the decimal literal.
fn round_half_away(value: f64, decimals: i32) -> f64 {
    let factor = 10f64.powi(decimals);
    // `f64::round` is defined as half away from zero, which is exactly the rule we
    // want; doing it here means the later `{:.n}` has nothing left to round.
    (value * factor).round() / factor
}

/// Formats a slice value, dropping a pointless fractional part.
fn format_value(value: f64) -> String {
    if !value.is_finite() {
        return "—".to_string();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{value:.0}");
    }
    let text = format!("{:.2}", round_half_away(value, 2));
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Formats a share given in tenths of a percent, e.g. `412` as `41.2%`.
fn format_percent(tenths: u32) -> String {
    format!("{}.{}%", tenths / 10, tenths % 10)
}

/// Splits `1000` tenths of a percent across the slices by largest remainder.
///
/// Rounding every slice independently is a real reporting bug: four slices of one
/// seventh each print as `14.3%` and the column sums to `100.4%`. The largest-remainder
/// (Hamilton) method instead hands the leftover tenths to the slices with the biggest
/// truncated fractions, so **the printed percentages always sum to exactly `100.0%`**.
/// Ties are broken by position, so the result stays deterministic.
///
/// How finely a remainder is measured before slices are considered tied.
///
/// Nine digits is far beyond any real chart's precision and far short of the point
/// where a `f64` division's last bits start to matter.
const TIE_SCALE: f64 = 1e9;

/// A chart whose values sum to zero (or to something non-finite) gets all-zero shares
/// rather than a division by zero; its total row then honestly reads `0.0%`.
fn apportion(slices: &[&PieSlice]) -> Vec<u32> {
    let total: f64 = slices.iter().map(|slice| slice.value).sum();
    if !total.is_finite() || total <= 0.0 {
        return vec![0; slices.len()];
    }
    let exact: Vec<f64> = slices
        .iter()
        .map(|slice| {
            if slice.value.is_finite() && slice.value > 0.0 {
                slice.value / total * 1000.0
            } else {
                0.0
            }
        })
        .collect();
    // `exact` values are non-negative and sum to at most 1000, so every floor fits.
    let mut shares: Vec<u32> = exact.iter().map(|share| *share as u32).collect();
    let assigned: u32 = shares.iter().sum();
    let mut leftover = 1000u32.saturating_sub(assigned);

    // Largest remainder wins the leftover tenths. Remainders are quantised before
    // they are compared, because two shares that are mathematically the same fraction
    // — three slices of a third each, say — come out of the division differing in
    // their last bits. Without the quantisation the declaration-order tie-break below
    // could never fire, and the leftovers would land on whichever slice happened to
    // round up. Quantising keeps the comparison a total order, which a tolerance
    // would not.
    let quantised = |index: usize| -> i64 {
        let fraction = exact[index] - exact[index].floor();
        (fraction * TIE_SCALE).round() as i64
    };
    let mut order: Vec<usize> = (0..slices.len()).collect();
    order.sort_by_key(|&index| (std::cmp::Reverse(quantised(index)), index));
    for index in order {
        if leftover == 0 {
            break;
        }
        shares[index] += 1;
        leftover -= 1;
    }
    shares
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::grapheme_width;

    #[test]
    fn swatch_is_one_column_wide() {
        assert_eq!(grapheme_width(SWATCH), 1);
    }

    /// Builds slices from bare values; labels do not affect any arithmetic here.
    fn slices(values: &[f64]) -> Vec<PieSlice> {
        values
            .iter()
            .map(|value| PieSlice {
                label: crate::mermaid::ast::Label::default(),
                value: *value,
            })
            .collect()
    }

    /// Applies [`apportion`] to a list of values.
    fn shares(values: &[f64]) -> Vec<u32> {
        let owned = slices(values);
        apportion(&owned.iter().collect::<Vec<_>>())
    }

    #[test]
    fn rounding_is_half_away_from_zero() {
        // The rule, asserted as a rule: an exact half always rounds away from zero,
        // in both directions and at both precisions we use. Every case below is a
        // dyadic rational, so the tie really is a tie — a literal such as `2.135` is
        // stored as slightly *less* than 2.135 and is correctly not a tie at all.
        for (value, decimals, expected) in [
            (2.125f64, 2, 2.13f64),
            (0.375, 2, 0.38),
            (-2.125, 2, -2.13),
            (-0.375, 2, -0.38),
            (0.25, 1, 0.3),
            (0.75, 1, 0.8),
            (-0.25, 1, -0.3),
        ] {
            let rounded = round_half_away(value, decimals);
            assert!(
                (rounded - expected).abs() < 1e-9,
                "round_half_away({value}, {decimals}) = {rounded}, expected {expected}"
            );
        }
    }

    #[test]
    fn values_are_formatted_without_noise() {
        assert_eq!(format_value(245.0), "245");
        assert_eq!(format_value(2.5), "2.5");
        assert_eq!(format_value(2.125), "2.13");
        assert_eq!(format_value(2.126), "2.13");
        assert_eq!(format_value(2.124), "2.12");
        assert_eq!(format_value(f64::INFINITY), "—");
    }

    #[test]
    fn percentages_are_formatted_from_tenths() {
        assert_eq!(format_percent(0), "0.0%");
        assert_eq!(format_percent(250), "25.0%");
        assert_eq!(format_percent(412), "41.2%");
        assert_eq!(format_percent(1000), "100.0%");
    }

    #[test]
    fn shares_always_sum_to_exactly_one_hundred_percent() {
        // Seven equal slices are the classic counter-example: rounding each one
        // independently gives 14.3% × 7 = 100.1%.
        for values in [
            vec![1.0; 7],
            vec![1.0; 3],
            vec![1.0; 6],
            vec![2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            vec![99.0, 1.0],
            vec![1e-9, 1.0],
            vec![5.0],
        ] {
            let apportioned = shares(&values);
            assert_eq!(
                apportioned.iter().sum::<u32>(),
                1000,
                "shares for {values:?} were {apportioned:?}"
            );
        }
    }

    #[test]
    fn shares_stay_close_to_the_exact_value() {
        // Largest remainder never moves a slice by more than one tenth of a percent.
        let values = [3.0, 3.0, 3.0, 1.0];
        let total: f64 = values.iter().sum();
        for (share, value) in shares(&values).iter().zip(values) {
            let exact = value / total * 1000.0;
            assert!(
                (f64::from(*share) - exact).abs() <= 1.0,
                "share {share} is too far from {exact}"
            );
        }
    }

    #[test]
    fn an_empty_total_apportions_nothing_rather_than_dividing_by_zero() {
        assert_eq!(shares(&[0.0, 0.0, 0.0]), vec![0, 0, 0]);
        assert_eq!(shares(&[]), Vec::<u32>::new());
    }

    #[test]
    fn apportionment_is_deterministic_for_ties() {
        // Equal values must always hand the leftover tenth to the earliest slice.
        for _ in 0..8 {
            assert_eq!(shares(&[1.0; 3]), vec![334, 333, 333]);
        }
    }

    #[test]
    fn slices_with_the_same_fraction_tie_by_declaration_order() {
        // These are not equal values, but they are equal *fractions*: each is a
        // third of the total. The division leaves them differing in their last bits,
        // so before the remainders were quantised the declaration-order tie-break
        // could never fire and the leftover landed on an arbitrary slice.
        assert_eq!(shares(&[2.0, 2.0, 2.0]), vec![334, 333, 333]);
        assert_eq!(shares(&[1.0, 2.0, 3.0]), vec![167, 333, 500]);
        // The shares always account for exactly one thousand tenths of a percent.
        for values in [
            vec![1.25, 0.5, 0.13],
            vec![7.0, 11.0, 13.0],
            vec![0.1, 0.2, 0.3, 0.4],
        ] {
            assert_eq!(shares(&values).iter().sum::<u32>(), 1000, "{values:?}");
        }
    }
}
