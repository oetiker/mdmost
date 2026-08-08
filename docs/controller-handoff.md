# Controller Handoff — mdless

> Starter pack for the next controller session. This handoff lives in ONE
> worktree — run `git worktree list` first and confirm this is the workstream
> you're resuming. Read this first, then `git log <handoff-commit>..HEAD` for
> everything that changed since. Detail is NOT here — it's in git and the docs
> named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry
> forward any lesson in §4/§5 that is still true. Fresh synthesis, not blank
> page.

Handoff commit: 67e135c   Date: 2026-08-08   Reason: context budget
Worktree / branch: main checkout (/home/oetiker/checkouts/mdless) @ main
Trunk at time of writing: `main` @ 67e135c — this IS trunk; there is no other
branch. **Reader: re-derive anyway** (`git log --oneline -5`).
Sibling worktrees: four stale scratch worktrees under
`/scratch/oetiker/claude-worktrees/` (`mdless-gate` @ 6e4fcc6, `mdless-layout`
@ 66e2898, `mdless-qa` @ 8883ed0, `mdless-rendercheck` @ bdd05f1). All are dead
agent scratch, all behind trunk, none owns any workstream. **Delete them** (ask
the user first — they are under `/scratch`, and this project's convention is to
confirm before `rm`). Nothing of value is in them; every agent's work was swept
into trunk.

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

The project was built by a fan-out of ~12 agents over one session, then put
through three adversarial QA reviews (visual, usability, code), then a full fix
wave against those findings. It is feature-complete and green.

## 2. Where we are now

As of the handoff commit: **697 tests passing, 0 failures, `clippy
--all-targets -- -D warnings` clean, `cargo fmt --check` clean.** Re-derive
before trusting (§8).

Everything in the design spec is implemented:

- Foundation: `Canvas` contract, `text` width/wrapping primitives, `doc` AST,
  `theme` with two built-ins plus TOML themes.
- Renderers: inline, block, table (recursive Markdown in cells, natural-width
  sizing, horizontal overflow), code with syntect mapped onto the mdless
  palette.
- All seven Mermaid families render: flowchart, sequence, class, ER, state,
  pie, gantt. Class/ER/state sit on the shared graph engine without having
  needed a single change to it.
- Pager: horizontal scrolling (per-block widening via `tui::wide`), TOC pane,
  search with literal/regex modes, scrollable help overlay, themes, config,
  `--render-once` for headless/scripted use, `$PAGER`/stdin path.
- README, spec, `docs/maintainer-notes.md`, three QA reports in `docs/qa/`.

**What is NOT verified:** the last workstream (`fix-tui`) ended without ever
sending a status report, and its final uncommitted changes were committed by me
sight-unseen at 67e135c (gate was green with them). So the pager's *polish*
items — the ~15 smaller usability findings — are of unknown completeness. Some
are demonstrably done (I drove them in tmux myself: horizontal scroll,
non-destructive Esc, scrollable help, position report, status-bar separators).
Others were never confirmed: theme background fill across TOC pane and
overlays, TOC current-section tracking, `[toc] open` config field being read,
SIGPIPE on `mdless x.md | head`, empty-document message, sticky TOC filter,
search centring, SIGINT registration, startup performance on large documents,
and whether `tui/icons.rs` still duplicates the eighth-block maths now that
`canvas::meter` exists (and whether it has a display-width test — its twin in
`render/glyphs.rs` had one and found a real bug through it).

**Two QA agents (`qa-visual2`, `qa-use2`) were launched against this build and
died before reporting.** That second review round is the single biggest open
item — see §3.

## 3. Do this next

1. **Re-run the second QA round.** Two fresh reviewers, one visual and one
   usability, driving the real binary in tmux. Briefs: be ruthless, do NOT read
   `docs/qa/` (those are the first round — you want independent eyes, not
   confirmation of an existing list), and build to a **private**
   `CARGO_TARGET_DIR` (see §4, trap 1). The first round's verdicts were both
   "no"; the whole fix wave was aimed at moving them. Nobody has yet judged the
   fixed build.
