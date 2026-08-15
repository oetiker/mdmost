# Controller Handoff — mdmost trunk, published but unreleased

> Starter pack for the next controller session. This handoff lives in ONE worktree — run
> `git worktree list` first and confirm this is the workstream you're resuming. Read this
> first, then `git log <handoff-commit>..HEAD`. Detail is NOT here — it is in git and in
> the documents named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry forward any
> lesson in §4/§5 still true. Fresh synthesis, not blank page. On merge into another
> branch, rewrite that branch's handoff to the merged reality — do not preserve this text,
> and tombstone the branch you merged.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-15   Reason: milestone — the demo's blocking ansidrama fix shipped; the tour re-think is designed but not started
Worktree / branch: main checkout `/home/oetiker/checkouts/mdmost` @ `main`
Trunk at time of writing: this **is** trunk. `main` @ `56da66b`, which equals `origin/main`.

Sibling worktrees, as of this commit — verify with `git worktree list`, this line cannot
see anything created later:

- `/scratch/oetiker/claude-worktrees/mdmost-tryout` — detached at `b2ccc74`, **merged into
  `main`**; the owner's test binary and pages (`links.md`, `buttons.md`). Never commit here.
- `/scratch/oetiker/claude-worktrees/mdmost-verify` — **the controller's own worktree**,
  detached at `172d39b`, **merged into `main`**. Every gate run and every mutation happens
  here, never in an implementer's tree (§4.1). Never let an implementer near it.
- `mdmost-semantic-selection` — merged, tombstoned, worktree removed 2026-08-13. The branch
  survives carrying nothing but its gravestone.

**A second repository is now part of this workstream:** `~/checkouts/ansidrama`, which
records the demo. See §2 and §6.

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
*is* the design; stop defending the old framing and follow it. They also move fast on
their own: a PR opened this session was merged and released while the session was still
running (§4.11).

## 2. Where we are, as of the handoff commit

Re-derive rather than inherit (§8). Every plan ever written for **mdmost** is executed and
on trunk; the four are listed in the previous handoff's §2 and in `docs/superpowers/plans/`.

**What changed since `dbe236d` (the previous handoff):**

- **The repository is public and pushed.** `github.com/oetiker/mdmost`, created
  2026-08-13, `main == origin/main`, CI green on the last three pushes. The previous
  handoff's "there is no remote and nothing has ever been pushed" is dead — as it warned it
  would be. **mdmost has no tags, so its release workflow has still never executed.**
- **A documentation design spec landed but was never ruled on:**
  `docs/superpowers/specs/2026-08-13-documentation-design.md`, status *"proposed, awaiting
  owner review"*. One manual (`docs/manual.md`) as single source; README shrinks to a
  30-second pitch; `man/mdmost.1` becomes a CI-generated artifact, gitignored, with
  `man/**` added to `Cargo.toml`'s `exclude`. No plan exists for it.
- **The demo was re-recorded on ansidrama 0.2.0**, with the theme beat cut (§4.5).

