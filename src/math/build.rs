// SPDX-License-Identifier: MIT
//! `pulldown-latex` events to a box tree.
//!
//! This is the one engine of design spec §4. [`Mode`] is the one flag, and it is consulted
//! only here: a display formula and an inline formula are the same tree, and inline is the
//! constraint `above == 0 && below == 0` over it. A construct that cannot meet the
//! constraint rewrites itself — a fraction becomes `a/b` — or, where no honest one-row
//! form exists, fails with [`MathError::NotInline`] so the caller can show the source.
//!
//! Spacing is not decided here. Every adjacent pair goes through `spacing::gap`, and the
//! one exception the owner ruled — no spaces inside a script operand — is applied by
//! multiplying that answer by zero, because *where an operand sits* is knowledge this file
//! has and the table does not.

// Nothing outside this module's own tests calls these yet: `render_inline` is rewired onto
// this builder in a later task, so the lib target sees the whole surface as dead while the
// test target sees all of it live -- the same situation `boxes.rs` and `spacing.rs` are in,
// and for the same reason. `expect` cannot express that -- it fires
// `unfulfilled_lint_expectations` on the test target -- so this is `allow`.
//
// Measured, not assumed: with this line removed and the module wired into `mod.rs`, clippy
// reports all eleven items here as never used. `dead_code` is transitive, so this module
// being dead is also why `boxes.rs` and `spacing.rs` keep theirs -- their first caller is
// this file, and a dead caller does not make a callee live. All three come out together
// when the renderer calls in.
#![allow(dead_code)]

use pulldown_latex::event::{Content, DelimiterType, Event, Grouping, Visual};
use pulldown_latex::{Parser, Storage};

use crate::error::MathError;
use crate::math::boxes::{MathBox, row, text};
use crate::math::spacing::{Class, gap};

/// Whether the formula may use more than the row the prose sits on.
///
/// Nothing branches on this yet. Only the flat part of a formula is built here, and a flat
/// formula has one answer in either mode — `display_mode_builds_the_same_flat_row_for_flat_input`
/// says so. The flag is threaded through the walk from the start so that the two-dimensional
/// constructs, each of which arrives in its own task, have it where they need it rather than
/// having to be re-plumbed for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// One row. A taller construct rewrites itself or fails.
    Inline,
    /// Two dimensions.
    Display,
}

/// Parses `src` into an event stream borrowed from `storage`.
///
/// The arena is the caller's because the events borrow from it: `Storage` is
/// `pulldown-latex`'s bump allocator and the `Event`s hold `&'a str` into it.
///
/// Lifted from stage 1's `render_inline` otherwise unchanged, including why only the first
/// line of the parser's message survives: `ParserError`'s `Display` is four lines — the
/// message, then a `╭─►` box quoting the input — and that box is unreadable in a one-row
/// caption and would put box drawing into `tests/glyph_inventory.rs` that the manual does
/// not claim.
///
/// # Errors
///
/// [`MathError::Parse`] if the LaTeX does not parse.
pub(crate) fn parse<'a>(src: &'a str, storage: &'a Storage) -> Result<Vec<Event<'a>>, MathError> {
    let mut events = Vec::new();
    for event in Parser::new(src, storage) {
        events.push(event.map_err(|err| {
            MathError::Parse {
                message: err
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            }
        })?);
    }
    Ok(events)
}

/// Turns an event stream into a laid-out box.
///
/// # Errors
///
/// [`MathError::NotInline`] when `mode` is [`Mode::Inline`] and the formula needs a
/// second row.
pub(crate) fn build(events: &[Event<'_>], mode: Mode) -> Result<MathBox, MathError> {
    let (parts, _) = build_run(events, mode, Spacing::Normal)?;
    Ok(parts)
}

/// Whether this run writes the spaces the table asks for.
///
/// The owner's ruling: no spaces inside a script operand, because the Unicode script
/// tables have no raised space and `scripts::substitute` declines on one, which would
/// turn `x^{a+b}` from `xᵃ⁺ᵇ` into the source dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spacing {
    /// The table's answer is used.
    Normal,
    /// Every gap is zero.
    Suppressed,
}

impl Spacing {
    /// The gap actually inserted between two classes under this policy.
    const fn between(self, left: Class, right: Class) -> u16 {
        match self {
            Self::Normal => gap(left, right),
            Self::Suppressed => 0,
        }
    }
}

