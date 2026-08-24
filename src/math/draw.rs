// SPDX-License-Identifier: MIT
//! A box tree onto cells.
//!
//! Two outputs from one tree. [`to_row`] writes the inline form as a single string,
//! because an inline formula is placed as a run of text inside a paragraph and a one-row
//! canvas would gain nothing. [`to_canvas`] draws the display form.
//!
//! Placement is by baseline. Every box knows how many rows it has above and below the row
//! the reader's eye is on, so drawing is one recursive walk carrying an origin: the row
//! the child's baseline lands on, and the column its left edge starts at. Nothing here
//! decides *where on the page* the formula goes — that is design spec §7 and belongs to
//! the renderer, which is the only thing that knows the measure.
//!
//! Both walks carry a depth and stop at [`MAX_DEPTH`]. `build.rs` already refuses source
//! nested past its own cap, but a box tree is deeper than the source that produced it and
//! this module's entry points take a `&MathBox` from anywhere, so the bound has to be
//! checked here too. An unbounded walk on a hostile formula overflows the stack, and that
//! aborts the process rather than raising the error design spec §9 asks for.

use crate::canvas::Canvas;
use crate::error::MathError;
use crate::math::boxes::{BoxContent, MathBox};
use crate::math::delim;
use crate::theme::Theme;

/// How many levels of box either walk descends before it stops drawing.
///
/// Deeper than any tree `build.rs` can hand over. Its cap is 64 levels of *source*
/// grouping, and 64 nested groups measure as a box tree 66 deep; the constructs that
/// arrive in later tasks turn one source level into a handful of box levels rather than
/// into hundreds, so the ceiling stays in this range.
///
/// And far below where the walk runs out of stack. [`place`] is the binding one — its
/// frame is the larger — and on a debug build 3200 levels of nested fraction draw while
/// 3400 abort. [`write_flat`] reaches an order of magnitude further, past the depth at
/// which dropping the tree overflows on its own.
const MAX_DEPTH: usize = 512;

const _: () = assert!(
    MAX_DEPTH <= 1024,
    "MAX_DEPTH must stay far below the measured overflow: 3200 levels draw, 3400 abort"
);

/// The inline form: one row of text.
///
/// # Errors
///
/// [`MathError::NotInline`] if the box needs more than the row the prose sits on. The
/// check is here rather than at the call site so the constraint of design spec §4 cannot
/// be forgotten by a future caller.
pub(crate) fn to_row(b: &MathBox) -> Result<String, MathError> {
    if !b.is_inline() {
        return Err(MathError::NotInline("this formula"));
    }
    let mut out = String::new();
    write_flat(b, &mut out, 0);
    Ok(out)
}

/// Appends a zero-height box's cells to `out`.
fn write_flat(b: &MathBox, out: &mut String, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match &b.content {
        BoxContent::Text(s) => out.push_str(s),
        BoxContent::Row(parts) => {
            for part in parts {
                write_flat(part, out, depth.saturating_add(1));
            }
        }
        BoxContent::Fenced { left, right, body } => {
            if let Some(c) = left {
                out.push(*c);
            }
            write_flat(body, out, depth.saturating_add(1));
            if let Some(c) = right {
                out.push(*c);
            }
        }
        // A `Scripts` or `Limits` whose operands are both `None` is zero-height, so
        // `is_inline` let it through and its base must still be written -- which is what
        // `place` does. A present operand always costs a row (`boxes::scripts` takes the
        // script's whole `height()`, which is at least 1, into `above`/`below`, and
        // `boxes::limits` adds it), so under `is_inline` both are `None` and the base is
        // the whole box.
        BoxContent::Scripts { base, .. } | BoxContent::Limits { base, .. } => {
            write_flat(base, out, depth.saturating_add(1));
        }
        // A `Fraction`'s `above` is `num.height()` and a `Radical`'s is its radicand's
        // `above` plus the overline row, and a height is at least 1 -- so neither can
        // ever be zero-height and `is_inline` has already rejected them. Checked against
        // `boxes::fraction` and `boxes::radical`, not assumed.
        BoxContent::Fraction { .. } | BoxContent::Radical { .. } => {}
    }
}

/// The display form, drawn into a canvas at least `width` columns wide.
///
/// `width` is a floor, not a cap: a formula has exactly one width (design spec §7) and
/// clipping it is the renderer's decision, made where the measure is known. A narrower
/// `width` therefore yields a wider canvas.
///
/// No caller takes that branch today. [`crate::math::render_display`] is the only one
/// outside this module's own tests, and it returns [`MathError::TooWide`] before calling,
/// so through it the canvas is always exactly `width` — asserted, not argued, by the
/// display proptest. The floor stays because this function is handed a *box* and cannot
/// know whether its caller means to scroll; deciding that here would put the renderer's
/// policy in the drawer.
pub(crate) fn to_canvas(b: &MathBox, width: u16, theme: &Theme) -> Canvas {
    let width = width.max(b.width);
    let mut canvas = Canvas::new(width, usize::from(b.height()), theme.base());
    // The baseline row of the whole formula. Everything below is relative to it.
    place(b, &mut canvas, i32::from(b.above), 0, theme, 0);
    canvas
}

