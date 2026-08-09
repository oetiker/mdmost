//! Fenced code block highlighting.
//!
//! The whole module exists to serve one function, [`highlight`], which turns the body
//! of a fenced code block into styled [`Line`]s:
//!
//! ```
//! use mdmost::{highlight::highlight, theme::Theme};
//!
//! let theme = Theme::default_dark();
//! let lines = highlight(Some("rust"), "let x = 1;\n", &theme);
//! assert_eq!(lines.len(), 1);
//! assert_eq!(lines[0].text(), "let x = 1;");
//! ```
//!
//! Three properties are load-bearing for the rest of `mdmost` (design spec §8):
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

mod acknowledgements;
mod scopes;

pub use acknowledgements::syntax_acknowledgements;

use std::sync::LazyLock;

use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{
    ParseState, ScopeStack, ScopeStackOp, SyntaxDefinition, SyntaxReference, SyntaxSet,
    SyntaxSetBuilder,
};
use syntect::util::LinesWithEndings;

use crate::text::{Line, Span, display_width, graphemes};
use crate::theme::{Style, Theme};

/// Columns between tab stops when expanding a tab in a code block.
pub const TAB_WIDTH: usize = 4;

/// Blocks larger than this (in bytes) are rendered as plain themed text.
///
/// Highlighting cost grows with the input, and a code block this large is being
/// skimmed, not read. The guard keeps `mdmost` responsive on generated files.
pub const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;

/// Blocks with more lines than this are rendered as plain themed text.
pub const MAX_HIGHLIGHT_LINES: usize = 10_000;

/// Syntax definitions written for `mdmost`, compiled into the binary.
///
/// Each is written under the project's own licence rather than vendored, and each
/// declares its `file_extensions`, so the ordinary token lookup finds it with no alias
/// entry. Keep the tuple's first element in step with the definition's `name` key: it is
/// only used for the error message when a definition fails to parse.
///
/// [`BUNDLED_SYNTAXES`] now carries a TOML and a Dockerfile definition of its own, so
/// these two are no longer the only way to highlight those fences — they are kept because
/// they are measurably better against this project's scope table. `bat`'s TOML gives a
/// table header no scope at all, so `[server.http]` lands in the plain-text slot instead
/// of the namespace one; its Dockerfile emits `RUN apk add --no-cache curl` as a single
/// undifferentiated span. Both are asserted by
/// `toml_covers_sections_keys_values_dates_and_arrays` and
/// `dockerfile_directives_are_keywords_not_commands`, which is where the comparison was
/// actually made. Delete these definitions only after re-running those two tests against
/// the bundled set.
const EXTRA_SYNTAXES: &[(&str, &str)] = &[
    (
        "TOML",
        include_str!("../assets/syntaxes/TOML.sublime-syntax"),
    ),
    (
        "Dockerfile",
        include_str!("../assets/syntaxes/Dockerfile.sublime-syntax"),
    ),
];

/// The bundled syntax set, deserialised from a compiled dump.
///
/// Not `syntect`'s own `load_defaults_newlines`: that is the Sublime Text bundle as it
/// stood in 2016 — seventy-five syntaxes with no TypeScript, Kotlin, Swift, Zig, Nix,
/// Terraform, Elixir, GraphQL, Vue, Svelte or SCSS in it. `two-face` re-packages the set
/// `bat` curates, versioned against a `bat` release, behind the same `SyntaxSet` type and
/// the same lookup API. It costs roughly 0.6 MiB of embedded definitions.
///
/// Loading is lazy and pre-linked, so a document with no code block pays nothing and one
/// with a code block pays a single deserialisation.
static BUNDLED_SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

/// A second set holding only [`EXTRA_SYNTAXES`].
///
/// Deliberately *not* merged into [`BUNDLED_SYNTAXES`]: adding a definition to that set
/// means calling `into_builder().build()`, which re-links the context references of every
/// bundled syntax and cost about 180 ms back when there were seventy-five of them — a
/// cost every document with any code block would pay, and one that has only grown with
/// the set. Built on its own, the same two definitions link in about 9 ms, and only a
/// document that actually contains a TOML or Dockerfile fence pays even that.
///
/// A definition that fails to parse is skipped rather than panicking;
/// `every_extra_syntax_loads` asserts that none currently does, so a broken definition
/// is a test failure and not a silent loss of highlighting.
static EXTRA_SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(|| {
    let mut builder = SyntaxSetBuilder::new();
    for (_, source) in EXTRA_SYNTAXES {
        if let Ok(definition) = SyntaxDefinition::load_from_str(source, true, None) {
            builder.add(definition);
        }
    }
    builder.build()
});

