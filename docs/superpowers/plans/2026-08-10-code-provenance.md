# Code block provenance and copy buttons — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give code block cells a link back to the document source so search and copy work inside fences, then add a clickable `[copy]` to code frames and to tables, the latter copying a grid Excel and Sheets paste as cells.

**Architecture:** The source-line mapping is built at parse time in `doc::convert`, where the source string is in hand, and carried on `NodeKind::CodeBlock`; the renderer turns it into `SearchSpan` records the way inline text already does. The buttons are a fourth canvas metadata channel, `Hotspot`, carrying the clipboard payloads; the label and the hotspot are emitted together by one module so neither can exist without the other. Table payloads come from a new `src/export/` module that is a pure function from AST to string.

**Tech Stack:** Rust 2024, ratatui 0.30, comrak 0.54, arboard 3.6 (optional `clipboard` feature), insta for snapshots.

## Global Constraints

- **4-core cap on every cargo invocation.** Always `--jobs 4`. This machine has 128 cores and is shared.
- **Give every agent its own `CARGO_TARGET_DIR`** when work runs in parallel.
- **The standing clippy gate is `cargo clippy --all-targets --jobs 4 -- -D warnings`.** Plain `cargo clippy` exits 0 on warnings and proves nothing.
- **Never read a gate's result through a pipe.** Run it, look at the exit status directly.
- **Prove every behavioural test red before you fix it.** A test that was never seen failing is not evidence.
- `#![forbid(unsafe_code)]` in the library.
- `render` must not depend on `tui`. The new `export` module may depend only on `doc`.
- **No HTML rendering.** Generating an HTML *clipboard flavour* is permitted and is what Task 6 does; nothing in this plan makes the pager interpret markup from a document.
- Bullets, task boxes and the copy button are **ASCII** and never vary by font detection.
- The status bar never lies: byte counts report the payload the reader actually received.
- Design authority: `docs/superpowers/specs/2026-08-10-code-provenance-design.md`.

---

### Task 1: Source line mapping for code blocks

`comrak` strips the container prefix from every line of a code block's literal — four spaces from an indented block, `> ` from a fence in a block quote, the item indent from a fence in a list. So the literal is not a slice of the source and no arithmetic on the node's span recovers where a line starts. This task builds the mapping by matching each literal line as a **suffix** of a source line, which is prefix-stripping run backwards and can be verified against the real text.

**Files:**
- Modify: `src/doc/mod.rs` — `NodeKind::CodeBlock` gains a `lines` field
- Modify: `src/doc/convert.rs` — `LineOffsets` gains the source text; new `code_lines`
- Test: `src/doc/tests.rs`

**Interfaces:**
- Consumes: `SourceSpan::new(start, end)`, `SourceSpan::default()` (an empty span), `Node { kind, children, source }`, `Doc::parse(&str)`, `Doc::root()`.
- Produces: `NodeKind::CodeBlock { info: String, language: Option<String>, literal: String, fenced: bool, lines: Vec<SourceSpan> }`. `lines` has exactly one entry per line of `literal` (a trailing newline does **not** produce a final empty entry). An entry is `SourceSpan::default()` when the line is empty or could not be located.

- [ ] **Step 1: Write the failing tests**

Add to `src/doc/tests.rs`:

```rust
/// The `lines` of the first code block in `markdown`, as the text they point at.
fn code_line_texts(markdown: &str) -> Vec<String> {
    let doc = Doc::parse(markdown);
    let block = find(doc.root(), &|n| matches!(n.kind, NodeKind::CodeBlock { .. }))
        .expect("a code block");
    let NodeKind::CodeBlock { lines, .. } = &block.kind else {
        unreachable!()
    };
    lines
        .iter()
        .map(|s| doc.source()[s.start..s.end].to_string())
        .collect()
}

#[test]
fn a_fenced_block_maps_each_line_to_its_source() {
    let texts = code_line_texts("```rust\nlet a = 1;\nlet b = 2;\n```\n");
    assert_eq!(texts, ["let a = 1;", "let b = 2;"]);
}

#[test]
fn an_indented_block_maps_past_the_stripped_indent() {
    let texts = code_line_texts("    let a = 1;\n    let b = 2;\n");
    assert_eq!(texts, ["let a = 1;", "let b = 2;"]);
}

#[test]
fn a_quoted_fence_maps_past_the_quote_marker() {
    let texts = code_line_texts("> ```\n> let a = 1;\n> ```\n");
    assert_eq!(texts, ["let a = 1;"]);
}

#[test]
fn a_fence_in_a_list_item_maps_past_the_item_indent() {
    let texts = code_line_texts("- item\n\n  ```\n  let a = 1;\n  ```\n");
    assert_eq!(texts, ["let a = 1;"]);
}

#[test]
fn a_blank_code_line_gets_an_empty_span() {
    let doc = Doc::parse("```\na\n\nb\n```\n");
    let block =
        find(doc.root(), &|n| matches!(n.kind, NodeKind::CodeBlock { .. })).expect("a code block");
    let NodeKind::CodeBlock { lines, .. } = &block.kind else {
        unreachable!()
    };
    assert_eq!(lines.len(), 3, "one entry per literal line");
    assert!(lines[1].is_empty(), "the blank line points at nothing");
    assert_eq!(&doc.source()[lines[0].start..lines[0].end], "a");
    assert_eq!(&doc.source()[lines[2].start..lines[2].end], "b");
}

#[test]
fn a_fence_holding_a_fence_maps_the_inner_one() {
    // The opening `~~~` must not be mistaken for the literal line "```".
    let texts = code_line_texts("~~~\n```\n~~~\n");
    assert_eq!(texts, ["```"]);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib doc::tests 2>&1 | tail -20`
Expected: FAIL — `struct variant NodeKind::CodeBlock has no field named lines`.

- [ ] **Step 3: Add the field**

In `src/doc/mod.rs`, in the `NodeKind::CodeBlock` variant, after `fenced`:

```rust
        /// Whether the block was fenced (as opposed to indented).
        fenced: bool,
        /// Where each line of `literal` came from in the document source.
        ///
        /// One entry per line of `literal`; an empty span for a line that is blank or
        /// that could not be located. comrak strips the container prefix from every
        /// line — four spaces, `> `, a list indent — so this cannot be recovered from
        /// [`Node::source`] by arithmetic, and it is built in [`super::convert`] where
        /// the source text is in hand.
        lines: Vec<SourceSpan>,
```

- [ ] **Step 4: Give `LineOffsets` the source text**

In `src/doc/convert.rs`, change the struct and its constructor. The current definition holds `starts` and `len`; add the text and a line accessor:

```rust
struct LineOffsets<'s> {
    source: &'s str,
    starts: Vec<usize>,
    len: usize,
}

impl<'s> LineOffsets<'s> {
    fn new(source: &'s str) -> Self {
        let mut starts = vec![0usize];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self {
            source,
            starts,
            len: source.len(),
        }
    }

