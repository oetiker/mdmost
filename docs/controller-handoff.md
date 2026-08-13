# Controller Handoff — mdmost trunk, pre-public

> Starter pack for the next controller session. This handoff lives in ONE worktree — run
> `git worktree list` first and confirm this is the workstream you're resuming. Read this
> first, then `git log <handoff-commit>..HEAD`. Detail is NOT here — it is in git and in
> the documents named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry forward any
> lesson in §4/§5 still true. Fresh synthesis, not blank page. On merge into another
> branch, rewrite that branch's handoff to the merged reality — do not preserve this text,
> and tombstone the branch you merged.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-13   Reason: milestone — every written plan is now executed and merged to trunk
Worktree / branch: main checkout `/home/oetiker/checkouts/mdmost` @ `main`
Trunk at time of writing: this **is** trunk. `main` @ `dbe236d`.

Sibling worktrees, as of this commit — verify with `git worktree list`, this line cannot
see anything created later:

- `mdmost-semantic-selection` — **merged, tombstoned, and the worktree is gone** (removed
  2026-08-13 with the owner's approval). The branch survives, carrying nothing but its
  gravestone. Its gitignored ledger was archived out first — see §6.
- `/scratch/oetiker/claude-worktrees/mdmost-tryout` — detached; the owner's test binary
  and pages (`links.md`, `buttons.md`). Never commit here.
- `/scratch/oetiker/claude-worktrees/mdmost-verify` — **the controller's own worktree.**
  Every gate run and every mutation happens here, never in an implementer's tree (§4.1).
  Detached at whatever was last verified. Never let an implementer near it.

## 1. Mission

`mdmost` is a full-screen terminal pager for one Markdown document — "as pleasant to look
at as btop, as pleasant to use as less". Rust + ratatui. GFM including tables with
Markdown inside cells, syntax-highlighted code, and seven Mermaid families as Unicode box
art.

Load-bearing mental models:

1. **Rendering is a pure function of `(AST, width, theme, options)`.** Parse once; no
   layout decision at parse time; a resize discards the canvas and renders again. Anything
   depending on the pointer — hover, the selection wash, `[copied]` — is **paint-time** in
   `src/tui/`, never render-time. `render` must not depend on `tui`.
2. **A `SearchSpan`'s source is a byte-for-byte copy of the cells it names.** A `Hotspot`
   is deliberately exempt: it claims *drawn cells*, not source bytes, which is why a
   link's synthetic ` (url)` suffix belongs to the control and to no source range.
3. **Syntax comes off the source; text is decoded at the leaves.**

**How the owner works, and it is not optional.** They review by *looking at rendered
output*, and their findings are consistently precise. **Answer a design question with a
rendered sample, never with prose** — build a release binary in the tryout worktree and
write them a page. They cut through analysis that has become over-engineered; treat that
as direction. They report bugs from real use, so expect renderer bug reports mid-plan.
When they reframe a question — "think how selection works in a web browser" — the reframe
*is* the design; stop defending the old framing and follow it.

## 2. Where we are, as of the handoff commit

Re-derive rather than inherit (§8). **Every plan ever written for this project is now
executed and on trunk.** Four of them:

| Plan | What landed |
| --- | --- |
| `2026-08-09-publishing` | CI, release workflow for five targets, packaging, man page, Homebrew formula, the demo |
| `2026-08-10-code-provenance` | code blocks map back to source; search and copy reach inside a fence; `src/export/`; copy buttons |
| `2026-08-11-semantic-selection` | a selection is a range over the document, not a rectangle of cells |
| `2026-08-12-clickable-links` | hover, click, browser, anchors, footnote popup, keyboard cursor |

At `dbe236d`: **1292 tests across 33 suites**; `cargo fmt --check`, `cargo clippy --jobs 4
--all-targets -- -D warnings`, `cargo test --jobs 4 --no-fail-fast` and `cargo check
--jobs 4 --target x86_64-pc-windows-msvc` all exit 0. Re-derived by the controller in the
verify worktree at `172d39b`, and `git diff 172d39b dbe236d` is empty, so the merge commit
carries no content of its own.

**There is still no remote and nothing has ever been pushed** as of this commit — but that
rots the moment the owner creates the repository, so check `git remote -v` rather than
believing this sentence.

## 3. Do this next

1. **The owner's manual GitHub steps are the only thing blocking a release**, and they
   have been outstanding across three handoffs: create `github.com/oetiker/mdmost`, add
   `CRATES_IO_TOKEN`, grant Actions write permission. Public-vs-private was never
   answered. The owner authorised `gh` use, but **the token lacks the `workflow` scope**,
   so pushing `.github/workflows/` needs `gh auth refresh -s workflow` first.
2. **Then push.** The owner ruled on 2026-08-13: "everything should be merged with main
   before we push", and that merge is done.
3. **The release workflow has never executed** (§7.2). Its first real run is its first
   test, and that is worth watching rather than firing and walking away.
4. **No plan is waiting.** The next workstream is whatever the owner asks for next; if
   they ask for a feature, that is a `superpowers:brainstorming` conversation before a
   plan, not a task to start.

## 4. Lessons & traps ← the irreplaceable part

Carried forward and still true: **give every agent its own `CARGO_TARGET_DIR`**; **never
read a gate's result through a pipe** (a redirect is *not* a pipe and preserves exit
status); the clippy gate is `--all-targets -- -D warnings` because plain `cargo clippy`
exits 0 on warnings; **fault injection must run `--no-fail-fast`**, or cargo stops at the
first failing binary and hides which other suites catch the mutation; **read the log, not
the exit code** — exit 101 is identical for a panicking test and a failed compile, so grep
`^error\[` before believing any red; **verify a subagent's arithmetic, not its
adjectives**; **do not choose glyphs by measuring them**; **a rename is not a `sed`**;
**when two code paths render "the same" thing, prove it**; **measure box art in columns,
not bytes**; **backticks in `git commit -m` are command-substituted — use a quoted
heredoc**; **`git merge` does not accept `-F -`** (a file works, and `git commit -F -`
does too); **never merge into a dirty worktree**; **`git status --porcelain --ignored`
before removing a worktree** — the ledger is gitignored and dies with it; **diagnose a
silent agent by the worktree, never by the silence**; **state process constraints as
actions, not prohibitions**; **dispatch at most ONE file-writing agent per worktree**, and
the test file is shared even when the source files are disjoint.

