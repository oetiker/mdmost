# Viewport-driven highlighter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the pager, draw code blocks plain on the first screen and colour them in
time-budgeted slices on the main loop — viewport first, then one screen each way — by
patching cell styles, while `--render-once` keeps its blocking output.

**Architecture:** `render` asks a `CodeSource` (carried in `Ctx`) for each block's lines.
`--render-once` and the public block entry points use a `BlockingSource` that memoises per
instance; the pager passes its `Highlighter`, which returns plain or part-coloured lines
and parses more between key presses. A new canvas side list, `CodeRow`, records where each
code line landed so `App` can find blocks in the viewport and patch their cells.

**Tech Stack:** Rust 2024, ratatui, vendored `syntect` 5.3.0 (`parse_line_with_limit`).

**Spec:** `docs/superpowers/specs/2026-09-25-viewport-highlighter-design.md` — read it
before any task. The section numbers below (§N) refer to it.

## Deviations from the spec, decided while planning

- **`CodeSource` lives in `src/highlight/source.rs`, not `src/render/`.** `render` already
  depends on `highlight` (through `bridge`); putting the trait in `render` would make
  `highlight` depend on `render` for `BlockingSource` and `Highlighter` to implement it.
- **No separate `CodeState` enum.** `highlight::Outcome` gains a `Pending` variant.
- **The source travels in `Ctx`, not `RenderOptions`.** `RenderOptions` is `Copy + Eq` and
  is the render-cache key; a `&dyn CodeSource` in `Ctx` stays out of the key by
  construction (§4 left this to the plan).
- **The block key is a `u64` issued by the source** (`CodeBlock::key`), not a hash. The
  blocking sources return `None`, so `--render-once` records no `CodeRow`s at all.
- **The input check between lines is a caller-supplied `should_stop` closure** handed to
  `Highlighter::advance`, so `highlight` never learns about the terminal.

## Global Constraints

- Vendored `syntect` stays `5.3.0`; nothing in `vendor/syntect/src/` changes (§11).
- `render` must not depend on `tui`; `src/export/` may depend only on `doc`.
- No worker thread. `highlight()` keeps its signature and blocking behaviour.
- Nothing may panic on document content: no `unwrap`/`expect`/indexing on anything
  derived from the document in non-test code.
- `#![forbid(unsafe_code)]` stays.
- Build and test with at most 4 cores and a target dir of your own:
  `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4`.
- Gates before every commit, all from the worktree root
  `/scratch/oetiker/claude-worktrees/mdmost-viewport-highlighter`:
  - `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 -- --test-threads=4`
  - `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo clippy -j4 --no-deps --all-targets -- -D warnings`
  - `cargo fmt --check`
- Pass `timeout: 600000` on every Bash call that builds or tests. Never end a turn while a
  background build is running.
- Comments and identifiers in English; comment density like the surrounding code.
- Test code in this plan writes `Doc::parse(..)`, `Theme::default_light()` and similar
  for brevity; use the constructors the neighbouring tests in the same file use. The
  assertions are the specification and stay as written.
- `CHANGES.md` entries go under `## Unreleased`, written for users (see Task 8).
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Theme switch while a block is half parsed** → the new render shows every code cell in
   the new theme's plain style (no leftover colour from the old theme), and colour returns
   on later ticks. Test in Task 7.
2. **Resize while a block is half parsed** → the re-render shows the finished lines
   coloured at their new positions and the rest plain; later patches land at the new
   positions, never at the old ones. Test in Task 7.
3. **Two fences with identical language and text** → one key, and both copies on screen
   are painted. Covered by the duplicate fence in Task 6's fixture.
4. **Reload that removes a block that is parked** → the block and its parked state are
   dropped; no panic; the next tick does nothing for it. Test in Task 5 (`end_render`) and
   Task 7.
5. **Tabs and double-width characters in highlighted code** → patched columns match the
   blocking render exactly. Covered by Task 6's fixture (a tab line and a CJK line).

---

### Task 1: A resumable parse

Split the per-line body of `highlight_with` into a `Parse` that can stop after any line.
Add `Outcome::Pending`. No behaviour changes.

**Files:**
- Modify: `src/highlight.rs` (`Outcome` at ~141, `highlight_with` at ~537)
- Test: `src/highlight/tests.rs`

**Interfaces:**
- Produces:
  - `pub(crate) struct Parse<'a>` with
    `pub(crate) fn new(set: &'a SyntaxSet, syntax: &'a SyntaxReference) -> Self` and
    `pub(crate) fn line(&mut self, raw: &str, styles: &CodeStyles, limit: NonZeroUsize) -> Result<Line, HighlightError>`.
  - `HighlightError` becomes `pub(crate)` (variants `TokenLimitExceeded`, `Other`).
  - `Outcome::Pending` — "highlighting has not finished yet; the lines are plain from the
    first unfinished line on". Never returned by the blocking path.
  - `pub(crate) fn plain(src: &str, styles: &CodeStyles) -> Vec<Line>` (was private).
  - `pub(crate) fn token_limit_for(line: &str) -> NonZeroUsize` (was private).

- [ ] **Step 1: Write the failing test**

Add to `src/highlight/tests.rs`:

```rust
/// A parse stopped after any line and continued with the same `Parse` gives exactly the
/// lines an uninterrupted parse gives: the state carried between lines is the whole of
/// what a resumed parse needs.
#[test]
fn a_parse_continued_line_by_line_matches_one_uninterrupted_parse() {
    let theme = Theme::default();
    let src = "/* a comment\n   spanning */\nfn main() {\n\tlet s = \"x\";\n}\n";
    let (set, syntax) = resolve_syntax(Some("rust")).expect("rust resolves");
    let whole = highlight_with(set, syntax, src, &theme.code, &token_limit_for)
        .unwrap_or_else(|_| panic!("rust parses"));
    let mut parse = Parse::new(set, syntax);
    let stepped: Vec<Line> = LinesWithEndings::from(src)
        .map(|raw| {
            parse
                .line(raw, &theme.code, token_limit_for(raw))
                .unwrap_or_else(|_| panic!("rust parses"))
        })
        .collect();
    assert_eq!(stepped, whole);
}
```

If `highlight/tests.rs` does not already `use syntect::util::LinesWithEndings;`, add it.

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib a_parse_continued_line_by_line`
Expected: compile error, `Parse` not found.

- [ ] **Step 3: Implement**

In `src/highlight.rs`, add `Pending` to `Outcome` (after `Failed`) with the doc comment
above, make `HighlightError`, `plain` and `token_limit_for` `pub(crate)`, and replace the
body of `highlight_with` with a loop over a new `Parse`:

```rust
/// A parse that can stop after any line and be continued later.
///
/// Holds exactly what `syntect` carries from one line to the next: the parser state and
/// the scope stack. After [`Parse::line`] returns an error the parse must be discarded —
/// `syntect` leaves its state undefined mid-line — so a caller that wants to try again
/// starts a fresh `Parse` from the block's first line.
pub(crate) struct Parse<'a> {
    set: &'a SyntaxSet,
    state: ParseState,
    stack: ScopeStack,
}

impl<'a> Parse<'a> {
    /// A parse positioned before the first line of a block in `syntax`.
    pub(crate) fn new(set: &'a SyntaxSet, syntax: &'a SyntaxReference) -> Self {
        Self {
            set,
            state: ParseState::new(syntax),
            stack: ScopeStack::new(),
        }
    }

    /// Parses one source line (with its line ending) into a styled [`Line`].
    pub(crate) fn line(
        &mut self,
        raw: &str,
        styles: &CodeStyles,
        limit: NonZeroUsize,
    ) -> Result<Line, HighlightError> {
        let ops = match self.state.parse_line_with_limit(raw, self.set, Some(limit)) {
            Ok(ops) => ops,
            Err(ParsingError::TokenLimitExceeded { .. }) => {
                return Err(HighlightError::TokenLimitExceeded);
            }
            Err(_) => return Err(HighlightError::Other),
        };
        let mut line = Line::empty();
        let mut column = 0usize;
        let mut style = styles.text;
        let mut restyle = true;
        for (text, op) in ScopeRegionIterator::new(&ops, raw) {
            if !matches!(op, ScopeStackOp::Noop) {
                self.stack.apply(op).map_err(|_| HighlightError::Other)?;
                restyle = true;
            }
            let text = strip_eol(text);
            if text.is_empty() {
                continue;
            }
            if restyle {
                style = scopes::style_for(self.stack.as_slice(), styles);
                restyle = false;
            }
            line.push(Span::new(expand_tabs(text, &mut column), style));
        }
        Ok(line)
    }
}

