# A viewport-driven highlighter

Design authority for the work that follows. Written 2026-09-25, after v0.3.5.

This is the second of two branches. The first, `2026-09-22-bounded-syntect-design.md`,
made every `syntect` parse finite and removed the watchdog thread. That is what makes it
safe to parse on the main event loop, in slices, between key presses.

## 1. What this changes

In the pager, a document's first screen is drawn with its code blocks in plain themed
text. Colour then arrives in slices: first for the blocks in the viewport, then for the
blocks about one screen above and below, then work stops. Colour reaches the screen by
changing the `style` of cells already on the canvas, not by laying the document out again.

`--render-once`, the stdout path and every test that renders a document keep the blocking
behaviour: all blocks are highlighted before output. Their output does not change, with
one exception in §8 (the caption style).

Measured 2026-09-25 on the reference document (36 fences, release, `--render-once`):
v0.3.5 takes ~1.87 s, of which highlighting was ~1.4 s in an earlier profile. The first
screen in the pager is expected to appear in roughly ~0.4 s. That figure is an estimate;
§10 says how it is measured.

## 2. Settled rulings

These were decided with the owner before this spec was written and are not reopened here.

1. Colour reaches the screen by patching the canvas cells' `style` field. Plain and
   highlighted output have identical geometry, pinned by
   `a_caption_does_not_change_a_block_s_height`.
2. Partial colour, as each slice arrives. A long block colours top-down.
3. A slice is sized by a time budget, not a line count, and is tested against an injected
   clock.
4. Viewport first, then a halo of about one screen each way, then idle.
5. Suspended parser state goes in a parking lot, oldest evicted, cap 4, separate from the
   finished lines.
6. A failure poisons the block, not the language. The caption is `highlighting gave up`,
   in the frame's bottom border. No process-global kill switch.
7. A `Highlighter` owned by `App`, not more process globals. `render` does not depend on
   `tui`; `App` hands `render` what it needs.

Also settled: `highlight()` keeps its blocking form. There is no worker thread.

Decided while writing this spec:

- **A pager line cap** (§6). A line that costs more than a slice can afford gives up in the
  pager. `--render-once` keeps the full v0.3.5 budget.
- **`render` reads a snapshot** (§4). `render` asks a code source for each block's lines;
  in the pager that source is the `Highlighter`, so a re-layout comes out already coloured.

## 3. Components

| Unit | Lives in | Purpose |
|---|---|---|
| `CodeSource` | `src/render/` | The seam: gives `render` a block's lines and their state. |
| `BlockingSource` | `src/highlight/` | Highlights on a miss with the full budget; memo per instance. |
| `Highlighter` | `src/highlight/` | The pager's source: finished lines, parking lot, slicing. |
| `CodeRow` | `src/canvas/` | Side list: where each drawn code line landed on the canvas. |
| `highlight_tick_at` | `src/tui/` | Picks wanted blocks, runs one slice, patches cells. |

## 4. The seam: `CodeSource`

`RenderOptions` carries a `&dyn CodeSource`. `framed_code` and `fallback` in
`src/render/code.rs` — the only two callers of `bridge::highlight` — ask it for a block
instead of calling `bridge::highlight` and `bridge::outcome`:

```rust
pub(crate) enum CodeState { Highlighted, Plain, Failed, Pending }

pub(crate) trait CodeSource {
    /// The lines to draw for this block and what became of it, in one call.
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> (Cow<'_, [Line]>, CodeState);
}
```

One call returns both, so there is no second lookup between the lines and the outcome
(the race in v0.3.5 where an eviction between `highlight()` and `outcome()` dropped a
caption). The caption is drawn only for `CodeState::Failed`.

`RenderOptions` is part of the render-cache key today. The source is not a rendering
option in that sense and does not enter the key; how it is carried alongside the options
without entering the key is left to the plan.

**`BlockingSource`.** On a miss it highlights with `token_limit_for` (the v0.3.5 budget)
and stores the result in a map held by the instance, behind interior mutability. A block
laid out at several widths during the clip search (`render::document::render_widened`) is
therefore highlighted once per render, as it is today. It never returns `Pending`.

**This retires the process-global `CACHE`** in `src/highlight.rs`, the `outcome()`
function, `computed_count`'s global form and `HIGHLIGHT_GLOBALS_TEST_LOCK`. The free
function `highlight()` keeps its signature and its blocking behaviour; it no longer
memoises. Tests that counted computations count them on a `BlockingSource` instance.

