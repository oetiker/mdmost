# Controller Handoff — mdmost semantic-selection

> Starter pack for the next controller session. This handoff lives in ONE
> worktree — run `git worktree list` first and confirm this is the workstream
> you're resuming. Read this first, then `git log <handoff-commit>..HEAD` for
> everything that changed since. Detail is NOT here — it's in git and the
> ledger named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry forward
> any lesson in §4/§5 that is still true. Fresh synthesis, not blank page. On
> merge into another branch, rewrite that branch's handoff to the merged
> reality — do not merge or preserve this text.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-11   Reason: context budget — controller rolling over before Task 6
Worktree / branch: `/scratch/oetiker/claude-worktrees/mdmost-semantic-selection` @ `semantic-selection`
Trunk at time of writing: `main` @ `344f4e1` — **reader: if trunk has moved, §2 is provisionally stale; if trunk now contains this branch's HEAD, this file is a tombstone** (`git merge-base --is-ancestor HEAD main`)
Sibling worktrees: the `main` checkout at `/home/oetiker/checkouts/mdmost`, which owns nothing active — its handoff is the pre-plan one and is now superseded for this workstream. This line cannot see worktrees created later; check yourself.

## 1. Mission

Execute `docs/superpowers/plans/2026-08-11-semantic-selection.md` — ten tasks
making a selection a **range over the document** rather than a rectangle on
screen, giving every diagram label a mapping back to its source, and giving all
three block kinds a muted three-state `[copy]` button.

The mental model that matters: **rendering is a pure function of
`(AST, width, theme, options)`**. Anything depending on the pointer — hover, the
selection wash, the `[copied]` flash — is a **paint-time** concern in
`src/tui/draw.rs`, never a render-time one.

**How the owner works, and it is not optional.** They review by *looking at
rendered output*, and their findings are precise. **Answer a design question with
a rendered sample, never with prose.** When they reframe a question, the reframe
*is* the design — stop defending the old framing and follow it. Three of the four
rulings in §7 came that way, and each was better than the options I had drafted.

## 2. Where we are now, as of the handoff commit

Re-derive rather than inherit (§8). Seven commits on `semantic-selection`,
`main` untouched.

| Task | Commit | State |
| --- | --- | --- |
| 1 — cell → source offset | `89be3f7` | complete, review clean |
| 2 — hull is a range, not a rectangle | `05c2332` | complete, review clean |
| 3 — highlight paints from the hull | `eec0faf` | complete, **owner gate passed** |
| 4 — `Label` carries provenance | `ffc9dc3`, `b7e5452` | complete after 1 fix round |
| 5 — flowchart labels reach the document | `f64951c` | complete, review clean |
| 5b — a diagram is atomic in a selection | `ab79c98` | complete, review clean |

**1053 tests / 30 suites**, `cargo fmt --check`, `cargo clippy --jobs 4
--all-targets -- -D warnings` and `cargo test --jobs 4` all exit 0 at `ab79c98`,
re-run by a reviewer that did not produce the numbers.

**Task 5b has an UNCOMMITTED fix round in the worktree** — `src/tui/select.rs`,
`src/tui/tests.rs`, `src/canvas/mod.rs` and the design spec are modified. It was
dispatched to `impl-task-5b-fix` and covers the two owner rulings in §7.1–7.2 plus
two doc minors. **Check `git status` first**: if those files are still dirty and no
`cargo` is running, that agent stalled — run the gates and commit for it rather
than re-dispatching (§4.1). It has not been through a scoped re-review yet; that
is owed before Task 6 starts.

`selection-review.html` is an untracked owner artifact from Task 3. Leave it.

**Tasks 6–10 are not started.** Task 5b was inserted by owner ruling and is not
in the plan file.

## 3. Do this next

1. **Land the 5b fix round**: confirm it committed, then run
   `scripts/review-package … ab79c98 HEAD` and dispatch a **scoped re-review**
   (`re-review-prompt.md`) verdicting each of the four items ADDRESSED /
   NOT ADDRESSED. Do not let it merge into Task 6 unreviewed.
2. **Task 6 — the remaining six families.** Its brief must carry three things the
   plan cannot know: (a) Task 4's finding that **14 of 15 threaded parse sites have
   no test** — the shared `lex::label_at` makes them *look* covered, and design §6
   risk 4 warns a shared helper does not make seven families one behaviour;
   (b) Task 5's M3 — flowchart **edge labels and subgraph titles still carry no
   spans**, flattened to `Vec<String>` at the shared `graph` seam that every
   graph-drawn family uses, which is Task 6's blast radius; (c) atomicity is
   family-agnostic, so each family gets it free once it emits spans — but 5b's
   tests should be re-run per family.
3. **Task 7 — the button.** The owner ruled its payload is the **whole fenced
   block**, superseding spec §4 and plan Task 7 Step 3. The plan's own test would
   pass either way (it asserts the payload contains `flowchart LR`, true of a
   fenced payload too) — **require asserting the fences explicitly**, or the ruling
   ships untested.

Tasks 8 (button colours) and 10 (demo) are **owner gates**: stop and show rendered
output, do not choose colours yourself.

