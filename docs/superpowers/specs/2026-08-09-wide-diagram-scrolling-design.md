# Wide diagrams scroll instead of dumping source

Design doc. Revision 2, 2026-08-09, after two independent hostile reviews (one
auditing mechanics against the code, one driving the binary in tmux as a
reader). Revision 1 is in git; every change below is traceable to a finding.

Supplements `2026-08-08-mdmost-design.md` §7.3 and §8, which promise horizontal
scrolling for over-wide blocks. This extends that promise to diagrams — and
first repairs the scroll model it would extend.

## The problem

A Mermaid diagram that will not fit degrades through the layout engine's fit
ladder; when the ladder is exhausted the block falls back to a dump of its own
Mermaid source (`docs/qa/visual-review-3.md` §1).

In the pager this is worse than it sounds. `render::document::render_document`
widens any block that comes back clipped, so **for the too-narrow case** the
machinery to show the whole thing is aimed at the source dump: the reader
side-scrolls through raw Mermaid. (For a *syntax error* the same behaviour is
correct — the source is the content — which is why the sentence is scoped.)

**The motivating example has moved.** Review 3 bisected the seven-node
`flowchart LR` as dumping at 80 and drawing from 92. Commit `89e9c54` changed
that: it now draws from inner width ~59. The case this feature exists for is
the genuinely large chart — a twelve-node LR that dumps at every width to 192
and draws at 196, or a twenty-node one whose natural width is ~929.

## What the reviews changed

Revision 1 was a single change. It is now a sequence, because two prerequisites
must land first and one of them shrinks the feature.

**A. Fit is non-monotone in width, and that is a bug, not a constraint.**
Revision 1 built the search around it ("must not bisect"). Root cause is
`graph.rs`: `budget = (width / share).max(6)` — the engine refuses to use *less*
budget than a rung grants it, so one more column of width can hand every node a
wider budget and overshoot. Since drawn width is nondecreasing in budget,
`∃b ≤ w : drawn(b) ≤ w` **is** monotone in `w`. Fixing it is a ninth step after
the eight rungs exhaust: bisect on budget. It runs only on exhaustion, so every
currently-drawing diagram stays byte-identical — the same first-fit safety
argument that made the new rungs safe. It re-legalises bisection, makes
`TooNarrow.needed` a true floor, and shrinks the set of diagrams that ever need
scrolling.

**B. Horizontal scroll drags the whole page, and has no way home.** One wide
block scrolls the title off-screen and cuts every paragraph mid-word; `g`,
`Home`, `0` and `^` all fail to return. Today the worst realistic surplus is a
wide table (~40 columns). A widened diagram is 116, or 850. Widening diagrams
into this model deletes the paragraph that explains the diagram, at exactly the
moment the reader is looking at the branch it describes. **Prerequisite.**

**C. The rungs that break words must not survive on the scrollable path.**
`(1,6)` and `(1,8)` were accepted on an explicit argument (handoff §4.12): *the
counterfactual at a new rung is never a prettier diagram; it is the source
dump.* This design destroys that counterfactual. What the rungs buy at width 72
is `Star/t`, `Pars/e Mark/down`, `Repo/rt erro/r`. Both reviewers reached this
independently, and review B's own precedent is visual-review-2 §5: *"Rendering
an unreadable box is strictly worse than both drawing it properly and refusing,
because it looks like the diagram is the information."*

**D. The seam must return the canvas, not the width.** As specified it laid out
every *fitting* diagram twice — the branch runs the full ladder, throws the
canvas away, then `render_widened` runs it again. Measured at +43 % startup on a
diagram-heavy document. Revision 1's claims "costs nothing to build" and "the
new branch is inert" were false.

**E. Width must be capped far below `MAX_BLOCK_WIDTH`.** `Canvas::append` pads
every row of the document to the widest part, so one wide diagram inflates the
whole document canvas: measured 7× peak RSS and 2.7× time at 2048 columns. A
diagram nobody can navigate is not better than readable source.

## Stage 1 — prerequisites

Independently valuable; each ships on its own merits.

### 1a. Budget bisection makes fit monotone

In `graph::draw`, after `LADDER` is exhausted, retry the tightest rung with the
budget bisected downward: find the smallest budget whose drawing fits `width`,
or confirm none does. ~7 extra layouts, only on a path that today returns an
error.