/// Draws `b` with its baseline on `baseline` and its left edge at `col`.
fn place(b: &MathBox, canvas: &mut Canvas, baseline: i32, col: u16, theme: &Theme, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let deeper = depth.saturating_add(1);
    match &b.content {
        BoxContent::Text(s) => {
            // A negative baseline is a row above the canvas, reachable only after a `u16`
            // saturation upstream; skipping the draw clips it as the canvas would.
            if let Ok(row) = usize::try_from(baseline) {
                canvas.write_str(row, usize::from(col), s, theme.base());
            }
        }
        BoxContent::Row(parts) => {
            let mut at = col;
            for part in parts {
                place(part, canvas, baseline, at, theme, deeper);
                at = at.saturating_add(part.width);
            }
        }
        BoxContent::Fraction { num, den } => {
            // The rule is the baseline and spans the wider part; each half is centred
            // over or under it. The `if let Ok` is the same off-canvas clip as above.
            if let Ok(row) = usize::try_from(baseline) {
                canvas.hline(
                    row,
                    usize::from(col),
                    usize::from(b.width),
                    "─",
                    theme.base(),
                );
            }
            place(
                num,
                canvas,
                baseline
                    .saturating_sub(i32::from(num.below))
                    .saturating_sub(1),
                col.saturating_add(centre(b.width, num.width)),
                theme,
                deeper,
            );
            place(
                den,
                canvas,
                baseline
                    .saturating_add(i32::from(den.above))
                    .saturating_add(1),
                col.saturating_add(centre(b.width, den.width)),
                theme,
                deeper,
            );
        }
        BoxContent::Scripts { base, sub, sup } => {
            place(base, canvas, baseline, col, theme, deeper);
            let at = col.saturating_add(base.width);
            if let Some(sup) = sup {
                place(
                    sup,
                    canvas,
                    baseline
                        .saturating_sub(i32::from(sup.below))
                        .saturating_sub(1),
                    at,
                    theme,
                    deeper,
                );
            }
            if let Some(sub) = sub {
                place(
                    sub,
                    canvas,
                    baseline
                        .saturating_add(i32::from(sub.above))
                        .saturating_add(1),
                    at,
                    theme,
                    deeper,
                );
            }
        }
        BoxContent::Limits { base, under, over } => {
            place(
                base,
                canvas,
                baseline,
                col.saturating_add(centre(b.width, base.width)),
                theme,
                deeper,
            );
            if let Some(over) = over {
                place(
                    over,
                    canvas,
                    baseline
                        .saturating_sub(i32::from(base.above))
                        .saturating_sub(i32::from(over.below))
                        .saturating_sub(1),
                    col.saturating_add(centre(b.width, over.width)),
                    theme,
                    deeper,
                );
            }
            if let Some(under) = under {
                place(
                    under,
                    canvas,
                    baseline
                        .saturating_add(i32::from(base.below))
                        .saturating_add(i32::from(under.above))
                        .saturating_add(1),
                    col.saturating_add(centre(b.width, under.width)),
                    theme,
                    deeper,
                );
            }
        }
        BoxContent::Radical { radicand, index } => {
            // The index's last column is the stroke's *first* column, so everything past
            // the index's own first column hangs off to the left and pushes the stroke
            // right. `boxes::radical` reserved exactly this much and nothing here may
            // disagree with it.
            let overhang = index.as_ref().map_or(0, |i| i.width.saturating_sub(1));
            let stroke = col.saturating_add(overhang);
            let bottom = baseline.saturating_add(i32::from(b.below));
            // The row the overline or the cap sits on, and it is measured from the
            // RADICAND, not from the top of the box. Those are the same row only while
            // the index fits under the stroke's own ascent; once `boxes::radical` grows
            // `above` to hold a taller index, `baseline - b.above` is the top of the
            // *index* and drawing the overline there would strand it several rows clear
            // of what it is supposed to cover.
            let stroke_top = baseline
                .saturating_sub(i32::from(radicand.above))
                .saturating_sub(1);

            let radicand_col = if radicand.is_inline() {
                // The plain sign on the baseline, and the overline on the row above it.
                // The overline spans the radicand alone: drawn to `b.width` it would
                // reach back over the sign itself.
                if let Ok(row) = usize::try_from(baseline) {
                    canvas.write_str(row, usize::from(stroke), "√", theme.base());
                }
                if let Ok(row) = usize::try_from(stroke_top) {
                    canvas.hline(
                        row,
                        usize::from(stroke.saturating_add(2)),
                        usize::from(radicand.width),
                        "─",
                        theme.base(),
                    );
                }
                stroke.saturating_add(2)
            } else {
                // Owner's ruling, 2026-08-22: a vertical stem capped by a square right
                // angle, with the tick beside the diagonal on the bottom row. Columns
                // from `stroke`: tick, diagonal, stem, gap, then the radicand.
                //
                // The corner sits ON the stem column, so the overline meets it and
                // covers the gap column as well -- `radicand.width + 1` columns starting
                // at the gap, not `radicand.width` as in the one-row form above.
                let stem = stroke.saturating_add(2);
                if let Ok(row) = usize::try_from(stroke_top) {
                    canvas.write_str(row, usize::from(stem), "┌", theme.base());
                    canvas.hline(
                        row,
                        usize::from(stem.saturating_add(1)),
                        usize::from(radicand.width).saturating_add(1),
                        "─",
                        theme.base(),
                    );
                }
                for r in stroke_top.saturating_add(1)..=bottom {
                    if let Ok(row) = usize::try_from(r) {
                        canvas.write_str(row, usize::from(stem), "│", theme.base());
                    }
                }
                if let Ok(row) = usize::try_from(bottom) {
                    canvas.write_str(row, usize::from(stroke), "‾", theme.base());
                    canvas.write_str(
                        row,
                        usize::from(stroke.saturating_add(1)),
                        "╲",
                        theme.base(),
                    );
                }
                stroke.saturating_add(4)
            };

            if let Some(index) = index {
                // Two rules, one for each axis, and both hold for an index of any size.
                //
                // Column: the index's rightmost column is the stroke's first -- over the
                // `√` in the one-row form, over the tick in the tall one, the tick being
                // the top of the short initial stroke. So it starts one past the stroke
                // and backs up by its own width, which is the overhang `boxes::radical`
                // reserved.
                //
                // Row: the index's LAST row is one above the bottom row, so its baseline
                // is that row less its own descent. On a one-row root with a one-row
                // index the two rules are exactly `["3 ─", "√ x"]`; a taller index grows
                // upwards from the same anchor, into the rows `boxes::radical` added to
                // `above` for it.
                place(
                    index,
                    canvas,
                    bottom
                        .saturating_sub(1)
                        .saturating_sub(i32::from(index.below)),
                    stroke.saturating_add(1).saturating_sub(index.width),
                    theme,
                    deeper,
                );
            }
            place(radicand, canvas, baseline, radicand_col, theme, deeper);
        }
        BoxContent::Fenced { left, right, body } => {
            // The columns `boxes::fenced` charged for, walked left to right. Each side
            // that HAS a delimiter costs one column for the delimiter and, against a tall
            // body, one more for the space between it and the content. A side that is
            // `None` is a `\left.` and costs nothing at all, so the advance is per side
            // rather than a constant -- `\left[\frac{a}{b}\right.` is three columns wide,
            // not five, and advancing by a constant would put the body one column right of
            // where the box reserved it and overrun the canvas.
            let padded = !body.is_inline();
            let side_cost = 1u16.saturating_add(u16::from(padded));
            let top = baseline.saturating_sub(i32::from(b.above));
            let height = b.height();

            let mut at = col;
            if let Some(delimiter) = left {
                write_delimiter(canvas, *delimiter, top, height, at, theme);
                at = at.saturating_add(side_cost);
            }
            place(body, canvas, baseline, at, theme, deeper);
            if let Some(delimiter) = right {
                let at = at
                    .saturating_add(body.width)
                    .saturating_add(u16::from(padded));
                write_delimiter(canvas, *delimiter, top, height, at, theme);
            }
        }
    }
}

