// SPDX-License-Identifier: MIT
//! How much room goes between two adjacent pieces of a formula.
//!
//! One table, indexed by the class of the piece on the left and the class of the piece on
//! the right. This shape is a direct answer to stage 1, where the same question was
//! answered by five rules that each read correctly on its own — suppress at the head of a
//! run, suppress after a function name, suppress inside a script — and produced five
//! defects at the joins between them (`-x`, `\sin x`, `2\sin^2 x`, `2{\sin x}^2`,
//! `2{ab}^2`). A table has no joins: every pair is a cell, and a wrong rendering is a
//! wrong cell rather than an interaction nobody enumerated.
//!
//! [`TABLE`] is a literal grid, not a precedence-ordered `match`. An overlapping `match`
//! is the same defect class stage 1 shipped, one level down: whichever arm is listed
//! first silently wins at a cell two arms both claim, the same way stage 1's five rules
//! silently picked a winner at their seams. A grid has no arm order to depend on — every
//! cell is written once.
//!
//! The owner's ruling, which this table encodes: **one space either side of a relation
//! always; one either side of a binary operator except inside a script operand.** The
//! exception is deliberately *not* in this table — see [`gap`].

/// Rows and columns in [`TABLE`].
const N: usize = 10;

/// What a piece of a formula is, for the purpose of what goes beside it.
///
/// These are `pulldown-latex`'s content categories (`Content` at
/// `~/.cargo/registry/src/index.crates.io-*/pulldown-latex-0.8.0/src/event.rs:102`)
/// collapsed to the distinctions that change spacing, plus two of our own: [`Class::Unary`]
/// for a sign with nothing to its left to bind to, and [`Class::Edge`] for the ends of the
/// formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    /// The start or end of the formula. Nothing is ever inserted against it.
    Edge,
    /// A variable, a number, a symbol: `Content::Ordinary`, `Content::Number`.
    Ordinary,
    /// A function name: `Content::Function`. Parts from its argument.
    Function,
    /// A binary operator with operands either side: `Content::BinaryOp`.
    Binary,
    /// A sign with no left operand — the `-` of `-x`. Binds tight to what follows: the
    /// plan's decision, not TeX's ([`UNARY_ROW`]) — `-x` and `-\sin x` both stay tight.
    Unary,
    /// A relation: `Content::Relation`. Always spaced.
    Relation,
    /// A large operator: `Content::LargeOp`.
    Large,
    /// An opening delimiter: `Content::Delimiter { ty: DelimiterType::Open, .. }`.
    Open,
    /// A closing delimiter: `Content::Delimiter { ty: DelimiterType::Close, .. }`.
    Close,
    /// A comma, a full stop, a semicolon: `Content::Punctuation`.
    Punct,
}

impl Class {
    /// Every class, for tests that iterate over all ten — including [`TABLE`]'s own
    /// whole-grid snapshot. Kept in step with [`Class::index`] by hand; adding an
    /// eleventh class without extending both leaves [`Class::index`] non-exhaustive,
    /// which fails to compile rather than leaving the new class silently uncovered here.
    ///
    /// The one item in this module with no caller outside its own tests, which is why the
    /// suppression sits here rather than over the module: measured with `build.rs` in the
    /// tree, [`gap`], [`Class`] and all ten variants are live, and only this is not. A
    /// module-wide `allow` would now be broad enough to hide the *next* dead item. It is
    /// `cfg_attr` rather than `expect` because `expect` fires
    /// `unfulfilled_lint_expectations` on the test target, where this is used.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const ALL: [Self; N] = [
        Self::Edge,
        Self::Ordinary,
        Self::Function,
        Self::Binary,
        Self::Unary,
        Self::Relation,
        Self::Large,
        Self::Open,
        Self::Close,
        Self::Punct,
    ];

    /// This class's row and column position in [`TABLE`].
    ///
    /// Exhaustive and without a wildcard arm on purpose: adding an eleventh [`Class`]
    /// fails this match at compile time, rather than the new variant silently falling
    /// through to index `0` and colliding with [`Class::Edge`]'s row.
    const fn index(self) -> usize {
        match self {
            Self::Edge => 0,
            Self::Ordinary => 1,
            Self::Function => 2,
            Self::Binary => 3,
            Self::Unary => 4,
            Self::Relation => 5,
            Self::Large => 6,
            Self::Open => 7,
            Self::Close => 8,
            Self::Punct => 9,
        }
    }
}

