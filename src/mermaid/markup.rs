// SPDX-License-Identifier: MIT
//! Bold and italic inside Mermaid label text.
//!
//! Two notations reach a label. Any label may carry the HTML tags `<b>`, `<strong>`,
//! `<i>` and `<em>`, which Mermaid passes through to the browser. A Mermaid *markdown
//! string* — label text wrapped in backticks, ``"`like **this**`"`` — also reads
//! `**bold**`, `__bold__`, `*italic*` and `_italic_`. Mermaid supports nothing more
//! inside one, so neither does this.
//!
//! The markup itself draws nothing. [`parse_line`] removes it and reports the visible
//! text, which bytes of the raw text each part of it was copied from, and which parts
//! are bold or italic. The copy map is the one [`entity::decode_runs`] gives, extended
//! over the removed markup, so a selection inside a styled label still resolves to the
//! right source bytes.
//!
//! Any other tag, and a delimiter with nothing to pair with, is left as written.

use std::ops::Range;

use crate::mermaid::entity::{self, Run};
use crate::theme::Attributes;

/// One line of label text with its markup read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Styled {
    /// The visible text, entities decoded.
    pub text: String,
    /// Where each run of `text` came from in the raw line; they tile `text` in order.
    pub runs: Vec<Run>,
    /// Byte ranges of `text` drawn bold or italic, in order and not overlapping.
    pub marks: Vec<(Range<usize>, Attributes)>,
}