**The ansidrama workstream, opened and closed this session.** The demo's recorder gained
0.3.0 (a rewrite of the capture path) and then 0.4.0 (this session's work):

| Version | What it means for this demo |
| --- | --- |
| 0.3.0 | continuous sampling; `await`; `animated`/`realtime`; `manifest.tsv`. **`settle_ms` and `react_ms` are gone and a config carrying either fails to parse.** |
| 0.4.0 | a glide *reports* its motion under a mouse-mode gate; a new `move` scene action |

`0ad863c` + `d9a95ea` on `feat/pointer-motion` → PR #3 → merged as `321ee5f` → tagged
`v0.4.0` (`1963bf2`). Verified by the controller: fmt/clippy/test all exit 0, 95 → 98
tests, both gate mutations kill exactly one test each, and an end-to-end probe against
`mdmost` records a frame with the arrow resting on the link and `https://github.com/…` in
the status bar. **This is the frame the tour could never produce.**

**The tour re-think (project B) is designed in conversation but not started.** No spec, no
plan, no code. Its dependency is now satisfied.

## 3. Do this next

1. **`demo/mdmost.toml:23` still has `settle_ms = 300`, and ansidrama ≥0.3.0 refuses to
   parse a config carrying it.** The tour cannot be recorded *at all* until that line goes.
   This is the first thing to discover and the cheapest thing to fix, so know it before
   promising anything about the demo.
2. **Project B — re-think the tour — is the owner's live request and it is unblocked.**
   They chose "machinery, plus re-stage the beats that fought the old model", not a
   narrative rewrite: same seven acts, same story. That is an **architectural**
   brainstorming conversation (spec → plan), not a task to start editing. §7.1 lists the
   beats it must cover.
3. **The mdmost release has still never run** (§7.2). No tags exist. Its first real run is
   its first test and is worth watching, not firing and walking away.
4. **The documentation spec is awaiting the owner's review** (§2) and has been since
   2026-08-13. Ask; do not start implementing it unasked.

## 4. Lessons & traps ← the irreplaceable part

Carried forward and still true: **give every agent its own `CARGO_TARGET_DIR`**; **never
read a gate's result through a pipe** (a redirect is *not* a pipe and preserves exit
status — and note `${PIPESTATUS[0]}` when you must pipe); the clippy gate is
`--all-targets -- -D warnings` because plain `cargo clippy` exits 0 on warnings; **fault
injection must run `--no-fail-fast`**; **read the log, not the exit code** — exit 101 is
identical for a panicking test and a failed compile, so grep `^error\[` before believing
any red; **verify a subagent's arithmetic, not its adjectives**; **do not choose glyphs by
measuring them**; **a rename is not a `sed`**; **when two code paths render "the same"
thing, prove it**; **measure box art in columns, not bytes**; **backticks in `git commit
-m` are command-substituted — use a quoted heredoc**; **`git merge` does not accept `-F -`**;
**never merge into a dirty worktree**; **`git status --porcelain --ignored` before removing
a worktree** — the ledger is gitignored and dies with it; **diagnose a silent agent by the
worktree, never by the silence**; **state process constraints as actions, not
prohibitions**; **dispatch at most ONE file-writing agent per worktree**.

1. **THE CONTROLLER IS A WRITER.** Corrupted an implementer's tree twice by running a
   backup → mutate → suite → restore cycle in the worktree it was live in. "Check it is
   clean first" does not work: a gate run takes minutes and the agent resumes inside that
   window, so the check and the write are not atomic. **The controller owns
   `mdmost-verify` and does every gate run and every mutation there.**
2. **A missing test is only a gap if you can name a mutation that survives without it, and
   a Minor is only Minor for as long as its blast radius holds.** A stale click candidate
   rated Minor on "today it is a spurious re-copy" became Important because the *next task*
   made it a browser opening unbidden. The inverse matters too: mutating for the bug a
   demanded second test would catch showed the existing test already caught it.
3. **Three vacuous-test species, all caught only by mutation.** A test that calls the
   helper directly proves the tool works, not that it is *used*. An assertion that holds
   for two different reasons cannot distinguish them ("no raw ESC in the output" is true
   whether or not the sanitiser runs, because ratatui drops `Cc` itself). And — new this
   session — **an assertion whose target also appears somewhere innocent**: the hover probe
   awaited `github.com`, which is *also in the document body*, so an unscoped pattern would
   have passed without any hover happening. `row = -1` scoping is what made it a real test.
   Also: **prove a mutation makes a test FAIL, not skip.**
4. **A gate is tested by the case where it does NOT fire.** The pointer-motion gate got two
   tests deliberately: one app in `?1003h` that must receive motion, one in `?1000h` that
   must receive none. Each dies under the mutation that breaks only it (`reports = true`
   kills the second, `reports = false` kills the first). **A gate that has quietly stopped
   gating looks identical to one that works if you only ever test the open case.**
5. **When a recorded beat looks wrong, check the app in a live pane before changing the
   script.** It cost a take once: the footnote counter read `7 → 6 → 5 → 5`, an apparently
   swallowed keystroke, and the "fix" was three separate scenes. It was an off-by-one — the
   0.2.0 run log *counted* frames rather than numbering them. **That specific trap is now
   retired: ansidrama ≥0.3.0 writes `manifest.tsv` mapping frame → scene → input.** The
   lesson that outlives it is the method: a `tmux -L probe` pane walking the same keys
   settles these in a minute, and the wrong beat always looks plausible.
