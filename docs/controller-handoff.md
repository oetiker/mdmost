# Controller Handoff — mdless

> Starter pack for the next controller session. This handoff lives in ONE
> worktree — run `git worktree list` first and confirm this is the workstream
> you're resuming. Read this first, then `git log <handoff-commit>..HEAD` for
> everything that changed since. Detail is NOT here — it's in git and the docs
> named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry
> forward any lesson in §4/§5 that is still true. Fresh synthesis, not blank
> page.

Handoff commit: 0ee8124   Date: 2026-08-09   Reason: context budget
Worktree / branch: main checkout (/home/oetiker/checkouts/mdless) @ main
Trunk at time of writing: `main` @ 0ee8124 — this IS trunk. **Re-derive anyway**
(`git log --oneline -5`).
Sibling worktrees: five under `/scratch/oetiker/claude-worktrees/`. Four
(`mdless-gate`, `-layout`, `-qa`, `-rendercheck`) are dead agent scratch from
the original build, all far behind trunk, all with uncommitted edits that were
swept into trunk long ago; the user was asked on 2026-08-09 and chose to **leave
them**, so leave them. The fifth, `mdless-icons` @ `icons-autodetect`, **is
merged into main** and its handoff is a tombstone — do not work there.

## 1. Mission

`mdless` is a full-screen terminal pager for a single Markdown document — "as
pleasant to look at as btop, as pleasant to use as less". Rust + ratatui.
GFM including tables with Markdown inside cells, syntax-highlighted code, and
seven Mermaid families drawn as Unicode box art. Reflows on resize, TOC pane,
search, themes.

The mental model that matters: **rendering is a pure function of
`(AST, width, theme, options)`**. The document is parsed once; no layout
decision is ever taken at parse time; a resize discards the canvas and renders
again. Everything else follows from that — recursion over width budgets is what
makes Markdown-inside-tables work without special-casing, and `Canvas` is the
single currency between every renderer.

## 2. Where we are now

As of the handoff commit: **713 tests passing, 0 failures, `clippy
--all-targets -- -D warnings` clean, `cargo fmt --check` clean.** Re-derive
before trusting (§8).

**The second QA round is done, and its most important result is a
disagreement.** Three independent reviewers drove the real binary in tmux,
none allowed to read any earlier report:

| report | question | verdict |
|---|---|---|
| `docs/qa/usability-review-2.md` | reach for it instead of `less`? | **yes** |
| `docs/qa/visual-review-2.md` | happy to look at it every day? | **yes** |
| `docs/qa/visual-review-3.md` | happy to look at it every day? | **no**, 22 findings, 3 severe |

The first round was "no" on both. **Do not read this as "we passed".** The two
visual reviewers saw the same commit under the same brief and split, and the
pessimist is consistently the more specific of the two — it names widths,
columns and code points where the optimist generalises. Its three severes are a
seven-node flowchart declining to draw at 80 columns and dumping raw Mermaid
source; heading hierarchy carried almost entirely by hue, leaving every heading
*less* prominent than body text; and no left margin or right gutter in the live
TUI (which `visual-review-2.md` also found, ranked medium — the one place they
agree, which is a good reason to believe it).

Treat "yes" as the ceiling of what this build has earned, not the finding.
`visual-review-3.md` is the better worklist.

Landed this session, on top of the previous handoff's 697:

- **A real canvas-contract bug, found by the property test.** `cell_clusters`
  split an over-wide grapheme cluster at the next *advancing* character, which
  cut a ZWJ emoji sequence carrying a spacing mark in half: pieces claiming
  2+2+1 columns for a cluster that draws 3. The pieces re-joined when written
  into adjacent cells, so the row rendered two columns short while every
  per-cell assertion passed. Split point is now chosen by measurement.
- **`check_invariants` now measures the assembled row**, not only each cell.
  That hole is why the bug reached a property test instead of a unit test.
