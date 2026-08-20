// SPDX-License-Identifier: MIT
//! One formula, on one row.
//!
//! Design spec §5. Inline math must not give a paragraph line a variable height, which
//! is the constraint the whole renderer's reflow rests on — so this walks the event
//! stream and writes text, and any construct that genuinely needs a second row is
//! reported as [`MathError::NotInline`] rather than being approximated into nonsense.

use pulldown_latex::event::{Content, DelimiterType, Event, Grouping, ScriptType, Visual};
use pulldown_latex::{Parser, Storage};

use crate::error::MathError;
use crate::math::scripts;

/// Draws `src` as a single row of text.
///
/// # Errors
///
/// [`MathError::Parse`] if the LaTeX does not parse, [`MathError::NotInline`] if it
/// parses but cannot be written on one row.
pub fn render_inline(src: &str) -> Result<String, MathError> {
    let storage = Storage::new();
    let parser = Parser::new(src, &storage);
    let mut events = Vec::new();
    for event in parser {
        events.push(event.map_err(|err| {
            MathError::Parse {
                // Only the first line. `ParserError`'s `Display` is four lines: the message,
                // then a `╭─►` context box quoting the input. That box is unreadable in a
                // one-row caption and would put box drawing the manual does not claim into
                // `tests/glyph_inventory.rs`.
                message: err
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            }
        })?);
    }
    write_events(&events, Spacing::Normal)
}

/// The characters `src`'s own commands resolved to.
///
/// Design spec §13. `pulldown-latex` resolves `\alpha` to `α` and `\times` to `×`; those
/// are the *document's* characters, asked for by name. What [`render_inline`] puts around
/// them is mdmost's — §5.1's script forms, §5.2's radical sign, slash and brackets — so
/// `tests/glyph_inventory.rs` subtracts this and keeps the rest.
///
/// # Errors
///
/// [`MathError::Parse`] if the LaTeX does not parse.
pub fn symbols(src: &str) -> Result<String, MathError> {
    let storage = Storage::new();
    let mut out = String::new();
    for event in Parser::new(src, &storage) {
        let event = event.map_err(|err| MathError::Parse {
            message: err
                .to_string()
                .lines()
                .next()
                .unwrap_or_default()
                .to_string(),
        })?;
        if let Event::Content(content) = event {
            write_content(&content, &mut out, Spacing::Suppressed, false);
        }
    }
    Ok(out)
}

/// Whether the walk writes the spaces of the spacing rule above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spacing {
    /// Spaces are written: `1 + 2`, `E = mc²`, `∑ᵢ₌₁ⁿ i`.
    Normal,
    /// Spaces are not written, because the caller is about to raise or lower this text
    /// and the Unicode script tables of §5.1 have no space — one there would make the
    /// all-or-nothing rule decline the whole group, turning `xⁿ⁺¹` into `x^{n + 1}`.
    ///
    /// Only script operands suppress. A fraction or radical operand is written at full
    /// size and keeps its spaces.
    Suppressed,
}

