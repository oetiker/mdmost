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

/// Decodes the entity escapes in `text`, once.
///
/// Scanning is strictly left to right and the output is never re-read, so `&amp;lt;` is
/// the literal text `&lt;` rather than `<`: an author who escaped their escape gets
/// what they asked for. Unrecognised escapes are copied through untouched.
pub fn decode(text: &str) -> Cow<'_, str> {
    if !text.contains(['&', '#']) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['&', '#']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        match entity_at(tail) {
            Some((ch, len)) => {
                out.push(ch);
                rest = &tail[len..];
            }
            None => {
                // Not an escape: keep the sigil and resume after it.
                out.push_str(&tail[..1]);
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
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
