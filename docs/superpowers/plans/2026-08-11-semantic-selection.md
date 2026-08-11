# Semantic Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a selection a range over the document rather than a rectangle on screen, give every diagram label a mapping back to its source, and give all three block kinds a muted three-state `[copy]` button.

**Architecture:** One hull of source bytes feeds both the clipboard and the highlight, so they cannot disagree. Chrome carries no `SearchSpan`s and therefore never highlights, without a special case. Diagram labels gain source ranges through the single shared `Label` type, and each family emits a span where it draws one; the bridge rebases mermaid-local offsets onto document offsets the way `render::code` already does for fenced lines. The button's position is decided at render time (rendering stays pure); its three appearances are decided at paint time from pointer and flash state.

**Tech Stack:** Rust 1.96, ratatui, crossterm, comrak. No new dependencies.

**Design authority:** `docs/superpowers/specs/2026-08-11-semantic-selection-design.md`. Where this plan and that spec disagree, **the spec wins** — say so in the commit message.

## Global Constraints

- **Every `cargo` command runs in the FOREGROUND**, never backgrounded, never behind a monitor or `until` loop. Seven agents on this project have deadlocked by ending a turn with a build pending.
- **Never read a gate's result through a pipe** — `cargo test | tail` reports `tail`'s exit status.
- **`--jobs 4` on every cargo invocation.** 128-core machine, heavily shared.
- **The clippy gate is `cargo clippy --jobs 4 --all-targets -- -D warnings`.** Plain `cargo clippy` exits 0 on warnings.
- **Baseline: 1020 tests across 30 suites**, with `cargo fmt --check`, clippy, `cargo test` and `cargo check --jobs 4 --target x86_64-pc-windows-msvc` all exit 0.
- **Fault injection is mandatory.** For each new test: revert the mechanism, confirm *that* test fails, restore, report the mutation. A test that goes *skipped* rather than red under mutation is vacuous in disguise.
- **Measure box art in columns, not bytes** — every box-drawing glyph is 3 bytes and 1 column. `perl -CSD`, or count with `.chars()`.
- **Rendering is a pure function of `(AST, width, theme, options)`.** Hover must never reach into it.
- **`render` must not depend on `tui`.** `#![forbid(unsafe_code)]`. `Esc` never quits. No centring anywhere. The status bar never lies.
- **Do not push. There is no git remote.**
- Commit messages via quoted heredoc (`git commit -F - <<'EOF'`), never `-m "…"` — backticks get command-substituted.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `src/tui/select.rs` | Resolve cells to source offsets; the hull; which cells a hull highlights. Modified throughout. |
| `src/tui/draw.rs` | Paint the highlight from the hull; paint the button's three states. Modified. |
| `src/tui/app.rs` | Hovered-hotspot state. Modified. |
| `src/tui/term.rs` | The `Moved` arm. Modified. |
| `src/mermaid/ast.rs` | `Label` carries a source range. Modified. |
| `src/mermaid/parse*.rs` | Parse sites pass the offset. Modified. |
| `src/mermaid/layout/*`, `src/mermaid/gantt/` | Each family emits a label span. Modified. |
| `src/render/diagram.rs`, `src/render/bridge.rs` | Rebase mermaid-local offsets onto document offsets; place the floating button. Modified. |
| `src/theme/*` | The muted button token. Modified. |

---

### Task 1: A cell resolves to a source offset

**Files:**
- Modify: `src/tui/select.rs`
- Test: `src/tui/tests.rs`

**Interfaces:**
- Produces: `fn offset_at(canvas: &Canvas, source: &str, pos: Pos, bias: Bias) -> Option<usize>` and `pub enum Bias { Start, End }`, both private to `select` except as tests need them.

`Bias` says which way an endpoint that lands on chrome resolves: `Start` for the anchor end of the range, `End` for the far end. Spec §2.1.

- [ ] **Step 1: Write the failing tests**

Add to `src/tui/tests.rs`. `table_doc()` is a fixture you write beside them: a two-column table whose first cell says `alpha` and whose second says `beta`.

