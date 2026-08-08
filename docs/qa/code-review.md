# mdless code review

Reviewer: QA code agent
Date: 2026-08-08
Findings verified at commit `6546f81`, re-verified verbatim at `bfc27cf`.

## Scope caveat (read first)

Findings were established at committed HEAD `6546f81`. **The entire graph layout engine landed
uncommitted during this review** — `src/mermaid/layout/{graph.rs,flowchart.rs,class.rs,er.rs,state.rs,graph/*}`
plus 16 snapshots were untracked at the time. Layout WIP is excluded per the review brief.

Two transient build failures were observed mid-review (`E0583: file not found for module flowchart`,
and later `E0063`/`E0560` in `glyphs.rs`/`layout`). Both were another agent mid-edit. **Neither is a
defect and neither is reported below.**

Every finding in this document was re-confirmed against `git show HEAD:<file>` at `bfc27cf` and still
stands verbatim — same file, same line, same code.

## Verified gate output

```
cargo fmt --check                          → clean, exit 0
cargo clippy --all-targets -- -D warnings  → clean, exit 0 (forced rebuild, not cached)
cargo test                                 → FAILED, exit 101
    render_property::arbitrary_text_renders_cleanly
    row 0 at width 2 is not exactly 2 columns: left 3, right 2
    minimal failing input: markdown = "𗀀ᩗ", width = 2
    (all other 12 test binaries green: 341 + 14 + 17 + 13 + 3 + 12 + 14 + 4 + 32 + 10 + 14 + 14 pass)
```

**The same suite was green 90 minutes earlier on the same committed code.** That is not flakiness in
the bug — it is flakiness in the gate. See MUST FIX 2.

Note for anyone reading a "339 lib tests pass" status: the failure is in the `render_property`
**integration** binary and is not covered by the lib test count.

---

# MUST FIX

## 1. §4 CANVAS CONTRACT VIOLATION — a grapheme cluster wider than 2 columns overruns the row

**`src/text/mod.rs:57-63`** — foundation code from commit `bdd05f1`, **not** the layout WIP; the
reproducer is plain markdown with no mermaid in it.

```rust
pub fn grapheme_width(cluster: &str) -> u8 {
    match cluster.width() { 0 => 0, 1 => 1, _ => 2 }   // clamps 3+ down to 2
}
```

The doc comment directly above it claims: *"anything `unicode-width` reports as wider is clamped **so
that the canvas contract cannot be violated**"*. The opposite is true. `Cell::new`
(`src/canvas/cell.rs:51-57`) stores the **full cluster text** while recording the **clamped** width,
so a cluster that really occupies 3 columns is booked as 2 and the row over-runs by one.

Reproducer, verified against the built binary:

```
$ printf '\xf0\x97\x80\x80\xe1\xa9\x97\n' > /tmp/t.md   # U+17000 (Tangut, EAW=W) + U+1A57 (Tai Tham, EAW=N)
$ mdless --render-once --width 2 /tmp/t.md              # emits 3 display columns
```

`unicode-segmentation` joins the two code points into one grapheme cluster; `str::width()` reports 3;
we book 2.

**`check_invariants()` cannot catch this.** `src/canvas/mod.rs:360` sums `cell.width()` — the clamped
lie. The invariant checker is structurally blind to exactly this class and will keep certifying
broken canvases.

**Fix — both halves are required:**
- (a) `Canvas::write_str` must reject or replace a cluster with `display_width(cluster) > 2`
  (U+FFFD, or split it);
- (b) `check_invariants` must additionally assert
  `display_width(cell.text()) == usize::from(cell.width())` for every non-continuation cell.

Without (b) the checker remains blind and the class can recur.

## 2. The CI gate is nondeterministic — proptest never persists its failures

Observed test output: `proptest: FileFailurePersistence::SourceParallel set, but failed to find
lib.rs or main.rs`. Proptest's default persistence cannot resolve a source root for tests living
under `tests/`, so **no failing seed is ever written to disk**. `tests/render_property.rs:87` sets
only `cases: 96`:

```rust
#![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]
```

Result: MUST FIX 1 has been in the tree since the foundation commit, and the gate reports green or
red depending on the seed. A red-on-lucky-seed CI is worse than a red CI, because it manufactures
confidence.

**Fix:**
```rust
ProptestConfig {
    cases: 96,
    failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
    ..ProptestConfig::default()
}
```
Commit the resulting `.txt`, and add `𗀀ᩗ` to `tests/corpus/adversarial.md` so the case is pinned
deterministically rather than rediscovered by chance.

## 3. PANIC — integer underflow in the code renderer at terminal width 1–2

**`src/render/code.rs:126`**:

```rust
out.write_str(row, budget - 1, OVERFLOW_MARKER, theme.code.overflow_marker);
```

where `budget = usize::from(width)` (`code.rs:97`).