2. **Audit the unverified pager polish list in §2** — cheapest as a single
   agent owning `src/tui/**` that walks the list and reports done/not-done
   before any new work is commissioned.
3. **Ask the user about the icons default** (§7) — it is the one open product
   question and two capable reviewers disagreed with the current answer.

## 4. Lessons & traps ← the irreplaceable part

1. **The shared `CARGO_TARGET_DIR` is the most expensive hazard in this
   project.** `~/scratch/cargo-target` is shared, so with several agents
   building concurrently: (a) test binaries go stale, so `cargo test` reports
   results for code that is not on disk — symptoms include "0 passed, N
   filtered out" while `--list` shows the tests present, and a finished module
   reporting itself as an unimplemented stub; (b) the binary at
   `debug/mdless` is *whichever agent built last*, so a visual check may be
   showing you someone else's tree entirely; (c) a build race can replace the
   rlib mid-rustdoc, making every doctest "fail" to compile. This bit or nearly
   bit **every** agent, including the one who discovered it — twice, once badly
   enough to report a regression that did not exist. Mitigation: give each
   agent its own `CARGO_TARGET_DIR`, and `touch src/lib.rs && cargo build`
   before regenerating a snapshot or believing a surprising result. One agent
   nearly committed snapshots that silently reverted its own fixes this way.
2. **False doc comments are the most dangerous defect class here.** The canvas
   width bug survived the entire project behind a comment claiming the clamp
   existed "so that the canvas contract cannot be violated" — it did the
   opposite. A dead `pub fn` claimed "the viewport uses this for horizontal
   scrolling"; it had no caller. `--help` advertised `--no-icons` as the
   default after the default was reverted. **No test catches any of these.**
   When you change behaviour, grep the prose.