/// Writes an already-collected event stream.
///
/// Split from [`render_inline`] so that later tasks can test the walk without going
/// through the parser, and so the error path above stays one statement. A thin wrapper
/// over [`write_into`], which does the actual walking into a caller-supplied buffer;
/// this just gives that walk its own buffer and trims the trailing space a formula
/// ending in an operator or a function name would otherwise keep.
fn write_events(events: &[Event<'_>], spacing: Spacing) -> Result<String, MathError> {
    let mut out = String::new();
    write_into(events, &mut out, spacing)?;
    Ok(out.trim_end().to_string())
}

/// Writes an already-collected event stream into `out`, continuing whatever is already
/// there rather than starting fresh.
///
/// This is what lets a group's *first* token see real context: `take_base`'s group
/// branch calls this directly on the caller's accumulated `out`, so a function name
/// that opens a `{…}` base still gets the leading space its position (not the group's)
/// earns it -- see `take_base`'s doc comment.
///
/// `pulldown-latex` emits `Event::Script { ty, .. }` followed by the base and then the
/// script operand(s) as complete sub-sequences (confirmed against the parser's own unit
/// tests: `subsuperscript` in `pulldown_latex::parser::tests` gives `a^{1+3}_2` as
/// `Script`, base `a`, subscript `2`, superscript `{1+3}` — base first, then subscript,
/// then superscript, regardless of the order the source wrote them). So this walk takes
/// groups rather than single events.
fn write_into(events: &[Event<'_>], out: &mut String, spacing: Spacing) -> Result<(), MathError> {
    let mut index = 0usize;
    while index < events.len() {
        match &events[index] {
            Event::Script { ty, .. } => {
                let ty = *ty;
                index += 1;
                // A big operator base (`\sum`, `\int`, …) takes one space after it and
                // after its limits, so `\sum_{i=1}^{n} i` reads `∑ᵢ₌₁ⁿ i` and not
                // `∑ᵢ₌₁ⁿi`. `pulldown-latex` discards the literal space in the source,
                // so this walk is the only thing that can put one back.
                let big = is_large_op(events, index);
                // The base is written under the caller's spacing; the scripts are not,
                // because a space in a script group makes §5.1's all-or-nothing rule
                // decline it. `take_base` writes it straight into `out` (not an
                // isolated buffer) so its own leading space sees the true accumulated
                // context (`2\sin^2 x` needs the space before `sin` that separates it
                // from `2`, same as plain `2\sin x` does) and always trims the
                // trailing one, because a script sits flush against its base
                // regardless of context: `\sin^2 x` reads `sin²x`, not `sin ²x`.
                take_base(events, &mut index, out, spacing)?;
                match ty {
                    ScriptType::Subscript => {
                        let sub = take_group(events, &mut index, Spacing::Suppressed)?;
                        out.push_str(&lowered(&sub));
                    }
                    ScriptType::Superscript => {
                        let sup = take_group(events, &mut index, Spacing::Suppressed)?;
                        out.push_str(&raised(&sup));
                    }
                    ScriptType::SubSuperscript => {
                        let sub = take_group(events, &mut index, Spacing::Suppressed)?;
                        let sup = take_group(events, &mut index, Spacing::Suppressed)?;
                        out.push_str(&lowered(&sub));
                        out.push_str(&raised(&sup));
                    }
                }
                if big {
                    after_large_op(out, spacing);
                }
            }
            Event::Visual(visual) => {
                index += 1;
                match visual {
                    Visual::Fraction(_) => {
                        let numerator = take_group(events, &mut index, spacing)?;
                        let denominator = take_group(events, &mut index, spacing)?;
                        out.push_str(&bracketed(&numerator));
                        out.push('/');
                        out.push_str(&bracketed(&denominator));
                    }
                    Visual::SquareRoot => {
                        let radicand = take_group(events, &mut index, spacing)?;
                        out.push('√');
                        out.push_str(&bracketed(&radicand));
                    }
                    // Radicand first, then the degree. `pulldown-latex`'s own
                    // documentation for this variant reads "the radicand and the index
                    // of the root", and it maps to MathML `mroot`, whose child order is
                    // base then index. Taking them the other way round renders
                    // `\sqrt[3]{x}` as `ˣ√3`.
                    Visual::Root => {
                        let radicand = take_group(events, &mut index, spacing)?;
                        let degree = take_group(events, &mut index, Spacing::Suppressed)?;
                        out.push_str(&raised(&degree));
                        out.push('√');
                        out.push_str(&bracketed(&radicand));
                    }
                    // A negation slashes the symbol after it, which needs a second
                    // layer of cells that one row does not have.
                    _ => return Err(MathError::NotInline("this construct")),
                }
            }
            _ => {
                let big = is_large_op(events, index);
                write_one(events, &mut index, out, spacing)?;
                if big {
                    after_large_op(out, spacing);
                }
            }
        }
    }
    Ok(())
}

/// Index of the `Event::End` matching the `Event::Begin` at `events[open]`.
fn group_end(events: &[Event<'_>], open: usize) -> Result<usize, MathError> {
    let mut depth = 1usize;
    let mut cursor = open + 1;
    while cursor < events.len() && depth > 0 {
        match events[cursor] {
            Event::Begin(_) => depth += 1,
            Event::End => depth -= 1,
            _ => {}
        }
        cursor += 1;
    }
    if depth > 0 {
        return Err(MathError::NotInline("an unclosed group"));
    }
    Ok(cursor)
}

/// Writes a script's base directly into `out`.
///
/// Unlike `take_group`, this does not build the base in an isolated buffer first. An
/// isolated buffer always starts empty, so the base's own head-of-run check
/// (`spaced_word`'s `out.is_empty()`) would wrongly conclude it opens the formula even
/// when it does not -- `2\sin^2 x` needs the leading space `2\sin x` gets, and an
/// isolated buffer cannot see the `2` already written. Writing straight into the real
/// `out` gives that check the true context. This applies just as much to a `{…}` base
/// as to a bare one: `2{\sin x}^2` needs that same leading space before `sin`, so its
/// first token is written via `write_into` on `out` itself rather than recursed into an
/// isolated buffer. Either way, the trailing space a function name would otherwise earn
/// is then trimmed unconditionally, because a script always sits flush against its base
/// no matter what precedes it.
///
/// A group is a typographic bracket for spacing, as above, but *not* for grouping: a
/// script applies to the whole base, not to whatever atom happened to be written last,
/// so `2{ab}^2` must read `2(ab)²`, not `2ab²`. The `{…}` branch below restores that
/// grouping after the fact, by bracketing what it just appended -- it cannot bracket
/// first and write second, because the bracket characters themselves must not be part
/// of what the head-of-run check or the trailing-space trim see.
fn take_base(
    events: &[Event<'_>],
    index: &mut usize,
    out: &mut String,
    spacing: Spacing,
) -> Result<(), MathError> {
    let Some(first) = events.get(*index) else {
        return Err(MathError::NotInline("an unfinished script"));
    };
    let start = out.len();
    // `\left(…\right)` as a base still needs its own delimiters -- `write_one`'s
    // `LeftRight` arm is the one place that knows how to draw them, so this delegates
    // rather than repeating that logic here and risking it drift out of sync.
    if matches!(first, Event::Begin(Grouping::LeftRight(..))) {
        write_one(events, index, out, spacing)?;
    } else if matches!(first, Event::Begin(_)) {
        let group_start = *index + 1;
        let cursor = group_end(events, *index)?;
        write_into(&events[group_start..cursor - 1], out, spacing)?;
        *index = cursor;
        // A script applies to the whole base group, not just its last atom -- a
        // group is a typographic bracket for grouping as much as it is for spacing
        // (see this function's doc comment on the latter), so `2{ab}^2` must read
        // `2(ab)²`, the same visual grouping `bracketed()` already gives a fraction
        // or radical operand. `write_into` wrote straight into `out`, so any leading
        // space it added for head-of-run spacing sits before `start`'s tail and must
        // stay outside the parentheses. The trailing space a function name or an
        // operator can leave (`\sum` writes `∑ `, ready for what follows it) must be
        // trimmed *before* bracketing, not after: sealed inside the parentheses it
        // would make a one-character base count as two, so `bracketed()`'s
        // single-atom exemption would never fire and `{\sum}^2` would draw `(∑ )²`
        // instead of `∑²`. The unconditional trim below only ever sees `out`'s new
        // tail from *outside* the parentheses this pushes, so it cannot reach a space
        // sealed in here.
        let leading_space = out[start..].starts_with(' ');
        let body_start = start + usize::from(leading_space);
        let body = out.split_off(body_start);
        out.push_str(&bracketed(body.trim_end()));
    } else if let Event::Visual(visual) = first {
        // `write_one` has no arm for `Event::Visual` at all -- a fraction or a radical
        // is drawn only by `write_into`'s own match, never by `write_one`'s -- so
        // delegating here would reach its catch-all and report "this construct cannot
        // be drawn on one row", which is false of a fraction or a radical: both draw
        // fine elsewhere on this very row (design spec §5.2, §6.1). What actually
        // cannot be drawn is one of them *in this position*, a two-dimensional box
        // used as flat script content -- Task 5's carried deferral, real to stage 2's
        // proper layout rather than to whether the walk can produce one row at all.
        return Err(MathError::NotInline(visual_as_base_name(visual)));
    } else {
        write_one(events, index, out, spacing)?;
    }
    // Trim only what this call appended -- never reach back before `start`.
    while out.len() > start && out.ends_with(' ') {
        out.pop();
    }
    Ok(())
}

/// Renders the next operand — a single event, or a balanced `Begin`..`End` group.
///
/// Always builds the result in its own isolated buffer, unlike `take_base`: this is
/// used only for a script's own sub/superscript operand, which is about to be raised or
/// lowered as self-contained text (§5.1), not spliced into the surrounding run, so it
/// must not see -- or leak -- the caller's spacing context.
fn take_group(
    events: &[Event<'_>],
    index: &mut usize,
    spacing: Spacing,
) -> Result<String, MathError> {
    let Some(first) = events.get(*index) else {
        return Err(MathError::NotInline("an unfinished script"));
    };
    if !matches!(first, Event::Begin(_)) {
        let mut out = String::new();
        write_one(events, index, &mut out, spacing)?;
        return Ok(out);
    }
    let start = *index + 1;
    let cursor = group_end(events, *index)?;
    let inner = write_events(&events[start..cursor - 1], spacing)?;
    *index = cursor;
    Ok(inner)
}

/// One event that is not a script. Advances `*index` past whatever it consumed.
fn write_one(
    events: &[Event<'_>],
    index: &mut usize,
    out: &mut String,
    spacing: Spacing,
) -> Result<(), MathError> {
    let Some(event) = events.get(*index) else {
        return Err(MathError::NotInline("an unfinished script"));
    };
    // `\left…\right` sizes to its content and, on one row, degrades to the plain
    // delimiter character rather than the tall box design spec §6.4 draws when there is
    // room for one: `(x)`, never `╭x╮`. Those characters live in `Grouping::LeftRight`'s
    // own two fields, not as separate `Content` events either side (verified against
    // `event.rs:316` in the vendored crate source) — a comment here once claimed
    // otherwise, which is why the delimiters used to vanish silently. So this is the one
    // grouping that must write something at its own boundary rather than only at what is
    // inside it, which means it cannot share the single-event, index-plus-one shape the
    // rest of this match uses: it consumes the whole group itself and returns early.
    if let Event::Begin(Grouping::LeftRight(opening, closing)) = event {
        let opening = *opening;
        let closing = *closing;
        let group_start = *index + 1;
        let cursor = group_end(events, *index)?;
        // `None` is a real `\left.` or `\right.` — an intentionally invisible
        // delimiter, not a missing one — so nothing is written for it, not a
        // placeholder.
        if let Some(delim) = opening {
            out.push(delim);
        }
        write_into(&events[group_start..cursor - 1], out, spacing)?;
        if let Some(delim) = closing {
            out.push(delim);
        }
        *index = cursor;
        return Ok(());
    }
    match event {
        Event::Content(content) => {
            // A function name reads tight against an opening delimiter that
            // immediately follows it — `\sin(x)` is `sin(x)`, not `sin (x)`, which
            // is how real LaTeX sets it too: the gap after an operator name comes
            // from separating it from its operand, and a delimited group is
            // already visibly its own thing without one. Only `write_content`'s
            // `Function` arm reads this flag; every other arm ignores it.
            let followed_by_open_delimiter = matches!(
                events.get(*index + 1),
                Some(Event::Content(Content::Delimiter {
                    ty: DelimiterType::Open,
                    ..
                }))
            );
            write_content(content, out, spacing, followed_by_open_delimiter);
        }
        // A group boundary draws nothing of itself. What a group *does* — a
        // fraction, a root, a matrix — arrives as `Visual` or as an environment.
        // `Normal` is `{}`, drawn by its content alone. `LeftRight` is handled above,
        // before this match, because it must draw its own delimiters. Everything
        // else — a matrix, `cases`, an `array`, an `align` — is a grid, and a grid is
        // exactly the failure spec §9 describes: a well-formed formula in the wrong
        // place.
        Event::Begin(Grouping::Normal) | Event::End => {}
        Event::Begin(grouping) => return Err(MathError::NotInline(grouping_name(grouping))),
        Event::StateChange(_) => {}
        // Only a *horizontal* spacing command draws a column. `\mathstrut` and
        // `\strut` are `Space { width: None, height: Some(…) }` — vertical struts
        // that occupy no columns at all.
        Event::Space { width, .. } if width.is_some() => out.push(' '),
        Event::Space { .. } => {}
        _ => return Err(MathError::NotInline("this construct")),
    }
    *index += 1;
    Ok(())
}

/// Whether the operand at `index` is a big operator (`\sum`, `\int`, `\prod`, …).
fn is_large_op(events: &[Event<'_>], index: usize) -> bool {
    matches!(
        events.get(index),
        Some(Event::Content(Content::LargeOp { .. }))
    )
}

/// One space after a big operator, unless spacing is suppressed or there is one already.
fn after_large_op(out: &mut String, spacing: Spacing) {
    if spacing == Spacing::Normal && !out.is_empty() && !out.ends_with(' ') {
        out.push(' ');
    }
}

/// `text` raised, or written with a caret when Unicode cannot raise all of it (§5.1).
fn raised(text: &str) -> String {
    scripts::superscript(text).unwrap_or_else(|| flat('^', text))
}

/// `text` lowered, or written with an underscore when Unicode cannot lower all of it.
fn lowered(text: &str) -> String {
    scripts::subscript(text).unwrap_or_else(|| flat('_', text))
}

/// The flat notation: `x^q`, and `a_{bc}` when the group is more than one character.
///
/// The braces are not decoration. `a_bc` reads as `(a_b)c`, so a group that lost its
/// raising must keep the grouping the author wrote.
fn flat(marker: char, text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 3);
    out.push(marker);
    if text.chars().count() > 1 {
        out.push('{');
        out.push_str(text);
        out.push('}');
    } else {
        out.push_str(text);
    }
    out
}

/// Writes one piece of content.
///
/// Symbols arrive from `pulldown-latex` already resolved to a `char`. There is
/// deliberately no symbol table in this crate: a second table would drift from the one
/// upstream maintains, and drift here means a formula that renders differently here
/// than everywhere else.
fn write_content(
    content: &Content<'_>,
    out: &mut String,
    spacing: Spacing,
    followed_by_open_delimiter: bool,
) {
    match content {
        Content::Text(text) | Content::Number(text) => {
            out.push_str(text);
        }
        // A function name is a word, not an operator: it is never unary the way a
        // leading `-` can be, so unlike `spaced` below it does not suppress *both*
        // sides at the head of a run, only the leading one -- `\sin x` must read
        // `sin x` even though `\sin` opens the formula.
        Content::Function(text) => {
            spaced_word(out, text, spacing, followed_by_open_delimiter);
        }
        Content::BinaryOp { content, .. } => {
            spaced(out, content.encode_utf8(&mut [0u8; 4]), spacing);
        }
        // A relation is the one content that is not a `char`: `RelationContent` may hold
        // two characters, and its only public accessor writes them into a caller's
        // buffer, which the upstream doc comment requires be at least eight bytes. It
        // therefore cannot share an or-pattern with the arm below, whose alternatives all
        // bind a `char`.
        //
        // The two-character relations are the sixteen `multirelation` calls at
        // `pulldown-latex-0.8.0/src/parser/primitives.rs:1157-1172` -- `\coloneq` is `:`
        // then `−`, and six of them are a base character plus U+FE00. This comment named
        // `\shortparallel`, which is not one of them: it is
        // `RelationContent::single_char('∥')` at `primitives.rs:1066`.
        Content::Relation { content, .. } => {
            let mut buf = [0u8; 8];
            let text =
                std::str::from_utf8(content.encode_utf8_to_buf(&mut buf)).unwrap_or_default();
            spaced(out, text, spacing);
        }
        Content::Ordinary { content, .. }
        | Content::LargeOp { content, .. }
        | Content::Delimiter { content, .. } => out.push(*content),
        Content::Punctuation(ch) => out.push(*ch),
    }
}

/// Writes `text` with one space either side, unless spacing is suppressed.
///
/// Never a leading space at the head of a run and never two in a row, so a unary `−x`
/// reads `−x` while `a − b` reads `a − b`. `pulldown-latex` does not mark a leading `-`
/// as unary the way TeX's own bin/ord reclassification would — it is `Content::BinaryOp`
/// exactly like the one in `a-b` — so the head-of-run position (`out` still empty) is
/// the only signal this walk has, and it is sufficient: nothing upstream of the first
/// token can make this use binary. When that is where we are, the operator is glued to
/// what follows with no space on either side, the same as `Spacing::Suppressed`.
fn spaced(out: &mut String, text: &str, spacing: Spacing) {
    if spacing == Spacing::Suppressed || out.is_empty() {
        out.push_str(text);
        return;
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out.push_str(text);
    out.push(' ');
}

/// What a grouping is called, for [`MathError::NotInline`]'s payload.
///
/// The payload is what the caption shows the reader, so it names the construct rather
/// than the event. `Normal` and `LeftRight` never reach here.
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

/// What a [`Visual`] is called when it is used as a script base, for
/// [`MathError::NotInline`]'s payload.
///
/// Named for the *position*, not the construct: a fraction or a radical draws fine at
/// full size elsewhere on the row (`write_into`'s own `Visual` arms), so the generic
/// "this construct cannot be drawn on one row" `write_one` falls back to is false of the
/// thing itself. `Negation` never reaches here as things stand -- `take_base` is the only
/// caller, and nothing routes a negation into a script base without failing earlier --
/// so it keeps the same generic wording `write_into`'s own catch-all uses for it.
fn visual_as_base_name(visual: &Visual) -> &'static str {
    match visual {
        Visual::Fraction(_) => "a fraction as a script base",
        Visual::SquareRoot | Visual::Root => "a radical as a script base",
        _ => "this construct",
    }
}

/// `text` in parentheses unless it is a single atom that cannot be misread.
///
/// `a/b` needs none; `a + b/c` is a different expression from `(a + b)/c`, and this is
/// the only thing standing between the two. One grapheme is the only case that is
/// certainly safe, so it is the only case exempted: a test for "already parenthesised"
/// would be wrong for `\frac{(a)+(b)}{c}`, where the outer parentheses are two groups
/// and not one, which is the exact misreading this function exists to prevent. The cost
/// is `((a))` for an author who bracketed a whole operand themselves, which reads
/// correctly and is merely redundant.
fn bracketed(text: &str) -> String {
    if text.chars().count() == 1 {
        text.to_string()
    } else {
        format!("({text})")
    }
}

/// Writes a function name (`sin`, `log`, …) with a leading space unless it opens the
/// run, and a trailing space unless spacing is suppressed or `suppress_trailing` says
/// the next thing is an opening delimiter it should sit against.
///
/// Deliberately not `spaced`: a word can never be unary, so — unlike an operator at the
/// head of a run — its own trailing space is never dropped just for being first. Only
/// its leading space follows the "never at the head of a run" rule.
fn spaced_word(out: &mut String, text: &str, spacing: Spacing, suppress_trailing: bool) {
    if spacing == Spacing::Suppressed {
        out.push_str(text);
        return;
    }
    if !out.is_empty() && !out.ends_with(' ') {
        out.push(' ');
    }
    out.push_str(text);
    if !suppress_trailing {
        out.push(' ');
    }
}
