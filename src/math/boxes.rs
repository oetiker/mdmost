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

// Nothing outside this module's own tests calls these yet: `build.rs` is the first caller
// and arrives in task 3, so the lib target sees the whole surface as dead while the test
// target sees all of it live. `expect` cannot express that -- it fires
// `unfulfilled_lint_expectations` on the test target -- so this is `allow`, and it comes
// out once the builder lands.
#![allow(dead_code)]

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
    /// Numerator over denominator. The rule row is the baseline.
    Fraction {
        /// Drawn above the rule.
        num: std::boxed::Box<MathBox>,
        /// Drawn below it.
        den: std::boxed::Box<MathBox>,
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

    /// Whether this box draws no cells at all.
    ///
    /// Design spec §16.3: a display block whose layout produces nothing contributes no
    /// rows to the document — no frame, no caption, no blank line.
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
/// Two columns go to the stroke and the space after it, and one row to the overline. An
/// index — the `3` of a cube root — is drawn into the stroke's own columns where it fits
/// and widens the box where it does not.
pub(crate) fn radical(radicand: MathBox, index: Option<MathBox>) -> MathBox {
    const STROKE: u16 = 2;
    let overhang = index
        .as_ref()
        .map_or(0, |b| b.width.saturating_sub(STROKE - 1));
    MathBox {
        width: radicand
            .width
            .saturating_add(STROKE)
            .saturating_add(overhang),
        above: radicand.above.saturating_add(1),
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
    let sides = u16::from(left.is_some()) + u16::from(right.is_some());
    let padding = if body.is_inline() { 0 } else { sides };
    MathBox {
        width: body.width.saturating_add(sides).saturating_add(padding),
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
    fn a_one_row_fence_takes_the_plain_characters_and_no_padding() {
        let b = fenced(Some('('), Some(')'), text("x"));
        assert_eq!((b.width, b.above, b.below), (3, 0, 0));
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
}