```rust
#[test]
fn a_cell_on_text_resolves_to_that_byte() {
    let doc = "| alpha | beta |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| &doc[s.source_start..s.source_end] == "alpha")
        .expect("a span for alpha");
    let at = select::offset_at(&canvas, doc, Pos { row: span.row, col: span.col }, select::Bias::Start);
    assert_eq!(at, Some(span.source_start));
}

#[test]
fn a_cell_on_a_border_resolves_to_the_next_text_in_document_order() {
    // Column 1 of a table row is the left vertical rule: chrome, with no span.
    let doc = "| alpha | beta |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| &doc[s.source_start..s.source_end] == "alpha")
        .expect("a span for alpha");
    let on_rule = Pos { row: span.row, col: 1 };
    assert!(
        canvas.spans().iter().all(|s| s.row != span.row || s.col > 1),
        "column 1 must really be chrome for this test to mean anything"
    );
    assert_eq!(
        select::offset_at(&canvas, doc, on_rule, select::Bias::Start),
        Some(span.source_start),
        "a press on the rule takes the start of the cell's text"
    );
}

#[test]
fn a_cell_past_the_end_of_a_row_resolves_to_the_last_span_on_it() {
    let doc = "| alpha | beta |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| &doc[s.source_start..s.source_end] == "beta")
        .expect("a span for beta");
    let past = Pos { row: span.row, col: 200 };
    assert_eq!(
        select::offset_at(&canvas, doc, past, select::Bias::End),
        Some(span.source_end)
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib resolves_to`
Expected: FAIL — `offset_at` does not exist.

- [ ] **Step 3: Implement `offset_at`**

In `src/tui/select.rs`. `byte_at_column` already exists in this file and maps a column offset within a span's text to a byte offset within it.

```rust
/// Which way an endpoint resolves when it lands on a cell no span covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bias {
    /// The near end of the range: take the start of the next text.
    Start,
    /// The far end: take the end of the previous text.
    End,
}

/// The source byte a cell points at.
///
/// A cell inside a span is exact. A cell on chrome — a border, the gutter, padding,
/// the blank tail of a row — has no span to ask, so it resolves to the nearest text in
/// document order in the direction `bias` names. This is the only coordinate in the
/// selection that is interpreted rather than looked up (design spec §2.1).
pub(crate) fn offset_at(canvas: &Canvas, source: &str, pos: Pos, bias: Bias) -> Option<usize> {
    // Exact hit first: the cell is inside some span's drawn columns.
    for span in canvas.spans() {
        let end = span.col.saturating_add(span.cols);
        if span.row == pos.row && pos.col >= span.col && pos.col < end {
            let body = source
                .get(span.source_start..span.source_end)
                .unwrap_or_default();
            return Some(span.source_start + byte_at_column(body, pos.col - span.col));
        }
    }
    // Chrome. Search in READING ORDER — (row, col) across the whole canvas, not just
    // this row — because "document order" is the whole point and a drag inside a
    // diagram's blank interior has no span on its own rows at all.
    let key = (pos.row, pos.col);
    match bias {
        // The near end takes the first text at or after the cell.
        Bias::Start => canvas
            .spans()
            .iter()
            .filter(|s| (s.row, s.col) >= key)
            .min_by_key(|s| (s.row, s.col))
            .map(|s| s.source_start)
            .or(Some(source.len())),
        // The far end takes the last text at or before it.
        Bias::End => canvas
            .spans()
            .iter()
            .filter(|s| (s.row, s.col) <= key)
            .max_by_key(|s| (s.row, s.col))
            .map(|s| s.source_end)
            .or(Some(0)),
    }
}
```

**Read the fallbacks carefully — they are inverted on purpose, and getting them backwards
selects the whole document.** `Bias::Start` finding no text *after* the cell means the
drag began past all content, so it yields `source.len()`; `Bias::End` finding none
*before* it yields `0`. Either way `lo >= hi` and Task 2's hull comes back `None`, which
is exactly the spec's "a drag across only chrome selects nothing". The tempting values —
`0` for Start and `len()` for End — would make a drag inside a diagram select every byte
in the document.

Add this test to Step 1 to pin it:

```rust
#[test]
fn a_drag_entirely_on_chrome_selects_nothing() {
    // The interior of a diagram: box art, no labels under either endpoint.
    let doc = "```mermaid\nflowchart LR\n  A[Parse] --> B[Layout]\n```\n";
    let canvas = render(doc, 60);
    let lo = select::offset_at(&canvas, doc, Pos { row: 0, col: 0 }, select::Bias::Start);
    let hi = select::offset_at(&canvas, doc, Pos { row: 0, col: 1 }, select::Bias::End);
    assert!(
        lo >= hi,
        "an empty range, not the whole document: {lo:?}..{hi:?}"
    );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib resolves_to`