// The rows of `TABLE`, named for what they mean rather than written out ten times by
// hand. Column order matches `Class::index`: Edge, Ordinary, Function, Binary, Unary,
// Relation, Large, Open, Close, Punct.

/// Borders nothing, on either side: [`Class::Edge`] and [`Class::Open`]. A delimiter
/// hugs its contents on the inside exactly as the formula's own boundary hugs its ends.
const ZERO_ROW: [u16; N] = [0; N];

/// Touches whatever comes next, the way two adjacent atoms touch: [`Class::Ordinary`]
/// and [`Class::Close`] (a closing delimiter's own trailing behaviour matches an
/// ordinary atom's: `)y` and `)(` both touch, but `)=` and `) sin` still space).
/// [`Class::Unary`] is *not* this row — see [`UNARY_ROW`].
const TOUCHES_ROW: [u16; N] = [0, 0, 1, 1, 0, 1, 1, 0, 0, 0];

/// [`Class::Unary`]: binds tight to what follows, per the plan's decision that a sign
/// with no left operand hugs its operand — `-x` -> `−x`, `-\sin x` -> `−sin x`. Not yet
/// pinned by a passing test: `src/math/tests.rs`'s
/// `a_leading_unary_minus_before_a_function_name_keeps_a_space` still asserts the
/// stage-1 defect (`"− sin x"`, loose) on purpose, until Task 5 renames it to
/// `a_leading_unary_minus_binds_tight_to_a_function_name` and flips the expectation.
///
/// Two cells break that pattern: `Binary` and `Relation`, because the owner's ruling —
/// "one space either side of a relation always" — is not something a wildcard is
/// entitled to override *here*. That is not a global precedence, though: `(Open,
/// Relation)` is 0 ([`ZERO_ROW`]) and `(Relation, Close)` is 0 (the `Close` column of
/// [`ALWAYS_SPACED_ROW`]), so the relation ruling loses to the delimiter-hugging rule at
/// a delimiter boundary. The actual precedence: the delimiter-hugs rule wins at a
/// delimiter boundary; the relation rule wins everywhere else, including here, where
/// neither side is a delimiter. All four of these cells are reachable only on
/// degenerate input (`-=`, `-+`, `(=`, `=)`), in the same class as Ruling D's cells:
/// correct if reached, not because a real formula hits them. This row is *not*
/// [`TOUCHES_ROW`] with two cells patched on top; it is its own row, because the two
/// exceptions are a deliberate departure from the "binds tight" pattern, not a
/// coincidence of it.
const UNARY_ROW: [u16; N] = [0, 0, 0, 1, 0, 1, 0, 0, 0, 0];

/// Parts from its operand, but hugs a delimited group that is already visibly its own
/// thing: [`Class::Function`] and [`Class::Large`]. `\sin(x)` and `\sum(a_i + b_i)` are
/// both tight before `(` — the `TeXbook`'s spacing table sets the Op-Open pair tight, and
/// `src/math/tests.rs`'s `a_function_name_sits_tight_against_an_opening_delimiter` pins
/// the first. The chapter is deliberately not cited; see the note on `build.rs`'s
/// `assemble`, which drops the same unverified reference.
const OPERAND_SEEKING_ROW: [u16; N] = [0, 1, 1, 1, 1, 1, 1, 0, 0, 0];

/// Always spaced, with no exception for a following delimiter: [`Class::Binary`] and
/// [`Class::Relation`]. The owner's ruling is unconditional ("one space either side ...
/// always"), so unlike the row above there is no Open exception here — `a + (b)` and
/// `a = (b)` both space before the parenthesis.
const ALWAYS_SPACED_ROW: [u16; N] = [0, 1, 1, 1, 1, 1, 1, 1, 0, 0];

/// [`Class::Punct`]: spaced on every side except the formula's own edge. Includes two
/// cells TeX decides and nothing else here speaks to — `(Punct, Punct)` and
/// `(Punct, Close)` — rather than leaving them to fall out of the open/close and
/// punctuation column rules by accident.
const PUNCT_ROW: [u16; N] = [0, 1, 1, 1, 1, 1, 1, 1, 1, 1];

/// The whole spacing grid. Row `left.index()`, column `right.index()`.
const TABLE: [[u16; N]; N] = [
    ZERO_ROW,            // Edge
    TOUCHES_ROW,         // Ordinary
    OPERAND_SEEKING_ROW, // Function
    ALWAYS_SPACED_ROW,   // Binary
    UNARY_ROW,           // Unary
    ALWAYS_SPACED_ROW,   // Relation
    OPERAND_SEEKING_ROW, // Large
    ZERO_ROW,            // Open
    TOUCHES_ROW,         // Close
    PUNCT_ROW,           // Punct
];

