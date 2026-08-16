# Controller Handoff — mdmost trunk, the demo tour re-think shipped and unpushed

> Starter pack for the next controller session. This handoff lives in ONE worktree — run
> `git worktree list` first and confirm this is the workstream you're resuming. Read this
> first, then `git log <handoff-commit>..HEAD`. Detail is NOT here — it is in git and in
> the documents named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry forward any
> lesson in §4/§5 still true. Fresh synthesis, not blank page. On merge into another
> branch, rewrite that branch's handoff to the merged reality — do not preserve this text,
> and tombstone the branch you merged.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-16   Reason: milestone — project B is done, reviewed and committed; nothing is pushed
Worktree / branch: main checkout `/home/oetiker/checkouts/mdmost` @ `main`
Trunk at time of writing: this **is** trunk. `main` @ `73fb4f6`, **18 commits ahead of
`origin/main` (`56da66b`) and not pushed.** Re-derive with `git status -sb`; do not trust
this line.

Sibling worktrees, as of this commit — verify with `git worktree list`, this line cannot
see anything created later:

- `/scratch/oetiker/claude-worktrees/mdmost-tryout` — detached at `b2ccc74`, merged. The
  owner's test binary and pages (`links.md`, `buttons.md`). Never commit here.
- `/scratch/oetiker/claude-worktrees/mdmost-verify` — detached at `172d39b`, merged. Unused
  this session: project B was recording work, not gate work, and ran on trunk with the
  owner's explicit consent.

## 1. Mission

`mdmost` is a full-screen terminal pager for one Markdown document — "as pleasant to look
at as btop, as pleasant to use as less". Rust + ratatui. GFM including tables with
Markdown inside cells, syntax-highlighted code, and seven Mermaid families as Unicode box
art.

Load-bearing mental models, all still true:

1. **Rendering is a pure function of `(AST, width, theme, options)`.** Parse once; no
   layout decision at parse time; a resize discards the canvas and renders again. Anything
   depending on the pointer — hover, the selection wash, `[copied]` — is **paint-time** in
   `src/tui/`, never render-time. `render` must not depend on `tui`.
2. **A `SearchSpan`'s source is a byte-for-byte copy of the cells it names.** A `Hotspot`
   is deliberately exempt: it claims *drawn cells*, not source bytes.
3. **Syntax comes off the source; text is decoded at the leaves.**

**How the owner works, and it is not optional.** They review by *looking at rendered
output*, and their findings are consistently precise. **Answer a design question with a
rendered sample, never with prose.** They cut through over-engineering; treat that as
direction. When they reframe a question, the reframe *is* the design. They move fast on
their own — a PR opened during a session was merged and released while it still ran.

## 2. Where we are, as of the handoff commit

Re-derive rather than inherit (§8).

**Project B — the demo tour re-think — is DONE.** Spec, plan and ten tasks, all reviewed,
`fe88c75..73fb4f6`. The tour was re-staged from ansidrama 0.2.0's timing model onto
0.4.0's. Same seven acts, same story, as the owner ruled.

What actually changed:

- **The blocker is gone.** `settle_ms` no longer exists in the config (or in ansidrama).
- **15 `await` gates** across all seven acts. *(`grep -c await` says 24; nine of those are
  comment prose. `grep -c '^await = '` is the real count — I got this wrong for most of a
  session.)* A run that completes now means every gated beat matched.
- **Act 6 hovers with a real `move` scene.** All three hand-typed SGR escapes are gone, and
  the hover frame finally shows an arrow on the link instead of a text caret.
- **The theme beat is restored** and verified by eye by three separate parties.
- **Act 5's three sacrificial `g`s are deleted** — three independent runs passed with only
  the `await`. See §4.6 before you trust that on another machine.
- **Act 4's `keys = []` re-capture is deleted**, confirmed by two independent trials.
- `demo/tour.md` no longer claims the status bar names the copied form "every time".
- `docs/maintainer-notes.md` describes the recorder that exists, with a numbered
  regeneration recipe.
- The artifact: 922 frames, 1,594,864 bytes (was 906 / 1,718,646).

**Everything above is committed and NOTHING is pushed.** The final whole-branch review
said "fit to merge and push".

## 3. Do this next

1. **Push, or ask.** 18 commits sit on trunk unpushed. The final review cleared them. This
   is the owner's call, not yours to assume either way.