- **Nerd Font glyphs are detected, not assumed** — the user overturned the
  original design decision (§7 of the previous handoff). Spec §2.1 records the
  change on the record; read it before touching this.
- **One unknown config setting no longer discards the whole file.** The second
  usability review's highest finding, and the README's own example config
  triggered it (`toc_open`/`toc_width` were never keys). A test now parses the
  example straight out of `README.md`.
- **The highlighter's cost guard measures the shape of the cost curve**, not a
  wall-clock budget, so a loaded shared machine stops failing the gate.

**The previous handoff's "unverified pager polish" list is now audited and is
essentially all done.** Verified present in code: SIGINT/SIGTERM/SIGHUP
registration (`tui/term.rs:46-50`), SIGPIPE (`main.rs`, `is_broken_pipe`),
`[toc] open` read *and* consumed (`config.rs` → `main.rs:162` → `app.rs:171`),
empty-document notice (`draw.rs:70`), TOC current-section tracking
(`app.rs:409/560/909`), search centring (`reveal_centered`), sticky TOC filter,
theme background fill across pane and overlays (`chrome.rs:55/537/588`).
Startup measured at **0.31 s for a 300 KB / 6.4k-line document containing 34
flowcharts**. `tui/icons.rs` had no display-width test — it does now, and the
chrome/renderer glyph tables both enumerate their glyphs so detection derives
from them.

## 3. Do this next

Work `visual-review-3.md` first; it is the harsher and more specific list, and
its severes are the difference between "yes" and "no" on the headline question.

1. **A seven-node flowchart will not draw at 80 columns** — it gives up and
   dumps raw Mermaid source (`visual-review-3.md` §1, SEVERE). Box art is the
   feature this project leads with and 80 columns is the commonest terminal
   width, so this one finding is most of the "no". Related and cheap: §12, the
   "cannot draw" message never says what it would have needed.
2. **Heading hierarchy is carried almost entirely by hue** (§16, SEVERE), so
   every heading is *less* prominent than body text and the light theme's ramp
   is flat. This is a theme/weight decision, not a layout bug, and it is the
   finding most likely to change how the tool feels.
3. **No left margin and no right gutter in the live TUI** (§15, SEVERE; also
   `visual-review-2.md` §3). The two reviewers agree here — content is welded to
   the scrollbar. Cheap to fix, visible every session.
4. **Diagram routing and label placement** — where the two visual reviews
   overlap most. Connectors attaching off-centre or at box corners
   (`-3` §2), edge labels bisected by crossing wires and opposite-direction
   edges collapsing into one line (`-2` §1 and §2), duplicated edge labels in ER
   and state diagrams (`-3` §8, §9). Expect the fixes in edge-lane assignment
   under `src/mermaid/layout/graph/route*` rather than in any family's renderer.
5. **`docs/qa/usability-review-2.md` findings 2-13.** The cheap high-value ones:
   `Esc` on a pending count cancels correctly but reports "nothing to cancel" (a
   lie, in the one subsystem whose whole virtue is that it never lies); `n`/`N`
   with no active search are silent; the help overlay eats the next keypress in
   four different ways; the invalid-regex message is truncated before the part
   that says what is wrong.

Both visual reviews end with a "what looks good — do not break this" section and
`-2` ends with a suggested fix order. Read those before touching the renderers.

## 4. Lessons & traps ← the irreplaceable part

1. **The shared `CARGO_TARGET_DIR` is still the most expensive hazard here.**
   `~/scratch/cargo-target` is shared, so with several agents building: test
   binaries go stale and `cargo test` reports results for code not on disk;
   `debug/mdless` is whichever agent built last, so a visual check may show you
   someone else's tree; a build race can replace the rlib mid-rustdoc. Give
   every agent its own `CARGO_TARGET_DIR` — every brief in this project now
   says so — and `touch src/lib.rs && cargo build` before believing a surprising
   result.