/// Columns to insert between a `left` piece and a `right` piece.
///
/// Always 0 or 1: this is a terminal, and the finer quads TeX sets are not available at
/// cell resolution. Returning a number rather than a `bool` keeps the caller's arithmetic
/// honest — a `Row`'s width is the sum of its parts plus the sum of its gaps.
///
/// **The script-operand exception is not here.** Inside `x^{a+b}` the ruling is that no
/// spaces are set, because the Unicode superscript tables have no raised space and the
/// substitution would decline on it (`src/math/scripts.rs`, the all-or-nothing rule).
/// That is a property of *where the operand is*, known only to the builder, so
/// `build.rs` multiplies this answer by zero rather than passing a flag in here. Two
/// independent decisions; neither can shadow the other, which is exactly what went wrong
/// on the stage-1 branch.
pub(crate) const fn gap(left: Class, right: Class) -> u16 {
    // Both indices are below `N`: `Class::index` is an exhaustive match over every
    // variant into `0..N`, so this cannot go out of bounds.
    TABLE[left.index()][right.index()]
}

#[cfg(test)]
mod tests {
    use super::Class::{Close, Function, Large, Open, Ordinary, Punct, Relation, Unary};
    use super::{Class, gap};

    #[test]
    fn a_relation_always_takes_a_space_on_both_sides() {
        assert_eq!(gap(Ordinary, Relation), 1);
        assert_eq!(gap(Relation, Ordinary), 1);
        // Ruling D: a deliberate departure from TeX, which sets this cell to 0. The
        // owner's ruling is unconditional ("one space either side of a relation
        // always"), so `Relation, Relation` stays 1 here rather than following TeX.
        assert_eq!(
            gap(Relation, Relation),
            1,
            "the owner's ruling has no exception here"
        );
    }

    #[test]
    fn a_binary_operator_takes_a_space_on_both_sides() {
        assert_eq!(gap(Ordinary, super::Class::Binary), 1);
        assert_eq!(gap(super::Class::Binary, Ordinary), 1);
    }

    #[test]
    fn two_ordinaries_touch() {
        assert_eq!(gap(Ordinary, Ordinary), 0, "ab is one product, not a b");
    }

    #[test]
    fn a_function_name_takes_a_space_before_its_argument_but_not_before_a_delimited_one() {
        assert_eq!(gap(Function, Ordinary), 1, "sin x, not sinx");
        // Ruling A: a delimited group is already visibly its own thing, so the operator
        // name sits tight against it -- real LaTeX sets `\sin(x)` tight too. Pinned in
        // `src/math/tests.rs`'s `a_function_name_sits_tight_against_an_opening_delimiter`.
        assert_eq!(gap(Function, Open), 0, "sin(x + y), not sin (x + y)");
    }

    #[test]
    fn a_large_operator_also_sits_tight_against_an_opening_delimiter() {
        // Ruling A, one row down from the function case above: `\sum(a_i + b_i)` must
        // not ship as `∑ (aᵢ + bᵢ)`.
        assert_eq!(gap(Large, Open), 0, "sum(a_i + b_i)");
    }

    #[test]
    fn nothing_is_inserted_after_an_opening_or_before_a_closing_delimiter() {
        assert_eq!(gap(Open, Ordinary), 0, "(x, not ( x");
        assert_eq!(gap(Ordinary, Close), 0, "x), not x )");
        assert_eq!(
            gap(Open, super::Class::Binary),
            0,
            "(+x is degenerate LaTeX, but a delimiter still hugs whatever follows it"
        );
        assert_eq!(
            gap(Open, Unary),
            0,
            "(-x has no gap at the head -- the - here is Unary, not Binary, since Open \
             gives it no left operand"
        );
    }

    #[test]
    fn punctuation_hugs_what_precedes_it_and_parts_from_what_follows() {
        assert_eq!(gap(Ordinary, Punct), 0, "f(x, not f(x ,");
        assert_eq!(gap(Punct, Ordinary), 1, "f(x, y), not f(x,y)");
        // Ruling D: two more cells nothing else here speaks to. Both used to fall out
        // of the open/close and punctuation column rules as 0 by accident; TeX spaces
        // them, and there is no ruling here that says otherwise.
        assert_eq!(
            gap(Punct, Punct),
            1,
            "x,,y is degenerate LaTeX, but TeX still spaces it"
        );
        assert_eq!(
            gap(Punct, Close),
            1,
            "TeX spaces punctuation before a closing delimiter"
        );
    }

