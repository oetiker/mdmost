# Controller Handoff — mdless

> Starter pack for the next controller session. This handoff lives in ONE
> worktree — run `git worktree list` first and confirm this is the workstream
> you're resuming. Read this first, then `git log <handoff-commit>..HEAD` for
> everything that changed since. Detail is NOT here — it's in git and the docs
> named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry
> forward any lesson in §4/§5 that is still true. Fresh synthesis, not blank
> page.

Handoff commit: 5ac8a5f   Date: 2026-08-09   Reason: context budget
Worktree / branch: main checkout (/home/oetiker/checkouts/mdless) @ main
Trunk at time of writing: `main` @ 5ac8a5f — this IS trunk. **Re-derive anyway.**
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

Three findings from `docs/qa/visual-review-3.md` are fixed and on trunk:

- **§1 SEVERE** — a seven-node `flowchart LR` dumped raw source below 92
  columns. Now draws from ~76 at the fence, ~59-62 inner (`89e9c54`).
- **§12 MEDIUM** — the too-narrow caption restated the width you already had.
  Now names a floor (`7de4506`). **A later review showed the floor, while
  honest, is badly wrong in size** — see §3.
- **§15 SEVERE** — no left margin or right gutter in the live TUI, the one
  finding both visual reviewers agreed on (`9409229`). The pager assembles its
  own blocks in `tui::wide::render_scrollable` and had silently dropped the
  inset `render_document` documents.

Gates at `9409229`: 717 tests, 22 suites, all green with the exit code read
directly; clippy `-D warnings` clean; `fmt --check` clean. **Re-derive.**

Since then, trunk carries only docs: the spec for the next feature and its
revision after review.

## 3. Do this next

The active workstream is
`docs/superpowers/specs/2026-08-09-wide-diagram-scrolling-design.md`
(**read revision 2, not revision 1** — revision 1 is in git and is wrong in
four ways the reviews found). It began as "let diagrams side-scroll instead of
dumping source" and two hostile design reviews turned it into a sequence.

**Stage 1, dispatched to three subagents in worktrees at this commit:**

1. **1a — budget bisection.** Fit is non-monotone in width (draws at 63, fails
   at 64, draws at 65) because `graph.rs` computes `budget = (width/share)` and
   never uses *less* budget than a rung grants. Fix: a ninth step after the
   ladder exhausts, bisecting on budget. Runs only on exhaustion, so everything
   that draws today must stay byte-identical.
2. **1b — the horizontal scroll model.** Scrolling drags the *whole page*: one
   wide block scrolls the H1 off-screen and cuts every paragraph mid-word, and
   `g`/`Home`/`0`/`^` all fail to return. Plus `↔ n/N` only appears after you
   have already scrolled, and help/README say the arrows are for "wide tables
   and code".
3. **1c — chevrons on border rows** (`visual-review-3.md` §11), which make a
   clipped table's frame never close. Promoted from "out of scope" because
   these chevrons become the primary signal that a wide diagram continues.

**Stage 2 is the feature itself** and must not start until 1a and 1b land: the
`render::diagram` seam returning `(width, Canvas)` (returning only the width
lays out every fitting diagram twice, +43 % startup), a width cap of ~3×
viewport (one 929-column diagram costs 7× peak RSS because `Canvas::append`
pads every row of the document), a probe cap of 8 (`pie` reports no floor and
would linear-scan to 2048), a minimum surplus (never give the page a scrollbar
to gain one column), and the ladder split described below.

**After Stage 2, still open from `visual-review-3.md`:** §16 SEVERE (heading
hierarchy carried by hue; light theme's six-level ramp measured flat at
4.80→4.95→4.86:1 — `tests/theme_headings.rs` exists to extend), the diagram
routing findings (§2, §3, §8, §9), and `usability-review-2.md` findings 2-13.

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
16. **Do not hand-edit insta snapshots.** `INSTA_UPDATE=always cargo test --test
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
