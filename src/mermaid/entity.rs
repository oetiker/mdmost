// SPDX-License-Identifier: MIT
//! Character entity escapes in Mermaid label text.
//!
//! Mermaid renders through HTML, so a diagram author who wants a `<` in a label has to
//! write `&lt;` — mermaid.js has no other spelling for it. Mermaid additionally
//! documents its own escapes with a `#` sigil in place of the `&`, such as `#quot;` and
//! `#35;`; internally mermaid rewrites those into the `&…;` form and lets the browser
//! decode them, so the two families name exactly the same set of characters.
//!
//! We receive the fence body exactly as the Markdown parser found it, because comrak
//! deliberately leaves entities alone inside code blocks — right for code, wrong for a
//! diagram. So the Mermaid parser decodes label text itself, at the point where it
//! becomes display text.
//!
//! Turning `&lt;` into `<` is not HTML rendering (design spec §2 forbids that): no tag
//! is interpreted, nothing is styled, and the result is one plain character. The
//! renderer still never sees markup — `<br>` is split off *before* decoding, so
//! `&lt;br&gt;` stays the four visible characters `<br>` on one line.

use std::borrow::Cow;

/// The named entities Mermaid's own documentation uses in labels.
///
/// Deliberately short: this is not an HTML entity table. Anything not listed is left
/// exactly as written, so an unknown `&nosuch;` survives to the screen.
const NAMED: &[(&str, char)] = &[
    ("lt", '<'),
    ("gt", '>'),
    ("amp", '&'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", '\u{a0}'),
];

/// The longest body we will look at between a sigil and its `;`, in bytes.
///
/// Long enough for `#1114111`, short enough that a lone `&` in prose costs a glance.
const MAX_BODY: usize = 9;

/// One run of decoded text, and the bytes of the raw text that produced it.
///
/// A [`faithful`](Run::faithful) run is a byte-for-byte copy of its source, which is what
/// lets a selection resolve a column *inside* it back to a byte (design spec §2.1). An
/// unfaithful run is one decoded entity: `&amp;` draws `&`, and nothing in those five
/// bytes copies the one they drew, so the whole reference is what produced that
/// character and there is no honest sub-range inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// Byte range in the decoded string.
    pub text: std::ops::Range<usize>,
    /// Byte range in the raw text handed to [`decode_runs`].
    pub source: std::ops::Range<usize>,
    /// Whether the decoded bytes are a copy of the source bytes.
    pub faithful: bool,
}

/// Decodes the entity escapes in `text`, once.
///
/// Scanning is strictly left to right and the output is never re-read, so `&amp;lt;` is
/// the literal text `&lt;` rather than `<`: an author who escaped their escape gets
/// what they asked for. Unrecognised escapes are copied through untouched.
pub fn decode(text: &str) -> Cow<'_, str> {
    decode_runs(text).0
}

