//! Sub-cell bar fills: the eighth-block glyphs and the fraction arithmetic.
//!
//! A terminal can show eighths of a column, which is what lets a pie slice, a gantt bar
//! and the status-bar position meter all end somewhere other than a cell boundary. The
//! glyph table and the fraction-to-eighths rounding were written twice — once in the
//! TUI chrome and once in the Mermaid chart furniture — with different float types and
//! different clamping.
//!
//! They live here because neither owner is the right home for the other: a status-bar
//! meter is not chart furniture, and a chart should not depend on the TUI. This is the
//! shared layer both can reach.

/// Left-growing block elements, indexed by how many eighths of a cell are filled.
///
/// Index `0` is empty and index `8` is a full block. Note that index `0` is the empty
/// *string*, not a space: callers that want a blank cell should say so, and callers
/// building a run want nothing at all.
pub const EIGHTH_BLOCKS: [&str; 9] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

/// The glyph an unfilled meter track is drawn with.
///
/// A gauge needs a track: left blank, an empty meter reads as a hole rather than as
/// "nothing yet".
pub const TROUGH: &str = "░";

/// Converts a `0.0..=1.0` fraction of `cells` columns into eighths of a cell.
///
/// Values outside the range, and non-finite values, are clamped, so a degenerate input
/// — a zero total, a `NaN` from dividing nothing by nothing — cannot produce a bar of
/// nonsensical length.
pub fn eighths_of(fraction: f64, cells: usize) -> usize {
    if !fraction.is_finite() || fraction <= 0.0 {
        return 0;
    }
    let eighths = (fraction.min(1.0) * (cells as f64) * 8.0).round();
    // Clamped and finite, so this cannot exceed `cells * 8`.
    eighths.max(0.0) as usize
}

/// Renders a bar `eighths` eighths of a cell long, at most `max_cells` columns wide.
///
/// A non-zero length always produces at least one visible glyph, so a tiny but present
/// value never disappears entirely — a slice worth 0.4% of a pie should still be seen.
pub fn eighth_bar(eighths: usize, max_cells: usize) -> String {
    let clamped = eighths.min(max_cells.saturating_mul(8));
    let full = clamped / 8;
    let rest = clamped % 8;
    let mut out = "█".repeat(full);
    if rest > 0 && full < max_cells {
        out.push_str(EIGHTH_BLOCKS[rest]);
    } else if clamped == 0 && eighths > 0 && max_cells > 0 {
        out.push_str(EIGHTH_BLOCKS[1]);
    }
    out
}

/// A meter split into the three runs it is drawn in, left to right.
///
/// They are separate because each takes a different style, and in particular because
/// the partial cell needs *both* colours at once — see [`meter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meter {
    /// The whole cells that are filled, drawn in the fill colour alone.
    pub full: String,
    /// The one cell that is part filled, or empty when the value lands on a boundary.
    ///
    /// A left-aligned eighth block paints only its left fraction; the rest of the cell
    /// shows through as background, so this run must be drawn with the fill colour in
    /// front of the *track* colour.
    pub partial: String,
    /// The cells past the value, drawn in the track colour alone.
    pub trough: String,
}