/// Writes one delimiter down the rows the enclosure spans.
///
/// `top` is the enclosure's own top row, so the delimiter runs the full height of what it
/// encloses: `boxes::fenced` copies the body's `above` and `below` unchanged, which is the
/// statement that a fence is exactly as tall as its content and no taller.
fn write_delimiter(
    canvas: &mut Canvas,
    delimiter: char,
    top: i32,
    height: u16,
    col: u16,
    theme: &Theme,
) {
    // `char::encode_utf8` writes into this, so the `&str` the canvas wants costs a stack
    // buffer rather than a `String` per row.
    let mut buf = [0u8; 4];
    for (offset, piece) in delim::pieces(delimiter, height).into_iter().enumerate() {
        // The same off-canvas clip the other arms make: a negative row is only reachable
        // after a `u16` saturation upstream, and skipping the draw clips it as the canvas
        // would.
        let Ok(offset) = i32::try_from(offset) else {
            continue;
        };
        let Ok(row) = usize::try_from(top.saturating_add(offset)) else {
            continue;
        };
        canvas.write_str(
            row,
            usize::from(col),
            piece.encode_utf8(&mut buf),
            theme.base(),
        );
    }
}

/// The left offset that centres `content` in `field`, rounding left.
///
/// Rounding left rather than right so that a one-column overhang falls on the side the
/// reader's eye starts from, which is the same choice `canvas::align_offset`
/// (`src/canvas/mod.rs:771`) makes for a centred table cell.
const fn centre(field: u16, content: u16) -> u16 {
    field.saturating_sub(content) / 2
}