2. **The documentation design spec is STILL unruled** —
   `docs/superpowers/specs/2026-08-13-documentation-design.md`, "awaiting owner review"
   since 2026-08-13, now three sessions old. Ask; do not start implementing it unasked.
3. **mdmost's release workflow has still never executed.** `git tag` is empty. Its first
   run is its first test and is worth watching, not firing and walking away.
4. **`~/checkouts/ansidrama` has an uncommitted `Cargo.lock`** (0.3.0 → 0.4.0), a byproduct
   of building it here. Not this repo's to commit; mention it to the owner.

## 4. Lessons & traps ← the irreplaceable part

Carried forward and still true: **give every agent its own `CARGO_TARGET_DIR`**; **never
read a gate's result through a pipe** (a redirect is not a pipe; `${PIPESTATUS[0]}` when
you must); clippy is `--all-targets -- -D warnings`; **fault injection must run
`--no-fail-fast`**; **read the log, not the exit code**; **verify a subagent's arithmetic,
not its adjectives**; **do not choose glyphs by measuring them**; **a rename is not a
`sed`**; **when two code paths render "the same" thing, prove it**; **measure box art in
columns, not bytes**; **backticks in `git commit -m` are command-substituted — use a quoted
heredoc**; **`git merge` does not accept `-F -`**; **never merge into a dirty worktree**;
**`git status --porcelain --ignored` before removing a worktree**; **diagnose a silent
agent by the worktree, never by the silence**; **state process constraints as actions, not
prohibitions**; **dispatch at most ONE file-writing agent per worktree**; **THE CONTROLLER
IS A WRITER** — own `mdmost-verify` for gate runs and mutations, never an implementer's
tree; **read coordinates off the screen, never derive them**; **a missing test is only a
gap if you can name a mutation that survives without it**; **a Minor is only Minor while
its blast radius holds**; **a gate is tested by the case where it does NOT fire**; **prove
a mutation makes a test FAIL, not skip**; **`read -t N </dev/null` is not a delay** (use
`timeout N tail -f /dev/null`); **before discarding a dirty tree, read the diff**; **an
abandoned branch can hold the only copy of a real fix**; **integration state changes UNDER
you, not just between sessions**; **verify a claim about a neighbouring codebase by reading
it, not by inferring**.

New this session, and the recording-specific ones are the valuable half:

1. **`await` patterns are REGEXES.** `Pattern::new` → `Regex::new` (`regex_lite`, no
   implicit `(?m)`). Everyone — plan, implementers, reviewers, me — treated them as literal
   strings for five tasks. Two consequences, and the second is why it matters: `[copy]` is a
   **character class** matching one of `c`/`o`/`p`/`y`, so a gate written the obvious way
   "passes" while verifying nothing; and a regex can express **structural** discriminators
   that scoping cannot.
2. **The best gate separates by construction, not by coincidence.** Three gates in this
   plan were sent back for working by accident — a one-character wrapping margin, and rows
   where the other pane "happened" to be blank. Their replacements: `│` (U+2502, drawn only
   by the renderer) versus `|` (ASCII, all the raw Markdown has), and `^` (offset 0 = frame
   column 1 = the left pane, while the split exists). **When a gate's justification is a
   measurement rather than a mechanism, it will rot silently.**
3. **`row` is 0-indexed; `tmux capture-pane` is 1-indexed.** Moving between them cost an
   attempt. Also: mdmost draws a **1-column left margin**, so a bare `^` against mdmost
   content never matches — it needs `\s*`.
4. **An aborted run leaves the tmux server alive**, and the next `new-session` silently
   no-ops. The ONLY symptom is "the launched command exited after scene 00", which looks
   nothing like a socket problem. `tmux -L mdmost-demo kill-server` after any abort. Found
   because fault injection deliberately aborts runs — a trap nobody predicted.
5. **Some things cannot be gated, and saying so is the right answer.** The theme beat's
   broken frame carried a *correct* caption over a wrong colour; no text pattern can catch
   that. An honest ungated beat with a comment beats a decorative `await`, because a
   decorative gate makes a run look verified when nothing was verified.
6. **Three clean runs on one machine is evidence, not proof.** Act 5's `g`s were deleted on
   that basis and it is probably right, but the hazard is timing-dependent. If act 5 ever
   aborts on its `await`, re-add one `g` — that remedy is written into the notes precisely
   because the keystroke did not survive to carry it.