6. **Read coordinates off the screen; never derive them.** Re-confirmed this session: the
   link in act 6 sits on **row 19, columns 5–20** of a 100-column frame, read off
   `tmux capture-pane -p`, which is how `(12, 19)` was validated rather than assumed. A
   click or hover that lands on nothing is **silent**.
7. **Do not ship a frame that contradicts its own status bar.** The theme beat produced a
   dark screen captioned `theme: light` and held it 2.8 s on the hero image. **The likely
   cause is now fixed upstream**: 0.3.0's changelog says output still draining from the
   *previous* input could end the current input's wait, and the grace is now measured on
   real grid changes rather than PTY bytes — which is mechanism (a) of the findings doc,
   and matches the bisect exactly (only the script with a five-repaint keyboard walk before
   the `t` failed). **But `await` matches TEXT, not COLOUR.** The broken frame had a
   correct `theme: light` status bar over a dark body, so an `await` on that string would
   happily match it again. Restoring the beat needs a *pixel* check of the dumped frame,
   not an await. Do not claim the beat is fixed on the strength of a green recording.
8. **Six of my own file references were wrong**, and five implementers correctly overruled
   me. **Name the CONSTRAINT — which layer must not learn this — not the file.**
9. **Before discarding a dirty tree, read the diff.** After interfering once I assumed the
   dirt was my own residue and was one command from `git checkout --`. It was the
   implementer's *better* uncommitted work.
10. **The backgrounding bug hit five of six implementers.** All three of `timeout: 600000`,
    foreground, and *wait for it in the same turn* must be in every brief — and it still
    recurs, so the reliable defence is the diagnosis: **dirty tree + live build = resume
    that agent, never re-dispatch**.
11. **Integration state changes UNDER you, not just between sessions.** PR #3 was opened,
    reviewed, merged and released *within this session*; a `gh pr view` early in the
    handoff writing said OPEN and the next said MERGED. This is the §8 rule biting in real
    time rather than a hypothetical about stale files. Re-derive before every claim.
12. **Verify a claim about a neighbouring codebase by reading it, not by inferring.** I
    told the owner every mouse beat in the tour was decorative. Wrong: `drag` already sent
    real motion per cell (`record.rs:306–310`) and `click` a real press/release. Only the
    *approach glide* was silent. Reading the code first would have scoped the project
    correctly at the outset instead of one message later.
13. **`read -t N </dev/null` is not a delay** — it hits EOF and returns instantly, so a
    probe "waiting" 6 s waited 0 and reported a splash screen twice. Where the harness
    blocks foreground `sleep`, use `timeout N tail -f /dev/null`.
14. **An abandoned branch can hold the only copy of a real fix.** Check by content, not by
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
  for — which is why `--render-once` shows none. But **`--render-once` DOES record
  hotspots**; gating links on it would blank every link for every mouseless reader. Spec
  §4's contrary line was struck by owner ruling (`172d39b`).
- **`HOVER_SHIFT = 0.6`** by owner ruling. Do not retune without asking.
- **Only `http` and `https` become controls**, case-insensitively; `#fragment` folds
  through the *shared* `doc::slug::base_slug`. Everything else — `mailto:`, `file:`,
  `javascript:`, local `.md` — is **wholly inert**, failing closed.
- **A hotspot carries the FULL url**, never the `elide_middle`d form on screen.
- **Activation is on the RELEASE edge for every control**, `[copy]` included.
- **`blit` carries hotspots; it drops `Pin` and `Atom`.** A hotspot is clamped, and dropped
  when nothing survives. A hotspot over the column an overflow chevron kept is revoked.
- **No apt/yum repository, no GitHub Pages site, no container image, no macOS
  notarisation.** All four considered and rejected; reasons in the publishing spec §1.

Settled on the ansidrama side, this session:

- **Motion is mode-gated, and the gate is not configurable.** A glide reports only under
  any-event tracking (`?1003h`). A per-scene `hover = true` override was designed as
  insurance and then **rejected**, because the probe showed tmux does propagate `?1003h` on
  behalf of the focused pane. Do not reintroduce the knob without a case that needs it.
