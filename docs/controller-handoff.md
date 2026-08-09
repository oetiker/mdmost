# Controller Handoff — mdless

> Starter pack for the next controller session. This handoff lives in ONE
> worktree — run `git worktree list` first and confirm this is the workstream
> you're resuming. Read this first, then `git log <handoff-commit>..HEAD` for
> everything that changed since. Detail is NOT here — it's in git and the docs
> named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry
> forward any lesson in §4/§5 that is still true. Fresh synthesis, not blank
> page.

Handoff commit: see `git log -1` — this file is rewritten in place; the last
commit touching it is the marker.   Date: 2026-08-09   Reason: context budget
Worktree / branch: main checkout (/home/oetiker/checkouts/mdless) @ main
Trunk at time of writing: `main`, with all of Stage 1 and Stage 2 merged — this
IS trunk. **Re-derive anyway.**
Sibling worktrees: **three implementer worktrees were live when this was
written** (Stage 1a/1b/1c of the wide-diagram work, see §3), created by
`isolation: worktree` subagents and therefore under
`/scratch/oetiker/claude-worktrees/`. Their branches are NOT merged. `git
worktree list` is the only authority on what survived. Also still present: four
dead agent scratch worktrees from the original build (`mdless-gate`, `-layout`,
`-qa`, `-rendercheck`), far behind trunk, which the user chose on 2026-08-09 to
**leave alone**; and `mdless-icons` @ `icons-autodetect`, merged, tombstoned.

## 1. Mission

`mdless` is a full-screen terminal pager for a single Markdown document — "as
pleasant to look at as btop, as pleasant to use as less". Rust + ratatui.
GFM including tables with Markdown inside cells, syntax-highlighted code, and
seven Mermaid families drawn as Unicode box art. Reflows on resize, TOC pane,
search, themes.

The mental model that matters: **rendering is a pure function of
`(AST, width, theme, options)`**. The document is parsed once; no layout
decision is ever taken at parse time; a resize discards the canvas and renders
again. Everything else follows from that.

The user's standing instruction for this workstream: *"make the best md viewer
there is"*, drive everything through subagents, and have reviewers **drive the
real binary in tmux** rather than trust tests. They do not want to be consulted
on mechanics; they want to be told when it is done and the reviewers are happy.

## 2. Where we are now

**The wide-diagram workstream is essentially done.** 755 tests, 24 suites, all
green with the exit code read directly; clippy `-D warnings` clean; `fmt
--check` clean. **Re-derive** — see §8.

Landed, in order:

- **§1, §12, §15 of `visual-review-3.md`** (`89e9c54`, `7de4506`, `9409229`) —
  the flowchart that dumped source at 80 columns, the caption that restated the
  width you already had, and the pager's missing side margins.
- **Stage 1a, monotone fit** (`351af4b`) — a floor probe at (gap 1, budget 6)
  plus budget bisection after the ladder exhausts. A wider terminal can no
  longer make a diagram disappear, and `TooNarrow.needed` is now the EXACT
  narrowest width that draws.
- **Stage 1b, the scroll model** (`bba73f3`) — per-run offsets, so the block
  that is wide scrolls and the page does not; `g`/`Home` resets both axes;
  `↔ n/N` shows at rest.
- **Stage 1c, frame closing** (`d3ecb7f`) — a cut rule ends in its own corner
  instead of a chevron, in the renderer *and* at the viewport edge.
- **Stage 2, the feature** (`899156b`, `19cb74a`) — a diagram too wide for the
  pane is drawn at the width it needs and reached by scrolling, capped at 3×
  viewport and 8 probes, with a minimum surplus.
- **The Stage 1 review's three defects** (`65a4b2d`) — quoted prose dragged by a
  wide block inside a blockquote; the `↔` chip dropped at 40 columns for a long
  file name; the right-edge chevron invisible behind a double-width glyph.
- **An indivisible grapheme wider than a cell** (`01622b6`) — U+17D8, the only
  scalar in Unicode wider than two columns, could reach `Cell::new` and break
  the grid. Found by the property test, on a seed now committed with its fix.
