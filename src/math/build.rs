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
//! one exception the owner ruled — no spaces inside a script operand — is applied *here*
//! rather than in the table, because *where an operand sits* is knowledge this file has
//! and the table does not.
//!
//! That exception needs two arms, because a space reaches the row by two routes and both
//! have to answer to it. [`Spacing::between`] multiplies a looked-up gap by zero;
//! [`Spacing::writes_source_spaces`] drops a space the *source* asked for — `\,`, `\quad`,
//! `\kern` — which never passed through the table at all and so has no gap to multiply.
//! Only the first is a multiply, and calling the whole policy "multiplying by zero" named
//! one arm while leaving the other sounding covered, which is how it came to have a hole.

// Nothing outside this module's own tests calls these yet: `render_inline` is rewired onto
// this builder in a later task, so the lib target sees the whole surface as dead while the
// test target sees all of it live -- the same situation `boxes.rs` and `spacing.rs` are in,
// and for the same reason. `expect` cannot express that -- it fires
// `unfulfilled_lint_expectations` on the test target -- so this is `allow`.
//
// Measured, not assumed: with this line removed, clippy reports every item in this file as
// never used -- sixteen warnings, `between` and `writes_source_spaces` sharing one.
// `dead_code` is transitive, so this module
// being dead is also why `boxes.rs` and `spacing.rs` keep theirs -- their first caller is
// this file, and a dead caller does not make a callee live. All three come out together
// when the renderer calls in.
#![allow(dead_code)]

use pulldown_latex::event::{
    Content, DelimiterType, Dimension, Event, Grouping, ScriptPosition, ScriptType, Visual,
};
use pulldown_latex::{Parser, Storage};

use crate::error::MathError;
use crate::math::boxes::{self, BoxContent, MathBox, row, text};
use crate::math::spacing::{Class, gap};
use crate::math::{draw, scripts};

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
    let (parts, _) = build_run(events, mode, Spacing::Normal, 0)?;
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

    /// Whether a space the *source* asked for — `\,`, `\quad`, `\kern` — is written.
    ///
    /// The same answer [`Spacing::between`] gives for a looked-up gap, and for the same
    /// reason. `scripts::substitute` declines on a space whatever put it there, so an
    /// operand that kept its `\,` falls back to the source dump exactly as one that kept
    /// a table gap would. Without this the suppression has a hole in it, and the
    /// guarantee the policy exists to give would be true only of table gaps.
    const fn writes_source_spaces(self) -> bool {
        match self {
            Self::Normal => true,
            Self::Suppressed => false,
        }
    }
}

/// How deep a formula may nest before it is refused.
///
/// [`build_run`], [`element`], [`group`], [`visual_box`] and [`script_box`] are mutually
/// recursive, one round trip per nesting level, and a formula is attacker-supplied text.
/// Unbounded, `$` followed by a few thousand `{` overflows the stack, and a stack overflow
/// **aborts the process** — it is not a panic, it cannot be caught, and it is therefore
/// strictly worse than the panic design spec §9 forbids. Measured on this branch: 500
/// levels of braces build, 5000 abort with `SIGABRT`.
///
/// **A level is not always a brace.** `\sqrt\sqrt…x` opens no group at all and still
/// recurses, through [`element`] and [`visual_box`], which is why the cap is counted in
/// [`element`] as well as here — a cap that only counted [`Event::Begin`] would not see
/// that chain.
///
/// 64 sits far below where it breaks rather than just below it. No formula a person
/// writes comes close — a deep continued fraction is a dozen — and the margin is
/// deliberate: the two-dimensional constructs of later tasks add their own frames to this
/// same recursion and will spend some of it.
///
/// The margin is not the same on every construct, and the tighter one is upstream. On the
/// brace chain the parser is iterative and hands back all 10001 events; on a `\sqrt` chain
/// **the parser itself recurses**, and `parse` aborts the process somewhere between 129
/// and 144 repeats — measured on this branch. So a `\sqrt` chain never reaches the numbers
/// the brace chain does, and this cap has to stay well under the parser's, which it does
/// by rather less than the braces suggest.
const MAX_NESTING: usize = 64;

// The tests pin the *behaviour* — one past the cap refuses, well inside it builds — but
// they hold for any cap between 33 and 4999, so a cap raised to 4096 passes both while
// sitting inside the unmeasured band between "500 builds" and "5000 aborts". This pins the
// safety property the number exists for, at compile time, where the number is.
const _: () = assert!(
    MAX_NESTING <= 256,
    "MAX_NESTING must stay far below the measured overflow: 500 levels build, 5000 abort"
);