- **The tour re-think is machinery and staging, NOT a narrative rewrite.** Owner's ruling:
  same seven acts, same story, same features shown.

## 6. Where the detail lives

- **Design authorities:** `docs/superpowers/specs/2026-08-08-mdmost-design.md` (renderer),
  `2026-08-11-semantic-selection-design.md`, `2026-08-11-clickable-links-design.md`,
  `2026-08-13-documentation-design.md` (**unruled**).
- **Finished plans:** `docs/superpowers/plans/`.
- **The SDD ledgers live at `/scratch/oetiker/mdmost-ledgers/`** — 112 files for the
  semantic-selection and clickable-links plans, outside git by design, copied out before
  the worktree holding them was removed. **Nothing backs this directory up.**
- **Demo:** `demo/tour.md`, `demo/mdmost.toml` (the ansidrama script), `demo/tmux.conf`,
  `demo/config.toml`; regeneration recipe, drift warnings and the cut theme beat in
  `docs/maintainer-notes.md`. **Both files still describe the 0.2.0 timing model
  (`settle_ms`, "the run log counts frames") and are wrong for ≥0.3.0** — rewriting them is
  part of project B, not a separate chore.
- **ansidrama** (`~/checkouts/ansidrama`, `github.com/oetiker/ansidrama`):
  `docs/superpowers/specs/2026-08-15-pointer-motion-design.md` is this session's design;
  `docs/mdmost-theme-capture-findings.md` is the bisect evidence for the theme bug;
  `docs/regression-gate.md` covers its capture gate. The README's new **Hover** section is
  the user-facing contract.
- **The mouse-mode probe recipe**, should it need repeating: run tmux under `script`, then
  `grep -aoP '\x1b\[\?(1000|1002|1003|1006)[hl]'` the typescript. That is what proved tmux
  propagates `?1003h`.
- **Owner's test pages:** `mdmost-tryout/links.md` and `buttons.md`. **Run with `--mouse`
  or there are no controls at all.**
- **Durable facts:** DEL, U+009B, NEL, LINE/PARAGRAPH SEPARATOR and both C1 endpoints all
  survive CommonMark's bare-destination grammar into a link's URL unchanged. **ratatui's
  `Buffer::set_line` silently drops every `Cc` character**, which is why the
  escape-injection threat does not reproduce and why the real defect is a silent
  zero-width drop against width arithmetic that charged one column.
- Key files: `src/canvas/ops.rs` (`blit`, `merge_hotspots`, `clamped_claim`,
  `revoke_hotspots_over`), `src/canvas/mod.rs` (`Hotspot`, `HotspotKind`),
  `src/render/inline.rs`, `src/render/link.rs` (`classify`), `src/tui/draw.rs`
  (`hover_highlight`), `src/tui/chrome.rs` (`draw_status`, `sanitized`), `src/tui/app.rs`
  (`press_hotspot`, `release_hotspot`, `cursor_step`, `control_targets`),
  `src/tui/term.rs:249` (mouse capture is `?1003h`), `src/tui/popup.rs`, `src/tui/select.rs`.

## 7. Open questions / pending decisions

1. **Project B's scope, already agreed and waiting for a design.** The beats that fought
   the old model and should be re-staged:
   - **the hard blocker:** `settle_ms = 300` must go (§3.1);
   - **act 5's three sacrificial `g`s** — a documented "coin toss" buying time for tmux to
     close a pane. Replaceable with an `await` on the full-width screen.
   - **act 4's extra `keys = []` re-capture** — a workaround for nano redrawing a paste in
     several writes, which continuous sampling may make unnecessary.
   - **act 6's hover** — now expressible as `move = { x, y }` with an `await`, retiring
     eleven lines of comment and the raw SGR escape hatch.
   - **the theme beat** — restorable, but see §4.7 on why `await` cannot verify it.
   - **act 4's 49-column pane drops the copy notice** the tour promises aloud (below).
   - Every `[copy]` click, currently silent on a miss, can become an `await`.
