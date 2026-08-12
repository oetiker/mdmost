# Controller Handoff — mdmost, branch `semantic-selection`

> Starter pack for the next controller session. This handoff lives in ONE worktree — run
> `git worktree list` first and confirm this is the workstream you're resuming. Read this
> first, then `git log <handoff-commit>..HEAD`. Detail is NOT here — it is in git and in
> the ledger named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry forward any
> lesson in §4/§5 still true. Fresh synthesis, not blank page. On merge into another
> branch, rewrite that branch's handoff to the merged reality — do not preserve this text,
> and tombstone this one.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-13   Reason: context rollover mid-plan, five tasks of ten done
Worktree / branch: `/scratch/oetiker/claude-worktrees/mdmost-semantic-selection` @ `semantic-selection`
Trunk: `main` @ `344f4e1`. **This branch is NOT merged and there is still no remote.**

**This branch now carries TWO complete workstreams, neither merged**: the semantic-selection
plan (finished 2026-08-12, ~20 commits) and the clickable-links plan (in progress). That is
a lot of unmerged work on a branch with no remote. Weigh merging the finished half.

Sibling worktrees:
- `/home/oetiker/checkouts/mdmost` @ `main` — owns nothing; **its handoff is three plans
  stale and actively wrong.** Do not believe it.
- `/scratch/oetiker/claude-worktrees/mdmost-tryout` — detached, the owner's test binary and
  pages (`links.md`, `buttons.md`). Never commit here.
- `/scratch/oetiker/claude-worktrees/mdmost-verify` — **the controller's own worktree**, see
  §4.1. Detached at whatever commit was last verified. Never let an implementer near it.

## 1. Mission

`mdmost` is a full-screen terminal pager for one Markdown document — "as pleasant to look at
as btop, as pleasant to use as less". Rust + ratatui.

The current plan makes a link **react and be followed**: hover, click, `http`/`https` in the
browser, `#anchor` in this document, a footnote popup, and a keyboard cursor so none of it
needs a mouse.

Load-bearing mental models:

1. **Rendering is a pure function of `(AST, width, theme, options)`.** Anything depending on
   the pointer — hover, the selection wash, `[copied]` — is **paint-time** in `src/tui/`,
   never render-time. `render` must not depend on `tui`.
2. **A `SearchSpan`'s source is a byte-for-byte copy of the cells it names.** A `Hotspot` is
   deliberately exempt: it claims *drawn cells*, not source bytes, which is why a link's
   synthetic ` (url)` suffix belongs to the control and to no source range.
3. **Syntax comes off the source; text is decoded at the leaves.**

**How the owner works, and it is not optional.** They review by *looking at rendered
output*. **Answer a design question with a rendered sample, never with prose** — build a
release binary in the tryout worktree and write them a page. When they reframe a question,
the reframe *is* the design.

## 2. Where we are, as of the handoff commit

Re-derive rather than inherit (§8). Plan: `docs/superpowers/plans/2026-08-12-clickable-links.md`.

| Task | Commit | Tests |
| --- | --- | --- |
| 1 — `Hotspot` grows a kind | `78fd8aa` | 1178 |
| 2 — links record hotspots | `32c357b` | 1191 |
| 3 — scheme allowlist + status-bar URL | `c4d1bdd` | 1203 |
| 2b — a link in a table cell reacts (off-plan) | `d6a262a` | 1218 |
| 4 — hover lights a whole control | `38c667d` | 1222 |
| 5 — the click state machine | `3fc1513` | 1235 |

At `3fc1513`: **1235 tests across 32 suites**; `cargo fmt --check`, `cargo clippy --jobs 4
--all-targets -- -D warnings`, `cargo test --jobs 4` and `cargo check --jobs 4 --target
x86_64-pc-windows-msvc` all exit 0. **Every commit had its gates re-derived by the
controller in the verify worktree, its test count reconciled two ways, and at least one
load-bearing mutation re-run by the controller rather than taken on the implementer's
word.** That caught three things a report would not have: a vacuous security test, an
uncovered rebase path, and a "red" that was a compile error.

**Remaining: Tasks 6 (the opener), 7 (anchors), 8 (keyboard cursor), 9 (footnote popup),
10 (the demo).** Task 9's plan text still has five stub test bodies — write them when Task
9 is next up, not before (§4.6).