#[cfg(test)]
mod tests {
    use super::{MAX_DEPTH, to_canvas, to_row};
    use crate::math::boxes::{MathBox, fenced, fraction, limits, radical, row, scripts, text};
    use crate::text::display_width;
    use crate::theme::Theme;

    /// The canvas as one string per row, trailing blanks trimmed, for readable asserts.
    fn rows(c: &crate::canvas::Canvas) -> Vec<String> {
        (0..c.height())
            .map(|r| c.row_text(r).trim_end().to_string())
            .collect()
    }

    #[test]
    fn a_flat_row_draws_as_its_own_text() {
        let b = row(vec![text("E"), text(" = "), text("mc")]);
        assert_eq!(to_row(&b).expect("is inline"), "E = mc");
    }

    #[test]
    fn a_box_that_needs_a_second_row_refuses_the_inline_path() {
        // Two rows is the boundary and the only value that says where the refusal is
        // drawn. A three-row fraction is rejected by any threshold between one and three,
        // so on its own it pins nothing: `!b.is_inline()` and `b.height() > 2` agree on
        // it. They disagree on a superscript, which is two rows, and under the looser
        // test `write_flat` would return the base alone -- a formula that quietly loses
        // its exponent instead of raising `NotInline`.
        let two_rows = scripts(text("x"), None, Some(text("2")));
        assert_eq!(two_rows.height(), 2, "the boundary case must be two rows");
        assert!(
            to_row(&two_rows).is_err(),
            "one row over the prose line is already too many"
        );

        let b = fraction(text("a"), text("b"));
        assert!(to_row(&b).is_err(), "the constraint cannot be bypassed");
    }