/// Language tags that the bundled set's own name and extension lookup does not resolve,
/// or resolves to something other than what a Markdown author means by the tag.
///
/// The right-hand side is a `syntect` token — a syntax name or a file extension — that
/// [`SyntaxSet::find_syntax_by_token`] does resolve.
///
/// This table is a *last* resort, not a first one: `find_syntax_by_token` already matches
/// every syntax name and every declared file extension case-insensitively, which covers
/// `rs`, `py`, `yml`, `sh`, `ts`, `tsx`, `c++`, `hcl`, `kt` and most of what people write
/// in a fence. An entry here is justified only when the raw tag resolves to nothing, or
/// to the wrong thing; `aliases_only_cover_tags_syntect_misses_or_misresolves` fails if a
/// row stops earning its place. Widening the bundled set retired four rows — `ts`, `tsx`,
/// `typescript` no longer have to borrow JavaScript, `jinja` no longer has to borrow HTML,
/// and `vim` no longer has to give up and render as plain text.
///
/// TOML and Dockerfiles have their own definitions in [`EXTRA_SYNTAXES`] and therefore
/// need no alias either.
const ALIASES: &[(&str, &str)] = &[
    ("apache", "htaccess"),
    ("cjs", "JavaScript"),
    ("console", "sh"),
    ("csharp", "cs"),
    ("docker", "dockerfile"),
    ("fortran", "f90"),
    ("fsharp", "f#"),
    ("golang", "go"),
    ("graphviz", "dot"),
    ("jsonc", "json"),
    ("jsx", "JavaScript"),
    ("ksh", "sh"),
    ("mjs", "JavaScript"),
    ("node", "JavaScript"),
    ("objc", "objective-c"),
    ("objcpp", "objective-c++"),
    ("plaintext", "txt"),
    ("python3", "python"),
    ("scheme", "scm"),
    ("shell", "sh"),
    ("shell-session", "sh"),
    ("text", "txt"),
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
    let Some((set, syntax)) = resolve_syntax(lang) else {
        return plain(src, theme);
    };
    if LinesWithEndings::from(src).count() > MAX_HIGHLIGHT_LINES {
        return plain(src, theme);
    }
    highlight_with(set, syntax, src, theme).unwrap_or_else(|| plain(src, theme))
}

/// The name of the `syntect` syntax a language tag resolves to, if any.
///
/// Useful to a renderer that wants to show the real language name in a code frame, and
/// to callers that want to know whether a tag would be highlighted at all.
pub fn syntax_name(lang: Option<&str>) -> Option<&'static str> {
    resolve_syntax(lang).map(|(_, syntax)| syntax.name.as_str())
}

/// Resolves a fence info string to a syntax.
///
/// Resolution order: the info string's first token, lower-cased, is looked up in
/// [`ALIASES`]; the result (or the token itself) is then handed to
/// [`SyntaxSet::find_syntax_by_token`], which matches syntax names and file extensions
/// case-insensitively. [`EXTRA_SYNTAX_SET`] is consulted first, so a definition written
/// for `mdmost` always wins over a same-named one in [`BUNDLED_SYNTAXES`].
/// `None` means "render as plain text".
///
/// The set is returned alongside the syntax because a [`ParseState`] must be driven by
/// the very set its [`SyntaxReference`] came from.
fn resolve_syntax(lang: Option<&str>) -> Option<(&'static SyntaxSet, &'static SyntaxReference)> {
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
    find_in_sets(token)
}

/// Looks a `syntect` token up in both sets, [`EXTRA_SYNTAX_SET`] first.
///
/// Split out from [`resolve_syntax`] so that it can be asked what a tag resolves to
/// *without* the alias table in the way.
fn find_in_sets(token: &str) -> Option<(&'static SyntaxSet, &'static SyntaxReference)> {
    if let Some(syntax) = EXTRA_SYNTAX_SET.find_syntax_by_token(token) {
        return Some((&EXTRA_SYNTAX_SET, syntax));
    }
    BUNDLED_SYNTAXES
        .find_syntax_by_token(token)
        .map(|syntax| (&*BUNDLED_SYNTAXES, syntax))
}

/// Highlights `src` with `syntax`, or returns `None` if the parser gave up.
///
/// Any parser error degrades the whole block rather than half of it, so the reader
/// never sees a block that is colourful at the top and plain at the bottom for no
/// visible reason.
fn highlight_with(
    set: &SyntaxSet,
    syntax: &SyntaxReference,
    src: &str,
    theme: &Theme,
) -> Option<Vec<Line>> {
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out = Vec::new();

    for raw in LinesWithEndings::from(src) {
        let ops = state.parse_line(raw, set).ok()?;
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