## 3. Do this next

1. **Task 5's review is in flight** when this was written. Clear it, then Task 6.
2. **Tasks 6–8 are ordinary.** Task 9 (the popup) is the largest and its plan is
   incomplete by design. Task 10 is the demo and is an owner gate.
3. **Two owner answers are outstanding and neither blocks** (§7.1, §7.2).

## 4. Lessons & traps ← the irreplaceable part

Carried forward and still true: **give every agent its own `CARGO_TARGET_DIR`**; **never
read a gate's result through a pipe** (a redirect is *not* a pipe and preserves exit
status); the clippy gate is `--all-targets -- -D warnings`; **verify a subagent's
arithmetic, not its adjectives**; **do not choose glyphs by measuring them**; **a rename is
not a `sed`**; **when two code paths render "the same" thing, prove it**; **backticks in
`git commit -m` are command-substituted — use a quoted heredoc**; **`git merge` does not
accept `-F -`**; **never merge into a dirty worktree**; **`git status --porcelain
--ignored` before removing a worktree** — the ledger is gitignored and dies with it;
**diagnose a silent agent by the worktree, never by the silence**; **state process
constraints as actions, not prohibitions**; **dispatch at most ONE file-writing agent per
worktree**.

New this session, in rough order of what they are worth:

1. **THE CONTROLLER IS A WRITER.** I corrupted an implementer's working tree **twice** by
   running my own `cp`-backup → mutate → suite → restore cycle in the worktree it was live
   in. Checking "is it clean first" does not work: a gate run takes minutes and the agent
   resumes inside that window, so the check and the write are not atomic. **Fix, now in
   force: the controller owns `/scratch/oetiker/claude-worktrees/mdmost-verify` and does
   every gate run and every mutation there** (`git -C ../mdmost-verify checkout <sha>`,
   target dir `cargo-target-mdmost-verify`). I had written "two agents cannot share one git
   index" into my own ledger that morning and read it as being about two *implementers*.
2. **Fault injection must run `--no-fail-fast`.** Cargo's default stops at the first failing
   test *binary* and hides which other suites catch the mutation. On the allowlist mutation
   the default run showed one red and never ran the integration binary; `--no-fail-fast`
   showed **two reds in two suites**. Same mutation, twice the evidence.
3. **Read the log, not the exit code.** Exit 101 is identical for a panicking test and a
   failed compile. I recorded a "verified red" that was a type-inference error and only
   caught it by reading the log. Grep `^error\[` before believing any red.
4. **A missing test is only a gap if you can name a mutation that survives without it.**
   A brief demanded a second painting test for links in centred cells; the implementer
   wrote one test. Rather than log a gap I mutated for the bug such a test would catch —
   and the existing test caught it, because a prose-wrapped link already starts at a
   different column on each row. The second test would have been ceremony. This is the
   discipline's inverse and it stops it generating busywork.
5. **Brief reviewers to read the WORKING TREE, not only the diff they were handed.** That
   is how Task 2b's reviewer found both an uncommitted test that closed a real gap *and* a
   live fault injection with a build still running. Twice the useful finding came from what
   an agent was *doing*, not what it had *committed*.
6. **Before discarding a dirty tree, read the diff.** After my first interference I assumed
   the dirt was my own residue and was one command from `git checkout --`. It was the
   implementer's *better* uncommitted work — the route that removed the need for two
   `cfg(test)` seams. "The working tree is the only copy" applies to work you did not write
   and did not expect.
7. **The backgrounding bug hit five of six implementers.** Bash's 2-minute default
   backgrounds the command and the shell dies with the turn. All three of `timeout:
   600000`, foreground, and *wait for it in the same turn* must be in every brief — and it
   still recurs, so the reliable defence is the diagnosis: **dirty tree + live build =
   resume that agent, never re-dispatch**.
8. **Six of my own file references were wrong**, and five implementers correctly overruled
   me. The status bar is `chrome::draw_status`, not `draw.rs`. `blit` **drops** hotspots, so
   the collision fix belonged in `merge_hotspots`. `mdmost::render::document` does not
   exist. **Name the CONSTRAINT — which layer must not learn this — not the file.**
9. **A `#[cfg(test)]` constructor is not production surface.** An implementer resisted
   adding one on good instinct, misapplied; it does not exist in a release build. Say so, so
   the resistance does not cost a round. (In the end Route 1 removed the need entirely.)
