//! Fenced code block highlighting.
//!
//! The whole module exists to serve one function, [`highlight`], which turns the body
//! of a fenced code block into styled [`Line`]s:
//!
//! ```
//! use mdless::{highlight::highlight, theme::Theme};
//!
//! let theme = Theme::default_dark();
//! let lines = highlight(Some("rust"), "let x = 1;\n", &theme);
//! assert_eq!(lines.len(), 1);
//! assert_eq!(lines[0].text(), "let x = 1;");
//! ```
//!
//! Three properties are load-bearing for the rest of `mdless` (design spec §8):
//!
//! * **Colours come from the active [`Theme`], never from a `syntect` theme.** The
//!   scope → semantic-slot table lives in [`scopes`]; see its documentation for the
//!   reasoning behind the groupings.
//! * **Lines are never wrapped.** A long line is returned intact and the renderer
//!   clips or scrolls it horizontally, exactly like a wide table.
//! * **Highlighting cannot fail.** An unknown language tag, a syntax that bails out,
//!   or a block too large to be worth highlighting all degrade to plain themed text.
//!
//! Tabs are expanded to spaces on real tab stops *after* parsing, so that
//! tab-sensitive syntaxes (a `Makefile` recipe line) still parse correctly while the
//! canvas — which has no notion of a tab — receives only printable text.

mod scopes;

use std::sync::LazyLock;

use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, ScopeStack, ScopeStackOp, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::text::{Line, Span, display_width, graphemes};
use crate::theme::{Style, Theme};

/// Columns between tab stops when expanding a tab in a code block.
pub const TAB_WIDTH: usize = 4;

/// Blocks larger than this (in bytes) are rendered as plain themed text.
///
/// Highlighting cost grows with the input, and a code block this large is being
/// skimmed, not read. The guard keeps `mdless` responsive on generated files.
pub const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;

/// Blocks with more lines than this are rendered as plain themed text.
pub const MAX_HIGHLIGHT_LINES: usize = 10_000;

/// The default `syntect` syntax set, loaded once.
///
/// Loading takes long enough to be visible at startup for a pager, so it happens
/// lazily on the first highlighted block and never again.
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Language tags that `syntect`'s own name and extension lookup does not resolve, or
/// resolves to something other than what a Markdown author means by the tag.
///
/// The right-hand side is a `syntect` token — a syntax name or a file extension — that
/// [`SyntaxSet::find_syntax_by_token`] does resolve.
///
/// The default set has no syntax of its own for TOML, Dockerfiles or TypeScript, so
/// those tags borrow the closest relative rather than falling back to plain text: a
/// Dockerfile is mostly shell, a TOML file is `key = value` with `#` comments like a
/// properties file, and TypeScript is a superset of JavaScript.
const ALIASES: &[(&str, &str)] = &[
    ("cjs", "JavaScript"),
    ("console", "sh"),
    ("docker", "sh"),
    ("dockerfile", "sh"),
    ("golang", "go"),
    ("jinja", "html"),
    ("jsonc", "json"),
    ("jsx", "JavaScript"),
    ("ksh", "sh"),
    ("mjs", "JavaScript"),
    ("node", "JavaScript"),
    ("plaintext", "txt"),
    ("python3", "python"),
    ("shell", "sh"),
    ("shell-session", "sh"),
    ("text", "txt"),
    ("toml", "properties"),
    ("ts", "JavaScript"),
    ("tsx", "JavaScript"),
    ("typescript", "JavaScript"),
    ("vim", "txt"),
];

/// Highlights the body of a fenced code block.
///
/// `lang` is the fence's info string, if any; only the first word before a comma or
/// space is considered, so `rust,no_run` and `rust ignore` resolve like `rust`. An
/// unknown or absent tag produces plain themed text, never an error.
///
/// The returned [`Line`]s are unwrapped and unpadded: one per source line, in order,
/// with a trailing newline (and a `\r` before it) stripped. Every span carries a style
/// taken from `theme`.
pub fn highlight(lang: Option<&str>, src: &str, theme: &Theme) -> Vec<Line> {
    if src.len() > MAX_HIGHLIGHT_BYTES {
        return plain(src, theme);
    }
    let Some(syntax) = resolve_syntax(lang) else {
        return plain(src, theme);
    };
    if LinesWithEndings::from(src).count() > MAX_HIGHLIGHT_LINES {
        return plain(src, theme);
    }
    highlight_with(syntax, src, theme).unwrap_or_else(|| plain(src, theme))
}