fn highlight_with(
    set: &SyntaxSet,
    syntax: &SyntaxReference,
    src: &str,
    styles: &CodeStyles,
    limit_for: &dyn Fn(&str) -> NonZeroUsize,
) -> Result<Vec<Line>, HighlightError> {
    let mut parse = Parse::new(set, syntax);
    LinesWithEndings::from(src)
        .map(|raw| parse.line(raw, styles, limit_for(raw)))
        .collect()
}
```

Keep `highlight_with`'s existing doc comment. Any `match` on `Outcome` elsewhere
(`render/code.rs:370` compares with `==`, so it compiles) must still compile; add
`Outcome::Pending` arms where the compiler asks.

- [ ] **Step 4: Run the test and the whole highlight suite**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib highlight`
Expected: PASS, including every pre-existing highlight test.

- [ ] **Step 5: Gates and commit**

Run the three gates from Global Constraints, then:

```bash
git add src/highlight.rs src/highlight/tests.rs
git commit -m "highlight: a parse that can stop after any line

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The `CodeSource` seam; retire the global memo

**Files:**
- Create: `src/highlight/source.rs`
- Modify: `src/highlight.rs` (remove `CACHE`, `Cache`, `Entry`, `with_cache`, `cache_get`,
  `cache_put`, `outcome`, `computed_count`, `HIGHLIGHT_GLOBALS_TEST_LOCK`,
  `MAX_CACHE_ENTRIES`, `highlight_capped`, `highlight_with_limit`; keep
  `highlight_uncached`, `highlight_with_syntax` as un-memoised test seam), module docs
- Modify: `src/render/mod.rs` (`Ctx` gains `code`; `Ctx::new`; new `with_code`)
- Modify: `src/render/document.rs` (`DocCtx` gains `code`; new `render_document_with`)
- Modify: `src/render/block.rs:68,99,122` (public entry points own a `BlockingSource`)
- Modify: `src/render/code.rs` (`framed_code`, `fallback`)
- Modify: `src/render/bridge.rs` (drop `highlight`/`outcome`; see Step 3)
- Modify: `src/render/mod.rs:64` (re-export `render_document_with`)
- Test: `src/highlight/tests.rs`, `src/render/tests.rs`

**Interfaces:**
- Consumes: Task 1's `Outcome::Pending`, `plain`, `token_limit_for`.
- Produces (all in `crate::highlight`, re-exported from `src/highlight.rs` with
  `mod source; pub use source::{BlockingSource, CodeBlock, CodeSource};`):

```rust
/// What a code source hands the renderer for one block.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlock {
    /// One line per source line, tabs expanded, in the block's final geometry.
    pub lines: Vec<Line>,
    /// What became of the block so far.
    pub outcome: Outcome,
    /// The source's handle for this block, if it colours blocks after layout.
    pub key: Option<u64>,
}

pub trait CodeSource: std::fmt::Debug {
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock;
}

pub struct BlockingSource { /* limit_for: fn(&str) -> NonZeroUsize, memo: RefCell<Memo> */ }
impl BlockingSource {
    pub fn new() -> Self;                                           // full v0.3.5 budget
    pub(crate) fn with_line_limit(limit_for: fn(&str) -> NonZeroUsize) -> Self;
    #[cfg(test)] pub(crate) fn computed_count(&self, lang: Option<&str>, src: &str) -> Option<u64>;
}
impl Default for BlockingSource;

/// Highlights on every call, remembers nothing. The default in a fresh `Ctx`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Uncached;
pub(crate) static UNCACHED: Uncached = Uncached;
```

- `crate::render::Ctx` gains `pub code: &'a dyn CodeSource` and
  `pub(crate) fn with_code(self, code: &'a dyn CodeSource) -> Self`.
- `pub fn render_document_with(doc: &Doc, width: u16, body_width: Option<u16>, theme: &Theme, options: &RenderOptions, code: &dyn CodeSource) -> Canvas`.
- `render_document` keeps its signature and calls `render_document_with` with a local
  `BlockingSource::new()`.

- [ ] **Step 1: Write the failing tests**

In `src/highlight/tests.rs`, replace `a_second_highlight_of_the_same_block_is_not_recomputed`
and `a_different_theme_recomputes_rather_than_reusing_the_colours` (both hold
`HIGHLIGHT_GLOBALS_TEST_LOCK`) with instance-based versions, and add:

```rust
#[test]
fn a_blocking_source_highlights_a_block_once_per_instance() {
    let theme = Theme::default();
    let source = BlockingSource::new();
    assert_eq!(source.computed_count(Some("rust"), TASK1_SRC), None);
    let first = source.block(Some("rust"), TASK1_SRC, &theme);
    let second = source.block(Some("rust"), TASK1_SRC, &theme);
    assert_eq!(first, second);
    assert_eq!(first.outcome, Outcome::Highlighted);
    assert_eq!(first.key, None);
    assert_eq!(source.computed_count(Some("rust"), TASK1_SRC), Some(1));
}

#[test]
fn a_blocking_source_recomputes_for_another_theme() {
    let dark = Theme::default_dark();
    let light = Theme::default_light();
    let source = BlockingSource::new();
    let a = source.block(Some("rust"), TASK1_SRC, &dark);
    let b = source.block(Some("rust"), TASK1_SRC, &light);
    assert_ne!(a.lines, b.lines);
    assert_eq!(source.computed_count(Some("rust"), TASK1_SRC), Some(1));
}

#[test]
fn a_blocking_source_under_a_tiny_limit_reports_failed_and_plain_lines() {
    let theme = Theme::default();
    let source = BlockingSource::with_line_limit(|_| NonZeroUsize::MIN);
    let block = source.block(Some("rust"), "fn main() {}\n", &theme);
    assert_eq!(block.outcome, Outcome::Failed);
    assert_eq!(block.lines, plain("fn main() {}\n", &theme.code));
}
```

If `Theme::default_light` does not exist, use the two theme constructors the removed test
used. Convert the remaining tests that used the lock, `highlight_with_limit` or
`computed_count` (`a_highlighted_block_says_so`, `a_block_that_exceeds_its_token_budget_is_failed`,
`a_non_limit_parse_error_gives_outcome_plain_on_the_production_path`, and their neighbours
listed by `grep -n "outcome(\|LOCK\|computed_count\|highlight_with_limit" src/highlight/tests.rs`)
so that each builds its own `BlockingSource` and reads `block(...).outcome`; the
`highlight_with_syntax` seam returns `(Vec<Line>, Outcome)` directly and needs no memo. No
test may take a lock afterwards. This is M-9.

In `src/render/tests.rs`, convert the three tests at ~4613, ~4817 and ~4858:
- `a_clipping_code_block_is_highlighted_once_however_often_it_is_laid_out`: render with
  `render_document_with(.., &source)` where `source = BlockingSource::new()`, then assert
  `source.computed_count(Some("rust"), CODE) == Some(1)`.
- `a_failed_block_captions_its_frame` and `a_caption_does_not_change_a_block_s_height`:
  instead of priming the global memo with `highlight_with_limit`, render through a
  `BlockingSource::with_line_limit(|_| NonZeroUsize::MIN)` (and, for the height test, a
  `BlockingSource::new()` for the uncaptioned side) passed with `render_document_with` or
  with `block::render_block_ctx(node, width, Ctx::new(&theme, &options).with_code(&source))`.

- [ ] **Step 2: Run to verify they fail**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib blocking_source`
Expected: compile errors (`BlockingSource` not found).

- [ ] **Step 3: Implement**

`src/highlight/source.rs`:

```rust
// SPDX-License-Identifier: MIT
//! Where the renderer gets a code block's lines from.
//!
//! The renderer does not call the highlighter directly. It asks a [`CodeSource`], so that
//! `--render-once` can colour every block before output while the pager draws first and
//! colours later (see `Highlighter`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroUsize;

use super::{Outcome, highlight_full, token_limit_for};
use crate::text::Line;
use crate::theme::{CodeStyles, Theme};

/// What a code source hands the renderer for one block.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlock {
    /// One line per source line, tabs expanded. Plain and highlighted lines of the same
    /// block always have the same text, so they occupy the same cells.
    pub lines: Vec<Line>,
    /// What became of the block so far.
    pub outcome: Outcome,
    /// The source's handle for this block, if it colours blocks after layout; the
    /// renderer records it on every drawn row (`canvas::CodeRow`).
    pub key: Option<u64>,
}

/// Gives the renderer a code block's lines.
pub trait CodeSource: std::fmt::Debug {
    /// The lines to draw for this block and what became of it, in one call.
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock;
}

/// Highlights on every call and remembers nothing.
///
/// The source a fresh `render::Ctx` starts with. Entry points that lay a block out at
/// several widths hand the context a [`BlockingSource`] instead.
#[derive(Debug, Clone, Copy, Default)]
pub struct Uncached;

/// The one [`Uncached`] every fresh context points at.
pub(crate) static UNCACHED: Uncached = Uncached;

impl CodeSource for Uncached {
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock {
        let (lines, outcome) = highlight_full(lang, src, &theme.code, token_limit_for);
        CodeBlock { lines, outcome, key: None }
    }
}