/// One horizontal run of events, from `events[0]` until the run's end.
///
/// Returns the box and how many events it consumed, so a caller inside a group knows
/// where to resume. The terminating [`Event::End`] is *not* counted: the run stops on it
/// and leaves it for [`group`], which opened the group and knows it has to be paid for.
fn build_run(
    events: &[Event<'_>],
    mode: Mode,
    spacing: Spacing,
) -> Result<(MathBox, usize), MathError> {
    let mut pieces: Vec<(Class, MathBox)> = Vec::new();
    let mut index = 0;
    while index < events.len() {
        match &events[index] {
            Event::End => break,
            Event::Content(content) => {
                pieces.push(atom(content));
                index += 1;
            }
            Event::Begin(grouping) => {
                let (inner, used) = group(events, index, grouping, mode, spacing)?;
                // A braced group is an atom of the enclosing run: `2{ab}` sets the same
                // cells as `2ab`, which is what makes a group transparent to spacing.
                pieces.push((Class::Ordinary, inner));
                index = index.saturating_add(used);
            }
            // Tasks 6 to 11 replace each of these in turn. Until then a construct that
            // needs a second row says so by name rather than drawing something wrong.
            Event::Visual(visual) => return Err(MathError::NotInline(visual_name(visual))),
            Event::Script { .. } => return Err(MathError::NotInline("a script")),
            Event::EnvironmentFlow(_) => {
                return Err(MathError::NotInline("a multi-row environment"));
            }
            // `\kern` and friends: a horizontal space we honour as one column, and a
            // vertical one we ignore. Neither can fail.
            Event::Space { width, .. } => {
                if width.is_some() {
                    pieces.push((Class::Ordinary, text(" ")));
                }
                index += 1;
            }
            // Font and style changes carry no cells of their own.
            Event::StateChange(_) => index += 1,
        }
    }
    Ok((assemble(pieces, spacing), index))
}

/// Applies the spacing table to a classified sequence and returns the row.
///
/// The unary pass runs first and separately: a binary operator with nothing to bind to on
/// its left is a sign, and that is a fact about the sequence, not about any one pair.
///
/// The condition below **is TeX's bin-to-ord rule, complete** — not a list of classes
/// collected as cases turned up. TeX reclassifies a `Bin` atom as `Ord` when it is first in
/// the list, or follows `Bin`, `Op`, `Rel`, `Open` or `Punct` (`TeXbook` §18, the same
/// chapter `spacing.rs` takes its table from). Those six map onto this crate's classes as
/// [`Class::Edge`] for "first in the list", [`Class::Binary`], [`Class::Relation`],
/// [`Class::Open`], [`Class::Punct`], and TeX's single `Op` as **both**
/// [`Class::Function`] and [`Class::Large`] — an operator name and a large operator are one
/// atom class in TeX and two here.
///
/// Saying which list this is matters more than the cells: five classes with no source read
/// as arbitrary and invite a sixth to be added ad hoc, which is how stage 1's spacing grew
/// its seams. This list is closed. A case that seems to want another entry is a case where
/// the *class* is wrong, not this rule.
fn assemble(mut pieces: Vec<(Class, MathBox)>, spacing: Spacing) -> MathBox {
    for i in 0..pieces.len() {
        if pieces[i].0 != Class::Binary {
            continue;
        }
        let left = if i == 0 { Class::Edge } else { pieces[i - 1].0 };
        if matches!(
            left,
            Class::Edge
                | Class::Open
                | Class::Relation
                | Class::Binary
                | Class::Punct
                | Class::Function
                | Class::Large
        ) {
            pieces[i].0 = Class::Unary;
        }
    }

    let mut parts = Vec::with_capacity(pieces.len().saturating_mul(2));
    let mut previous = Class::Edge;
    for (class, part) in pieces {
        let columns = spacing.between(previous, class);
        if columns > 0 {
            parts.push(text(" ".repeat(usize::from(columns))));
        }
        previous = class;
        parts.push(part);
    }
    row(parts)
}

/// One `Content` event as its class and its cells.
///
/// `Content::Relation` is the one payload that is not a `char`: `RelationContent` may hold
/// two, its field is private, and the only accessor is `encode_utf8_to_buf`. Stage 1
/// handles it at `src/math/inline.rs:469-473`; this is the same handling in the new shape.
///
/// The two-char relations are the sixteen `multirelation` calls at
/// `pulldown-latex-0.8.0/src/parser/primitives.rs:1157-1172` — `\approxcolon` is `≈` then
/// `:`, and six of them are a base character plus U+FE00. `≠` is *not* one of them: it is
/// a single `char`, and `\not=` does not even arrive as a relation but as
/// `Visual(Negation)` followed by `=`.
fn atom(content: &Content<'_>) -> (Class, MathBox) {
    match content {
        Content::Text(s) => (Class::Ordinary, text(*s)),
        Content::Number(s) => (Class::Ordinary, text(*s)),
        Content::Function(s) => (Class::Function, text(*s)),
        Content::Ordinary { content, .. } => (Class::Ordinary, text(content.to_string())),
        Content::LargeOp { content, .. } => (Class::Large, text(content.to_string())),
        Content::BinaryOp { content, .. } => (Class::Binary, text(content.to_string())),
        Content::Relation { content, .. } => {
            // The upstream doc comment on `encode_utf8_to_buf` requires at least eight
            // bytes: two chars of up to four bytes each.
            let mut buf = [0u8; 8];
            let encoded = content.encode_utf8_to_buf(&mut buf);
            // `encode_utf8_to_buf` writes one or two `char`s, so this is valid UTF-8 by
            // construction; the lossy conversion is here because this module may not panic
            // and `from_utf8` returns a Result that has no honest failure branch.
            (
                Class::Relation,
                text(String::from_utf8_lossy(encoded).into_owned()),
            )
        }
        Content::Delimiter { content, ty, .. } => {
            let class = match ty {
                DelimiterType::Open => Class::Open,
                DelimiterType::Close => Class::Close,
                // `\middle|` sits between two operands and reads as a relation does.
                DelimiterType::Fence => Class::Relation,
            };
            (class, text(content.to_string()))
        }
        Content::Punctuation(c) => (Class::Punct, text(c.to_string())),
    }
}

/// The group opened by `events[start]`, and how many events it accounts for.
///
/// The count covers the opening [`Event::Begin`], the run inside it and the closing
/// [`Event::End`], so the caller resumes at the event after the group. An unterminated
/// group runs off the end of the slice and the count then simply exceeds it, which ends
/// the caller's loop rather than indexing past it.
///
/// This is a `match` over the whole `Grouping` and not a chain of `if let`, because
/// later tasks replace single arms of it: [`Grouping::LeftRight`] gets a real fenced box
/// of its own, and the environments stay here until grids arrive.
fn group(
    events: &[Event<'_>],
    start: usize,
    grouping: &Grouping,
    mode: Mode,
    spacing: Spacing,
) -> Result<(MathBox, usize), MathError> {
    match grouping {
        // Transparent: `{ab}` classifies and spaces exactly as `ab` does, so the brace
        // contributes no cells and no class of its own.
        Grouping::Normal => {
            let inside = start.saturating_add(1);
            let (inner, used) = build_run(events.get(inside..).unwrap_or_default(), mode, spacing)?;
            // `used` stops at the `End`; the `Begin` and that `End` are this group's own.
            Ok((inner, used.saturating_add(2)))
        }
        // `\left( ... \right)`: the delimiters are the variant's own fields, not separate
        // `Content::Delimiter` events, so dropping this arm would silently lose them.
        // A later task turns it into `boxes::fenced`; until then it refuses by name.
        Grouping::LeftRight(..) => Err(MathError::NotInline("a delimited group")),
        other => Err(MathError::NotInline(grouping_name(other))),
    }
}

/// What a grouping is called, for [`MathError::NotInline`]'s payload.
///
/// The payload is what the caption shows the reader, so it names the construct rather
/// than the event. Copied unchanged from `src/math/inline.rs:507`: those strings are what
/// the reader is told, and rewording them would change that without anyone deciding to.
/// `Normal` and `LeftRight` never reach here — [`group`] answers both above.
fn grouping_name(grouping: &Grouping) -> &'static str {
    match grouping {
        Grouping::Matrix { .. } => "a matrix",
        Grouping::Cases { .. } => "a cases environment",
        Grouping::Array(_) | Grouping::SubArray { .. } => "an array",
        Grouping::Align { .. }
        | Grouping::Aligned
        | Grouping::Alignat { .. }
        | Grouping::Alignedat { .. } => "an aligned environment",
        _ => "a multi-row environment",
    }
}

/// What a `Visual` is, for the caption a formula shows when it cannot be drawn.
///
/// Temporary: a later task gives every one of these an inline form or a named refusal of
/// its own, and this function goes with the arm that calls it.
const fn visual_name(visual: &Visual) -> &'static str {
    match visual {
        Visual::Fraction(_) => "a fraction",
        Visual::SquareRoot => "a square root",
        Visual::Root => "a root with an index",
        Visual::Negation => "a negation",
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, Spacing, build, build_run, parse};
    use crate::math::boxes::BoxContent;
    use pulldown_latex::Storage;

    /// Builds and flattens to the one row it must be, for the inline cases.
    fn inline(src: &str) -> String {
        let storage = Storage::new();
        let events = parse(src, &storage).expect("parses");
        let b = build(&events, Mode::Inline).expect("builds inline");
        assert!(b.is_inline(), "{src} did not come back on one row");
        flatten(&b)
    }

    fn flatten(b: &crate::math::boxes::MathBox) -> String {
        match &b.content {
            BoxContent::Text(s) => s.clone(),
            BoxContent::Row(parts) => parts.iter().map(flatten).collect(),
            other => panic!("not flat: {other:?}"),
        }
    }

    #[test]
    fn a_bare_variable_is_one_text_box() {
        assert_eq!(inline("x"), "x");
    }

    #[test]
    fn a_command_resolves_to_the_character_it_names() {
        assert_eq!(inline(r"\alpha"), "α");
        assert_eq!(inline(r"\times"), "×");
    }

    #[test]
    fn a_relation_is_spaced_on_both_sides() {
        assert_eq!(inline("E=mc"), "E = mc");
    }

    #[test]
    fn a_two_character_relation_survives_whole() {
        // `RelationContent` may hold two chars, its field is private, and the only way out
        // is `encode_utf8_to_buf`. `\approxcolon` is `multirelation('≈', ':')` at
        // `pulldown-latex-0.8.0/src/parser/primitives.rs:1165` -- verified by dumping the
        // stream, which yields `RelationContent { content: ('≈', Some(':')) }`. A handler
        // that read only the first char would drop the colon and set `a ≈ b`, which is a
        // different relation rather than a worse-looking one.
        //
        // The task brief cited `\not=` for this, which is wrong: that arrives as
        // `Visual(Negation)` plus a single-char `=` relation, and is pinned separately by
        // `a_negation_is_named_rather_than_silently_dropped` below.
        assert_eq!(inline(r"a \approxcolon b"), "a ≈: b");
        assert_eq!(inline(r"a \coloneq b"), "a :− b", "the other order, too");
    }

    #[test]
    fn a_negation_is_named_rather_than_silently_dropped() {
        // `\not=` is `Visual(Negation)` followed by the `=` relation, not a two-character
        // relation. A walk that ignored `Visual` would set `a = b` for `a \not= b` --
        // wrong mathematics, silently -- so it refuses by name until a later task draws
        // it.
        let storage = Storage::new();
        let events = parse(r"a \not= b", &storage).expect("parses");
        let err = build(&events, Mode::Inline).expect_err("no negation yet");
        assert_eq!(format!("{err}"), "a negation cannot be drawn on one row");
    }

    #[test]
    fn a_binary_operator_is_spaced_but_a_leading_sign_is_not() {
        assert_eq!(inline("a+b"), "a + b");
        assert_eq!(
            inline("-x"),
            "−x",
            "a sign at the head of the formula binds tight"
        );
        assert_eq!(inline("(-x)"), "(−x)", "and after an opening delimiter");
        assert_eq!(inline("a=-b"), "a = −b", "and after a relation");
        // The other two classes the unary pass answers to. Neither is degenerate input,
        // and without them the sign keeps a right-hand space it has no operand for:
        // `f(x, − y)` and `a + − b`.
        assert_eq!(inline("f(x,-y)"), "f(x, −y)", "and after a comma");
        assert_eq!(inline("a+-b"), "a + −b", "and after another operator");
    }

    #[test]
    fn a_sign_after_an_operator_name_binds_to_its_own_operand() {
        // TeX's `Op` is one atom class covering both an operator name and a large
        // operator; this crate splits it into `Function` and `Large`, so the bin-to-ord
        // rule needs both. Without them the `-` stays `Binary` and takes a space on its
        // right that it has no operand for: `sin − x` and `∑ − x`, which read as `\sin`
        // minus `x` rather than as the sine of `−x`.
        assert_eq!(inline(r"\sin -x"), "sin −x");
        assert_eq!(inline(r"\sum -x"), "∑ −x", "the same for a large operator");

        // The space before the sign is the operator's own, and stays: an operator name
        // parts from its operand whatever that operand starts with.
        assert_eq!(inline(r"2\sin -x"), "2 sin −x");
    }

    #[test]
    fn a_function_name_parts_from_its_argument() {
        assert_eq!(inline(r"\sin x"), "sin x");
        assert_eq!(inline(r"2\sin x"), "2 sin x");
        assert_eq!(
            inline(r"-\sin x"),
            "−sin x",
            "a carried stage-1 defect, now a table cell"
        );
    }

    #[test]
    fn juxtaposition_touches() {
        assert_eq!(inline("ab"), "ab");
        assert_eq!(inline("2x"), "2x");
    }

    #[test]
    fn punctuation_parts_from_what_follows_it() {
        assert_eq!(inline("f(x,y)"), "f(x, y)");
    }

    #[test]
    fn a_brace_group_is_transparent_to_spacing() {
        // {ab} is a group, not a delimiter: the cells are the same as ab.
        assert_eq!(inline("{ab}"), "ab");
        assert_eq!(inline("2{ab}"), "2ab");

        // Both cases above put the group last, where miscounting the events it spans is
        // invisible: the walk would land on the group's own `End` and stop, having
        // already emitted everything. Only content *after* the group shows it.
        assert_eq!(inline("{ab}c"), "abc", "the walk resumes after the group");
        assert_eq!(inline("{a{b}c}d"), "abcd", "and after a nested one");
        assert_eq!(inline("{a}+{b}"), "a + b", "a group is spaced as one atom");
    }

    #[test]
    fn a_left_right_group_is_named_rather_than_silently_dropped() {
        // `Grouping::LeftRight` carries its delimiters in the variant's own fields
        // (`pulldown-latex-0.8.0/src/event.rs:316`), not as `Content::Delimiter` events.
        // Stage 1 assumed the opposite and drew `\left(\frac{a}{b}\right)^2` as `a/b²`.
        // Until a later task builds a real fence, dropping to a named refusal is the only
        // honest answer.
        let storage = Storage::new();
        let events = parse(r"\left( x \right)", &storage).expect("parses");
        let err = build(&events, Mode::Inline).expect_err("no fences yet");
        assert_eq!(
            format!("{err}"),
            "a delimited group cannot be drawn on one row"
        );
    }

    #[test]
    fn text_mode_content_is_taken_verbatim() {
        assert_eq!(inline(r"\text{if } x"), "if x");
    }

    #[test]
    fn a_construct_that_needs_a_second_row_is_named_rather_than_guessed() {
        let storage = Storage::new();
        let events = parse(r"\begin{matrix} a \\ b \end{matrix}", &storage).expect("parses");
        let err = build(&events, Mode::Inline).expect_err("cannot be one row");
        assert!(
            format!("{err}").contains("cannot be drawn on one row"),
            "the caption has to say what the reader is looking at: {err}"
        );
    }

    #[test]
    fn a_parse_failure_carries_only_the_first_line_of_the_parser_message() {
        let storage = Storage::new();
        let err = parse(r"\frac{", &storage).expect_err("does not parse");
        let message = format!("{err}");
        assert!(!message.contains('\n'), "a caption is one row: {message:?}");
        assert!(!message.contains('╭'), "no context box: {message:?}");
    }

    #[test]
    fn display_mode_builds_the_same_flat_row_for_flat_input() {
        let storage = Storage::new();
        let events = parse("E=mc", &storage).expect("parses");
        let d = build(&events, Mode::Display).expect("builds");
        let i = build(&events, Mode::Inline).expect("builds");
        assert_eq!(
            flatten(&d),
            flatten(&i),
            "flat input has one answer, not two"
        );
    }

    #[test]
    fn a_suppressed_run_writes_none_of_the_spaces_the_table_asks_for() {
        // The script-operand exception, which no caller reaches until scripts arrive.
        // Without this the multiply-by-zero is unreachable code and a mutation that
        // deletes it survives the whole suite -- and the two carried stage-1 seam cases
        // (`2{\sin x}^2`, `2{ab}^2`) are exactly the ones it exists for.
        let storage = Storage::new();
        let events = parse("a+b=c", &storage).expect("parses");

        let (normal, _) = build_run(&events, Mode::Inline, Spacing::Normal).expect("builds");
        assert_eq!(flatten(&normal), "a + b = c", "the table's own answer");

        let (tight, _) = build_run(&events, Mode::Inline, Spacing::Suppressed).expect("builds");
        assert_eq!(
            flatten(&tight),
            "a+b=c",
            "inside a script operand every gap is zero, whatever the table says"
        );
    }
}