/// Splits a meter `width` cells wide into its filled, part-filled and unfilled runs.
///
/// The three are returned separately because they are drawn in three different styles:
/// the fill colour, the fill colour over the track colour, and the track colour. That
/// middle style is the whole reason this is not a two-way split — an eighth block
/// covers only the left fraction of its cell, so drawing it over anything but the
/// track leaves a hole exactly where the eye reads the value off.
///
/// Their display widths always add up to exactly `width`, so a caller can write them
/// one after the other and land where it expected.
pub fn meter(fraction: f64, width: usize) -> Meter {
    let bar = eighth_bar(eighths_of(fraction, width), width);
    // `eighth_bar` emits full blocks followed by at most one partial block, so the
    // boundary is simply the first glyph that is not a full block.
    let split = bar.find(|glyph| glyph != '\u{2588}').unwrap_or(bar.len());
    let (full, partial) = bar.split_at(split);
    let drawn = crate::text::display_width(&bar);
    Meter {
        full: full.to_string(),
        partial: partial.to_string(),
        trough: TROUGH.repeat(width.saturating_sub(drawn)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{display_width, grapheme_width};

    #[test]
    fn every_glyph_is_one_column_wide() {
        for glyph in EIGHTH_BLOCKS.iter().skip(1).chain(std::iter::once(&TROUGH)) {
            assert_eq!(grapheme_width(glyph), 1, "{glyph:?} must be one column");
        }
    }

    #[test]
    fn eighths_of_clamps_degenerate_input() {
        // A non-finite fraction is meaningless rather than "full", so it draws nothing
        // — an empty bar is a far better failure than a bar of arbitrary length.
        assert_eq!(eighths_of(f64::NAN, 10), 0);
        assert_eq!(eighths_of(f64::INFINITY, 10), 0);
        assert_eq!(eighths_of(f64::NEG_INFINITY, 10), 0);
        assert_eq!(eighths_of(-1.0, 10), 0);
        assert_eq!(eighths_of(2.0, 10), 80);
        assert_eq!(eighths_of(0.5, 10), 40);
        assert_eq!(eighths_of(0.5, 0), 0);
    }

    #[test]
    fn eighth_bar_respects_its_budget() {
        assert_eq!(eighth_bar(0, 4), "");
        assert_eq!(eighth_bar(8, 4), "█");
        assert_eq!(eighth_bar(12, 4), "█▌");
        assert_eq!(eighth_bar(999, 3), "███");
        assert_eq!(eighth_bar(1, 4), "▏", "a tiny value still shows");
        assert_eq!(eighth_bar(4, 0), "");
    }

    #[test]
    fn a_meter_always_fills_exactly_its_width() {
        for width in 0..12usize {
            for percent in 0..=100 {
                let bar = meter(f64::from(percent) / 100.0, width);
                assert_eq!(
                    display_width(&bar.full)
                        + display_width(&bar.partial)
                        + display_width(&bar.trough),
                    width,
                    "{percent}% of {width}"
                );
            }
        }
    }

    #[test]
    fn the_partial_run_is_one_cell_at_most_and_never_a_full_block() {
        // The whole point of the split is that this run alone needs the track colour
        // behind it, so it must hold exactly the cell that is part filled: no full
        // blocks (they would then be drawn over a track they completely hide) and
        // never more than one cell (there is only ever one boundary).
        for width in 0..12usize {
            for permille in 0..=1000 {
                let bar = meter(f64::from(permille) / 1000.0, width);
                assert!(
                    display_width(&bar.partial) <= 1,
                    "{permille}‰ of {width}: {:?}",
                    bar.partial
                );
                assert!(
                    !bar.partial.contains('\u{2588}'),
                    "{permille}‰ of {width}: {:?}",
                    bar.partial
                );
                assert!(
                    bar.full.chars().all(|glyph| glyph == '\u{2588}'),
                    "{permille}‰ of {width}: {:?}",
                    bar.full
                );
            }
        }
    }

    #[test]
    fn a_meter_splits_a_fractional_cell_out_of_the_fill() {
        // 3/8 of the way across four cells is one and a half cells: one full block,
        // then a half block that must not be lumped in with it.
        let bar = meter(0.375, 4);
        assert_eq!(bar.full, "█");
        assert_eq!(bar.partial, "▌");
        assert_eq!(bar.trough, TROUGH.repeat(2));
    }

    #[test]
    fn a_full_meter_has_no_trough_and_an_empty_one_is_all_trough() {
        let bar = meter(1.0, 6);
        assert_eq!(bar.full, "██████");
        assert!(bar.partial.is_empty());
        assert!(bar.trough.is_empty());

        let bar = meter(0.0, 6);
        assert!(bar.full.is_empty());
        assert!(bar.partial.is_empty());
        assert_eq!(bar.trough, TROUGH.repeat(6));
    }
}
