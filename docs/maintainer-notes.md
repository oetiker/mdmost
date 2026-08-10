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
base plus a *spacing* mark (Unicode category `Mc`, not `Mn`) measures three, and U+17D8
KHMER SIGN BEYYAL measures three all by itself. Clamping such a cluster to two makes the
cell lie, and every row containing one comes out a column too wide. `text::cell_clusters`
handles them, and it decides what to do purely by measuring:

- a cluster measuring at most two columns is passed through whole — which is why a bare
  ZWJ sequence or flag is never touched, *but* the same sequence carrying a spacing mark
  measures three and is;
- a wider cluster is cut at a point that preserves its total width, never inside a join
  (two adjacent cells that re-form one cluster draw narrower together than apart, and the
  row comes up short while every per-cell check passes — that has happened);
- a wider cluster with no such cut — one scalar of three columns, or one whose every
  boundary changes the total — cannot be put into cells at all, so it is replaced by
  `text::UNPLACEABLE` (`�`) padded with blanks to exactly the width it drew. The grid
  stays honest and everything after it keeps the column the layout gave it.

Do not "fix" the third case by widening the cell contract; it is one sign, and every
consumer of the grid assumes 0/1/2.

That clamp was found in three separate places over the project's life. Treat any
arithmetic built on `grapheme_width` as suspect until checked.

A cell also holds **no control character**. That is the same contract read the other way
round: `width` is a claim about what the *terminal* draws, and a control character is an
instruction rather than a glyph. `unicode-width` prices every Unicode `Cc` character at
one column, so a literal `TAB` in a paragraph passed every check in the program while the
terminal jumped to the next tab stop and drew the row some six columns wider than it was
laid out at; an `ESC` would have let a document write an escape sequence straight to the
reader's screen. `cell_clusters` substitutes one column of real text for each — a space
for the whitespace controls, `text::UNPLACEABLE` for the rest — and both `Cell::new` and
`check_invariants` reject any that gets past it. Tabs *inside a code block* are still
expanded to real tab stops by `highlight::expand_tabs`, which runs before anything
measures the line; that is the only place a tab's alignment carries information, and the
only place there is a column to expand it against.

## What the terminal draws is not always what `unicode-width` says

Investigated 2026-08-09 after a review reported emoji overflowing a 100-column pane and
corrupting the scrollbar, diagnosed there as "VS16 presentation sequences measured 1,
drawn 2". **That diagnosis is wrong**, and the measurements are worth keeping because the
next person will suspect the same thing. Under `unicode-width` 0.2.2, and probed against
tmux 3.4 by printing sixty copies into a 100-column pane and seeing which row the text
wrapped onto:

| Glyph | `display_width` | tmux 3.4 draws |
|---|---|---|
| `❤` `✔` `⚠` bare (text presentation) | 1 | 1 |
| `❤️` `✔️` (VS16), `1️⃣` `#️⃣` (keycap) | 2 | 2 |
| `☕` `✅` `👍` `👍🏽` (skin tone), `👩‍💻` (two-person ZWJ) | 2 | 2 |
| `🇨🇭` regional-indicator flag | 2 | **1** |
| `👨‍👩‍👧‍👦` four-person ZWJ sequence | 2 | **≈4** |

So variation selectors are measured correctly and drawn correctly. The two rows that do
not line up are both tmux 3.4 failing to treat an *extended grapheme cluster* as one
cell — it clusters a two-person ZWJ sequence and stops there, and it does not cluster a
flag at all. tmux 3.5 added extended grapheme cluster support; terminals that do their
own clustering (kitty, foot, WezTerm) draw both at two columns, which is what we measure
and what UTS #51 says.

**Deliberately not fixed.** An override table pinning a flag at one column and a family
at four would make `mdmost` correct on tmux 3.4 and wrong everywhere else, and wrong in
the direction that cannot be recovered — the canvas would then be lying about its own
width. The honest position is that the canvas is right and the terminal is behind. If it
has to be worked around one day, the workaround belongs behind a terminal capability
probe, not in `src/text`.

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

## Releasing

Releases are cut by the `Release` workflow, run from the Actions tab with a release type
of `bugfix`, `feature` or `major`. It must be run from `main`, and it refuses otherwise.

Before the first release:

1. Create `https://github.com/oetiker/mdmost` and push `main`.
2. Add the `CRATES_IO_TOKEN` repository secret (Settings → Secrets and variables →
   Actions). It is the only secret this project uses.
3. Settings → Actions → General → Workflow permissions: allow read and write. The
   `version` job pushes a commit and a tag, and the `homebrew` job pushes the rewritten
   formula.

Each release:

1. Put what changed under `## Unreleased` in `CHANGES.md`. The workflow moves that block
   into a dated section and uses it verbatim as the release notes — nothing else writes
   them.
2. Run the workflow. It bumps `Cargo.toml`, tags, builds five targets, packages `.deb`
   and `.rpm` for the two musl targets, publishes the crate, and rewrites
   `Formula/mdmost.rb` with the new checksums.
3. `git pull` afterwards: the workflow has pushed two commits and a tag to `main`.

What is deliberately not automated, and why, is in
`docs/superpowers/specs/2026-08-09-publishing-design.md` §1 — there is no apt/yum
repository, no Pages site, no container image and no macOS notarisation.
