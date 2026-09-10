// SPDX-License-Identifier: MIT
//! The box model of design spec §4.
//!
//! Three numbers describe every box, all in terminal cells: `width` in display columns,
//! and `above` / `below` as the rows either side of the baseline. The baseline is the row
//! the surrounding text sits on, which is what lets an inline formula share a row with
//! prose: a box with `above == 0 && below == 0` fits on that row and nothing else does.
//!
//! Composition is arithmetic, and it is all here rather than spread through the builder,
//! because a layout defect shows up as a wrong number long before it shows up as wrong
//! box art. `build.rs` decides *what* to compose; this file decides how big the result is.
//!
//! Every addition saturates. `src/math/` may not panic (design spec §9) and a formula is
//! attacker-supplied text, so a pathological nesting depth has to clamp rather than
//! overflow. A clamped box draws wrongly and is caught by the caller's width check; an
//! overflow aborts the pager.

use crate::text::display_width;

/// A laid-out formula fragment: how much space it takes, and what is in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MathBox {
    /// Display columns.
    pub(crate) width: u16,
    /// Rows above the baseline.
    pub(crate) above: u16,
    /// Rows below the baseline.
    pub(crate) below: u16,
    /// What is drawn.
    pub(crate) content: BoxContent,
}

/// What a [`MathBox`] draws, once [`MathBox`] has said how big it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoxContent {
    /// One row of already-resolved cells.
    Text(String),
    /// A horizontal list sharing one baseline.
    Row(Vec<MathBox>),
    /// Numerator over denominator. The baseline row is the rule, drawn or blank.
    Fraction {
        /// Drawn above the baseline row.
        num: std::boxed::Box<MathBox>,
        /// Drawn below it.
        den: std::boxed::Box<MathBox>,
        /// Whether the baseline row carries a rule.
        ///
        /// False for a binomial coefficient and for anything else the source declared
        /// zero-thickness; the row is still there and still the baseline, it is simply
        /// left blank. See [`fraction_ruleless`].
        rule: bool,
    },
    /// A base with scripts set to its right.
    Scripts {
        /// What is being scripted.
        base: std::boxed::Box<MathBox>,
        /// Below-right, if any.
        sub: Option<std::boxed::Box<MathBox>>,
        /// Above-right, if any.
        sup: Option<std::boxed::Box<MathBox>>,
    },
    /// A base with its scripts stacked over and under it.
    Limits {
        /// The operator, which keeps the baseline.
        base: std::boxed::Box<MathBox>,
        /// Stacked below.
        under: Option<std::boxed::Box<MathBox>>,
        /// Stacked above.
        over: Option<std::boxed::Box<MathBox>>,
    },
    /// A radical: a stroke, an overline, and what is under them.
    Radical {
        /// What the root is taken of.
        radicand: std::boxed::Box<MathBox>,
        /// The index of the root, as in a cube root.
        index: Option<std::boxed::Box<MathBox>>,
    },
    /// A body wrapped in delimiters sized to it.
    Fenced {
        /// The opening delimiter, absent for `\left.`.
        left: Option<char>,
        /// The closing delimiter, absent for `\right.`.
        right: Option<char>,
        /// What is enclosed.
        body: std::boxed::Box<MathBox>,
    },
}

impl MathBox {
    /// Total rows: everything above the baseline, the baseline, everything below.
    pub(crate) const fn height(&self) -> u16 {
        self.above.saturating_add(1).saturating_add(self.below)
    }

    /// Whether this box fits on the row the surrounding prose sits on.
    ///
    /// This is the whole of design spec §4's inline constraint. `build.rs` is the only
    /// caller that acts on it, and it does so by rewriting the box rather than by
    /// handing the formula to different code.
    pub(crate) const fn is_inline(&self) -> bool {
        self.above == 0 && self.below == 0
    }

