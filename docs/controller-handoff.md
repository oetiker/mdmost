# Controller Handoff — mdmost

> Starter pack for the next controller session. This handoff lives in ONE
> worktree — run `git worktree list` first and confirm this is the workstream
> you're resuming. Read this first, then `git log <handoff-commit>..HEAD` for
> everything that changed since. Detail is NOT here — it's in git and the docs
> named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry
> forward any lesson in §4/§5 that is still true. Fresh synthesis, not blank
> page. On merge into another branch, rewrite that branch's handoff to the
> merged reality — do not merge or preserve this text.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-10   Reason: context budget
Worktree / branch: main checkout @ `main`. The directory is
`/home/oetiker/checkouts/mdmost`, moved from `…/mdless` on 2026-08-10 together
with its Claude state directory (§7.1). Transcripts written before that date name
the old path.
Trunk at time of writing: `main` @ `3820aec` — **reader: if trunk has moved, §2
is provisionally stale; if trunk now contains this branch's HEAD, this file is a
tombstone** (`git merge-base --is-ancestor HEAD main`). At that commit: 930 tests
across 28 suites green, `cargo fmt --check` clean, `cargo clippy --all-targets --
-D warnings` clean, and `cargo check --target x86_64-pc-windows-msvc
--all-targets` clean. **Re-derive anyway** (§8).
Sibling worktrees: **none** — `git worktree list` shows one entry. The 33 dead
worktrees the previous handoff warned about are gone; that housekeeping item is
closed. This line cannot see worktrees created later; check yourself.

## 1. Mission

`mdmost` is a full-screen terminal pager for a single Markdown document — "as
pleasant to look at as btop, as pleasant to use as less". Rust + ratatui. GFM
including tables with Markdown inside cells, syntax-highlighted code, and seven
Mermaid families drawn as Unicode box art. Reflows on resize, TOC pane, search,
themes, mouse.

The mental model that matters: **rendering is a pure function of
`(AST, width, theme, options)`**. Parse once; no layout decision at parse time;
a resize discards the canvas and renders again.

**The workstream has shifted.** Previous sessions built and polished the
renderer. This one turned to **getting the program to users**: a rename, a
publishing design, and the product decisions that surfaced once output was being
looked at as a shipped artefact rather than as test fixtures. Expect the next
stretch to be execution of the publishing plan (§6), not renderer work.

**How the owner works, and it is not optional.** Everything is driven through
subagents; reviewers drive the real binary in tmux rather than trusting tests;
report when it is done, do not consult on mechanics. They review by *looking at
output*, and their findings are consistently precise. **When they ask a design
question, answer it with rendered samples, not prose** — this session, "what
would the title look like if it was multiline?" was answered by actually
rendering it, and the answer decided the feature in one exchange. They will cut
through analysis that has become over-engineered; treat that as direction, not as
a request for more options.

## 2. Where we are now

Five commits since `cacdf95`, all on `main`, none pushed **as of the handoff
commit** — re-derive (§8). There is still **no git remote configured at all**.

- **`72f82b1` — the rename.** `mdless` was taken, so the project is `mdmost`
  (via a short-lived `mdmst`, §4.1). Crate, `[[bin]]`, `MDMOST_ICONS`,
  `~/.config/mdmost/`, docs, tests, the design spec's filename. Zero survivors of
  either old name.
- **`87cc7f5` — the publishing design**, `docs/superpowers/specs/2026-08-09-publishing-design.md`.
  Approved by the owner ("I like it").
- **`e2626b0` — `DEFAULT_BODY_WIDTH` is 72**, down from 100.
- **`d57b580` — `--render-once` now renders what the pager renders.** It called
  `render_document` (the uncapped primitive), so `--body-width` was accepted and
  silently discarded on the pipe path. Both paths now go through
  `tui::wide::render_scrollable`; a dump may be wider than `--width` where a
  table earns it, which the owner chose explicitly (§4.4).
- **`3820aec` — the title banner wraps, and is opt-in.** A long title is broken
  between words into stacked, individually centred bands; `Letter` gained
  `row`/`rows` so a search hit in one band no longer lights the others.
  `title_banner` now defaults to **false** in both `Config` and `RenderOptions`.

