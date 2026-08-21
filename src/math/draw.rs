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

// Nothing outside this module's own tests calls these yet: the renderer is wired onto the
// drawer in a later task, so the lib target sees both entry points as dead while the test
// target sees them live. `expect` cannot express that -- it fires
// `unfulfilled_lint_expectations` on the test target -- so this is `allow`, and it comes
// out with the ones in `boxes.rs`, `build.rs` and `spacing.rs` when the renderer calls in.
//
// Measured, not assumed: with this line removed, clippy reports exactly five warnings --
// `to_row`, `write_flat`, `to_canvas`, `place` and `centre` never used. `MAX_DEPTH` is not
// among them, because the `const _` assertion below is a live item that reads it.
#![allow(dead_code)]

use crate::canvas::Canvas;
use crate::error::MathError;
use crate::math::boxes::{BoxContent, MathBox};
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
/// `width` therefore yields a wider canvas, and the caller compares.
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
        // Task 7 fills in `Radical` and Task 8 fills in `Fenced`. Drawing nothing is the
        // honest placeholder: the box already reserved the space, so a missing radical
        // shows as a gap rather than as misplaced neighbours.
        BoxContent::Radical { .. } | BoxContent::Fenced { .. } => {}
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

    /// The one shape where the two walks are allowed to disagree, and why.
    ///
    /// Deliberate staging, not a bug: `place`'s `Fenced` arm is Task 8's, and `build.rs`
    /// still refuses `\left(…\right)` by name, so no shipped state can render a blank
    /// fence. This test exists so that the gap is a written-down fact with an owner
    /// rather than a silence. **Task 8 must make it fail, and must delete it** — that
    /// failure is Task 8's acceptance criterion.
    #[test]
    fn the_canvas_side_of_a_one_row_fence_is_still_blank_until_task_8() {
        let theme = Theme::default();
        let b = fenced(Some('('), Some(')'), text("x"));
        assert!(b.is_inline(), "a one-row body keeps the fence on one row");

        assert_eq!(
            to_row(&b).expect("is inline"),
            "(x)",
            "the flat walk draws the delimiters today"
        );
        assert_eq!(
            to_canvas(&b, b.width, &theme).row_text(0),
            "   ",
            "TASK 8: `place` does not draw `Fenced` yet, so the canvas is the three \
             columns the box reserved and nothing in them. When Task 8 writes that arm \
             this assert must fail -- delete this test and let \
             `the_flat_walk_and_the_canvas_walk_render_the_same_cells` cover the fence \
             instead. See the module doc: one tree, one rendering."
        );

        // Deliberately NOT asserted here: the same fence inside a row, where the gap must
        // stay a hole rather than shift the neighbour. That is the reviewer's D5, ruled to
        // land with the arm it protects rather than beside this canary.
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