/// Highlights a block on first request and remembers it for the life of the instance.
///
/// `render::document::render_widened` lays a clipped block out at several widths; the
/// highlighter takes no width, so every one of those layouts asks for the same lines. One
/// instance per render keeps that to one parse, and dropping the instance with the render
/// is what keeps the memo from outliving the document it describes.
pub struct BlockingSource {
    limit_for: fn(&str) -> NonZeroUsize,
    memo: RefCell<Memo>,
}

#[derive(Default)]
struct Memo {
    styles: Option<CodeStyles>,
    entries: HashMap<(Option<String>, String), Entry>,
}

struct Entry {
    lines: Vec<Line>,
    outcome: Outcome,
    #[cfg(test)]
    computed: u64,
}

impl std::fmt::Debug for BlockingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockingSource").finish_non_exhaustive()
    }
}

impl Default for BlockingSource {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockingSource {
    /// A source with the full per-line budget, `token_limit_for`.
    pub fn new() -> Self {
        Self::with_line_limit(token_limit_for)
    }

    /// A source with the per-line budget replaced, for tests that need a real
    /// `Outcome::Failed` on a real syntax.
    pub(crate) fn with_line_limit(limit_for: fn(&str) -> NonZeroUsize) -> Self {
        Self {
            limit_for,
            memo: RefCell::new(Memo::default()),
        }
    }

    /// How many times this instance has highlighted the block, or `None` if it has not.
    #[cfg(test)]
    pub(crate) fn computed_count(&self, lang: Option<&str>, src: &str) -> Option<u64> {
        let memo = self.memo.borrow();
        memo.entries
            .get(&(lang.map(str::to_owned), src.to_owned()))
            .map(|entry| entry.computed)
    }
}

impl CodeSource for BlockingSource {
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock {
        let mut memo = self.memo.borrow_mut();
        if memo.styles != Some(theme.code) {
            memo.entries.clear();
            memo.styles = Some(theme.code);
        }
        let key = (lang.map(str::to_owned), src.to_owned());
        let entry = memo.entries.entry(key).or_insert_with(|| {
            let (lines, outcome) = highlight_full(lang, src, &theme.code, self.limit_for);
            Entry {
                lines,
                outcome,
                #[cfg(test)]
                computed: 1,
            }
        });
        CodeBlock {
            lines: entry.lines.clone(),
            outcome: entry.outcome,
            key: None,
        }
    }
}
```

The theme-change test above asserts `computed_count == Some(1)` after the switch: the
clear drops the old entry and the new one starts at 1.

In `src/highlight.rs`:
- Add `mod source; pub use source::{BlockingSource, CodeBlock, CodeSource, Uncached}; pub(crate) use source::UNCACHED;`.
- Add the un-memoised core both sources call:

```rust
/// Highlights `src` under `limit_for`, with no memo, and says what became of it.
pub(crate) fn highlight_full(
    lang: Option<&str>,
    src: &str,
    styles: &CodeStyles,
    limit_for: fn(&str) -> NonZeroUsize,
) -> (Vec<Line>, Outcome) {
    highlight_uncached(|| resolve_syntax(lang), src, styles, &limit_for)
}
```

- `highlight()` becomes `highlight_full(lang, src, &theme.code, token_limit_for).0`; update
  its doc comment (the paragraph about the memo goes; say it highlights on every call and
  that the renderer memoises through `BlockingSource`).
- `highlight_with_syntax` (test seam) returns `(Vec<Line>, Outcome)` from
  `highlight_uncached(|| Some((set, syntax)), src, &theme.code, &token_limit_for)`.
- Delete the memo items listed under **Files**. Update the module-level docs if they
  mention the memo.

In `src/render/mod.rs`: add the field to `Ctx`

```rust
    /// Where code blocks get their lines from (see `crate::highlight::CodeSource`).
    ///
    /// A fresh context highlights on every request; `render_document_with` and the public
    /// block entry points replace it with a memoising or a deferred source.
    pub code: &'a dyn crate::highlight::CodeSource,
```

set `code: &crate::highlight::UNCACHED` in `Ctx::new`, and add

```rust
    /// The same context, taking code blocks from `code`.
    pub(crate) fn with_code(self, code: &'a dyn crate::highlight::CodeSource) -> Self {
        Self { code, ..self }
    }
```

Any other struct-literal construction of `Ctx` (`block.rs:520,611,821`,
`document.rs:154`) uses `..` from an existing `Ctx` and needs no change; check each
compiles.

In `src/render/document.rs`: add `code: &'a dyn CodeSource` to `DocCtx`, chain
`.with_code(self.code)` in `DocCtx::ctx`, rename the body of `render_document` into

```rust
/// [`render_document`], with code blocks taken from `code` — the pager passes its
/// `Highlighter` here so the first render does not wait for highlighting.
pub fn render_document_with(
    doc: &Doc,
    width: u16,
    body_width: Option<u16>,
    theme: &Theme,
    options: &RenderOptions,
    code: &dyn CodeSource,
) -> Canvas { /* the old body, with `code` in DocCtx */ }

pub fn render_document(
    doc: &Doc,
    width: u16,
    body_width: Option<u16>,
    theme: &Theme,
    options: &RenderOptions,
) -> Canvas {
    render_document_with(doc, width, body_width, theme, options, &BlockingSource::new())
}
```

`render_flat` (called at the top of the body) must receive `code` too; thread it through
and set it on the `Ctx` it builds. Re-export `render_document_with` beside
`render_document` in `src/render/mod.rs:64`.

In `src/render/block.rs`, each public entry point at ~68, ~99, ~122 creates
`let code = BlockingSource::new();` and chains `.with_code(&code)` onto its `Ctx::new(..)`.

In `src/render/code.rs`, `framed_code`:

```rust
    let block = ctx.code.block(language, literal, theme);
    let lines = block.lines;
    ...
    let note = (block.outcome == crate::highlight::Outcome::Failed)
        .then(|| outcome_caption("highlighting gave up", ctx));
```

and `fallback`: `let lines = ctx.code.block(language, literal, theme).lines;`.
Delete `bridge::highlight` and `bridge::outcome` and remove `highlight` from the bridge's
module-doc list (the bridge now routes three collaborators; say the code source replaced
the fourth).

- [ ] **Step 4: Run the suites**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 -- --test-threads=4`
Expected: all PASS. The render snapshot tests prove output is unchanged. Also run
`grep -rn "HIGHLIGHT_GLOBALS_TEST_LOCK\|computed_count(Some\|highlight::outcome" src` —
expected: only `source.computed_count(...)` method calls remain.

- [ ] **Step 5: Gates and commit**

```bash
git add -A src
git commit -m "render: take code lines from a CodeSource; retire the global memo

The global (lang, src, theme) memo cleared itself whole above 256 fences and forced
every test that touched it to hold a process-wide lock. A BlockingSource memoises per
render instead; --render-once output is unchanged.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: One caption helper (M-7)

**Files:**
- Modify: `src/render/code.rs` (`framed_code` ~370, `outcome_caption` ~589, `fallback` ~788)
- Test: `src/render/tests.rs`

**Interfaces:**
- Produces: `fn bottom_caption(text: &str, width: u16, ctx: Ctx<'_>) -> Line` in
  `code.rs`, replacing `outcome_caption` and the inline construction in `fallback`.

- [ ] **Step 1: Write the failing tests**

Add to `src/render/tests.rs` near `a_failed_block_captions_its_frame`:

```rust
/// A narrow frame shortens the caption with an ellipsis rather than cutting it off, and
/// the caption is drawn in the same style as a Mermaid or math failure caption.
#[test]
fn a_failed_block_s_caption_is_ellipsized_and_styled_like_other_captions() {
    let theme = Theme::default();
    let options = RenderOptions::default();
    let source = BlockingSource::with_line_limit(|_| std::num::NonZeroUsize::MIN);
    let doc = Doc::parse("```rust\nfn main() {}\n```\n");
    let node = &doc.root().children[0];
    let canvas = block::render_block_ctx(
        node,
        16,
        Ctx::new(&theme, &options).with_code(&source),
    );
    let bottom = canvas.row_text(canvas.height() - 1);
    assert!(bottom.contains('…'), "caption not ellipsized: {bottom:?}");
    let cells = canvas.row(canvas.height() - 1).unwrap_or_default();
    let caption_cell = cells
        .iter()
        .find(|cell| cell.text() == "h")
        .expect("caption starts with 'h'");
    assert_eq!(caption_cell.style(), theme.block.caption);
}
```

Adjust the imports to what `render/tests.rs` already uses for `Doc`, `block`, `Ctx`,
and cell access; the assertion content must stay as written.

- [ ] **Step 2: Run to verify it fails**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib a_failed_block_s_caption_is_ellipsized`
Expected: FAIL (no `…`, and style is `code.overflow_marker`).

- [ ] **Step 3: Implement**

```rust
/// The label drawn into a code frame's bottom edge: what happened to this block.
///
/// Shared by a block whose highlighting gave up and by a diagram or formula that fell
/// back to its source, so every such report looks the same. The bottom edge is as long
/// as the block; a caption longer than that is elided rather than hard-cut, so it never
/// ends mid-word against the corner glyph.
fn bottom_caption(text: &str, width: u16, ctx: Ctx<'_>) -> Line {
    let room = usize::from(width).saturating_sub(4);
    Line::styled(crate::text::ellipsize(text, room), ctx.theme.block.caption)
}
```

`framed_code`: `.then(|| bottom_caption("highlighting gave up", width, ctx))`.
`fallback`: `let caption = bottom_caption(&caption.to_string(), width, ctx);` (remove the
old `room` computation and comment). Delete `outcome_caption`.

- [ ] **Step 4: Run render tests**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib render`
Expected: PASS. If a snapshot of a failed caption changes style, update it — that is the
intended visible change.

- [ ] **Step 5: Gates and commit**

```bash
git add src/render
git commit -m "render: one caption helper for code frames

'highlighting gave up' is ellipsized at narrow widths and drawn in the caption style
Mermaid and math failures already use.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `CodeRow`, the canvas side list

**Files:**
- Modify: `src/canvas/mod.rs` (struct, `empty`, accessors, `paint_code_line`, `check_invariants`)
- Modify: `src/canvas/ops.rs` (`merge_metadata` ~375, `slice_rows` ~565)
- Modify: `src/render/code.rs` (`code_area` records rows; callers pass the key)
- Test: `src/canvas/tests.rs`, `src/render/tests.rs`

**Interfaces:**
- Consumes: `CodeBlock::key` (Task 2).
- Produces:

```rust
/// Where one line of a code block was drawn, so its colour can be applied later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeRow {
    /// The code source's key for the block (`highlight::CodeBlock::key`).
    pub block: u64,
    /// The source line index within the block.
    pub line: usize,
    /// The canvas row.
    pub row: usize,
    /// The canvas column of the line's first code column.
    pub col: u16,
    /// How many columns of the line are drawn, stopping before a clip marker.
    pub cols: u16,
    /// The style each span is patched onto, as `Canvas::write_line` did.
    pub base: Style,
}