Expected: PASS — three tests.

- [ ] **Step 5: Fault-inject each test**

For each: remove the `pos.col >= span.col && pos.col < end` exact-hit branch, then the `pos.col < span.col` side selection, then the `(None, …)` clamp. Confirm the matching test fails each time and restore. Record the mutations and failures in the commit body.

- [ ] **Step 6: Gates and commit**

```
cargo fmt --check
cargo clippy --jobs 4 --all-targets -- -D warnings
cargo test --jobs 4
```

Expected: 1023 tests / 30 suites. Commit `src/tui/select.rs` and `src/tui/tests.rs`.

---

### Task 2: The hull is the range between two endpoints

**Files:**
- Modify: `src/tui/select.rs:210-235` — `source_hull`
- Test: `src/tui/tests.rs`

**Interfaces:**
- Consumes: `offset_at`, `Bias` (Task 1).
- Produces: `source_hull` with unchanged parameters, computed from endpoints rather than from geometry.

**Visibility:** `source_hull` is private today. The tests below live in `src/tui/tests.rs`,
a sibling module, which cannot reach a private item — so it becomes `pub(crate)`, as do
`offset_at` and `Bias` from Task 1. If you would rather keep them private, put the tests
in a `#[cfg(test)] mod tests` inside `select.rs` instead; either is fine, but decide once
and do not leave a test unable to compile.

This is the behaviour change. Today `source_hull` walks every span, asks `columns_on` whether the geometry touches it, and takes the min/max. That is why a mid-row drag selects by rectangle. From here it resolves two endpoints and takes the range between them, which is document order.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_drag_across_a_table_selects_whole_cells_in_document_order() {
    // Dragging from the second cell of row 1 to the first cell of row 2 selects the
    // text between them in the *source*, which is row-major — not the rectangle the
    // two corners describe on screen.
    let doc = "| a | b |\n| --- | --- |\n| one | two |\n| three | four |\n";
    let canvas = render(doc, 40);
    let two = span_for(&canvas, doc, "two");
    let three = span_for(&canvas, doc, "three");
    let sel = Selection::from_to(
        Pos { row: two.row, col: two.col },
        Pos { row: three.row, col: three.col },
    );
    let (lo, hi) = select::source_hull(&canvas, doc, sel).expect("a hull");
    let text = &doc[lo..hi];
    assert!(text.starts_with("two"), "got {text:?}");
    assert!(text.ends_with("three"), "got {text:?}");
    assert!(
        text.contains('\n'),
        "the hull crosses the source's row boundary: {text:?}"
    );
}
```

Write `span_for(&canvas, doc, needle)` as a helper beside it if one does not already exist, and `Selection::from_to` if `Selection` has no such constructor — check first; it has `started(anchor)` and a way to move the head.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --jobs 4 --lib selects_whole_cells`
Expected: FAIL — the geometric hull does not reach `three`, because the rectangle between those two cells excludes it.

- [ ] **Step 3: Rewrite `source_hull`**

```rust
/// The source range a selection covers.
///
/// Two endpoints, resolved to source offsets, and everything between them — which is
/// document order, not screen geometry. A wrapped table cell therefore continues into
/// the *next cell* rather than into whatever sits beside it on the same screen row, and
/// a drag whose corners describe a rectangle still selects what a reader would read
/// between them (design spec §2).
fn source_hull(canvas: &Canvas, source: &str, selection: Selection) -> Option<(usize, usize)> {
    let (start, end) = selection.ordered();
    let lo = offset_at(canvas, source, start, Bias::Start)?;
    let hi = offset_at(canvas, source, end, Bias::End)?;
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    (lo < hi).then_some((lo, hi))
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --jobs 4 --lib selects_whole_cells`
Expected: PASS.

- [ ] **Step 5: Run the whole suite and account for every move**

Run: `cargo test --jobs 4`

Existing selection tests written against rectangle semantics **will** change. For each one that fails, decide deliberately: is it asserting the old geometric behaviour (rewrite it to the new model, and say so) or has the new model broken something real (stop and report)? **Do not delete a failing test to make the suite green.** Record each rewritten test and why in the commit body.

