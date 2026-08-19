// SPDX-License-Identifier: MIT
//! One formula, on one row.
//!
//! Design spec §5. Inline math must not give a paragraph line a variable height, which
//! is the constraint the whole renderer's reflow rests on — so this walks the event
//! stream and writes text, and any construct that genuinely needs a second row is
//! reported as [`MathError::NotInline`] rather than being approximated into nonsense.

use pulldown_latex::event::{Content, DelimiterType, Event, ScriptType};
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
/// through the parser, and so the error path above stays one statement.
///
/// `pulldown-latex` emits `Event::Script { ty, .. }` followed by the base and then the
/// script operand(s) as complete sub-sequences (confirmed against the parser's own unit
/// tests: `subsuperscript` in `pulldown_latex::parser::tests` gives `a^{1+3}_2` as
/// `Script`, base `a`, subscript `2`, superscript `{1+3}` — base first, then subscript,
/// then superscript, regardless of the order the source wrote them). So this walk takes
/// groups rather than single events.
fn write_events(events: &[Event<'_>], spacing: Spacing) -> Result<String, MathError> {
    let mut out = String::new();
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
                // decline it. Written into the real `out`, not an isolated buffer --
                // `take_base` needs the true accumulated context to get the base's own
                // *leading* space right (`2\sin^2 x` needs the space before `sin` that
                // separates it from `2`, same as plain `2\sin x` does) -- and always
                // trims the *trailing* one, because a script sits flush against its
                // base regardless of context: `\sin^2 x` reads `sin²x`, not `sin ²x`.
                take_base(events, &mut index, &mut out, spacing)?;
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
                    after_large_op(&mut out, spacing);
                }
            }
            _ => {
                let big = is_large_op(events, index);
                write_one(events, &mut index, &mut out, spacing)?;
                if big {
                    after_large_op(&mut out, spacing);
                }
            }
        }
    }
    // A formula ending in an operator or a function name would otherwise keep the
    // space written after it.
    Ok(out.trim_end().to_string())
}

/// Writes a script's base directly into `out`.
///
/// Unlike `take_group`, this does not build the base in an isolated buffer first. An
/// isolated buffer always starts empty, so a single-token base's own head-of-run check
/// (`spaced_word`'s `out.is_empty()`) would wrongly conclude it opens the formula even
/// when it does not -- `2\sin^2 x` needs the leading space `2\sin x` gets, and an
/// isolated buffer cannot see the `2` already written. Writing straight into the real
/// `out` gives that check the true context. The trailing space a function name would
/// otherwise earn is then trimmed unconditionally, because a script always sits flush
/// against its base no matter what precedes it.
fn take_base(
    events: &[Event<'_>],
    index: &mut usize,
    out: &mut String,
    spacing: Spacing,
) -> Result<(), MathError> {
    let Some(first) = events.get(*index) else {
        return Err(MathError::NotInline("an unfinished script"));
    };
    if matches!(first, Event::Begin(_)) {
        // A group is its own bracketed context (`{\sin x}^2`) -- consistent with every
        // other group in this walk, its first token is head-of-run *within the group*,
        // which `take_group` already gets right via its own fresh buffer.
        let group = take_group(events, index, spacing)?;
        out.push_str(group.trim_end());
        return Ok(());
    }
    let start = out.len();
    write_one(events, index, out, spacing)?;
    // Trim only what this call appended -- never reach back before `start`.
    while out.len() > start && out.ends_with(' ') {
        out.pop();
    }
    Ok(())
}

/// Renders the next operand — a single event, or a balanced `Begin`..`End` group.
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
    let mut depth = 1usize;
    let mut cursor = start;
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
        // fraction, a root, a matrix — arrives as `Visual` or as an environment,
        // and those are Task 5's.
        Event::Begin(_) | Event::End | Event::StateChange(_) => {}
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
        // two characters (`\shortparallel` and friends), and its only public accessor
        // writes them into a caller's buffer, which the upstream doc comment requires be
        // at least eight bytes. It therefore cannot share an or-pattern with the arm
        // below, whose alternatives all bind a `char`.
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