2. **mdmost's release workflow has never executed.** No tags. The changelog rewrite, the
   formula marker rewrite and the deb/rpm builds were rehearsed locally; the matrix builds,
   crates.io publish, GitHub release and Homebrew commit are unexercised.
3. **The documentation design spec is unruled** (§2).
4. **Windows compiles but has never been run.** Mouse, clipboard, alternate screen, motion
   and release handling all unexercised there.
5. **`demo/tour.md` claims "the status bar says which, every time"**, but at act 4's
   49-column pane the copy notice is dropped by the status bar's width budget. Pre-existing
   and by design; fixing it means changing drop priorities or re-staging act 4 at full width.
6. **Two unguarded `Span` sites**, pre-existing: `chrome::highlighted` (TOC heading text)
   and the status-bar breadcrumb. Same width-drift exposure the URL had.
7. **Feature idea, owner's, 2026-08-13 — a language icon on the code frame's label.**
   `src/tui/icons.rs` has ten fixed UI icons behind `Icons::new(nerd_font)`; nothing is
   per-language. **Needs its own brainstorm and spec.** Known risk: PUA glyphs have an
   advance width that need not match `unicode-width`, and the frame label does column
   arithmetic against the frame edge — the same shape as the `Cc` zero-width drop.
8. **A deferred minor worth doing:** "a hotspot never claims a cell the canvas does not
   have" is held by argument at three call sites, not by construction. Adding it to
   `Canvas::check_invariants` would make a future op that forgets to clamp fail loudly in
   ~40 existing tests.
9. **The ledger archive is unbacked** (§6). Now that a remote exists, this is the moment to
   decide whether the ledgers belong in it, in a sibling repository, or nowhere.
10. Carried and still open: the RPM payload is unverified (`rpm(1)` is not installed here);
    `fc-list` as the macOS icon probe is untested there; nested diagrams are never widened
    while nested tables are; Nerd Fonts v2 patches fail icon detection (deliberate); the
    banner fixture is self-referential because `figlet`'s `Small` font is not installed
    here; `&nbsp;` as a wrapping opportunity is flagged and unruled; the banner's internal
    band centring was never ruled on; the light theme's heading ramp is flat and
    non-monotone (4.80 → 4.89 → 4.92 → 4.95 → 4.90 → 4.86:1) — re-measure before designing
    against it.
11. An environmental quirk with no root cause: `diff <(git show HEAD:path) file` produced a
    spurious full-file diff twice on byte-identical files (confirmed by `sha256sum`). Use
    real intermediate files for verification diffs.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** Whether this branch is merged,
  pushed or superseded is not knowable from this file: `git merge-base --is-ancestor HEAD
  main`, `git log --oneline HEAD..main`, `git branch -a --contains HEAD`. If this branch is
  merged, stop reading and go to the successor's handoff. §4.11 is this rule failing inside
  a single session — treat it as a live hazard, not a formality.
- **Sibling worktrees created after this commit are invisible here.** Run `git worktree
  list`.
- **ansidrama's `v0.4.0` release run was IN PROGRESS as this was written**, and
  `gh release view v0.4.0` said "release not found". The tag `1963bf2` exists and the merge
  is real; whether the release, its assets and any crates.io publish completed is not
  something this file knows. Check `gh run list --workflow=release.yml -R oetiker/ansidrama`.
- **mdmost had no tags at this commit**, so its own release has never run. `git tag`.
- The claim that mdmost's gates are green is inherited from `dbe236d` and was **not
  re-derived this session** — nothing touched mdmost's `src/` since, but the honest move is
  to run them rather than quote this sentence. The last figure was 1292 tests across 33
  suites; a count that moved without an explanation is the signal.
- **`demo/mdmost.toml~` is an untracked editor backup** in the working tree, differing from
  the real file, and `~/checkouts/ansidrama/README.md~` is another. Neither is mine to
  delete.
- **~14 GB of orphaned cargo target dirs** live under `/scratch/oetiker/` from removed
  worktrees, plus a new `cargo-target-ansidrama`. There was no disk pressure. Ask before
  deleting, and never use `cargo sweep --stamp`/`--file` with a shared target dir.