Verified:
```
$ printf '> ```\n> xxxxxxxxxx\n> ```\n' > /tmp/c.md
$ mdless --render-once --width 2 /tmp/c.md
thread 'main' panicked at src/render/code.rs:126:28: attempt to subtract with overflow
```

Also panics at width 1. Reached whenever `framed_code` takes the `width < 4` bare path
(`code.rs:64`) with `width == 0` — a blockquote gutter (`block.rs:173`, `gutter = 2u16.min(width)`)
or a list marker field (`block.rs:191`) can eat the budget down to zero. In release the subtraction
wraps to `usize::MAX` and the overflow marker silently vanishes instead of panicking; both behaviours
are wrong. Spec §12 calls a panic a release blocker.

**Fix:** `budget.saturating_sub(1)` — or better, fold this into the shared op described in DUP-1,
where the sibling call site already got the guard right.

## 4. PANIC — i64 overflow in the gantt parser from `dateFormat X`

**`src/mermaid/parse/gantt.rs:119`**:

```rust
Some(End::Duration(seconds)) => start + seconds,   // unchecked
```

`parse/date.rs:23-27` accepts an arbitrary `i64` for `dateFormat X`, and `date.rs:158` saturates a
huge duration to `i64::MAX`.

Verified:

````
```mermaid
gantt
dateFormat X
section s
  a : -9223372036854775000, 1d
  b : 9223372036854775000, 1d
```
````

→ `thread 'main' panicked at src/mermaid/parse/gantt.rs:119:45: attempt to add with overflow`, at
every width.

Note the sibling case degrades correctly — a `1e30d` duration is rejected with
``duration unit `e30d`; use ms, s, m, h, d or w`` — so **only the timestamp path is broken**. Do not
chase the duration-unit path.

The same unchecked class appears downstream at `gantt/mod.rs:61,233-234,399` and
`gantt/time.rs:184,190`.

**Fix:** clamp `X` timestamps to a sane instant range in `parse/date.rs:26`, and use
`checked_add`/`checked_sub` at the sites above.

## 5. Spec §7.3 is unimplemented, and a dead public function's doc comment claims otherwise

**`src/render/table.rs:44`** `pub fn render_table_full` — doc comment reads *"The viewport uses this
for horizontal scrolling (design spec §7.3)"*. **It has no non-test caller.**

```
$ git grep -n 'render_table_full' HEAD -- src tests
src/render/mod.rs:52:pub use table::{render_table, render_table_full};   # re-export
src/render/table.rs:44:pub fn render_table_full(                          # definition
src/render/tests.rs:470:    let full = render_table_full(table, 12, &theme, &PLAIN);   # one unit test
```

Consequence: `render_document` renders every table clipped to `width` with a `›` marker
(`table.rs:59-66`), so `App::hscroll_max()` (`app.rs:309`) evaluates to
`canvas.width() - viewport_width()` = **0** unless `--width` forced a wider render. Spec §7.3 says a
table whose minimums exceed the terminal *"becomes horizontally scrollable rather than mangled"* — it
is mangled. Spec §10's `←`/`→` are no-ops in the default case, and the status-bar horizontal offset
indicator (`tui/chrome.rs:188`) can never appear.

**Fix:** either wire `render_table_full` into the block renderer for over-wide tables, or delete it
and amend the spec. The current state — a dead function whose doc asserts a behaviour the product
does not have — is the worst of the three options.

---

# DUPLICATION

Spec §14: *"any duplication between the table renderer, the code renderer and the mermaid renderers
is a defect to be factored into the shared `Canvas` layer."*

**Read the verdict section first.** Nearly every item below is a **missing canvas op**, not a lazy
agent. Adding roughly five operations to `src/canvas/ops.rs` makes most of this list evaporate.

### DUP-1 · "clip to width, stamp an overflow marker" — written twice, one copy guarded, one not

- `src/render/code.rs:119-126` (per line)
- `src/render/table.rs:59-66` (per row)

`src/render/table.rs:22` literally does `use super::code::OVERFLOW_MARKER;` — the table renderer
reaching into the code renderer for a glyph constant is the proof that these are one concept
implemented twice. `table.rs` guards with `if width > 0`; `code.rs` does not — **that asymmetry is
MUST FIX 3**.

**Winner: neither.** New `Canvas::clip_with_marker(&mut self, width: u16, marker: &str, style: Style)`
in `src/canvas/ops.rs`, beside its existing sibling `push_text_ellipsized` (`ops.rs:284-292`). Both
call sites collapse to one line, the `width == 0` guard exists once, and the marker glyph stops being
owned by `code.rs`. **~14 lines, and the panic dies structurally.**

### DUP-2 · Howard Hinnant's `days_from_civil` implemented twice, verbatim

- `src/mermaid/parse/date.rs:113-121` — private, `i64` month/day
- `src/mermaid/gantt/time.rs:147-156` — public, `u32` month/day, clamped

