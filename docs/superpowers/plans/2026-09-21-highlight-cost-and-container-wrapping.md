# Highlight Cost and Container Wrapping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `mdmost` re-highlighting the same code block once per layout probe, stop a wide fence inside a list or quote dragging that container's prose to the terminal width, and stop a pathological syntax definition hanging the pager.

**Architecture:** Three independent changes to existing code plus one report. A memo in `src/highlight.rs` makes `highlight()` idempotent in cost as well as in value, which collapses the width-search's repeated calls and makes a resize cheap. The per-block width escalation in `src/render/document.rs` is extracted into a helper and applied inside `render_sequence`, so a container escalates the child that needs it rather than itself. A deadline guard runs each uncached highlight on a worker thread and falls back to plain text when the parser does not return.

**Tech Stack:** Rust 2024, `syntect` 5.3.0 (`default-fancy`), `two-face` 0.5.2, `ratatui`, `crossterm`.

**Spec:** No separate spec document. The Findings section below is the authority this plan argues from; every measurement in it was taken on this branch's parent commit and is reproducible with the commands given.

## Findings

Measured on `eae5f99`, release build, `--render-once`.

**F1 — the width search re-highlights.** `render_placed()` (`src/render/document.rs:272`) renders a block at `measure.prose`, and when `ClipTest` says it was cut short calls `render_widened()` (`src/render/document.rs:382`), which renders it again at full width, then doubles, then bisects — `block::render_block_ctx(node, at, …)` per probe. For a code block each probe re-enters `framed_code()` (`src/render/code.rs:342`) and calls `bridge::highlight()` from scratch. `highlight(lang, src, theme)` takes no width, so every probe recomputes an identical result.

    1077-line ts block, --width 250 --no-body-width   1.49 s   (nothing clips)
    1077-line ts block, --width 100 --no-body-width   7.28 s   (block is 113 columns)
    mdmost::highlight() on the same body             1.41 s   (measured in-process)

A 1689-line document of TypeScript plans takes 6.40 s; with every fence tag removed it takes 0.28 s.

**F2 — a container escalates whole.** The escalation in `render_placed()` applies to the top-level block. A list is one top-level block, so one over-wide fence inside one item re-lays the entire list at the full body width, prose included. Reproducer, rendered at `--width 120` (paragraphs wrap at 72, both list items wrap at 118):

    Plain paragraph long enough to wrap so we can see where the prose cap ends.

    1. A first item, long enough to wrap so we can see which column it wraps at.

       ```sh
       some --command --with --a --line --that --is --clearly --wider --than --the --cap
       ```

    2. A second item, long enough to wrap so we can see which column it wraps at.

Replacing the fence body with `echo hi` returns the list to 72. Block quotes behave the same way. A table nested in a list does not trigger it, because the table renderer fits itself to the width it is handed and never reports as clipped.

**F3 — a syntax definition can hang.** `ParseState::parse_line` does not return for the second of these two lines under the bundled `JavaScript` syntax:

    ```js
      | { type: "a" }
      /** x */
    ```

Observed: 180 s of CPU, flat 2 MB RSS, no progress. The loop in `highlight_with()` never regains control, so a budget checked per line cannot help. The same two lines hang under the `oniguruma` backend too when that syntax is selected by name, so the fault is in the syntax definition rather than in `fancy-regex`. `MAX_HIGHLIGHT_BYTES` and `MAX_HIGHLIGHT_LINES` cannot catch it: the input is two lines.

## Global Constraints

- `#![forbid(unsafe_code)]` is crate-level at `src/lib.rs:47`. No `unsafe`, anywhere, including in tests.
- Gates, all three must pass before any commit: `cargo fmt --check -p mdmost`, `cargo clippy --all-targets -p mdmost -- -D warnings`, `cargo test -p mdmost -j 4`.
- This machine is shared. Never let a build or a test run use more than 4 cores: pass `-j 4` to every `cargo` invocation.
- `pub fn highlight(lang: Option<&str>, src: &str, theme: &Theme) -> Vec<Line>` keeps that exact signature. The doc example at `src/highlight.rs:8-14` is a compiled test and calls it.
- No new C or system dependencies. The release builds `.deb`, `.rpm` and a Homebrew bottle from pure Rust; `oniguruma` is therefore out of scope for this plan however much faster it parses.
- Comments, identifiers and documentation in English.
- Documentation follows the house style already in these files: state what the code does and why a non-obvious choice was made; no self-praise, no restating the code in prose.
- Render tests live in `src/render/tests.rs`, which opens with `use super::*;` and defines `const PLAIN: RenderOptions = RenderOptions::new(false, false);` at line 35. Every test in this plan uses `PLAIN` and the bare `render_document(...)` path, because that is what the file's existing tests use. `RenderOptions` has no `Default`.
- The highlighter's geometry contract holds throughout: `plain()` and `highlight_with()` return one `Line` per source line, with identical text and identical `Line::width()`. Only the span division and the `Style` differ. Task 3 depends on this.