    /// The cells this box draws, with nothing put between them.
    ///
    /// Design spec §13 asks a narrow question and this answers exactly it:
    /// `crate::math::symbols` walks the *content* events and wants the characters the
    /// document named. A content event becomes a [`BoxContent::Text`], so those two arms
    /// are the whole answer. The rest draw nothing here on purpose, not by omission: a
    /// fraction's slash, a radical's sign and a fence's delimiters are composed by this
    /// crate rather than named by the author, and `tests/glyph_inventory.rs` subtracts
    /// what this returns from what was drawn and claims the remainder.
    pub(crate) fn plain_text(&self) -> String {
        match &self.content {
            BoxContent::Text(cells) => cells.clone(),
            BoxContent::Row(parts) => parts.iter().map(MathBox::plain_text).collect(),
            _ => String::new(),
        }
    }

    /// Whether this box draws no cells at all.
    ///
    /// Design spec §16.3: a display block whose layout produces nothing contributes no
    /// rows to the document — no frame, no caption, no blank line.
    /// `crate::math::render_display` acts on the answer by returning an empty canvas.
    pub(crate) fn is_empty(&self) -> bool {
        self.width == 0 && self.above == 0 && self.below == 0
    }
}

/// One row of cells, measured as the terminal will draw it.
pub(crate) fn text(s: impl Into<String>) -> MathBox {
    let s = s.into();
    MathBox {
        width: u16::try_from(display_width(&s)).unwrap_or(u16::MAX),
        above: 0,
        below: 0,
        content: BoxContent::Text(s),
    }
}

/// A horizontal list: widths add, and each side of the baseline takes the tallest part.
pub(crate) fn row(parts: Vec<MathBox>) -> MathBox {
    let mut width = 0u16;
    let mut above = 0u16;
    let mut below = 0u16;
    for part in &parts {
        width = width.saturating_add(part.width);
        above = above.max(part.above);
        below = below.max(part.below);
    }
    MathBox {
        width,
        above,
        below,
        content: BoxContent::Row(parts),
    }
}

/// Numerator over denominator, the rule spanning the wider part (design spec §6.1).
///
/// The rule row *is* the baseline, so the numerator is entirely above and the denominator
/// entirely below — each contributing its whole height, not just its `above`.
pub(crate) fn fraction(num: MathBox, den: MathBox) -> MathBox {
    stack(num, den, true)
}

/// The same stack with the baseline row left blank: a binomial coefficient.
///
/// RULING, owner, 2026-09-10: a ruleless fraction keeps the geometry of a ruled one, and
/// the rule row is simply not drawn. Nothing else moves, so a fence or radical wrapped
/// round this box reserves exactly what it would reserve for [`fraction`].
///
/// A separate constructor rather than a third parameter on [`fraction`]: the flag is read
/// at one place in the whole crate (`build.rs`'s `Visual::Fraction` arm, from the
/// thickness the parser reports), while `fraction` itself is called from some fifty
/// places, none of which have anything to say about rules. A bool at each of those would
/// be noise standing where a reason should be.
pub(crate) fn fraction_ruleless(num: MathBox, den: MathBox) -> MathBox {
    stack(num, den, false)
}

/// The geometry both fraction constructors share, so it cannot drift between them.
fn stack(num: MathBox, den: MathBox, rule: bool) -> MathBox {
    let width = num.width.max(den.width);
    let above = num.height();
    let below = den.height();
    MathBox {
        width,
        above,
        below,
        content: BoxContent::Fraction {
            num: std::boxed::Box::new(num),
            den: std::boxed::Box::new(den),
            rule,
        },
    }
}

/// A base with scripts to its right (design spec §4, `ScriptPosition::Right`).
///
/// The superscript's lowest row sits one above the base's baseline and the subscript's
/// highest row one below it, so a tall script pushes the box out by its whole height —
/// that one row of offset is already the script's own first row, not clearance on top of it.
pub(crate) fn scripts(base: MathBox, sub: Option<MathBox>, sup: Option<MathBox>) -> MathBox {
    let script_width = sub
        .as_ref()
        .map_or(0, |b| b.width)
        .max(sup.as_ref().map_or(0, |b| b.width));
    let above = base.above.max(sup.as_ref().map_or(0, |b| b.height()));
    let below = base.below.max(sub.as_ref().map_or(0, |b| b.height()));
    MathBox {
        width: base.width.saturating_add(script_width),
        above,
        below,
        content: BoxContent::Scripts {
            base: std::boxed::Box::new(base),
            sub: sub.map(std::boxed::Box::new),
            sup: sup.map(std::boxed::Box::new),
        },
    }
}

