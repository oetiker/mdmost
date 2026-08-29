// SPDX-License-Identifier: MIT
//! Placing a display formula: centred if it fits, side-scrolled if it does not.
//!
//! The counterpart to [`super::diagram`], and nearly the same shape so that `document.rs`
//! places both the same way. Two differences, and both are simplifications.
//!
//! **A formula has exactly one width.** A diagram can be re-laid out narrower, so
//! `diagram` searches for the narrowest width that works and spends a probe budget doing
//! it. This module measures once: [`MathError::TooWide`] carries the answer rather than a
//! hint to search from. [`Limits::probes`](super::diagram::Limits) is therefore ignored
//! here, and that is deliberate rather than an oversight — the type is shared so the
//! pager's ceiling stays in one place.
//!
//! **[`formula`] takes one parameter `diagram` does not** — the frame to centre in. A
//! diagram is never centred and so has nothing to be told about; a formula is, and the
//! frame is not derivable from the width it is laid out at (see [`centred`]). The
//! signatures have not drifted apart by neglect.
//!
//! Centring is design spec §7, and it is the only centring in this program. It applies
//! only where there is room: a formula wider than the frame is left-aligned, because
//! centring something that is about to be side-scrolled would put its left edge off screen
//! before the reader had seen it.
//!
//! **[`formula`] answers for every formula that builds**, not only for the wide ones, and
//! that is what makes it the single rule deciding a display formula's width. Returning
//! `None` for one that fits would hand it to `document.rs`'s clip search, which widens
//! whatever it is given without consulting [`MIN_SURPLUS`] — so a formula three columns
//! past the measure used to give the whole document a horizontal scrollbar, which is the
//! exact thing `MIN_SURPLUS` was written to prevent. `None` is left for the cases where
//! the formula does not build at all: there, what clips is the reader's own broken LaTeX,
//! and widening a source dump so they can read it is the existing, correct code-block
//! behaviour.

use crate::canvas::Canvas;
use crate::doc::{Node, NodeKind};
use crate::error::MathError;
use crate::theme::{Style, Theme};

use super::diagram::Limits;
use super::document::MIN_SURPLUS;
use super::{Ctx, RenderOptions, bridge, code};

/// Centres `canvas` within `frame`, on a canvas `width` columns wide.
///
/// Design spec §7, and the only centring in this program — both callers that centre a
/// formula call this, so `centring_is_the_only_place_in_this_program_that_centres` is
/// literally true and worth having as a guard.
///
/// **The frame is the prose measure, not the width being laid out into.** A formula is
/// centred in the column of prose it belongs to. On a wide terminal the prose cap
/// (`body_width`, default 72) is narrower than the body, and centring in the body would
/// float the formula off to the right of the text it belongs with. The two numbers are
/// [`Measure::prose` and `Measure::full`](super::document); passing the frame is what keeps
/// "how wide is this laid out" and "what is it centred in" two questions, which is the
/// whole reason a formula is exempt from the cap without being centred against the body.
///
/// Three cases fall out of the arithmetic and none of them is special-cased:
///
/// * narrower than the frame — centred in it, and padded out to `width` on the right;
/// * wider than the frame but within `width` — left-aligned, because there is no slack to
///   centre in and it has already left the prose column;
/// * wider than `width` — left-aligned at its own width, for the side-scroll to pick up.
///
/// The odd column goes to the right.
pub(crate) fn centred(canvas: &Canvas, width: u16, frame: u16, fill: Style) -> Canvas {
    let room = frame.min(width);
    let slack = room.saturating_sub(canvas.width());
    let left = slack / 2;
    let right = width.saturating_sub(canvas.width()).saturating_sub(left);
    canvas.indent(left, right, fill)
}