    /// The 0-based index of the line containing `offset`.
    fn line_index(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        }
    }

    /// The content of 0-based line `index`, without its line ending, and where it starts.
    ///
    /// A trailing `\r` is dropped so that a CRLF document matches like any other.
    fn line(&self, index: usize) -> Option<(usize, &'s str)> {
        let start = *self.starts.get(index)?;
        let end = self
            .starts
            .get(index + 1)
            .map_or(self.len, |next| next.saturating_sub(1));
        let text = self.source.get(start..end)?;
        Some((start, text.strip_suffix('\r').unwrap_or(text)))
    }
}
```

Then update every existing `LineOffsets` mention to carry the lifetime: the field/param types in `fn convert<'a>(node, offsets: &LineOffsets<'_>, …)` and in `document`. `offset` and `span` keep their bodies unchanged.

- [ ] **Step 5: Write the mapping**

Add to `src/doc/convert.rs`:

```rust
/// Where each line of a code block's `literal` came from in the source.
///
/// Matched as a **suffix** of a source line, which is comrak's prefix-stripping run
/// backwards: four spaces, `> ` and a list indent all fall out of the same rule without
/// being special-cased, and the match is checked against the real text rather than
/// assumed. A line that cannot be located gets an empty span and the walk carries on —
/// no provenance is today's behaviour, whereas a *wrong* offset would put a search hit
/// on the wrong cells and copy the wrong bytes.
fn code_lines(offsets: &LineOffsets<'_>, span: SourceSpan, literal: &str) -> Vec<SourceSpan> {
    let mut out = Vec::new();
    let mut index = offsets.line_index(span.start);
    let last = offsets.line_index(span.end.saturating_sub(1));
    for line in literal.strip_suffix('\n').unwrap_or(literal).split('\n') {
        if line.is_empty() {
            out.push(SourceSpan::default());
            continue;
        }
        let found = (index..=last).find_map(|at| {
            let (start, text) = offsets.line(at)?;
            text.ends_with(line)
                .then(|| (at, SourceSpan::new(start + text.len() - line.len(), start + text.len())))
        });
        match found {
            Some((at, found)) => {
                index = at + 1;
                out.push(found);
            }
            None => out.push(SourceSpan::default()),
        }
    }
    out
}
```

- [ ] **Step 6: Populate the field**

In `src/doc/convert.rs`, the `NodeValue::CodeBlock(code)` arm becomes:

```rust
        NodeValue::CodeBlock(code) => {
            let info = code.info.clone();
            let language = info
                .split_whitespace()
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase);
            NodeKind::CodeBlock {
                info,
                language,
                lines: code_lines(offsets, source, &code.literal),
                literal: code.literal.clone(),
                fenced: code.fenced,
            }
        }
```

`source` is the `SourceSpan` already computed at the top of `convert` from `ast.sourcepos`.

- [ ] **Step 7: Fix the other construction sites**

Run `cargo build --jobs 4` and add `lines: Vec::new()` (or the real mapping where one exists) to every other place that constructs `NodeKind::CodeBlock`. Expect hits in `src/doc/tests.rs` and possibly `src/render/tests.rs`.

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib doc::tests`
Expected: PASS, including the six new tests.

- [ ] **Step 9: Run the full gates**

Run each separately, reading each exit status directly:
```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
```
Expected: all clean; the test total rises by 6 from 930 to 936.

- [ ] **Step 10: Commit**

```bash
git add src/doc/mod.rs src/doc/convert.rs src/doc/tests.rs
git commit -m "change: a code block remembers where each of its lines came from"
```

---

### Task 2: Search spans for drawn code

With the mapping in hand, the renderer records a `SearchSpan` per drawn code line. This is the task that actually fixes search-in-fences and copy-as-source; everything after it is the buttons.

**Files:**
- Modify: `src/render/code.rs` — `render_code_block`, `framed_code`, `fallback`, `code_area`
- Modify: `src/render/block.rs:190-193` — pass the new field through
- Test: `src/render/tests.rs`, `src/search.rs` tests, `src/tui/tests.rs`

**Interfaces:**
- Consumes: `NodeKind::CodeBlock { lines, .. }` from Task 1; `Canvas::add_span(SearchSpan)`; `SearchSpan { source_start, source_end, row, col, cols }`.
- Produces: `render_code_block(language: Option<&str>, literal: &str, fenced: bool, origins: &[SourceSpan], width: u16, ctx: Ctx<'_>) -> Canvas`. Every non-empty, non-clipped-away code line carries exactly one span.

- [ ] **Step 1: Write the failing tests**

Add to `src/render/tests.rs`:

```rust
#[test]
fn a_code_line_carries_a_span_back_to_the_source() {
    let markdown = "```rust\nlet a = 1;\n```\n";
    let canvas = render(markdown, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| &markdown[s.source_start..s.source_end] == "let a = 1;")
        .expect("the code line maps back to the source");
    assert_eq!(
        canvas.row_text(span.row)[..]
            .chars()
            .skip(usize::from(span.col))
            .take(usize::from(span.cols))
            .collect::<String>(),
        "let a = 1;"
    );
}

#[test]
fn a_clipped_code_line_spans_only_the_drawn_columns() {
    // The frame, its padding and the overflow marker leave far less than the line needs.
    let markdown = "```\nabcdefghijklmnopqrstuvwxyz\n```\n";
    let canvas = render(markdown, 14);
    let span = canvas
        .spans()
        .iter()
        .find(|s| markdown[s.source_start..s.source_end].starts_with('a'))
        .expect("a span for the clipped line");
    let text = &markdown[s_range(span)];
    assert!(
        text.len() < "abcdefghijklmnopqrstuvwxyz".len(),
        "the span must stop where the drawing stopped, got {text:?}"
    );
    assert!(
        !text.contains('z'),
        "the clipped tail is not on screen and must not be spanned"
    );
}

/// The byte range of a span, as a `Range`, for slicing the source in assertions.
fn s_range(span: &crate::canvas::SearchSpan) -> std::ops::Range<usize> {
    span.source_start..span.source_end
}

#[test]
fn the_line_number_gutter_carries_no_span() {
    let options = PLAIN.with_line_numbers(true);
    let markdown = "```\nlet a = 1;\n```\n";
    let canvas = render_with(markdown, 40, &options);
    for span in canvas.spans() {
        let text = &markdown[s_range(span)];
        assert!(
            !text.trim().is_empty() && !text.chars().all(|c| c.is_ascii_digit()),
            "a gutter number is not in the document: {text:?}"
        );
    }
}
```

If `RenderOptions` has no `with_line_numbers`, construct the options with `RenderOptions::new(false, true)` instead and drop that helper call.

Add to `src/tui/tests.rs` (or wherever selection tests live, beside the existing `copied 47 bytes` tests):

```rust
#[test]
fn a_selection_over_code_yields_markdown_source() {
    let markdown = "```rust\nlet a = 1;\n```\n";
    let extract = extract_over_code(markdown);
    assert!(
        extract.from_source,
        "code now has provenance and must be reported as source"
    );
    assert!(
        extract.text.contains("let a = 1;"),
        "got {:?}",
        extract.text
    );
}
```

Write `extract_over_code` against the helpers already in that file: render the document, build a `Selection` covering the row and columns the code line occupies, and call `select::extract`. Find the row by searching `canvas.row_text(row)` for `let a = 1;`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib a_code_line_carries_a_span 2>&1 | tail -20`
Expected: FAIL — `the code line maps back to the source` panics, because no spans exist.

- [ ] **Step 3: Thread the mapping to the renderer**

`src/render/block.rs`, the `NodeKind::CodeBlock` arm:

```rust
        NodeKind::CodeBlock {
            language,
            literal,
            fenced,
            lines,
            ..
        } => code::render_code_block(language.as_deref(), literal, *fenced, lines, width, ctx),