impl Canvas {
    pub fn code_rows(&self) -> &[CodeRow];
    pub fn add_code_row(&mut self, row: CodeRow);
    /// Restyles every drawn copy of `line` of `block` from `text`'s spans.
    pub fn paint_code_line(&mut self, block: u64, line: usize, text: &Line);
}
```

- [ ] **Step 1: Write the failing tests**

`src/canvas/tests.rs`:

```rust
fn code_row(row: usize, col: u16, cols: u16) -> CodeRow {
    CodeRow { block: 7, line: 0, row, col, cols, base: Style::default() }
}

#[test]
fn code_rows_travel_with_cells_through_blit_append_and_indent() {
    let mut inner = Canvas::new(10, 2, Style::default());
    inner.add_code_row(code_row(1, 2, 5));
    let mut outer = Canvas::new(20, 4, Style::default());
    outer.blit(&inner, 1, 3);
    assert_eq!(outer.code_rows(), &[code_row(2, 5, 5)]);

    let mut stacked = Canvas::new(20, 3, Style::default());
    stacked.append(&outer, Style::default());
    assert_eq!(stacked.code_rows(), &[code_row(5, 5, 5)]);

    let indented = inner.indent(4, 0, Style::default());
    assert_eq!(indented.code_rows(), &[code_row(1, 6, 5)]);

    let framed = inner.framed_captioned(
        BorderSet::ROUNDED, Style::default(), None, None, Style::default(),
    );
    assert_eq!(framed.code_rows(), &[code_row(2, 3, 5)]);
}

#[test]
fn slice_rows_keeps_only_code_rows_inside_the_slice() {
    let mut canvas = Canvas::new(10, 4, Style::default());
    canvas.add_code_row(code_row(0, 0, 3));
    canvas.add_code_row(code_row(2, 0, 3));
    let slice = canvas.slice_rows(1, 2);
    assert_eq!(slice.code_rows(), &[code_row(1, 0, 3)]);
}

#[test]
fn painting_a_code_line_restyles_only_its_drawn_columns() {
    let base = Style::default();
    let keyword = Style::default().fg(Color::Red);
    let mut canvas = Canvas::new(10, 1, base);
    canvas.write_str(0, 2, "let xy", base);
    canvas.add_code_row(CodeRow { block: 7, line: 0, row: 0, col: 2, cols: 4, base });
    let mut line = Line::empty();
    line.push(Span::new("let", keyword));
    line.push(Span::new(" xy", base));
    canvas.paint_code_line(7, 0, &line);
    let styles: Vec<Style> = canvas.row(0).unwrap_or_default().iter().map(|c| c.style()).collect();
    assert_eq!(styles[1], base);
    assert_eq!(&styles[2..5], &[keyword; 3]);
    assert_eq!(styles[5], base);       // inside cols, span style is base
    assert_eq!(styles[6], base);       // outside cols: untouched
    canvas.paint_code_line(8, 0, &line); // unknown block: no effect, no panic
    canvas.paint_code_line(7, 9, &line); // unknown line: no effect, no panic
}
```

Use whatever `Style`/`Color` builder the canvas tests already use for a coloured style
(`grep -n "fg(" src/canvas/tests.rs | head`); the assertions stay. Adjust `blit`,
`append`, `indent` and `framed_captioned` argument order to the real signatures in
`src/canvas/ops.rs` — the expected translated positions follow from them: blit at
(row 1, col 3); append below 3 rows; indent 4 columns; framed adds 1 row and 1 column.

`src/render/tests.rs`:

```rust
/// Every drawn code line records where it landed, and a blocking render records none.
#[test]
fn a_keyed_code_block_records_one_code_row_per_line() {
    #[derive(Debug)]
    struct Keyed;
    impl CodeSource for Keyed {
        fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock {
            CodeBlock { key: Some(42), ..Uncached.block(lang, src, theme) }
        }
    }
    let theme = Theme::default();
    let options = RenderOptions::default();
    let doc = Doc::parse("```rust\nfn a() {}\nfn b() {}\n```\n");
    let keyed = render_document_with(&doc, 60, None, &theme, &options, &Keyed);
    let rows: Vec<(u64, usize)> = keyed.code_rows().iter().map(|r| (r.block, r.line)).collect();
    assert_eq!(rows, vec![(42, 0), (42, 1)]);
    for row in keyed.code_rows() {
        let text: String = keyed.row_text(row.row).chars().skip(usize::from(row.col)).take(usize::from(row.cols)).collect();
        assert!(text.starts_with("fn "), "row {row:?} points at {text:?}");
    }
    let blocking = render_document(&doc, 60, None, &theme, &options);
    assert!(blocking.code_rows().is_empty());
}
```

`row_text` indexes by character, which equals columns for this ASCII fixture.

- [ ] **Step 2: Run to verify they fail**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib code_row`
Expected: compile errors (`CodeRow` not found).

- [ ] **Step 3: Implement**

`src/canvas/mod.rs`: add the `CodeRow` struct (with the doc comment below), a
`code_rows: Vec<CodeRow>` field initialised in `empty()`, and:

```rust
/// Where one line of a code block was drawn, so that its colour can arrive after layout.
///
/// A fifth metadata channel, beside anchors, spans, pins and hotspots. The pager draws a
/// document before its code is highlighted and patches the colour in later
/// (`tui::App::highlight_slice`); this records which cells belong to which source line.
/// It travels with cells, like a [`SearchSpan`], so a block inside a list, a quote or a
/// table cell is found where it was placed. `cols` stops before a clip marker, so
/// painting never recolours the marker.
```

```rust
    /// The code rows recorded by the renderer.
    pub fn code_rows(&self) -> &[CodeRow] {
        &self.code_rows
    }

    /// Records where a code line was drawn.
    pub fn add_code_row(&mut self, row: CodeRow) {
        self.code_rows.push(row);
    }

    /// Restyles every drawn copy of `line` of `block` from `text`'s spans.
    ///
    /// Each span's style is patched onto the row's `base`, exactly as
    /// [`Canvas::write_line`] combined them, so a painted line is cell-for-cell what a
    /// line written highlighted would have been. Symbols are never touched. A block or
    /// line with no recorded row is ignored.
    pub fn paint_code_line(&mut self, block: u64, line: usize, text: &Line) {
        let rows: Vec<CodeRow> = self
            .code_rows
            .iter()
            .filter(|r| r.block == block && r.line == line)
            .copied()
            .collect();
        for r in rows {
            let end = usize::from(r.cols);
            let mut x = 0usize;
            for span in &text.spans {
                if x >= end {
                    break;
                }
                let w = display_width(&span.text);
                let len = w.min(end - x);
                self.set_style(r.row, usize::from(r.col) + x, len, r.base.patch(span.style));
                x += w;
            }
        }
    }
```