**The publishing plan is written and committed but never reviewed and never
executed**: `docs/superpowers/plans/2026-08-09-publishing.md`, six tasks. It is
also **two edits out of date** — see §3.2. Nothing in `.github/` exists yet;
`CHANGES.md`, `LICENSE-MIT`, `man/`, `Formula/` and `demo/` do not exist yet.

## 3. Do this next

1. **Answer the goldens question, then act on it.** `tests/snapshot.rs:20`
   renders through `render_document` — the uncapped primitive that, since
   `d57b580`, **no user-facing path uses**. 375 lines differ between a golden and
   what the binary emits for the same fixture at the same width. The suite is
   green while guarding something nobody can see. Options put to the owner and
   not yet answered: regenerate against the real path; keep these as
   layout-primitive tests and add a small second set through the binary; or defer
   deliberately. This is the highest-value open item because every later change is
   measured against these files.
2. **Fix the two known staleness bugs in the publishing plan before executing
   it.** Task 2's man page must document `title_banner` as opt-in (the plan's
   roff was written when it defaulted on), and Task 5's demo script must set
   `title_banner = true` — otherwise the recording opens with a plain heading and
   undersells the tour. Both are in `docs/superpowers/plans/2026-08-09-publishing.md`.
3. **Then execute the plan**, Task 1 first (it is the only one with a source
   change: one `#[cfg(unix)]` in `src/tui/term.rs:188`, already proven necessary
   and sufficient, §4.5).

## 4. Lessons & traps ← the irreplaceable part

Carried forward from the previous handoff and still true: 4.1-4.20 of that
version, in particular **give every agent its own `CARGO_TARGET_DIR`**; **never
read a gate's result through a pipe**; **the standing clippy gate is
`cargo clippy --all-targets -- -D warnings`** because plain `cargo clippy` exits
0 on warnings; **verify a subagent's arithmetic, not its adjectives** (test count
is the check); **prove every behavioural test red before you fix**; **do not
choose glyphs by measuring them**; **do not hand-resolve snapshot conflicts**.
Read `git show 72f82b1:docs/controller-handoff.md` for the full text — it is
worth the tokens, especially §4.9 and §4.10 if anything touches glyphs. (That is
the last commit that carried the previous version; `git log --follow --
docs/controller-handoff.md` finds it if the SHA has drifted.)

New this session:

1. **A rename is not a `sed`, in two specific places.** The insta snapshots
   embed the name inside *width-aligned box art*, so substituting a
   different-length name corrupts every border on the line — restore and
   regenerate instead. And `src/render/banner/tests.rs` holds hand-transcribed
   FIGlet art of the project name as a fixture, with its exact column width and
   letter count asserted: `mdless` → `mdmst` → `mdmost` moved that art three
   times (29 columns, 6 letters now). Budget for both before starting; a rename
   here is a 20-minute job, not a 2-minute one.
2. **That banner fixture is normally checked against `figlet -f Small`, and on
   this machine it cannot be.** The `Small` font is not installed (`figlet` is,
   the font is not), so the current reference art came from our own layout. The
   neighbouring smushing test still pins the algorithm against real figlet
   output, so this is not unguarded — but **the project-name test is currently
   self-referential**, and its doc comment claims otherwise. Worth re-deriving
   against a real `figlet -f Small mdmost` if the font ever appears.
3. **A default that "changes nothing for anyone" is a default doing nothing.**
   The old 100-column body cap was justified by exactly that property — it never
   bit below 102 columns. That is not a conservative default, it is an inert one.
   72 was chosen to actually shorten the measure. Watch for the same reasoning
   elsewhere in this codebase; it is a house style and it is sometimes wrong.
4. **When two code paths render "the same" thing, prove it rather than assuming
   it.** The cap change looked applied and was not: `--render-once` had its own
   renderer, and the divergence had been invisible because *the golden tests use
   the same wrong path* (§3.1). The tell was cheap — render the same file both
   ways and diff. **Do that diff whenever you change layout policy**, because the
   test suite cannot see a disagreement it is standing on the wrong side of.