/// One horizontal run of events, from `events[0]` until the run's end.
///
/// Returns the box and how many events it consumed, so a caller inside a group knows
/// where to resume. The terminating [`Event::End`] is *not* counted: the run stops on it
/// and leaves it for [`group`], which opened the group and knows it has to be paid for.
///
/// `depth` is how many groups enclose this run; see [`MAX_NESTING`].
fn build_run(
    events: &[Event<'_>],
    mode: Mode,
    spacing: Spacing,
    depth: usize,
) -> Result<(MathBox, usize), MathError> {
    // The one check that guards the recursion, at the point the recursion re-enters, so
    // that every path into it is covered by the single test.
    if depth > MAX_NESTING {
        return Err(MathError::NotInline("a formula nested too deeply"));
    }
    let mut pieces: Vec<(Option<Class>, MathBox)> = Vec::new();
    let mut index = 0;
    while index < events.len() {
        match &events[index] {
            Event::End => break,
            // Neither of these is an atom of the run, so neither goes through [`element`]
            // here: a space the source asked for is glue, and glue has cells but no class.
            // Both halves matter. A piece that draws nothing is not the same as no piece
            // at all, and a piece that draws something is not the same as an atom: an
            // `Ordinary` between `\,` and `-x` would give the `−` a left operand it has
            // not got and set `\,-x` as `  − x`, where TeX skips the glue and sets a sign.
            // Hence `None` — [`assemble`] writes the cells and reads through them.
            Event::Space { width, .. } => {
                if let Some(cells) = source_space(*width, spacing) {
                    pieces.push((None, cells));
                }
                index += 1;
            }
            // Font and style changes carry no cells of their own.
            Event::StateChange(_) => index += 1,
            _ => {
                let (class, piece, used) = element(events, index, mode, spacing, depth)?;
                pieces.push((Some(class), piece));
                index = index.saturating_add(used);
            }
        }
    }
    Ok((assemble(pieces, spacing), index))
}

/// The cells a `Space` event contributes, or `None` for the ones that contribute none.
///
/// `\,`, `\quad`, `\kern`: a horizontal space we honour as one column, and a vertical one
/// we ignore. Neither can fail.
///
/// This is the only cell the walk writes that no table lookup produced, so it answers to
/// both of the constraints a looked-up gap answers to.
///
/// *Suppressed* — a script operand takes no spaces at all, and one that came from `\,` is
/// no more substitutable than one that came from the table.
///
/// *Positive* — `\!`, `\negthinspace`, `\negmedspace` and `\negthickspace` are
/// `Space { width: Some(Dimension { value: -3.0/18.0, .. }) }` and friends;
/// `pulldown-latex` labels them "Negative spacing" at `parser/primitives.rs:827-841`.
/// `width.is_some()` is true for every one of them, so testing only that drew `a\!b` as
/// `a b` — wider than `ab`, where the author asked for tighter — and gave
/// `\int\!\!\!\int` three visible gaps. A terminal cannot set a negative width, so the
/// honest answer is no column. The comparison is `> 0.0` and not `>= 0.0`: `\kern` takes
/// an arbitrary dimension, and a zero-width one asked for nothing, so it gets nothing.
fn source_space(width: Option<Dimension>, spacing: Spacing) -> Option<MathBox> {
    let widens = matches!(width, Some(dimension) if dimension.value > 0.0);
    (widens && spacing.writes_source_spaces()).then(|| text(" "))
}

/// Applies the spacing table to a classified sequence and returns the row.
///
/// The unary pass runs first and separately: a binary operator with nothing to bind to on
/// its left is a sign, and that is a fact about the sequence, not about any one pair.
///
/// This is the `TeXbook`'s bin-to-ord rule, **both halves of it**. An operator is only an
/// operator when it has an operand on each side, and TeX states that as two conditions:
///
/// 1. A `Bin` with nothing on its **left** to bind to — first in the list, or after `Bin`,
///    `Op`, `Rel`, `Open` or `Punct` — becomes `Ord`. That is the sign of `-x`.
/// 2. A `Bin` with nothing on its **right** to bind to — immediately before `Rel`, `Close`
///    or `Punct` — becomes `Ord`. That is the `+` of `(a+)`, which TeX sets tight.
///
/// They map onto this crate's classes with [`Class::Edge`] standing for both ends of the
/// list, and TeX's single `Op` splitting into **both** [`Class::Function`] and
/// [`Class::Large`] — an operator name and a large operator are one atom class in TeX and
/// two here. Condition 2 is written as a right-neighbour test rather than TeX's
/// look-back-from-the-next-atom so that `Edge` covers the end of the list, which is the
/// exact mirror of "first in the list" in condition 1: `a+` sets `a+`, not `a +`.
///
/// **That trailing-`Bin` half of condition 2 is held at high confidence, not certainty.**
/// Its supports are the mirror argument above and a recollection of TeX's own end-of-list
/// handling that could not be opened from here — the same unverifiable source as the
/// missing chapter, and it is recorded rather than smoothed away for the same reason. What
/// raised it above a guess is a structural check: `Class::Edge` here bounds a **run**, not
/// the formula, so condition 2 already fires at every brace boundary — `{a+}b` sets `a+b`,
/// because `{a+}` is its own list and its trailing `Bin` is demoted inside it. An
/// over-eager extension would have shown first at that boundary and it does not. If the
/// reference is ever checked and says otherwise, `Class::Edge` is one entry to remove from
/// the right-context set and one assertion to drop.
///
/// The demotion in condition 2 is to [`Class::Ordinary`], which is what TeX says. It is
/// *not* [`Class::Unary`], even though the two are indistinguishable here — column `Unary`
/// and column `Ordinary` are identical in every row of `spacing`'s table, and the rows
/// differ only at `Function` and `Large`, which condition 2 can never be followed by. So
/// the choice is unobservable and is made on honesty: [`Class::Unary`] is documented as a
/// sign with no *left* operand, and the `+` of `(a+)` has one.
///
/// No chapter is cited on purpose. The rule's *content* is well established and is what
/// the conditions above are checked against; its exact location in the book was not
/// verifiable when this was written, and a chapter number that might be wrong is worse
/// than none — every defect found in this module so far has been a confidently-stated
/// wrong value. If you have the book to hand, add the reference.
///
/// Saying which rule this is matters more than the cells: conditions with no source read
/// as arbitrary and invite another to be added ad hoc, which is how stage 1's spacing grew
/// its seams. **This rule is closed.** A case that seems to want a third condition is a
/// case where the *class* is wrong, not this rule.
///
/// The pass reclassifies in place and reads its left neighbour after that neighbour has
/// been decided, which is what makes a run of signs work: in `a+--b` the second `−` sees
/// the first already demoted, so only one of them is a sign and the result is `a + − − b`.
fn assemble(mut pieces: Vec<(Option<Class>, MathBox)>, spacing: Spacing) -> MathBox {
    for i in 0..pieces.len() {
        if pieces[i].0 != Some(Class::Binary) {
            continue;
        }
        // Both neighbours are found by looking *through* the classless pieces, which is
        // TeX's own rule: bin-to-ord looks back at the most recent previous atom, and
        // glue is not an atom. `\,-x` sets a sign exactly as `-x` does.
        let left = neighbour(pieces.iter().take(i).rev());
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
            pieces[i].0 = Some(Class::Unary);
            continue;
        }
        // Reading the *unreclassified* right neighbour is correct: this pass only ever
        // turns `Binary` into `Unary` or `Ordinary`, and neither is in the set below, so
        // a neighbour decided later cannot change this answer.
        let right = neighbour(pieces.iter().skip(i.saturating_add(1)));
        if matches!(
            right,
            Class::Relation | Class::Close | Class::Punct | Class::Edge
        ) {
            pieces[i].0 = Some(Class::Ordinary);
        }
    }

    let mut parts = Vec::with_capacity(pieces.len().saturating_mul(2));
    let mut previous = Class::Edge;
    for (class, part) in pieces {
        // A classless piece writes its cells and leaves `previous` alone, so the two
        // atoms it sits between are still a pair for the table: `a\,=b` takes the source
        // space *and* the relation's own column, which is what TeX sets.
        if let Some(class) = class {
            let columns = spacing.between(previous, class);
            if columns > 0 {
                parts.push(text(" ".repeat(usize::from(columns))));
            }
            previous = class;
        }
        parts.push(part);
    }
    row(parts)
}