7. **Test, don't infer — three separate times this session the confident answer was wrong.**
   My whitespace anchor, a reviewer's "the table has no vertical rules", and a reviewer's
   "the nano rc pin doesn't exist" (it is the `-I` flag). Each was settled in one command by
   someone who looked instead of reasoning.
8. **Verify an absence before reporting it.** I nearly reported a documented fact missing
   because my grep used the wrong wording, and nearly accepted deleting two `settle_ms`
   mentions that were legitimate history. A pattern that found nothing is not proof.
9. **Reviewers here finished their work and then ended the turn without sending it — six
   times out of nine.** The work was always done and always correct. Chase with
   `SendMessage`; never re-dispatch, and never read silence as either success or failure.
10. **Give a reviewer the specific question, not just the diff.** Every substantive finding
    this session came from a pointed question with evidence attached ("here is my
    measurement, verify it and rate the risk yourself"). Open-ended review requests produced
    approvals; targeted ones produced the four findings that mattered.

## 5. Don'ts & constraints

Carried forward and binding: **no HTML rendering**; **Mermaid is Unicode box art only**;
**bullets and task boxes are ASCII**; **Nerd Font glyphs are detected, not defaulted on**;
**`Esc` never quits**; **do not widen the `NodeArt` seam**; **`render` must not depend on
`tui`**; **`#![forbid(unsafe_code)]`**; **the status bar never lies**; **no 1000-node golden
snapshot**; **4-core cap on every cargo invocation**; **tmux: kill only your own session, on
your own socket**; **leave no stray `mdmost` processes — check `ps -o ppid=` first**, the
owner runs this pager himself here.

Settled; do not relitigate: **there is no centring anywhere**; **`src/export/` may depend
only on `doc`**; **TSV is what every reader receives**; **the title banner is opt-in**;
**`--render-once` may emit lines wider than `--width`**; **the table gap-row threshold is 30
display columns**; **the copy button follows what mouse capture actually did, but
`--render-once` DOES record hotspots**; **`HOVER_SHIFT = 0.6`**; **only `http`/`https`
become controls, everything else is wholly inert**; **a hotspot carries the FULL url**;
**activation is on the RELEASE edge**; **`blit` carries hotspots, drops `Pin` and `Atom`**;
**no apt/yum repo, no GitHub Pages, no container image, no macOS notarisation**.

Settled on the demo side, this session:

- **The tour re-think was machinery and staging, NOT a narrative rewrite.** Owner's ruling,
  and it held: same seven acts, same story, same features.
- **The theme beat's verification is a human looking at a PNG.** It cannot be automated and
  must not be replaced by a green recording. `#fdfcf9` light, `#11141b` dark.
- **The theme beat may not be verified from a truncated script** — the bisect showed the
  trigger is act 6's keyboard walk immediately before it, so truncating is exactly the
  configuration that always worked.
- **Recording: run from the repo root, and always `-o` to scratch** unless you intend to
  write the committed artifact.
- **`demo/mdmost.toml:33`'s claim that nano's rc files are pinned is CORRECT** — the pin is
  `nano -I` (`--ignorercfiles`), not an env var. A reviewer called this unsupported; it was
  a false positive. Do not "fix" it.

## 6. Where the detail lives

- **Design authorities:** `docs/superpowers/specs/2026-08-08-mdmost-design.md` (renderer),
  `2026-08-11-semantic-selection-design.md`, `2026-08-11-clickable-links-design.md`,
  `2026-08-13-documentation-design.md` (**unruled**),
  `2026-08-15-demo-tour-rethink-design.md` (**executed**).
- **Plans:** `docs/superpowers/plans/`, including `2026-08-15-demo-tour-rethink.md`.
- **The SDD ledgers** for the older plans live at `/scratch/oetiker/mdmost-ledgers/` — 112
  files, outside git by design. **Nothing backs this directory up.** Project B's own ledger
  was under `.superpowers/sdd/` and is deleted at completion by design; its content is in
  this file and in the commit messages.
- **Demo:** `demo/tour.md`, `demo/mdmost.toml`, `demo/tmux.conf`, `demo/config.toml`.
  `docs/maintainer-notes.md` now carries the numbered regeneration recipe, the regex/row
  gotchas, the tmux-socket trap, and the theme beat's verification — **it is current as of
  this commit**, which is new.
- **ansidrama** (`~/checkouts/ansidrama`, `github.com/oetiker/ansidrama`) at `v0.4.0`.
  `src/pattern.rs` is the `await` implementation and the authority on matching semantics.
- **Owner's test pages:** `mdmost-tryout/links.md` and `buttons.md`. **Run with `--mouse`.**
- Key files: `src/canvas/ops.rs`, `src/canvas/mod.rs`, `src/canvas/border.rs`,
  `src/render/inline.rs`, `src/render/link.rs`, `src/render/table.rs`, `src/tui/draw.rs`,
  `src/tui/chrome.rs` (`draw_status`, and the `Drop` ordering §7.4 depends on),
  `src/tui/app.rs`, `src/tui/term.rs:249` (mouse capture is `?1003h`), `src/tui/popup.rs`.

## 7. Open questions / pending decisions

1. **Push the 18 commits?** (§3.1)
2. **The documentation design spec is unruled** since 2026-08-13 (§3.2).
3. **The release workflow has never run.** No tags (§3.3).
4. **The un-hover gate at `demo/mdmost.toml:379` is the most fragile in the script.** It
   works because `Drop::Url` and `Drop::Context` contend for width at 100 columns. A
   status-bar change that stops them competing makes it decorative *silently*. The comment
   now warns of this; a `chrome.rs` change is a reason to re-verify it by eye.
5. **Windows compiles but has never been run.**
6. **Two unguarded `Span` sites**, pre-existing: `chrome::highlighted` and the status-bar
   breadcrumb.
7. **Feature idea, owner's, 2026-08-13 — a language icon on the code frame's label.** Needs
   its own brainstorm and spec. Known risk: PUA glyph advance widths need not match
   `unicode-width`.
8. **A deferred minor:** "a hotspot never claims a cell the canvas does not have" is held by
   argument at three call sites, not by construction.
9. **The ledger archive at `/scratch/oetiker/mdmost-ledgers/` is unbacked.** Now that a
   remote exists, decide whether it belongs in it, in a sibling repo, or nowhere.
10. Carried and still open: RPM payload unverified; `fc-list` untested on macOS; nested
    diagrams never widened while nested tables are; Nerd Fonts v2 fail icon detection
    (deliberate); the banner fixture is self-referential; `&nbsp;` as a wrapping opportunity
    is unruled; the banner's internal band centring was never ruled on; the light theme's
    heading ramp is flat and non-monotone — re-measure before designing against it.
11. Deferred demo minors, none blocking: act 6's `Down Down Down` counter walk is ungated
    (visible failure, one mid-tour frame); act 5's `55` threshold sits nearer 46 than 72
    (safe — the pane close is discrete); act 2's second widening gate is byte-identical to
    the first, so its meaning depends on the ungated narrowing drag between them.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** `git merge-base --is-ancestor
  HEAD main`, `git log --oneline HEAD..main`, `git branch -a --contains HEAD`,
  `git status -sb`. **This file says 18 commits unpushed; that is the fastest-rotting line
  in it.**
- **Sibling worktrees created after this commit are invisible here.** `git worktree list`.
- **mdmost had no tags at this commit.** `git tag`.
- **mdmost's test suite was NOT run this session.** Project B touched no `src/`, so the
  inherited "1292 tests across 33 suites, green" is untested but also unthreatened. Run the
  gates rather than quoting this sentence.
- **`demo/mdmost.toml~` is an untracked editor backup** in the working tree, and
  `~/checkouts/ansidrama/README.md~` is another. Neither is mine to delete.
- **Two tmux servers from earlier sessions** (`-L mdact1`, `-L mdprobe`) were still alive at
  this commit, each holding an `mdmost` from the removed `mdmost-semantic-selection`
  worktree's target dir. Harmless — the `mdmost-demo` socket file is stale with no server
  behind it — but they are the owner's to kill, not yours.
- **~14 GB of orphaned cargo target dirs** under `/scratch/oetiker/`. No disk pressure. Ask
  before deleting, and never `cargo sweep --stamp`/`--file` with a shared target dir.
- **Frame numbers in `docs/maintainer-notes.md` name the commit they were read against**
  (`299bc81`). If the artifact is re-recorded, they move.
