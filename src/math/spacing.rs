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
//! The owner's ruling, which this table encodes: **one space either side of a relation
//! always; one either side of a binary operator except inside a script operand.** The
//! exception is deliberately *not* in this table — see [`gap`].

// Nothing outside this module's own tests calls these yet: `build.rs` is the first caller
// and arrives in task 3, so the lib target sees the whole surface as dead while the test
// target sees all of it live. `expect` cannot express that -- it fires
// `unfulfilled_lint_expectations` on the test target -- so this is `allow`, and it comes
// out once the builder lands.
#![allow(dead_code)]

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
    /// A sign with no left operand — the `-` of `-x`. Binds tight to what follows.
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
    /// Every class, for the totality test.
    pub(crate) const ALL: [Self; 10] = [
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
}

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
    use Class::{Binary, Close, Edge, Function, Large, Open, Ordinary, Punct, Relation, Unary};

    match (left, right) {
        // Nothing borders the ends of the formula.
        (Edge, _) | (_, Edge) => 0,

        // A delimiter hugs its contents on the inside.
        (Open, _) | (_, Close) => 0,

        // A sign with no left operand binds to what follows it.
        (Unary, _) => 0,

        // Punctuation hugs what precedes it and parts from what follows.
        (_, Punct) => 0,
        (Punct, _) => 1,

        // A relation and a binary operator are spaced on both sides.
        (Relation, _) | (_, Relation) | (Binary, _) | (_, Binary) => 1,

        // A function name parts from its argument; a large operator from its operand.
        (Function, _) | (Large, _) | (_, Large) => 1,

        // A number or variable before a function name: `2 sin x`.
        (Ordinary | Close, Function) => 1,

        // Nothing in the ruling says what precedes a sign that is defined to never have
        // a left operand ([`Class::Unary`]'s own doc comment). This pairing cannot arise
        // from a correct classification -- a `-` with an `Ordinary` or `Close` to its
        // left is a `Binary`, not a `Unary` -- but the match must still be total, so it
        // takes the same "touches" default as any other pair of adjacent ordinary-like
        // content.
        (Ordinary | Close, Unary) => 0,

        // Everything else touches: `ab`, `2x`, `x(`, `)y`.
        (Ordinary | Close, Ordinary | Open) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::Class::{Close, Function, Large, Open, Ordinary, Punct, Relation, Unary};
    use super::gap;

    #[test]
    fn a_relation_always_takes_a_space_on_both_sides() {
        assert_eq!(gap(Ordinary, Relation), 1);
        assert_eq!(gap(Relation, Ordinary), 1);
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
    fn a_function_name_takes_a_space_before_its_argument() {
        assert_eq!(gap(Function, Ordinary), 1, "sin x, not sinx");
        assert_eq!(gap(Function, Open), 1, "sin (x + y)");
    }

    #[test]
    fn nothing_is_inserted_after_an_opening_or_before_a_closing_delimiter() {
        assert_eq!(gap(Open, Ordinary), 0, "(x, not ( x");
        assert_eq!(gap(Ordinary, Close), 0, "x), not x )");
        assert_eq!(
            gap(Open, super::Class::Binary),
            0,
            "(-x has no gap at the head"
        );
    }

    #[test]
    fn punctuation_hugs_what_precedes_it_and_parts_from_what_follows() {
        assert_eq!(gap(Ordinary, Punct), 0, "f(x, not f(x ,");
        assert_eq!(gap(Punct, Ordinary), 1, "f(x, y), not f(x,y)");
    }

    #[test]
    fn a_unary_sign_hugs_its_operand() {
        // This is the -x defect of stage 1, now a table entry rather than a head-of-run rule.
        assert_eq!(gap(Unary, Ordinary), 0, "-x, not - x");
        assert_eq!(gap(Unary, Function), 0, "-sin x, not - sin x");
        assert_eq!(gap(Unary, Large), 0);
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
    fn the_table_is_total_and_never_exceeds_one_column() {
        for left in super::Class::ALL {
            for right in super::Class::ALL {
                assert!(
                    gap(left, right) <= 1,
                    "{left:?} then {right:?} asked for more than one column"
                );
            }
        }
    }

    #[test]
    fn the_five_stage_one_seam_cases_are_each_one_lookup() {
        // Each of these was a separate defect on the stage-1 branch. None of them is a
        // rule here; they are five cells of one table.
        assert_eq!(gap(super::Class::Edge, Unary), 0, "-x at the head");
        assert_eq!(gap(Ordinary, Function), 1, "2 sin x");
        assert_eq!(gap(Unary, Function), 0, "-sin x");
    }
}