In `check_invariants`, add: every code row's `row < self.height()`, with an error message
naming the row.

`src/canvas/ops.rs`: in `merge_metadata`, extend `self.code_rows` from `src.code_rows`
translated by `top` and `left16` (same as spans). In `slice_rows`, filter to
`start..end` and subtract `start` from `row`. Every operation that builds a new canvas by
struct literal or `Canvas::empty` and copies metadata by hand (search `spans:` and
`.spans =` in `ops.rs`) must copy `code_rows` the same way spans are copied there.

`src/render/code.rs`: add a `key: Option<u64>` parameter to `code_area`; `framed_code`
passes `block.key`, `fallback` passes its block's key. Inside the row loop, after
`out.write_line(row, gutter, line, theme.code.background);`:

```rust
        if let Some(block) = key {
            // Colour for this line may arrive after layout; record which cells it will
            // restyle. The same per-row clip question as the search span below: a row
            // with content past the budget loses its last column to the marker.
            let code_budget = budget.saturating_sub(gutter);
            let content_width = display_width(line.text().trim_end_matches(' '));
            let cols = if content_width > code_budget {
                code_budget.saturating_sub(display_width(OVERFLOW_MARKER))
            } else {
                line.width().min(code_budget)
            };
            out.add_code_row(CodeRow {
                block,
                line: row,
                row,
                col: u16::try_from(gutter).unwrap_or(u16::MAX),
                cols: u16::try_from(cols).unwrap_or(u16::MAX),
                base: theme.code.background,
            });
        }
```

The `width < 4` early returns in `framed_code` and `fallback` pass the key too.

- [ ] **Step 4: Run tests**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

```bash
git add src/canvas src/render
git commit -m "canvas: record where each code line is drawn

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: `Highlighter`, the pager's code source

**Files:**
- Create: `src/highlight/pager.rs`
- Modify: `src/highlight.rs` (`mod pager; pub use pager::{Highlighter, PAGER_LINE_TOKENS};`)
- Test: `src/highlight/pager_tests.rs` (declared `#[cfg(test)] mod tests;` style: put
  `#[cfg(test)] #[path = "pager_tests.rs"] mod tests;` at the bottom of `pager.rs`)

**Interfaces:**
- Consumes: `Parse`, `HighlightError`, `plain`, `token_limit_for`, `resolve_syntax`,
  `MAX_HIGHLIGHT_BYTES`, `MAX_HIGHLIGHT_LINES` (Task 1), `CodeSource`, `CodeBlock` (Task 2).
- Produces:

```rust
/// The per-line token cap in the pager; see `docs/maintainer-notes.md`.
pub const PAGER_LINE_TOKENS: NonZeroUsize;   // provisional 4_500, measured in Task 8

pub struct Highlighter { /* RefCell<Inner> */ }
impl Highlighter {
    pub fn new() -> Self;                     // line limit = min(token_limit_for, PAGER_LINE_TOKENS)
    pub(crate) fn with_line_limit(limit_for: fn(&str) -> NonZeroUsize) -> Self;
    pub fn outcome(&self, key: u64) -> Option<Outcome>;
    pub fn line(&self, key: u64, index: usize) -> Option<Line>;
    /// Parses `key` from where it stopped; returns the range of lines newly finished.
    pub fn advance(&mut self, key: u64, should_stop: &mut dyn FnMut() -> bool) -> std::ops::Range<usize>;
    /// Drops blocks the last render did not ask for; call once after each render.
    pub fn end_render(&mut self);
    /// Whether a block failed or went plain mid-parse since the last call.
    pub fn take_failed(&mut self) -> bool;
}
impl CodeSource for Highlighter;
impl Default for Highlighter;
```

`line` returns an owned `Line` (a clone) so callers never hold a `RefCell` borrow.

- [ ] **Step 1: Write the failing tests** (`src/highlight/pager_tests.rs`)

```rust
use std::num::NonZeroUsize;

use super::*;
use crate::highlight::{BlockingSource, CodeSource, Outcome, plain};
use crate::theme::Theme;

const RUST: &str = "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n";

fn run_to_end(h: &mut Highlighter, key: u64) {
    while h.outcome(key) == Some(Outcome::Pending) {
        h.advance(key, &mut || false);
    }
}

#[test]
fn a_new_block_is_plain_and_pending_with_a_key() {
    let theme = Theme::default();
    let h = Highlighter::new();
    let block = h.block(Some("rust"), RUST, &theme);
    assert_eq!(block.outcome, Outcome::Pending);
    assert_eq!(block.lines, plain(RUST, &theme.code));
    assert!(block.key.is_some());
    assert_eq!(h.block(Some("rust"), RUST, &theme).key, block.key);
}

#[test]
fn an_untagged_block_is_finished_plain_at_once() {
    let theme = Theme::default();
    let h = Highlighter::new();
    let block = h.block(None, RUST, &theme);
    assert_eq!(block.outcome, Outcome::Plain);
}

#[test]
fn advance_stops_when_asked_and_always_makes_progress() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let key = h.block(Some("rust"), RUST, &theme).key.unwrap_or_default();
    // Stop after every line: one line per call.
    assert_eq!(h.advance(key, &mut || true), 0..1);
    // Stop after the second check: two more lines.
    let mut checks = 0;
    assert_eq!(h.advance(key, &mut || { checks += 1; checks == 2 }), 1..3);
    let partial = h.block(Some("rust"), RUST, &theme);
    assert_eq!(partial.outcome, Outcome::Pending);
    let full = BlockingSource::new().block(Some("rust"), RUST, &theme);
    assert_eq!(&partial.lines[..3], &full.lines[..3]);
    assert_eq!(&partial.lines[3..], &plain(RUST, &theme.code)[3..]);
}

#[test]
fn a_finished_block_equals_the_blocking_result() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let key = h.block(Some("rust"), RUST, &theme).key.unwrap_or_default();
    run_to_end(&mut h, key);
    let done = h.block(Some("rust"), RUST, &theme);
    assert_eq!(done.outcome, Outcome::Highlighted);
    assert_eq!(done.lines, BlockingSource::new().block(Some("rust"), RUST, &theme).lines);
    assert_eq!(h.advance(key, &mut || false), 0..0);
}

#[test]
fn a_fifth_parked_block_evicts_the_least_recently_advanced() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let srcs: Vec<String> = (0..5).map(|i| format!("/* {i}\n*/ fn x{i}() {{}}\nfn y() {{}}\n")).collect();
    let keys: Vec<u64> = srcs
        .iter()
        .map(|s| h.block(Some("rust"), s, &theme).key.unwrap_or_default())
        .collect();
    for &key in &keys {
        h.advance(key, &mut || true); // one line each; the fifth evicts keys[0]
    }
    assert_eq!(h.parked_keys(), keys[1..].to_vec());
    // The evicted block keeps its first line and, resumed, ends where a fresh parse ends.
    assert!(h.line(keys[0], 0).is_some());
    run_to_end(&mut h, keys[0]);
    let expect = BlockingSource::new().block(Some("rust"), &srcs[0], &theme).lines;
    assert_eq!(h.block(Some("rust"), &srcs[0], &theme).lines, expect);
}

#[test]
fn a_line_over_the_pager_cap_fails_the_block_but_not_the_blocking_path() {
    let theme = Theme::default();
    let mut h = Highlighter::with_line_limit(|_| NonZeroUsize::new(3).unwrap_or(NonZeroUsize::MIN));
    let src = "fn a() {}\nlet x = [1, 2, 3, 4, 5, 6, 7, 8];\n";
    let key = h.block(Some("rust"), src, &theme).key.unwrap_or_default();
    run_to_end(&mut h, key);
    assert_eq!(h.outcome(key), Some(Outcome::Failed));
    assert!(h.take_failed());
    assert!(!h.take_failed());
    let block = h.block(Some("rust"), src, &theme);
    assert_eq!(block.lines, plain(src, &theme.code));
    assert_eq!(BlockingSource::new().block(Some("rust"), src, &theme).outcome, Outcome::Highlighted);
}

#[test]
fn the_pager_cap_is_the_smaller_of_the_two_budgets() {
    let short = "x";
    let long = "x".repeat(100_000);
    assert_eq!(pager_limit_for(short), crate::highlight::token_limit_for(short));
    assert_eq!(pager_limit_for(&long), PAGER_LINE_TOKENS);
}

#[test]
fn end_render_drops_blocks_the_render_did_not_ask_for() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let kept = h.block(Some("rust"), RUST, &theme).key.unwrap_or_default();
    let gone = h.block(Some("rust"), "fn gone() {}\n", &theme).key.unwrap_or_default();
    h.advance(gone, &mut || true); // parked
    h.end_render();
    // Next render asks only for `kept`.
    h.block(Some("rust"), RUST, &theme);
    h.end_render();
    assert_eq!(h.outcome(kept), Some(Outcome::Pending));
    assert_eq!(h.outcome(gone), None);
    assert!(h.parked_keys().is_empty());
    assert_eq!(h.advance(gone, &mut || false), 0..0);
}

#[test]
fn a_theme_change_starts_every_block_over() {
    let dark = Theme::default_dark();
    let light = Theme::default_light();
    let mut h = Highlighter::new();
    let key = h.block(Some("rust"), RUST, &dark).key.unwrap_or_default();
    run_to_end(&mut h, key);
    let again = h.block(Some("rust"), RUST, &light);
    assert_eq!(again.outcome, Outcome::Pending);
    assert_eq!(again.lines, plain(RUST, &light.code));
}

#[test]
fn more_than_256_blocks_are_each_highlighted_once() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let keys: Vec<u64> = (0..300)
        .map(|i| h.block(Some("rust"), &format!("fn f{i}() {{}}\n"), &theme).key.unwrap_or_default())
        .collect();
    h.end_render();
    for &key in &keys {
        run_to_end(&mut h, key);
    }
    for &key in &keys {
        assert_eq!(h.outcome(key), Some(Outcome::Highlighted));
        assert_eq!(h.advance(key, &mut || false), 0..0);
    }
}
```