**The pager source** is the `Highlighter` (§5). For a block it has not seen, it records
the key (so the `Highlighter` learns what the document holds) and returns plain lines with
`Pending`. For a block in progress, it returns the coloured lines finished so far followed
by plain lines for the rest, with `Pending`. For a finished block it returns the lines and
`Highlighted`, `Plain` or `Failed`.

Recording a key from `&self` needs interior mutability. `App` and `render` run on one
thread, so a `RefCell` is sufficient.

## 5. `Highlighter`

Owned by `App`. Holds:

- **The theme's `CodeStyles`**, once for the whole map. A theme change clears the map.
- **`blocks: HashMap<(Option<String>, String), Block>`.** A `Block` holds the finished
  lines, the resolved syntax (or `None` for a plain block), the index of the next line to
  parse, and its `CodeState`. Lines and state live in one entry.
- **A parking lot**: at most 4 `(key, ParseState, ScopeStack)` for blocks that are part
  way through. Adding a fifth evicts the one least recently advanced. An evicted block
  keeps its finished lines and its next-line index is reset to 0; when it is resumed, the
  parse runs from line 0 again and the lines it recomputes replace identical ones.
- **A generation mark per block.** After each render, blocks the render did not ask for
  are dropped. The map is therefore bounded by the current document, not by a constant.
  This replaces `MAX_CACHE_ENTRIES` and the whole-map clear above 256 fences (M-8).

The size guards `MAX_HIGHLIGHT_BYTES` and `MAX_HIGHLIGHT_LINES` and syntax resolution
apply when a block is first recorded, exactly as in `highlight_uncached`. A block they
reject is finished at once as `Plain`, with no parse.

The parse loop is the one in `highlight_with`, split so that it can stop after any line
and resume from the parked `ParseState` and `ScopeStack`. The per-line conversion from
scope ops to `Span`s is shared with the blocking path, not copied.

## 6. The pager line cap

The vendored `syntect` can stop a line that exceeds its token limit, but it cannot resume
that line: after `TokenLimitExceeded` the `ParseState` must be discarded. A line can only
be finished by parsing it in one uninterrupted run. Retrying with a larger limit does not
help, because the last attempt still pays for the whole line. One line is therefore the
smallest unit a slice can be.

In the pager, each line is parsed with the limit
`min(token_limit_for(line), PAGER_LINE_TOKENS)`. `PAGER_LINE_TOKENS` is chosen so that one
line costs about 50 ms in a release build; the plan measures tokens per millisecond and
records the constant and its measurement in `docs/maintainer-notes.md`, beside the v0.3.5
budget table. For scale: `jquery.min.js`, one 88,947-byte line, costs 86,535 tokens and
~961 ms, so it gives up in the pager and is highlighted by `--render-once`.

A line that exceeds the pager cap fails the block as in §7. This is the only point at which
the pager and `--render-once` can disagree about a block.

## 7. Scheduling and patching

**First paint.** `App::ensure_rendered` renders with the `Highlighter` as the source. Every
new block comes back plain and `Pending`, and the first screen is drawn before any parse.

**`CodeRow`.** `code_area` records one `CodeRow { block, line, row, col, cols }` per drawn
code row: the block key, the source line index, the canvas position of the line's first
code column, and how many columns are drawn before the clip marker. It is a new canvas side
list, carried like `SearchSpan`: translated by `blit`/`blit_rebased` (and so through
`framed_captioned`, `hconcat` and table cells), merged by `append` and `indent`, and kept
or dropped by row in `slice_rows`. The `block` field is a key the `Highlighter` can look
up; its exact form (a hash of `(lang, src)` checked against the stored key, or an id handed
out by the source) is left to the plan. `check_invariants` covers it.

**What is wanted.** Each tick, `App` builds an ordered list from the `CodeRow`s:

1. blocks with a row in `[scroll, scroll + height)`, top to bottom;
2. then blocks with a row within one viewport height above or below that range;
3. nothing else.

A scroll changes the list on the next tick. A block whose visible rows are far down still
parses from its line 0. When the list holds no unfinished block, there is no work.

**The loop.** In `term.rs`'s `event_loop`, while the `Highlighter` has wanted work, the
wait uses a zero timeout instead of `POLL_INTERVAL`. `highlight_tick_at(app, now)` runs
beside `reload_tick_at`, with the same injected-clock pattern.