```

`src/render/code.rs`: add `origins: &[SourceSpan]` after `fenced` to `render_code_block`, `framed_code` and `fallback`, and pass it on. Import `use crate::doc::SourceSpan;`. The Mermaid *diagram* path ignores it; the `fallback` path passes it through to `code_area` like `framed_code` does, because that block really is showing the source.

- [ ] **Step 4: Record the spans in `code_area`**

`code_area` gains `origins: &[SourceSpan]` and records one span per line.

The subtlety: lines are written onto an over-wide canvas and the whole block is clipped afterwards, so a span recorded at the line's full length can describe columns that are no longer drawn. That would put a search hit on cells the reader cannot see and make a selection copy bytes that were never on screen. **The span is therefore cut to the visible width as it is recorded** — the clip width is known here, so nothing has to be repaired afterwards.

The byte end is cut by walking graphemes, not bytes or `char`s, for the reason `byte_at_column` gives in `src/tui/select.rs`: a double-width cluster is two columns and one boundary, so counting anything else lands inside it.

Replace the write loop with:

```rust
fn code_area(
    lines: &[Line],
    origins: &[SourceSpan],
    width: u16,
    numbered: bool,
    ctx: Ctx<'_>,
) -> Canvas {
    // … unchanged up to and including the `Canvas::new` call …
    for (row, line) in lines.iter().enumerate() {
        if gutter > 0 {
            let number = format!("{:>digits$} ", row + 1, digits = digits);
            out.write_str(row, 0, &number, theme.code.line_number);
            out.write_str(row, digits + 1, GUTTER_RULE, theme.code.frame);
        }
        out.write_line(row, gutter, line, theme.code.background);
        // The gutter is chrome and is not in the document, so the span starts where the
        // code does. `origins` is empty for a block rendered without a mapping — a
        // fragment, or a construction site that has none — and then this block behaves
        // exactly as it did before spans existed.
        if let Some(origin) = origins.get(row).filter(|o| !o.is_empty()) {
            // How many columns of this line survive the clip below. When the block is
            // clipped, the last column carries the overflow marker and is not code.
            let room = budget.saturating_sub(gutter).saturating_sub(
                usize::from(natural > budget) * display_width(OVERFLOW_MARKER),
            );
            let drawn = line.width().min(room);
            if drawn > 0 {
                out.add_span(SearchSpan {
                    source_start: origin.start,
                    source_end: origin.start + bytes_for_columns(&line.text(), drawn),
                    row,
                    col: u16::try_from(gutter).unwrap_or(u16::MAX),
                    cols: u16::try_from(drawn).unwrap_or(u16::MAX),
                });
            }
        }
    }
    debug_assert!(gutter < budget || budget == 0);
    out.clip_with_marker(width, OVERFLOW_MARKER, theme.code.overflow_marker);
    out.resize_width(width, theme.code.background);
    out
}

/// How many bytes of `text` the first `columns` display columns occupy.
///
/// Grapheme-wise, and for the reason `tui::select::byte_at_column` gives in the other
/// direction: a double-width cluster is two columns and one boundary, so counting bytes
/// or `char`s would land inside it and cut a span mid-character.
fn bytes_for_columns(text: &str, columns: usize) -> usize {
    let mut used = 0usize;
    let mut offset = 0usize;
    for cluster in crate::text::graphemes(text) {
        let width = display_width(cluster);
        if used + width > columns {
            break;
        }
        used += width;
        offset += cluster.len();
    }
    offset
}
```

`Line::text()` is whatever the type already offers for its concatenated text; if there is none, build it with `line.spans.iter().map(|s| s.text.as_str()).collect::<String>()`. `crate::text::graphemes` is the iterator `src/render/inline.rs` already uses — import it the same way that file does.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib code`
Expected: PASS, including the clipped-line test.

- [ ] **Step 6: Check search end to end**

Add to `src/search.rs`'s tests (or `src/tui/tests.rs`, wherever search-over-document tests live):

```rust
#[test]
fn search_matches_inside_a_fenced_code_block() {
    let markdown = "text\n\n```rust\nlet needle = 1;\n```\n";
    let hits = hits_for(markdown, "needle");
    assert_eq!(hits.len(), 1, "the fence is searchable");
    assert!(!hits[0].segments.is_empty(), "and the hit has cells to draw");
}

#[test]
fn search_matches_inside_a_quoted_fence() {
    let markdown = "> ```\n> let needle = 1;\n> ```\n";
    let hits = hits_for(markdown, "needle");
    assert_eq!(hits.len(), 1);
}
```

Write `hits_for` against the existing search-test helpers in that file.

- [ ] **Step 7: Run the full gates and check the goldens**

```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
```
The goldens must **not** move: this task adds metadata, not cells. If any snapshot changed, stop — something is drawing differently and that is a bug, not a golden to accept. Do not hand-resolve snapshot conflicts.

- [ ] **Step 8: Commit**

```bash
git add src/render/code.rs src/render/block.rs src/render/tests.rs src/search.rs src/tui/tests.rs
git commit -m "fix: search and copy can see inside a code fence"
```

---

### Task 3: The `Hotspot` channel

A fourth canvas metadata channel beside `Anchor`, `SearchSpan` and `Pin`, carrying a click target and what it puts on the clipboard.

**Files:**
- Modify: `src/canvas/mod.rs` — the type, the field, the accessors
- Modify: `src/canvas/ops.rs` — merging through `append` and `indent`, dropped by `blit`
- Test: `src/canvas/tests.rs`

**Interfaces:**
- Produces: `Hotspot { row: usize, col: u16, cols: u16, text: String, html: Option<String> }`, `Canvas::hotspots() -> &[Hotspot]`, `Canvas::add_hotspot(Hotspot)`.

- [ ] **Step 1: Write the failing tests**

Add to `src/canvas/tests.rs`:

```rust
/// A hotspot at a known place, for the propagation tests.
fn marked(width: u16) -> Canvas {
    let mut canvas = Canvas::new(width, 2, Style::default());
    canvas.add_hotspot(Hotspot {
        row: 0,
        col: 3,
        cols: 6,
        text: "payload".to_string(),
        html: None,
    });
    canvas
}

#[test]
fn append_moves_a_hotspot_down() {
    let mut top = Canvas::new(20, 3, Style::default());
    top.append(&marked(20), Style::default());
    let spot = &top.hotspots()[0];
    assert_eq!((spot.row, spot.col), (3, 3));
    assert_eq!(spot.text, "payload");
}

#[test]
fn indent_moves_a_hotspot_right() {
    let indented = marked(20).indent(2, 1, Style::default());
    let spot = &indented.hotspots()[0];
    assert_eq!((spot.row, spot.col), (0, 5));
}

#[test]
fn blit_drops_a_hotspot() {
    let mut host = Canvas::new(40, 4, Style::default());
    host.blit(1, 5, &marked(20), Style::default());
    assert!(
        host.hotspots().is_empty(),
        "a canvas placed into a row it shares cannot claim a control there"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib canvas::tests 2>&1 | tail -20`
Expected: FAIL — `cannot find type Hotspot`.

- [ ] **Step 3: Add the type and the field**

In `src/canvas/mod.rs`, after the `Pin` definition:

```rust
/// A region of a row that is a control, and what clicking it copies.
///
/// The fourth metadata channel, and it exists for the reason the other three do: the
/// pager needs to know something about a region that only the renderer which drew it can
/// know. Here that these cells are a button, and what it puts on the clipboard.
///
/// The payload is text, not a source byte range, because the two are not the same
/// answer: the source of a fence inside a block quote carries `> ` on every interior
/// line, and copying that is not what the button promises.
///
/// Like [`Pin`], a hotspot is a claim about a region of one row, so it travels through
/// [`Canvas::append`] and [`Canvas::indent`] and is dropped by [`Canvas::blit`] — a
/// canvas placed at an arbitrary column of a row it shares with other content cannot
/// claim that a control lives there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
    /// The row the control is drawn on.
    pub row: usize,
    /// The first column it occupies.
    pub col: u16,
    /// How many display columns it occupies.
    pub cols: u16,
    /// The plain-text payload. Always present: the only thing OSC 52 can carry.
    pub text: String,
    /// A richer flavour offered to a local clipboard only. `None` for a code block.
    pub html: Option<String>,
}
```

Add `hotspots: Vec<Hotspot>` to `Canvas`, and beside `pins()`/`add_pin`:

```rust
    /// The controls recorded in this canvas.
    pub fn hotspots(&self) -> &[Hotspot] {
        &self.hotspots
    }

    /// Records a control.
    pub fn add_hotspot(&mut self, hotspot: Hotspot) {
        self.hotspots.push(hotspot);
    }
```

- [ ] **Step 4: Merge through `append` and `indent`**

In `src/canvas/ops.rs`, beside `merge_pins`:

```rust
    /// Translates and merges `src`'s hotspots into `self`.
    ///
    /// Separate from [`Canvas::merge_metadata`] for the reason [`Canvas::merge_pins`] is:
    /// a control belongs to a row a block owns outright, so it travels with the
    /// operations that stack and inset whole rows and not with `blit`.
    fn merge_hotspots(&mut self, src: &Canvas, top: usize, left: u16) {
        self.hotspots.extend(src.hotspots.iter().map(|spot| Hotspot {
            row: spot.row + top,
            col: spot.col.saturating_add(left),
            cols: spot.cols,
            text: spot.text.clone(),
            html: spot.html.clone(),
        }));
    }
```