`parked_keys` is a `#[cfg(test)] pub(crate) fn parked_keys(&self) -> Vec<u64>` returning
the parking lot oldest first. Use the same light/dark theme constructors as Task 2.

- [ ] **Step 2: Run to verify they fail**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib highlight::pager`
Expected: compile errors.

- [ ] **Step 3: Implement** (`src/highlight/pager.rs`)

```rust
// SPDX-License-Identifier: MIT
//! The pager's code source: draw first, colour later.
//!
//! [`Highlighter`] answers the renderer at once — plain lines for a block it has not
//! parsed, coloured lines for the part it has — and parses more in [`Highlighter::advance`],
//! which the pager calls between key presses. Design:
//! `docs/superpowers/specs/2026-09-25-viewport-highlighter-design.md`.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::ops::Range;

use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use super::{
    CodeBlock, CodeSource, HighlightError, MAX_HIGHLIGHT_BYTES, MAX_HIGHLIGHT_LINES, Outcome,
    Parse, plain, resolve_syntax, token_limit_for,
};
use crate::text::Line;
use crate::theme::{CodeStyles, Theme};

/// The per-line token cap in the pager.
///
/// One line cannot be split across slices: `syntect` cannot resume inside a line. This
/// bounds how long one line can hold up a key press. Measured in `docs/maintainer-notes.md`.
pub const PAGER_LINE_TOKENS: NonZeroUsize = match NonZeroUsize::new(4_500) {
    Some(n) => n,
    None => NonZeroUsize::MIN,
};

/// How many part-parsed blocks keep their parser state.
const PARKING_LOT: usize = 4;

/// The pager's per-line limit: the full budget, but never above [`PAGER_LINE_TOKENS`].
pub(crate) fn pager_limit_for(line: &str) -> NonZeroUsize {
    token_limit_for(line).min(PAGER_LINE_TOKENS)
}

type Syntax = (&'static SyntaxSet, &'static SyntaxReference);

struct Block {
    lang: Option<String>,
    src: String,
    /// Byte ranges of the source lines, line endings included.
    raw: Vec<Range<usize>>,
    syntax: Option<Syntax>,
    /// Finished lines: coloured while `Pending`/`Highlighted`, plain once `Failed`/`Plain`.
    lines: Vec<Line>,
    outcome: Outcome,
    /// The render generation that last asked for this block.
    seen: u64,
}

struct Inner {
    styles: Option<CodeStyles>,
    limit_for: fn(&str) -> NonZeroUsize,
    keys: HashMap<(Option<String>, String), u64>,
    blocks: HashMap<u64, Block>,
    next_key: u64,
    /// Parser state of part-parsed blocks, least recently advanced first, with the index
    /// of the next line each will parse.
    parked: VecDeque<(u64, Parse<'static>, usize)>,
    generation: u64,
    failed: bool,
}

/// The pager's code source. See the module documentation.
pub struct Highlighter {
    inner: RefCell<Inner>,
}
```

Implementation rules (write the code from these; every one is tested above):

- `new()` = `with_line_limit(pager_limit_for)`. `Default` calls `new()`. `Debug` is
  implemented by hand (`finish_non_exhaustive`), because `Parse` holds no `Debug`.
- `CodeSource::block`:
  1. If `inner.styles != Some(theme.code)`: clear `keys`, `blocks`, `parked`; set `styles`.
  2. Look up `(lang, src)` in `keys`; if absent, issue `next_key` (then increment) and
     insert a `Block`: `raw` from `LinesWithEndings::from(src)` accumulating byte offsets;
     `syntax` = `None` if `src.len() > MAX_HIGHLIGHT_BYTES`, else `resolve_syntax(lang)`,
     else `None` if `raw.len() > MAX_HIGHLIGHT_LINES`. With `syntax == None` the block is
     finished at once: `lines = plain(src, styles)`, `outcome = Outcome::Plain`. Otherwise
     `lines = Vec::new()`, `outcome = Outcome::Pending`. The order of checks matches
     `highlight_uncached` so the outcome agrees with the blocking path.
  3. Set `seen = generation`.
  4. Return `CodeBlock { key: Some(key), outcome, lines }` where `lines` is `block.lines`
     for any finished outcome, and for `Pending` is `block.lines` followed by
     `plain(src)` from index `block.lines.len()` on.
- `advance(key, should_stop)`:
  1. Return `0..0` if the block is missing or not `Pending`.
  2. Take the parked entry for `key` out of `parked` if present; otherwise start
     `Parse::new(set, syntax)` at line 0.
  3. `let first = block.lines.len();` If `raw` is empty, set `Highlighted` and return
     `first..first`. Loop: parse `raw[next]` with `limit_for(raw_line)`. On `Ok(line)`:
     if `next >= block.lines.len()` push it (lines below `block.lines.len()` were
     finished before an eviction and are identical); `next += 1`; if
     `next == raw.len()`, set `Highlighted`. On `Err(TokenLimitExceeded)`:
     `lines = plain(src)`, `outcome = Failed`, `failed = true`, return `first..first`.
     On `Err(Other)`: the same with `outcome = Plain` (a coloured prefix may already be
     on screen, so the pager must re-render it plain too). Then call `should_stop()`
     exactly once — also after the last line, so the caller's clock sees every parsed
     line — and stop if it says so or the block is `Highlighted`.
  4. If still `Pending`, push `(key, parse, next)` to the back of `parked`; while
     `parked.len() > PARKING_LOT`, pop the front.
  5. Return `first..block.lines.len()`.
  Get the raw line with `block.src.get(range.clone())` and treat `None` as `Err(Other)` —
  no indexing that can panic.
- `end_render()`: remove blocks whose `seen != generation` (and their `keys` and
  `parked` entries); then `generation += 1`.
- `take_failed()`: return and clear `failed`.
- `outcome(key)`, `line(key, index)`: read `blocks`, clone.

Borrowing: `advance` takes `&mut self`, so use `self.inner.get_mut()` there; `block`
takes `&self` and uses `borrow_mut()`. No `RefCell` borrow is held across a call that
could re-enter.

- [ ] **Step 4: Run tests**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib highlight`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

```bash
git add src/highlight.rs src/highlight
git commit -m "highlight: Highlighter, a code source that parses in slices

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Equivalence — patched pager canvas equals the blocking render

**Files:**
- Test: `src/render/tests.rs` (new section at the end) and a fixture constant there

**Interfaces:**
- Consumes: `render_document_with`, `BlockingSource`, `Highlighter::{advance, line, outcome}`,
  `Canvas::{code_rows, paint_code_line, rows}`.

- [ ] **Step 1: Write the test**

```rust
/// Fences at top level, in a list, in a quote and in a table cell; a tab, a CJK line, and
/// the same fence twice. The pager's plain-then-patched canvas must equal the blocking
/// render cell for cell.
const EQUIVALENCE_DOC: &str = "\
# Equivalence

```rust
fn main() {
\tlet s = \"漢字 and tabs\";
}
```

- item