/// A base with its scripts stacked over and under it (design spec §6.3).
///
/// The operator keeps the baseline, so a `\sum` stays on the reader's line and its limits
/// grow the block around it.
pub(crate) fn limits(base: MathBox, under: Option<MathBox>, over: Option<MathBox>) -> MathBox {
    let width = base
        .width
        .max(under.as_ref().map_or(0, |b| b.width))
        .max(over.as_ref().map_or(0, |b| b.width));
    let above = base
        .above
        .saturating_add(over.as_ref().map_or(0, |b| b.height()));
    let below = base
        .below
        .saturating_add(under.as_ref().map_or(0, |b| b.height()));
    MathBox {
        width,
        above,
        below,
        content: BoxContent::Limits {
            base: std::boxed::Box::new(base),
            under: under.map(std::boxed::Box::new),
            over: over.map(std::boxed::Box::new),
        },
    }
}

/// The stroke, the overline, and the radicand under them (design spec §6.2).
///
/// One row goes to the overline. The stroke's width depends on the shape of the
/// radicand, because the two forms are drawn differently and the owner has ruled that
/// they do not share a left offset:
///
/// * A one-row radicand keeps the plain sign — `√ b + 4` — and that costs **two**
///   columns, the sign and the space after it.
/// * A taller radicand is drawn as a vertical stem capped by a square right angle, with
///   the tick beside the diagonal on the bottom row, and that costs **four**: tick,
///   diagonal, stem, gap.
///
/// An index — the `3` of a cube root — is placed by two rules, and both are the owner's
/// ruling of 2026-08-22 (spec §6.2 shows no index example):
///
/// * its **rightmost column** sits on the stroke's *first* column — over the `√` in the
///   one-row form, over the tick in the tall one. The tick is the top of the short
///   initial stroke, so this is one rule in both forms and not two that agree.
/// * its **last row** sits one row above the radical's bottom row.
///
/// On a one-row root with a one-row index those two rules are exactly `["3 ─", "√ x"]`.
/// They say nothing about the index being one row, so an index of any size is placed by
/// them, and this box grows `above` to hold whatever they ask for — see below.
///
/// # Why `above` is a maximum
///
/// An index is a box like any other and may be a fraction or a root of its own. It is
/// **not** flattened to one row (that was tried, on 2026-08-22, and reversed the next day:
/// it refused `\sqrt[\sqrt[q]{2}]{x}`, a formula that draws perfectly well). Instead the
/// radical is sized to hold it.
///
/// The index's last row is one above the bottom row, so it reaches
/// `index.height() - 1` rows further up than that — and the bottom row is
/// `radicand.below` below the baseline. The index therefore needs
/// `index.height() - radicand.below` rows above the baseline, against the
/// `radicand.above + 1` the stroke and the overline need. Whichever is larger wins.
///
/// The big shape already nested inside the *radicand* — `\sqrt{\sqrt{\sqrt{x}}}` draws
/// three stems side by side — and this is what makes it nest inside the *index* too.
/// Without the maximum a tall index drew off the top of its own box and over the stroke,
/// erasing the `√` on a one-row root and the tick on a tall one.
pub(crate) fn radical(radicand: MathBox, index: Option<MathBox>) -> MathBox {
    // Not `STROKE - 1`, and deliberately not a function of `stroke` at all. Exactly ONE
    // column is free for the index in either form — the stroke's first column, which the
    // index's last column takes — so everything past the first column of the index
    // overhangs to the left, one column at a time. The count is 1 because that is how
    // many columns the *index* is given, not because of how wide the stroke is: the tall
    // stroke's three further columns are the diagonal, the stem and the gap, and an
    // index reaching into any of them would collide with the art or read as sitting over
    // the radicand. Re-coupling this to `stroke` would silently grant a tall root three
    // free columns.
    let overhang = index.as_ref().map_or(0, |b| b.width.saturating_sub(1));
    let stroke = if radicand.is_inline() { 2 } else { 4 };
    // The index's last row is one above the bottom row, and the bottom row is
    // `radicand.below` below the baseline, so the index reaches this far above it. Zero
    // when there is no index, which leaves the stroke's own requirement standing.
    let index_reach = index
        .as_ref()
        .map_or(0, |b| b.height().saturating_sub(radicand.below));
    MathBox {
        width: radicand
            .width
            .saturating_add(stroke)
            .saturating_add(overhang),
        above: radicand.above.saturating_add(1).max(index_reach),
        below: radicand.below,
        content: BoxContent::Radical {
            radicand: std::boxed::Box::new(radicand),
            index: index.map(std::boxed::Box::new),
        },
    }
}

