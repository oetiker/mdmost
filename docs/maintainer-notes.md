# Maintainer notes

Things that are true about this codebase and are cheap to break by accident. Written
by the people who built each area, kept short on purpose.

## The diagram engine seam is `NodeArt`

`layout::graph` knows about layering, ordering, placement and routing, and nothing about
what is inside a box. Families supply node bodies through one method:

```rust
fn render(&self, node: NodeIdx, budget: u16, theme: &Theme) -> Canvas
```

It measures and paints in a single call, so the two cannot drift, and the engine calls
it again at smaller budgets as it walks the width-degradation ladder. Flowchart, class,
ER and state were all built on this without changing it — resist widening it. When the
engine needed to know where a node's internal compartment rules were, reading them back
off the drawn canvas turned out to be cheaper and more general than adding a method.

## Gantt state is carried by colour alone

Bars are solid `█` everywhere; state is colour plus the legend. An earlier version varied
fill density per state (`░` done, `▒` planned, `▓` critical) and that made the *default*
state the least visible thing on a near-black page, with completed work looking washed
out. Reintroducing per-state densities reintroduces that defect.

## `gantt::time` owns the instant range

Everything entering a timeline passes through `clamp_instant` / `clamp_span`. That is
what makes the arithmetic downstream unable to overflow — a `dateFormat X` timestamp near
`i64::MAX` used to panic three hops later. Do not add a path that skips it.

## Cell width is a contract, not a hint

A `Cell` must draw exactly the width it claims. `check_invariants` asserts
`display_width(cell.text()) == cell.width()` for this reason, and `Cell::new` carries a
matching `debug_assert`.

The subtlety: a grapheme cluster can legitimately occupy more than two columns — a wide
base plus a *spacing* mark (Unicode category `Mc`, not `Mn`) measures three. Clamping
such a cluster to two makes the cell lie, and every row containing one comes out a column
too wide. `text::cell_clusters` splits those; ZWJ sequences and flags already measure
correctly at two columns, so splitting never fires for an emoji.

That clamp was found in three separate places over the project's life. Treat any
arithmetic built on `grapheme_width` as suspect until checked.

## The shared layer is where shared logic goes

`src/text` and `src/canvas` own grapheme-safe width arithmetic, wrapping, truncation,
alignment and box drawing. Every duplication found in review turned out to be a caller
routing around a shared operation that was missing or unreachable — and each workaround
had quietly reintroduced a bug the shared version did not have (`align_offset` clones
used bare subtraction where the shared one saturates). If an operation is missing, add it
here rather than in the caller.

## Verifying your own work

Two failure modes cost real time on this project:

- **Stale binaries.** A shared `CARGO_TARGET_DIR` with several builds in flight will hand
  back a previous binary, so tests report results for code that is not on disk and
  regenerated snapshots can silently revert your own fixes. Run
  `touch src/lib.rs && cargo build` before regenerating a snapshot or trusting a
  surprising result, and re-run a surprising failure in isolation.
- **Tests that cannot fail.** Check every behavioural test in both directions by
  disabling the fix and confirming the test goes red. Several tests here passed with
  *and* without their fix and were only caught this way.
