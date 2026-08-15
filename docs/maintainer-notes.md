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

Six things in there are load-bearing and easy to break:

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
- **`await` patterns are regexes, and they replace the whole `settle_ms` timing model.**
  ansidrama no longer paces on quiet windows; it samples the terminal grid continuously
  and assembles frames from that log. A scene declares the screen it is waiting for with
  `await`, and if the pattern never matches the run **aborts**, naming the pattern that
  failed — there is no silent timeout. `hold_cs` is untouched by any of this: it is still
  a duration written into the WebP, not a sleep, and pacing the *shown* video is still
  its only job. The `Escape` consequence also survives unchanged: a bare ESC lays in
  front of the next keystroke and the pair arrives as `M-<key>`, losing both, which is
  why there is no `Escape` scene in the script — act 6 dismisses the footnote box with a
  click and act 7 relies on the same rule.

  Four things about `await` patterns have each cost an attempt to learn here:
  - **The pattern is `Regex::new(find)` (`ansidrama/src/pattern.rs`), not literal text.**
    Escape `. [ ] ( ) + * ? | ^ $ { } \` or the match tests something other than what it
    looks like. `[copy]` unescaped is a *character class* matching one `c`, `o`, `p` or
    `y` — it is not the button label, and a script carrying it "passes" while verifying
    nothing. It must be `\[copy\]`. (Two patterns in `demo/tour.md` — the sentence
    naming the pipeline width, and `github.com` — carry an unescaped `.`; that is a
    deliberate, harmless wildcard, not an oversight.) `regex_lite` has no implicit
    multiline mode, so `^` anchors the whole string, not each line.
  - **`row` is 0-indexed; `tmux capture-pane` is 1-indexed.** `resolve_row`
    (`pattern.rs:64-68`) uses a non-negative row as the array index directly, so row 0
    is the terminal's first row. A negative row counts from the bottom, so `row = -1` is
    the status bar. Carrying a row read off `capture-pane -p` (which numbers from 1)
    straight into `row` without subtracting one is an off-by-one that has cost a failed
    attempt here.
  - **Row scoping separates rows, never panes.** `row` is the only scoping primitive
    `await` has. In the vertical split, one row spans both panes, so a row-scoped
    pattern is matched against that row's full 100-column string with both panes'
    content side by side.
  - **`^` still separates the panes, for as long as the split exists.** A row-scoped
    match runs against that row with only trailing whitespace trimmed, so `^` means
    column 1 of the frame. nano and less live in columns 1–49; mdmost is the right-hand
    pane and never draws before about column 51 — so an `^`-anchored, row-scoped
    pattern can never match mdmost's content while the split is up. Act 4's three gates
    lean on this structurally, not by luck. mdmost also draws its own one-column left
    margin, so even a pattern aimed at mdmost's content needs `\s*` in front of it, not
    a bare `^`; that stops mattering only once the split closes and mdmost owns column 1
    too.
- **An aborted run leaves the tmux server alive.** Every failed `await` — so every fault
  injection run on purpose while testing a script — exits through ansidrama's error path
  before it tears tmux down. The next `ansidrama record` then runs `new-session -s demo`
  against a socket that is still occupied, and its launch command exits immediately. The
  only symptom is "the launched command exited after scene 00" — nothing that looks like
  a leftover session. After any aborted run: `tmux -L mdmost-demo kill-server` before
  recording again.

**Act 5's fallback, if it is ever needed again.** The script used to spend three
throwaway `g` presses right after closing the left pane, to buy time before the next
real keystroke. They are gone now, deleted once three independent recordings passed
with nothing but the `await` on the full-width screen. Three clean runs on one machine
is not proof the hazard is gone, though — only that it did not show up this time. What
they guarded against is a keystroke reaching a pane before tmux has finished closing it,
which is timing-dependent by nature: a slower or more loaded host is exactly where it
would reappear. **If act 5 ever aborts on its `await`, the first thing to try is adding
a sacrificial `g` back before the `Tab`.** `g` costs nothing to throw away here — act 4
already leaves the document at the top, and `g` goes to the top, so it changes nothing
visible even where it does nothing useful. And when this happens, treat it as the gate
doing its job: the abort means a miss that used to pass silently is now caught, not that
something regressed.

Recording takes about five minutes and the result is lossless WebP, about 1.7 MB. Frame
count is reproducible under the current recorder: two consecutive full recordings
produced 908 frames each, with a byte-identical `manifest.tsv`. The WebP bytes
themselves were not re-checked this round; the earlier finding — same frame count and
loop duration, but a few dozen bytes drift between runs — still stands as the last
measurement of that. Re-record only when the demo actually changes, not as routine
hygiene.

**`--dump-png <dir>` writes `manifest.tsv` beside the frames**, mapping `frame`,
`scene`, `input`, `kind` and `hold_cs` for every frame — the old run log's frame tally
is gone along with the recorder that printed it. The lesson that outlives the mechanism:
**when a beat looks wrong, check the app in a live pane before changing the script**:
`tmux -L probe -f demo/tmux.conf new-session -x 100 -y 30 …`, walk the same keys with a
pause between them, and read `capture-pane -p`.

**Watch the result before committing it.** `ansidrama record demo/mdmost.toml
--dump-png <dir>` writes every frame as a PNG beside the WebP, and `manifest.tsv` names
the frame each scene ends on, so a beat can be checked by opening one file. Three of the
four takes behind the current recording had a silently broken act in them — a copy that
pasted the last thing twice, a note that never closed, a contents pane that opened one
beat too late — and none of it was visible from the byte size or the frame count.

The copy buttons only exist when mouse capture succeeded, so `--render-once` shows none.
That is correct, not a broken render.

**Regenerating, step by step.** Skipping this order is how a broken take gets skipped
too:

1. Rebuild both binaries — `mdmost` and `ansidrama` — so neither step runs against a
   stale one.
2. Check the width thresholds with `mdmost --render-once --width N demo/tour.md` at 48,
   50, 51, 59, 60, 64 and 100.
3. Re-read every `click` coordinate off a live 49-column pane, as above — do not trust
   coordinates from a previous recording.
4. Record with `ansidrama record demo/mdmost.toml --dump-png <dir>`.
5. Open the theme frame named in `manifest.tsv` and confirm its background is light
   (`#fdfcf9`).