    #[test]
    fn a_fraction_draws_the_rule_on_the_baseline_with_both_parts_centred() {
        let theme = Theme::default();
        let b = fraction(text("-b + d"), text("2a"));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["-b + d", "──────", "  2a"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn the_rule_spans_the_denominator_when_that_is_the_wider_part() {
        // The other fraction asserts here all have the wider part on top, where the rule
        // drawn to the numerator's width and the rule drawn to the box's width are the
        // same run of cells. Only a bottom-heavy fraction tells the two apart.
        let theme = Theme::default();
        let b = fraction(text("a"), text("bcdef"));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["  a", "─────", "bcdef"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn an_odd_overhang_falls_to_the_left() {
        // Every other centring assert here has an even slack, where rounding left and
        // rounding right land on the same column. Four columns over one leaves three, so
        // this is the only shape that says which way `centre` goes: one blank before the
        // `x`, not two. `canvas::align_offset` (`src/canvas/mod.rs:771`) rounds the same
        // way for a centred table cell.
        let theme = Theme::default();
        let b = fraction(text("abcd"), text("x"));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["abcd", "────", " x"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_canvas_is_padded_to_the_width_it_was_asked_for() {
        let theme = Theme::default();
        let b = fraction(text("a"), text("b"));
        let canvas = to_canvas(&b, 20, &theme);
        assert_eq!(canvas.width(), 20);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_formula_wider_than_the_floor_keeps_its_own_width() {
        let theme = Theme::default();
        let b = fraction(text("a very long numerator"), text("b"));
        let canvas = to_canvas(&b, 10, &theme);
        assert_eq!(
            canvas.width(),
            b.width,
            "the caller asked for a floor, not a cap"
        );
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_superscript_sits_on_the_row_above_the_base() {
        let theme = Theme::default();
        let b = scripts(text("x"), None, Some(text("2")));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec![" 2", "x"]);
    }

    #[test]
    fn a_subscript_sits_on_the_row_below_the_base() {
        let theme = Theme::default();
        let b = scripts(text("x"), Some(text("i")), None);
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["x", " i"]);
    }

    #[test]
    fn limits_stack_over_and_under_and_are_centred_on_the_operator() {
        let theme = Theme::default();
        let b = limits(text("∑"), Some(text("i=1")), Some(text("n")));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec![" n", " ∑", "i=1"]);
    }

    #[test]
    fn a_one_row_radicand_keeps_the_plain_sign_and_an_overline() {
        let theme = Theme::default();
        let b = radical(text("b + 4"), None);
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["  ─────", "√ b + 4"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn the_one_row_overline_spans_exactly_the_radicand() {
        // The stroke column and the gap after it are not under the overline. A rule
        // drawn to the box's width instead of the radicand's -- which is what the
        // `Fraction` arm one screen up does -- would reach back over the `√` itself,
        // and only a radicand narrower than the box tells the two apart.
        let theme = Theme::default();
        let b = radical(text("xy"), None);
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["  ──", "√ xy"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_one_row_root_index_sits_over_the_sign() {
        let theme = Theme::default();
        let b = radical(text("x"), Some(text("3")));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["3 ─", "√ x"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_tall_radicand_takes_a_stem_capped_by_a_right_angle() {
        // The owner's ruling of 2026-08-22. Columns: tick, diagonal, stem, gap,
        // radicand -- and the tick and the diagonal are drawn on the bottom row only,
        // so the two columns left of the stem are blank everywhere else.
        let theme = Theme::default();
        let b = radical(fraction(text("a"), text("b")), None);
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["  ┌──", "  │ a", "  │ ─", "‾╲│ b"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_five_row_radicand_draws_the_stem_as_a_stem() {
        // A three-row radicand has exactly one row between the cap and the bottom, so
        // it cannot tell a stem from a single middle row. Five rows can.
        let theme = Theme::default();
        let b = radical(fraction(fraction(text("a"), text("b")), text("cd")), None);
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(
            rows(&canvas),
            vec!["  ┌───", "  │ a", "  │ ─", "  │ b", "  │ ──", "‾╲│ cd"]
        );
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_tall_root_index_sits_over_the_tick() {
        // Ruled CROOK-TICK, owner, 2026-08-22: the index is right-aligned so its last
        // column is the tick's, on the row one above the bottom. The tick is the top of
        // the short initial stroke, which is where the index sits above `√` in the
        // one-row form -- the same rule transcribed, not a second one invented.
        let theme = Theme::default();
        let b = radical(fraction(text("a"), text("b")), Some(text("3")));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["  ┌──", "  │ a", "3 │ ─", "‾╲│ b"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_wide_tall_root_index_overhangs_and_carries_the_stroke_right() {
        // The accepted cost of the ruling: a two-column index widens the tall box by
        // one column, the same charge the one-row root already takes. The diagonal's
        // column stays clear of the index in both forms.
        let theme = Theme::default();
        let b = radical(fraction(text("a"), text("b")), Some(text("10")));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["   ┌──", "   │ a", "10 │ ─", " ‾╲│ b"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_tall_index_stacks_up_from_the_row_above_the_bottom() {
        // The arm's own test, without the parser. Both placement rules are visible here
        // and neither is the one-row case in disguise: the index's rightmost column is
        // the `√`'s column, and its LAST row -- the `b` -- is one above the bottom row,
        // with the rest of it growing upwards into the rows `boxes::radical` reserved.
        //
        // The overline is the other half. It sits beside the radicand it covers, on
        // `baseline - radicand.above - 1`, and not at the top of the box: the index is
        // two rows taller than the stroke here, so drawing it at `baseline - b.above`
        // would strand it on row 0 with nothing under it.
        let theme = Theme::default();
        let b = radical(text("x"), Some(fraction(text("a"), text("b"))));
        assert_eq!((b.width, b.above, b.below), (3, 3, 0));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["a", "─", "b ─", "√ x"]);
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn the_baseline_of_a_nested_box_is_the_baseline_of_the_whole() {
        // a + 1/2 + b: the a and the b sit on the fraction's rule row, not above it.
        let theme = Theme::default();
        let b = row(vec![
            text("a + "),
            fraction(text("1"), text("2")),
            text(" + b"),
        ]);
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["    1", "a + ─ + b", "    2"]);
    }

    /// The `Row` arm must advance by the whole of a radical, stroke and overhang and all.
    ///
    /// This discharges defect D5 from the Task 4 review. The `Row` arm advances by
    /// `part.width` and nothing checks that the columns it skipped are the columns the
    /// part actually used, so an arm that reserves more than it draws -- or a row that
    /// advanced by what it *drew* rather than by what the box *measured* -- would put
    /// the neighbour in the wrong place with no test to say so. Until now no test put a
    /// radical inside a row at all.
    ///
    /// Both shapes are here because they charge different amounts. The indexed one-row
    /// root reserves an overhang column that sits left of the sign, so a row that
    /// advanced by `radicand.width + STROKE` would land `b` one column early; the tall
    /// root's stroke is four columns rather than two, so a row that assumed a constant
    /// stroke would land it two columns early.
    #[test]
    fn a_row_advances_across_a_radical_by_the_width_the_box_reserved() {
        let theme = Theme::default();

        let indexed = row(vec![
            text("a"),
            radical(text("x"), Some(text("10"))),
            text("b"),
        ]);
        assert_eq!(
            indexed.width, 6,
            "1 + (1 radicand + 2 stroke + 1 overhang) + 1"
        );
        let canvas = to_canvas(&indexed, indexed.width, &theme);
        assert_eq!(
            rows(&canvas),
            vec![" 10 ─", "a √ xb"],
            "`b` must start at column 5, past the overhang as well as the stroke"
        );
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");

        let tall = row(vec![
            text("a"),
            radical(fraction(text("x"), text("y")), None),
            text("b"),
        ]);
        assert_eq!(tall.width, 7, "1 + (1 radicand + 4 stroke) + 1");
        let canvas = to_canvas(&tall, tall.width, &theme);
        assert_eq!(
            rows(&canvas),
            vec!["   ┌──", "   │ x", "a  │ ─b", " ‾╲│ y"],
            "`b` must start at column 6, past the four-column stroke"
        );
        canvas
            .check_invariants()
            .expect("exactly width columns on every row");
    }

    #[test]
    fn a_tree_deeper_than_the_cap_stops_drawing_instead_of_overflowing_the_stack() {
        let theme = Theme::default();
        // Nested fractions, each one box level. The innermost `z` is the marker: if the
        // walk reached it, it is on the canvas.
        let nest = |levels: usize| {
            let mut b = text("z");
            for _ in 0..levels {
                b = fraction(text("a"), b);
            }
            b
        };

        let shallow = nest(MAX_DEPTH / 2);
        assert!(
            to_canvas(&shallow, 1, &theme).plain_text().contains('z'),
            "a tree inside the cap must be drawn all the way down"
        );

        // Measured on a debug build: 3400 levels abort the process on a stack overflow
        // with the depth check removed, which no `Result` can catch. This asserts the
        // return as much as the content -- reaching the assertion at all is the point.
        let deep = nest(4000);
        assert!(
            !to_canvas(&deep, 1, &theme).plain_text().contains('z'),
            "past the cap the walk must stop rather than descend"
        );
    }

    #[test]
    fn the_flat_walk_stops_at_the_cap_as_well() {
        // `to_row` recurses through `Row` on its own path, so it needs its own bound.
        // One box level per nesting, and the `z` again says how far the walk got.
        let nest = |levels: usize| {
            let mut b = text("z");
            for _ in 0..levels {
                b = row(vec![b]);
            }
            b
        };

        assert_eq!(
            to_row(&nest(MAX_DEPTH / 2)).expect("is inline"),
            "z",
            "a tree inside the cap is flattened all the way down"
        );
        assert_eq!(
            to_row(&nest(MAX_DEPTH + 8)).expect("is inline"),
            "",
            "past the cap the flat walk stops rather than descend"
        );
    }

    #[test]
    fn every_canvas_this_module_makes_holds_its_invariants() {
        let theme = Theme::default();
        for b in [
            text("x"),
            row(vec![]),
            fraction(text("a"), text("bcdef")),
            scripts(text("e"), Some(text("i")), Some(text("2n"))),
            limits(text("∫"), Some(text("0")), Some(text("∞"))),
        ] {
            let canvas = to_canvas(&b, 40, &theme);
            canvas
                .check_invariants()
                .unwrap_or_else(|e| panic!("{b:?} broke the canvas contract: {e}"));
        }
    }

    /// One tree, one rendering: `to_row` and row 0 of `to_canvas` must be the same cells.
    ///
    /// This is the ONE ENGINE ruling, and until now it was only a sentence in the module
    /// doc — each walk was tested against its own expectations and neither against the
    /// other. `write_flat` and `place` are separate matches over the same enum, so every
    /// task that adds an arm to one of them can silently disagree with the other.
    ///
    /// The padding is handled by pinning it to zero rather than by a `trim_end()`. A
    /// zero-height box has exactly one row and the canvas is asked for the box's own
    /// width, so the flat string fills the row edge to edge and the two can be compared
    /// raw. Asserting `display_width(flat) == b.width` first is what makes that safe: if
    /// a later arm reserves columns it does not write, that assert names the shortfall,
    /// where a `trim_end()` would swallow it along with any real trailing-space
    /// difference in the drawn row.
    #[test]
    fn the_flat_walk_and_the_canvas_walk_render_the_same_cells() {
        let theme = Theme::default();
        let cases: Vec<(&str, MathBox)> = vec![
            ("plain text", text("E = mc")),
            (
                "a row of parts",
                row(vec![text("E"), text(" = "), text("mc")]),
            ),
            ("an empty row", row(vec![])),
            (
                "scripts with neither operand",
                scripts(text("x"), None, None),
            ),
            ("limits with neither operand", limits(text("∑"), None, None)),
            (
                "a row holding both bare-base shapes",
                row(vec![
                    text("a"),
                    scripts(text("x"), None, None),
                    limits(text("∑"), None, None),
                    text("b"),
                ]),
            ),
            (
                "nested bare bases",
                scripts(
                    limits(scripts(text("q"), None, None), None, None),
                    None,
                    None,
                ),
            ),
            // The fence, inherited from the canary this test replaced. Both walks are
            // exercised on each combination of present and absent delimiter, because a
            // one-row fence pays a *per-side* cost and the two-sided case is the one
            // where a constant advance and a per-side advance agree by accident.
            ("a one-row fence", fenced(Some('('), Some(')'), text("x"))),
            (
                "a fence with only an opening delimiter",
                fenced(Some('['), None, text("x")),
            ),
            (
                "a fence with only a closing delimiter",
                fenced(None, Some(']'), text("x")),
            ),
            (
                "a fence with neither, which is `\\left. … \\right.`",
                fenced(None, None, text("x")),
            ),
            (
                "a fence inside a row",
                row(vec![
                    text("a"),
                    fenced(Some('('), Some(')'), text("x")),
                    text("b"),
                ]),
            ),
            // A delimiter with no box-art form, on the walk that compares the two
            // renderings cell by cell. `write_flat` pushes the author's own character, so
            // this is where a substituting `place` would be caught: drawing `│` for `⌊`
            // makes the two walks disagree, by design.
            (
                "a fence whose delimiters have no box-art form",
                fenced(Some('⌊'), Some('⌋'), text("x")),
            ),
        ];

        for (what, b) in cases {
            assert!(b.is_inline(), "{what}: the case itself must be zero-height");
            let flat = to_row(&b).expect("is inline");
            assert_eq!(
                display_width(&flat),
                usize::from(b.width),
                "{what}: the flat form must fill the width the box reserved, \
                 so that the canvas row carries no padding to strip"
            );
            let canvas = to_canvas(&b, b.width, &theme);
            assert_eq!(canvas.height(), 1, "{what}: a zero-height box is one row");
            assert_eq!(
                canvas.row_text(0),
                flat,
                "{what}: the two walks must draw the same cells"
            );
        }
    }

    #[test]
    fn a_one_row_fence_takes_the_plain_characters() {
        let theme = Theme::default();
        let b = fenced(Some('('), Some(')'), text("x + y"));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["(x + y)"]);
        canvas.check_invariants().expect("width holds");
    }

    #[test]
    fn a_tall_fence_grows_box_art_and_pads_off_the_content() {
        let theme = Theme::default();
        let b = fenced(Some('('), Some(')'), fraction(text("a"), text("b")));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["╭ a ╮", "│ ─ │", "╰ b ╯"]);
        canvas.check_invariants().expect("width holds");
    }

    /// The other three presence combinations, drawn rather than reasoned about.
    ///
    /// A two-sided fence is the case where a per-side advance and a constant one agree,
    /// so on its own it pins neither. These three tell them apart: with one side absent
    /// the box is narrower by that side's whole cost, and the body sits where the
    /// narrower box put it.
    #[test]
    fn a_tall_fence_charges_and_draws_only_the_sides_that_are_there() {
        let theme = Theme::default();

        let left_only = fenced(Some('['), None, fraction(text("a"), text("b")));
        assert_eq!(left_only.width, 3, "body 1 + one side at 2");
        let canvas = to_canvas(&left_only, left_only.width, &theme);
        assert_eq!(rows(&canvas), vec!["┌ a", "│ ─", "└ b"]);
        canvas.check_invariants().expect("width holds");

        let right_only = fenced(None, Some(']'), fraction(text("a"), text("b")));
        assert_eq!(right_only.width, 3, "body 1 + one side at 2");
        let canvas = to_canvas(&right_only, right_only.width, &theme);
        assert_eq!(rows(&canvas), vec!["a ┐", "─ │", "b ┘"]);
        canvas.check_invariants().expect("width holds");

        // `\left. … \right.`: both delimiters intentionally invisible. The body is drawn
        // flush, and no column anywhere is reserved for a fence that is not there.
        let neither = fenced(None, None, fraction(text("a"), text("b")));
        assert_eq!(neither.width, 1, "an invisible fence costs nothing");
        let canvas = to_canvas(&neither, neither.width, &theme);
        assert_eq!(rows(&canvas), vec!["a", "─", "b"]);
        canvas.check_invariants().expect("width holds");
    }

    #[test]
    fn an_invisible_delimiter_draws_nothing_and_reserves_nothing() {
        let theme = Theme::default();
        let b = fenced(None, Some('}'), fraction(text("a"), text("b")));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(canvas.width(), b.width);
        assert!(rows(&canvas)[0].starts_with('a'), "no left piece was drawn");
        canvas.check_invariants().expect("width holds");
    }

    /// A delimiter is never replaced by a different one — owner's ruling, 2026-08-24.
    ///
    /// `delim.rs` pins this over the whole reachable set, but only in terms of the pieces
    /// it hands back. This is the same rule stated about *cells on a canvas*, which is
    /// the thing the reader actually sees, and it is stated tall as well as one-row:
    /// the one-row form takes the plain character on any path, so a substitution that
    /// only bites when the body grows would survive a one-row test alone.
    #[test]
    fn a_tall_delimiter_with_no_box_art_is_still_the_authors_own_character() {
        let theme = Theme::default();
        let b = fenced(Some('⌊'), Some('⌋'), fraction(text("a"), text("b")));
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec!["⌊ a ⌋", "⌊ ─ ⌋", "⌊ b ⌋"]);
        canvas.check_invariants().expect("width holds");

        let one_row = fenced(Some('⌈'), Some('⌉'), text("x"));
        let canvas = to_canvas(&one_row, one_row.width, &theme);
        assert_eq!(rows(&canvas), vec!["⌈x⌉"]);
        canvas.check_invariants().expect("width holds");
    }

    /// `‖` joins into `║` when it grows, and stays plain on one row.
    ///
    /// Owner's ruling, 2026-08-24, and the case that has to be rendered BOTH ways: three
    /// stacked `‖` read as three norms where one joined double rule reads as one, but a
    /// one-row `\left\|x\right\|` must still be the author's own plain character.
    #[test]
    fn a_double_bar_fence_joins_up_when_it_grows_and_stays_plain_on_one_row() {
        let theme = Theme::default();
        let tall = fenced(Some('‖'), Some('‖'), fraction(text("a"), text("b")));
        let canvas = to_canvas(&tall, tall.width, &theme);
        assert_eq!(rows(&canvas), vec!["║ a ║", "║ ─ ║", "║ b ║"]);
        canvas.check_invariants().expect("width holds");

        let one_row = fenced(Some('‖'), Some('‖'), text("x"));
        let canvas = to_canvas(&one_row, one_row.width, &theme);
        assert_eq!(rows(&canvas), vec!["‖x‖"]);
        canvas.check_invariants().expect("width holds");
    }

    /// Defect D5, in the form the radical test cannot take.
    ///
    /// `a_row_advances_across_a_radical_by_the_width_the_box_reserved` already kills the
    /// "advance by 0" mutation on this same `Row` arm, so a fence repeat of it would pin
    /// nothing new. What it pins instead is that a fence draws *inside* the columns
    /// `boxes::fenced` charged for, with a neighbour hard up against them: the fence here
    /// is one-sided and tall, so its per-side cost is two, and an arm that spent one
    /// column on a padded side draws the body over the neighbour's column.
    ///
    /// Measured, not claimed. With `side_cost` cut to a bare 1 this test goes red along
    /// with five others. It is NOT sensitive to the other half of the per-side rule --
    /// charging for an absent delimiter -- because both of this fence's delimiters that
    /// matter are on the present side; `a_tall_fence_charges_and_draws_only_the_sides_
    /// that_are_there` and the two-walk test are what catch that one.
    #[test]
    fn a_row_advances_across_a_fence_by_the_width_the_box_reserved() {
        let theme = Theme::default();
        let b = row(vec![
            text("a"),
            fenced(Some('('), None, fraction(text("n"), text("m"))),
            text("z"),
        ]);
        assert_eq!(b.width, 5, "1 + (body 1 + one side at 2) + 1");
        let canvas = to_canvas(&b, b.width, &theme);
        assert_eq!(rows(&canvas), vec![" ╭ n", "a│ ─z", " ╰ m"]);
        canvas.check_invariants().expect("width holds");
    }

    /// `Fraction` and `Radical` cannot be zero-height, which is what the `write_flat`
    /// catch-all now claims. Pinned here because the claim is about `boxes.rs`, and the
    /// previous comment asserted a wider invariant that was simply false.
    #[test]
    fn a_fraction_and_a_radical_are_never_inline() {
        assert!(
            !fraction(text(""), text("")).is_inline(),
            "a fraction's `above` is the numerator's height, which is at least 1"
        );
        assert!(
            !radical(text(""), None).is_inline(),
            "a radical's `above` is its radicand's plus the overline row"
        );
    }
}