- Every diagram that draws today draws identically. The suite being unchanged
  is the check.
- `TooNarrow.needed` becomes a genuine floor: not "a floor about widening", but
  a width below which nothing draws.
- Pin the currently non-monotone cases as tests **before** the fix (inner 63 ✓ /
  64 ✗ / 65 ✓), and assert monotonicity after: for a sweep of widths, once a
  chart draws it draws at every greater width.

### 1b. Horizontal scroll stops dragging the page, and gains a way home

- A row that is not over-wide stays anchored at column 0 while an over-wide
  block scrolls. Prose, headings and narrow blocks must not move.
- A horizontal-home action returns the offset to 0. `0` currently starts a
  count prefix, so bind it deliberately — extending `g`/`Home` to reset both
  axes is the cheaper option and matches the reader's mental model of "go
  back to the start".
- The `↔ n/N` readout appears at offset 0, not only after the reader has
  already discovered the key. A reader who is never shown `↔ 0/116` has no way
  to judge what they are missing.
- Help and `README.md` say "wide tables and code" for `←`/`→`. After this work
  it is "wide content". A reader who sees chevrons on a diagram and consults
  help must not be told the key is for something else.

### 1c. Overflow chevrons stop breaking table frames

`visual-review-3.md` §11: the clip marker is painted on border rows, so the
frame never closes. Revision 1 called this out of scope. It is not: the same
chevrons are the entire discoverability mechanism this feature relies on, and
on a widened diagram they land on the diagram's own box art. Shipping the
feature on a marker that reads as breakage is a poor trade.

## Stage 2 — the feature

### The seam

```rust
/// The diagram this block draws, and the width it needed, at the narrowest
/// width of at least `from` that works.
///
/// `None` when the node is not a Mermaid fence, and when a fence fails for any
/// reason other than being too narrow.
pub(crate) fn diagram(
    node: &Node,
    from: u16,
    limit: Limits,        // max width and max probes, owned by the caller
    theme: &Theme,
    options: &RenderOptions,
) -> Option<(u16, Canvas)>
```

Returning the canvas is what makes the fitting case free (finding D): at
`at == width` the caller uses it as `narrow` instead of re-rendering.

`Limits` is passed in, not read from `render::document`: the caller owns the
policy, `render::diagram` owns the question. When this was written the argument
was also a module-boundary one — the widening lived in `tui::wide` and `render`
must not depend on `tui`. That half lapsed on 2026-08-10, when the widening
became the one document renderer and moved into `render::document`; the
separation of policy from question is why the seam still looks like this.
`MAX_BLOCK_WIDTH` remains private to `render::document`.

The `None` cases are load-bearing. A syntax error, an unsupported family and an
internal error keep dumping source at viewport width. **Only
`MermaidError::TooNarrow` earns a wider canvas.**

### The search

With 1a landed, fit is monotone, so the search may bisect. It still starts from
`TooNarrow.needed`, which is now a true floor.

Two bounds, both from the caller:

- **Width:** `min(MAX_BLOCK_WIDTH, k × viewport)`, `k = 3` unless measurement
  argues otherwise. Beyond it, the source dump is the better answer — 116 arrow
  presses to cross a diagram is not reading. Revisit only if a navigation
  primitive (page-left/right, jump-to-edge) lands.
- **Probes:** a hard cap of 8. `pie.rs` returns `needed: None` and `gantt`
  returns a constant floor independent of width; without a probe cap the search
  degenerates to a linear scan to 2048 for them. Bounding probes makes those
  safe by construction rather than by an argument about which renderer can fail
  where.

**Minimum surplus.** Do not widen by a trivial amount. A diagram that needs one
column more than the viewport must not give the whole document a horizontal
scrollbar, chevrons on every row and an `↔ 1/1` readout. Below a small
threshold, prefer the fit ladder's result or the dump.

### The ladder split (finding C)

The rungs that break words inside a node label are dropped **on the scrollable
path** and kept on the piped path, where the source dump genuinely is the only
alternative. The design already splits by path; this makes the split explicit
rather than letting a ladder change quietly contradict a documented decision.

Implement as a policy the caller passes, not two ladders: the accept condition
gains "and no node label was broken mid-word", enabled for the pager and
disabled for `--render-once`.