- [ ] **Step 6: Fault-inject**

Replace `Bias::End` with `Bias::Start` at the `hi` site: the whole-cells test must fail. Restore.

- [ ] **Step 7: Gates and commit**

The three gates in the foreground. Commit with a body naming every rewritten test.

---

### Task 3: The highlight is painted from the hull

**Files:**
- Modify: `src/tui/select.rs` — add `highlighted_columns`; `columns_on` becomes unused and is deleted
- Modify: `src/tui/draw.rs:662-688` — `highlight_selection`
- Test: `src/render/tests.rs` or `src/tui/tests.rs` — assert on the canvas

**Interfaces:**
- Consumes: `source_hull` (Task 2).
- Produces: `pub(crate) fn highlighted_columns(canvas: &Canvas, source: &str, selection: Selection, row: usize) -> Vec<Range<u16>>` — the column ranges to wash on one row.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_table_border_is_never_highlighted() {
    let doc = "| a | b |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let one = span_for(&canvas, doc, "one");
    let two = span_for(&canvas, doc, "two");
    let sel = Selection::from_to(
        Pos { row: one.row, col: one.col },
        Pos { row: two.row, col: two.col + two.cols - 1 },
    );
    let ranges = select::highlighted_columns(&canvas, doc, sel, one.row);
    let row = canvas.row_text(one.row);
    for range in &ranges {
        for col in range.clone() {
            let ch = row.chars().nth(usize::from(col)).unwrap_or(' ');
            assert!(
                !"│├┤┬┴┼╭╮╰╯─".contains(ch),
                "chrome at column {col} is highlighted: {ch:?} in {row:?}"
            );
        }
    }
    assert!(!ranges.is_empty(), "the cells themselves are highlighted");
}