    #[test]
    fn a_unary_sign_hugs_its_operand() {
        // This is the -x defect of stage 1, now a table entry rather than a
        // head-of-run rule. The plan's decision: a sign with no left operand binds
        // tight to what follows -- including a function name or a large operator,
        // unlike an `Ordinary` atom in the same position. Not yet pinned by a passing
        // test: `src/math/tests.rs`'s
        // `a_leading_unary_minus_before_a_function_name_keeps_a_space` still asserts
        // the loose stage-1 output on purpose, until Task 5 renames it and flips the
        // expectation to `"−sin x"`.
        assert_eq!(gap(Unary, Ordinary), 0, "-x, not - x");
        assert_eq!(gap(Unary, Function), 0, "-sin x, not - sin x");
        assert_eq!(gap(Unary, Large), 0, "-sum i, not - sum i");
    }

    #[test]
    fn a_unary_sign_still_spaces_before_a_relation_or_a_binary_operator() {
        // Degenerate input (`-=`, `-+`) but the cell must say something, and the
        // owner's ruling -- "one space either side of a relation always" -- is not
        // something a wildcard is entitled to override. Correct if reached, not
        // because a real formula hits it (same class as Ruling D's cells).
        assert_eq!(
            gap(Unary, Relation),
            1,
            "the owner's ruling has no exception here"
        );
        assert_eq!(
            gap(Unary, super::Class::Binary),
            1,
            "same ruling, same reason"
        );
    }

    #[test]
    fn the_unary_row_is_zero_except_before_a_relation_or_a_binary_operator() {
        // States `UNARY_ROW`'s shape precisely, over every column at once, rather than
        // leaving it to a handful of individual assertions: `Unary` binds tight to
        // everything except the two cells the owner's ruling claims.
        for right in Class::ALL {
            let expected = match right {
                Relation | super::Class::Binary => 1,
                _ => 0,
            };
            assert_eq!(gap(Unary, right), expected, "{right:?}");
        }
    }

    #[test]
    fn a_large_operator_parts_from_what_follows_it_but_not_from_what_precedes() {
        assert_eq!(gap(Large, Ordinary), 1, "sum i, not sumi");
        assert_eq!(gap(Ordinary, Large), 1);
    }

    #[test]
    fn nothing_ever_borders_the_start_or_end_of_a_formula() {
        assert_eq!(gap(super::Class::Edge, Relation), 0);
        assert_eq!(gap(Relation, super::Class::Edge), 0);
        assert_eq!(gap(super::Class::Edge, super::Class::Binary), 0);
    }

    #[test]
    fn the_whole_table_is_pinned_as_a_grid() {
        // Replaces a totality test that only checked `gap(left, right) <= 1` -- true by
        // inspection of every arm's literal return value, so it had no power over a 0/1
        // flip. A snapshot of every cell does: any single-cell change anywhere in the
        // grid shows up as a labelled diff in review, which is the closest thing to
        // exhaustive an automated check can offer here.
        // `{:>9?}` alone would not pad: a derived `Debug` impl does not call
        // `Formatter::pad`, so a width flag on `?` is silently ignored. Formatting to a
        // `String` first and padding that as a plain string sidesteps it.
        let mut grid = String::from("        ");
        for right in Class::ALL {
            grid.push_str(&format!("{:>9}", format!("{right:?}")));
        }
        grid.push('\n');
        for left in Class::ALL {
            grid.push_str(&format!("{:>8}", format!("{left:?}")));
            for right in Class::ALL {
                grid.push_str(&format!("{:>9}", gap(left, right)));
            }
            grid.push('\n');
        }
        insta::assert_snapshot!(grid);
    }

    #[test]
    fn three_of_the_five_stage_one_seam_cases_are_each_one_lookup() {
        // Three of stage 1's five spacing defects are single-cell lookups here; the
        // other two -- `2{\sin x}^2` and `2{ab}^2` -- need a class for a braced group
        // and the caller's multiply-by-zero for a script operand, neither of which
        // exists in this module. Those two are Task 3's to prove.
        assert_eq!(gap(super::Class::Edge, Unary), 0, "-x at the head");
        assert_eq!(gap(Ordinary, Function), 1, "2 sin x");
        assert_eq!(gap(Unary, Function), 0, "-sin x");
    }
}