Same algorithm; `era` differs only as `if year >= 0 { year } else { year - 399 } / 400` versus
`year.div_euclid(400)`, which are equivalent. `const DAY` is duplicated too (`date.rs:13` vs
`time.rs:18`), and `date.rs:145-147`'s `3600.0` / `60.0` literals restate `time::HOUR` / `time::MINUTE`.

**Winner: `gantt/time.rs`** — public, documented, and covered by a round-trip property test. Have
`date.rs` import it with a `u32::try_from` at the boundary. **~20 lines.** (Both bodies were compared
line by line.)

### DUP-3 · Eighth-block bar rendering, twice

- `src/tui/icons.rs:74-98` — `EIGHTHS` + `meter`, `f32`
- `src/mermaid/chrome.rs:20-84` — `EIGHTH_BLOCKS` + `eighth_bar` + `eighths_of`, `f64`

Identical glyph table, identical full/remainder arithmetic.

**The fix is not "tui calls mermaid chrome".** `chrome.rs` is documented as *mermaid chart furniture*
and a status-bar meter is not that; proposing that will and should be rejected. Move the glyph table
and the fraction→eighths maths to a neutral home (`src/text` or `src/canvas`) and have both callers
use it. **~25 lines.**

### DUP-4 · `align_offset` re-implemented four times, because the shared one is private

`src/canvas/mod.rs:382` is `fn align_offset`, **not `pub`**:

```rust
fn align_offset(field_width: usize, content_width: usize, align: Align) -> usize {
    let slack = field_width.saturating_sub(content_width);
    match align { Align::Left => 0, Align::Center => slack / 2, Align::Right => slack }
}
```

Clones outside `src/canvas/`:
- `src/render/table.rs:258-263` — the closest, a full three-arm match
- `src/mermaid/chrome.rs:51` — `let left = usize::from(width - body.width()) / 2;`
- `src/mermaid/sequence/mod.rs:251` — `let left = low + 1 + (room - display_width(&text)) / 2;`
- `src/mermaid/layout/graph.rs:139` — `let left = slack / 2;` (layout WIP)

This is precisely the table↔mermaid triangle §14 names. The clones exist **only** because callers
cannot reach the shared one.

**Fix: make it `pub(crate)`,** then delete four clones. **~14 lines, and the cheapest win on this
list.**

### DUP-5 · `chrome::fit` vs `Canvas::push_text_ellipsized`

`src/mermaid/chrome.rs:91-99` and `src/canvas/ops.rs:284-292` implement the same
truncate-to-`width - 1`-plus-`…` logic in two homes, with two ellipsis constants. Canvas should own it.

### DUP-6 · Empty-diagram placeholder, three copies

- `src/mermaid/pie.rs:62-68` — `fn empty_body`
- `src/mermaid/gantt/mod.rs:67-73` — `fn empty_body`, character-identical apart from the text constant
- `src/mermaid/sequence/mod.rs:73-77` — the same five lines, inlined into `draw`

All three do `chrome::fit(TEXT, width)` → `u16::try_from(display_width(..))` → `Canvas::new(cols, 0, base)`
→ `push_text(.., Align::Left, theme.text.dim)`.

**Winner: a new `chrome::placeholder(text: &str, width: u16, theme: &Theme) -> Canvas`.** **~15 lines**,
and it makes the three empty-state renderings provably identical.

### DUP-7 · Even-slack distribution, twice

- `src/render/table.rs:323` — `fn spread(widths: &mut [usize], slack: usize)`
- `src/mermaid/sequence/columns.rs:399-404` — inside `satisfy`:
  ```rust
  let share = deficit / gaps; let extra = deficit % gaps;
  for (offset, slot) in distance[range].iter_mut().enumerate() {
      *slot += share + usize::from(offset < extra);
  }
  ```

Same algorithm. Another exact table↔mermaid §14 hit.

**Winner: one `distribute_evenly(&mut [usize], extra)` in `src/text`.** **~10 lines.** Note that
`pie::apportion`'s largest-remainder loop (`pie.rs:340-354`) is *genuinely different* — it is a
reporting rule, not a layout rule — and should stay where it is.

### DUP-8 · The gantt tick nudge computed twice, with *different* formulas

A live drift hazard, not merely duplication.

- Draw: `src/mermaid/gantt/mod.rs:155-160`
  ```rust
  let left = at.saturating_sub(span / 2)
               .clamp(columns.plot_start(), columns.total().saturating_sub(span));
  ```
- Thinning: `src/mermaid/gantt/mod.rs:424-428`
  ```rust
  let left = tick.column.saturating_sub(span / 2).min(plot.saturating_sub(span));
  ```

One clamps into `[plot_start, total - span]` in content coordinates; the other `min`s into plot-local
coordinates with no lower bound. The doc comment at `mod.rs:414-416` admits that `thin` only *"models
that same nudge"*.

**Winner: one `fn tick_left(column, span, columns) -> usize` used by both.** **~6 lines**, and it
removes a real off-by-one class between *which* ticks are kept and *where* they are drawn.