/// The name of the `syntect` syntax a language tag resolves to, if any.
///
/// Useful to a renderer that wants to show the real language name in a code frame, and
/// to callers that want to know whether a tag would be highlighted at all.
pub fn syntax_name(lang: Option<&str>) -> Option<&'static str> {
    resolve_syntax(lang).map(|syntax| syntax.name.as_str())
}

/// Resolves a fence info string to a syntax.
///
/// Resolution order: the info string's first token, lower-cased, is looked up in
/// [`ALIASES`]; the result (or the token itself) is then handed to
/// [`SyntaxSet::find_syntax_by_token`], which matches syntax names and file extensions
/// case-insensitively. `None` means "render as plain text".
fn resolve_syntax(lang: Option<&str>) -> Option<&'static SyntaxReference> {
    let tag = lang?
        .trim()
        .split([',', ' ', '\t', '{'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if tag.is_empty() {
        return None;
    }
    let token = ALIASES
        .iter()
        .find(|(alias, _)| *alias == tag)
        .map_or(tag.as_str(), |(_, token)| token);
    SYNTAX_SET.find_syntax_by_token(token)
}

/// Highlights `src` with `syntax`, or returns `None` if the parser gave up.
///
/// Any parser error degrades the whole block rather than half of it, so the reader
/// never sees a block that is colourful at the top and plain at the bottom for no
/// visible reason.
fn highlight_with(syntax: &SyntaxReference, src: &str, theme: &Theme) -> Option<Vec<Line>> {
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out = Vec::new();

    for raw in LinesWithEndings::from(src) {
        let ops = state.parse_line(raw, &SYNTAX_SET).ok()?;
        let mut line = Line::empty();
        let mut column = 0usize;
        let mut style = theme.code.text;
        let mut restyle = true;

        for (text, op) in ScopeRegionIterator::new(&ops, raw) {
            if !matches!(op, ScopeStackOp::Noop) {
                stack.apply(op).ok()?;
                restyle = true;
            }
            let text = strip_eol(text);
            if text.is_empty() {
                continue;
            }
            if restyle {
                style = scopes::style_for(stack.as_slice(), &theme.code);
                restyle = false;
            }
            line.push(Span::new(expand_tabs(text, &mut column), style));
        }
        out.push(line);
    }
    Some(out)
}

/// Renders `src` as plain themed text, one [`Line`] per source line.
fn plain(src: &str, theme: &Theme) -> Vec<Line> {
    LinesWithEndings::from(src)
        .map(|raw| {
            let mut column = 0usize;
            let text = expand_tabs(strip_eol(raw), &mut column);
            if text.is_empty() {
                Line::empty()
            } else {
                Line::styled(text, theme.code.text)
            }
        })
        .collect()
}

/// Strips one trailing `\n` and the `\r` of a CRLF pair.
fn strip_eol(text: &str) -> &str {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.strip_suffix('\r').unwrap_or(text)
}

/// Expands tabs in `text` to the next tab stop, advancing `column` past the result.
///
/// `column` is the display column the run starts at, so tab stops are correct across
/// span boundaries within one line. Text without tabs is passed through untouched
/// apart from the column bookkeeping.
fn expand_tabs(text: &str, column: &mut usize) -> String {
    if !text.contains('\t') {
        *column += display_width(text);
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + TAB_WIDTH);
    for cluster in graphemes(text) {
        if cluster == "\t" {
            let spaces = TAB_WIDTH - (*column % TAB_WIDTH);
            out.extend(std::iter::repeat_n(' ', spaces));
            *column += spaces;
        } else {
            out.push_str(cluster);
            *column += display_width(cluster);
        }
    }
    out
}

/// The style a plain, unhighlighted code line is drawn in.
///
/// Exposed so that callers rendering a degraded block (an oversized fence, a syntax
/// that is not installed) can match the highlighter exactly instead of guessing.
pub fn plain_style(theme: &Theme) -> Style {
    theme.code.text
}

#[cfg(test)]
mod tests;
