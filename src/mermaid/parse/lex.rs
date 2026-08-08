//! Shared lexical helpers for the seven Mermaid family parsers.
//!
//! Everything in this module is family-agnostic string surgery: comment stripping,
//! quote- and bracket-aware splitting, unquoting, identifier scanning and error
//! construction. A family parser that reimplements any of this is a defect
//! (design spec §14).
//!
//! Line numbers are attached in the very first pass ([`preprocess`]) and carried on
//! every [`SrcLine`], because every [`MermaidError`] variant reports one.

use crate::error::MermaidError;

/// One significant source line together with its 1-based line number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrcLine<'a> {
    /// The 1-based line number in the original Mermaid source.
    pub number: usize,
    /// The line's text, trimmed, with comments removed. Never empty.
    pub text: &'a str,
}

/// Strips comments, `%%{init}%%` directive blocks and blank lines.
///
/// Directives may span several lines, so a line opening with `%%{` swallows following
/// lines until one closes the block with `}%%`.
pub fn preprocess(src: &str) -> Vec<SrcLine<'_>> {
    let mut out = Vec::new();
    let mut in_directive = false;
    for (index, raw) in src.lines().enumerate() {
        let number = index + 1;
        let trimmed = raw.trim();
        if in_directive {
            if trimmed.contains("}%%") {
                in_directive = false;
            }
            continue;
        }
        if trimmed.starts_with("%%{") {
            // A single-line directive closes on the same line.
            if !trimmed.contains("}%%") {
                in_directive = true;
            }
            continue;
        }
        let text = strip_comment(trimmed).trim();
        if text.is_empty() {
            continue;
        }
        out.push(SrcLine { number, text });
    }
    out
}

/// Removes a trailing `%%` comment that is not inside a quoted string.
fn strip_comment(text: &str) -> &str {
    match find_top_level(text, "%%", Nesting::Ignore) {
        Some(at) => &text[..at],
        None => text,
    }
}

/// Whether a scan should honour bracket nesting in addition to quotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nesting {
    /// Only skip quoted regions.
    Ignore,
    /// Skip quoted regions and anything inside `[]`, `()` or `{}`.
    Honour,
}

/// Tracks quoting and bracket depth while scanning a line left to right.
#[derive(Debug, Default, Clone, Copy)]
pub struct Scanner {
    quoted: bool,
    depth: i32,
}

impl Scanner {
    /// Feeds one character, returning `true` when that character sits at the top
    /// level (outside quotes and, if requested, outside brackets).
    pub fn step(&mut self, ch: char, nesting: Nesting) -> bool {
        if self.quoted {
            if ch == '"' {
                self.quoted = false;
            }
            return false;
        }
        let top = self.depth == 0;
        match ch {
            '"' => {
                self.quoted = true;
                return false;
            }
            '[' | '(' | '{' if nesting == Nesting::Honour => self.depth += 1,
            ']' | ')' | '}' if nesting == Nesting::Honour => {
                self.depth = (self.depth - 1).max(0);
                return false;
            }
            _ => {}
        }
        top
    }
}

/// Finds `needle` at the top level of `text`, returning its byte offset.
pub fn find_top_level(text: &str, needle: &str, nesting: Nesting) -> Option<usize> {
    let mut scanner = Scanner::default();
    for (at, ch) in text.char_indices() {
        let top = scanner.step(ch, nesting);
        if top && text[at..].starts_with(needle) {
            return Some(at);
        }
    }
    None
}

/// Splits `text` on every top-level occurrence of `sep`, trimming each part.
///
/// Empty parts are dropped, so `A; B;` yields `["A", "B"]`.
pub fn split_top_level(text: &str, sep: char, nesting: Nesting) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut scanner = Scanner::default();
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        let top = scanner.step(ch, nesting);
        if top && ch == sep {
            parts.push(text[start..at].trim());
            start = at + ch.len_utf8();
        }
    }
    parts.push(text[start..].trim());
    parts.retain(|part| !part.is_empty());
    parts
}

/// Splits `text` at the first top-level `sep`, returning the two trimmed halves.
pub fn split_once_top_level(text: &str, sep: char, nesting: Nesting) -> Option<(&str, &str)> {
    let mut buf = [0u8; 4];
    let needle: &str = sep.encode_utf8(&mut buf);
    let at = find_top_level(text, needle, nesting)?;
    Some((text[..at].trim(), text[at + needle.len()..].trim()))
}

/// Removes one layer of matching `"` (or `'`) quotes and trims the result.
pub fn unquote(text: &str) -> &str {
    let text = text.trim();
    for quote in ['"', '\''] {
        if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            return text[1..text.len() - 1].trim();
        }
    }
    text
}

/// Splits off the first whitespace-delimited word, returning it and the rest.
pub fn split_word(text: &str) -> (&str, &str) {
    let text = text.trim();
    match text.find(char::is_whitespace) {
        Some(at) => (&text[..at], text[at..].trim()),
        None => (text, ""),
    }
}

/// Whether `text` starts with `keyword` as a whole word, ignoring ASCII case.
///
/// Returns the remainder of the line when it matches.
pub fn strip_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let (word, rest) = split_word(text);
    word.eq_ignore_ascii_case(keyword).then_some(rest)
}

/// Characters that may appear in a Mermaid identifier.
///
/// Deliberately liberal: real-world diagrams use dots, dashes and Unicode in ids.
pub fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | '\'')
}

/// Reads the identifier at the start of `text`, returning it and the rest.
///
/// A quoted identifier (`"a b"`) is accepted and returned without its quotes.
pub fn take_ident(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    if let Some(rest) = text.strip_prefix('"')
        && let Some(end) = rest.find('"')
    {
        return (&rest[..end], rest[end + 1..].trim_start());
    }
    let end = text
        .find(|ch: char| !is_ident_char(ch))
        .unwrap_or(text.len());
    (&text[..end], text[end..].trim_start())
}

/// Builds a [`MermaidError::Syntax`].
pub fn syntax(line: usize, message: impl Into<String>) -> MermaidError {
    MermaidError::Syntax {
        line,
        message: message.into(),
    }
}

/// Builds a [`MermaidError::Unsupported`].
pub fn unsupported(line: usize, message: impl Into<String>) -> MermaidError {
    MermaidError::Unsupported {
        line,
        message: message.into(),
    }
}