### DUP-9 · Duplicated parser helpers

`src/mermaid/parse/lex.rs:1-6` explicitly declares reimplementing its helpers a §14 defect.

- Note-placement prefix parsing: `parse/sequence.rs:148-161` ≡ `parse/state.rs:215-226` — identical
  `to_ascii_lowercase()` + `strip_prefix("left of"/"right of")` + `&rest[rest.len() - after.len()..]`
  byte dance; state omits `over`. → `lex::split_note_placement`. ~14 lines.
- `" as "` splitting: `parse/sequence.rs:302-306 split_alias` ≡ `parse/state.rs:352-356 split_as` —
  byte-identical bodies, different names. → `lex::split_as`. ~6 lines.
- `<<…>>` stereotype extraction: `parse/class.rs:256-267 split_annotation` ≈ `parse/state.rs:150-159`.
  ~8 lines.

### DUP-10 · `chrome::lines_width` / `label_natural_width` re-inlined

- `src/mermaid/pie.rs:117-126` rewrites `.map(String::as_str).map(display_width).max().unwrap_or(0)` —
  byte-identical to `chrome::lines_width` (`chrome.rs:117-124`).
- `src/mermaid/sequence/columns.rs:290-298` rewrites the same expression over `note.text.lines` —
  that is `chrome::label_natural_width` (`chrome.rs:127-135`).

The second one is a straight miss rather than a deliberate divergence: `columns.rs:219-223` calls
`chrome::label_lines` and `chrome::lines_width` correctly, three lines away.

Separately, `chrome::label_natural_width` is itself just `lines_width` applied to `label.lines` — a
duplication *within* `chrome.rs`.

### DUP-11 · Gantt reimplements its own tested calendar helpers

`src/mermaid/gantt/mod.rs:361-373` (`aligned_month_index`, `epoch_of_month_index`) restates
`time::add_months` and `time::month_start` (`time.rs:178-191`) — `at.year * 12 + i64::from(at.month) - 1`,
`div_euclid(12)`, `rem_euclid(12) + 1`, `days_from_civil(..) * DAY`.

`grep -rn 'add_months\|month_start' src/` shows **both public helpers are called only from their own
unit tests**. Live code reimplements the dead-but-tested API. **Winner: `time.rs`** — express
`epoch_of_month_index` via `month_start`/`add_months`, or delete the unused pair. ~12 lines.

### DUP-12 · Two independent Nerd-Font glyph modules

- `src/render/glyphs.rs` — `Glyphs::PLAIN` (`:35`) / `Glyphs::NERD` (`:49`) / `Glyphs::new(icons: bool)` (`:70`)
- `src/tui/icons.rs` — `Icons::NERD` (`:34`) / `Icons::PLAIN` (`:52`) / `Icons::new(nerd_font: bool)` (`:66`)

Both module docs state the same invariant in the same words — *"plain Unicode of the same display
width"* (`glyphs.rs:4-5`, `icons.rs:4-5`) — but **only `glyphs.rs` enforces it**:
`glyphs.rs:169 every_glyph_is_exactly_one_display_column` asserts `grapheme_width(glyph) == 1` for all
21 glyphs. `icons.rs` has no test at all, so `icons.rs:35 file: "\u{f0219}"` (a Plane-15 private-use
code point) and nine siblings are unverified — while `tui/chrome.rs:264-268` sizes the status bar with
`display_width`, so one double-width icon silently shifts every right-hand segment. This is exactly
the failure `icons.rs`'s own comment worries about.

Same boolean, two parameter names (`icons` vs `nerd_font`).

**Winner: `render/glyphs.rs`** — it owns the invariant and the test. Move both sets under one module
and extend `all()` to cover `Icons`. **~30 lines**, and the width assertion gains ten unchecked glyphs.

### DUP-13 · `content_width` and `viewport_width` share a duplicated body

`src/tui/app.rs:319-324` and `:332-338` are byte-identical:
`self.size.0.saturating_sub(self.toc_width()).saturating_sub(1).max(1)`.

The first should read `self.options.width.unwrap_or_else(|| self.viewport_width())`.

### DUP-14 · Hand-drawn box art where the shared layer nearly serves

- `src/mermaid/sequence/mod.rs:287-302` stamps a hollow rectangle corner by corner.
- `src/render/table.rs:156-177` (`border_row`) and `:217-233` (`render_row`) hand-draw table borders,
  including `for _ in 0..width + 2 { text.push(set.horizontal) }` — which is
  `text::repeat_to_width` (`text/mod.rs:127`) — and a `display_width` round-trip at `:175` that
  re-measures a string whose width the function just computed by construction.
- Meanwhile `render/code.rs:72` and `render/block.rs:286` correctly use `Canvas::framed`.