## 4. Lessons & traps ← the irreplaceable part

Carried forward and still true: **give every agent its own `CARGO_TARGET_DIR`**;
**never read a gate's result through a pipe**; the clippy gate is
`--all-targets -- -D warnings` because plain `cargo clippy` exits 0 on warnings;
**verify a subagent's arithmetic, not its adjectives**; **do not choose glyphs by
measuring them**; **do not hand-resolve snapshot conflicts**; **measure box art in
columns, not bytes**; **backticks inside `git commit -m` are command-substituted —
use a quoted heredoc**; **never merge into a dirty worktree**; **tombstone every
merged branch immediately**; **use `git status --porcelain --ignored` before
removing a worktree** (the SDD ledger is gitignored and dies with it).

New this session, in rough order of what they cost:

1. **Subagents deadlock on long builds, and the cause is not disobedience.** The
   Bash tool's default timeout is 120 s, shorter than this suite. On timeout the
   harness *backgrounds* the command and says so — then the subagent ends its turn
   anyway and the shell is killed with the turn
   (anthropics/claude-code#50572, closed "not planned"; there is no
   `synchronous: true`). **Put `timeout: 600000` in every dispatch prompt.** It
   happened to three agents in a row before I understood it. Diagnose a silent
   agent by the worktree, never by the silence: *clean tree + commit* = finished
   silently, go read the commit; *dirty tree + no build* = stalled, run the gates
   and commit for it; *dirty tree + live build* = this bug, message it to resume —
   the edits are still there, so re-dispatching duplicates work.
2. **Prohibitions don't land; actions do.** "Do not pipe the gate" was ignored by
   three agents running. They were pattern-matching "output too long" when the real
   signal was "call too slow". State it as an action with the mechanism attached.
3. **This plan's own sample code and fixtures are wrong more often than not —
   six defects in six tasks.** A `(lo.min(hi), lo.max(hi))` line that inverted a
   deliberate fallback into a whole-document selection; a missing inclusive-column
   `+1` that lost every selection's tail grapheme; "delete `columns_on`" when it
   still had a live caller and would not compile; a synthesis site at
   `state.rs:310` that constructs no `Label` at all; `render/diagram.rs` named as
   the rebasing site when it has no `origins` in hand; and "follow `render::code`
   for CRLF and indent" when both are solved in `doc/convert.rs::code_lines`.
   **Brief every implementer to treat the plan's code as a draft to verify.** The
   code they write has been sound; the plan's *transcriptions* have not.
4. **Four of the plan's suggested fault-injection mutations turned NO test red.**
   Not because the code was right — because the sample assertions were too weak
   (`.any(|s| …)`, `lo >= hi`, a column-1 probe that sat before every span on its
   row). **A mutation nothing catches is a finding about the test, not a pass.**
   Say so in every brief, and require "watched it go red", never "verified".