- **The pager outliving its terminal** (`0fd645a`, `32d654a`) — reported from
  the wild by the user, who had to kill processes eating a core each.

## 3. Do this next

**In flight at this commit:** one agent pinning the line-number gutter so it
stops scrolling off to the left (the user asked for this explicitly). Check
`git worktree list` and `git branch -a` before assuming it is unmerged.

**Then the remaining `visual-review-3.md` findings**, which are now the largest
untouched body of work:

1. **§16 SEVERE — heading hierarchy is carried almost entirely by hue**, so
   every heading is *dimmer* than the body text it introduces, and the light
   theme's six-level ramp measured flat (4.80 → 4.95 → 4.86:1). `visual-review-2.md`
   §4 independently found levels 4-6 indistinguishable. `tests/theme_headings.rs`
   exists and is the place to assert a monotone ramp with measured ratios.
   This is the finding most likely to change how the tool *feels*.
2. **Diagram routing** — §2 connectors attaching off-centre or at box corners,
   §3 rounded elbows mixed with square boxes, §8 and §9 duplicated edge labels in
   ER and state diagrams. Expect the fixes in edge-lane assignment under
   `src/mermaid/layout/graph/route*`.
3. **`usability-review-2.md` findings 2-13** — cheap and high value: `Esc` on a
   pending count cancels correctly but reports "nothing to cancel" (a lie, in the
   one subsystem whose virtue is that it never lies); `n`/`N` are silent with no
   active search; the help overlay eats the next keypress four ways.

**Known, recorded, not fixed:**

- A widened CJK **table** appears to render differently with and without an
  active search at the same offset, reproducibly. Two agents hit it and neither
  could settle whether it is `highlight_matches` patching styles onto
  continuation cells or a tmux grid artifact. **Needs a real terminal.**
- Search finds a horizontally off-screen match and does not scroll to it.
  Pre-existing; per-row offsets make it more reachable.
- Nested diagrams (in a list or blockquote) are never widened, while nested
  *tables* are — the same fence behaves differently indented two spaces.
  Accepted for v1 and recorded in the feature spec's "Out of scope".
- Mermaid subgraph titles truncate with no marker at all (`╭ Outer bounda╮`).
- The residual race in the hangup fix: if the terminal dies between our `poll`
  and crossterm's read, the old spin is still reachable. Documented at the code.

## 4. Lessons & traps ← the irreplaceable part

1. **The shared `CARGO_TARGET_DIR` is still the most expensive hazard here.**
   Give every agent its own; every brief in this project says so. `touch
   src/lib.rs && cargo build` before believing a surprising result.
2. **Never read a gate's result through a pipe.** `cargo test 2>&1 | tail -40`
   returns tail's exit code. This project has reported a green suite that was
   red exactly that way, hiding a real bug.
3. **A green property test proves nothing about the run you didn't do.**
   `render_property` is randomly seeded. When
   `tests/render_property.proptest-regressions` grows, commit the seed with the
   fix, and treat a suite that mysteriously fails once as a finding, not a flake.
4. **False doc comments remain the most dangerous defect class.** Three
   instances now, and the newest is the most instructive: `render_document`'s
   doc promised that "no block is ever welded to the viewport edge or to the
   scrollbar next to it" — true of the function, false of the pager, which does
   not call it. The claim was load-bearing, unverified, and survived because it
   was false only about margins. **No test catches any of these.** Grep the
   prose when you change behaviour.
5. **Per-item invariants do not add up to a whole-object invariant.** Every cell
   was honest and the row was still short; `check_invariants` now measures the
   assembled row.
6. **Prove every behavioural test red before you fix.** Tests here have passed
   with *and* without their fix, more than once.
7. **Back a deliverable up before respawning any agent.** One agent delivered
   one minute after being written off, and the replacement was pointed at the
   same output path.