---

### Task 1: Memoise `highlight()`

**Files:**
- Modify: `src/highlight.rs` (add the cache; rename the current body of `highlight` to `highlight_uncached`)
- Test: `src/highlight/tests.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `pub(crate) fn computed_count(lang: Option<&str>, src: &str, theme: &Theme) -> Option<u64>` — how many times the cache has computed the entry for this key, or `None` if there is no entry. Task 4 uses the same cache and must keep this counter meaning "times the highlighter actually ran".

- [ ] **Step 1: Write the failing tests**

Append to `src/highlight/tests.rs`:

```rust
/// A key no other test uses, so the assertions below are unaffected by tests
/// running in parallel against the same global cache.
const TASK1_SRC: &str = "let task1_unique_probe = 1;\n";

#[test]
fn a_second_highlight_of_the_same_block_is_not_recomputed() {
    let theme = Theme::default_dark();
    assert_eq!(computed_count(Some("rust"), TASK1_SRC, &theme), None);

    let first = highlight(Some("rust"), TASK1_SRC, &theme);
    assert_eq!(computed_count(Some("rust"), TASK1_SRC, &theme), Some(1));

    let second = highlight(Some("rust"), TASK1_SRC, &theme);
    assert_eq!(computed_count(Some("rust"), TASK1_SRC, &theme), Some(1));
    assert_eq!(first, second);
}