10. **Two vacuous-test species, both caught only by mutation.** (a) A test that calls the
    helper directly proves the tool works, not that the tool is *used* — removing the call
    site from `draw_status` turned nothing red. (b) An assertion that holds for two
    different reasons cannot distinguish them: "no raw ESC in the output" is true whether or
    not the sanitiser runs, because ratatui drops `Cc` itself. The load-bearing assertion
    was the **presence of the U+FFFD marker**.

## 5. Don'ts & constraints

Carried forward and binding: **no HTML rendering**; **Mermaid is Unicode box art only**;
**bullets and task boxes are ASCII**; **`Esc` never quits**; **do not widen the `NodeArt`
seam**; **`render` must not depend on `tui`**; **`#![forbid(unsafe_code)]`**; **the status
bar never lies**; **no 1000-node golden snapshot**; **4-core cap on every cargo
invocation**; **there is no centring anywhere**; **`src/export/` may depend only on `doc`**;
**TSV is what every reader receives**; **the table gap-row threshold is 30 display
columns**; **the copy button follows what mouse capture actually did**; **do not push —
creating the remote is the owner's step**; **tmux: kill only your own session, and check a
process's parent with `ps -o ppid=` before killing an `mdmost`** — the owner runs this pager
himself on this machine.

Owner rulings this session, all binding:

1. **`HOVER_SHIFT = 0.6`** (was 0.4). The owner looked and asked for more pronounced. The
   old comment predicting "past 0.6 it reads as an accent" was a *measurement*, not a look,
   and is retired. Do not retune without asking.
2. **The clickable-links spec ships in full** — anchors and the footnote popup included:
   "we want it all."
3. **The demo gains a footnote-popup click** (Task 10).
4. **Carried, unconfirmed:** "the first and final panel needs to stay 3× longer so they can
   be read" — read as demo timing, to be confirmed before recording. It arrived attached to
   a comment the owner then withdrew, so the attachment is uncertain, not the request.

Settled by the work; do not relitigate:

- **`blit` carries hotspots; it drops `Pin` and `Atom`.** A `Pin` claims the leading columns
  *from column zero* — a whole-row claim, meaningless once blitted into a shared row. A
  `Hotspot` carries its own `col`, so translated it names specific destination cells where
  the control's characters really are drawn — the same claim a search span makes, and
  `blit` has always translated those. Unlike a search span (verbatim, consumers clamp), a
  hotspot is **clamped and dropped when nothing survives**: a region that reacts while
  showing nothing of the control is worse than no region.
- **A hotspot over the column an overflow chevron kept is revoked too** — "a cell that opens
  a link without looking like one is the same fault as a claim on a cell that is not there."
- **Only `http` and `https` become controls**, matched case-insensitively; a `#fragment`
  becomes an anchor folded through the *shared* `doc::slug::base_slug`, so a heading and a
  link can never drift apart. Everything else — `mailto:`, `file:`, `javascript:`, local
  `.md` — is **wholly inert**, failing closed. A control that lights up and then declines is
  worse than one never offered.
- **A hotspot carries the FULL url**, never the `elide_middle`d form drawn on screen.
- **Hover lights every hotspot sharing a `target`**, which is what makes a wrapped link one
  control.
- **Activation is on the RELEASE edge for every control** — see §7.2, stated for veto.
- **`Copied::for_button` / the copy payload is the block's content without its fences**;
  a *selection* still yields the fenced block. Deliberately different.

## 6. Where the detail lives

- **Plan:** `docs/superpowers/plans/2026-08-12-clickable-links.md`. Tasks 2b is not in it.
  Task 9 has stub test bodies by design.
- **Design authority:** `docs/superpowers/specs/2026-08-11-clickable-links-design.md`
- **Ledger — read this second, after this file:**
  `.superpowers/sdd/2026-08-12-clickable-links/progress.md`. **Gitignored, dies with the
  worktree.** Every ruling, deferred minor, brief, report and review is in that directory.
- Previous workstream's plan and spec: `2026-08-11-semantic-selection*`.
- **Owner's test pages:** `mdmost-tryout/links.md` (this plan) and `buttons.md` (previous),
  binary at `/scratch/oetiker/cargo-target-mdmost-tryout/release/mdmost`. **Run with
  `--mouse` or there are no controls at all.**