Call it wherever `merge_pins` is called: in `append` as `self.merge_hotspots(other, top, 0)` and in `indent` as `out.merge_hotspots(self, 0, left)`. Do **not** call it from `blit`.

In the slicing operation that filters `spans` and `pins` by row range (around `src/canvas/ops.rs:404-432`), filter and translate `hotspots` the same way `pins` are handled.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib canvas::tests`
Expected: PASS.

- [ ] **Step 6: Run the gates and commit**

```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
git add src/canvas/mod.rs src/canvas/ops.rs src/canvas/tests.rs
git commit -m "change: a canvas can say that a region of a row is a control"
```

---

### Task 4: The button module and the code frame's `[copy]`

One module owns the button's label, its geometry and the rule that **the label and the hotspot are emitted together or not at all** — so a drawn button always does something.

**Files:**
- Create: `src/render/button.rs`
- Modify: `src/render/mod.rs` — declare the module; `RenderOptions.copy_button`
- Modify: `src/render/code.rs` — call it from `framed_code`
- Test: `src/render/tests.rs`

**Interfaces:**
- Consumes: `Canvas::add_hotspot`, `Hotspot` (Task 3); `Ctx { theme, options, .. }`.
- Produces:
  - `RenderOptions.copy_button: bool`, default **false** in `RenderOptions::new`, plus `RenderOptions::with_copy_button(self, bool) -> Self`.
  - `pub(crate) const LABEL: &str = "[copy]";`
  - `pub(crate) const FLASH: &str = "[copied]";`
  - `pub(crate) const REGION: u16 = 9;` — inner columns reserved at the right of a top edge.
  - `pub(crate) fn place(out: &mut Canvas, row: usize, occupied_until: u16, style: Style, text: String, html: Option<String>) -> bool` — draws the label and records the hotspot, or does neither and returns `false`.

- [ ] **Step 1: Write the failing tests**

Add to `src/render/tests.rs`:

```rust
/// Options with the copy button asked for; it is off by default like the banner.
const BUTTONS: RenderOptions = PLAIN.with_copy_button(true);

#[test]
fn a_code_frame_offers_a_copy_button() {
    let canvas = render_with("```rust\nlet a = 1;\n```\n", 40, &BUTTONS);
    let top = canvas.row_text(0);
    assert!(top.contains("[copy]"), "got {top:?}");
    let spot = canvas.hotspots().first().expect("a hotspot");
    assert_eq!(spot.row, 0);
    assert_eq!(spot.cols, 6);
    assert_eq!(spot.text, "let a = 1;\n");
    assert_eq!(spot.html, None, "code has one flavour");
}

#[test]
fn the_copy_button_is_off_by_default() {
    let canvas = render("```rust\nlet a = 1;\n```\n", 40);
    assert!(!canvas.row_text(0).contains("[copy]"));
    assert!(canvas.hotspots().is_empty());
}

#[test]
fn a_narrow_code_frame_drops_the_button_entirely() {
    let canvas = render_with("```rust\nlet a = 1;\n```\n", 16, &BUTTONS);
    assert!(!canvas.row_text(0).contains("[copy]"));
    assert!(
        canvas.hotspots().is_empty(),
        "a label without a hotspot would be a control that does nothing"
    );
}

#[test]
fn the_button_never_overwrites_the_language_label() {
    let canvas = render_with("```rust\nlet a = 1;\n```\n", 40, &BUTTONS);
    let top = canvas.row_text(0);
    assert!(top.contains("rust"), "the language label survives: {top:?}");
}

#[test]
fn a_failed_mermaid_block_offers_its_source() {
    // The fence degraded to a highlighted code block showing Mermaid source, and that
    // source is exactly what a reader who just saw the failure caption wants.
    let canvas = render_with("```mermaid\nnot a diagram at all\n```\n", 40, &BUTTONS);
    let spot = canvas.hotspots().first().expect("a hotspot on the fallback");
    assert_eq!(spot.text, "not a diagram at all\n");
}