/// Reads the markup in one line of raw label text.
///
/// `markdown` says the line belongs to a markdown string, which adds the `*` and `_`
/// delimiters to the HTML tags every label understands.
pub fn parse_line(raw: &str, markdown: bool) -> Styled {
    let mut out = Styled {
        text: String::new(),
        runs: Vec::new(),
        marks: Vec::new(),
    };
    let mut state = State::default();
    // The raw bytes not yet copied out, from `copied` to the current position.
    let mut copied = 0usize;
    let mut at = 0usize;
    while at < raw.len() {
        let Some((len, change)) = markup_at(raw, at, markdown, &state) else {
            at += raw[at..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        out.push(raw, copied..at, state.attrs());
        state.apply(change);
        at += len;
        copied = at;
    }
    out.push(raw, copied..raw.len(), state.attrs());
    out
}

impl Styled {
    /// Appends the raw bytes `range`, entity-decoded, in the attributes `attrs`.
    fn push(&mut self, raw: &str, range: Range<usize>, attrs: Attributes) {
        if range.is_empty() {
            return;
        }
        let (decoded, runs) = entity::decode_runs(&raw[range.clone()]);
        let base = self.text.len();
        self.text.push_str(&decoded);
        self.runs.extend(runs.into_iter().map(|run| Run {
            text: base + run.text.start..base + run.text.end,
            source: range.start + run.source.start..range.start + run.source.end,
            faithful: run.faithful,
        }));
        if attrs == Attributes::NONE {
            return;
        }
        let span = base..self.text.len();
        match self.marks.last_mut() {
            Some((last, kind)) if *kind == attrs && last.end == span.start => last.end = span.end,
            _ => self.marks.push((span, attrs)),
        }
    }
}

/// Which emphasis is open, and what opened it.
#[derive(Debug, Default)]
struct State {
    /// Open `<b>` / `<strong>` tags.
    bold_tags: usize,
    /// Open `<i>` / `<em>` tags.
    italic_tags: usize,
    /// The markdown delimiter that opened bold, `**` or `__`.
    bold_mark: Option<&'static str>,
    /// The markdown delimiter that opened italic, `*` or `_`.
    italic_mark: Option<&'static str>,
}

/// What one piece of markup does.
#[derive(Debug, Clone, Copy)]
enum Change {
    OpenBoldTag,
    CloseBoldTag,
    OpenItalicTag,
    CloseItalicTag,
    BoldMark(&'static str),
    ItalicMark(&'static str),
}

impl State {
    fn attrs(&self) -> Attributes {
        let mut attrs = Attributes::NONE;
        if self.bold_tags > 0 || self.bold_mark.is_some() {
            attrs = attrs | Attributes::BOLD;
        }
        if self.italic_tags > 0 || self.italic_mark.is_some() {
            attrs = attrs | Attributes::ITALIC;
        }
        attrs
    }

    fn apply(&mut self, change: Change) {
        match change {
            Change::OpenBoldTag => self.bold_tags += 1,
            Change::CloseBoldTag => self.bold_tags = self.bold_tags.saturating_sub(1),
            Change::OpenItalicTag => self.italic_tags += 1,
            Change::CloseItalicTag => self.italic_tags = self.italic_tags.saturating_sub(1),
            Change::BoldMark(mark) => {
                self.bold_mark = if self.bold_mark.is_some() {
                    None
                } else {
                    Some(mark)
                };
            }
            Change::ItalicMark(mark) => {
                self.italic_mark = if self.italic_mark.is_some() {
                    None
                } else {
                    Some(mark)
                };
            }
        }
    }
}

/// The tags read as emphasis, lower case, with what they do.
const TAGS: [(&str, Change); 8] = [
    ("<b>", Change::OpenBoldTag),
    ("</b>", Change::CloseBoldTag),
    ("<strong>", Change::OpenBoldTag),
    ("</strong>", Change::CloseBoldTag),
    ("<i>", Change::OpenItalicTag),
    ("</i>", Change::CloseItalicTag),
    ("<em>", Change::OpenItalicTag),
    ("</em>", Change::CloseItalicTag),
];

/// The markup starting at byte `at` of `raw`, as its length and its effect.
fn markup_at(raw: &str, at: usize, markdown: bool, state: &State) -> Option<(usize, Change)> {
    let rest = &raw[at..];
    if rest.starts_with('<') {
        return TAGS.iter().find_map(|&(tag, change)| {
            rest.get(..tag.len())
                .filter(|head| head.eq_ignore_ascii_case(tag))
                .map(|_| (tag.len(), change))
        });
    }
    if !markdown {
        return None;
    }
    for mark in ["**", "__"] {
        if rest.starts_with(mark) && toggles(raw, at, mark, state.bold_mark) {
            return Some((mark.len(), Change::BoldMark(mark)));
        }
    }
    for mark in ["*", "_"] {
        if rest.starts_with(mark) && toggles(raw, at, mark, state.italic_mark) {
            return Some((mark.len(), Change::ItalicMark(mark)));
        }
    }
    None
}

/// Whether the delimiter `mark` at byte `at` opens or closes emphasis.
///
/// `open` is the delimiter that opened this kind of emphasis, if any. Only that same
/// delimiter closes it, and only straight after visible text; an opener needs visible
/// text after it and a closer further on. An `_` inside a word, as in `snake_case`, is
/// never a delimiter, which is the `CommonMark` rule Mermaid follows.
fn toggles(raw: &str, at: usize, mark: &str, open: Option<&str>) -> bool {
    let before = raw[..at].chars().next_back();
    let after = raw[at + mark.len()..].chars().next();
    let underscore = mark.starts_with('_');
    match open {
        Some(opened) => {
            opened == mark
                && before.is_some_and(|ch| !ch.is_whitespace())
                && !(underscore && after.is_some_and(char::is_alphanumeric))
        }
        None => {
            let flanked = after.is_some_and(|ch| !ch.is_whitespace() && ch.to_string() != mark);
            let intraword = underscore && before.is_some_and(char::is_alphanumeric);
            flanked && !intraword && closer_after(raw, at + mark.len(), mark)
        }
    }
}

/// Whether a closing `mark` follows byte `from`, straight after visible text.
fn closer_after(raw: &str, from: usize, mark: &str) -> bool {
    let mut search = from;
    while let Some(found) = raw[search..].find(mark) {
        let at = search + found;
        let before = raw[..at].chars().next_back();
        if at > from && before.is_some_and(|ch| !ch.is_whitespace()) {
            return true;
        }
        search = at + mark.len();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The marked stretches of `raw` as `(text, bold, italic)` triples.
    fn marked(raw: &str, markdown: bool) -> (String, Vec<(String, bool, bool)>) {
        let styled = parse_line(raw, markdown);
        let marks = styled
            .marks
            .iter()
            .map(|(range, attrs)| {
                (
                    styled.text[range.clone()].to_string(),
                    attrs.contains(Attributes::BOLD),
                    attrs.contains(Attributes::ITALIC),
                )
            })
            .collect();
        (styled.text, marks)
    }

    #[test]
    fn html_tags_become_emphasis() {
        let (text, marks) = marked(
            "a <b>bold</b> <i>it</i> <em>em</em> <STRONG>s</STRONG>",
            false,
        );
        assert_eq!(text, "a bold it em s");
        assert_eq!(
            marks,
            vec![
                ("bold".into(), true, false),
                ("it".into(), false, true),
                ("em".into(), false, true),
                ("s".into(), true, false),
            ]
        );
    }

    #[test]
    fn nested_tags_combine() {
        let (text, marks) = marked("<b>x <i>y</i></b>", false);
        assert_eq!(text, "x y");
        assert_eq!(
            marks,
            vec![("x ".into(), true, false), ("y".into(), true, true)]
        );
    }

    #[test]
    fn other_tags_stay_as_written() {
        let (text, marks) = marked("<u>under</u> a<b", false);
        assert_eq!(text, "<u>under</u> a<b");
        assert!(marks.is_empty());
    }

    #[test]
    fn markdown_delimiters_need_a_markdown_string() {
        assert_eq!(marked("**x**", false).0, "**x**");
        let (text, marks) = marked("**bold** and *it* and __b__ and _i_", true);
        assert_eq!(text, "bold and it and b and i");
        assert_eq!(
            marks,
            vec![
                ("bold".into(), true, false),
                ("it".into(), false, true),
                ("b".into(), true, false),
                ("i".into(), false, true),
            ]
        );
    }

    #[test]
    fn triple_stars_are_bold_and_italic() {
        let (text, marks) = marked("***both***", true);
        assert_eq!(text, "both");
        assert_eq!(marks, vec![("both".into(), true, true)]);
    }

    #[test]
    fn unpaired_and_intraword_delimiters_stay() {
        assert_eq!(marked("snake_case_name", true).0, "snake_case_name");
        assert_eq!(marked("2 * 3 = 6", true).0, "2 * 3 = 6");
        assert_eq!(marked("a **b", true).0, "a **b");
    }

    #[test]
    fn runs_map_visible_text_back_to_the_raw_bytes() {
        let raw = "<b>a&amp;b</b> c";
        let styled = parse_line(raw, false);
        assert_eq!(styled.text, "a&b c");
        for run in &styled.runs {
            if run.faithful {
                assert_eq!(&styled.text[run.text.clone()], &raw[run.source.clone()]);
            }
        }
        let tiled: String = styled
            .runs
            .iter()
            .map(|run| &styled.text[run.text.clone()])
            .collect();
        assert_eq!(tiled, styled.text);
    }
}