3. **One root cause reached four separate sites.** `grapheme_width` clamps a
   cluster to ≤2 columns, but a wide base plus a *spacing* mark (category `Mc`,
   not `Mn`) draws 3. Found in `Canvas::write_str`, `text::wrap`,
   `truncate_to_width`, and `render/inline.rs` (where it dragged search spans
   left of their own text — a user would report that as "search highlights the
   wrong characters"). Treat any `grapheme_width`-based arithmetic as suspect.
   `check_invariants` now asserts `display_width(cell.text()) == cell.width()`,
   which is what makes the class findable at all.
4. **Every duplication found was a missing shared operation, not a lazy
   author** — and each workaround had quietly reintroduced a bug the shared
   version did not have (three `align_offset` clones used bare subtraction
   where the shared one saturates; two of them were latent panics). The tell
   was `table.rs` importing a constant from `code.rs`: someone knew it should
   be shared and had nowhere to put it. If you find duplication, add the op to
   `src/canvas` or `src/text` rather than deduplicating in place.
5. **Tests here have twice passed with *and* without their fix.** Two agents
   caught this in their own work by deliberately disabling the fix and
   confirming the test went red. Do that for every behavioural test. A code
   reviewer separately found a snapshot test that was green and empty
   (snapshotting a placeholder) and an assertion that was a tautology.
6. **Agents go idle without reporting, constantly.** Silence means nothing.
   Verify state yourself (`git status`, `wc -l`, run the binary) rather than
   asking a third time. Conversely, **do not conclude an agent produced nothing
   because the checkout looks empty** — one was working in a worktree; I wrote
   it off, respawned its task, and the two collided, destroying the
   replacement's work. `git worktree list` before writing anyone off.
7. **Reviewer findings are wrong often enough to check.** Two were wrong on
   contact with the code (a `.parse().ok()` that must stay `None` because it is
   how the parser distinguishes a date from an id; a `NotImplemented` variant
   that would have had no constructor). Both were caught by agents reading the
   surrounding contract instead of applying the finding mechanically.
8. **`git add -A` sweeps other agents' dirty files into your commits** — I
   committed unformatted files this way and had to fix the fmt gate afterwards,
   and committed one agent's work mid-edit. It is the right default when you
   are the only committer, but check `git status` first.
9. **Do not hand-edit insta snapshots.** I stripped `assertion_line:` metadata
   trying to accept some by hand and turned 6 failures into 7. Use
   `INSTA_UPDATE=always cargo test --test <target>`, and **review each diff** —
   blind acceptance is how a regression becomes an expectation.

## 5. Don'ts & constraints

- **No HTML.** Raw HTML is not rendered and not passed through. Settled in the
  original design Q&A.
- **Mermaid is Unicode box art only**, never raster. Settled the same way.
- **Nerd Font glyphs are the default** (`--no-icons` is the escape hatch).
  Settled explicitly in the design brief and spec §2 — but see §7, this is the
  one I would reopen with the user.
- **`Esc` never quits.** It unwinds count → search → TOC filter → TOC focus →
  TOC pane, then says `nothing to cancel — press q to quit`. The spec was
  changed to match the implementation, not the reverse.
- **Do not widen the `NodeArt` seam.** One method, `render(node, budget, theme)
  -> Canvas`, measuring and painting in one call so they cannot drift. Four
  families were built on it without changing it. When the engine needed to know
  about compartment rules, reading them back off the drawn canvas was cheaper
  and more general than a new method.
- **Gantt state is carried by colour alone.** Reintroducing per-state fill
  densities reintroduces the finding that the *default* state became the least
  visible thing on the page.
- **No 1000-node golden snapshot** (spec §13.2 records why): a diff nobody can
  read gets rubber-stamped and manufactures false confidence. Scale is covered
  by property tests.
- 4-core cap on this machine: `CARGO_BUILD_JOBS=2` on every cargo invocation.

## 6. Where the detail lives

- Change history: `git log 67e135c..HEAD`, and `git log --oneline` for the
  ~20-commit build.
- **Design spec (the authority):** `docs/superpowers/specs/2026-08-08-mdless-design.md`
  — §3 the central rule, §4 Canvas contract, §6 per-family Mermaid subsets,
  §7 tables, §10 keys, §13 testing.
- **Maintainer notes (judgment, not task state):** `docs/maintainer-notes.md`
  — the engine seam, the cell-width contract, why gantt is colour-only, the two
  verification traps.
- **First-round QA reports:** `docs/qa/visual-review.md`,
  `docs/qa/usability-review.md`, `docs/qa/code-review.md`. Both user-facing
  verdicts were "no". Most findings are fixed; they are the record of what
  "harsh enough" looks like for this project.
- `README.md` — key map is generated from the live binding table, so it cannot
  drift from the help overlay or the code.

## 7. Open questions / pending decisions

1. **The icons default.** Nerd Font glyphs are on by default, per the explicit
   design decision. But two capable agents independently argued for
   plain-Unicode-by-default, because you cannot ask a terminal whether it has a
   patched font, so most first runs on an unpatched terminal show holes where
   heading markers and task boxes should be. One of them flipped the default
   and I overruled it on the grounds that the decision was already settled on
   the record. **This is the user's call and worth putting to them plainly.**
2. **Whether the pager polish list in §2 is actually done.** Unknown, not
   unresolved — it needs an audit, not a decision.
3. `src/render/glyphs.rs` has a display-width test for its icon set;
   `src/tui/icons.rs` does not, and `tui/chrome.rs` sizes the status bar with
   `display_width`, so one double-width icon would shift every right-hand
   segment. The render-side twin found a real bug through exactly this test.

## 8. Staleness watch

- The §2 test count and gate status reflect commit 67e135c. **Re-run**:
  `export CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdless-lead && touch src/lib.rs && CARGO_BUILD_JOBS=2 cargo test`.
- The §2 "not verified" list is my inference from an agent that never reported,
  not a tested claim. Some of it may well be done.
- **Integration state must be re-derived, never inherited.** Whether this
  branch is merged, pushed or superseded is not knowable from this file:
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`. If this branch is merged, stop reading and
  go to the successor's handoff.
- **Sibling worktrees / other workstreams may exist that this file cannot
  name** — anything started after the handoff commit is invisible here.
