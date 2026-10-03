// SPDX-License-Identifier: MIT
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
use crate::mermaid::ast::NotePlacement;
use crate::mermaid::entity;

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
///
/// A markdown string (``"`…`"``) may span several lines too, its line breaks being
/// breaks in the label. A line that leaves one open is joined with the lines after it
/// until the string closes, into one [`SrcLine`] numbered after its first line. Only a
/// markdown string does this: a stray `"` in a plain message must not swallow the rest
/// of the diagram.
pub fn preprocess(src: &str) -> Vec<SrcLine<'_>> {
    let mut out = Vec::new();
    let mut in_directive = false;
    // The first line number and start byte of a statement with a markdown string open.
    let mut open: Option<(usize, usize)> = None;
    for (index, raw) in src.lines().enumerate() {
        let number = index + 1;
        if let Some((first, start)) = open {
            let Some(end) = offset_of(src, raw).map(|at| at + raw.len()) else {
                continue;
            };
            let joined = &src[start..end];
            if !opens_markdown_string(joined) {
                open = None;
                let text = strip_comment(joined).trim();
                out.push(SrcLine {
                    number: first,
                    text,
                });
            }
            continue;
        }
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
        if opens_markdown_string(text)
            && let Some(start) = offset_of(src, text)
        {
            open = Some((number, start));
            continue;
        }
        out.push(SrcLine { number, text });
    }
    // A string never closed is handed on as it stands, for the parser to report.
    if let Some((number, start)) = open {
        let text = src[start..].trim();
        out.push(SrcLine { number, text });
    }
    out
}

/// Whether `text` ends inside a markdown string, one opened by ``"` ``.
fn opens_markdown_string(text: &str) -> bool {
    let mut quoted = false;
    let mut markdown = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            quoted = !quoted;
            markdown = quoted && chars.peek() == Some(&'`');
        }
    }
    quoted && markdown
}

/// Removes a trailing `%%` comment that is not inside a quoted string.
fn strip_comment(text: &str) -> &str {
    match find_top_level(text, "%%", Nesting::Ignore) {
        Some(at) => &text[..at],
        None => text,
    }
}

/// Computes the byte offset of `sub` within `src`, or `None` when `sub` is not a
/// byte-for-byte subslice of `src`.
///
/// `sub` should always be such a subslice — every lexing helper in this module slices
/// rather than allocates, so a label text handed to [`label_at`] is always genuinely
/// read from the mermaid source it claims to be. But that invariant is
/// convention-enforced, not type-enforced (a `Builder`'s `src` field carries no
/// lifetime tie to the strings it hands to `label_at`), so this checks both ends
/// rather than trusting the caller: plain `usize` subtraction has no overflow check
/// in a release build, and a wrapped offset looks like a small, plausible, *wrong*
/// position rather than a panic — one that a later pipeline stage could offset
/// further into a different kind of wrong. Failing closed here is what lets
/// [`label_at`] fail closed in turn.
///
/// Public (rather than folded entirely into `label_at`) because `state`'s multi-line
/// `note … end note` form grows one range across several pushed lines instead of
/// building a fresh label per line, and needs the same closed-failure guarantee.
pub fn offset_of(src: &str, sub: &str) -> Option<usize> {
    let at = (sub.as_ptr() as usize).checked_sub(src.as_ptr() as usize)?;
    (at + sub.len() <= src.len()).then_some(at)
}