  ```python
  def f(x):
      return x  # comment
  ```

> ```sh
> echo \"$HOME\" | grep -v x
> ```

| code |
|------|
| `x` |

```rust
fn main() {
\tlet s = \"漢字 and tabs\";
}
```
";

#[test]
fn the_pager_s_patched_canvas_equals_the_blocking_render() {
    let theme = Theme::default();
    let options = RenderOptions::default();
    let doc = Doc::parse(EQUIVALENCE_DOC);
    for width in [40u16, 80, 120] {
        let blocking = render_document(&doc, width, Some(72), &theme, &options);
        let mut h = Highlighter::new();
        let mut canvas = render_document_with(&doc, width, Some(72), &theme, &options, &h);
        h.end_render();
        let mut keys: Vec<u64> = canvas.code_rows().iter().map(|r| r.block).collect();
        keys.dedup();
        for key in keys {
            while h.outcome(key) == Some(Outcome::Pending) {
                for index in h.advance(key, &mut || false) {
                    if let Some(line) = h.line(key, index) {
                        canvas.paint_code_line(key, index, &line);
                    }
                }
            }
        }
        assert_eq!(canvas.rows(), blocking.rows(), "width {width}");
    }
}
```

A table cell with a fenced block is not expressible in GFM pipe syntax; if
`render/tests.rs` already has a helper or fixture that places a code block inside a table
cell (see `a_wide_fence_in_a_table_cell_does_not_widen_the_table` at ~4780), reuse its
document text for the table part so the blit path is exercised. Compare `rows()`, not the
canvases: the pager canvas carries `code_rows` the blocking one does not.

- [ ] **Step 2: Run it**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib the_pager_s_patched_canvas_equals`
Expected: PASS. If it fails, the diff names the first differing row: fix the code, not
the test (`CodeRow::cols`, `base`, or a canvas operation that drops `code_rows`).

- [ ] **Step 3: Gates and commit**

```bash
git add src/render/tests.rs
git commit -m "test: the pager's patched canvas equals the blocking render

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Wire the `Highlighter` into `App` and the event loop

**Files:**
- Modify: `src/tui/cache.rs` (`canvas_mut`, `invalidate`)
- Modify: `src/tui/app.rs` (field ~291, `ensure_rendered` ~976, new methods)
- Modify: `src/tui/term.rs` (`event_loop` ~249–300, new tick next to `reload_tick_at` ~335)
- Test: `src/tui/tests.rs`

**Interfaces:**
- Consumes: Tasks 2, 4, 5.
- Produces:
  - `RenderCache::canvas_mut(&mut self) -> &mut Canvas`, `RenderCache::invalidate(&mut self)` (sets `key = None`).
  - `App::wanted_blocks(&self) -> Vec<u64>`, `App::has_highlight_work(&self) -> bool`,
    `App::highlight_slice(&mut self, budget: Duration, clock: &mut dyn FnMut() -> Instant, input_waiting: &mut dyn FnMut() -> bool)`.
  - `term::SLICE_BUDGET: Duration` (provisional 10 ms), `term::wait_timeout(app: &App) -> Duration`,
    `term::highlight_tick_at(app, clock, input_waiting)`.

- [ ] **Step 1: Write the failing tests** (`src/tui/tests.rs`, near the `reload_tick_at` tests at ~8640)

Use the existing `pager(source)` helper (tests.rs:40) and whatever it offers for size;
`app.canvas()` triggers the render.

```rust
/// A document with enough code to fill several screens.
fn code_heavy(blocks: usize) -> String {
    (0..blocks)
        .map(|i| format!("```rust\nfn f{i}() {{ let x = {i}; }}\n```\n\n"))
        .collect()
}

fn code_style_at(app: &mut App, row: usize) -> Vec<Style> {
    let canvas = app.canvas();
    canvas.code_rows().iter().filter(|r| r.row == row)
        .flat_map(|r| canvas.row(r.row).unwrap_or_default()
            [usize::from(r.col)..usize::from(r.col) + usize::from(r.cols)].iter().map(|c| c.style()))
        .collect()
}

#[test]
fn the_first_render_is_plain_and_ticks_colour_the_viewport_first() {
    let mut app = pager(&code_heavy(60));
    let plain = app.theme().code.text;
    let first = app.canvas().code_rows()[0].row;
    assert!(code_style_at(&mut app, first).iter().all(|s| *s == app.theme().code.background.patch(plain)));
    assert!(app.has_highlight_work());
    let mut clock = Clock::default();
    highlight_tick_at(&mut app, &mut || clock.step(Duration::from_millis(1)), &mut || false);
    assert!(code_style_at(&mut app, first).iter().any(|s| *s != app.theme().code.background.patch(plain)));
}

#[test]
fn highlighting_stops_at_the_halo_and_the_loop_goes_back_to_waiting() {
    let mut app = pager(&code_heavy(200));
    let mut clock = Clock::default();
    while app.has_highlight_work() {
        highlight_tick_at(&mut app, &mut || clock.step(Duration::from_millis(1)), &mut || false);
    }
    assert_eq!(wait_timeout(&app), POLL_INTERVAL);
    let height = app.viewport_height();
    let last = app.canvas().code_rows().last().copied().expect("code rows");
    assert!(last.row > 3 * height, "fixture must extend past the halo");
    let far = last.block;
    assert_eq!(app.highlighter().outcome(far), Some(Outcome::Pending));
}

#[test]
fn a_slice_stops_at_its_budget_on_the_injected_clock() {
    let mut app = pager(&code_heavy(60));
    let mut clock = Clock::default();
    // Each clock read advances 4 ms; a 10 ms budget is spent after the third read.
    highlight_tick_at(&mut app, &mut || clock.step(Duration::from_millis(4)), &mut || false);
    let done = app.wanted_blocks().iter()
        .filter(|&&k| app.highlighter().outcome(k) == Some(Outcome::Highlighted)).count();
    assert_eq!(done, 3);
}

#[test]
fn waiting_input_ends_a_slice_after_one_line() {
    let mut app = pager(&code_heavy(60));
    let mut clock = Clock::default();
    highlight_tick_at(&mut app, &mut || clock.step(Duration::ZERO), &mut || true);
    let done = app.wanted_blocks().iter()
        .filter(|&&k| app.highlighter().outcome(k) == Some(Outcome::Highlighted)).count();
    assert_eq!(done, 1);
}

#[test]
fn a_theme_switch_mid_parse_shows_plain_code_in_the_new_theme() { /* Review Focus 1 */ }

#[test]
fn a_resize_mid_parse_keeps_finished_colour_and_patches_new_positions() { /* Review Focus 2 */ }

#[test]
fn a_reload_that_removes_a_parked_block_does_not_panic() { /* Review Focus 4 */ }
```

Write the three Review Focus tests fully, following these exact assertions:
- **Theme switch:** advance one tick with `|| true` input (one line), switch theme with
  the same call the theme-switch key uses (`grep -n "fn .*theme" src/tui/app.rs`), render,
  assert every cell in every `CodeRow` range has style `new.code.background.patch(new.code.text)`;
  then tick until no work and assert some cell differs.
- **Resize:** use a multi-line block (`"```rust\nfn a() {}\nfn b() {}\nfn c() {}\n```\n"`
  repeated 10×); tick once with `|| true` input; resize (`app.resize(..)`, app.rs:899) to a
  different width; render; assert line 0 of the first block is coloured (differs from
  plain) and line 2 is plain; tick to completion; assert the canvas's code cells equal
  those of `render_document` at the new width (compare `rows()`).
- **Reload:** tick once so the first block is parked, then `app.reload(Doc::parse("# none\n"))`,
  render, tick; assert `!app.has_highlight_work()` and no panic.

`Clock` is the existing test helper at ~8640; if it has no `step`, add
`fn step(&mut self, by: Duration) -> Instant` that returns the current instant and then
advances it by `by`. Add `pub(super) fn highlighter(&self) -> &Highlighter` and
`pub(super) fn theme(&self) -> &Theme` on `App` if they do not exist.

The budget test's arithmetic: `highlight_slice` reads the clock once for the start
(t=0, deadline t=10). `should_stop` then reads it once after every parsed line,
including a block's last line (Task 5, `advance` step 3), at t=4, 8 and 12. `code_heavy`
has one line per block, so blocks 1 and 2 finish with time left, and block 3 finishes
on the read at t=12, which also ends the slice: 3 blocks. The rule is the
specification: each parsed line is followed by exactly one clock read, and the slice
stops at the first read at or past the deadline.

- [ ] **Step 2: Run to verify they fail**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --lib tui::tests::highlight tui::tests::the_first_render`
Expected: compile errors.

- [ ] **Step 3: Implement**

`src/tui/cache.rs`:

```rust
    /// The rendered canvas, for restyling cells after layout (colour arriving from the
    /// highlighter). Callers must not change symbols: `reach` and `pinned` were
    /// measured from them.
    pub fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    /// Forgets the key, so the next `refresh` renders again.
    pub fn invalidate(&mut self) {
        self.key = None;
    }
```

`src/tui/app.rs`: add `highlighter: crate::highlight::Highlighter` to `App` (default in
the constructor). In `ensure_rendered`, the render closure calls
`crate::render::render_document_with(&self.doc, width, self.config.body_width, &self.theme, &options, &self.highlighter)`,
and inside `if stale {` first call `self.highlighter.end_render();`. Then:

```rust
    /// The blocks worth highlighting now, most wanted first: those on screen, then those
    /// within one screen above or below (design spec §7).
    pub(super) fn wanted_blocks(&self) -> Vec<u64> {
        let height = self.viewport_height();
        let top = self.scroll;
        let view = top..top.saturating_add(height);
        let halo = top.saturating_sub(height)..top.saturating_add(2 * height);
        let rows = self.cache.canvas().code_rows();
        let mut out: Vec<u64> = Vec::new();
        for range in [view, halo] {
            let mut hits: Vec<(usize, u64)> = rows
                .iter()
                .filter(|r| range.contains(&r.row))
                .map(|r| (r.row, r.block))
                .collect();
            hits.sort_unstable();
            for (_, block) in hits {
                if !out.contains(&block) {
                    out.push(block);
                }
            }
        }
        out
    }

    /// Whether any wanted block still has lines to colour.
    pub(super) fn has_highlight_work(&self) -> bool {
        self.wanted_blocks()
            .into_iter()
            .any(|key| self.highlighter.outcome(key) == Some(Outcome::Pending))
    }

    /// Colours wanted blocks for up to `budget`, patching each finished line into the
    /// rendered canvas. Stops early when `input_waiting` says a key is pending. A block
    /// that gave up forces one re-render, which draws it plain with its caption.
    pub(super) fn highlight_slice(
        &mut self,
        budget: Duration,
        clock: &mut dyn FnMut() -> Instant,
        input_waiting: &mut dyn FnMut() -> bool,
    ) {
        let deadline = clock() + budget;
        for key in self.wanted_blocks() {
            if self.highlighter.outcome(key) != Some(Outcome::Pending) {
                continue;
            }
            let mut out_of_time = false;
            let finished = self.highlighter.advance(key, &mut || {
                out_of_time = clock() >= deadline || input_waiting();
                out_of_time
            });
            if self.highlighter.take_failed() {
                self.cache.invalidate();
                return;
            }
            let canvas = self.cache.canvas_mut();
            for index in finished {
                if let Some(line) = self.highlighter.line(key, index) {
                    canvas.paint_code_line(key, index, &line);
                }
            }
            if out_of_time {
                return;
            }
        }
    }
```

Note the clock is read once per finished line inside `should_stop` — that is the rule the
budget test states. `highlight_slice` must only run after a render; `event_loop` draws
(which renders) before it waits, so the canvas is current.

`src/tui/term.rs`:

```rust
/// How long one highlighting slice may run before the loop looks at input again.
/// Measured in `docs/maintainer-notes.md`.
pub(super) const SLICE_BUDGET: Duration = Duration::from_millis(10);

/// How long the loop may sleep: not at all while highlighting has work.
pub(super) fn wait_timeout(app: &App) -> Duration {
    if app.has_highlight_work() { Duration::ZERO } else { POLL_INTERVAL }
}

fn highlight_tick(app: &mut App) {
    highlight_tick_at(app, &mut Instant::now, &mut || {
        crossterm::event::poll(Duration::ZERO).unwrap_or(true)
    });
}

/// [`highlight_tick`], against a clock and an input probe the caller supplies, for tests.
pub(super) fn highlight_tick_at(
    app: &mut App,
    clock: &mut dyn FnMut() -> Instant,
    input_waiting: &mut dyn FnMut() -> bool,
) {
    app.highlight_slice(SLICE_BUDGET, clock, input_waiting);
}
```

In `event_loop`: `let waited = input.wait(wait_timeout(app))?;`, and replace

```rust
        if !crossterm::event::poll(Duration::ZERO)? {
            continue;
        }
```

with

```rust
        if !crossterm::event::poll(Duration::ZERO)? {
            // Nothing to answer: spend the moment on colour. Input always goes first.
            highlight_tick(app);
            continue;
        }
```

Make `POLL_INTERVAL` `pub(super)` if the test needs it.

- [ ] **Step 4: Run tests**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 -- --test-threads=4`
Expected: PASS. Existing TUI tests that compare rendered cells against a coloured
snapshot will now see plain code on the first render; for each such failure, decide: if
the test is about highlighting, run `highlight_tick_at` until `!has_highlight_work()`
before asserting; if it is about something else, keep it as is only if it does not look
at code colours. List every test changed in the commit message.

- [ ] **Step 5: Try it**

Build release and open the reference document; scroll; press keys while colour arrives:

```bash
CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo build -j4 --release
/scratch/oetiker/cargo-target-mdmost-vh/release/mdmost /home/oetiker/checkouts/oxutlk/docs/superpowers/plans/2026-09-21-media-worker.md
```

This step is for a human or a controller with a terminal; a subagent skips it and says so.

- [ ] **Step 6: Gates and commit**

```bash
git add src/tui
git commit -m "tui: colour code blocks in slices, viewport first

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Measure the constants; document; changelog

**Files:**
- Modify: `src/highlight/pager.rs` (`PAGER_LINE_TOKENS` value), `src/tui/term.rs` (`SLICE_BUDGET` value)
- Modify: `docs/maintainer-notes.md`, `docs/manual.md` (§ Syntax highlighting, ~400), `CHANGES.md`
- Test: `src/highlight/pager_tests.rs` (an `#[ignore]` measurement)

- [ ] **Step 1: Write the measurement**

```rust
/// Tokens per millisecond on this machine, release build. Run by hand:
/// `cargo test --release --lib tokens_per_millisecond -- --ignored --nocapture`.
#[test]
#[ignore = "measurement, not a check"]
fn tokens_per_millisecond() {
    use crate::highlight::tests::MINIFIED_JS_LINE;
    let (set, syntax) = resolve_syntax(Some("js")).expect("js resolves");
    let theme = Theme::default();
    let raw = format!("{MINIFIED_JS_LINE}\n");
    let unlimited = NonZeroUsize::new(usize::MAX).unwrap_or(NonZeroUsize::MIN);
    let mut samples = Vec::new();
    for _ in 0..20 {
        let mut parse = Parse::new(set, syntax);
        let start = std::time::Instant::now();
        let _ = parse.line(&raw, &theme.code, unlimited);
        samples.push(start.elapsed());
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    // Token count: the smallest limit that does not fail, by bisection.
    let (mut lo, mut hi) = (1usize, 1_000_000usize);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let limit = NonZeroUsize::new(mid).unwrap_or(NonZeroUsize::MIN);
        if Parse::new(set, syntax).line(&raw, &theme.code, limit).is_ok() { hi = mid } else { lo = mid + 1 }
    }
    println!("{lo} tokens in {median:?}: {:.1} tokens/ms", lo as f64 / median.as_secs_f64() / 1000.0);
}
```

- [ ] **Step 2: Run it three times, interleaved with other load noted**

Run: `CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-vh cargo test -j4 --release --lib tokens_per_millisecond -- --ignored --nocapture`
Set `PAGER_LINE_TOKENS` to 50 ms × the lowest of the three rates, rounded down to a
readable number. Keep `SLICE_BUDGET` at 10 ms unless the Task 7 Step 5 trial showed
visible lag; record either way.

- [ ] **Step 3: Time to first screen, before and after**

Build v0.3.5 (`git archive v0.3.5 | tar -x -C /scratch/oetiker/claude-tmp/mdmost-v035`)
and this branch in separate target dirs. Add an `#[ignore]` test in
`src/render/tests.rs` that reads the path in `MDMOST_TIMING_DOC` and prints the median of
10 `render_document_with(.., &Highlighter::new())` calls and of 10 `render_document`
calls at width 100. Also time `--render-once` for both binaries, interleaved, 5 runs each.
Record the figures as ranges; the ~0.4 s target is an estimate and not a gate, but
`--render-once` must not be slower than v0.3.5 beyond noise.

- [ ] **Step 4: Documentation**

`docs/maintainer-notes.md`: beside the v0.3.5 budget table, a section "Pager slices" with
the tokens/ms runs, the chosen `PAGER_LINE_TOKENS`, `SLICE_BUDGET`, the first-screen and
`--render-once` timings, the date, and the machine note ("shared host, ranges").

`docs/manual.md`, § Syntax highlighting, add (man-pages style, facts only):

> In the pager, code is drawn uncoloured first. Colour is added to the blocks on screen,
> then to the blocks up to one screen above and below. A block with a line too long to
> colour quickly is left uncoloured and its frame says `highlighting gave up`;
> `--render-once` colours such a block.

`CHANGES.md` under `## Unreleased`:

```markdown
### Changed

- Documents with many code blocks open at once: code is shown uncoloured first, and
  colour is added to the blocks on screen and about one screen around them.
- In the pager, a code block with one very long line, such as minified JavaScript, is
  shown uncoloured with `highlighting gave up` in its frame; `--render-once` still
  colours it.

### Fixed

- The `highlighting gave up` caption ends in `…` when the frame is too narrow for it,
  and is drawn in the same colour as the captions of diagrams and formulas that could
  not be drawn.
```

- [ ] **Step 5: Gates and commit**

```bash
git add -A src docs CHANGES.md
git commit -m "docs: measure pager slice constants; manual and changelog

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