1. **THE CONTROLLER IS A WRITER.** Corrupted an implementer's tree twice by running a
   backup → mutate → suite → restore cycle in the worktree it was live in. Checking "is it
   clean first" does not work: a gate run takes minutes and the agent resumes inside that
   window, so the check and the write are not atomic. **The controller owns
   `mdmost-verify` and does every gate run and every mutation there.**
2. **A missing test is only a gap if you can name a mutation that survives without it,
   and a Minor is only Minor for as long as its blast radius holds.** A stale click
   candidate rated Minor on "today it is a spurious re-copy" became Important because the
   *next task* made it a browser opening unbidden. A defect whose severity is set by what
   changed around it is not a deferred minor. The inverse matters too: when a brief
   demanded a second painting test, mutating for the bug it would catch showed the
   existing test already caught it — the second test would have been ceremony.
3. **Two vacuous-test species, both caught only by mutation.** A test that calls the
   helper directly proves the tool works, not that it is *used* — removing the call site
   turned nothing red. And an assertion that holds for two different reasons cannot
   distinguish them: "no raw ESC in the output" is true whether or not the sanitiser runs,
   because ratatui drops `Cc` itself. Also: **prove a mutation makes a test FAIL, not
   skip** — a skip is a vacuous test in disguise.
4. **When a recorded beat looks wrong, check the app in a live pane before changing the
   script.** New this session and it cost a take: the demo's footnote counter read as
   `7 → 6 → 5 → 5`, an apparently swallowed keystroke, and the "fix" was three separate
   scenes. It was an off-by-one — **the run log counts frames, it does not number them**,
   so `scene 49 → 833 frames total` means `frame0832.png`, and the frame I read as the
   first keypress was the one before it. A `tmux -L probe` pane walking the same keys
   showed `8 → 7 → 6 → 5` and settled it in a minute. The wrong beat looked entirely
   plausible, which is the whole danger.
5. **Do not ship a frame that contradicts its own status bar.** The demo's theme beat
   produced a dark screen captioned `theme: light`, held 2.8 s on the hero image — and it
   had shipped that way in the previous recording without being noticed. mdmost was
   provably correct (a live pane repaints to `#fdfcf9`; ansidrama alone records the switch
   correctly; ansidrama driving tmux does too). Only the full tour fails, after act 5 kills
   a pane and resizes the survivor. **Rule the recording out before suspecting the app,
   and cut a beat you cannot record honestly** rather than shipping a lie at 2.8 seconds.
   Diagnosis and the recipe for restoring it are in `docs/maintainer-notes.md`.
6. **Six of my own file references were wrong**, and five implementers correctly overruled
   me. **Name the CONSTRAINT — which layer must not learn this — not the file.**
7. **Before discarding a dirty tree, read the diff.** After interfering once I assumed the
   dirt was my own residue and was one command from `git checkout --`. It was the
   implementer's *better* uncommitted work.
8. **Brief reviewers to read the WORKING TREE, not only the diff they were handed.** Twice
   the useful finding came from what an agent was *doing*, not what it had *committed*.