6. Confirm the tour's closing frame is dark (`#11141b`).
7. Only then replace `docs/demo/mdmost.webp`.

**Testing one act without paying for the whole tour:** copy `demo/mdmost.toml`, delete
the scenes after the act under test, and record the copy. Two things about the copy
matter — run it from the repository root, because the `launch` line uses paths relative
to there, and always pass `-o` to a scratch path, because the config's `out` key
resolves relative to *the config file's own directory* and an unmoved copy overwrites
`docs/demo/mdmost.webp` from underneath you. **Exception: the theme beat cannot be
verified this way.** The bisect below found the trigger is act 6's keyboard walk running
immediately before it; truncating removes exactly that walk, so a truncated script is
the one configuration that always passes regardless of whether the beat is actually
fixed.

### The theme beat, and how to verify it

The tour ends by pressing `t` to the light theme and `t` back to dark. It was cut on
2026-08-13 because at that point in the recording — and only there — the frame after the
first `t` was still the *dark* screen while its status bar named `theme: light`. A status
bar that names a theme the screen is not wearing is the one thing this project says a
status bar may never do.

ansidrama 0.3.0 fixed the cause: the settle grace it waits out before sampling a frame
is now measured against real changes to the terminal grid rather than against PTY bytes,
so a repaint that lands late is no longer sampled early. The beat is back in
`demo/mdmost.toml`.

**`await` cannot verify this beat, and never will.** A pattern matches text, never
colour, and the broken frame's *text* was correct — its status bar said `theme: light`
truthfully while the pixels underneath were still dark. Verifying the switch means a
human opening the frame and reading the background colour: light is `#fdfcf9`, dark is
`#11141b`. Verified this session at frame0920 (light, correct) and frame0922 (dark,
correct); check the equivalent frames again after any re-recording, since frame numbers
shift whenever anything earlier in the script changes.

The bisect table below is the evidence that makes the fix explicable rather than a hope
that a version bump happened to help. Keep it.

It is not mdmost, and three checks say so:

- a live pane repaints correctly — after `t` the background is `#fdfcf9` every time;
- a three-scene ansidrama script (launch, `t`, hold) records the switch correctly;
- the same script driving `tmux -f demo/tmux.conf` records it correctly too.

Only the full tour fails, and **so far only the full tour**. Neither an empty re-capture
nor a `j`/`k` that forces a complete repaint recovers the frame, so it is not a capture
settling early: those bytes never landed.

**Bisected on 2026-08-13 against ansidrama 0.2.0. The trigger is act 6's keyboard walk.**
Truncate `demo/mdmost.toml` at a given line, append a `t` scene and an empty scene, and
record; that is one four-minute probe per question. Results:

| Script | Theme frame | Result |
| --- | --- | --- |
| acts 1–5 (through the pane close) | 807 | **light — correct** |
| acts 1–6 up to `# No mouse from here` (hover, popup, click away) | 901 | **light — correct** |
| same, padded with ten empty scenes | 911 | **light — correct** |
| same, plus `G` instead of the walk | 902 | **light — correct** |
| **the full tour — the walk `F F F f Enter`** | 906 | **DARK, captioned `theme: light`** |

So five hypotheses are dead, and nobody should re-run them: it is **not** act 5's pane
kill and resize, **not** the alternate screen nano leaves behind, **not** accumulated
frame count, **not** scroll position or a large repaint (`G` moves the whole screen and
is fine), and **not** the capture race that 0.2.0's `react_ms` fixes — `react_ms = 2000`,
four times the default, changes nothing.

**And mdmost is not at fault.** In a live pane the same sequence repaints correctly every
time: `G`, then `F F F f Enter`, then `t`, gives a background of `#fdfcf9`. Only the
recording disagrees.

What was left, at the time, was the difference between a keyboard walk and any other
input inside ansidrama — five keystrokes that move a *painted* cursor and then follow an
anchor, each redrawing, with the last one scrolling. That is the shape of what 0.3.0's
grid-based settle grace fixed: a capture race between a late repaint and an early sample,
exactly where a walk like this one produces one.

**If a future recording shows the same failure**, the fix has regressed — re-open this
section, not the recipe above it. Day-to-day, verifying the beat is just steps 5 and 6
of the regeneration recipe: open the theme frame and confirm it is light, then confirm
the tour's closing frame is dark.