`framed()` genuinely cannot serve either case: it *wraps* a canvas rather than stamping over existing
ink, and it has no tees or crosses. The right move is two new canvas ops beside `framed`:
`Canvas::rect(top, left, w, h, BorderSet, style)` and
`Canvas::grid_border_row(&widths, left, middle, right, set, style)`.
**~35 lines out of `table.rs`**, and the incoming `layout/graph/frame.rs` needs `rect` too.

**Realistic total across DUP-1..14: ~200 lines, plus two silent-drift hazards (DUP-2, DUP-8) removed.**

---

# SHOULD FIX

- **`[toc] open` in `config.toml` does nothing.** `config.rs:50` declares it, `config.rs:273`
  populates it, and **nothing reads it**: `main.rs:134` builds `AppOptions { toc_open: cli.toc }` from
  the CLI flag alone. Every other config field is threaded correctly (`icons` main.rs:112,
  `line_numbers` main.rs:116, `toc_width` app.rs:265, `mouse` term.rs:50, `scroll_step` app.rs:829).
  **Fix:** `toc_open: cli.toc || config.toc_open`. A pure seven-agent seam — the config workstream
  defined the field, the CLI workstream wired only the flag.
- **`src/lib.rs:30-34` is stale and actively misleading:** *"The remaining modules are placeholders
  owned by other workstreams; they exist so the module map is visible."* All seven are fully
  implemented (~14 000 lines). Delete the paragraph.