5. **Windows portability was one line, and only a real cross-check found it.**
   `SIGHUP` has no Windows counterpart (`SIGTERM`/`SIGINT` do). Reading the
   `cfg` gates in the source suggested portability was handled; it was not.
   `rustup target add x86_64-pc-windows-msvc` + `cargo check --target …` is cheap
   and conclusive — **a `cargo check` against the real target std is worth more
   than any amount of reading `#[cfg]` attributes.** Note what it still does not
   establish: check is not link, and link is not run. Nobody has ever *run*
   mdmost on Windows.
6. **Ask the design question with a rendered artefact.** The multiline-banner
   decision took one exchange because the option was shown as real art at a real
   measure, produced by a throwaway `#[test]` that printed it and was then
   deleted. A temporary test that prints is a legitimate instrument here; just
   `git checkout --` the file afterwards and confirm with `git status`.
7. **Flipping a default breaks the tests that used the old default as a
   convenience, and those are not the tests about the feature.** `NO_BANNER` in
   `src/render/tests.rs` existed so that one-heading documents (the shortest way
   to write any heading test) would not get a banner. With the banner off by
   default it became meaningless and its inverse — `BANNER` — was needed by the
   two tests whose subject the banner actually is. **When you flip a default,
   grep for the constant that used to suppress it**; it is usually load-bearing
   for unrelated tests.
8. **Keep every commit green when splitting work into commits.** The
   `--render-once` fix and the banner change touched one file in common
   (`tests/app_cli.rs`), where an assertion belonged to the *second* commit. Both
   commits were made green by staging an intermediate version of that file, then
   restoring. Running the suite against the working tree does not verify the
   staged commit — reason about what that commit contains, or check it out.
9. **My own commit splitting failed once, silently.** `git add -A` followed by
   two `git commit` calls put everything in the first and left the second empty
   (`--allow-empty` hid it). Caught by `git show --stat`. **Verify a split by
   inspecting both commits, not by the exit code of the commands that made
   them.**

## 5. Don'ts & constraints

Carried forward and still binding: **no HTML**; **Mermaid is Unicode box art
only**; **bullets and task boxes are ASCII and do not vary by font detection**;
**Nerd Font glyphs are detected, not defaulted on**; **`Esc` never quits**; **do
not widen the `NodeArt` seam**; **`render` must not depend on `tui`** (see §7.3 —
this one now has a wrinkle); **`#![forbid(unsafe_code)]`** in the library; **the
status bar never lies** and on-screen key hints come from the live key table;
**no 1000-node golden snapshot**; **4-core cap on every cargo invocation**;
**tmux: kill only your own session**; **leave no stray `mdmost` processes**.

New or changed:

- **The name is `mdmost`.** It went `mdless` → `mdmst` → `mdmost` in one
  session; do not "correct" it back, and do not assume the directory name is the
  project name.
- **Nothing has ever been pushed, and there is no remote.** Creating
  `github.com/oetiker/mdmost` and pushing is a step in the publishing plan and
  the owner's to take. Do not push without asking.
- **The title banner is opt-in and stays opt-in.** Settled with reasoning: art in
  place of somebody else's title is a decoration, and a default is the wrong
  place to hold that opinion. Do not relitigate.
- **`--render-once` may emit lines wider than `--width`.** Deliberate, chosen by
  the owner over keeping an exact-width guarantee, so that unreflowable content is
  laid out rather than mangled. The README must not promise exact width.
- **No apt/yum repository, no GitHub Pages site, no container image, no macOS
  notarisation.** All four were considered and rejected with reasons recorded in
  the publishing spec §1. The Pages repository in particular was dropped by the
  owner on bandwidth grounds *after* being designed; do not re-propose it.

## 6. Where the detail lives

- **Publishing design (new, the authority for the current workstream):**
  `docs/superpowers/specs/2026-08-09-publishing-design.md`
- **Publishing plan (unexecuted, two known stale spots):**
  `docs/superpowers/plans/2026-08-09-publishing.md`
