# Wide diagrams scroll instead of dumping source

Design doc. Status: approved 2026-08-09. Supplements the main design spec
(`2026-08-08-mdless-design.md`) §7.3 and §8, which promise horizontal scrolling
for over-wide blocks; this extends that promise to diagrams.

## The problem

A Mermaid diagram that will not fit the pane degrades through the layout
engine's fit ladder, and when the ladder is exhausted the block falls back to a
syntax-highlighted dump of its own Mermaid source
(`docs/qa/visual-review-3.md` §1, the review's worst finding).

In the pager this is worse than it sounds. `tui::wide::render_scrollable`
widens any block that comes back clipped, so the *source dump* is what gets
widened, and the reader side-scrolls through raw Mermaid:

```
 ╰ needs at least 88 columns to draw — this block has 81 ───›
 after 20× →   ‹mber 3 of the pipeline] │
```

The machinery to show the whole thing is already there and aimed at the wrong
artifact. Tables and long code lines already scroll correctly; diagrams are the
hole.

## The decision

**Degrade first, then scroll.** The fit ladder keeps its job: a diagram that can
be made to fit is made to fit, exactly as today, with no scrolling. Only when
the ladder is exhausted does the pager draw the diagram at its natural width and
let horizontal scrolling reach it. Source dumping stops being what a narrow
terminal produces and becomes what a *broken* diagram produces.

Two alternatives were considered and rejected:

- *Always draw at natural width.* Diagrams would always look their best, but a
  chart that fits at 80 today would start demanding scrolling. Fit is worth more
  than maximum size.
- *A key that expands the diagram under the cursor.* Costs a binding, an overlay
  mode and "which diagram am I on?" tracking, to make optional something that
  should simply happen.

## Discoverability

A widened diagram announces itself with the same `›` / `‹` chevrons a wide table
uses. **This costs nothing to build**: the chevrons are painted by the viewport
(`src/tui/draw.rs`, `LEFT_MARKER` / `RIGHT_MARKER`), per row, whenever the
document canvas is wider than the visible slice. They are not painted by the
renderers. A widened diagram inherits them the moment it is widened, and the
status bar's existing horizontal-offset readout does the rest.

Note the distinction, because the two are easy to confuse: `wide::OVERFLOW_MARKER`
is the glyph a *renderer* paints inside a block it had to clip, and it is what
`ClipTest` looks for when deciding to widen. The viewport's chevrons are a
different mechanism with the same glyph.

## Architecture

### The seam

One new function in `render`, because that is where Mermaid lives and where
`MermaidError` is interpreted:

```rust
/// The narrowest width, at least `from`, at which this block's diagram draws.
///
/// `None` when the node is not a Mermaid fence, and when a fence fails for any
/// reason other than being too narrow.
pub(crate) fn diagram_width(
    node: &Node,
    from: u16,
    theme: &Theme,
    options: &RenderOptions,
) -> Option<u16>
```

The `None` cases are load-bearing. A syntax error, an unsupported family and an
internal error must keep dumping source at viewport width: widening them would
show the reader an enormous canvas of the same unusable dump. **Only
`MermaidError::TooNarrow` earns a wider canvas.**

`render_code_block` already routes Mermaid fences through `bridge::mermaid` and
turns any `Err` into `fallback`; `diagram_width` asks the same question of the
same bridge without building the fallback.

### The search

`MermaidError::TooNarrow` carries `needed`, the narrowest drawing the engine
managed at the attempted width — a floor, added for `visual-review-3.md` §12. It
is the search's starting point, so the search jumps to the width the engine
itself asked for instead of doubling blindly up from the viewport.

**The search scans for the first success. It must not bisect.** Fit is not
monotone in width: the probe chart in `docs/qa/visual-review-3.md` §1 draws at
inner widths 61–65, *fails* at 66, and draws again at 67, because
`budget = width / share` quantises and one more column can hand every node a
wider budget that overshoots. A bisection assumes a single crossing point and
there is none.

Each failed attempt returns a fresh `needed`, so the scan jumps rather than
stepping one column at a time:

1. `at = max(from, needed_at_from)`.
2. Try to draw at `at`. On success, return `at`.
3. On `TooNarrow { needed }`, set `at = max(at + 1, needed)` and repeat.
4. Give up at `MAX_BLOCK_WIDTH` (the existing bound in `tui::wide`), returning
   `None` so the block dumps source as it does today.

Step 3's `at + 1` is what guarantees termination when a fresh `needed` comes
back no larger than the width just tried — which the non-monotonicity makes
possible.

### The plug-in point

`tui::wide::render_widened` today renders the block at viewport width and, if
the canvas carries the renderer's clip marker, hunts for a width that does not
clip. One branch goes in front of that:

```
if let Some(at) = diagram_width(node, width, ...) && at > width {
    return render_block(node, at, ...);
}
```

Nothing downstream changes. A part wider than the viewport is exactly what a
wide table already produces: `Canvas::append` pads the narrower parts, anchors
and search spans are translated, `hscroll_max` picks up the surplus, and the
viewport paints the chevrons.

## What does not change

- A diagram that fits at any ladder rung draws exactly as it does now, with no
  scrolling. The new branch is inert for it, and the suite being unchanged is
  the check that this is true.
- Piped `--render-once` goes through `render_document`, not `render_scrollable`,
  so it keeps degrading and then dumping source. There is nothing to scroll in a
  pipe, and a canvas clipped at the pipe's width would be strictly worse than
  readable source.
- Syntax errors, unsupported families and internal errors: source dump plus
  caption, untouched.
- The "needs at least N columns to draw" caption survives for the piped path and
  for a diagram that will not draw even at `MAX_BLOCK_WIDTH`.
- The document margin (`render::DOCUMENT_MARGIN`) applies as it now does in
  `render_scrollable`: the widened diagram's surplus extends past the right
  margin, which is what the scroll reaches.

## Testing

Every behavioural test is proved red before its fix (design spec §13, and this
repo has passed tests both with and without their fix more than once).

1. **The headline case.** The seven-node `flowchart LR` from
   `visual-review-3.md` §1, at a viewport narrow enough to exhaust the ladder:
   `render_scrollable` returns a canvas wider than the viewport whose plain text
   contains the node labels drawn as box art and does **not** contain the string
   `flowchart LR`.
2. **The guard.** A fence with a syntax error, and one with an unrecognised
   family, still come back at exactly viewport width, still as a source dump
   with their caption. This is the test that stops the feature widening the
   failures it must not widen.
3. **Inertness.** A diagram that already fits is byte-identical to today. The
   existing snapshots carry most of this; one explicit assertion that
   `render_scrollable` returns exactly viewport width for a fitting diagram
   makes the intent legible.
4. **Honesty.** The width the search chose must actually draw — the same shape
   as the §12 floor test. A width that is reported but does not draw is worse
   than no width, because it is acted on.
5. **Non-monotonicity.** A case that draws at `w`, fails at `w+1` and draws at
   `w+2` must still terminate and return a drawable width. If no natural case
   can be pinned down stably, a unit test over the search function with a
   stubbed oracle covers it instead.
6. **tmux.** The real binary at 80×24: a wide flowchart shows chevrons, scrolls
   right to its far edge, scrolls back, and the status bar reports the offset.
   Reviewers drive the binary; a passing test is not a substitute.

## Performance

The search re-renders a diagram several times on a cache miss. The fit ladder
already renders up to eight times per attempt, and the render cache is keyed on
(document, width, theme, options), so the cost is per-resize rather than
per-frame.

**This must be measured, not assumed.** The handoff records 0.31 s startup for a
300 KB, 6.4k-line document containing 34 flowcharts. Measure that document
before and after; a regression beyond noise on a loaded machine means the search
needs a tighter bound. Measure a ratio against a baseline, never a wall-clock
budget — this is a shared 128-core box and an absolute bound is a coin flip.

## Out of scope

- `visual-review-3.md` §11: overflow chevrons applied to a table's border rows,
  so the frame never closes. Visible in the same captures and worth fixing, but
  it is a renderer bug in table clipping, not this feature.
- §16 (heading hierarchy) and the remaining diagram-routing findings.