8. **Reviewer findings are wrong often enough to check — and reviews go stale
   fast.** `visual-review-3.md` §1 bisected the fallback threshold at 92; by the
   time the fix was designed the same chart drew from 62, because an earlier
   commit had already moved it. Re-derive a finding's numbers against the tree
   you actually have before building on them.
9. **`git add -A` sweeps unrelated work into your commit.** Stage by path.
10. **A wall-clock budget on a shared 128-core box is a coin flip.** Measure a
    ratio against a baseline taken in the same session, never an absolute bound.
    Baseline for this workstream: `scratchpad/perf-baseline.md`.
11. **Every duplication found has been a missing shared operation.** Add the op
    to `src/canvas` or `src/text` rather than deduplicating in place.
12. **The fit ladder is a first-fit search, which makes new rungs free** — a new
    rung is reachable only at widths that previously failed, so the suite being
    byte-identical is the proof the reasoning held. Repeat that check before
    touching `LADDER`.
13. **An argument can be correct and then be invalidated by a later change, and
    nothing will tell you.** The word-breaking rungs `(1,6)`/`(1,8)` were
    accepted this morning on the explicit argument that *the counterfactual is
    never a prettier diagram; it is the source dump*. Adding scrollable diagrams
    destroys that counterfactual, so the same rungs become indefensible
    (`Star`/`t`, `Repo`/`rt erro`/`r`). Two reviewers found this independently.
    **When you accept a trade-off, write down the premise it rests on**, because
    the premise is what a future change will quietly remove.
    **Done, in Stage 2** (`899156b`): `Fit::ROOMY` drops those rungs *on the
    pager path only* and `--render-once` keeps `Fit::COMPACT`, so `89e9c54`'s
    reasoning is reversed exactly where the better counterfactual now exists and
    nowhere else. This is not a regression — do not "restore" it.
    A second lesson came out of implementing it: **dropping the rungs alone was
    a no-op.** Budget bisection (1a) runs below the tightest *remaining* rung and
    finds the largest budget that fits, which recovers everything a dropped rung
    would have found. The policy needed a width-independent *floor* on the label
    budget (14 columns) to have any teeth. A ladder change is now three things —
    rungs, floor, and the bisection between them — and reasoning about one of
    them alone will be wrong.
14. **Two independent hostile reviewers, briefed differently and blind to each
    other, are worth far more than one.** One audited mechanics against the code
    and found the memory blow-up and the double layout; the other drove the
    binary and found the whole-page scroll drag. Neither found the other's. The
    overlap — the ladder rungs — is the finding to trust most.
15. **A reviewer who cannot reproduce the objection they were briefed to make
    should say so.** Review B was briefed to argue that scrolling an LR diagram
    would be incoherent, tried it, found it reads as a filmstrip, and reported
    that instead. Brief for hostility, not for a predetermined conclusion.
17. **Ask a reviewer to refute your diagnosis, not to implement it.** I told an
    agent the CPU spin was inside crossterm's `read()`, with reasoning. It was
    inside `poll()`, and the agent proved it with an instrumented probe before
    fixing anything. The brief said "confirm or refute, I would rather be
    corrected than have you implement around a wrong theory" — that sentence is
    why the fix is correct. Put it in every brief where you are guessing.
18. **A test can be green for a reason that has nothing to do with the code.**
    The hangup test had two false greens before it was trustworthy: `openpty`
    returns inheritable fds, so two tests in parallel each inherited the other's
    *master* and neither pty ever hung up; and when the test stopped draining
    the pty, the pager blocked in `write` and the test passed while the defect
    was untouched. Ask what would make this test pass with the bug present.
19. **Prefer a ratio against a physical constant over a wall-clock bound.** The
    hangup test asserts the pager burned fewer than 50 jiffies, because a
    spinner burns 100 per second. That holds on a loaded 128-core box where any
    absolute timeout is a coin flip (§4.10 is the same lesson from the other
    direction).
20. **When you enumerate a class, enumerate it exhaustively if you can.** The
    U+17D8 fix checked all 0x110000 scalars against the shipped
    `unicode-width` and established it is the *only* scalar wider than two
    columns. That turns "we fixed the reported character" into "we closed the
    class", and it is the antidote to §4.4's list-of-cases failure mode.