#[test]
fn a_code_block_in_a_table_cell_shows_no_button() {
    let markdown = "| a |\n| --- |\n| `x` |\n";
    let canvas = render_with(markdown, 40, &BUTTONS);
    for row in 0..canvas.height() {
        assert!(!canvas.row_text(row).contains("[copy]") || canvas.hotspots().iter().any(|s| s.row == row),
            "row {row} draws a label with no hotspot behind it");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib a_code_frame_offers 2>&1 | tail -20`
Expected: FAIL — no `with_copy_button`.

- [ ] **Step 3: Add the option**

In `src/render/mod.rs`, add to `RenderOptions`:

```rust
    /// Whether a code frame and a table offer a clickable `[copy]`.
    ///
    /// **Off by default**, and set by the pager only when mouse capture was actually
    /// granted: a button nobody can click is worse than no button. `--render-once`
    /// leaves it off, because a dump is text in a pipe.
    pub copy_button: bool,
```

Set `copy_button: false` in `RenderOptions::new`, and add:

```rust
    /// The same options with the copy button turned on or off.
    #[must_use]
    pub const fn with_copy_button(self, copy_button: bool) -> Self {
        Self { copy_button, ..self }
    }
```

- [ ] **Step 4: Write the button module**

Create `src/render/button.rs`:

```rust
//! The clickable `[copy]` drawn into the top edge of a code frame or a table.
//!
//! ASCII, unconditionally — not a Nerd Font glyph behind detection and not a lone
//! Unicode symbol. This is the rule that already governs bullets and task boxes: a mark
//! a reader has to *act on* looks the same in every terminal and can never arrive as
//! tofu. The language label beside it keeps its detected icon, because that one is
//! decoration.
//!
//! One module owns the label, the geometry and the hotspot **because they must not be
//! decided separately**: a drawn label with no hotspot behind it is a control that does
//! nothing, which is exactly what the mouse gate in `RenderOptions::copy_button` exists
//! to prevent. [`place`] emits both or neither.

use crate::canvas::{Canvas, Hotspot};
use crate::theme::Style;

/// What the button says at rest.
pub(crate) const LABEL: &str = "[copy]";

/// What it says just after a copy. Drawn by `tui::draw`, never by a renderer.
pub(crate) const FLASH: &str = "[copied]";

/// Inner columns reserved at the right of a top edge.
///
/// Wider than [`LABEL`] because [`FLASH`] has to fit in the same place without a
/// re-render: `[copied] ` is nine columns and this is what makes the overwrite possible.
pub(crate) const REGION: u16 = 9;

/// Draws the button into `row` and records its hotspot, or does neither.
///
/// `occupied_until` is the first column to the right of everything already in that edge —
/// the language label, the gutter junction — so the button can decline rather than
/// overwrite it. Returns whether it was placed.
pub(crate) fn place(
    out: &mut Canvas,
    row: usize,
    occupied_until: u16,
    style: Style,
    text: String,
    html: Option<String>,
) -> bool {
    let width = out.width();
    // The region ends one column left of the right corner. Two spare columns are asked
    // for beyond whatever already occupies the edge, so the button never sits flush
    // against the language label.
    let Some(region_start) = width.checked_sub(REGION + 1) else {
        return false;
    };
    if region_start < occupied_until.saturating_add(2) {
        return false;
    }
    let label_col = width.saturating_sub(u16::try_from(LABEL.chars().count()).unwrap_or(6) + 2);
    out.write_str(row, usize::from(label_col), LABEL, style);
    out.add_hotspot(Hotspot {
        row,
        col: label_col,
        cols: u16::try_from(LABEL.chars().count()).unwrap_or(6),
        text,
        html,
    });
    true
}
```

Adjust the `write_str` call to the real signature — `write_str(row: usize, col: usize, text: &str, style: Style)` per `src/render/code.rs:220` — and drop the casts that the compiler shows to be unnecessary. Declare the module in `src/render/mod.rs` with `mod button;`.

- [ ] **Step 5: Call it from the code frame**

In `src/render/code.rs`, at the end of `framed_code`, after `pin_gutter`:

```rust
    // The label and the junction have already taken what they need of the top edge; the
    // button is the third occupant and the only optional one, so it is the one that
    // yields. A block inside a table cell is blitted into a row it shares and would lose
    // its hotspot while keeping its cells, so it is not offered one at all.
    if ctx.options.copy_button && ctx.table_depth == 0 {
        let occupied = top_edge_occupied(&out, title.as_ref());
        button::place(
            &mut out,
            0,
            occupied,
            theme.code.frame,
            literal.to_string(),
            None,
        );
    }
    out
```

`framed_code` needs `literal` in scope — it already takes it.

**Do the same in `fallback`**, which builds its own `framed_captioned` rather than going through `framed_code`. That block is a highlighted code block showing Mermaid source, and per the design it gets a button; its top edge already names the language, so pass the same `top_edge_occupied` result. A drawn diagram gets nothing — it is box art, and there is no code to copy.

Add a small helper beside `pin_gutter` that reports the first free column of the top edge, reading it back off the drawn row exactly as `pin_gutter` already does rather than re-deriving the arithmetic:

```rust
/// The first column of the top edge that nothing has claimed yet.
///
/// Read back off the drawn row for the same reason `pin_gutter` does it: the label's
/// column depends on whether it collided with the gutter junction, and a second copy of
/// that arithmetic is what would drift.
fn top_edge_occupied(out: &Canvas, title: Option<&Line>) -> u16 {
    let Some(title) = title else { return 1 };
    let text = out.row_text(0);
    let start = title
        .spans
        .first()
        .and_then(|span| text.find(span.text.as_str()))
        .map_or(2, |byte| display_width(&text[..byte]));
    u16::try_from(start + title.width() + 1).unwrap_or(u16::MAX)
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib button; cargo test --jobs 4 --lib code`
Expected: PASS.

- [ ] **Step 7: Run the gates**

```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
```
The goldens must not move — `copy_button` defaults to false and the goldens render with default options. If they moved, the default is wrong.

- [ ] **Step 8: Commit**

```bash
git add src/render/button.rs src/render/mod.rs src/render/code.rs src/render/tests.rs
git commit -m "change: a code frame offers a copy button, when a mouse can reach it"
```

---

### Task 5: `export` — the TSV grid

TSV is what makes Excel and Google Sheets split a paste into cells, and it is the only thing OSC 52 can carry, so it is the payload every reader receives. It is built first and alone.

**Files:**
- Create: `src/export/mod.rs`, `src/export/tsv.rs`, `src/export/tests.rs`
- Modify: `src/lib.rs` — declare the module

**Interfaces:**
- Consumes: `Node`, `NodeKind::{Table, TableRow, TableCell}`, `Node::plain_text()`.
- Produces: `pub fn table_tsv(node: &Node) -> String` — rows separated by `\n`, cells by `\t`, with a trailing newline.

- [ ] **Step 1: Write the failing tests**

Create `src/export/tests.rs`:

```rust
//! Unit tests for the clipboard exporters.

use super::*;
use crate::doc::{Doc, Node, NodeKind};

/// Runs `f` on the first table in `markdown`.
///
/// A closure rather than a returned `Node`, so the helper does not depend on `Node`
/// being `Clone` and the borrow of the parsed document stays alive for the assertion.
fn with_table<T>(markdown: &str, f: impl FnOnce(&Node) -> T) -> T {
    fn find(node: &Node) -> Option<&Node> {
        if matches!(node.kind, NodeKind::Table(_)) {
            return Some(node);
        }
        node.children.iter().find_map(find)
    }
    let doc = Doc::parse(markdown);
    f(find(doc.root()).expect("a table"))
}

/// The TSV of the first table in `markdown`.
fn tsv_of(markdown: &str) -> String {
    with_table(markdown, table_tsv)
}

/// The HTML of the first table in `markdown`.
fn html_of(markdown: &str) -> String {
    with_table(markdown, table_html)
}

#[test]
fn a_table_becomes_a_tab_separated_grid() {
    let grid = tsv_of("| a | b |\n| --- | --- |\n| 1 | 2 |\n");
    assert_eq!(grid, "a\tb\n1\t2\n");
}

#[test]
fn cell_markup_is_flattened_to_its_text() {
    let grid = tsv_of("| a |\n| --- |\n| **bold** `code` |\n");
    assert_eq!(grid, "a\nbold code\n");
}

#[test]
fn a_tab_inside_a_cell_cannot_break_the_grid() {
    // A literal tab in a cell would otherwise add a column to that row alone.
    let grid = tsv_of("| a | b |\n| --- | --- |\n| x\ty | z |\n");
    assert_eq!(
        grid.lines().nth(1).unwrap().split('\t').count(),
        2,
        "every row has the same number of columns: {grid:?}"
    );
}

#[test]
fn a_line_break_inside_a_cell_cannot_break_the_grid() {
    let grid = tsv_of("| a |\n| --- |\n| x<br>y |\n");
    assert_eq!(grid.lines().count(), 2, "two rows, not three: {grid:?}");
}

#[test]
fn an_empty_cell_keeps_its_column() {
    let grid = tsv_of("| a | b |\n| --- | --- |\n|  | 2 |\n");
    assert_eq!(grid, "a\tb\n\t2\n");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib export 2>&1 | tail -20`
Expected: FAIL — module `export` does not exist.

- [ ] **Step 3: Create the module**

`src/export/mod.rs`:

```rust
//! Turning a parsed table into clipboard payloads.
//!
//! A pure function from AST to string: this module depends on [`crate::doc`] and nothing
//! else — not on `canvas`, not on `theme`, and above all not on `tui`. That is what makes
//! it the easiest thing here to test exhaustively, and it is why the renderer can build a
//! payload at render time without dragging the pager in with it.
//!
//! Two flavours, and they are not equal partners. **TSV is what makes Excel and Google
//! Sheets split a paste into cells** — not HTML — and it is the only thing OSC 52 can
//! carry, so it is what every reader receives. The HTML in [`html`] is an upgrade
//! offered to a local clipboard on top of it.

mod html;
mod tsv;

#[cfg(test)]
mod tests;

pub use html::table_html;
pub use tsv::table_tsv;
```

`src/export/tsv.rs`:

```rust
//! The tab-separated grid: the payload every reader receives.

use crate::doc::{Node, NodeKind};

/// A table as tab-separated rows, with a trailing newline.
///
/// Each cell is flattened to a single line. A tab or a newline *inside* a cell becomes a
/// space, which is a deliberate choice over Excel's `"…"` quoting convention: quoting is
/// fragile and Sheets and Excel disagree about it, whereas flattening cannot produce a
/// grid that misaligns — and the pager is already showing that cell on one line.
pub fn table_tsv(node: &Node) -> String {
    let mut out = String::new();
    for row in node.children.iter().filter(|c| matches!(c.kind, NodeKind::TableRow { .. })) {
        let cells: Vec<String> = row
            .children
            .iter()
            .filter(|c| matches!(c.kind, NodeKind::TableCell))
            .map(|cell| flatten(&cell.plain_text()))
            .collect();
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

/// One cell's text, with everything that would break the grid replaced by a space.
fn flatten(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}
```

Add `mod export;` to `src/lib.rs` (as `pub mod export;` if the crate exposes its modules; match the existing style there). Create a placeholder `src/export/html.rs` with `pub fn table_html(_node: &crate::doc::Node) -> String { String::new() }` so the module compiles; Task 6 fills it in.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib export`
Expected: PASS — five tests.

- [ ] **Step 5: Run the gates and commit**

```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
git add src/export src/lib.rs
git commit -m "change: a table can be handed to a spreadsheet as a grid"
```

---

### Task 6: `export` — the HTML flavour

The upgrade a local clipboard can carry: emphasis, alignment and links. **Escaping is the load-bearing part** — a document is untrusted input.

**Files:**
- Modify: `src/export/html.rs`
- Test: `src/export/tests.rs`

**Interfaces:**
- Consumes: `TableInfo { alignments: Vec<Option<Align>>, columns: usize }`, `NodeKind::{Text, Strong, Emph, Strikethrough, Code, Link, Image, LineBreak, SoftBreak}`, `Align`.
- Produces: `pub fn table_html(node: &Node) -> String` — one `<table>…</table>`, no newline needed.

- [ ] **Step 1: Write the failing tests**

Add to `src/export/tests.rs`:

```rust
#[test]
fn a_table_becomes_an_html_table() {
    let html = html_of("| a | b |\n| --- | --- |\n| 1 | 2 |\n");
    assert!(html.starts_with("<table>"), "got {html}");
    assert!(html.contains("<th>a</th>"), "the header row is th: {html}");
    assert!(html.contains("<td>1</td>"), "got {html}");
}

#[test]
fn inline_markup_becomes_inline_html() {
    let html = html_of("| a |\n| --- |\n| **b** *i* ~~s~~ `c` |\n");
    assert!(html.contains("<strong>b</strong>"), "got {html}");
    assert!(html.contains("<em>i</em>"), "got {html}");
    assert!(html.contains("<del>s</del>"), "got {html}");
    assert!(html.contains("<code>c</code>"), "got {html}");
}

#[test]
fn declared_alignment_reaches_the_cells() {
    let html = html_of("| a |\n| ---: |\n| 1 |\n");
    assert!(html.contains(r#"align="right""#), "got {html}");
}

#[test]
fn text_is_escaped() {
    // A `<` the parser did not read as a tag stays in the text, and must not be able to
    // open one in the payload.
    let html = html_of("| a |\n| --- |\n| 1 < 2 & \"q\" |\n");
    assert!(html.contains("1 &lt; 2"), "got {html}");
    assert!(html.contains("&amp;"), "got {html}");
    assert!(html.contains("&quot;q&quot;"), "got {html}");
}

// Corrected during execution: the plan originally fed `<script>x</script>` to this test
// and expected `&lt;script&gt;`. The parser turns raw HTML into `SkippedHtml`, which
// contributes nothing, so the tag is dropped rather than escaped -- and dropping is the
// right answer, because the reader never saw the tag either.
#[test]
fn raw_html_in_a_cell_reaches_the_clipboard_as_nothing() {
    let html = html_of("| a |\n| --- |\n| <script>x</script> |\n");
    assert!(!html.contains("<script"), "no live tag: {html}");
    assert!(!html.contains("script"), "not even escaped: {html}");
    assert!(html.contains("<td>x</td>"), "the text survives: {html}");
}

#[test]
fn an_http_link_keeps_its_href() {
    let html = html_of("| a |\n| --- |\n| [t](https://example.com/x?a=1&b=2) |\n");
    assert!(
        html.contains(r#"<a href="https://example.com/x?a=1&amp;b=2">t</a>"#),
        "got {html}"
    );
}

#[test]
fn a_javascript_link_loses_its_href_and_keeps_its_text() {
    let html = html_of("| a |\n| --- |\n| [click](javascript:alert(1)) |\n");
    assert!(!html.contains("javascript"), "got {html}");
    assert!(!html.contains("<a "), "no anchor at all: {html}");
    assert!(html.contains("click"), "the text survives: {html}");
}

#[test]
fn a_quote_inside_a_url_cannot_escape_the_attribute() {
    let html = html_of("| a |\n| --- |\n| [t](https://e.com/\"onx=1) |\n");
    assert!(!html.contains(r#""onx"#), "got {html}");
}

#[test]
fn a_line_break_in_a_cell_becomes_br() {
    let html = html_of("| a |\n| --- |\n| x<br>y |\n");
    assert!(html.contains("<br>"), "got {html}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib export::tests 2>&1 | tail -20`
Expected: FAIL — the placeholder returns an empty string.

- [ ] **Step 3: Write the exporter**

`src/export/html.rs`:

```rust
//! The HTML flavour: the upgrade a local clipboard can carry.
//!
//! Offered beside the TSV, never instead of it — OSC 52 has no MIME flavours, so a
//! reader on a remote host receives the TSV alone and must still get cells.
//!
//! **Everything here is generated and escaped here.** A document is untrusted input, and
//! this payload is handed to another application to interpret: a cell containing
//! `<script>` arrives as escaped text, and only `http`, `https` and `mailto` links keep
//! an `href`. No markup from the document is ever passed through — that would be the
//! "no HTML" rule, and this is the opposite direction: an AST the pager already parsed,
//! serialised out.

use crate::doc::{Node, NodeKind, TableInfo};
use crate::text::Align;

/// A table as an HTML `<table>`.
pub fn table_html(node: &Node) -> String {
    let info = match &node.kind {
        NodeKind::Table(info) => info,
        _ => return String::new(),
    };
    let mut out = String::from("<table>");
    for row in node.children.iter() {
        let NodeKind::TableRow { header } = row.kind else {
            continue;
        };
        out.push_str("<tr>");
        for (index, cell) in row
            .children
            .iter()
            .filter(|c| matches!(c.kind, NodeKind::TableCell))
            .enumerate()
        {
            let tag = if header { "th" } else { "td" };
            out.push('<');
            out.push_str(tag);
            out.push_str(&align_attribute(info, index));
            out.push('>');
            for child in &cell.children {
                inline(child, &mut out);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

/// The `align` attribute for a column, or nothing when none was declared.
fn align_attribute(info: &TableInfo, column: usize) -> String {
    match info.alignments.get(column).copied().flatten() {
        Some(Align::Right) => r#" align="right""#.to_string(),
        Some(Align::Center) => r#" align="center""#.to_string(),
        Some(Align::Left) => r#" align="left""#.to_string(),
        None => String::new(),
    }
}

/// Serialises one inline node, escaping everything it emits.
fn inline(node: &Node, out: &mut String) {
    let wrap = |out: &mut String, tag: &str, node: &Node| {
        out.push('<');
        out.push_str(tag);
        out.push('>');
        for child in &node.children {
            inline(child, out);
        }
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    };
    match &node.kind {
        NodeKind::Text(text) => escape_into(text, out),
        NodeKind::Strong => wrap(out, "strong", node),
        NodeKind::Emph => wrap(out, "em", node),
        NodeKind::Strikethrough => wrap(out, "del", node),
        NodeKind::Code { literal } => {
            out.push_str("<code>");
            escape_into(literal, out);
            out.push_str("</code>");
        }
        NodeKind::Link { url, .. } => {
            if is_safe_url(url) {
                out.push_str(r#"<a href=""#);
                escape_into(url, out);
                out.push_str(r#"">"#);
                for child in &node.children {
                    inline(child, out);
                }
                out.push_str("</a>");
            } else {
                // The scheme is not one another application should be handed. The link
                // text is still what the reader saw, so it stays.
                for child in &node.children {
                    inline(child, out);
                }
            }
        }
        NodeKind::LineBreak => out.push_str("<br>"),
        NodeKind::SoftBreak => out.push(' '),
        _ => escape_into(&node.plain_text(), out),
    }
}

/// Whether a URL may be handed to another application as an `href`.
///
/// An allow-list, not a deny-list: the payload leaves this process and is interpreted
/// elsewhere, so the question is closed by naming what is permitted rather than by
/// trying to name every scheme that is not.
fn is_safe_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    // Corrected during execution: the plan also allowed a relative target, which the
    // design's "only http, https and mailto" does not. The design wins -- a relative
    // target would resolve against whatever document the paste lands in.
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Appends `text` with the four characters that would otherwise be markup escaped.
///
/// `"` is escaped along with the rest rather than only in attribute position, so that no
/// caller can pick the wrong helper: there is only one.
fn escape_into(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}
```

Note the `r#"">"#` above closes the attribute and the tag; check it compiles and reads as `">`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib export`
Expected: PASS — fourteen tests in total for the module.

- [ ] **Step 5: Run the gates and commit**

```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
git add src/export/html.rs src/export/tests.rs
git commit -m "change: a table can also be handed over with its formatting"
```

---

### Task 7: A dual-flavour copy, and honest wording

**Files:**
- Modify: `src/tui/clipboard.rs`
- Modify: `src/tui/app.rs:1598` — the `message` call
- Modify: `src/tui/term.rs:366-372` — `copy_selection`
- Test: `src/tui/tests.rs:3385-3430`

**Interfaces:**
- Produces:
  - `pub enum Copied { Source, Rendered, Code, Table }` with `fn what(self) -> &'static str` returning `"Markdown source"`, `"rendered text"`, `"code"`, `"table"`.
  - `Delivery::message(&self, bytes: usize, copied: Copied) -> (String, bool)` — replaces the `from_source: bool` parameter.
  - `pub fn copy_rich(text: &str, html: Option<&str>) -> Delivery`.

- [ ] **Step 1: Write the failing tests**

Replace the existing wording tests in `src/tui/tests.rs` (around lines 3385-3430) so they pass `Copied` instead of a bool, and add:

```rust
#[test]
fn a_table_copy_says_what_it_was() {
    let (text, is_error) = Delivery::Confirmed.message(47, Copied::Table);
    assert_eq!(text, "copied 47 bytes of table");
    assert!(!is_error);
}

#[test]
fn a_code_copy_says_what_it_was() {
    let (text, _) = Delivery::Confirmed.message(12, Copied::Code);
    assert_eq!(text, "copied 12 bytes of code");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib clipboard 2>&1 | tail -20`
Expected: FAIL — `cannot find type Copied`.

- [ ] **Step 3: Add the wording type**

In `src/tui/clipboard.rs`:

```rust
/// What was copied, for the status bar to name.
///
/// A type rather than a `bool` because there are now four answers and because the
/// wording is the whole point: telling a reader they copied Markdown when they copied
/// box art is the kind of lie this project keeps finding in its own doc comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    /// A selection that mapped back to the document source.
    Source,
    /// A selection that did not, so the drawn cells were taken instead.
    Rendered,
    /// A whole code block, from its button.
    Code,
    /// A whole table, from its button.
    Table,
}

impl Copied {
    /// The noun the status bar uses.
    fn what(self) -> &'static str {
        match self {
            Copied::Source => "Markdown source",
            Copied::Rendered => "rendered text",
            Copied::Code => "code",
            Copied::Table => "table",
        }
    }
}
```

Change `Delivery::message` to take `copied: Copied` and open with `let what = copied.what();`. Update `src/tui/app.rs:1598` and its caller `App::report_copy` to take and pass a `Copied`; at `src/tui/term.rs:371` pass `if extract.from_source { Copied::Source } else { Copied::Rendered }`.

- [ ] **Step 4: Add the dual-flavour copy**

In `src/tui/clipboard.rs`, beside `copy`:

```rust
/// Copies `text`, offering `html` as a richer flavour where a local clipboard exists.
///
/// The asymmetry is not an oversight. OSC 52 is one escape sequence carrying one
/// plain-text payload — it has no MIME flavours — and it is the route that survives SSH,
/// which is why it is written first and unconditionally. The HTML is therefore an upgrade
/// for a reader at a local display server, and **nobody ever receives less than `text`**.
pub fn copy_rich(text: &str, html: Option<&str>) -> Delivery {
    classify(write_osc52(text), local_clipboard_rich(text, html))
}
```

With the `clipboard` feature, `local_clipboard_rich` mirrors `local_clipboard` but calls `arboard`'s HTML setter with `text` as the alternate when `html` is `Some`, and falls back to the plain path when it is `None`. Without the feature it is the same stub that returns `None`. Keep `copy` as `copy_rich(text, None)` so there is one implementation.

Look up the exact `arboard` 3.6 setter shape before writing it:

```bash
cargo doc --jobs 4 -p arboard --no-deps 2>/dev/null; grep -rn "fn html" ~/.cargo/registry/src/*/arboard-3.6.1/src/common.rs | head
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib clipboard`
Expected: PASS.

- [ ] **Step 6: Run the gates and commit**

```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
git add src/tui/clipboard.rs src/tui/app.rs src/tui/term.rs src/tui/tests.rs
git commit -m "change: a copy can carry a second flavour, and says which one it was"
```

---

### Task 8: The table's copy button

**Files:**
- Modify: `src/render/table.rs` — `render_table_node`
- Test: `src/render/tests.rs`

**Interfaces:**
- Consumes: `button::place` (Task 4), `export::{table_tsv, table_html}` (Tasks 5-6), `ctx.options.copy_button`.

- [ ] **Step 1: Write the failing tests**

Add to `src/render/tests.rs`:

```rust
#[test]
fn a_table_offers_a_copy_button_with_both_flavours() {
    let canvas = render_with("| a | b |\n| --- | --- |\n| 1 | 2 |\n", 40, &BUTTONS);
    assert!(canvas.row_text(0).contains("[copy]"), "got {:?}", canvas.row_text(0));
    let spot = canvas.hotspots().first().expect("a hotspot");
    assert_eq!(spot.row, 0, "the button is in the top rule");
    assert_eq!(spot.text, "a\tb\n1\t2\n");
    assert!(
        spot.html.as_deref().unwrap_or_default().starts_with("<table>"),
        "a table offers the richer flavour too"
    );
}

#[test]
fn a_table_button_is_off_by_default() {
    let canvas = render("| a | b |\n| --- | --- |\n| 1 | 2 |\n", 40);
    assert!(canvas.hotspots().is_empty());
}

#[test]
fn a_narrow_table_drops_its_button() {
    let canvas = render_with("| a | b |\n| --- | --- |\n| 1 | 2 |\n", 12, &BUTTONS);
    assert!(!canvas.row_text(0).contains("[copy]"));
    assert!(canvas.hotspots().is_empty());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib a_table_offers 2>&1 | tail -20`
Expected: FAIL — no `[copy]` in the top rule.

- [ ] **Step 3: Place it**

In `src/render/table.rs`, at the end of `render_table_node`, after the clip:

```rust
    // The top rule has no label of its own, so nothing but the corner is in the way.
    // A nested table is blitted into a row it shares and would lose its hotspot while
    // keeping its cells, so only a top-level table is offered one.
    if ctx.options.copy_button && ctx.table_depth == 0 {
        super::button::place(
            &mut canvas,
            0,
            1,
            ctx.theme.table.frame,
            crate::export::table_tsv(node),
            Some(crate::export::table_html(node)),
        );
    }
```

Use the theme field the table's border actually uses — check `ctx.theme.table` for the right name; the border style is whatever `lay_out` passes to the rule rows.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib table`
Expected: PASS.

- [ ] **Step 5: Run the gates and commit**

```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
git add src/render/table.rs src/render/tests.rs
git commit -m "change: a table offers a copy button too"
```

---

### Task 9: Wiring the buttons to the mouse

The last task, and the only one that touches the pager. Three things: turn the option on when the mouse is real, make a click copy, and paint the flash.

**Files:**
- Modify: `src/tui/app.rs` — the option setter, the hit test, the flash state
- Modify: `src/tui/term.rs:195-198` — set the option after the capture attempt; `:343` — handle the click
- Modify: `src/tui/draw.rs` — paint the flash
- Test: `src/tui/tests.rs`

**Interfaces:**
- Consumes: `Canvas::hotspots()`, `Hotspot`, `clipboard::{copy_rich, Copied}`, `button::FLASH`.
- Produces on `App`:
  - `pub fn set_copy_button(&mut self, on: bool)` — updates the options and invalidates the canvas cache.
  - `pub fn hotspot_at(&self, x: u16, y: u16) -> Option<&Hotspot>` — in canvas coordinates, the same translation the selection uses.
  - `pub fn flash_copied(&mut self, row: usize, col: u16)` and `pub fn copied_flash(&self) -> Option<(usize, u16)>` — the latter returning `None` once the flash has expired.

- [ ] **Step 1: Write the failing tests**

Add to `src/tui/tests.rs`:

```rust
#[test]
fn a_click_on_the_button_copies_and_does_not_select() {
    let mut app = app_with("```rust\nlet a = 1;\n```\n", 40, 20);
    app.set_copy_button(true);
    let (row, col) = button_cell(&app);
    let spot = app.hotspot_at(col, row).expect("a hotspot under the pointer");
    assert_eq!(spot.text, "let a = 1;\n");
    app.begin_selection(col, row);
    assert!(
        app.selection().is_none(),
        "a press on a control is not the start of a drag"
    );
}

#[test]
fn a_click_one_column_left_of_the_button_still_selects() {
    let mut app = app_with("```rust\nlet a = 1;\n```\n", 40, 20);
    app.set_copy_button(true);
    let (row, col) = button_cell(&app);
    assert!(app.hotspot_at(col - 1, row).is_none());
}

#[test]
fn the_flash_expires() {
    let mut app = app_with("```rust\nlet a = 1;\n```\n", 40, 20);
    app.flash_copied(0, 30);
    assert!(app.copied_flash().is_some());
    std::thread::sleep(std::time::Duration::from_millis(FLASH_FOR + 50));
    assert!(app.copied_flash().is_none(), "the label goes back to [copy]");
}
```

Write `app_with` and `button_cell` against the existing helpers in that file: `button_cell` finds the row whose text contains `[copy]` and returns the column of its `[`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --jobs 4 --lib a_click_on_the_button 2>&1 | tail -20`
Expected: FAIL — no `set_copy_button`.

- [ ] **Step 3: Add the app state**

In `src/tui/app.rs`:

```rust
/// How long the `[copied]` label stays up, in milliseconds.
///
/// The event loop redraws every `POLL_INTERVAL` (120 ms) whether or not anything
/// happened, so the flash clears itself without any new timer: the next tick after the
/// deadline draws the label back. Nothing here schedules a wake-up.
pub(crate) const FLASH_FOR: u64 = 600;
```

Add a field `copied_flash: Option<(usize, u16, std::time::Instant)>` and:

```rust
    /// Turns the copy button on or off, discarding the canvas that was drawn without it.
    pub fn set_copy_button(&mut self, on: bool) {
        if self.options.copy_button == on {
            return;
        }
        self.options = self.options.with_copy_button(on);
        self.invalidate_render();
    }

    /// The control under a pointer at canvas coordinates `(x, y)`, if any.
    pub fn hotspot_at(&self, x: u16, y: u16) -> Option<&Hotspot> {
        let row = usize::from(y) + self.top_row();
        self.canvas().hotspots().iter().find(|spot| {
            spot.row == row && x >= spot.col && x < spot.col.saturating_add(spot.cols)
        })
    }

    /// Records that the control at `(row, col)` was just used.
    pub fn flash_copied(&mut self, row: usize, col: u16) {
        self.copied_flash = Some((row, col, std::time::Instant::now()));
    }

    /// The control still showing its flash, if the flash has not expired.
    pub fn copied_flash(&self) -> Option<(usize, u16)> {
        self.copied_flash
            .filter(|(_, _, at)| at.elapsed() < std::time::Duration::from_millis(FLASH_FOR))
            .map(|(row, col, _)| (row, col))
    }
```

Use the real names for the scroll offset, the canvas accessor and the cache invalidation — `top_row`, `canvas` and `invalidate_render` above are placeholders for whatever `App` already calls them. Find them by reading how `begin_selection` translates a pointer to canvas coordinates and reuse **exactly that** translation, so a click and a selection can never disagree about which cell is under the pointer.

- [ ] **Step 4: Turn the option on only when the mouse is real**

In `src/tui/term.rs`, at the capture attempt:

```rust
    let mouse = app.config().mouse && execute!(io::stdout(), EnableMouseCapture).is_ok();
    if app.config().mouse && !mouse {
        app.notify("this terminal refused mouse capture", true);
    }
    // A button nobody can click is worse than no button, so the renderer is told what
    // actually happened rather than what was asked for.
    app.set_copy_button(mouse);
```

- [ ] **Step 5: Handle the click**

In `src/tui/term.rs`, **before** the `MouseEventKind::Down(MouseButton::Left) if in_doc` arm:

```rust
        MouseEventKind::Down(MouseButton::Left)
            if in_doc && {
                let (x, y) = local();
                app.hotspot_at(x, y).is_some()
            } =>
        {
            let (x, y) = local();
            copy_hotspot(app, x, y);
        }
```

And beside `copy_selection`:

```rust
/// Copies the control under the pointer.
fn copy_hotspot(app: &mut App, x: u16, y: u16) {
    let Some(spot) = app.hotspot_at(x, y) else {
        return;
    };
    let (row, col) = (spot.row, spot.col);
    let (text, html) = (spot.text.clone(), spot.html.clone());
    let what = if html.is_some() {
        Copied::Table
    } else {
        Copied::Code
    };
    let delivery = super::clipboard::copy_rich(&text, html.as_deref());
    // The byte count is the plain payload's: it is what every reader receives, and a
    // reader on a remote host never got the HTML at all.
    app.report_copy(text.len(), what, &delivery);
    app.flash_copied(row, col);
}
```

- [ ] **Step 6: Paint the flash**

In `src/tui/draw.rs`, where the document rows are painted, after the canvas has been drawn: if `app.copied_flash()` names a hotspot on a visible row, overwrite `[copied] ` starting two columns left of the hotspot's column, in the same style the surrounding frame uses.

```rust
    // Nine columns were reserved at render time precisely so this fits without a
    // re-render: rendering is a pure function of (AST, width, theme, options), and
    // "this block was copied 300 ms ago" is pager state that must not enter it.
    if let Some((row, col)) = app.copied_flash() {
        // `col` is the label's first column; the reserved region starts two left of it.
        paint_over(frame, row, col.saturating_sub(2), render::button::FLASH);
    }
```

Write `paint_over` against how `draw` already writes cells into the frame buffer, applying the same vertical scroll offset and horizontal offset as the surrounding row, and skipping the paint entirely when the row is off screen.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test --jobs 4 --lib tui`
Expected: PASS.

- [ ] **Step 8: Run the full gates**

```bash
cargo fmt --check
cargo clippy --all-targets --jobs 4 -- -D warnings
cargo test --jobs 4
cargo check --target x86_64-pc-windows-msvc --all-targets --jobs 4
```

The Windows check is **expected to fail** on `SIGHUP` at `src/tui/term.rs:188` — that is a pre-existing failure, Task 1 of the publishing plan, and not this work's to fix. Confirm the error is only that one and that nothing in this plan added a second.

- [ ] **Step 9: Drive the real binary**

Tests are not the gate the owner reviews by. In a tmux session you created and will kill:

```bash
cargo build --jobs 4
printf '# Title\n\n```rust\nlet a = 1;\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n' > /tmp/claude-1003/-home-oetiker-checkouts-mdmost/*/scratchpad/demo.md
./target/debug/mdmost /tmp/.../demo.md
```

Confirm by eye: both buttons appear, clicking each flips to `[copied]` and back, the status bar names `code` and `table`, searching for a word inside the fence finds it, and selecting a code line reports Markdown source. Leave no stray `mdmost` processes and kill only your own tmux session.

- [ ] **Step 10: Commit**

```bash
git add src/tui/app.rs src/tui/term.rs src/tui/draw.rs src/tui/tests.rs
git commit -m "change: the copy buttons answer the mouse"
```

---

## After the plan

Update `docs/superpowers/specs/2026-08-08-mdmost-design.md` if the button changes anything it asserts about code frames or tables, and note the new `copy_button` option in `README.md` beside the other render options — it is not a CLI flag, so say that it follows mouse capture.