2. **Never read a gate's result through a pipe.** I ran
   `cargo test 2>&1 | tail -40`, got exit 0 from `tail`, and reported the suite
   green when it had a failure. The failure was a real latent bug. Use
   `${PIPESTATUS[0]}`, or grep the summary lines and total them. This is the
   single cheapest mistake available in this repo and it hides exactly the
   findings you most want.
3. **A green property test proves nothing about the run you didn't do.**
   `render_property` is randomly seeded, so the ZWJ bug had been latent through
   the entire project and surfaced on one run. The durable artifact is
   `tests/render_property.proptest-regressions` — when it grows, **commit the
   new seed with the fix**, and treat any suite that mysteriously fails once as
   a real finding rather than a flake.
4. **False doc comments remain the most dangerous defect class.** The ZWJ bug
   sat behind a comment claiming ZWJ sequences were "untouched" and that
   splitting moved the accounting "from a lie to the truth" — both false, in the
   exact case that broke. Earlier instances: a clamp documented as upholding the
   contract it violated, a dead `pub fn` with a claimed caller, `--help`
   advertising a reverted default. **No test catches any of these.** When you
   change behaviour, grep the prose — and prefer rules stated as *measurements*
   over rules stated as *lists of cases*, because the list is what goes stale.
5. **Per-item invariants do not add up to a whole-object invariant.** Every cell
   was honest and the row was still short. If you add a check, ask what it
   cannot see; the assembled-row check now in `check_invariants` is that lesson
   made executable.
6. **Tests here have passed with *and* without their fix, more than once.**
   Prove every behavioural test red before you fix. I did this for the ZWJ fix;
   for the new assembled-row check the proof came from having *seen*
   `check_invariants` return `ok` on the broken row before the change.
7. **Agents go silent, and they also finish the minute after you write them
   off.** I checked for `qa-visual2`'s report, saw nothing, confirmed an idle
   target dir, and respawned it — it had delivered one minute later, and the
   replacement was pointed at the same output path and would have overwritten
   it. **Back the deliverable up before respawning anything**, and prefer giving
   a replacement a different output path. The previous handoff's version of this
   lesson (an agent working in a worktree looked like it had produced nothing)
   is the same trap from the other side.
8. **Reviewer findings are wrong often enough to check — and your own changes
   can destroy the evidence they were built on.** The usability review proved
   the config bug by comparing Nerd vs plain glyphs; on my branch that
   comparison no longer discriminated, because I had just made glyph use
   auto-detected and `--render-once` to a pipe now picks plain. The finding was
   real; the reproduction had to be redone another way. Re-verify against the
   tree you actually have.
9. **`git add -A` sweeps unrelated work into your commit.** It put the perf-test
   rewrite inside the config-fix commit, whose message said nothing about it; I
   split it back out. Check `git status` and stage by path.
10. **A wall-clock budget on a shared 128-core box is a coin flip.** The
    highlighter guard failed at 47 s against a 40 s budget on load average 47,
    having passed an hour earlier on the same commit. Its author already knew
    the machine was shared and had set the bound generously — no absolute bound
    is generous enough. Measure a ratio, not a duration.
11. **Every duplication found has been a missing shared operation.** If you find
    duplication, add the op to `src/canvas` or `src/text` rather than
    deduplicating in place. Related: `Glyphs::language()` became a table rather
    than a `match` specifically so the icons could be *enumerated*, which is
    what lets font detection derive its probe from the glyphs that actually
    draw. Any second hand-written list of the same code points would drift.
12. **Do not hand-edit insta snapshots.** Use
    `INSTA_UPDATE=always cargo test --test <target>` and review each diff.

## 5. Don'ts & constraints

- **No HTML.** Not rendered, not passed through. Settled in the design Q&A.
- **Mermaid is Unicode box art only**, never raster. Settled the same way.
- **Nerd Font glyphs are DETECTED, not defaulted on** — changed by the user on
  2026-08-09 and recorded in spec §2.1 with its reasoning. The previous handoff
  said the opposite; that is the superseded position. Detection answers "yes"
  only on positive evidence and falls back to plain whenever it cannot tell.
  **Do not "fix" it back to an unconditional default.**