5. **The per-task review is what makes this loop work, and re-running the gates
   independently is the part that matters.** A resumed subagent can report a stale
   log as a fresh green run and the loop cannot tell (obra/superpowers#2113).
   Every green claim here was re-derived by an agent that did not produce it; one
   reviewer hand-summed 30 per-suite lines rather than trust a summary. Keep that.
6. **The best findings came from asking a reviewer to adjudicate a specific claim
   on the merits.** Naming the claim, the stakes, and "verify rather than assume
   either party is right" produced the rhombus/cylinder span-drop — a real bug the
   plan's all-`Rect` fixtures could never have caught — and the release-build
   pointer-underflow. Generic "review this diff" would not have.
7. **Setting a task's `owner` field re-notifies an agent that already delivered.**
   It re-pinged `impl-task-5b`, which correctly declined the duplicate. Set owner
   at dispatch time only.
8. **A crash kills every subagent; the worktree survives.** Recover by reading the
   agent's report file — the reports under §6 are written to be exactly that
   memory, and a fresh implementer resumed from one without losing the thread.

## 5. Don'ts & constraints

Carried forward and still binding: **no HTML rendering**; **Mermaid is Unicode box
art only**; **bullets and task boxes are ASCII**; **`Esc` never quits**; **do not
widen the `NodeArt` seam**; **`render` must not depend on `tui`**;
**`#![forbid(unsafe_code)]`**; **the status bar never lies**; **no 1000-node golden
snapshot**; **4-core cap on every cargo invocation**; **there is no centring
anywhere**; **`src/export/` may depend only on `doc`**; **TSV is what every reader
receives**; **the table gap-row threshold is 30 display columns**; **the copy button
follows what mouse capture actually did**; **do not push — creating the remote is
the owner's step**; **tmux: kill only your own session, and check a process's parent
with `ps -o ppid=` before killing an `mdmost`** — the owner runs this pager himself
on this machine.

Settled this session; do not relitigate:

- **The highlight and the clipboard are decided by ONE computation.** `extract` and
  `highlighted_columns` share a `Resolved` whose fields and `range()` are private
  precisely so the byte range cannot be recomputed elsewhere. Two paths that merely
  agree today is the defect 5b exists to remove.
- **A diagram is atomic in a selection** (§7.1) — and the whole-diagram wash covers
  box art, a deliberate documented exception to "chrome is never highlighted".
- **`Label`'s equality deliberately ignores `source`.** So `assert_eq!(label,
  Label::line("Parse"))` proves nothing about provenance — assert on `label.source`
  or on the copied bytes. This is the easiest way to write a vacuous test here.
- **An empty `Label::source` means "not from the source ⇒ emit no span".**
  `lex::label_at` fails closed to it. Do not invent a plausible range for
  synthesised text, and do not "fix" `offset_of` back to `unwrap_or(0)` — offset 0
  is a positive claim that the label began at the block's first byte.
- **A wrapped label gives every drawn row a span naming the whole label**, so "the
  cells under a span read its source bytes" holds only for unwrapped labels
  (§2.2 atomicity). Anything assuming otherwise — search-hit highlighting inside a
  diagram — will be surprised.
- **`carry_spans` has no structural guard.** Any future shape helper that copies
  cells by hand rather than through `blit`/`framed`/`indent` silently drops spans;
  only `every_node_shape_puts_its_label_span_on_the_drawn_text` will notice. That
  test is load-bearing — extend it when an eighth shape lands, never delete it.

## 6. Where the detail lives

- **Plan:** `docs/superpowers/plans/2026-08-11-semantic-selection.md` (Task 5b is
  not in it — see §7.1)
- **Design authority, amended this session:**
  `docs/superpowers/specs/2026-08-11-semantic-selection-design.md` §2.2/§3
- **Progress ledger — read this second, after this file:**
  `.superpowers/sdd/2026-08-11-semantic-selection/progress.md`. Gitignored, so it
  dies with the worktree. Every ruling, plan defect and deferred minor is in it.
- **Per-task briefs, reports, reviews:** same directory, `task-N-{brief,report,review}.md`.
  The reports are an agent's memory — a fresh implementer can resume from one.
- Key files: `src/tui/select.rs` (`resolve`, `Resolved`, `offset_at`, `Bias`,
  `source_hull`, `highlighted_columns`, `extend_over_markup:391`),
  `src/render/code.rs` (`diagram_block`, `rebase_spans`, `document_offset`),
  `src/doc/convert.rs:100-144` (`code_lines` — the suffix rule that solves CRLF and
  indent in one), `src/mermaid/parse/lex.rs` (`offset_of`, `label_at`),
  `src/mermaid/ast.rs` (`Label`, its manual `PartialEq`),
  `src/mermaid/layout/flowchart/shape.rs` (`carry_spans`).

## 7. Open questions / pending decisions

1. **Owner rulings this session, all superseding written docs.** (a) A diagram is
   atomic in a selection; wider-than-one-label washes the whole rectangle and copies
   the fenced block — supersedes §3's former closing sentence. (b) A drag that
   **starts** outside any label takes the whole diagram immediately — decided by the
   anchor cell's atom, *not* by comparing rectangles to cells. (c) The `[copy]`
   button's payload is the fenced block too — supersedes spec §4 / plan Task 7
   Step 3. (d) Container prefixes are **stripped** from a copied block — no line
   keeps `> `, fence lines included, and it must reuse `code_lines`' suffix rule
   rather than a regex, which would strip a `> ` genuinely inside the Mermaid.
2. **The 5b fix round is unreviewed** (§2).
3. **`BASH_DEFAULT_TIMEOUT_MS` / `BASH_MAX_TIMEOUT_MS` were NOT set.** The edit to
   `~/.claude/settings.json` was blocked by the permission classifier and the owner
   has not applied it. Until then, every dispatch must carry `timeout: 600000`
   explicitly (§4.1).
4. Carried forward and still open: the owner's manual GitHub steps (create the
   repo, `CRATES_IO_TOKEN`, Actions write) — both CI workflows are inert until then;
   the release workflow has never executed; **Windows compiles but has never been
   run**, and Task 9 adds motion-event handling; the light theme's heading ramp is
   flat and non-monotone (re-measure before Task 8); the banner's internal band
   centring was never ruled on; the RPM payload is unverified; nested diagrams are
   never widened while nested tables are.
5. **~14 GB of orphaned `cargo-target-mdmost-*` dirs** under `/scratch/oetiker/`,
   plus this branch's own. No pressure at last check; ask before deleting.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** Whether this branch is
  merged, pushed or superseded is not knowable from this file:
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`. If this branch is merged, stop reading and go to
  the successor's handoff.
- **Sibling worktrees / other workstreams may exist that this file cannot name** —
  anything started after the handoff commit is invisible here.
- The 1053-test count and the three green gates are as of `ab79c98`, **before** the
  uncommitted 5b fix round. Re-run them; a count that moved without an explanation
  is the signal.
- Line references in §6 were true at `ab79c98` and the 5b fix round touches
  `select.rs` — verify before quoting them to a subagent.
- The plan's remaining task text (6–10) is written against code as it was at
  `344f4e1`, five behaviour changes ago. Treat its file lists and sample code as
  drafts (§4.3), and expect Task 6's "same shape as Task 5" to have the usual hole.