/// The cache is keyed on the theme's code styles, so a second theme is a miss
/// rather than a wrong-coloured hit.
#[test]
fn a_different_theme_recomputes_rather_than_reusing_the_colours() {
    const SRC: &str = "let task1_theme_probe = 2;\n";
    let dark = Theme::default_dark();
    let light = Theme::default_light();

    let in_dark = highlight(Some("rust"), SRC, &dark);
    let in_light = highlight(Some("rust"), SRC, &light);

    assert_eq!(in_dark.len(), in_light.len());
    assert_ne!(in_dark, in_light, "the two themes colour code differently");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mdmost -j 4 highlight::tests::a_second_highlight 2>&1 | tail -20`

Expected: FAIL — `cannot find function computed_count in this scope`.

- [ ] **Step 3: Write the cache**

In `src/highlight.rs`, add to the `use` block:

```rust
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use crate::theme::CodeStyles;
```

Add the cache, below `EXTRA_SYNTAX_SET`:

```rust
/// How many highlighted blocks are kept. Reached only by a document with more
/// distinct code blocks than this; the cache is then cleared whole rather than
/// evicting one entry, because a document is opened as a unit and its blocks
/// are wanted as a unit.
const MAX_CACHE_ENTRIES: usize = 256;

/// One cached block.
struct Entry {
    lines: Vec<Line>,
    /// How many times the highlighter has run for this key. One, unless
    /// something recomputes an entry it already had — which is the defect this
    /// cache exists to stop, and what `computed_count` lets a test assert.
    computed: u64,
}

/// The memo behind [`highlight`].
///
/// `highlight` is a pure function of `(lang, src, theme)` — no width reaches it,
/// which is why a block laid out at five widths during the clip search
/// (`render::document::render_widened`) produced five identical results and paid
/// for each. The key is therefore the whole of that triple.
///
/// The theme is held once for the whole map rather than per entry: a theme change
/// invalidates every entry at the same moment, and a reader changes theme far less
/// often than a document is laid out.
struct Cache {
    styles: Option<CodeStyles>,
    entries: HashMap<(Option<String>, String), Entry>,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| {
    Mutex::new(Cache {
        styles: None,
        entries: HashMap::new(),
    })
});

/// Runs `body` against the locked cache.
///
/// A panic elsewhere must not take highlighting with it: a poisoned lock is
/// recovered rather than propagated, because the worst a torn cache can hold is
/// lines that have to be computed again.
fn with_cache<T>(body: impl FnOnce(&mut Cache) -> T) -> T {
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    body(&mut cache)
}

/// The cached lines for this key, if the cache holds them for this theme.
fn cache_get(lang: Option<&str>, src: &str, theme: &Theme) -> Option<Vec<Line>> {
    with_cache(|cache| {
        if cache.styles != Some(theme.code) {
            return None;
        }
        let key = (lang.map(str::to_owned), src.to_owned());
        cache.entries.get(&key).map(|entry| entry.lines.clone())
    })
}

/// Records `lines` for this key, dropping everything cached for another theme.
fn cache_put(lang: Option<&str>, src: &str, theme: &Theme, lines: &[Line]) {
    with_cache(|cache| {
        if cache.styles != Some(theme.code) {
            cache.entries.clear();
            cache.styles = Some(theme.code);
        }
        if cache.entries.len() >= MAX_CACHE_ENTRIES {
            cache.entries.clear();
        }
        let key = (lang.map(str::to_owned), src.to_owned());
        cache
            .entries
            .entry(key)
            .and_modify(|entry| {
                entry.lines = lines.to_vec();
                entry.computed += 1;
            })
            .or_insert_with(|| Entry {
                lines: lines.to_vec(),
                computed: 1,
            });
    })
}

/// How many times the highlighter has run for this key, or `None` if the cache
/// does not hold it.
///
/// Exists for the tests that assert a block is highlighted once however many
/// widths it is laid out at.
pub(crate) fn computed_count(lang: Option<&str>, src: &str, theme: &Theme) -> Option<u64> {
    with_cache(|cache| {
        if cache.styles != Some(theme.code) {
            return None;
        }
        let key = (lang.map(str::to_owned), src.to_owned());
        cache.entries.get(&key).map(|entry| entry.computed)
    })
}
```

- [ ] **Step 4: Route `highlight` through the cache**

Rename the existing `pub fn highlight` body to a private `highlight_uncached` with the same parameters and return type, and add the public wrapper in its place. The lock is not held across the computation: a highlight can take seconds, and Task 4 runs it on another thread.

```rust
pub fn highlight(lang: Option<&str>, src: &str, theme: &Theme) -> Vec<Line> {
    if let Some(hit) = cache_get(lang, src, theme) {
        return hit;
    }
    let lines = highlight_uncached(lang, src, theme);
    cache_put(lang, src, theme, &lines);
    lines
}

/// Highlights without consulting the cache. See [`highlight`] for the contract.
fn highlight_uncached(lang: Option<&str>, src: &str, theme: &Theme) -> Vec<Line> {
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
```

Move the doc comment that is currently on `highlight` so it stays on the public `highlight`, and add one line to it recording that repeated calls with the same arguments are served from a memo.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p mdmost -j 4 highlight 2>&1 | tail -20`

Expected: PASS, including the two new tests and every existing test in `src/highlight/tests.rs`.

- [ ] **Step 6: Add the render-level test that proves the defect is gone**

Append to `src/render/tests.rs`. It asserts that the clip search lays this block out several times but highlights it once. At width 40 with a cap of 20 the fence clips, so `render_widened` runs its full probe ladder.

```rust
/// A fence wider than the layout sends `render_widened` through its probe
/// ladder; every probe used to re-enter the highlighter with identical
/// arguments. See the plan's finding F1.
#[test]
fn a_clipping_code_block_is_highlighted_once_however_often_it_is_laid_out() {
    const CODE: &str =
        "let probe_for_the_clip_search = \"a line far wider than any prose cap set here\";\n";
    let source = format!("Text.\n\n```rust\n{CODE}```\n");
    let doc = Doc::parse(&source);
    let theme = Theme::default_dark();

    let _ = render_document(&doc, 40, Some(20), &theme, &PLAIN);

    assert_eq!(
        crate::highlight::computed_count(Some("rust"), CODE, &theme),
        Some(1),
        "the clip search must not recompute the highlight"
    );
}
```

- [ ] **Step 7: Run it, and prove it is measuring the defect**

Run: `cargo test -p mdmost -j 4 a_clipping_code_block 2>&1 | tail -20`

Expected: PASS.

The test cannot be red on the parent commit, because `computed_count` does not exist there. Demonstrate the defect instead: temporarily make `cache_get` return `None` unconditionally, re-run, and record the count the assertion reports — it should be well above one, which is the number of times the clip search was re-highlighting. Restore `cache_get`, re-run, confirm `Some(1)`, and put both numbers in the task report.

- [ ] **Step 8: Verify the end-to-end cost**

Run, from the repository root:

```bash
cargo build --release -p mdmost -j 4
printf 'x\n' > /dev/null
/scratch/oetiker/cargo-target/release/mdmost --render-once --width 100 \
  docs/superpowers/plans/2026-09-21-highlight-cost-and-container-wrapping.md > /dev/null
```

Then time the file named in F1 if it is still present, or any document with several wide fences. Record the before and after timings in the task report. A document that took several seconds should now take roughly the cost of one highlight pass.

- [ ] **Step 9: Run the full gates**

Run: `cargo fmt --check -p mdmost && cargo clippy --all-targets -p mdmost -j 4 -- -D warnings && cargo test -p mdmost -j 4 2>&1 | tail -20`

Expected: all three clean. `cargo test` reports the same count as before plus the three new tests.

- [ ] **Step 10: Commit**

```bash
git add src/highlight.rs src/highlight/tests.rs src/render/tests.rs
git commit -m "perf: memoise highlight so the clip search stops recomputing it"
```

---

### Task 2: Extract the placement helper and carry the prose cap in `Ctx`

Behaviour-preserving refactor. Nothing a reader sees may change; the whole point is that the existing suite stays green while the escalation logic becomes reachable from inside a container.

**Files:**
- Modify: `src/render/mod.rs` (add a field to `Ctx`)
- Modify: `src/render/document.rs` (extract the helper, set the new field)
- Test: `src/render/tests.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `pub(crate) fn place(node: &Node, measure: Measure, ctx: Ctx<'_>, clip: &ClipTest, fill: Style) -> Canvas` — the current body of `render_placed`, callable from `block.rs`. And `Ctx.measure: Option<Measure>`, the prose cap in force for the children of the node being rendered, `None` when no cap applies. Task 3 consumes both.

- [ ] **Step 1: Write the characterisation test**

Before changing anything, pin the current top-level behaviour so the refactor cannot silently alter it. Append to `src/render/tests.rs`:

```rust
/// A top-level fence wider than the cap takes the full body width. This is the
/// behaviour `render_placed` has today and the refactor in Task 2 must preserve
/// it exactly; Task 3 changes only what happens inside a container.
#[test]
fn a_top_level_wide_fence_still_takes_the_full_body() {
    let source = "Short.\n\n```sh\n\
        some --command --with --a --line --clearly --wider --than --the --prose --cap\n\
        ```\n";
    let doc = Doc::parse(source);
    let theme = Theme::default_dark();
    let canvas = render_document(&doc, 120, Some(72), &theme, &PLAIN);

    let widest = (0..canvas.height())
        .map(|row| canvas.row_text(row).trim_end().chars().count())
        .max()
        .unwrap_or(0);
    assert!(
        widest > 72,
        "a wide top-level fence is granted more than the prose cap, got {widest}"
    );
}
```

Match the helper names to the ones `src/render/tests.rs` already uses for building a document and reading a row; read the file first and reuse them rather than introducing new ones.

- [ ] **Step 2: Run it to confirm it passes today**

Run: `cargo test -p mdmost -j 4 a_top_level_wide_fence 2>&1 | tail -10`

Expected: PASS. It is a characterisation test — it must pass before and after.

- [ ] **Step 3: Make `Measure` and the placement reachable**

In `src/render/document.rs`, change `struct Measure` to `pub(crate) struct Measure` and give it `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`; make `new` and `is_capped` `pub(crate)`. Rename `render_placed` to `pub(crate) fn place` and **change its `doc: DocCtx<'_>` parameter to `ctx: Ctx<'_>`**, doing the same to `render_widened`. This is the load-bearing part of the task, so do not skip the reasoning: `DocCtx::ctx()` (`src/render/document.rs:83`) calls `Ctx::new(...)`, which builds a *fresh top-level* context. Called from inside a container that would discard the quote's base style, the list depth and the quote depth, and a quoted paragraph would be drawn as if it were at the top level. Passing the `Ctx` already in force is what makes the function safe to call at depth.

Everything the two functions take from `DocCtx` is reachable from a `Ctx`: `doc.theme` becomes `ctx.theme`, `doc.options` becomes `ctx.options`, and both `doc.ctx()` call sites become `ctx` itself. `Ctx` is `Copy` (`src/render/mod.rs:187`), so this costs no borrow juggling.

At the two call sites in `render_document`, pass `doc.ctx()` where `doc` was passed. That is exactly what the functions built for themselves before, so top-level behaviour is unchanged — which the characterisation test in Step 1 is there to prove.

Leave `is_exempt` private: it answers a question about a top-level block's kind and Task 3 does not need it.

- [ ] **Step 4: Add the field to `Ctx`**

In `src/render/mod.rs`, add to `pub(crate) struct Ctx<'a>`:

```rust
    /// The prose cap in force for the blocks being laid out, if any.
    ///
    /// `None` for a render with no cap and for a fragment rendered on its own —
    /// a table cell, a footnote popup — where there is no body to cap against.
    /// A container narrows this by its own gutter before handing it to its
    /// children, so a list item's cap is the body's cap less the marker column.
    pub measure: Option<crate::render::document::Measure>,
```

Set it to `None` in every existing `Ctx` constructor and in every struct literal that builds a `Ctx`, so this task changes no behaviour. Find them with:

```bash
grep -rn "Ctx {" src/ | grep -v "^src/render/mod.rs:.*struct"
```

If `Ctx` is built through a constructor or a `..Default::default()` tail, add the field there once instead.

- [ ] **Step 5: Set the field in `render_document`**

In `render_document`, after `let measure = Measure::new(full, body_width);`, make the `Ctx` handed to blocks carry `Some(measure)`. Do not yet read it anywhere — Task 3 does that. Confirm the suite is still green, which is what proves the field is inert.

- [ ] **Step 6: Run the full gates**

Run: `cargo fmt --check -p mdmost && cargo clippy --all-targets -p mdmost -j 4 -- -D warnings && cargo test -p mdmost -j 4 2>&1 | tail -20`

Expected: all clean, with the same test count as Task 1 plus the one added here. Any test whose output changed means the refactor was not behaviour-preserving — fix that before committing rather than updating the expectation.

- [ ] **Step 7: Commit**

```bash
git add src/render/mod.rs src/render/document.rs src/render/tests.rs
git commit -m "refactor: make block placement callable from inside a container"
```

---

### Task 3: Escalate the child, not the container

**Files:**
- Modify: `src/render/block.rs` (`render_sequence`, and the two container renderers that call it)
- Test: `src/render/tests.rs`

**Interfaces:**
- Consumes: `document::place`, `document::Measure` and `Ctx.measure` from Task 2.
- Produces: nothing later tasks rely on.

- [ ] **Step 1: Write the failing tests**

Append to `src/render/tests.rs`. These encode finding F2 directly.

```rust
/// The widest row of a canvas, ignoring trailing blanks.
fn widest_row(canvas: &Canvas) -> usize {
    (0..canvas.height())
        .map(|row| canvas.row_text(row).trim_end().chars().count())
        .max()
        .unwrap_or(0)
}

/// The widest row that is not part of a code frame, which is how the tests
/// below ask "where does the prose wrap" without matching on prose text.
fn widest_prose_row(canvas: &Canvas) -> usize {
    (0..canvas.height())
        .map(|row| canvas.row_text(row))
        .filter(|text| !text.contains('\u{2502}') && !text.contains('\u{256d}') && !text.contains('\u{2570}'))
        .map(|text| text.trim_end().chars().count())
        .max()
        .unwrap_or(0)
}

/// A wide fence inside a list item used to re-lay the whole list at the body
/// width, so every item's prose wrapped at the terminal width instead of the
/// cap. See the plan's finding F2.
#[test]
fn a_wide_fence_in_a_list_does_not_widen_the_list_prose() {
    let source = "Intro.\n\n\
        1. A first item, long enough to wrap so we can see which column it wraps at in practice here.\n\n   \
        ```sh\n   some --command --with --a --line --that --is --clearly --wider --than --the --cap\n   ```\n\n\
        2. A second item, long enough to wrap so we can see which column it wraps at in practice here.\n";
    let doc = Doc::parse(source);
    let theme = Theme::default_dark();
    let canvas = render_document(&doc, 120, Some(72), &theme, &PLAIN);

    assert!(
        widest_prose_row(&canvas) <= 72,
        "list prose must keep the prose cap, got {}",
        widest_prose_row(&canvas)
    );
    assert!(
        widest_row(&canvas) > 72,
        "the fence itself must still be granted the room it needs"
    );
}

/// The same defect, one container over.
#[test]
fn a_wide_fence_in_a_quote_does_not_widen_the_quoted_prose() {
    let source = "Intro.\n\n\
        > A quoted sentence, long enough to wrap so we can see which column the quote wraps at here.\n>\n\
        > ```sh\n> some --command --with --a --line --that --is --clearly --wider --than --the --cap\n> ```\n";
    let doc = Doc::parse(source);
    let theme = Theme::default_dark();
    let canvas = render_document(&doc, 120, Some(72), &theme, &PLAIN);

    assert!(
        widest_prose_row(&canvas) <= 72,
        "quoted prose must keep the prose cap, got {}",
        widest_prose_row(&canvas)
    );
}

/// A narrow fence changes nothing: the container was never escalating for its
/// own sake, and must not start.
#[test]
fn a_narrow_fence_in_a_list_leaves_everything_at_the_cap() {
    let source = "Intro.\n\n\
        1. A first item, long enough to wrap so we can see which column it wraps at in practice here.\n\n   \
        ```sh\n   echo hi\n   ```\n";
    let doc = Doc::parse(source);
    let theme = Theme::default_dark();
    let canvas = render_document(&doc, 120, Some(72), &theme, &PLAIN);

    assert!(
        widest_row(&canvas) <= 72,
        "nothing here wants more than the cap, got {}",
        widest_row(&canvas)
    );
}
```

Read the top of `src/render/tests.rs` first and reuse its existing imports and its existing document-building helper. If it already has a `widest_row` equivalent, use that instead of adding one.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mdmost -j 4 a_wide_fence_in_a 2>&1 | tail -30`

Expected: both `a_wide_fence_in_a_list_does_not_widen_the_list_prose` and `a_wide_fence_in_a_quote_does_not_widen_the_quoted_prose` FAIL, reporting a prose width of about 116. `a_narrow_fence_in_a_list_leaves_everything_at_the_cap` PASSES already.

- [ ] **Step 3: Apply placement to each child in `render_sequence`**

In `src/render/block.rs`, `render_sequence` currently renders each child with `render_block_ctx(child, width, ctx)`. Give it the same two-stage treatment `render_document` gives a top-level block, but only when a cap is in force:

```rust
    let part = match ctx.measure {
        // A capped sequence places each child the way the document places a
        // top-level block: laid out at the prose width, and granted the full
        // width only if the cap would cut it short. Without this a container is
        // escalated whole, and one over-wide fence drags every sentence in the
        // list or quote out to the terminal's edge.
        Some(measure) if measure.is_capped() => {
            crate::render::document::place(child, measure, ctx, &clip, fill)
        }
        _ => render_block_ctx(child, width, ctx),
    };
```

`render_sequence` has `ctx` already. The other two it must build itself, once, before the loop: `fill` is `ctx.base`, and `clip` is `ClipTest::new(ctx.theme)` — `ClipTest::new` (`src/render/document.rs:580`) takes only a `&Theme`, so make it and the struct `pub(crate)` in Task 2 alongside `place`.

- [ ] **Step 4: Narrow the cap for each container's gutter**

A container's children are laid out inside its gutter, so the cap they inherit must shrink by the same amount or their prose will run `gutter` columns past the body's cap.

In `quote()` (`src/render/block.rs:438`), the children are rendered at `width - gutter`; narrow `inner.measure` by `gutter` before the `render_sequence` call. In `list()`, do the same with the marker width the items are indented by. Add a `Measure::narrowed(self, by: u16) -> Measure` to `src/render/document.rs` that subtracts from both `full` and `prose`, saturating at 1, and use it in both places.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p mdmost -j 4 2>&1 | tail -30`

Expected: the three tests from Step 1 pass, `a_top_level_wide_fence_still_takes_the_full_body` from Task 2 still passes, and the whole suite is green. Existing layout tests that now report a different width are the interesting case: decide per test whether the old expectation encoded this defect. If it did, update it and say which test and why in the report; if it did not, the change is wrong.

- [ ] **Step 6: Check the real document by eye**

Run the binary against the reproducer in F2 at `--width 120` and confirm the paragraphs and both list items wrap at 72 while the fence is granted its width. Paste the output into the task report.

- [ ] **Step 7: Run the full gates**

Run: `cargo fmt --check -p mdmost && cargo clippy --all-targets -p mdmost -j 4 -- -D warnings && cargo test -p mdmost -j 4 2>&1 | tail -20`

- [ ] **Step 8: Commit**

```bash
git add src/render/block.rs src/render/document.rs src/render/tests.rs
git commit -m "fix: a wide block in a list or quote no longer widens its container's prose"
```

---

### Task 4: Survive a syntax definition that does not return

**Files:**
- Modify: `src/highlight.rs` (deadline guard around the uncached path; `highlight_with` to take `&CodeStyles`)
- Test: `src/highlight/tests.rs`
- Create: `docs/upstream/2026-09-21-javascript-syntax-hang.md`

**Interfaces:**
- Consumes: `highlight_uncached` and the cache from Task 1.
- Produces: nothing later tasks rely on.

Design, with the reasoning the implementer must not re-derive: the hang is inside one `ParseState::parse_line` call (finding F3), so no check placed between lines can reach it. Rust cannot cancel a thread, so the only defence is to run the work somewhere the caller can walk away from, and accept that the abandoned thread spins until the process exits. The guard therefore caps how many threads may be abandoned; past that, uncached blocks degrade to plain text rather than costing another core.

The budget scales with the input so that a legitimately large block is not degraded for being large: a 1077-line TypeScript block takes about 1.4 s of honest work, while the F3 reproducer is two lines and never finishes.

- [ ] **Step 1: Write the failing tests**

Append to `src/highlight/tests.rs`:

```rust
use std::time::Duration;

/// The budget must leave an honest block alone and still be finite.
#[test]
fn the_budget_grows_with_the_block() {
    assert!(budget_for(2) >= Duration::from_millis(100));
    assert!(budget_for(2) <= Duration::from_millis(500));
    assert!(budget_for(1077) >= Duration::from_secs(2));
    assert!(budget_for(1_000_000) <= MAX_HIGHLIGHT_BUDGET);
}

/// A parse that does not return within its budget degrades to plain text
/// rather than hanging the caller.
#[test]
fn work_that_overruns_its_budget_is_abandoned() {
    let outcome = run_within(Duration::from_millis(50), || {
        std::thread::sleep(Duration::from_secs(30));
        vec![Line::empty()]
    });
    assert!(outcome.is_none(), "the caller must not wait for it");
}

/// Work that finishes inside its budget is returned unchanged.
#[test]
fn work_that_finishes_in_time_is_returned() {
    let outcome = run_within(Duration::from_secs(5), || vec![Line::empty(), Line::empty()]);
    assert_eq!(outcome.map(|lines| lines.len()), Some(2));
}
```

And, marked ignored because it abandons a thread that then spins for the lifetime of the test binary:

```rust
/// The reproducer from the plan's finding F3. Ignored by default: the abandoned
/// parser thread burns a core until the process exits, which is not something to
/// impose on every `cargo test`.
///
/// Run with: cargo test -p mdmost -j 4 -- --ignored the_javascript_hang
#[test]
#[ignore = "abandons a spinning thread; see the doc comment"]
fn the_javascript_hang_degrades_to_plain_text() {
    let theme = Theme::default_dark();
    let src = "  | { type: \"a\" }\n  /** x */\n";
    let lines = highlight(Some("js"), src, &theme);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text(), "  | { type: \"a\" }");
    assert_eq!(lines[1].text(), "  /** x */");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mdmost -j 4 the_budget_grows 2>&1 | tail -20`

Expected: FAIL — `cannot find function budget_for in this scope`.

- [ ] **Step 3: Let the highlighter run without a `Theme`**

`highlight_with` and `plain` take `&Theme` but use only `theme.code`. Change both to take `&CodeStyles`, and update their call sites in `highlight_uncached`. `CodeStyles` is `Copy`, so the guarded closure in Step 4 can own one; `Theme` cannot cross the thread boundary as a borrow.

Keep `plain_style(theme: &Theme)` as it is: it is public API used by callers that hold a `Theme`.

- [ ] **Step 4: Add the guard**

In `src/highlight.rs`:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

/// The longest any one block may be highlighted for, however large it is.
pub const MAX_HIGHLIGHT_BUDGET: Duration = Duration::from_secs(20);

/// How long a block of `lines` lines is given before it is abandoned.
///
/// Generous against honest work and still finite: a thousand lines of
/// TypeScript cost about 1.4 s, while a block that has hit a pathological rule
/// in a syntax definition does not finish at all.
fn budget_for(lines: usize) -> Duration {
    let scaled = Duration::from_millis(200) + Duration::from_millis(2) * lines as u32;
    scaled.min(MAX_HIGHLIGHT_BUDGET)
}

/// How many abandoned threads are tolerated before the guard stops spawning.
///
/// An abandoned thread cannot be stopped — Rust has no way to cancel one — so
/// each costs a core until the process exits. Past this, an uncached block is
/// plain text rather than another core.
const MAX_ABANDONED: usize = 2;

static ABANDONED: AtomicUsize = AtomicUsize::new(0);

/// Runs `work` on a thread and gives up on it after `budget`.
///
/// `None` means the work did not finish in time, or that too many threads have
/// already been abandoned to risk another.
fn run_within(
    budget: Duration,
    work: impl FnOnce() -> Vec<Line> + Send + 'static,
) -> Option<Vec<Line>> {
    if ABANDONED.load(Ordering::Relaxed) >= MAX_ABANDONED {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    // The result is sent rather than joined: a join would wait for a thread
    // that may never return, which is the whole thing being defended against.
    // The send fails harmlessly when this side has already given up.
    std::thread::Builder::new()
        .name("mdmost-highlight".to_owned())
        .spawn(move || {
            let _ = tx.send(work());
        })
        .ok()?;
    match rx.recv_timeout(budget) {
        Ok(lines) => Some(lines),
        Err(_) => {
            ABANDONED.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}
```

Then route the uncached path through it, inside `highlight`:

```rust
pub fn highlight(lang: Option<&str>, src: &str, theme: &Theme) -> Vec<Line> {
    if let Some(hit) = cache_get(lang, src, theme) {
        return hit;
    }
    let styles = theme.code;
    let owned_lang = lang.map(str::to_owned);
    let owned_src = src.to_owned();
    let line_count = LinesWithEndings::from(src).count();
    let lines = run_within(budget_for(line_count), move || {
        highlight_uncached(owned_lang.as_deref(), &owned_src, &styles)
    })
    // A block that overran is cached as plain text, so it is neither
    // re-attempted on the next layout probe nor on the next resize.
    .unwrap_or_else(|| plain(src, &theme.code));
    cache_put(lang, src, theme, &lines);
    lines
}
```

`highlight_uncached` takes `&CodeStyles` after Step 3; adjust its signature accordingly.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p mdmost -j 4 highlight 2>&1 | tail -20`

Expected: PASS, with `the_javascript_hang_degrades_to_plain_text` reported as ignored.

- [ ] **Step 6: Run the ignored test once, deliberately**

Run: `cargo test -p mdmost -j 4 -- --ignored the_javascript_hang 2>&1 | tail -10`

Expected: PASS within a second or so. Paste the output into the task report. This is the only direct evidence that the guard closes F3.

- [ ] **Step 7: Confirm the cost did not move**

Re-run the timing from Task 1 Step 8. The guard adds one thread spawn per cache miss, which must not be visible. If a document got measurably slower, say so in the report rather than proceeding.

- [ ] **Step 8: Write the upstream report**

Create `docs/upstream/2026-09-21-javascript-syntax-hang.md` containing: the two-line reproducer, the exact versions (`syntect` 5.3.0 with `default-fancy`, `two-face` 0.5.2+bat-0.26.1), the observation that `ParseState::parse_line` does not return for the second line, the evidence that the `oniguruma` backend hangs on the same syntax when it is selected by name — so the fault is the syntax definition and not the regex engine — and the note that `two-face`'s token lookup resolves `js` to `JavaScript` under `fancy` and to `JavaScript (Babel)` under `onig`. State plainly what was measured and what was not; do not speculate about which rule in the definition is responsible without having found it.

Do not file anything anywhere. The file is the deliverable; the repository owner decides where it goes.

- [ ] **Step 9: Run the full gates**

Run: `cargo fmt --check -p mdmost && cargo clippy --all-targets -p mdmost -j 4 -- -D warnings && cargo test -p mdmost -j 4 2>&1 | tail -20`

- [ ] **Step 10: Commit**

```bash
git add src/highlight.rs src/highlight/tests.rs docs/upstream/2026-09-21-javascript-syntax-hang.md
git commit -m "fix: abandon a highlight that does not return, and report it upstream"
```

---

## Notes for the executor

- Task 1 is worth doing first whatever else changes: it makes every later test run faster and it is the largest single win.
- Tasks 2 and 3 are one change split at the point where behaviour starts to differ. If Task 2's suite is not green, do not start Task 3 — a refactor that altered layout has hidden the thing Task 3 is meant to fix.
- Task 4 touches the same function Task 1 does. Do not run them concurrently.
- `CHANGES.md` is maintained in this repository. Add an entry for each user-visible change — the wrapping fix and the hang — under whatever heading the file uses for unreleased work. The memo is a performance change and belongs there too.