- **Design spec (the authority for the renderer):**
  `docs/superpowers/specs/2026-08-08-mdmost-design.md` — §3.2 body width, §9.2
  the banner; both were edited this session to match the new behaviour.
- **Feature spec:** `docs/superpowers/specs/2026-08-09-wide-diagram-scrolling-design.md`
- **Maintainer notes:** `docs/maintainer-notes.md`
- **QA:** `docs/qa/visual-review-3.md` is still the best renderer worklist; its
  "what is genuinely good" section is the list of things not to break.
- Key files this session: `src/render/banner.rs` (`wrap`, `stack`, `Letter.row`),
  `src/render/tests.rs` (`BANNER` / `lines_with`), `src/config.rs:49`
  (`DEFAULT_BODY_WIDTH`), `src/main.rs` (`render_once`), `src/tui/wide.rs`
  (`Measure`, `render_placed` — the cap rule lives here), `tests/app_cli.rs` (the
  two new dump tests).
- Reference repos for the publishing work, all read this session:
  `~/checkouts/ansidrama` (closest model: musl targets, deb/rpm, and the demo
  recorder itself), `~/checkouts/edaptor`, `~/checkouts/byonk` (container job and
  Windows binaries).

## 7. Open questions / pending decisions

1. ~~The checkout directory~~ **Settled 2026-08-10.** Moved to
   `/home/oetiker/checkouts/mdmost`. The owner's reason for holding off was that
   Claude keys its per-project state on the absolute path, so
   `~/.claude/projects/-home-oetiker-checkouts-mdless/` was renamed to
   `…-mdmost` in the same step, carrying the 29 session transcripts and the
   `memory/` directory with it.
2. ~~The goldens question~~ **Settled 2026-08-10.** Answered "regenerate against
   the real path and drop the code", and done: the goldens go through
   `render::document::render_document` at the shipped body cap, and the flat
   primitive is crate-private as `render::render_flat`. See §2.
3. ~~`--render-once` calls into `src/tui/wide.rs`~~ **Settled 2026-08-10.** The
   tidy answer named here was taken: the module is `src/render/document.rs` and
   the function is `render::document::render_document`. `tests/body_width.rs` was
   touched, as predicted.
4. **Nobody has ever run mdmost on Windows** (§4.5), and as of 2026-08-10 it does
   not even compile there: `cargo check --target x86_64-pc-windows-msvc` fails on
   `SIGHUP` at `src/tui/term.rs:188`. **The previous handoff claimed this gate was
   clean at `3820aec`; it was not** — the one-line `#[cfg(unix)]` is Task 1 of the
   publishing plan and has never been applied. Treat the other three gates'
   provenance with the same suspicion. The mouse, the clipboard and the alternate
   screen are unexercised there regardless.
5. **The banner fixture is self-referential on this machine** (§4.2).
6. Carried forward and still open: `fc-list` as the macOS icon probe is untested
   there; nested diagrams are never widened while nested tables are; Nerd Fonts
   v2 patches fail icon detection (deliberate, safe direction); a copy made just
   before `q` still dies on a desktop with no clipboard manager.
7. **Search still does not match inside fenced code blocks** — the previous
   handoff's top-priority defect, untouched this session, and still the highest
   value renderer bug for a pager aimed at code documents. The light theme's flat
   heading ramp (measured 4.80 → 4.89 → 4.92 → 4.95 → 4.90 → 4.86:1, non-monotone)
   is likewise untouched. **Re-measure before designing** — reviews and
   measurements go stale within hours.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** Whether this branch
  is merged, pushed or superseded is not knowable from this file:
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`. If this branch is merged, stop reading and go
  to the successor's handoff.
- **Sibling worktrees / other workstreams may exist that this file cannot name** —
  anything started after the handoff commit is invisible here.
- The "no remote" claim rots the moment the owner creates the repository, which
  is step one of the publishing plan. `git remote -v`.
- The 930-test count and the four clean gates are as of `3820aec`. Re-run them;
  a count that moved without an explanation is the signal (§4 carried-forward).
- The §7.7 renderer defects are inherited from the previous handoff and were not
  re-verified this session. They may have been fixed, or the measurements may
  have drifted.