**One slice.** Take the first unfinished wanted block; resume it from the parking lot or
start it. Parse line by line under the pager cap until `SLICE_BUDGET` (about 10 ms; the
plan measures and records it) has passed on the injected clock, or until input is waiting,
checked between lines. After each finished line, write it into the `Block` and patch its
cells. After the slice, set `redraw`.

**Patching.** For each `CodeRow` of the line, walk the line's spans and `set_style` their
cells, clamped to `cols` and to the canvas. The gutter and the clip marker are outside
`cols` and keep their style. Plain and highlighted lines both expand tabs with
`expand_tabs`, so their display columns match. Search, selection and hover are drawn over
the canvas at draw time, so they compose with patched cells unchanged.

`RenderCache` exposes only `&Canvas` today; it gains a mutable accessor used only by
patching. Styles do not feed `reach` or `pinned`, so those stay valid.

**Rebuilds.** Resize, reload, option toggles and theme change render again through the
`Highlighter`. Finished and part-finished blocks come out coloured from the snapshot.

**Failure.** A line over the cap sets the block `Failed`, removes it from the parking lot
and drops its finished lines: a failed block is plain everywhere, as in the blocking path.
`App` then invalidates the render cache once, and the next render draws the block plain
with its caption. This is the only event that costs a full re-render.

## 8. The caption (M-7)

`framed_code` and `fallback` draw their bottom-edge caption through one helper. It
shortens the text with an ellipsis to the room on the edge, and styles it with
`theme.block.caption`.

Visible change: `highlighting gave up` was drawn in `theme.code.overflow_marker` and was
cut without an ellipsis at narrow widths. It now matches the Mermaid and math failure
captions. The changelog entry says so.

## 9. Errors

Nothing here may panic on document content.

- Patching clamps every write to `cols` and to the canvas.
- A `CodeRow` whose key the `Highlighter` does not hold is ignored.
- `CodeRow`s are rebuilt with the canvas, so they cannot refer to an older layout.
- A parse error other than the token limit makes the block `Plain`, as in v0.3.5.
- The parking lot and the block map only ever lose work, never correctness: a lost parse
  state restarts the block from line 0.

## 10. Testing

1. **Equivalence.** A fixture document with fences at top level, in a list, in a quote and
   in a table cell, at three widths: the `Highlighter` run until it has no work, with its
   patches applied, gives a canvas equal cell for cell — symbols and styles — to the
   render with a `BlockingSource`. This test guards ruling 1.
2. **Slicing.** The injected clock advances a fixed step per parsed line; a slice stops at
   exactly the expected line.
3. **Order.** The viewport block finishes before any halo block. A block outside the halo
   stays `Pending`. With no work, the loop's timeout returns to `POLL_INTERVAL`.
4. **Parking lot.** A fifth block evicts the least recently advanced. The evicted block
   keeps its coloured lines, and resuming it produces the same lines as an uninterrupted
   parse.
5. **Pager cap.** A line above `PAGER_LINE_TOKENS` but below `token_limit_for` gives
   `Failed` and the caption in the pager, and `Highlighted` with a `BlockingSource`. A test
   seam overrides the cap, as `highlight_with_limit` overrides the budget today.
6. **`CodeRow` transport.** Translated by `blit`, `indent`, `append` and
   `framed_captioned`; filtered by `slice_rows`.
7. **Memory.** After a reload that removes a block, the `Highlighter` no longer holds it.
   A document with more than 256 fences highlights each block once.
8. **Tests without the global lock (M-9).** `a_highlighted_block_says_so` and the other
   tests that relied on the global memo use their own `BlockingSource`, so they cannot be
   disturbed by a concurrently running test.

**Measurement** (a report, not a gate). Time to the first drawn screen on the reference
document, v0.3.5 against this branch: interleaved release runs of binaries built in
separate target dirs, as in branch A. `--render-once` time is reported too and must not
regress.

## 11. Out of scope

- **Resuming a parse inside a line.** Would need a new `syntect` API and an upstream
  change; §6's cap is the answer for now.
- **Keeping colour across a theme change.** Lines would have to store semantic slots
  instead of styles. A theme change recolours the viewport from plain.
- **Highlighting outside the halo while idle.** The halo bounds the work; scrolling brings
  more blocks into it.
- **Changes to the vendored `syntect`.** None are needed.