/// Builds a label from `text`, which should be a subslice of the mermaid source `src`.
///
/// This is the one place a family parser should reach for when building a label from
/// real source text — it is [`offset_of`] plus the fallback the contract already
/// defines for "not from the source": when `text` cannot be placed in `src`, the
/// label gets [`Label::parse`]'s empty range instead of a wrong one.
pub fn label_at(src: &str, text: &str) -> crate::mermaid::ast::Label {
    match offset_of(src, text) {
        Some(at) => crate::mermaid::ast::Label::parse_at(text, at),
        None => crate::mermaid::ast::Label::parse(text),
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

/// Splits a line into `;`-separated statements, keeping a character reference whole.
///
/// `;` is Mermaid's statement separator and *also* the terminator of a character
/// reference, so a scan that knows nothing about references cuts a statement in half at
/// the entity's own semicolon: `s1 --> s2 : press &amp; hold` becomes a transition
/// labelled `press &amp` plus a second statement `hold`, which in a state diagram draws
/// a node the author never wrote. The scan therefore steps over exactly what
/// [`entity::reference_len`] recognises — which is exactly what the decoder will later
/// consume, so the splitter and the decoder cannot disagree.
///
/// This is the one home for that knowledge; every family whose separator is `;` calls
/// here. [`split_top_level`] deliberately keeps none of it, because its other separators
/// are not entity terminators: the class parser splits generic parameters on `,` and the
/// flowchart splits multi-node syntax on `&`, and neither may change.
pub fn split_statements(text: &str) -> Vec<&str> {
    split_scanned(text, false)
}

/// [`split_statements`] for the flowchart, which additionally keeps `|…|` labels intact.
///
/// A flowchart edge label is delimited by pipes rather than by brackets or quotes, so
/// [`Scanner`] does not protect it and a plain `;` typed inside one — `A -->|do this;
/// then that| B` — is label text rather than a separator. Only the flowchart family
/// needs this: no other family writes a `|`-delimited label, and ER's crow's-foot
/// operators use unpaired pipes that must not be mistaken for one.
pub fn split_piped_statements(text: &str) -> Vec<&str> {
    split_scanned(text, true)
}

/// The shared statement scan behind [`split_statements`] and [`split_piped_statements`].
fn split_scanned(text: &str, pipes: bool) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut scanner = Scanner::default();
    let mut piped = false;
    let mut start = 0;
    // The end of the reference currently being stepped over, so its `;` cannot separate.
    let mut skip_to = 0;
    for (at, ch) in text.char_indices() {
        if at < skip_to {
            continue;
        }
        if !scanner.step(ch, Nesting::Honour) {
            continue;
        }
        if matches!(ch, '&' | '#')
            && let Some(len) = entity::reference_len(&text[at..])
        {
            // A reference body is ASCII alphanumerics and `#` only, so nothing skipped
            // here could have been a quote or a bracket the scanner needed to see.
            skip_to = at + len;
            continue;
        }
        match ch {
            '|' if pipes => piped = !piped,
            ';' if !piped => {
                parts.push(text[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts.retain(|part| !part.is_empty());
    parts
}

/// Characters of a class name, as `classDef` and `:::name` write it.
pub fn is_class_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

/// Splits a trailing `:::name` class suffix off a node reference such as `A:::c` or
/// `A[text]:::c`, returning the reference and the class name.
///
/// This is the one reader of `:::` for every family that has it, so that the colour
/// work can keep the name where the parsers drop it today. The suffix counts only at
/// the top level and only when a class name runs from it to the end: inside a label or
/// a quoted name `:::` is text, and `A:::foo bar` is handed back whole for the caller
/// to read or reject as it would any other malformed reference.
pub fn split_class_suffix(text: &str) -> (&str, Option<&str>) {
    let text = text.trim();
    let Some(at) = find_top_level(text, ":::", Nesting::Honour) else {
        return (text, None);
    };
    let name = text[at + 3..].trim_end();
    if name.is_empty() || !name.chars().all(is_class_char) {
        return (text, None);
    }
    (text[..at].trim_end(), Some(name))
}

/// Drops the CSS fragments a `;` cut off a styling statement.
///
/// `;` ends a statement in Mermaid, also inside `style A fill:#fff;stroke:#000`, so the
/// splitter hands on `stroke:#000` as a statement of its own. Read as a node or a state
/// description it would fail the diagram or draw a box nobody wrote. Mermaid itself
/// ignores such a fragment, and so does this: a statement that reads as a CSS property
/// (`word:value`) is dropped when it follows a styling statement on the same line, or
/// another fragment dropped this way. The rule is tied to the styling statement rather
/// than to the shape alone, because `s2:waiting` on its own is a state description and
/// `Animal:+int age` a class member. For the same reason a property is read narrowly,
/// as CSS is written in practice: a lower-case name directly followed by `:`, so that
/// `class A foo; C : waiting` still gives state `C` its description.
///
/// `styling` lists the family's styling keywords in lower case: `class` styles in a
/// flowchart but declares in a class diagram.
pub fn drop_css_spill<'a>(statements: Vec<&'a str>, styling: &[&str]) -> Vec<&'a str> {
    let mut after_styling = false;
    statements
        .into_iter()
        .filter(|statement| {
            if after_styling && is_css_property(statement) {
                return false;
            }
            let word = split_word(statement).0.to_ascii_lowercase();
            after_styling = styling.contains(&word.as_str());
            true
        })
        .collect()
}

/// Whether `text` reads as one CSS property, `name:value`, such as `stroke-width:3px`.
fn is_css_property(text: &str) -> bool {
    let Some((name, value)) = text.split_once(':') else {
        return false;
    };
    !name.is_empty()
        && name.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-')
        && !value.trim().is_empty()
        && !value.starts_with(':')
}

/// Splits `text` at the first top-level `:` that starts a label or a description,
/// returning the two trimmed halves.
///
/// [`split_once_top_level`] with `:` would cut `A:::foo : text` inside the class
/// suffix and read `::foo : text` as the label. `:::` binds tighter than `:` (colour
/// spec §3.3), so a `:::` and the class name after it are stepped over here.
pub fn split_label_colon(text: &str, nesting: Nesting) -> Option<(&str, &str)> {
    let mut scanner = Scanner::default();
    // The end of the class suffix currently being stepped over.
    let mut skip_to = 0;
    for (at, ch) in text.char_indices() {
        if at < skip_to {
            continue;
        }
        if !scanner.step(ch, nesting) || ch != ':' {
            continue;
        }
        if text[at..].starts_with(":::") {
            // A class name is ASCII and holds no quote or bracket the scanner needs.
            let name = text[at + 3..]
                .find(|ch: char| !is_class_char(ch))
                .unwrap_or(text.len() - at - 3);
            skip_to = at + 3 + name;
            continue;
        }
        return Some((text[..at].trim(), text[at + 1..].trim()));
    }
    None
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

/// Splits `A as Alice` — or `"Long description" as s2` — into its two halves.
///
/// The keyword match is case-insensitive, as Mermaid's is. Shared by the sequence and
/// state parsers, which had byte-identical copies of this (design spec §14).
pub fn split_as(text: &str) -> Option<(&str, &str)> {
    let lowered = text.to_ascii_lowercase();
    let at = lowered.find(" as ")?;
    Some((text[..at].trim(), text[at + 4..].trim()))
}

/// Splits a leading `left of` / `right of` / `over` off a note statement.
///
/// Returns the placement and the rest of the line. A family that does not support
/// every placement — the state parser has no `over` — checks the result and reports
/// its own error, so the prefix matching itself lives in one place.
pub fn split_note_placement(rest: &str) -> Option<(NotePlacement, &str)> {
    let lowered = rest.to_ascii_lowercase();
    for (prefix, placement) in [
        ("left of", NotePlacement::LeftOf),
        ("right of", NotePlacement::RightOf),
        ("over", NotePlacement::Over),
    ] {
        if let Some(after) = lowered.strip_prefix(prefix) {
            // `to_ascii_lowercase` preserves byte length, so the suffix lines up.
            return Some((placement, &rest[rest.len() - after.len()..]));
        }
    }
    None
}

/// Splits a trailing `<<annotation>>` off `head`.
///
/// Shared by the class parser (`<<interface>>`) and the state parser (`<<choice>>`).
///
/// # Errors
///
/// Returns a syntax error when `head` opens a `<<` it never closes.
pub fn split_stereotype(head: &str, line: usize) -> Result<(&str, Option<&str>), MermaidError> {
    let Some(at) = head.find("<<") else {
        return Ok((head.trim(), None));
    };
    let after = &head[at + 2..];
    let close = after
        .find(">>")
        .ok_or_else(|| syntax(line, "unterminated `<<…>>` annotation".to_string()))?;
    Ok((head[..at].trim(), Some(after[..close].trim())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `offset_of`'s whole point: `sub` byte-identical to a piece of `src` but drawn
    /// from a different allocation is not a subslice of `src`, no matter what its
    /// contents say, and must fail closed rather than hand back a wrapped or
    /// out-of-bounds `usize`.
    #[test]
    fn offset_of_a_slice_from_a_different_allocation_fails_closed() {
        let src = "flowchart LR\n  A[Parse] --> B[Layout]\n";
        // Same bytes as the real `Parse`, but a separate allocation — `checked_sub`
        // must see this as "not within `src`", not as an in-bounds-looking offset.
        let elsewhere = String::from("Parse");
        assert_eq!(offset_of(src, &elsewhere), None);
    }

    /// The consumer-facing half of the same guarantee: `label_at` must not let a
    /// failed `offset_of` leak through as a wrong `source` — it falls back to
    /// `Label::parse`'s empty range, same as a label nobody claims a position for.
    #[test]
    fn label_at_a_slice_from_a_different_allocation_gets_an_empty_range() {
        let src = "flowchart LR\n  A[Parse] --> B[Layout]\n";
        let elsewhere = String::from("Parse");
        let label = label_at(src, &elsewhere);
        assert_eq!(label.lines, ["Parse"]);
        assert_eq!(label.source, 0..0);
    }

    #[test]
    fn split_class_suffix_takes_the_name_off_the_end() {
        assert_eq!(split_class_suffix("A:::foo"), ("A", Some("foo")));
        assert_eq!(split_class_suffix("A[x]:::a-b_1"), ("A[x]", Some("a-b_1")));
        assert_eq!(split_class_suffix(" A :::foo "), ("A", Some("foo")));
        assert_eq!(split_class_suffix("A"), ("A", None));
        // Inside a label or a quoted name `:::` is text, not a suffix.
        assert_eq!(split_class_suffix("A[\"x:::y\"]"), ("A[\"x:::y\"]", None));
        assert_eq!(split_class_suffix("A[x:::y]"), ("A[x:::y]", None));
        // Not a class name: left for the caller to report or read as it stands.
        assert_eq!(split_class_suffix("A:::"), ("A:::", None));
        assert_eq!(split_class_suffix("A:::foo bar"), ("A:::foo bar", None));
    }

    #[test]
    fn split_label_colon_steps_over_a_class_suffix() {
        let split = |text| split_label_colon(text, Nesting::Honour);
        assert_eq!(split("A:::foo : text"), Some(("A:::foo", "text")));
        assert_eq!(
            split("A:::foo --> B:::bar: go"),
            Some(("A:::foo --> B:::bar", "go"))
        );
        assert_eq!(split("a --> b : x:::y"), Some(("a --> b", "x:::y")));
        assert_eq!(split("A:::foo"), None);
        assert_eq!(split("[*] --> A:::foo"), None);
        assert_eq!(split("A : text"), Some(("A", "text")));
    }

    #[test]
    fn drop_css_spill_drops_only_what_follows_a_styling_statement() {
        let styling = ["style", "classdef"];
        let split = |text| drop_css_spill(split_statements(text), &styling);
        assert_eq!(
            split("style A fill:#fff;stroke:#000; stroke-width: 3px"),
            ["style A fill:#fff"]
        );
        assert_eq!(
            split("classDef c fill:red;stroke:#000; B"),
            ["classDef c fill:red", "B"]
        );
        // Without a styling statement before it, `word:value` is the family's own
        // business, such as a state description or a class member.
        assert_eq!(split("A --> B; s2:waiting"), ["A --> B", "s2:waiting"]);
        // A statement that is not CSS ends the run, so a later fragment is kept.
        assert_eq!(
            split("style A fill:#fff; B; s2:waiting"),
            ["style A fill:#fff", "B", "s2:waiting"]
        );
        // Only a lower-case name directly followed by `:` reads as a property.
        assert_eq!(
            split("style A fill:#fff; C : waiting"),
            ["style A fill:#fff", "C : waiting"]
        );
        assert_eq!(
            split("style A fill:#fff; Up:x"),
            ["style A fill:#fff", "Up:x"]
        );
        // A class suffix is not a property.
        assert_eq!(
            split("style A fill:#fff; B:::c"),
            ["style A fill:#fff", "B:::c"]
        );
    }
}