- **`Esc` never quits.** It unwinds count → search → TOC filter → TOC focus →
  TOC pane, then says `nothing to cancel — press q to quit`. The spec was
  changed to match the implementation, not the reverse. (The pending-count step
  reports the wrong message — see §3.3 — but the *behaviour* is right.)
- **Do not widen the `NodeArt` seam.** One method,
  `render(node, budget, theme) -> Canvas`, measuring and painting in one call so
  they cannot drift. Four families were built on it without changing it.
- **Gantt state is carried by colour alone.** Per-state fill densities made the
  default state the least visible thing on the page.
- **No 1000-node golden snapshot** (spec §13.2 records why): a diff nobody can
  read gets rubber-stamped. Scale is covered by property tests.
- 4-core cap on this machine: `CARGO_BUILD_JOBS=2` on every cargo invocation.

## 6. Where the detail lives

- Change history: `git log 0ee8124..HEAD`, and `git log --oneline` for the build.
- **Design spec (the authority):** `docs/superpowers/specs/2026-08-08-mdless-design.md`
  — §2.1 the icons decision and why it changed, §3 the central rule, §4 Canvas
  contract, §6 per-family Mermaid subsets, §7 tables, §10 keys, §13 testing.
- **Maintainer notes (judgment):** `docs/maintainer-notes.md` — the engine seam,
  the cell-width contract, why gantt is colour-only.
- **QA, second round (current):** `docs/qa/visual-review-2.md`,
  `docs/qa/usability-review-2.md`. Both verdicts "yes"; both end with what works
  well, which is the list of things not to break.
- **QA, first round (historical):** `docs/qa/visual-review.md`,
  `docs/qa/usability-review.md`, `docs/qa/code-review.md`. Both user-facing
  verdicts were "no". Mostly fixed; kept as the record of what "harsh enough"
  looks like here.
- `README.md` — key map generated from the live binding table, and the second
  usability review verified that claim by rebinding keys and watching the
  overlay change. The config example is now covered by a test.

## 7. Open questions / pending decisions

1. **Nothing is blocked on the user.** The three questions the previous handoff
   raised were put to them on 2026-08-09 and answered: detect-and-fall-back for
   icons (done), run the second QA round (done), leave the stale worktrees
   (done — leave them).
2. **Whether `fc-list` is the right probe on macOS.** It is normally absent
   there, so detection falls back to plain and every Mac user needs
   `MDLESS_ICONS=1` or `icons = true`. That is safe but pessimistic, and nobody
   has tested it on a Mac. A `TERM_PROGRAM`-based signal could help; it was
   deliberately not added, because guessing from the terminal's *name* is the
   category-enumeration mistake §4.4 warns about.
3. Nothing has been pushed to any remote in this session — re-derive with
   `git status -sb` and `git log --oneline @{u}..` rather than believing this
   sentence.

## 8. Staleness watch

- The §2 test count and gate status reflect ba9eeae. **Re-run**:
  `export CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdless-lead && touch src/lib.rs && CARGO_BUILD_JOBS=2 cargo test`
  — and read the exit code, not the tail of a pipe (§4.2).
- §2's polish audit is a *code* audit: I confirmed each item exists and is
  wired, not that each behaves perfectly under a user's fingers. The second
  usability review drove most of them and was satisfied; where the two
  disagree, believe the review.
- The §2 verdict table is the least stale thing here and the most easily
  misread. Two reviewers said yes and one said no; quoting only the first half
  of that would be the kind of claim this project has been bitten by before.
- **Integration state must be re-derived, never inherited.**
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`.
- **Sibling worktrees or workstreams started after this commit are invisible
  here.** `git worktree list`.