/// The class of the first classed piece in `towards`, or [`Class::Edge`] if there is none.
///
/// The caller passes the pieces on one side of a [`Class::Binary`], nearest first; a run
/// that has nothing but glue on that side has nothing to bind to, which is what `Edge`
/// says.
fn neighbour<'a>(mut towards: impl Iterator<Item = &'a (Option<Class>, MathBox)>) -> Class {
    towards.find_map(|piece| piece.0).unwrap_or(Class::Edge)
}

/// One `Content` event as its class and its cells.
///
/// `Content::Relation` is the one payload that is not a `char`: `RelationContent` may hold
/// two, its field is private, and the only accessor is `encode_utf8_to_buf`. Stage 1 does
/// the same job at `src/math/inline.rs:469-473`, but not the same way: it uses
/// `from_utf8(..).unwrap_or_default()`, which drops the whole relation if the bytes were
/// ever not UTF-8, where the lossy conversion below keeps what it can. Neither branch is
/// reachable — `encode_utf8_to_buf` writes `char`s — so this is a difference in what each
/// would do if the impossible happened, not in what either draws.
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
    depth: usize,
) -> Result<(MathBox, usize), MathError> {
    match grouping {
        // Transparent: `{ab}` classifies and spaces exactly as `ab` does, so the brace
        // contributes no cells and no class of its own.
        Grouping::Normal => {
            let inside = start.saturating_add(1);
            let (inner, used) = build_run(
                events.get(inside..).unwrap_or_default(),
                mode,
                spacing,
                depth.saturating_add(1),
            )?;
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

/// One element, and the class it takes in the run around it.
///
/// "Element" is the crate's own word for what a `Visual` or a `Script` governs, and the
/// counts are given per variant (`$PL/src/event.rs:157-172,178-188`). A whole
/// `Begin…End` group counts as one, and so does a construct that governs elements of its
/// own — which is why this dispatches rather than deferring to [`build_run`] over a
/// one-event slice. `x^\frac{a}{b}` and `\sqrt{x}^2` are both a construct whose element
/// begins with the event that *introduces* another construct, and a one-event slice cuts
/// that construct off from the elements it governs.
///
/// The returned class is the base's own, so that a scripted piece is spaced by what it is
/// rather than by what happened to it: `\sum_{i=1}^{n} i` sets `∑ᵢ₌₁ⁿ i` because `∑` is
/// still a [`Class::Large`] after its limits are folded into it, and `x^2 + 1` sets
/// `x² + 1` because `x` is still ordinary after its.
fn element(
    events: &[Event<'_>],
    at: usize,
    mode: Mode,
    spacing: Spacing,
    depth: usize,
) -> Result<(Class, MathBox, usize), MathError> {
    // The second of the two places the recursion is capped; see [`MAX_NESTING`]. This one
    // catches the chains that open no group -- `\sqrt\sqrt…x` -- which the check in
    // [`build_run`] never sees because they never re-enter it.
    if depth > MAX_NESTING {
        return Err(MathError::NotInline("a formula nested too deeply"));
    }
    let deeper = depth.saturating_add(1);
    match events.get(at) {
        // A construct whose element is missing. No input reaches this: `pulldown-latex`
        // answers `{\sqrt}`, `{x^}`, `\sqrt`, `x^` and `\frac{a}{\sqrt}` alike with
        // "expected a token" and never returns a stream -- measured, not assumed. It is
        // written all the same, because the alternative is not a wrong rendering but a
        // hang: an element of zero events leaves the caller's walk exactly where it
        // started. An *empty group* is a different thing and does arrive -- `\sqrt{}` is
        // `Begin`, `End` -- and it is an element, of two events, that draws nothing.
        None | Some(Event::End) => Err(MathError::NotInline("an unfinished construct")),
        Some(Event::Content(content)) => {
            let (class, cells) = atom(content);
            Ok((class, cells, 1))
        }
        // A braced group is an atom of the enclosing run: `2{ab}` sets the same cells as
        // `2ab`, which is what makes a group transparent to spacing. `group` accounts for
        // the depth of what is inside it, so it is passed `depth` and not `deeper`.
        Some(Event::Begin(grouping)) => {
            let (inner, used) = group(events, at, grouping, mode, spacing, depth)?;
            Ok((Class::Ordinary, inner, used))
        }
        Some(Event::Visual(visual)) => visual_box(events, at, *visual, mode, spacing, deeper),
        Some(Event::Script { ty, position }) => {
            script_box(events, at, *ty, *position, mode, spacing, deeper)
        }
        Some(Event::EnvironmentFlow(_)) => Err(MathError::NotInline("a multi-row environment")),
        // Neither carries an atom of its own, so as an element each draws nothing —
        // `x^\,` and `x^\bf` ask for an empty script, and an empty script declines
        // (`scripts::substitute`). They are elements all the same: a construct that
        // skipped them would take the event *after* as its element and lose count.
        Some(Event::Space { width, .. }) => Ok((
            Class::Ordinary,
            source_space(*width, spacing).unwrap_or_else(|| row(Vec::new())),
            1,
        )),
        Some(Event::StateChange(_)) => Ok((Class::Ordinary, row(Vec::new()), 1)),
    }
}

/// A `Visual` and the elements it governs.
///
/// The counts are the crate's, documented per variant at `$PL/src/event.rs:157-172`:
/// `SquareRoot` takes one following element, `Root` and `Fraction` take two — radicand
/// first, then index — and `Negation` applies to the next event whatever it is.
///
/// In [`Mode::Display`] each becomes the two-dimensional box `boxes` builds for it. In
/// [`Mode::Inline`] it rewrites itself onto one row, per design spec §5.2, or says why it
/// cannot.
fn visual_box(
    events: &[Event<'_>],
    at: usize,
    visual: Visual,
    mode: Mode,
    spacing: Spacing,
    depth: usize,
) -> Result<(Class, MathBox, usize), MathError> {
    let after = at.saturating_add(1);
    match visual {
        Visual::Fraction(_) => {
            let (_, num, a) = element(events, after, mode, spacing, depth)?;
            let (_, den, b) = element(events, after.saturating_add(a), mode, spacing, depth)?;
            let used = 1usize.saturating_add(a).saturating_add(b);
            let cells = match mode {
                Mode::Display => boxes::fraction(num, den),
                // The one-row rewrite. The slash carries no class and no gap of its own:
                // `a/b` is set tight, and what keeps `\frac{a+b}{c}` from reading as the
                // different expression `a + b/c` is the parentheses, not a space.
                Mode::Inline => row(vec![bracketed(num), text("/"), bracketed(den)]),
            };
            Ok((Class::Ordinary, cells, used))
        }
        Visual::SquareRoot => {
            let (_, radicand, a) = element(events, after, mode, spacing, depth)?;
            let cells = match mode {
                Mode::Display => boxes::radical(radicand, None),
                Mode::Inline => row(vec![text("√"), bracketed(radicand)]),
            };
            Ok((Class::Ordinary, cells, 1usize.saturating_add(a)))
        }
        Visual::Root => {
            // Declined before the operands are built, so the caption names the root and
            // not whatever the radicand happens to contain.
            //
            // `3√x` reads as three times a square root, which is a different number.
            // Declining shows the source instead, which is design spec §9's whole point:
            // a formula in the wrong place, not an error.
            if matches!(mode, Mode::Inline) {
                return Err(MathError::NotInline("a root with an index"));
            }
            let (_, radicand, a) = element(events, after, mode, spacing, depth)?;
            let (_, index, b) = element(events, after.saturating_add(a), mode, spacing, depth)?;
            let used = 1usize.saturating_add(a).saturating_add(b);
            Ok((Class::Ordinary, boxes::radical(radicand, Some(index)), used))
        }
        Visual::Negation => {
            let (class, inner, a) = element(events, after, mode, spacing, depth)?;
            // U+0338 COMBINING LONG SOLIDUS OVERLAY. The crate leaves the rendering to us
            // (`$PL/src/event.rs:166-171`) and this is what a terminal can do: the
            // overlay follows the character it strikes, and measures no column of its own.
            //
            // The negated thing keeps its class, so `a \not= b` is still a relation and
            // still takes a space either side. Classing it `Ordinary` would set `a≠b`.
            Ok((
                class,
                row(vec![inner, text("\u{338}")]),
                1usize.saturating_add(a),
            ))
        }
    }
}

/// A base and its scripts.
///
/// `ScriptPosition::Movable` is the crate's "above and below by preference, to the right
/// when inline" (`$PL/src/event.rs:193-201`), so [`Mode`] resolves it. This is the third
/// and last place in this file that reads the flag.
fn script_box(
    events: &[Event<'_>],
    at: usize,
    ty: ScriptType,
    position: ScriptPosition,
    mode: Mode,
    spacing: Spacing,
    depth: usize,
) -> Result<(Class, MathBox, usize), MathError> {
    let after = at.saturating_add(1);
    let (class, base, a) = element(events, after, mode, spacing, depth)?;
    // Operands of a script are built with spacing suppressed: there is no raised space in
    // Unicode, so a space in the operand makes the substitution decline and `x^{a+b}`
    // would be written flat as `x^{a + b}` instead of raised at all. The owner ruled this
    // exception; it lives here rather than in the table because it is a fact about
    // position, not about a pair. Display keeps the enclosing policy, because a script
    // drawn on its own row can hold a space like any other row.
    let operand_spacing = match mode {
        Mode::Inline => Spacing::Suppressed,
        Mode::Display => spacing,
    };
    let first = after.saturating_add(a);
    let (sub, sup, used) = match ty {
        ScriptType::Subscript => {
            let (_, s, b) = element(events, first, mode, operand_spacing, depth)?;
            (Some(s), None, 1usize.saturating_add(a).saturating_add(b))
        }
        ScriptType::Superscript => {
            let (_, s, b) = element(events, first, mode, operand_spacing, depth)?;
            (None, Some(s), 1usize.saturating_add(a).saturating_add(b))
        }
        // Base, then subscript, then superscript, whichever order the source wrote them.
        ScriptType::SubSuperscript => {
            let (_, lo, b) = element(events, first, mode, operand_spacing, depth)?;
            let (_, hi, c) = element(
                events,
                first.saturating_add(b),
                mode,
                operand_spacing,
                depth,
            )?;
            (
                Some(lo),
                Some(hi),
                1usize.saturating_add(a).saturating_add(b).saturating_add(c),
            )
        }
    };

    let stacked = matches!(position, ScriptPosition::AboveBelow)
        || (matches!(position, ScriptPosition::Movable) && matches!(mode, Mode::Display));

    let cells = match (mode, stacked) {
        (Mode::Display, true) => boxes::limits(base, sub, sup),
        (Mode::Display, false) => boxes::scripts(base, sub, sup),
        (Mode::Inline, _) => {
            // The base is bracketed for the same reason a fraction's part is: `2{ab}^2`
            // is `2(ab)²`, and `2ab²` reads as `2·a·b²`. The *operands* are not — a
            // raised group delimits itself, and Unicode has `⁽⁾`, so bracketing them
            // would set `x⁽ᵃ⁺ᵇ⁾` where `xᵃ⁺ᵇ` is what was written.
            let mut out = draw::to_row(&bracketed(base))?;
            // All-or-nothing per group, and the unit is the *group*: substitute the whole
            // operand or none of it — a partial substitution renders `a_{bc}` as `a_b c`,
            // a different expression — and where none of it can be substituted, write the
            // group flat with the marker the author typed (design spec §5.1). The two
            // groups of one script are decided separately, so `x_i^q` sets `xᵢ^q`.
            //
            // Refusing the formula instead would be a different rule: §5.1 says this
            // formula *is* representable on one row, so §9's fallback to the source is
            // not the answer here.
            if let Some(sub) = &sub {
                out.push_str(&scripts::lowered(&draw::to_row(sub)?));
            }
            if let Some(sup) = &sup {
                out.push_str(&scripts::raised(&draw::to_row(sup)?));
            }
            text(out)
        }
    };
    Ok((class, cells, used))
}

/// Whether a box draws a single atom, and so cannot be misread when something is set
/// against it.
///
/// Design spec §5.2 asks for parentheses "when either part is not a single atom".
/// *Atom* is this engine's own word — one piece of a run — so this counts pieces and not
/// characters: `x²` is one piece and as unmisreadable as `x`, while `ab` is two, and two
/// is what `\frac{1}{2a}` and `2{ab}^2` have to bracket. An empty box is not one atom, so
/// `\frac{}{b}` sets `()/b` rather than `/b`.
///
/// A test for "already parenthesised" would be the wrong rule: `\frac{(a)+(b)}{c}` has
/// parentheses at both ends and is still three atoms, which is the exact misreading the
/// bracket exists to prevent.
fn is_one_atom(b: &MathBox) -> bool {
    match &b.content {
        BoxContent::Row(parts) => matches!(parts.as_slice(), [only] if is_one_atom(only)),
        _ => true,
    }
}

/// `b` in parentheses unless it is one atom (design spec §5.2).
///
/// `a/b` needs none and `√x` needs none; `a + b/c` is a different expression from
/// `(a + b)/c`, and this is the only thing standing between the two.
fn bracketed(b: MathBox) -> MathBox {
    if is_one_atom(&b) {
        b
    } else {
        row(vec![text("("), b, text(")")])
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_NESTING, Mode, Spacing, build, build_run, draw, parse};
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
    fn a_negation_strikes_the_element_it_applies_to_and_keeps_its_class() {
        // `\not=` is `Visual(Negation)` followed by the `=` relation, not a two-character
        // relation. A walk that ignored `Visual` would set `a = b` for `a \not= b` --
        // wrong mathematics, silently.
        //
        // The overlay follows the character it strikes and measures no column, so the
        // whole thing is one cell wide. And it is still a relation: classing the negated
        // box `Ordinary` would set `a≠b`, which is why the spacing is asserted here and
        // not just the characters.
        assert_eq!(inline(r"a \not= b"), "a =\u{338} b");
        let storage = Storage::new();
        let events = parse(r"a \not= b", &storage).expect("parses");
        let b = build(&events, Mode::Inline).expect("builds");
        assert_eq!(b.width, 5, "a, space, the struck =, space, b");
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
        // Distinguishes reclassifying in place from deciding every atom against a
        // snapshot of the original classes. In place, the second `−` sees the first
        // already demoted and so keeps its operator spacing; against a snapshot it would
        // see a `Binary` and become a second sign, giving `a + −−b`.
        assert_eq!(inline("a+--b"), "a + − − b", "one sign, not a run of them");
    }

    #[test]
    fn an_operator_with_no_right_operand_is_not_an_operator_either() {
        // The second half of the bin-to-ord rule. A `+` immediately before a closing
        // delimiter, a punctuation mark or a relation has nothing on its right to bind
        // to, so it is no longer an operator and stops claiming an operator's space.
        assert_eq!(inline("(a+)"), "(a+)", "not (a +)");
        assert_eq!(inline("a+,b"), "a+, b", "not a +, b");
        assert_eq!(inline("a+=b"), "a+ = b", "not a + = b");
        // The end of the formula is the mirror of "first in the list" in the first half:
        // `Edge` bounds the run on both sides, so a trailing operator is demoted exactly
        // as a leading one is.
        assert_eq!(inline("a+"), "a+", "not a +");

        // `Class::Edge` bounds a *run*, not the formula, so condition 2 fires at a brace
        // boundary as well: `{a+}` is its own list and its trailing operator is demoted
        // inside it. This is where an over-eager extension of the rule would show first,
        // which is what makes it worth asserting rather than merely true.
        assert_eq!(inline("{a+}b"), "a+b", "a run ends at a brace, too");
        assert_eq!(inline("{a+}{+b}"), "a++b", "both boundaries, both halves");
    }

    #[test]
    fn a_negative_space_never_widens_the_row() {
        // `\!` and its relatives are `Space { width: Some(-3/18 em) }`, so `width.is_some()`
        // is true for them and testing only that drew `a\!b` wider than `ab` -- the
        // opposite of what the author asked for. A terminal cannot set a negative width,
        // so the honest answer is no column at all.
        assert_eq!(
            inline(r"a\!b"),
            "ab",
            "a negative space cannot widen anything"
        );
        assert_eq!(
            inline(r"a\,b"),
            "a b",
            "a positive one still writes its column"
        );
        // A zero-width `\kern` asked for nothing and gets nothing. This is the boundary
        // the `> 0.0` comparison sits on: with `>= 0.0` every assertion in this module
        // still passes and only this line fails.
        assert_eq!(inline(r"a\kern0pt b"), "ab", "zero is not a column");
        assert_eq!(
            inline(r"a\kern1pt b"),
            "a b",
            "and a positive kern still is"
        );

        // The idiom that exists to *close* a gap must not open three. What is left is one
        // space, and it is the table's `(Large, Large)` cell rather than anything the
        // `\!`s did -- which is what the comparison says, and the reason to write it as a
        // comparison instead of a literal.
        assert_eq!(
            inline(r"\int\!\!\!\int"),
            inline(r"\int\int"),
            "three negative spaces changed nothing, which is all a cell grid can honour"
        );
        // The literal is the canary, not the invariant. It records what that one space is
        // *today*; a ruling on `spacing`'s `(Large, Large)` cell would change it, and the
        // next reader should update this line rather than investigate it. The comparison
        // above is what must never change, and the two fail for opposite reasons.
        assert_eq!(inline(r"\int\!\!\!\int"), "∫ ∫");
    }

    #[test]
    fn a_source_space_writes_its_cells_without_becoming_an_atom() {
        // A space the source asked for is glue, and glue is not an atom, so the bin-to-ord
        // pass looks straight through it: the `−` still has nothing on its left to bind
        // to and is still a sign. Pushed as an `Ordinary` the space gave it a left operand
        // and set `  − x` -- two columns and a spaced operator, where the author asked for
        // a thin space and a negative number.
        assert_eq!(inline(r"\,-x"), " −x");
        assert_eq!(inline(r"(\,-x)"), "( −x)");
        // The pair that tells glue from an atom, and the reason the fix is "no class"
        // rather than "no piece": `{}` *is* an ordinary atom in TeX, so it does give the
        // `−` a left operand, and these two inputs must not render alike.
        assert_eq!(inline(r"{}-x"), " − x");
        // Reading through the glue does not swallow it, and the two atoms it sits between
        // are still a pair for the table -- the relation keeps its own column as well.
        assert_eq!(inline(r"a\,b"), "a b");
        assert_eq!(inline(r"a\,=b"), "a  = b");
    }

    #[test]
    fn a_formula_nested_beyond_the_cap_is_refused_rather_than_overflowing_the_stack() {
        // Unguarded this aborts the process rather than panicking: measured on this
        // branch, 500 levels build and 5000 abort with SIGABRT, which `#[should_panic]`
        // and `catch_unwind` are both powerless against. The parser itself is iterative
        // and hands back all 10001 events happily, so the recursion here is the only
        // thing standing between a formula and the process.
        let nest = |depth: usize| "{".repeat(depth) + "x" + &"}".repeat(depth);

        // One past the cap, asserted FIRST and deliberately so. Without the guard, 65
        // levels recurse perfectly well, `build` returns `Ok`, and this line fails as a
        // named test in microseconds. The deep case below would instead take the whole
        // binary down with `SIGABRT`, which no test can report and `catch_unwind` cannot
        // reach -- so the order here is what turns "the suite went red somehow" into a
        // diagnosis. It also catches `group` passing `depth` instead of `depth + 1`.
        //
        // Written against `MAX_NESTING` rather than a literal so it follows the cap.
        assert_eq!(
            refusal(&nest(MAX_NESTING + 1)),
            "a formula nested too deeply cannot be drawn on one row"
        );

        // Far past it: this is the one that documents the real hazard, and it is only
        // reached when the guard is present, because the assertion above panics first.
        assert_eq!(
            refusal(&nest(5000)),
            "a formula nested too deeply cannot be drawn on one row"
        );

        // And the cap is not so tight that ordinary nesting trips it -- including at the
        // cap exactly, which is the other side of the boundary above.
        assert_eq!(inline(&nest(32)), "x");
        assert_eq!(
            inline(&nest(MAX_NESTING)),
            "x",
            "the cap itself still builds"
        );
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

    /// The caption a formula shows when it cannot be drawn, as the reader would read it.
    fn refusal(src: &str) -> String {
        let storage = Storage::new();
        let events = parse(src, &storage).expect("parses");
        let err = build(&events, Mode::Inline).expect_err("cannot be one row");
        format!("{err}")
    }

    #[test]
    fn a_construct_that_needs_a_second_row_is_named_rather_than_guessed() {
        // `assert_eq!` on the whole caption, not `contains`. Against `contains("cannot be
        // drawn on one row")` every arm of `grouping_name` could be deleted and the
        // catch-all left standing, and this test would still pass -- the payload is the
        // only part that says what the reader is looking at, so it is the part to pin.
        assert_eq!(
            refusal(r"\begin{matrix} a \\ b \end{matrix}"),
            "a matrix cannot be drawn on one row"
        );
        assert_eq!(
            refusal(r"\begin{cases} a \\ b \end{cases}"),
            "a cases environment cannot be drawn on one row"
        );
        assert_eq!(
            refusal(r"\begin{array}{c} a \end{array}"),
            "an array cannot be drawn on one row"
        );
        assert_eq!(
            refusal(r"\begin{aligned} a \end{aligned}"),
            "an aligned environment cannot be drawn on one row"
        );
    }

    #[test]
    fn an_indexed_root_has_no_honest_one_row_form() {
        // The one `Visual` with no inline rewrite. It is named, and named for what it is:
        // the other three all draw now, so a caption saying anything else here would be
        // reporting the wrong construct.
        assert_eq!(
            refusal(r"\sqrt[3]{x}"),
            "a root with an index cannot be drawn on one row"
        );

        // Declined before its operands are built. With the check placed after them,
        // `\sqrt[3]{\begin{matrix} a \end{matrix}}` reports the matrix -- true of
        // something inside it, but not the reason this formula cannot be drawn.
        assert_eq!(
            refusal(r"\sqrt[3]{\begin{matrix} a \end{matrix}}"),
            "a root with an index cannot be drawn on one row"
        );
    }

    #[test]
    fn a_script_group_with_no_form_is_written_flat_one_group_at_a_time() {
        // Design spec §5.1: a group is substituted only if every character in it has a
        // form, and a group that has none is written with the marker the author typed --
        // it is not refused. §9's fallback to the source is for a formula that cannot be
        // drawn on one row, and this one can. Unicode has no superscript `q` and no
        // subscript `b`.
        assert_eq!(inline("x_b"), "x_b");
        assert_eq!(inline("x^q"), "x^q");
        assert_eq!(
            inline("a_{bc}"),
            "a_{bc}",
            "the braces stay: a_bc reads as (a_b)c"
        );
        assert_eq!(
            inline("x^{2q}"),
            "x^{2q}",
            "the braces stay: x^2q reads as (x^2)q"
        );
        assert_eq!(inline("x_b^q"), "x_b^q", "both sides, both flat");
        // The case that says *per group* and not per formula: one group has forms and
        // the other has not, and each is decided on its own. A per-formula rule would
        // give `x_i^q` back as source, or throw the subscript's substitution away.
        assert_eq!(inline("x_i^q"), "xᵢ^q");
        // Per group and not per character, either: the inner `y^2` is raised on its own,
        // and `²` has no raised form, so the outer group keeps that substitution and
        // falls back around it.
        assert_eq!(inline("x^{y^2}"), "x^{y²}");
        // What a script still refuses: an operand that cannot be drawn at all. The
        // caption names the operand, because the script itself is fine.
        assert_eq!(
            refusal(r"x^{\sqrt[3]{2}}"),
            "a root with an index cannot be drawn on one row"
        );
    }

    #[test]
    fn a_state_change_carries_no_cells_but_still_advances_the_walk() {
        // `\bf` emits a bare `StateChange` and nothing else. Drop its `index += 1` and
        // the walk never advances: not a wrong rendering but a hang, which no assertion
        // about output can catch. `\text{if }` does not reach this arm -- a text argument
        // with no font emits only `Content::Text` -- so without this the arm has no test.
        assert_eq!(inline(r"x \bf y"), "xy");
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

        let (normal, _) = build_run(&events, Mode::Inline, Spacing::Normal, 0).expect("builds");
        assert_eq!(flatten(&normal), "a + b = c", "the table's own answer");

        let (tight, _) = build_run(&events, Mode::Inline, Spacing::Suppressed, 0).expect("builds");
        assert_eq!(
            flatten(&tight),
            "a+b=c",
            "inside a script operand every gap is zero, whatever the table says"
        );

        // A space the source asked for is the one cell that reaches the row without a
        // table lookup, so suppression has to be asserted separately for it. Left out,
        // `x^{a\,b}` builds an operand containing a literal space, `scripts::substitute`
        // declines on it, and the formula falls back to the source dump -- the exact
        // outcome the no-spaces-in-an-operand ruling exists to prevent.
        let spaced = parse(r"a\,b", &storage).expect("parses");
        let (normal, _) = build_run(&spaced, Mode::Inline, Spacing::Normal, 0).expect("builds");
        assert_eq!(flatten(&normal), "a b", "the source asked for it");
        let (tight, _) = build_run(&spaced, Mode::Inline, Spacing::Suppressed, 0).expect("builds");
        assert_eq!(
            flatten(&tight),
            "ab",
            "and inside an operand it is dropped like any other space"
        );
    }

    #[test]
    fn an_inline_fraction_becomes_a_slash() {
        assert_eq!(inline(r"\frac{a}{b}"), "a/b");
        // Both parts are more than one atom, so both are parenthesised, and each part
        // keeps the spacing the table gives it inside its own brackets.
        assert_eq!(inline(r"\frac{-b}{2a}"), "(−b)/(2a)");
        assert_eq!(inline(r"\frac{a+b}{c}"), "(a + b)/c");
        // Swapping the two parts is invisible whenever they are the same shape, so this
        // is the assertion that pins which is which.
        assert_eq!(inline(r"\frac{a}{bb}"), "a/(bb)");
        // The walk resumes after the fraction. With the element count short by one, `c`
        // is swallowed; long by one, it is dropped.
        assert_eq!(inline(r"\frac{a}{b}c"), "a/bc");
    }

    #[test]
    fn an_inline_square_root_takes_the_radical_sign() {
        assert_eq!(inline(r"\sqrt{x}"), "√x");
        assert_eq!(inline(r"\sqrt{b+4}"), "√(b + 4)");
        assert_eq!(inline(r"\sqrt{x}y"), "√xy", "and the walk resumes after it");
    }

    #[test]
    fn a_rewritten_fraction_or_radical_is_an_ordinary_atom_of_the_run_around_it() {
        // The rewrite does not decide the class: `a/b` and `√x` are ordinary atoms, and
        // the run around them spaces them as such. Every other test in this module places
        // them where several classes give the same cells -- first in the run, where a
        // `Binary` is demoted to `Unary` and `gap(Edge, Unary) == gap(Edge, Ordinary)`,
        // or before an `Ordinary`, where the `Unary` and `Ordinary` columns agree again.
        // Both neighbours here discriminate: an `Ordinary` sits tight on either side, and
        // a `Binary` keeps its class between two operands and would take a column either
        // side, setting `x a/b y` and `x √y z`.
        assert_eq!(inline(r"x\frac{a}{b}y"), "xa/by");
        assert_eq!(inline(r"x\sqrt{y}z"), "x√yz");
    }

    #[test]
    fn an_operand_of_more_than_one_atom_is_parenthesised() {
        // Design spec 5.2. This is the boundary itself, not the two sides of it: one atom
        // is bare, two atoms are bracketed, and the second atom is the whole difference.
        assert_eq!(inline(r"\sqrt{a}"), "√a", "one atom");
        assert_eq!(inline(r"\sqrt{ab}"), "√(ab)", "two");
        assert_eq!(inline(r"\frac{1}{2a}"), "1/(2a)");

        // A nested group is still whatever is inside it. `{{ab}}` is one *piece* holding
        // one piece holding two, so a check that stopped at the first level would call it
        // a single atom and set `ab/c`, which reads as `a·b/c`.
        assert_eq!(inline(r"\frac{{ab}}{c}"), "(ab)/c");

        // A construct that draws several characters is still one atom: `x²` cannot be
        // misread, so bracketing it would only add noise.
        assert_eq!(inline(r"\frac{x^2}{c}"), "x²/c");

        // An empty group is not one atom either, so it keeps its brackets and the reader
        // can see that the author wrote nothing there. Dropping them sets `√` and `/b`,
        // which look like a typo in this crate rather than one in the document.
        assert_eq!(inline(r"\sqrt{}"), "√()");
        assert_eq!(inline(r"\frac{}{b}"), "()/b");
    }

    #[test]
    fn an_inline_script_is_substituted_when_every_character_has_a_form() {
        assert_eq!(inline("x^2"), "x²");
        assert_eq!(inline("x^{-1}"), "x⁻¹", "the U+2212 minus, not ASCII");
        assert_eq!(inline("e^{2n}"), "e²ⁿ");
        assert_eq!(inline("a_1"), "a₁");
        assert_eq!(inline("a_i"), "aᵢ");
        assert_eq!(
            inline("x^2y"),
            "x²y",
            "and the walk resumes after the script"
        );
        // Subscript then superscript, whichever order the source wrote them in.
        assert_eq!(inline("a_i^n"), "aᵢⁿ");
        assert_eq!(
            inline("a^n_i"),
            "aᵢⁿ",
            "the same, written the other way round"
        );
    }

    #[test]
    fn no_spaces_are_set_inside_a_script_operand() {
        // There is no raised space, so the substitution would decline on one and the
        // whole formula would fall back to its source.
        assert_eq!(inline("x^{a+b}"), "xᵃ⁺ᵇ");
        assert_eq!(
            inline(r"x^{a\,b}"),
            "xᵃᵇ",
            "including one the source asked for"
        );
        // The base is *not* an operand and keeps the enclosing spacing: `\sin` still
        // parts from the `2` before it.
        assert_eq!(inline(r"2\sin^2 x"), "2 sin² x");
    }

    #[test]
    fn a_scripted_piece_is_spaced_by_what_its_base_is() {
        // A big operator is still a big operator once its limits are folded into it, so
        // it still parts from what follows: `∑ᵢ₌₁ⁿ i`, not `∑ᵢ₌₁ⁿi`. Classing the
        // scripted piece `Ordinary` loses the space, and no other assertion here sees it.
        assert_eq!(inline(r"\sum_{i=1}^{n} i"), "∑ᵢ₌₁ⁿ i");
        assert_eq!(inline(r"\int_0^1 f"), "∫₀¹ f");
        // A function name likewise. The third carried stage-1 defect: stage 1 set
        // `sin²x`, because its trailing space was suppressed by the script rather than
        // decided by the pair. `gap(Function, Ordinary)` is 1 and says so once.
        assert_eq!(inline(r"\sin^2 x"), "sin² x");
        // And an ordinary base stays ordinary, so nothing is gained everywhere.
        assert_eq!(inline("x^2 y"), "x²y");
    }

    #[test]
    fn a_script_base_of_more_than_one_atom_is_parenthesised() {
        // `2{ab}^2` is `2·(ab)²`. Unbracketed it sets `2ab²`, which reads as `2·a·b²`.
        assert_eq!(inline(r"2{ab}^2"), "2(ab)²");
        assert_eq!(inline(r"2{ab}_2"), "2(ab)₂");
        // The operand is *not* bracketed, even though Unicode has `⁽` and `⁾`: a raised
        // group already delimits itself, and `x⁽ᵃ⁺ᵇ⁾` is not what was written.
        assert_eq!(inline("x^{a+b}"), "xᵃ⁺ᵇ");
    }

    #[test]
    fn a_construct_may_be_the_element_of_another_construct() {
        // Stage 1 declined both of these by name: its one-row walk took a single event or
        // a `{…}` group as an element, and a bare `Visual` or `Script` is neither. In one
        // engine an element is just a box, so the only question left is whether the
        // *substitution* succeeds.
        assert_eq!(inline(r"\sqrt{x}^2"), "(√x)²");
        assert_eq!(inline(r"\frac{a}{b}^2"), "(a/b)²");
        // The other way round: a construct as the element of a `Visual`.
        assert_eq!(inline(r"\sqrt\sqrt x"), "√(√x)");
        // The substitution is still the only question, and a raised slash is one of the
        // characters Unicode has not got -- so the operand is written flat, around the
        // rewrite the fraction already made of itself.
        assert_eq!(inline(r"x^\frac{a}{b}"), "x^{a/b}");
    }

    #[test]
    fn a_script_whose_operand_draws_nothing_writes_the_bare_marker() {
        // `\,` and `\bf` are elements that carry no atom. As a script operand each gives
        // an empty group, and an empty group declines (`scripts::substitute`) rather than
        // raising nothing at all -- so the fallback runs on empty text, and a
        // zero-character group never reaches the brace rule. Without an arm for these two
        // the element count slips and the walk reads the *next* event as the operand.
        assert_eq!(inline(r"x^\,"), "x^");
        assert_eq!(inline(r"x^\bf"), "x^");
        // The same answer for the empty group the author can write directly, which is
        // what says the two are one rule and not two.
        assert_eq!(inline("x^{}"), "x^");
        assert_eq!(inline("x_{}"), "x_");
    }

    #[test]
    fn a_chain_of_rewrites_is_capped_even_though_it_opens_no_group() {
        // `\sqrt\sqrt…x` recurses `element` -> `visual_box` -> `element` and never opens
        // a brace, so the cap in `build_run` alone does not see it: without the one in
        // `element` this aborts the process rather than refusing. The boundary is pinned
        // on both sides, one repeat apart.
        let chain = |n: usize| r"\sqrt".repeat(n) + " x";
        assert_eq!(
            refusal(&chain(MAX_NESTING + 1)),
            "a formula nested too deeply cannot be drawn on one row"
        );
        let storage = Storage::new();
        let at_the_cap = chain(MAX_NESTING);
        let events = parse(&at_the_cap, &storage).expect("parses");
        assert!(
            build(&events, Mode::Inline).is_ok(),
            "the cap itself still builds"
        );

        // No `nest(5000)` counterpart exists for this chain, and that is a fact about the
        // parser rather than an omission: `pulldown-latex` recurses on `\sqrt` and `parse`
        // aborts the process between 129 and 144 repeats, measured on this branch. The
        // brace chain reaches 5000 only because the parser handles braces iteratively.
        let near_the_parsers_own_cap = chain(129);
        assert!(
            parse(&near_the_parsers_own_cap, &storage).is_ok(),
            "129 still parses, which is the margin this cap has"
        );
    }

    #[test]
    fn display_mode_keeps_the_two_dimensional_form_of_all_four() {
        use crate::math::boxes::BoxContent;

        /// The one piece of a display formula that is a single construct.
        fn only_part(src: &str) -> BoxContent {
            let storage = Storage::new();
            let events = parse(src, &storage).expect("parses");
            let b = build(&events, Mode::Display).expect("builds");
            let BoxContent::Row(parts) = &b.content else {
                panic!("expected a row, got {:?}", b.content)
            };
            assert_eq!(parts.len(), 1, "one construct, one piece: {src}");
            parts[0].content.clone()
        }

        // Parts of different widths, so a numerator/denominator swap cannot hide behind
        // a width that is a max and two heights that are both 1.
        let BoxContent::Fraction { num, den } = only_part(r"\frac{a}{bb}") else {
            panic!("display mode must not rewrite a fraction to a slash")
        };
        assert_eq!(
            draw::to_row(&num).expect("flat"),
            "a",
            "the numerator is on top"
        );
        assert_eq!(draw::to_row(&den).expect("flat"), "bb");
        assert!(
            matches!(
                only_part(r"\sqrt{x}"),
                BoxContent::Radical { index: None, .. }
            ),
            "nor a square root to the radical sign"
        );
        // The indexed root, which has no inline form at all, and which is also where a
        // radicand/index swap hides: `pulldown-latex` gives the radicand first.
        let BoxContent::Radical { radicand, index } = only_part(r"\sqrt[3]{x}") else {
            panic!("expected a radical")
        };
        assert_eq!(draw::to_row(&radicand).expect("flat"), "x");
        let index = index.expect("an index was written");
        assert_eq!(draw::to_row(&index).expect("flat"), "3");

        // `Right` stays to the right; `Movable` stacks, because display mode is exactly
        // the condition the crate documents for it.
        let BoxContent::Scripts { sub, sup, .. } = only_part(r"\int_0^1") else {
            panic!("a Right script must not stack")
        };
        assert_eq!(
            draw::to_row(&sub.expect("a subscript")).expect("flat"),
            "0",
            "the lower limit stays below"
        );
        assert_eq!(
            draw::to_row(&sup.expect("a superscript")).expect("flat"),
            "1"
        );

        let BoxContent::Limits { under, over, .. } = only_part(r"\sum_{i=1}^{n}") else {
            panic!("a Movable script stacks in display mode")
        };
        assert_eq!(
            draw::to_row(&under.expect("an under")).expect("flat"),
            "i = 1",
            "a display operand keeps the spacing the table gives it"
        );
        assert_eq!(draw::to_row(&over.expect("an over")).expect("flat"), "n");

        // `AboveBelow` is the position that asked for stacking outright, and it is a
        // separate arm from the one `Movable` borrows: with only the `Movable` arm left,
        // `\sum\limits` sets its limits to the right in display mode too.
        let BoxContent::Limits { under, over, .. } = only_part(r"\sum\limits_{i}^{n}") else {
            panic!("an AboveBelow script stacks whatever else is true")
        };
        assert_eq!(draw::to_row(&under.expect("an under")).expect("flat"), "i");
        assert_eq!(draw::to_row(&over.expect("an over")).expect("flat"), "n");
    }

    #[test]
    fn a_script_that_asked_to_be_stacked_still_comes_back_on_one_row() {
        // One row has nowhere to stack, so `AboveBelow` is set to the right inline --
        // the same answer `Movable` gets, reached for the same reason. `Mode` is what
        // decides it, which is why this is the mode flag's third and last reader.
        assert_eq!(inline(r"\sum\limits_{i}^{n} x"), "∑ᵢⁿ x");
        assert_eq!(inline(r"\underset{a}{b}"), "bₐ");
    }
}