- **Durable facts discovered, in the ledger, worth knowing:** DEL, U+009B, NEL, LINE and
  PARAGRAPH SEPARATOR and both C1 endpoints all survive CommonMark's bare-destination
  grammar into a link's URL unchanged — only true ASCII C0 controls are excluded. And
  **ratatui's `Buffer::set_line` silently drops every `Cc` character**, which is why the
  escape-injection threat does not reproduce and why the real defect is a *silent
  zero-width drop* against width arithmetic that charged one column.
- Key files: `src/canvas/ops.rs` (`blit`, `merge_hotspots`, `clamped_claim`,
  `revoke_hotspots_over`), `src/canvas/mod.rs` (`Hotspot`, `HotspotKind`),
  `src/render/inline.rs` (`link`, `flatten`, `reconcile` — where hotspots are recorded),
  `src/render/link.rs` (`classify`), `src/tui/draw.rs` (`hover_highlight`),
  `src/tui/chrome.rs` (`draw_status`, `sanitized_url`), `src/tui/app.rs`
  (`press_hotspot`, `release_hotspot`, `cancel_hotspot_press`).

## 7. Open questions / pending decisions

1. **`--render-once` and hotspots — spec §4 contradicts itself, owner ruling needed.** Line
   93 says render-once records no hotspots; the paragraph above says links are never hidden
   because a keyboard cursor makes them reachable. `--render-once` sets `copy_button:
   false`, **the same flag the pager sets when mouse capture is refused** *and* when stdout
   is not a terminal. Gating link hotspots on it would blank every link in every mouseless
   terminal — the exact population Task 8 exists to serve. Current behaviour (hotspots are
   recorded) is pinned by a test. Controller's recommendation: strike line 93; it was
   written about *buttons*, which are drawn chrome.
2. **Activation moved to the release edge**, so `[copy]` now fires on button-up. Stated in
   `3fc1513`'s message for veto. The owner has not responded.
3. **Task 4's colour gate is open.** Binary and `links.md` handed over 2026-08-12; no
   response yet. Not blocking — a shade is a one-constant change.
4. **Two unguarded `Span` sites**, pre-existing, for the final review: `chrome::highlighted`
   (`chrome.rs:182`, TOC heading text) and the status-bar breadcrumb (`chrome.rs:383`). Same
   width-drift exposure the URL had.
5. **Deferred minors worth triaging at the final review**, in the ledger. The strongest:
   the invariant "a hotspot never claims a cell the canvas does not have" is held by
   argument at three call sites, not by construction — adding it to
   `Canvas::check_invariants` would make a future op that forgets to clamp fail loudly in
   ~40 existing tests.
6. **Owner's manual GitHub steps, still outstanding:** create `github.com/oetiker/mdmost`,
   add `CRATES_IO_TOKEN`, grant Actions write. The owner authorised `gh` use; **the token
   lacks the `workflow` scope**, so pushing `.github/workflows/` needs `gh auth refresh -s
   workflow`. Public-vs-private was never answered.
7. Carried and still open: the release workflow has never executed; Windows compiles but
   has never been run (mouse, clipboard, alternate screen, and now motion and release
   handling, all unexercised); the RPM payload is unverified; `&nbsp;` is a wrapping
   opportunity, flagged and unruled; the banner's internal band centring was never ruled on.
8. An environmental quirk with no root cause: `diff <(git show HEAD:path) file` produced a
   spurious full-file diff twice on byte-identical files (confirmed by `sha256sum`). Use
   real intermediate files for verification diffs.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited**: `git merge-base --is-ancestor
  HEAD main`, `git log --oneline HEAD..main`, `git branch -a --contains HEAD`.
- **Sibling worktrees created after this commit are invisible here.** Run `git worktree
  list`.
- The 1235-test count and the four green gates are as of `3fc1513`. Re-run them; a count
  that moved without an explanation is the signal.
- **"There is no remote" rots the moment the owner creates the repository.** Check `git
  remote -v` rather than believing §7.6.
- The plan's line references were written against `392096f` and this session changed
  `canvas/ops.rs`, `canvas/mod.rs`, `render/inline.rs`, `tui/draw.rs`, `tui/chrome.rs` and
  `tui/app.rs` substantially. **Treat every file and line it names as a draft.**
