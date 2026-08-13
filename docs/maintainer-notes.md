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

## Regenerating the demo

`docs/demo/mdmost.webp` is the README's hero image. It is recorded with
[ansidrama](https://github.com/oetiker/ansidrama), from the repository root:

```sh
ansidrama record demo/mdmost.toml
```

The script drives `tmux` in an embedded terminal: `less` in the left pane and `mdmost`
in the right, on `demo/tour.md`. Everything it depends on is under `demo/` —
`config.toml` (mdmost's own settings, so the recording never inherits yours),
`tmux.conf` (the split, the mouse, and `set-clipboard on`), and `mdmost.toml` (the
script itself). It needs `tmux` >= 3.4, `less` and `nano` on the host, and nothing else.

The frame is dressed as a macOS window by ansidrama's own `[chrome]` table, so the
output is larger than the cell grid: 728×501 rather than 700×450, from `padding = 14` on
each side plus the title bar. The bar colours are keyed to the dark theme in
`src/theme/builtin.rs` rather than left at ansidrama's greys, and they are deliberately
fixed — a title bar that changed with the theme would read as a second window opening.

Five things in there are load-bearing and easy to break:

- **`demo/tour.md`'s widths.** The point of the drag is that the pane passes through
  the widths where the content changes shape. Measured against the current renderer:
  the three-column table has two-line cells up to 59 columns and single-line rows from
  60; the small flowchart's labels wrap up to 50 and are single-line from 51; the
  five-column table is 60 wide and the `pipeline.mmd` diagram declines to draw below
  65, so neither fits the 48-column pane. Change a cell's text and you can silently
  move one of those thresholds past the drag, leaving the demo showing nothing. Check
  with `mdmost --render-once --width N demo/tour.md` at 48, 50, 51, 59, 60, 64 and 100
  before re-recording. **These numbers drift on their own.** The flowchart's threshold
  was 59 when the demo was first recorded and is 51 now, and the width the pipeline
  reports it wants went from 188 to 127 — a sentence in `demo/tour.md` quotes that
  number aloud, so re-read it too. Nothing in the test suite pins any of this.
- **Coordinates are read off the screen, not derived.** Every `click` in the script is
  a column and a row of a hundred-column frame, and a click that lands on nothing is
  *silent*: the copy does not happen, and the `prefix ]` that follows pastes the
  previous buffer a second time, which looks almost right. The fenced block's `[copy]`
  moved from row 16 to row 1 between recordings for no reason but a renderer change.
  Before re-recording, put a 49-column pane on `demo/tour.md`, walk the same keys, and
  read every button's row and column off `tmux capture-pane -p`.
- **`DISPLAY` and `WAYLAND_DISPLAY` are emptied** in `demo/mdmost.toml`. mdmost writes
  OSC 52 unconditionally, and tmux's `set-clipboard on` takes it into its own paste
  buffer — that is the whole of act 4 and it needs no display server. With one
  reachable, `arboard` also takes the copy and the status bar says something else.
- **`set -g set-clipboard on`.** Without it `prefix ]` pastes nothing and act 4 is a
  mime.
- **`hold_cs` is a duration written into the WebP, not a sleep.** ansidrama sends the
  next scene's key as soon as the PTY has been quiet for `settle_ms`, so scenes that
  read as seconds apart on screen are milliseconds apart in the terminal. Two
  consequences, both of which have bitten this script:
  - an `Escape` scene lays a bare ESC in front of the next keystroke and the pair
    arrives as `M-<key>`, losing both. There is no `Escape` in the script; act 6
    dismisses the footnote box with a click and act 7 relies on the same rule.
  - a key sent right after the left pane's `exit` can reach the dying shell instead of
    mdmost, because `bash` is quiet for longer than `settle_ms` before tmux notices the
    EOF. Act 5 spends three throwaway `g`s there to buy quiet windows.

Recording takes about five minutes and the result is lossless WebP, currently 906
frames and about 1.7 MB. Successive runs produce the same frame count and the same loop
duration but **not** byte-identical files — a few dozen bytes drift between runs — so
re-record only when the demo actually changes, not as routine hygiene.

**The run log counts frames, it does not number them.** `scene 49 → 833 frames total`
means that scene's last frame is `frame0832.png`, because the dump is zero-indexed. Read
it as a frame number and every beat you check is the one *after* the one you meant, which
is usually a plausible-looking screen — it cost a take here: a footnote counter read as
`7 → 6 → 5 → 5` (a keystroke apparently swallowed) was really `8 → 7 → 6 → 5` with the
open frame mistaken for the first press. **When a beat looks wrong, check the app in a
live pane before changing the script**: `tmux -L probe -f demo/tmux.conf new-session -x
100 -y 30 …`, walk the same keys with a pause between them, and read `capture-pane -p`.

**Watch the result before committing it.** `ansidrama record demo/mdmost.toml
--dump-png <dir>` writes every frame as a PNG beside the WebP, and the run log names the
frame each scene ends on, so a beat can be checked by opening one file. Three of the
four takes behind the current recording had a silently broken act in them — a copy that
pasted the last thing twice, a note that never closed, a contents pane that opened one
beat too late — and none of it was visible from the byte size or the frame count.

The copy buttons only exist when mouse capture succeeded, so `--render-once` shows none.
That is correct, not a broken render.

### The theme beat is cut, and why — restore it when ansidrama is fixed

The tour used to end by pressing `t` to the light theme and `t` back to dark. **That beat
is gone**, because at that point in the recording — and only there — the frame after the
first `t` is still the *dark* screen carrying `theme: light` in its status bar. It held
for 2.8 s on the hero image, and it shipped that way in the recording before this one. A
status bar that names a theme the screen is not wearing is the one thing this project
says a status bar may never do.

It is not mdmost, and three checks say so:

- a live pane repaints correctly — after `t` the background is `#fdfcf9` every time;
- a three-scene ansidrama script (launch, `t`, hold) records the switch correctly;
- the same script driving `tmux -f demo/tmux.conf` records it correctly too.

Only the full tour fails, and **so far only the full tour**. Neither an empty re-capture
nor a `j`/`k` that forces a complete repaint recovers the frame, so it is not a capture
settling early: those bytes never landed.

**Two hypotheses have been tested and killed**, so nobody repeats them:

- *"It is the pane kill and the resize act 5 does."* No. A script that launches the same
  two panes, kills the left one and switches theme in the resized survivor renders light
  correctly.
- *"It is the alternate screen, from nano in act 4."* No. The same script with a nano
  session opened and closed before the pane kill also renders light correctly.

What remains untested, in the order worth trying: the **mouse drag** of act 2 and 3
(many resize events, not one), an **active search highlight**, the **contents pane**,
the raw SGR motion reports act 6 injects through the `keys` escape hatch, and simple
**accumulation** — the failing frame is number 906 of 907, and every probe so far has
been under 50 frames. The cheapest next step is to bisect the real script: truncate it
after each act, append a `t` and an empty scene, and record. That is about four minutes
a probe and roughly three probes by binary search — far cheaper than it looks, and much
cheaper than another synthetic guess.

**To restore the beat**, put two `t` scenes back at the end of `demo/mdmost.toml`, record,
and open the frame after the first one. If it is light, the bug is fixed and the tour can
show themes again. Until then the tour never leaves dark, and ends on the frame `enter`
left behind.