- **The crate's central rule is stated wrong in three of four places.** `lib.rs:5-7`,
  `render/mod.rs:18-19` and `tui.rs:12` all say `(AST, width, theme)` — omitting `options` — while
  `render/mod.rs:63` and `cache.rs:3` state the correct 4-tuple. Anyone implementing a cache from
  `lib.rs` gets `--no-icons` wrong. (The actual `RenderCache` key is correct: `cache.rs:20-30` has all
  four inputs, and `render_document`'s signature matches spec §3.1 exactly.)
- **`tui/chrome.rs:380` pads by char count, not display columns:**
  `format!("{:>width$}", row.keys, width = key_width)` where `key_width` comes from `help.rs:66`'s
  `display_width`. Rust's `{:>N$}` pads to N *chars*. Any wide-char key binding ragged-edges the whole
  help column. **Fix:** `text::pad_to_width(&row.keys, key_width, Align::Right)`. Same latent pattern,
  currently ASCII-safe: `render/code.rs:115`, `tui/chrome.rs:178`.
- **`tui/chrome.rs:119` is the only non-grapheme text path in the crate:**
  `for (index, ch) in text.chars().enumerate()`. A TOC heading containing `e` + U+0301 puts base and
  combining mark in different `TermSpan`s, both misdrawing and mis-widthing the entry. Everything else
  — `write_str`, `wrap_spans`, `Cell::append_zero_width` — is grapheme-based specifically to avoid
  this. Fix requires `toc.rs::fuzzy_match` to emit grapheme indices.
- **`tui/chrome.rs:91-100`: TOC entries clip silently, with no ellipsis.** `prefix` is up to 10
  columns; `draw_toc` only guards `area.width < 4`, so `room` saturates to 0 and a deep heading
  renders as a blank row with no indication text was dropped. Third variant of DUP-1's concept.
- **`render/block.rs:303-305`: unclamped `indent` in `hanging`.** `heading` (`:143`) and `list`
  (`:192`) clamp with `.min(width)`; `hanging` does not, so a large footnote label allocates a canvas
  that wide (`ops.rs:174` `self.width + left + right`, a plain `u16` add) before it is resized back
  down.
- **`canvas/ops.rs:159-160`: unchecked `u16` sum in `hconcat`** —
  `parts.iter().map(Canvas::width).sum::<u16>() + gap * ...`. The sibling at `ops.rs:110` uses
  `u16::try_from(..).unwrap_or(u16::MAX)`; the defensive idiom exists, it just was not applied here.
- **`SearchError` (`search.rs:41-43`) is the one library error `crate::error::Error` cannot absorb** —
  there is no `Search` variant in `error.rs:12-41`, and `app.rs:768` consumes it as a bare string.
  Everything else routes through `error.rs` correctly, with `anyhow` properly confined to `main.rs`
  per §14.
- **`config.rs:180-187 theme_names()` does not deduplicate**, and `next_theme_name` (`:190-197`) uses
  `position()` (first hit) — a user theme named `dark` puts `"dark"` in the cycle twice and stalls or
  skips the `t` cycle.
- **`tui/draw.rs:22` clones the whole `Theme` every frame** (15 colours + ~80 `Style` slots + a
  `String`) at the 120 ms poll cadence, purely to dodge the borrow checker. `cache.rs:63` also
  allocates a `String` on every `refresh`, including cache hits.
- **`mermaid/mod.rs:48-52`**: `TODO(dispatch)` with four families returning `UnsupportedFamily` while
  `layout/{flowchart,class,er,state}.rs` define matching `draw` functions. Expected per the review
  brief — flagged only so the four match arms are not forgotten when the WIP lands.
- **Two `unreachable!` in `parse/state.rs:66,305`** — the same construct as `unwrap`, which §14 forbids
  outside tests. Provably guarded today, so latent rather than live.
- **`gantt/mod.rs:188` `columns.gutter - indent`** is safe only because `MIN_GUTTER = 6` exceeds
  `TASK_INDENT = 2`, documented nowhere. **`gantt/mod.rs:159`'s `.clamp(plot_start, total - span)`
  panics if min > max** and is safe only via an equality-at-the-boundary guarantee from `chrome::fit`.
  Both want `saturating_sub` and a comment binding the constants.
- **`plan.rs:127-129` returns `0` when `centers` is empty**, which would make `plan.rs:138
  self.open[participant]` an out-of-bounds panic; prevented solely by the early return at
  `sequence/mod.rs:72-78`. Worth a `debug_assert!(!centers.is_empty())` in `plan::build`.

## Test quality (§13)

**The suite is honest.** Every commit touching `tests/` and `src/**/tests.rs` was read. No `#[ignore]`
anywhere, no `==`→`contains`/`>=`/`is_ok` loosening, no reduced proptest budgets, no expected values
bent to match buggy output.

- **`6546f81` (pie rounding) is a model fix, not a weakening.** The diff was read directly. The old
  test carried an *excuse* for the bug —
  `// 2.125 is exactly representable, so formatting rounds half to even;`
  `assert_eq!(format_value(2.125), "2.12")` — replaced by a `round_half_away` implementation, a
  rule-level test (`rounding_is_half_away_from_zero`), and largest-remainder apportionment so printed
  percentages sum to exactly 100.0%. The snapshot deltas (`66.7%`→`66.6%`, `0.12`→`0.13`) are the
  legitimate consequence. **Strengthened, not loosened.**
- **`190aaf4` deleting `tests/mermaid_eyeball.rs` (220 lines) reads like a mass deletion in a stat
  listing — it is not.** That file's own header called it a temporary visual harness; its single test
  contained `println!` calls and **zero assertions**. The same commit added 911 lines of asserted
  tests and 43 reviewed snapshots.
- **`tests/snapshot.rs` still snapshots a placeholder.** `render_at` (`:79-83`) carries its own
  confession: *"**Placeholder.** The block renderer does not exist yet, so this shows the document
  outline in a framed box built from the foundation layer."* That is false now —
  `tests/render_property.rs` already imports and uses `render_document`. The consequence is that the
  repo's only document-level golden snapshot, `tests/snapshots/snapshot__adversarial.md@80.snap`, is a
  list of headings in a box. **Nothing in §13.2 is actually snapshotted** — not nested tables, not
  Markdown inside cells, not deep lists, not mixed scripts, not any Mermaid family — even though
  `tests/corpus/adversarial.md` contains every one of them. `corpus_renders_are_stable_at_every_width`
  is a green test that proves nothing about the renderer. Also absent from §13.2: the 1000-node graph
  and the single-node graph.
- **`portable-pty = "0.9.0"` is a declared dev-dependency with zero usages anywhere** in `src/` or
  `tests/`. §13.4's pty resize test was planned and dropped. The `$PAGER` path is likewise untested —
  `grep -rn PAGER src/ tests/` hits only a doc comment in `main.rs:8`.
- **§13.3: two of four properties are missing.** Idempotence is not asserted — `render_property.rs:47`
  asserts *determinism* (same input rendered twice), which is weaker and different. Text round-trip
  does not exist at all (`grep -rni 'round.trip'` returns only unrelated hits).
- **The proptest generator is the weak link.** `render_property.rs:60`:
  `let text = "[ -~\u{00e9}\u{4e2d}\u{1f600}]{0,40}";` — ASCII, `é`, `中`, `😀`. No ZWJ sequence, no
  combining mark, no zero-width character, no RTL script: four of the five categories §13.1 names.
  The adversarial *corpus* does cover them at all 120 widths × 4 option sets, so the invariant is
  genuinely stress-tested; what is not tested is those graphemes in randomly generated **structural**
  positions (inside a negotiating table cell, at a hard-wrap boundary). A one-line change to the
  character class.
- **`tests/lead_smoke.rs:61 malformed_input_errors_without_panicking` asserts nothing** — ten malformed
  inputs, `let _ = parse(src)`, not one checked to be `Err`. `""` and `"flowchart"` could parse into
  empty successes and it still passes. The name promises more than the body delivers.
- **`render/tests.rs:319`**: `assert!(!out[start..].join(" ").is_empty())` where `start` is valid by
  construction — a tautology. (The three preceding assertions in that test are real.)
- **The pie tie-break test asserts a guarantee the code does not provide.** `pie.rs:470-475` states
  *"Equal values must always hand the leftover tenth to the earliest slice"*, but `pie.rs:346`'s
  `fraction_b.total_cmp(&fraction_a).then(a.cmp(b))` reaches the index tiebreak only when the f64
  fractions are **bit-identical**. In the `fractional` fixture (1.25 / 0.5 / 0.125 over 1.875) all
  three exact shares are mathematically ⅔-recurring but differ in their last bits, so the *smallest*
  slices win the leftovers and the largest prints **66.6%** where 66.7% is the natural rounding — and
  that snapshot is committed. Not dishonest; the test documents a stronger rule than the
  implementation has. **Fix:** compare remainders with a tolerance, or apportion in exact integer
  arithmetic.

**Positive, and verified rather than assumed:** the 88 committed mermaid `.snap` files were genuinely
reviewed — an independent display-width checker over all of them found every body row exactly its
declared width, with no garbled box art, no mojibake, no misaligned tables, and no
degenerate-where-content-was-expected. (Apparent oddities checked and cleared: the square box in
`mermaid_seq_render__one_participant@40.snap` is a `Note`, not a participant; the doubled accent in
`mermaid_pie_render__cjk_emoji@40.snap` is the deliberate `"café\u{301}"` input.) The per-family
`snapshot()` helpers assert `canvas.width()`, `check_invariants()` **and** per-row `display_width`
*before* snapshotting. `render_property.rs:41` uses `==`, not `<=`.

---

# NIT

- Dead public API: `tui/cache.rs:81 invalidate` (no callers), `doc/mod.rs:307 heading(&id)`,
  `doc/mod.rs:64 SourceSpan::contains`, `config.rs:87 Loaded.path` (written at three sites, read
  nowhere — and `config.rs:127-131` yields `path: None` for a missing file even though the path was
  known), `config/keys.rs:574,591,596`, `theme/mod.rs:62 Palette::accents`,
  `gantt/time.rs civil_from_days` / `add_months` / `month_start`.
  **Note:** `mermaid::render_diagram` was flagged as dead during the review and **that is wrong** — it
  is called from `mermaid/mod.rs:31`. Do not delete it.
- Wider-than-needed but used, should be `pub(crate)` or private: `App::viewport_width`, `scroll_by`,
  `reveal`, `notify`, `clear_notice`, `start_toc_filter` (`app.rs:332,456,474,487,495,858`);
  `Config::default_path` (`config.rs:105`).
- `App::set_icons` (`app.rs:213`) is called only from tests — no `Action` toggles icons at runtime, so
  `--no-icons` is start-up-only and this is test-only API.
- Three names for one quantity: `width` / `budget` / `cols`, sometimes in the same function
  (`code.rs:97`, `pie.rs:109`, `gantt/mod.rs:101`, `table.rs:108` all do
  `let budget = usize::from(width)`).
- Two names for one verb: mermaid families expose `draw`, `render::*` exposes `render_*`, and
  `table.rs:93` has a private `fn draw` inside the `render_*` module. Two distinct functions named
  `render_mermaid` (`mermaid/mod.rs:30`, `render/bridge.rs:30`). The module *containing* the mermaid
  `draw` functions is called `layout`, while `pie`/`gantt`/`sequence` live outside it and do the same
  job.
- `render::inline::wrap` (`inline.rs:28`) is a bare re-export of `wrap_spans` — mandated by spec §5,
  but noted.
- `render/inline.rs:126-129` re-implements `Canvas::from_lines` (`canvas/mod.rs:101-107`) character for
  character. 3 lines.
- `render/code.rs:132-140 digit_count` hand-rolls `ilog10`.
- `Canvas::hline` is a documented alias of `Canvas::fill`.
- `term.rs:52` `let _ = execute!(stdout(), EnableMouseCapture)` — with `mouse = true` configured, a
  failure means the mouse silently does nothing, with no notice to the user.
- `parse/date.rs:26,53` `.parse().ok()` / `.ok()?` — a malformed date silently becomes "no date"
  rather than a `MermaidError::Syntax` with a line number, unlike the rest of the mermaid parser.
- `SIGINT` is not registered (`term.rs:46-47` registers only SIGTERM/SIGHUP) — safe only because raw
  mode delivers `Ctrl-C` as a key, so a `kill -INT` from another terminal leaves the alt screen up.
- On panic the terminal is restored **twice** (hook + `Restore` drop), writing stray escapes to the
  normal screen. Benign, and the comment at `main.rs` correctly argues restoring twice beats not
  restoring — noted only for completeness.
- `config.rs:453-462 line_of_key` falls back to `unwrap_or(1)`, so a wrong line number is reported
  with the same confidence as a right one, and is indistinguishable from a genuine line-1 error.
- `sequence/columns.rs:256` `#[allow(clippy::too_many_arguments)]` — the crate's **only** `allow`.
  Acceptable, but `render/` solved the same problem two modules over with the `Ctx` struct; a small
  parameter struct would remove it. That inconsistency is itself a mild seam.
- `layout/graph/node.rs:229` `assert!(ports.top.is_empty() || w > 2)` inside a loop where
  `w ∈ {0,1,2}` — the clause is dead. (Layout WIP; noted only in passing.)

## Clean, stated affirmatively

`#![forbid(unsafe_code)]` holds — no `unsafe`, no FFI, no `transmute`, nothing evades it. Zero
`unwrap()` / `expect()` / `panic!` outside `#[cfg(test)]`. `use unicode_*` appears in exactly two
lines, both in `src/text/mod.rs` — no module reaches past the shared text layer. One `TODO`; no
`FIXME`, `XXX`, `HACK`, `todo!`, `unimplemented!`, `#[allow(dead_code)]`, or "for now". No
`partial_cmp` anywhere in `src/mermaid/` — every float sort uses `total_cmp`; every division has a
zero guard and each guard is tested. No unguarded slice indexing. Terminal restoration (§12) is
correct: the panic hook installs *before* `ratatui::try_init`, so ratatui's hook restores and then
chains to ours before the default hook prints the message; `Restore` covers normal return and `?`;
and `main.rs:79` covers the one init-failure window that escapes both. Argument order is consistent
across ~40 render entry points — `(subject, width, theme[, options])`, with no `(theme, width)`
inversion anywhere. The u16-at-the-canvas-boundary / usize-inside convention is coherent and
deliberate, with conversions clustered exactly at the seam. `render_document` matches spec §3.1
exactly. **No layout decision at parse time and no width-dependent value in either AST** — `src/doc`
and all 908 lines of `src/mermaid/ast.rs` were checked field by field: `Label.lines` is `<br>`
semantics not wrapped text, `GanttTask.start/end` are instants not columns, `PieChart.slices` keeps
declaration order with sorting deferred to render. No renderer retains state across calls
(`grep` for `static|OnceLock|lazy|RefCell|Mutex` in `src/mermaid/` returns nothing); every `&mut self`
is a per-call builder constructed and consumed inside one function. Resize genuinely discards and
re-renders rather than patching.

---

# Verdict

**One coherent system with visible seams — closer to the former than expected.**

This does not read as seven agents' work stapled together, and the review went looking hard for that.
The evidence for coherence is unusually strong: `src/text` really is the single home for width and
grapheme logic — two `use unicode_*` lines in the entire crate, and not one module re-rolled
`pad_to_width`, `truncate_to_width` or `repeat_to_width`. Zero `unwrap` outside tests, one
`#[allow]`, one `TODO`. Argument order never inverts across ~40 render entry points. `total_cmp`
everywhere. The three chart families independently converged on the same `struct Columns` +
`negotiate(chart, width) -> Result<_, TooNarrow>` shape — that is shared *taste*, not copy-paste, and
it is the strongest positive signal in the codebase.

Where it shows its origin is at the **edges nobody owned**:

**1. The shared layer was under-built, so callers routed around it.** `align_offset` is private → four
clones. `Canvas` has `framed` but no `rect` and no `clip_with_marker` → three hand-rolled
implementations and MUST FIX 3. `table.rs` importing `OVERFLOW_MARKER` from `code.rs` is the tell:
someone knew it should be shared and had nowhere to put it. **Nearly every duplication above is a
missing canvas op, not a lazy agent.** Add roughly five ops to `src/canvas/ops.rs` and most of the DUP
list evaporates. This reframes the DUP list from cleanup into a design fix, and it should be done
first, because every day it waits another renderer clones something.

**2. The seams between workstreams are where things are simply not connected.** `[toc] open` parsed
but never read. `render_table_full` written but never called — and its doc comment asserts a §7.3
behaviour the product does not have. `portable-pty` added but never used. `tests/snapshot.rs` still
rendering a placeholder outline. `time::add_months` tested but bypassed by live code that
reimplements it. In each case one agent built a half and the other never arrived.

**3. The documentation drifted from the code it describes.** `lib.rs` tells readers seven implemented
modules are placeholders. Three of four statements of the central architectural rule omit `options`.
Two doc comments assert guarantees that are false — `grapheme_width`'s *"the canvas contract cannot be
violated"* and `render_table_full`'s *"the viewport uses this"*. For a project whose spec is the
coordination mechanism, false doc comments are the most dangerous defect class present: **MUST FIX 1
hid behind one for the entire project.**

## What would have to change

- (a) Fix the two panics and the canvas contract, and make `check_invariants` measure cell *text*
  width so it can catch that class at all.
- (b) Pin proptest failures to disk — the gate is not trustworthy until a red run stays red.
- (c) Add the missing canvas ops and delete the ~200 duplicated lines.
- (d) Connect the four dangling seams, or delete both halves of each.
- (e) Do one editing pass over doc comments that assert behaviour, because several of them currently
  lie.

None of that is a rewrite. This is a coherent codebase with an under-provisioned shared layer and four
unfinished handshakes.