21. **Do not hand-edit insta snapshots.** `INSTA_UPDATE=always cargo test --test
    <target>`, and review each diff — the diff is the check on the fix.

## 5. Don'ts & constraints

- **No HTML.** Not rendered, not passed through.
- **Mermaid is Unicode box art only**, never raster.
- **Nerd Font glyphs are DETECTED, not defaulted on** (spec §2.1). Detection
  answers "yes" only on positive evidence. Do not "fix" it to an unconditional
  default.
- **`Esc` never quits.** It unwinds count → search → TOC filter → TOC focus →
  TOC pane, then says `nothing to cancel — press q to quit`.
- **Do not widen the `NodeArt` seam.** One method,
  `render(node, budget, theme) -> Canvas`. Read what you need off the drawn
  canvas instead — `ruled_offsets` is the pattern.
- **`render` must not depend on `tui`.** The dependency runs one way; policy
  constants like `MAX_BLOCK_WIDTH` live in `tui::wide` and are passed in.
- **Gantt state is carried by colour alone.**
- **No 1000-node golden snapshot** (spec §13.2): a diff nobody can read gets
  rubber-stamped.
- 4-core cap on this machine: `CARGO_BUILD_JOBS=2` on every cargo invocation.

## 6. Where the detail lives

- Change history: `git log 5ac8a5f..HEAD`.
- **Design spec (the authority):** `docs/superpowers/specs/2026-08-08-mdless-design.md`
  — §2.1 icons, §3 the central rule, §4 Canvas contract, §6 Mermaid subsets,
  §7 tables, §10 keys, §13 testing.
- **Active feature spec:** `docs/superpowers/specs/2026-08-09-wide-diagram-scrolling-design.md`
  (revision 2). Its "What the reviews changed" section is the compressed form of
  two reviews worth ~200k tokens.
- **Maintainer notes (judgment):** `docs/maintainer-notes.md`.
- **QA round two (current):** `docs/qa/visual-review-3.md` is the better
  worklist — harsher and more specific than `-2`, and its verdict was "no".
  `docs/qa/visual-review-2.md` and `usability-review-2.md` both said "yes".
  Two said yes, one said no; quoting only the first half of that would be the
  kind of claim this project has been bitten by.
- **QA round one (historical):** `docs/qa/visual-review.md`,
  `usability-review.md`, `code-review.md`.
- Design-review transcripts for the active feature are subagent task outputs,
  not tracked files. Their conclusions are in the spec; the rest is gone.

## 7. Open questions / pending decisions

1. **Nothing is blocked on the user.** They handed the workstream over with
   "you drive this… see you when you are done".
2. **Whether `fc-list` is the right probe on macOS.** Detection falls back to
   plain there, so every Mac user needs `MDLESS_ICONS=1`. Safe but pessimistic;
   a `TERM_PROGRAM` signal was deliberately not added, because guessing from the
   terminal's *name* is the category-enumeration mistake §4.4 warns about.
3. **Nested diagrams get no widening** (list items, blockquotes), while nested
   *tables* do — the same fence behaves differently indented two spaces.
   Accepted for v1 and recorded in the spec's "Out of scope". Revisit.
4. Nothing has been pushed to any remote — re-derive with `git status -sb` and
   `git log --oneline @{u}..` rather than believing this sentence.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.**
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`.
- **The three Stage 1 worktrees are the most volatile thing here.** They were
  running when this was written; by the time you read it they may be merged,
  abandoned, or still going. `git worktree list` and `git branch -a`, then read
  each branch's log. Do not assume the spec's Stage 1 is unimplemented.
- §2's gate numbers reflect `9409229`. Re-run:
  `export CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdless-lead && touch src/lib.rs && CARGO_BUILD_JOBS=2 cargo test`
  — and read the exit code, not the tail of a pipe (§4.2).
- Every width in this file and in the QA reviews is a measurement of a past
  tree. §4.8 exists because that has already caused wasted work once.