/// Delimiters sized to what they enclose (design spec §6.4).
///
/// A one-row body takes the plain characters and no padding — `(x)`, never `╭x╮`, and
/// never `( x )` either, because an inline formula that gained two spaces round every
/// bracket would not read as the author's. A taller body is drawn in box art, and there
/// the padding is not cosmetic: `│1  0│` puts a vertical rule flush against a digit and
/// reads as a table cell.
pub(crate) fn fenced(left: Option<char>, right: Option<char>, body: MathBox) -> MathBox {
    // Each delimiter that is present costs its own column, and against a tall body it
    // costs the space between itself and the content as well. So the width is the body
    // plus one *per-side cost* for each side that has a delimiter -- not the body plus a
    // constant, which is what it collapses to only in the two-sided case.
    let per_side = if body.is_inline() { 1 } else { 2 };
    let sides = u16::from(left.is_some()).saturating_add(u16::from(right.is_some()));
    MathBox {
        width: body.width.saturating_add(sides.saturating_mul(per_side)),
        above: body.above,
        below: body.below,
        content: BoxContent::Fenced {
            left,
            right,
            body: std::boxed::Box::new(body),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_box_is_one_row_as_wide_as_its_cells() {
        let b = text("E = mc");
        assert_eq!((b.width, b.above, b.below), (6, 0, 0));
        assert_eq!(b.height(), 1);
    }

    #[test]
    fn plain_text_gathers_the_cells_of_a_row_and_nothing_from_a_construct() {
        // The nesting is the point. `crate::math::symbols` asks this of a single atom, so
        // the `Row` arm is not on its path and a walk that returned only the top level
        // would look right there and be wrong here.
        assert_eq!(text("α").plain_text(), "α");
        assert_eq!(
            row(vec![text("a"), row(vec![text("b"), text("c")])]).plain_text(),
            "abc"
        );
        // A construct contributes nothing, which is what makes the answer "the document's
        // characters" rather than "the drawn ones": this crate composed the rule row and
        // the radical sign, so neither is the author's and neither may be subtracted from
        // what `tests/glyph_inventory.rs` claims. The `a` and `b` inside are unreachable
        // here for the same reason -- they arrive through their own content events.
        assert_eq!(fraction(text("a"), text("b")).plain_text(), "");
        assert_eq!(radical(text("a"), None).plain_text(), "");
    }

    #[test]
    fn a_row_sums_widths_and_takes_the_tallest_of_each_side() {
        let b = row(vec![text("a"), fraction(text("x"), text("y")), text("b")]);
        // 1 + 1 + 1 columns; the fraction is one row above and one below its rule.
        assert_eq!((b.width, b.above, b.below), (3, 1, 1));
    }

    #[test]
    fn a_fraction_rule_is_the_baseline_and_spans_the_wider_part() {
        let b = fraction(text("-b + d"), text("2a"));
        assert_eq!(b.width, 6, "the rule spans the wider part, spec 6.1");
        assert_eq!(
            b.above, 1,
            "the numerator is one row, all of it above the rule"
        );
        assert_eq!(b.below, 1);
    }

    /// A ruleless fraction reports every number a ruled one reports.
    ///
    /// RULING, owner, 2026-09-10 (Task 14b): the baseline of a ruleless fraction is the
    /// row the rule would have been on, left blank. So nothing around it moves -- a
    /// fence, a radical or a neighbour that sizes itself to a binomial sizes itself
    /// exactly as it would to a `\frac` of the same two parts. Asserting the geometry
    /// against the ruled box rather than against literal numbers is what makes this a
    /// claim about the two staying together, rather than about one of them alone.
    #[test]
    fn a_ruleless_fraction_measures_exactly_like_a_ruled_one() {
        let ruled = fraction(text("-b + d"), text("2a"));
        let ruleless = fraction_ruleless(text("-b + d"), text("2a"));
        assert_eq!(ruleless.width, ruled.width);
        assert_eq!(ruleless.above, ruled.above);
        assert_eq!(ruleless.below, ruled.below);
        assert_eq!(ruleless.height(), ruled.height());
        // The whole of the difference is the flag `draw` reads.
        assert!(
            matches!(ruled.content, BoxContent::Fraction { rule: true, .. }),
            "a `\\frac` draws its rule"
        );
        assert!(
            matches!(ruleless.content, BoxContent::Fraction { rule: false, .. }),
            "a binomial does not"
        );
        // Still three rows, so the `write_flat` catch-all never meets one -- the same
        // invariant `a_fraction_and_a_radical_are_never_inline` pins for the ruled box.
        assert!(!ruleless.is_inline());
    }

    #[test]
    fn a_stacked_fraction_grows_by_the_whole_height_of_each_part() {
        // A fraction whose numerator is itself a fraction: 3 rows above the outer rule.
        let inner = fraction(text("a"), text("b"));
        let b = fraction(inner, text("c"));
        assert_eq!(b.above, 3, "1 above + 1 below + the inner rule");
        assert_eq!(b.below, 1);
    }

    #[test]
    fn a_superscript_sits_one_row_above_the_baseline() {
        let b = scripts(text("x"), None, Some(text("2")));
        assert_eq!((b.width, b.above, b.below), (2, 1, 0));
    }

    #[test]
    fn a_subscript_sits_one_row_below_the_baseline() {
        let b = scripts(text("x"), Some(text("i")), None);
        assert_eq!((b.width, b.above, b.below), (2, 0, 1));
    }

    #[test]
    fn both_scripts_share_the_wider_of_the_two_columns() {
        let b = scripts(text("x"), Some(text("i")), Some(text("max")));
        assert_eq!(b.width, 4, "base 1 + the wider script 3");
        assert_eq!((b.above, b.below), (1, 1));
    }

    #[test]
    fn a_tall_superscript_pushes_the_top_up_by_its_whole_height() {
        let b = scripts(text("e"), None, Some(fraction(text("a"), text("b"))));
        // The script's lowest row sits one above the baseline, and it is three rows tall,
        // so it occupies rows +1, +2 and +3: spec section 4, "the script's lowest row
        // sits one above the base's baseline". The offset is the script's own first row.
        assert_eq!(b.above, 3);
        assert_eq!(b.below, 0);
    }

    #[test]
    fn limits_stack_onto_the_operator_and_keep_its_baseline() {
        let b = limits(text("S"), Some(text("i=1")), Some(text("n")));
        assert_eq!(b.width, 3, "the widest of operator, under and over");
        assert_eq!((b.above, b.below), (1, 1));
    }

    #[test]
    fn a_radical_adds_the_stroke_and_the_overline() {
        let b = radical(text("b + 4"), None);
        assert_eq!(
            b.width, 7,
            "the radicand plus the stroke column and a space"
        );
        assert_eq!(b.above, 1, "the overline");
        assert_eq!(b.below, 0);
    }

    #[test]
    fn a_tall_radicand_costs_two_more_stroke_columns_than_a_one_row_one() {
        // The two forms are drawn differently and do not share a left offset: `√ x` is
        // two columns of stroke, the stem-and-right-angle form is four -- tick, diagonal,
        // stem, gap.
        let one_row = radical(text("xyz"), None);
        assert_eq!(one_row.width, 5, "3 radicand + 2 stroke");

        let tall = radical(fraction(text("a"), text("bcd")), None);
        assert_eq!(tall.width, 7, "3 radicand + 4 stroke");
        assert_eq!(tall.above, 2, "the radicand's ascent plus the overline");
        assert_eq!(tall.below, 1, "and its descent, untouched by the stroke");
    }

    #[test]
    fn one_column_of_index_is_free_in_both_forms_and_the_rest_overhangs() {
        // The four cases the owner ruled on, 2026-08-22. The index's last column is the
        // stroke's first column in either form, so the free column count is 1 on both
        // sides and the overhang is `index.width - 1` with no `stroke` in it. A tall
        // radicand of width 3, so that the 3 and the 4 in `3 + 4 + overhang` cannot be
        // confused with each other.
        let tall = || fraction(text("a"), text("bcd"));
        assert_eq!(tall().width, 3, "the shared radicand width");

        assert_eq!(
            radical(text("x"), Some(text("3"))).width,
            3,
            "1 radicand + 2 stroke + 0 overhang"
        );
        assert_eq!(
            radical(text("x"), Some(text("10"))).width,
            4,
            "1 radicand + 2 stroke + 1 overhang"
        );
        assert_eq!(
            radical(tall(), Some(text("3"))).width,
            7,
            "3 radicand + 4 stroke + 0 overhang"
        );
        assert_eq!(
            radical(tall(), Some(text("10"))).width,
            8,
            "3 radicand + 4 stroke + 1 overhang"
        );

        // The index never touches the height, in either form.
        assert_eq!(
            (
                radical(tall(), Some(text("10"))).above,
                radical(tall(), Some(text("10"))).below,
            ),
            (2, 1),
            "the index rides on a row the radical already has"
        );
    }

    #[test]
    fn a_one_row_fence_takes_the_plain_characters_and_no_padding() {
        let b = fenced(Some('('), Some(')'), text("x"));
        assert_eq!((b.width, b.above, b.below), (3, 0, 0));
    }

    #[test]
    fn a_one_sided_fence_pays_only_for_the_side_it_has() {
        // `\left(` with no closer: one column for the opener and nothing for the side
        // that is absent. A width computed as a constant rather than per present side
        // charges for both here.
        assert_eq!(
            fenced(Some('('), None, text("x")).width,
            2,
            "one column for the opener, none for the absent closer"
        );

        // The mirror, and tall, so the present side costs its column and its space:
        // body 1 + 1 side x 2 columns.
        assert_eq!(
            fenced(None, Some(')'), fraction(text("a"), text("b"))).width,
            3,
            "a tall one-sided fence pays a column and a space, once"
        );

        // `\left. ... \right.` -- both sides dropped, so the fence costs nothing.
        let bare = fenced(None, None, text("x"));
        assert_eq!(bare.width, 1, "no delimiters, no columns");
        assert!(bare.is_inline(), "and it stays on the prose row");
    }

    #[test]
    fn a_tall_fence_is_padded_so_the_box_art_does_not_touch_the_content() {
        let b = fenced(Some('('), Some(')'), fraction(text("a"), text("b")));
        assert_eq!(b.width, 5, "1 + space + 1 + space + 1");
        assert_eq!((b.above, b.below), (1, 1));
    }

    #[test]
    fn an_empty_row_is_an_empty_box() {
        let b = row(vec![]);
        assert_eq!((b.width, b.above, b.below), (0, 0, 0));
        assert!(b.is_empty());
    }

    #[test]
    fn is_inline_is_exactly_the_zero_height_constraint() {
        assert!(text("x").is_inline());
        assert!(row(vec![text("a"), text("b")]).is_inline());
        assert!(!fraction(text("a"), text("b")).is_inline());
        assert!(!scripts(text("x"), None, Some(text("2"))).is_inline());
    }

    #[test]
    fn absurd_widths_saturate_rather_than_overflow() {
        let wide = MathBox {
            width: u16::MAX,
            above: 0,
            below: 0,
            content: BoxContent::Text(String::new()),
        };
        let b = row(vec![wide, text("x")]);
        assert_eq!(b.width, u16::MAX, "saturates, and must not panic");
    }

    #[test]
    fn text_measures_display_columns_and_not_bytes_or_chars() {
        // Two double-width cells: 4 columns, but 6 bytes and 2 chars. Measuring with
        // `len()` gives 6 and with `chars().count()` gives 2 -- the defect that
        // `src/text/mod.rs:2-17` makes `crate::text` the single home of width logic to
        // prevent. Only the display width is 4, so this rejects both.
        let cjk = text("日本");
        assert_eq!(
            cjk.width, 4,
            "two double-width cells; len() is 6, chars() is 2"
        );
        assert_eq!(cjk.height(), 1, "a wide cell is still one row");

        // The same claim from the other side: one grapheme cluster of one column, but 3
        // bytes and 2 chars, so here the display width is the *smallest* of the three.
        let combining = text("e\u{0301}");
        assert_eq!(
            combining.width, 1,
            "e plus a combining acute is one cell; len() is 3, chars() is 2"
        );
    }

    #[test]
    fn a_tall_base_keeps_its_own_height_when_the_superscript_is_short() {
        // Every other scripts test has a one-row base, which hides `base.above` behind
        // the script's height. Here the base is 3 rows above its own baseline and the
        // script is 1, so taking only the script loses the base entirely.
        let base = fraction(fraction(text("a"), text("b")), text("c"));
        assert_eq!(
            (base.above, base.below),
            (3, 1),
            "the base before scripting"
        );
        let b = scripts(base, None, Some(text("2")));
        assert_eq!(b.above, 3, "the taller base wins, not the one-row script");
        assert_eq!(b.below, 1, "and the base's own descent survives");
    }

    #[test]
    fn a_tall_base_keeps_its_own_depth_when_the_subscript_is_short() {
        // The mirror of the above: 3 rows below the baseline, a one-row subscript.
        let base = fraction(text("a"), fraction(text("b"), text("c")));
        assert_eq!(
            (base.above, base.below),
            (1, 3),
            "the base before scripting"
        );
        let b = scripts(base, Some(text("2")), None);
        assert_eq!(b.below, 3, "the deeper base wins, not the one-row script");
        assert_eq!(b.above, 1, "and the base's own ascent survives");
    }

    #[test]
    fn a_root_index_takes_the_one_free_column_over_the_stroke() {
        // The index is right-aligned into the single column above the stroke glyph, so a
        // one-column index costs nothing beyond the stroke that is already there.
        let cube = radical(text("x"), Some(text("3")));
        assert_eq!(cube.width, 3, "1 radicand + 2 stroke, nothing overhanging");
        assert_eq!(
            radical(text("x"), None).width,
            3,
            "an index of one column is free"
        );

        // A wider index has nowhere to go but leftwards, one column per extra column.
        assert_eq!(
            radical(text("x"), Some(text("10"))).width,
            4,
            "the second index column overhangs the stroke"
        );
    }

    #[test]
    fn a_radical_grows_above_only_as_far_as_its_index_actually_reaches() {
        // `above` is a maximum of two claims: the stroke's own (`radicand.above + 1`)
        // and the index's (`index.height() - radicand.below`). Each case below is here
        // because a different one of them wins.

        // No index, and a one-row index: the stroke's claim wins and nothing moves.
        assert_eq!(radical(text("x"), None).above, 1);
        assert_eq!(radical(text("x"), Some(text("3"))).above, 1, "1 - 0 == 1");

        // A three-row index over a one-row radicand: the index's claim wins outright.
        let b = radical(text("x"), Some(fraction(text("a"), text("b"))));
        assert_eq!(b.above, 3, "the index reaches 3 rows above the baseline");
        assert_eq!(b.below, 0, "and not one row below it");
        assert_eq!(b.width, 3, "1 radicand + 2 stroke + 0 overhang, unchanged");

        // The same index over a TALL radicand costs nothing: the radicand's own descent
        // already carries the bottom row a row further down, so the index's claim drops
        // to 2 and ties with the stroke's. Drop the `- radicand.below` term and this
        // becomes 3 -- a reserved row nothing ever draws in.
        let b = radical(
            fraction(text("p"), text("q")),
            Some(fraction(text("a"), text("b"))),
        );
        assert_eq!((b.above, b.below), (2, 1), "3 - 1 == 2, tying with 1 + 1");

        // A four-row index over a tall radicand: the index wins again, by one.
        let b = radical(
            fraction(text("p"), text("q")),
            Some(radical(fraction(text("a"), text("b")), None)),
        );
        assert_eq!(b.above, 3, "4 - 1 == 3 beats 1 + 1");
        assert_eq!(b.height(), 5);
    }

    #[test]
    fn a_fraction_stores_its_parts_the_right_way_round() {
        // Equal-height parts hide a num/den swap completely: the width is a max and both
        // heights are 1. Only the stored content shows it.
        let b = fraction(text("a"), text("bb"));
        let BoxContent::Fraction { num, den, .. } = &b.content else {
            panic!("fraction built {:?}", b.content)
        };
        assert_eq!(
            num.content,
            BoxContent::Text("a".into()),
            "the numerator is on top"
        );
        assert_eq!(den.content, BoxContent::Text("bb".into()));
    }

    #[test]
    fn a_fraction_is_not_symmetric_between_its_parts() {
        // The numbers do show a swap once the two parts differ in height, which is the
        // case a drawing task will regress on.
        let top_heavy = fraction(fraction(text("a"), text("b")), text("c"));
        let bottom_heavy = fraction(text("c"), fraction(text("a"), text("b")));
        assert_eq!((top_heavy.above, top_heavy.below), (3, 1));
        assert_eq!((bottom_heavy.above, bottom_heavy.below), (1, 3));
    }

    #[test]
    fn scripts_and_limits_store_each_operand_on_its_own_side() {
        // A sub/sup or under/over swap is invisible in the numbers whenever the two are
        // the same height, which is the common case.
        let s = scripts(text("x"), Some(text("i")), Some(text("n")));
        let BoxContent::Scripts { base, sub, sup } = &s.content else {
            panic!("scripts built {:?}", s.content)
        };
        assert_eq!(base.content, BoxContent::Text("x".into()));
        let sub = sub.as_ref().expect("a subscript was given");
        let sup = sup.as_ref().expect("a superscript was given");
        assert_eq!(sub.content, BoxContent::Text("i".into()), "sub stays below");
        assert_eq!(sup.content, BoxContent::Text("n".into()), "sup stays above");

        let l = limits(text("S"), Some(text("i=1")), Some(text("n")));
        let BoxContent::Limits { base, under, over } = &l.content else {
            panic!("limits built {:?}", l.content)
        };
        assert_eq!(base.content, BoxContent::Text("S".into()));
        let under = under.as_ref().expect("an under was given");
        let over = over.as_ref().expect("an over was given");
        assert_eq!(under.content, BoxContent::Text("i=1".into()));
        assert_eq!(over.content, BoxContent::Text("n".into()));
    }

    #[test]
    fn a_row_keeps_its_parts_in_source_order() {
        let b = row(vec![text("a"), text("b"), text("c")]);
        let BoxContent::Row(parts) = &b.content else {
            panic!("row built {:?}", b.content)
        };
        let contents: Vec<_> = parts.iter().map(|p| p.content.clone()).collect();
        assert_eq!(
            contents,
            vec![
                BoxContent::Text("a".into()),
                BoxContent::Text("b".into()),
                BoxContent::Text("c".into()),
            ],
            "a reordered row draws the formula in the wrong order"
        );
    }

    #[test]
    fn a_radical_and_a_fence_store_their_parts_distinguishably() {
        let r = radical(text("x"), Some(text("3")));
        let BoxContent::Radical { radicand, index } = &r.content else {
            panic!("radical built {:?}", r.content)
        };
        assert_eq!(radicand.content, BoxContent::Text("x".into()));
        let index = index.as_ref().expect("an index was given");
        assert_eq!(index.content, BoxContent::Text("3".into()));

        // Mismatched delimiters, so a left/right swap cannot hide in the symmetry.
        let f = fenced(Some('['), Some(')'), text("x"));
        let BoxContent::Fenced { left, right, body } = &f.content else {
            panic!("fenced built {:?}", f.content)
        };
        assert_eq!(
            (*left, *right),
            (Some('['), Some(')')),
            "the opener stays on the left"
        );
        assert_eq!(body.content, BoxContent::Text("x".into()));
    }
}