9. **The backgrounding bug hit five of six implementers.** All three of `timeout: 600000`,
   foreground, and *wait for it in the same turn* must be in every brief — and it still
   recurs, so the reliable defence is the diagnosis: **dirty tree + live build = resume
   that agent, never re-dispatch**.
10. **An abandoned branch can hold the only copy of a real fix.** Check by content, not by
    commit subject, and check every commit on a branch before deleting it.

## 5. Don'ts & constraints

Carried forward and binding: **no HTML rendering**; **Mermaid is Unicode box art only**;
**bullets and task boxes are ASCII and do not vary by font detection**; **Nerd Font glyphs
are detected, not defaulted on**; **`Esc` never quits**; **do not widen the `NodeArt`
seam**; **`render` must not depend on `tui`**; **`#![forbid(unsafe_code)]`**; **the status
bar never lies**; **no 1000-node golden snapshot**; **4-core cap on every cargo
invocation**; **tmux: kill only your own session, on your own socket**; **leave no stray
`mdmost` processes — but check a process's parent with `ps -o ppid=` first**, because the
owner runs this pager himself on this machine.

Settled; do not relitigate:

- **There is no centring anywhere.** Every block anchors at the same left margin.
- **`src/export/` may depend only on `doc`** — not `canvas`, not `theme`, not `tui`.
- **TSV is what every reader receives**; HTML is an upgrade where a flavoured clipboard
  exists. Nobody ever gets less than TSV.
- **The title banner is opt-in**; **`--render-once` may emit lines wider than `--width`**.
- **The table gap-row threshold is 30 display columns.**
- **The copy button follows what mouse capture actually did**, not what the config asked
  for. This is why `--render-once` shows none — correct, not a bug to work around. But
  **`--render-once` DOES record hotspots**: that flag also means "mouse capture refused"
  and "stdout is not a terminal", and gating links on it would blank every link for every
  mouseless reader. Spec §4's contrary line was struck by owner ruling (`172d39b`).
- **`HOVER_SHIFT = 0.6`** by owner ruling. Do not retune without asking.
- **Only `http` and `https` become controls**, case-insensitively; `#fragment` folds
  through the *shared* `doc::slug::base_slug`. Everything else — `mailto:`, `file:`,
  `javascript:`, local `.md` — is **wholly inert**, failing closed.
- **A hotspot carries the FULL url**, never the `elide_middle`d form on screen.
- **Activation is on the RELEASE edge for every control**, `[copy]` included.
- **`blit` carries hotspots; it drops `Pin` and `Atom`.** A hotspot is clamped, and
  dropped when nothing survives: a region that reacts while showing nothing of the control
  is worse than no region. A hotspot over the column an overflow chevron kept is revoked.
- **No apt/yum repository, no GitHub Pages site, no container image, no macOS
  notarisation.** All four considered and rejected; reasons in the publishing spec §1.

## 6. Where the detail lives

- **Design authorities:** `docs/superpowers/specs/2026-08-08-mdmost-design.md` (renderer),
  `2026-08-11-semantic-selection-design.md`, `2026-08-11-clickable-links-design.md`.
- **Finished plans**, for why the code looks as it does: `docs/superpowers/plans/`.
- **The SDD ledgers for both plans live at `/scratch/oetiker/mdmost-ledgers/`** — 112
  files: `2026-08-11-semantic-selection/` and `2026-08-12-clickable-links/`, each holding
  `progress.md` plus every brief, report, review and review diff, and every ruling on a
  deferred minor. They are **outside git by design** (the SDD directory is gitignored) and
  were copied out of the `mdmost-semantic-selection` worktree just before it was removed on
  2026-08-13, because the equivalent ledgers for an earlier plan were destroyed exactly
  that way and commit messages became the only record. `selection-review.html`, a Task 3
  review artifact, is archived beside them. **Nothing backs this directory up.**
- **Demo:** `demo/tour.md`, `demo/mdmost.toml` (the ansidrama script), `demo/tmux.conf`,
  `demo/config.toml`; regeneration recipe, drift warnings, the frame-numbering trap and
  the cut theme beat all in `docs/maintainer-notes.md`. Reference repo:
  `~/checkouts/ansidrama`.
- **Owner's test pages:** `mdmost-tryout/links.md` and `buttons.md`. **Run with `--mouse`
  or there are no controls at all.**
- **Durable facts worth knowing:** DEL, U+009B, NEL, LINE and PARAGRAPH SEPARATOR and both
  C1 endpoints all survive CommonMark's bare-destination grammar into a link's URL
  unchanged — only true ASCII C0 controls are excluded. And **ratatui's
  `Buffer::set_line` silently drops every `Cc` character**, which is why the
  escape-injection threat does not reproduce and why the real defect is a silent
  zero-width drop against width arithmetic that charged one column.