#[test]
fn the_highlight_stops_at_the_end_of_the_text() {
    let doc = "short line\n";
    let canvas = render(doc, 60);
    let span = span_for(&canvas, doc, "short line");
    let sel = Selection::from_to(
        Pos { row: span.row, col: span.col },
        Pos { row: span.row, col: 59 },
    );
    let ranges = select::highlighted_columns(&canvas, doc, sel, span.row);
    let last = ranges.iter().map(|r| r.end).max().expect("a range");
    assert_eq!(
        last,
        span.col + span.cols,
        "the wash must not run to the pane edge"
    );
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --jobs 4 --lib never_highlighted`
Expected: FAIL — `highlighted_columns` does not exist.

- [ ] **Step 3: Implement it**

```rust
/// The column ranges of `row` that a selection washes.
///
/// Every span the hull covers, clipped to the covered part. Chrome carries no spans, so
/// borders, the line-number gutter, cell padding and the blank tail of a row are not
/// in the answer and no rule had to say so.
pub(crate) fn highlighted_columns(
    canvas: &Canvas,
    source: &str,
    selection: Selection,
    row: usize,
) -> Vec<Range<u16>> {
    let Some((lo, hi)) = source_hull(canvas, source, selection) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for span in canvas.spans() {
        if span.row != row || span.source_end <= lo || span.source_start >= hi {
            continue;
        }
        let body = source
            .get(span.source_start..span.source_end)
            .unwrap_or_default();
        let from = column_at_byte(body, lo.saturating_sub(span.source_start));
        let to = if hi >= span.source_end {
            span.cols
        } else {
            column_at_byte(body, hi - span.source_start)
        };
        let (a, b) = (span.col + from, span.col + to);
        if a < b {
            out.push(a..b);
        }
    }
    out.sort_by_key(|r| r.start);
    out
}
```

`column_at_byte` is `byte_at_column`'s inverse and does not exist yet — write it in the same file, beside its partner, and give it its own unit test with a multi-byte character (`é`) and a wide one (`　`).

- [ ] **Step 4: Use it in `draw.rs`**

Replace the `columns_on` call in `highlight_selection` with a `highlighted_columns` call, iterating the returned ranges instead of one range. Everything else in that function — the `Offsets` translation, the `area.width` bound, `patch_term` — stays exactly as it is.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --jobs 4`
Expected: PASS. Delete `columns_on` and its tests once nothing calls it; the compiler will tell you.

- [ ] **Step 6: Fault-inject**

Remove the `span.source_end <= lo || span.source_start >= hi` guard so every span on the row is washed: `a_table_border_is_never_highlighted` must fail. Restore.

- [ ] **Step 7: Show the owner**

Render a document with a table, a code block and prose; make a selection across it; capture the pane in tmux and **show it**. The owner reviews by looking. Do not proceed to Task 4 until they have seen this.

- [ ] **Step 8: Gates and commit**

---

### Task 4: A `Label` knows where it came from

**Files:**
- Modify: `src/mermaid/ast.rs:35-60` — `Label`, `Label::parse`
- Modify: every parse site that builds a `Label` (find with `rg 'Label::parse'`)
- Test: `src/mermaid/parse/tests.rs` or wherever parser tests live — find with `rg 'mod tests' src/mermaid/parse*`

**Interfaces:**
- Produces: `Label { lines: Vec<String>, source: Range<usize> }` and `Label::parse_at(text: &str, at: usize) -> Self`. `Label::parse(text)` stays, as `parse_at(text, 0)`, so call sites that genuinely have no offset keep compiling.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_flowchart_node_label_knows_its_source_range() {
    let src = "flowchart LR\n  A[Parse] --> B[Layout]\n";
    let diagram = parse(src).expect("parses");
    let Diagram::Flowchart(chart) = diagram else {
        panic!("expected a flowchart");
    };
    let node = chart.nodes.values().find(|n| n.label.lines == ["Parse"]).expect("node A");
    assert_eq!(&src[node.label.source.clone()], "Parse");
}
```

Adjust `chart.nodes` access to whatever the real collection is — read `Flowchart` at `src/mermaid/ast.rs:159` first.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --jobs 4 --lib knows_its_source_range`
Expected: FAIL — no field `source`.

- [ ] **Step 3: Add the field**

```rust
pub struct Label {
    /// The label's lines, in order, without trailing newlines.
    pub lines: Vec<String>,
    /// Where the raw label text sat in the mermaid source, before `<br>` splitting and
    /// entity decoding.
    ///
    /// The range covers the text as written, so `A[Parse]` gives the range of `Parse`.
    /// It is relative to the mermaid block, not the document; `render::diagram` rebases
    /// it. An empty range means "synthesised, not from the source" — a state key used as
    /// a fallback label, for instance.
    pub source: std::ops::Range<usize>,
}
```

`Default` derives an empty range, which is the honest value for a synthesised label.

- [ ] **Step 4: Thread the offset through the parse sites**

`Label::parse_at(text, at)` where `at` is the offset of `text` within the mermaid source. Each parse site knows where it took `text` from — pass it. **Where a site genuinely synthesises a label rather than reading one (a state's key standing in for a missing description, `src/mermaid/layout/state.rs:310`), leave the range empty and add a one-line comment saying why.**

- [ ] **Step 5: Run the tests**

Run: `cargo test --jobs 4`
Expected: PASS, count unchanged apart from your new test.

- [ ] **Step 6: Fault-inject**

Make `parse_at` ignore `at` and always pass 0: the new test must fail with a range pointing at `flowchart LR`. Restore.

- [ ] **Step 7: Gates and commit**

---

### Task 5: A flowchart's labels reach the document — the rebasing

**Files:**
- Modify: `src/mermaid/layout/flowchart.rs` (and `graph.rs` if the label draw is shared)
- Modify: `src/render/diagram.rs` — rebase mermaid-local offsets onto document offsets
- Test: `src/render/tests.rs`

**Interfaces:**
- Consumes: `Label::source` (Task 4), `Canvas::add_span`.
- Produces: spans on the diagram canvas whose `source_start`/`source_end` are **document** offsets.

**This is the highest-risk task in the plan.** The arithmetic is the same shape as `render::code`'s, and this project has already been bitten twice in that neighbourhood: provenance was silently lost on every CRLF document, and a byte end measured against a tab-expanded line ran past the end of its own line.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_flowchart_label_maps_back_to_the_document() {
    let doc = "# Chart\n\n```mermaid\nflowchart LR\n  A[Parse] --> B[Layout]\n```\n";
    let canvas = render(doc, 60);
    let span = canvas
        .spans()
        .iter()
        .find(|s| doc.get(s.source_start..s.source_end) == Some("Parse"))
        .expect("a span for the Parse label");
    let row = canvas.row_text(span.row);
    let drawn: String = row
        .chars()
        .skip(usize::from(span.col))
        .take(usize::from(span.cols))
        .collect();
    assert_eq!(drawn, "Parse", "the span must sit on the drawn label: {row:?}");
}

#[test]
fn a_flowchart_label_maps_back_in_a_crlf_document() {
    // comrak keeps the \r in a fenced literal; a mapping that measures against the
    // stripped text lands every label one byte further left per preceding line.
    let doc = "# Chart\r\n\r\n```mermaid\r\nflowchart LR\r\n  A[Parse] --> B[Layout]\r\n```\r\n";
    let canvas = render(doc, 60);
    assert!(
        canvas
            .spans()
            .iter()
            .any(|s| doc.get(s.source_start..s.source_end) == Some("Parse")),
        "a CRLF document maps its labels too"
    );
}

#[test]
fn a_flowchart_indented_in_a_list_maps_back() {
    let doc = "- item\n\n  ```mermaid\n  flowchart LR\n    A[Parse] --> B[Layout]\n  ```\n";
    let canvas = render(doc, 60);
    assert!(
        canvas
            .spans()
            .iter()
            .any(|s| doc.get(s.source_start..s.source_end) == Some("Parse")),
        "the list's indent is not part of the mermaid source"
    );
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --jobs 4 --lib maps_back`
Expected: FAIL — no span covers `Parse`.

- [ ] **Step 3: Emit the span in the flowchart layout**

Where the layout writes a node's label lines onto its canvas, add a span per drawn line covering the columns that line occupies, with `source_start`/`source_end` taken from `label.source`. A label with an empty range emits nothing. **A multi-line label emits one span per drawn line, all pointing at the same source range** — that is what makes the whole label atomic (design spec §2.2).

- [ ] **Step 4: Rebase in `render::diagram`**

The spans arriving from `render_mermaid_with` are relative to the fenced block's own text. Add the document offset of the mermaid literal to each before the diagram's canvas is blitted into the document. Read `src/render/code.rs`'s `code_lines`/origins handling first and follow it: **it already solved the CRLF problem and the indent problem**, and the fix is to reuse its answer, not to re-derive one.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib maps_back`
Expected: PASS — three tests.

- [ ] **Step 6: Fault-inject each**

Drop the rebasing (add 0 instead of the literal's offset) — the first test must fail. Strip `\r` before measuring — the CRLF test must fail. Use the raw line rather than the block-relative offset — the list test must fail. Restore after each.

- [ ] **Step 7: Gates and commit**

---

### Task 6: The remaining six families

**Files:**
- Modify: sequence, class, ER, pie, gantt and state layouts. Find each by its `Diagram` variant (`src/mermaid/ast.rs:115`), **not** by directory listing — `src/mermaid/layout/` does not hold all of them; gantt is in `src/mermaid/gantt/`.
- Test: `src/render/tests.rs`

**Interfaces:**
- Consumes: everything from Task 5.

What counts as a label differs per family: a participant and a message in a sequence, a class name and its members, an entity and a relationship label, a slice's name in a pie, a task name in a gantt, a state's description. Where the AST holds a bare `String` rather than a `Label` (`src/mermaid/ast.rs:742`), either give it the same treatment or add an explicit comment saying why it stays unmapped.

- [ ] **Step 1: Write one failing test per family**

Six tests, each the shape of `a_flowchart_label_maps_back_to_the_document` with that family's syntax. Write all six before implementing any. **Do not write one generic test over a list of six documents** — a shared helper hides which family regressed, and six families are six behaviours.

- [ ] **Step 2: Run to verify all six fail**

Run: `cargo test --jobs 4 --lib maps_back`
Expected: FAIL × 6.

- [ ] **Step 3: Implement family by family**

One family, its test green, then the next. Commit after each if you like; the gate is that no family lands without its own test.

- [ ] **Step 4: Run the whole suite**

Run: `cargo test --jobs 4`

- [ ] **Step 5: Fault-inject per family**

Remove each family's span emission in turn; only that family's test may fail. **If removing one family's emission fails another family's test, the two are sharing a code path and the tests are not independent** — say so and fix the tests.

- [ ] **Step 6: Gates and commit**

---

### Task 7: The diagram's floating copy button

**Files:**
- Modify: `src/render/diagram.rs`
- Test: `src/render/tests.rs`

**Interfaces:**
- Consumes: `button::place` (`src/render/button.rs:35`), `ctx.options.copy_button`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_diagram_offers_a_copy_button_carrying_its_source() {
    let doc = "```mermaid\nflowchart LR\n  A[Parse] --> B[Layout]\n```\n";
    let canvas = render_with(doc, 60, &BUTTONS);
    let spot = canvas.hotspots().first().expect("a hotspot");
    assert!(spot.text.contains("flowchart LR"), "got {:?}", spot.text);
    assert!(
        spot.text.contains("A[Parse] --> B[Layout]"),
        "the whole diagram source: {:?}",
        spot.text
    );
    assert!(spot.html.is_none(), "a diagram has no richer flavour");
}

#[test]
fn a_diagram_button_is_off_by_default() {
    let doc = "```mermaid\nflowchart LR\n  A[Parse] --> B[Layout]\n```\n";
    let canvas = render(doc, 60);
    assert!(canvas.hotspots().is_empty());
}

#[test]
fn a_diagram_button_does_not_cover_box_art() {
    let doc = "```mermaid\nflowchart LR\n  A[Parse] --> B[Layout]\n```\n";
    let plain = render(doc, 60);
    let with_button = render_with(doc, 60, &BUTTONS);
    let row = with_button.row_text(0);
    assert!(row.contains("[copy]"), "got {row:?}");
    // Every non-blank cell the plain render drew is still there.
    for (col, ch) in plain.row_text(0).chars().enumerate() {
        if ch != ' ' {
            assert_eq!(
                row.chars().nth(col),
                Some(ch),
                "the button overwrote drawn art at column {col}"
            );
        }
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --jobs 4 --lib a_diagram_offers`
Expected: FAIL — no hotspot.

- [ ] **Step 3: Place it**

At the end of the diagram's render, after the canvas exists and before it is returned, when `ctx.options.copy_button`. Payload is the diagram's whole mermaid source; `html` is `None`. `button::place` already yields rather than overwriting when the target columns are occupied — that yielding is what the third test pins.

- [ ] **Step 4: Run the tests**

Run: `cargo test --jobs 4 --lib diagram`
Expected: PASS.

- [ ] **Step 5: Fault-inject**

Remove the `ctx.options.copy_button` gate — the off-by-default test fails. Force `place` to overwrite instead of yielding — the box-art test fails. Restore.

- [ ] **Step 6: Gates and commit**

---

### Task 8: The muted button style

**Files:**
- Modify: `src/theme/*` — a new token
- Modify: `src/render/button.rs`, `src/render/code.rs`, `src/render/table.rs`, `src/render/diagram.rs` — pass it
- Test: `src/theme/tests.rs` (contrast), `src/render/tests.rs` (snapshots)

**This task ends with a question to the owner, not with a commit of a colour you chose.**

- [ ] **Step 1: Add the token**

A `button` style beside the existing chrome tokens, in **every** theme. Do not reuse the frame colour — that is the thing being fixed.

- [ ] **Step 2: Pass it at all three call sites**

The code frame, the table and the diagram stop passing `theme.code.frame` / `theme.table.border` and pass the new token.

- [ ] **Step 3: Render all three states in every theme and show the owner**

Build a fixture with a code block, a table and a diagram; render it in each theme with buttons on; capture it. Show at rest, hovered (Task 9 lands the trigger — for now force the style) and `[copied]`. **Ask which colours are right.** Do not pick one and declare it done; the owner reviews by looking, and the light theme's heading ramp is separately known to be flat (measured 4.80 → 4.95 → 4.86:1, non-monotone), so it deserves the closest look.

- [ ] **Step 4: Apply the owner's answer, then measure contrast**

Add a contrast test in the style of the existing theme tests asserting the button is legible against the block's background in every theme.

- [ ] **Step 5: Regenerate snapshots and prove the churn**

Buttons already ship on code frames and tables, so goldens will move. Prove the movement is only a style change: strip leading and trailing whitespace from both sides, sort, diff — identical means nothing was added or lost. **Do not hand-resolve snapshot conflicts; regenerate.**

- [ ] **Step 6: Gates and commit**

---

### Task 9: Hover

**Files:**
- Modify: `src/tui/term.rs` — a `MouseEventKind::Moved` arm
- Modify: `src/tui/app.rs` — hovered-hotspot state
- Modify: `src/tui/draw.rs` — paint the hovered style
- Test: `src/tui/tests.rs`

**Interfaces:**
- Produces on `App`: `pub fn set_pointer(&mut self, x: u16, y: u16) -> bool` returning whether the *hovered hotspot identity* changed, and `pub fn hovered(&self) -> Option<usize>` — an index into `canvas.hotspots()`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_pointer_over_a_button_marks_it_hovered() {
    let mut app = app_with("```rust\nlet a = 1;\n```\n", 40, 20);
    app.set_copy_button(true);
    let (row, col) = button_cell(&app);
    assert!(app.hovered().is_none());
    app.set_pointer(col, row);
    assert!(app.hovered().is_some());
}

#[test]
fn moving_within_one_button_does_not_ask_for_a_redraw() {
    let mut app = app_with("```rust\nlet a = 1;\n```\n", 40, 20);
    app.set_copy_button(true);
    let (row, col) = button_cell(&app);
    assert!(app.set_pointer(col, row), "entering the button is a change");
    assert!(
        !app.set_pointer(col + 1, row),
        "moving within the same button is not"
    );
}

#[test]
fn leaving_a_button_is_a_change() {
    let mut app = app_with("```rust\nlet a = 1;\n```\n", 40, 20);
    app.set_copy_button(true);
    let (row, col) = button_cell(&app);
    app.set_pointer(col, row);
    assert!(app.set_pointer(0, row), "leaving it is a change");
    assert!(app.hovered().is_none());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --jobs 4 --lib hover`
Expected: FAIL — no `set_pointer`.

- [ ] **Step 3: Implement the state**

Store `hovered: Option<usize>`. `set_pointer` recomputes it from `hotspot_at`'s translation — **reuse `App::canvas_pos`, the same function `begin_selection` and the click use**, so hover and click cannot disagree about which cell is under the pointer — and returns whether the value changed.

- [ ] **Step 4: Handle `Moved` in `term.rs`**

An arm that calls `set_pointer` and requests a redraw **only when it returns true**. A motion event fires per cell crossed; redrawing on each would be a storm on a fast drag.

- [ ] **Step 5: Paint it**

In `draw.rs`, beside `copied_flash`: when `app.hovered()` names a hotspot, repaint its cells in the hovered style, through the same `Offsets` as everything else. The flash wins over hover when both apply.

- [ ] **Step 6: Run the tests**

Run: `cargo test --jobs 4`

- [ ] **Step 7: Fault-inject**

Make `set_pointer` always return `true` — the "moving within one button" test fails. Make it compare pointer coordinates rather than hotspot identity — the same test fails. Restore.

- [ ] **Step 8: Gates and commit**

---

### Task 10: Re-record the demo

**Files:**
- Modify: `docs/demo/mdmost.webp`

The demo shows selection highlighting and the buttons, both of which have changed.

- [ ] **Step 1: Re-read the recipe**

`docs/maintainer-notes.md` §"Regenerating the demo".

- [ ] **Step 2: Check the widths still hold**

`demo/tour.md` is written to the widths the act 2 drag passes through: the three-column table has two-line cells through 59 columns and single-line rows from 60. **Verify before recording** — nothing in this plan should have moved them, and that is exactly why it is worth confirming.

- [ ] **Step 3: Record**

Foreground. Then check the byte size: lossless WebP, under about 2 MB. If it is heavier, trim act 5's tour beats — never the act 2 drags or the act 4 copies.

- [ ] **Step 4: Look at it**

Watch the recording. The selection in act 4 should now hug the text rather than the row.

- [ ] **Step 5: Gates and commit**

Re-run the three gates plus `cargo check --jobs 4 --target x86_64-pc-windows-msvc`. No source changed, so the test count must match exactly.

---

## Notes for whoever executes this

- **The per-task review has earned its cost on this project four times over.** It has caught a byte-offset bug that copied wrong bytes from any tabbed code block, a silent total provenance loss on every CRLF document, and four vacuous tests. Keep it.
- **Plans have holes exactly where they say "the same way X is handled".** This one says it twice — Task 5's rebasing ("follow `render::code`") and Task 6's six families. Check that the analogy's *tests* came across too, not just its code.
- **Verify a subagent's arithmetic, not its adjectives.** A report saying "all green, 1,050 tests" is a claim; the number that matters is the delta and what accounts for it.