/// Decodes `text` and reports where each run of the result came from.
///
/// [`decode`]'s own implementation, so the two cannot drift: a run map that disagreed
/// with the text it maps would hand a selection bytes from the wrong place. The runs
/// tile the decoded text end to end and in order, so concatenating them reproduces it.
pub fn decode_runs(text: &str) -> (Cow<'_, str>, Vec<Run>) {
    if !text.contains(['&', '#']) {
        return (
            Cow::Borrowed(text),
            vec![Run {
                text: 0..text.len(),
                source: 0..text.len(),
                faithful: true,
            }],
        );
    }
    let mut out = String::with_capacity(text.len());
    let mut runs: Vec<Run> = Vec::new();
    let mut rest = text;
    // Where `rest` starts in `text`, and where the faithful run in progress started in
    // each of the two, so that a copied stretch is emitted as one run rather than one
    // per gap between entities.
    let mut at = 0usize;
    let (mut run_source, mut run_text) = (0usize, 0usize);
    while let Some(gap) = rest.find(['&', '#']) {
        out.push_str(&rest[..gap]);
        let tail = &rest[gap..];
        let sigil = at + gap;
        match entity_at(tail) {
            Some((ch, len)) => {
                if sigil > run_source {
                    runs.push(Run {
                        text: run_text..out.len(),
                        source: run_source..sigil,
                        faithful: true,
                    });
                }
                let drawn = out.len();
                out.push(ch);
                runs.push(Run {
                    text: drawn..out.len(),
                    source: sigil..sigil + len,
                    faithful: false,
                });
                rest = &tail[len..];
                at = sigil + len;
                run_source = at;
                run_text = out.len();
            }
            None => {
                // Not an escape: keep the sigil and resume after it. It is a copy of
                // itself, so the run in progress simply continues over it.
                out.push_str(&tail[..1]);
                rest = &tail[1..];
                at = sigil + 1;
            }
        }
    }
    out.push_str(rest);
    if text.len() > run_source {
        runs.push(Run {
            text: run_text..out.len(),
            source: run_source..text.len(),
            faithful: true,
        });
    }
    debug_assert!(
        runs.iter()
            .all(|run| !run.faithful || out[run.text.clone()] == text[run.source.clone()]),
        "a faithful run must copy its source"
    );
    (Cow::Owned(out), runs)
}

/// The byte length of the character reference `text` opens with, sigil and `;` included.
///
/// Exactly what [`decode`] will consume — this is [`entity_at`] with the character it
/// names thrown away, not a second matcher that merely agrees with it. A caller that has
/// to step *over* a reference rather than decode it is
/// [`lex::split_statements`](crate::mermaid::parse::lex::split_statements), whose
/// statement separator is the very `;` that terminates one; sharing the decision means
/// the splitter and the decoder cannot disagree about where a reference ends, or about
/// whether there is one at all. An `&nosuch;` the decoder passes through untouched is
/// therefore not a reference here either, and its `;` stays a separator.
pub fn reference_len(text: &str) -> Option<usize> {
    if !matches!(text.as_bytes().first(), Some(b'&' | b'#')) {
        return None;
    }
    entity_at(text).map(|(_, len)| len)
}

/// Reads the entity starting at the sigil `text` begins with.
///
/// Returns the character it names and the byte length of the whole escape.
fn entity_at(text: &str) -> Option<(char, usize)> {
    let sigil = text.as_bytes().first().copied()?;
    debug_assert!(sigil == b'&' || sigil == b'#');
    // Byte-wise, so that a body of multi-byte characters cannot land the search limit
    // inside one; `;` is ASCII and so never appears inside a multi-byte sequence.
    let end = 1 + text.as_bytes()[1..]
        .iter()
        .take(MAX_BODY + 1)
        .position(|byte| *byte == b';')?;
    let body = &text[1..end];
    // With a `&`, only a further `#` makes the body numeric — a bare `&35;` names
    // nothing. Mermaid's `#` sigil carries no second marker, so `#quot;` and `#35;`
    // are both valid and the name is tried first.
    let ch = if sigil == b'&' {
        match body.strip_prefix('#') {
            Some(number) => numeric(number)?,
            None => named(body)?,
        }
    } else {
        named(body).or_else(|| numeric(body))?
    };
    Some((ch, end + 1))
}

/// The character a named entity body such as `lt` stands for.
fn named(body: &str) -> Option<char> {
    NAMED
        .iter()
        .find(|(name, _)| *name == body)
        .map(|&(_, ch)| ch)
}

/// The character a numeric entity body such as `35` or `x3C` stands for.
fn numeric(body: &str) -> Option<char> {
    let code = match body.strip_prefix(['x', 'X']) {
        Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
            u32::from_str_radix(hex, 16).ok()?
        }
        Some(_) => return None,
        None if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) => {
            body.parse().ok()?
        }
        None => return None,
    };
    char::from_u32(code).filter(|ch| *ch != '\0')
}