- Key files: `src/canvas/ops.rs` (`blit`, `merge_hotspots`, `clamped_claim`,
  `revoke_hotspots_over`), `src/canvas/mod.rs` (`Hotspot`, `HotspotKind`),
  `src/render/inline.rs` (`link`, `flatten`, `reconcile`), `src/render/link.rs`
  (`classify`), `src/tui/draw.rs` (`hover_highlight`), `src/tui/chrome.rs`
  (`draw_status`, `sanitized`), `src/tui/app.rs` (`press_hotspot`, `release_hotspot`,
  `cursor_step`, `control_targets`), `src/tui/popup.rs`, `src/tui/select.rs`.

## 7. Open questions / pending decisions

1. **The owner's manual GitHub steps** (§3.1) — the only real blocker.
2. **The release workflow has never executed.** The changelog rewrite, the formula marker
   rewrite and the deb/rpm builds were rehearsed locally; the matrix builds, crates.io
   publish, GitHub release and Homebrew commit are unexercised.
3. **Windows compiles but has never been run.** Mouse, clipboard, alternate screen, motion
   and release handling all unexercised there.
4. **The demo's theme beat is cut and wants an ansidrama fix** (§4.5). Two related
   ansidrama limitations are also written up in `demo/mdmost.toml`: `Recorder::move_to`
   sends no bytes, so a recorded pointer never lights anything and hover has to be spelled
   as a raw SGR motion report through the `keys` escape hatch — which means the frame
   showing a lit link has no arrow resting on it. Fixing `move_to` would make every
   pointer glide in the tour truthful, and would want its own before/after.
5. **`demo/tour.md` claims "the status bar says which, every time"**, but at act 4's
   49-column pane the copy notice is dropped by the status bar's width budget, so the demo
   promises something the demo does not show. Pre-existing and by design; fixing it means
   changing the drop priorities or re-staging act 4 at full width.
6. **Two unguarded `Span` sites**, pre-existing: `chrome::highlighted` (TOC heading text)
   and the status-bar breadcrumb. Same width-drift exposure the URL had.
6b. **Feature idea, owner's, 2026-08-13 — a language icon on the code frame's label.**
   `src/tui/icons.rs` has ten fixed UI icons behind `Icons::new(nerd_font)`; nothing is
   per-language. A Nerd Font dev-icon map (Rust, Python, TOML, …) with a fallback for the
   long tail would put a glyph beside the `rust` label, degrading to today's plain label
   when icons are off. **Needs its own brainstorm and spec — it is a feature, not
   documentation.** The known risk is the one this project keeps paying: PUA glyphs have
   an advance width that need not match what `unicode-width` reports, and the frame label
   does column arithmetic against the frame edge — the same shape as the `Cc` zero-width
   drop in the status bar.
7. **A deferred minor worth doing:** the invariant "a hotspot never claims a cell the
   canvas does not have" is held by argument at three call sites, not by construction.
   Adding it to `Canvas::check_invariants` would make a future op that forgets to clamp
   fail loudly in ~40 existing tests.
8. **The ledger archive at `/scratch/oetiker/mdmost-ledgers/` is unbacked** (§6). If this
   project ever gets a remote, that is the moment to decide whether the ledgers belong in
   it, in a sibling repository, or nowhere.
9. Carried and still open: the RPM payload is unverified (`rpm(1)` is not installed here);
   `fc-list` as the macOS icon probe is untested there; nested diagrams are never widened
   while nested tables are; Nerd Fonts v2 patches fail icon detection (deliberate); the
   banner fixture is self-referential because `figlet`'s `Small` font is not installed
   here; `&nbsp;` as a wrapping opportunity is flagged and unruled; the banner's internal
   band centring was never ruled on; the light theme's heading ramp is flat and
   non-monotone (measured 4.80 → 4.89 → 4.92 → 4.95 → 4.90 → 4.86:1) — re-measure before
   designing against it.
10. An environmental quirk with no root cause: `diff <(git show HEAD:path) file` produced a
    spurious full-file diff twice on byte-identical files (confirmed by `sha256sum`). Use
    real intermediate files for verification diffs.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited**: `git merge-base --is-ancestor
  HEAD main`, `git log --oneline HEAD..main`, `git branch -a --contains HEAD`.
- **Sibling worktrees created after this commit are invisible here.** Run `git worktree
  list`. The one thing this file can promise is that `mdmost-semantic-selection` was
  merged and tombstoned at `dbe236d`.
- **"There is no remote" rots the moment the owner creates the repository**, which is step
  one of their list. Check `git remote -v`.
- The 1292-test count and the four green gates are as of `dbe236d`. Re-run them; a count
  that moved without an explanation is the signal.
- **~14 GB of orphaned cargo target dirs** live under `/scratch/oetiker/` from removed
  worktrees. There was no disk pressure at the time of writing. Ask before deleting, and
  never use `cargo sweep --stamp`/`--file` with a shared target dir.