This is a deliberate reversal of commit `89e9c54`'s reasoning, valid only
because this feature supplies the better counterfactual. Record it in the
handoff so the next reader does not re-derive it as a regression.

### The plug-in point

`render::document::render_widened` gains one branch in front of its existing clip
hunt, using the returned canvas rather than re-rendering. Nothing downstream
changes: `Canvas::append` pads narrower parts, anchors and search spans are
translated through `blit`/`merge_metadata`, `hscroll_max` picks up the surplus,
the viewport paints the chevrons.

### The caption

Once `diagram` exists the honest number is free. The caption says the width
that actually draws, or no number at all — never a floor presented as an
answer. This applies to the piped path and to a diagram that exceeds the cap.

## What does not change

- A diagram that fits draws exactly as now, with no scrolling and **no extra
  layout** (finding D).
- ~~Piped `--render-once` goes through `render_document`, not `render_scrollable`:
  it degrades — including through the word-breaking rungs — and then dumps
  source. There is nothing to scroll in a pipe.~~
  **Superseded 2026-08-10.** This was true when written and is no longer. The
  pipe having a renderer of its own is exactly the bug `d57b580` fixed — it also
  meant `--body-width` was silently dropped there — and the two renderers have
  since been collapsed into one, `render::document::render_document`. A pipe now
  emits what the pager draws, including lines wider than `--width`, which the
  owner chose deliberately over an exact-width guarantee.
- Syntax errors, unsupported families and internal errors: source dump plus
  caption.
- The document margin applies as it now does in `render::document::render_document`.

## Out of scope, stated explicitly

- **Nested diagrams.** `render_widened` walks only `doc.root().children`, so a
  Mermaid fence inside a list item or blockquote is not widened. Note the
  asymmetry, because it is sharp: a nested wide *table* IS widened today, since
  `ClipTest` scans the whole top-level canvas for the clip marker. After this
  change, "wide content inside a list scrolls" is true for tables and code and
  false for diagrams — the same fence behaves differently indented two spaces.
  Accepted for v1; revisit by walking the block for a descendant fence.
  (Diagrams in table cells are not a hole: comrak does not allow fenced blocks
  in GFM pipe cells.)
- **TD hub clipping.** A wide `flowchart TD`'s centred hub label is cut by the
  window edge in exactly the view showing the most children. LR windows read
  well; TD is weaker. Not a blocker, but the design must not call them the same
  case.
- **Page-left/right and jump-to-edge bindings.** Would raise the width cap;
  separate work.

## Testing

Every behavioural test proved red before its fix (design spec §13; this repo has
passed tests both with and without their fix more than once).

1. **The headline case.** A chart that *still* exhausts the ladder after 1a —
   not the seven-node chart, which now draws from ~59. `render_document`
   returns a canvas wider than the viewport whose plain text is box art and does
   not contain `flowchart LR`.
2. **The guard.** A syntax-error fence and an unrecognised-family fence still
   come back at exactly viewport width as a source dump with their caption.
3. **No double layout** (replaces revision 1's vacuous inertness test, which
   passed with the feature absent). Assert the diagram bridge is invoked exactly
   once per fitting fence — a counter, or the perf ratio gate below.
4. **Honesty, twice.** The width the search returns must actually draw, and the
   *caption the reader sees* must name a width that actually draws. Revision 1
   tested only the first.
5. **Monotonicity** (from 1a) replaces revision 1's non-monotonicity test, which
   was unimplementable against the specified signature and pinned pure `LADDER`
   artefacts that this design deletes.
6. **Bounds.** A diagram past the width cap dumps source. A renderer that
   reports no floor (`pie`) terminates within the probe cap.
7. **tmux, by reviewers, at several widths.** Chevrons appear; scrolling right
   reaches the far edge and back; prose stays anchored; the horizontal-home key
   returns; the status bar reads `↔ 0/N` before any scrolling. A passing test is
   not a substitute for driving the binary.

## Performance

Measured as a **ratio** against a baseline re-measured on the same machine in
the same session — never against absolute numbers, on a shared 128-core box.

Baseline recorded in `scratchpad/perf-baseline.md`: 300 KB, 53 flowcharts,
0.18–0.19 s at width 80.

Gate both axes, because revision 1 gated only time and the memory finding is the
larger one: **wall-clock ratio and peak RSS** on the 34-flowchart document. A
regression beyond noise means the width cap or the probe cap needs tightening.