/// The formula this block draws, and the width it needed.
///
/// The counterpart to [`diagram`](super::diagram::diagram), and used the same way: the
/// caller lays the block out at the width that comes back, and side-scrolls to it if that
/// is wider than the viewport.
///
/// The canvas is a document block exactly that many columns wide, assembled the way
/// [`render_block`](super::render_block) assembles one at that width — centred when it
/// fits, framed source when it does not.
///
/// `None` only when the node is not display math, or when the formula does not build:
/// [`MathError::Parse`] and [`MathError::NotDrawable`]. Those keep the ordinary block path
/// deliberately — see this module's header. Every formula that *does* build is answered
/// for here, including the ones that are not widened, because "do not widen this" is an
/// answer the caller cannot express any other way: falling through would put it into the
/// clip search, and the clip search would widen it.
pub(crate) fn formula(
    node: &Node,
    from: u16,
    frame: u16,
    limits: Limits,
    theme: &Theme,
    options: &RenderOptions,
) -> Option<(u16, Canvas)> {
    let NodeKind::Math {
        literal,
        display: true,
    } = &node.kind
    else {
        return None;
    };
    let source = literal.trim_matches('\n');
    let ctx = Ctx::new(theme, options);
    // Measured once, at its own width. The error carries the answer, not a hint to search
    // from, so there is no probe loop here and `limits.probes` goes unread.
    match bridge::math_natural(source, limits.width(), theme) {
        // It fits: the width it is laid out at is the width it was asked for, which makes
        // `at == width` at the call site and spends one layout where the clip search spent
        // seven.
        Ok(canvas) if canvas.width() <= from => {
            Some((from, centred(&canvas, from, frame, ctx.base)))
        }
        // Wider than the measure but the surplus earns the room: the canvas is its own
        // width and the caller side-scrolls to it. Centring is a no-op at that width, and
        // is left in rather than branched around so there is one path through this.
        Ok(canvas) if canvas.width() - from >= MIN_SURPLUS => {
            let at = canvas.width();
            Some((at, centred(&canvas, at, frame, ctx.base)))
        }
        // Wider, but not by enough to be worth a horizontal scrollbar on every row of the
        // document. The framed source at the measure is the answer, and returning it — as
        // opposed to `None` — is what stops the clip search from widening it anyway.
        Ok(canvas) => Some((
            from,
            code::fallback(
                source,
                Some("math"),
                &MathError::TooWide {
                    needed: canvas.width(),
                },
                &[],
                from,
                ctx,
            ),
        )),
        // Past the ceiling: crossing a formula three screens wide is not reading it, and
        // `Canvas::append` would pad every row of the document to it. Same answer, and it
        // is the reason `TooWide`'s caption reaches a reader at all.
        Err(MathError::TooWide { needed }) => Some((
            from,
            code::fallback(
                source,
                Some("math"),
                &MathError::TooWide { needed },
                &[],
                from,
                ctx,
            ),
        )),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Doc;

    /// A formula that draws one column wide.
    const NARROW: &str = "$$\\frac{a}{b}$$";
    /// A formula that draws 25 columns wide.
    const MID: &str = "$$\\frac{a + b + c + d + e + f + g}{2}$$";
    /// A formula that draws 41 columns wide.
    const WIDE: &str = "$$\\frac{a + b + c + d + e + f + g + h + i + j + k}{2}$$";

    /// A formula 89 columns wide: past the default prose cap of 72, inside a 120-column body.
    fn past_the_prose_cap() -> String {
        let terms: Vec<String> = ('a'..='w').map(|c| c.to_string()).collect();
        format!("$$\\frac{{{}}}{{2}}$$", terms.join(" + "))
    }

    fn theme() -> Theme {
        Theme::default()
    }

    fn node(doc: &Doc) -> &Node {
        &doc.root().children[0]
    }

    /// How many blank columns a row opens with.
    fn indent_of(canvas: &Canvas, row: usize) -> usize {
        let text = canvas.row_text(row);
        text.len() - text.trim_start().len()
    }

    /// The row the fraction rule is drawn on.
    fn rule_row(canvas: &Canvas) -> usize {
        (0..canvas.height())
            .find(|row| canvas.row_text(*row).contains('─'))
            .expect("a rule row")
    }

    /// The arithmetic in every test below is written against these three numbers, so they
    /// are asserted once here rather than being re-derived — and a spacing change that
    /// moves them fails one test with a number in the message instead of five with
    /// mysterious off-by-a-column assertions.
    #[test]
    fn the_widths_these_tests_are_written_against() {
        let theme = theme();
        for (src, expected) in [
            (NARROW, 1u16),
            (MID, 25),
            (WIDE, 41),
            (past_the_prose_cap().as_str(), 89),
        ] {
            let literal = src.trim_matches('$');
            let canvas = bridge::math_natural(literal, u16::MAX, &theme)
                .unwrap_or_else(|err| panic!("{literal} does not draw: {err}"));
            assert_eq!(canvas.width(), expected, "the drawn width of {literal}");
        }
    }

    #[test]
    fn a_formula_that_fits_is_centred_in_the_measure() {
        let doc = Doc::parse(NARROW);
        let canvas =
            super::super::render_block(node(&doc), 40, &theme(), &RenderOptions::default());
        assert_eq!(
            canvas.width(),
            40,
            "a block is exactly the width it was asked for"
        );
        let row = rule_row(&canvas);
        let text = canvas.row_text(row);
        let left = text.len() - text.trim_start().len();
        let right = text.len() - text.trim_end().len();
        assert!(
            left.abs_diff(right) <= 1,
            "not centred: {left} left, {right} right"
        );
        // Not merely balanced — the odd column goes right, so this is the only pair that
        // is both balanced and correct. A canvas drawn at column 0 gives 0 and 39.
        assert_eq!((left, right), (19, 20));
    }

    #[test]
    fn centring_is_the_only_place_in_this_program_that_centres() {
        // A guard, not a behaviour: if this ever fails, someone centred something else
        // and the rule in the design spec needs revisiting rather than the test deleting.
        let doc = Doc::parse("Just a paragraph of prose.");
        let canvas =
            super::super::render_block(node(&doc), 40, &theme(), &RenderOptions::default());
        assert!(
            canvas.row_text(0).starts_with("Just"),
            "prose is not centred"
        );
    }

    #[test]
    fn a_formula_is_centred_in_the_prose_column_not_the_body() {
        // The owner's ruling: a formula belongs to the column of prose around it. At a
        // 118-column body under the default 72-column prose cap, centring in the body
        // would put this 35 columns further right than the text it explains.
        let doc = Doc::parse(NARROW);
        let (at, canvas) = formula(
            node(&doc),
            118,
            72,
            Limits::new(354, 1),
            &theme(),
            &RenderOptions::default(),
        )
        .expect("a formula that builds is always answered for");
        assert_eq!(at, 118, "it fits, so it asks for the width it was given");
        assert_eq!(canvas.width(), 118);
        assert_eq!(indent_of(&canvas, rule_row(&canvas)), 35, "centred in 72");
        assert_ne!(
            indent_of(&canvas, rule_row(&canvas)),
            58,
            "centred in the body, which is the thing the ruling forbids"
        );
        canvas.check_invariants().expect("width holds");
    }

    #[test]
    fn a_formula_wider_than_the_prose_column_is_left_aligned_in_the_body() {
        // 89 columns of formula in a 72-column text column: there is no slack to centre
        // in, and it has already left the column, so it starts where every other block
        // starts. This is the case that a `slack / 2` with no `frame` would get wrong in
        // the other direction, by centring it in 118.
        let doc = Doc::parse(&past_the_prose_cap());
        let (at, canvas) = formula(
            node(&doc),
            118,
            72,
            Limits::new(354, 1),
            &theme(),
            &RenderOptions::default(),
        )
        .expect("a formula that builds is always answered for");
        assert_eq!(at, 118, "89 fits inside 118, so no widening is asked for");
        assert_eq!(canvas.width(), 118);
        assert_eq!(indent_of(&canvas, rule_row(&canvas)), 0, "left-aligned");
    }

    #[test]
    fn a_formula_wider_than_the_measure_is_left_aligned_at_its_own_width() {
        let doc = Doc::parse(WIDE);
        let (at, canvas) = formula(
            node(&doc),
            20,
            20,
            Limits::new(200, 1),
            &theme(),
            &RenderOptions::default(),
        )
        .expect("wants a wider canvas");
        assert_eq!(at, 41, "it asked for the width it draws at");
        assert_eq!(canvas.width(), at);
        assert_eq!(indent_of(&canvas, rule_row(&canvas)), 0, "left-aligned");
        canvas.check_invariants().expect("width holds");
    }

    #[test]
    fn a_surplus_of_two_is_not_worth_a_scrollbar_on_every_row() {
        // 25 columns of formula wanted, 23 available. Neither the `MIN_SURPLUS` guard nor
        // the clip search was covered at a surplus this small, which is why a formula used
        // to be widened here in defiance of the rule that exists to stop it.
        let doc = Doc::parse(MID);
        let (at, canvas) = formula(
            node(&doc),
            23,
            23,
            Limits::new(200, 1),
            &theme(),
            &RenderOptions::default(),
        )
        .expect("answered for, and the answer is 'do not widen this'");
        assert_eq!(at, 23, "the measure, not the 25 it would have liked");
        assert_eq!(canvas.width(), 23);
        let text = canvas.plain_text();
        assert!(
            text.contains("\\frac"),
            "the framed source, which the drawn form never contains: {text}"
        );
        // Twenty-three columns is too narrow for the whole caption, so the frame elides
        // it; what matters here is that the reader is told a width was the problem.
        assert!(text.contains("this formula needs"), "captioned: {text}");
    }

    #[test]
    fn the_caption_names_the_width_the_formula_wanted() {
        // `MathError::TooWide`'s message was unreachable before this task: the clip search
        // widened every formula until it drew, so nothing ever came back captioned at all.
        // This is the route by which a reader sees it. Thirty-six columns is wide enough
        // to print it whole, and a surplus of five is still under `MIN_SURPLUS`.
        let doc = Doc::parse(WIDE);
        let (at, canvas) = formula(
            node(&doc),
            36,
            36,
            Limits::new(200, 1),
            &theme(),
            &RenderOptions::default(),
        )
        .expect("answered for");
        assert_eq!(at, 36, "a surplus of five does not earn the room");
        let text = canvas.plain_text();
        assert!(
            text.contains("this formula needs 41 columns"),
            "the caption carries the answer: {text}"
        );
    }

    #[test]
    fn a_formula_past_the_ceiling_gets_the_framed_source_not_a_wider_canvas() {
        // Past the ceiling the source dump is the better answer: scrolling a hundred
        // columns is not reading. Answering `None` here would hand it to the clip search,
        // which would draw it at 25 anyway and make the ceiling a dead letter.
        let doc = Doc::parse(MID);
        let (at, canvas) = formula(
            node(&doc),
            20,
            20,
            Limits::new(24, 1),
            &theme(),
            &RenderOptions::default(),
        )
        .expect("answered for");
        assert_eq!(at, 20);
        assert_eq!(canvas.width(), 20);
        assert!(canvas.plain_text().contains("\\frac"), "the framed source");
    }

    #[test]
    fn a_node_that_is_not_display_math_is_not_this_functions_business() {
        let doc = Doc::parse("Just prose.");
        assert!(
            formula(
                node(&doc),
                20,
                20,
                Limits::new(200, 1),
                &theme(),
                &RenderOptions::default(),
            )
            .is_none()
        );
    }

    #[test]
    fn a_formula_that_will_not_build_keeps_the_ordinary_path() {
        // The reader's own broken LaTeX is what clips here, and widening a source dump so
        // they can read it is the code-block behaviour that already exists.
        const BROKEN: &str = "$$\\frac{a$$";
        let theme = theme();
        assert!(
            matches!(
                bridge::math_natural(BROKEN.trim_matches('$'), u16::MAX, &theme),
                Err(MathError::Parse { .. })
            ),
            "the premise of this test: this formula does not build"
        );
        let doc = Doc::parse(BROKEN);
        assert!(
            formula(
                node(&doc),
                20,
                20,
                Limits::new(200, 1),
                &theme,
                &RenderOptions::default(),
            )
            .is_none()
        );
    }

    #[test]
    fn a_definition_only_formula_still_contributes_no_rows() {
        // Design spec §16.3. It builds, so it is answered for; it draws nothing, so the
        // centring has nothing to centre and must not invent a row to put it on.
        let doc = Doc::parse("$$\\newcommand{\\x}{y}$$");
        let (at, canvas) = formula(
            node(&doc),
            40,
            40,
            Limits::new(200, 1),
            &theme(),
            &RenderOptions::default(),
        )
        .expect("it builds");
        assert_eq!(at, 40);
        assert_eq!(canvas.height(), 0, "no rows");
    }
}
