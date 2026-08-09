# Controller Handoff — mdless

> Starter pack for the next controller session. This handoff lives in ONE
> worktree — run `git worktree list` first and confirm this is the workstream
> you're resuming. Read this first, then `git log <handoff-commit>..HEAD` for
> everything that changed since. Detail is NOT here — it's in git and the docs
> named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry
> forward any lesson in §4/§5 that is still true. Fresh synthesis, not blank
> page.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-09   Reason: context budget (the user called it: I had started
hand-resolving merges instead of delegating them)
Worktree / branch: main checkout (/home/oetiker/checkouts/mdless) @ main
Trunk at time of writing: `main`, 897 tests / 28 suites green, clippy and fmt
clean. **Re-derive anyway** (§8).
Sibling worktrees: many, under `/scratch/oetiker/claude-worktrees/`, one per
`isolation: worktree` subagent. **Most are merged and dead.** `git worktree
list` plus `git branch -a --merged main` is the only authority. Also still
present: four dead scratch worktrees from the original build (`mdless-gate`,
`-layout`, `-qa`, `-rendercheck`) the user chose to leave alone, and
`mdless-icons`, merged and tombstoned.

## 1. Mission

`mdless` is a full-screen terminal pager for a single Markdown document — "as
pleasant to look at as btop, as pleasant to use as less". Rust + ratatui.
GFM including tables with Markdown inside cells, syntax-highlighted code, and
seven Mermaid families drawn as Unicode box art. Reflows on resize, TOC pane,
search, themes, mouse.

The mental model that matters: **rendering is a pure function of
`(AST, width, theme, options)`**. Parse once; no layout decision at parse time;
a resize discards the canvas and renders again.

**How the user works, and it is not optional.** They want everything driven
through subagents, reviewers driving the real binary in tmux rather than
trusting tests, and to be told when it is done — not consulted on mechanics.
They review by *looking at output* and their findings are consistently precise
(they have caught a partial-cell background, a bullet that was too heavy, and a
nested-list seam, all by eye). When they ask a design question, answer it with
rendered samples, not prose.

**The failure that triggered this handoff:** eight branches landed at once and I
merged them by hand, resolving conflicts myself. That is what burned the
context. **Delegate merges too** — there is a worked example: the agent
`a44d578c2c4f1f13a` was given the aborted `worktree-agent-a567d07f9fe243958`
merge with a description of what each side wanted, and told to regenerate
snapshots rather than hand-resolve them. Do that from the start next time.

## 2. Where we are now

An enormous amount landed today. `git log --oneline` is the record; the shape:

- **Wide diagrams scroll** instead of dumping source (spec
  `2026-08-09-wide-diagram-scrolling-design.md` rev 2), on top of monotone fit,
  a rebuilt horizontal-scroll model, frame-closing at clipped edges, and a
  pinned line-number gutter.
- **The gutter seam is published by the renderer**, not sniffed off the canvas
  by the pager — this cleared a code review's DO NOT SHIP.
- **No control character reaches a cell.** The reported bug was a raw tab
  breaking the width guarantee; the fix found ESC reaching the canvas too, so a
  document could repaint the reader's terminal from inside a paragraph.
- **The pager exits when its terminal dies** instead of spinning at 100 % CPU
  (user-reported from the wild).
- **Owner-requested typography:** bullets are ASCII (`*` `>` `+` `-`),
  heading prefix glyphs replaced by an underline ladder, a FIGlet banner for a
  lone `#` title, lists and multiline table rows get air, section numbering for
  deep documents, a body-width cap, `S` saves settings.
- **Mouse:** drag copies the *source* Markdown; the scrollbar drags; library
  stderr no longer scribbles on the screen; the X11 clipboard is held so it
  survives.

## 3. Do this next

**In flight at this commit** — check `git branch -a` and `git worktree list`
before assuming any of it is unmerged:

1. `a44d578c2c4f1f13a` is merging `worktree-agent-a567d07f9fe243958` (bullets,
   checkboxes, nested seam) into `main`. If it failed, its brief has the full
   description of what each side wants.
2. An agent is widening syntax highlighting via `two-face` (`bat`'s syntax set).
   Today's set is **75 syntaxes** — syntect's 2016 default — missing TypeScript,
   Kotlin, Swift, Zig, Nix, Dockerfile, Terraform, Elixir and **TOML**, which is
   mdless's own config format.

**Then, in rough priority:**

- **The light theme's heading ramp is flat and non-monotone** — measured
  4.80 → 4.89 → 4.92 → 4.95 → 4.90 → 4.86:1, so it *rises* through H4. Dark
  steps correctly but every heading is dimmer than the body text it introduces
  (8.42 down to 5.56 against body at 13.27). Now more urgent: the prefix glyphs
  are gone, so in the light theme H3/H4/H5 are separated by **dash period
  alone**. `tests/theme_contrast.rs` is where to pin the fix.
- **The code palette has fewer distinct roles than names.**
  `code.line_number == code.operator` and `code.language == block.quote_bar` in
  both themes. Nothing depends on them being distinct any more, but it is a
  smell worth resolving with the ramp work.
- **Remaining exerciser defects** (`git log` for the fifteen; several are fixed).
  Still open: diagram routing (edges entering a box's top border as `┴`, a thick
  edge attaching at a corner, `classDiagram` leaking `*`/`$` classifiers into
  return types, `stateDiagram` drawing `note right of` on the left and
  duplicating an edge label), and the `journey` family being called "not a
  diagram type" when it is a real Mermaid family.
- **`usability-review-2.md` findings 2-13** — still untouched, still cheap.
- **A widened CJK table renders differently with and without an active search**,
  reproducibly. Two agents hit it; neither settled whether it is
  `highlight_matches` patching continuation cells or a tmux artifact. Needs a
  real terminal.
- **Search does not scroll horizontally to an off-screen match.** Pre-existing.
- One agent reported `/query` finding 0 matches for any query, on trunk. **I
  never verified this** — check before believing it, and if true it is urgent.

## 4. Lessons & traps ← the irreplaceable part

1. **Give every agent its own `CARGO_TARGET_DIR`.** Shared is the most expensive
   hazard here. `touch src/lib.rs && cargo build` before believing a surprise.
2. **Never read a gate's result through a pipe.** `cargo test 2>&1 | tail`
   returns tail's exit code and has hidden a red suite here.
3. **A green property test proves nothing about the run you didn't do.** When
   `tests/render_property.proptest-regressions` grows, commit the seed **with**
   its fix — a seed alone hands the next merge a permanently red suite.
4. **False doc comments are the most dangerous defect class.** Five instances
   now. The pattern: a comment true of one function and false of its caller
   (`render_document`'s margins vs the pager; `code.rs`'s gutter claim vs the
   pager). **Grep the prose when you change behaviour.**
5. **Per-item invariants do not add up to a whole-object invariant.**
6. **Prove every behavioural test red before you fix.** Non-negotiable here.
7. **Ask reviewers to refute your diagnosis, not implement it.** I told an agent
   the CPU spin was in crossterm's `read()`; it was in `poll()`, and the agent
   proved it with an instrumented probe because the brief said "I would rather
   be corrected than have you implement around a wrong theory". Put that
   sentence in every brief where you are guessing.
8. **A test can be green for a reason unrelated to the code.** The hangup test
   had two false greens (inherited pty fds; an undrained pty). Ask: *what would
   make this pass with the bug present?*
9. **Do not choose glyphs by measuring them — you do not control the reader's
   font.** Three rounds were burnt tuning a bullet by rasterised em-fractions in
   one font that an early session merely *guessed* the owner used, then cited
   back as authority ever after. Coverage is the only legitimate font question
   (`◦` U+25E6 and `⦁` U+2981 both draw blank in real fonts), and the durable
   answer to it is to prefer characters that render everywhere: the bullets are
   now ASCII. A guess repeated in a doc comment becomes a false premise.
10. **When you enumerate a class, enumerate it exhaustively.** The U+17D8 fix
    checked all 0x110000 scalars and closed the class.
11. **Fixing a class finds worse than the instance.** The raw-tab fix found ESC
    reaching the canvas — an escape-injection hole nobody had reported.
12. **A partial glyph needs an explicit background.** Three bugs, one shape: the
    zebra stripe punched through at column rules, the help overlay's text in
    dark boxes, the meter's part-filled cell on a hole. Grep for foregrounds
    drawn with no background.
13. **An accepted trade-off should record the premise it rests on**, because a
    later change removes the premise silently. The word-breaking ladder rungs
    were justified by "the counterfactual is a source dump" — and scrollable
    diagrams destroyed that counterfactual.
14. **Two hostile reviewers, briefed differently and blind to each other, are
    worth far more than one.** One found a memory blow-up and a double layout;
    the other found the whole-page scroll drag. Neither found the other's.
15. **A reviewer who cannot reproduce the objection they were briefed to make
    should say so.** One was briefed that scrolling an LR diagram would be
    incoherent, tried it, found it reads as a filmstrip, and said so.
16. **Reviews and measurements go stale within hours.** A review bisected a
    threshold at 92 columns; by the time it was designed the same chart drew at
    62 because an earlier commit had moved it. Re-derive before building.
17. **My own verification can be wrong in the same way a reviewer's is.** I
    "confirmed" the page background was never painted by reading
    `capture-pane -e` line by line. The SGR stream is *continuous across
    lines*, so every row after the first looks bare. A stateful parse showed
    zero cells on the terminal's background. **Parse escape streams statefully.**
18. **Merge conflicts here are usually semantic, not textual.** Several merged
    cleanly and did not compile; one of my own resolutions silently dropped a
    `#[test]` attribute, caught only by the dead-code lint. **A test that stops
    running looks exactly like a test that passes.** Delegate merges (§1).
19. **Do not hand-resolve snapshot conflicts.** Take either side, regenerate
    with `INSTA_UPDATE=always`, read every diff, and check that *both* sides'
    changes are present — a diff showing only one side means something was lost.

## 5. Don'ts & constraints

- **No HTML.** Not rendered, not passed through.
- **Mermaid is Unicode box art only**, never raster.
- **Nerd Font glyphs are DETECTED, not defaulted on** (spec §2.1). Detection
  answers yes only on positive evidence.
- **`Esc` never quits.** It unwinds count → search → TOC filter → TOC focus →
  TOC pane.
- **Do not widen the `NodeArt` seam.** Read what you need off the drawn canvas.
- **`render` must not depend on `tui`.** Policy constants live in `tui` and are
  passed in. Canvas metadata (anchors, spans, pins) is the legitimate channel
  from renderer to pager.
- **`#![forbid(unsafe_code)]`** holds in the library. `rustix` for syscalls.
- **The status bar never lies.** If a claim cannot be verified (OSC 52), word it
  as what is known ("sent, unconfirmed"), never as success.
- **No 1000-node golden snapshot** (spec §13.2), and the same argument applies
  to any fixture nobody will read: prefer several named ones.
- 4-core cap: `CARGO_BUILD_JOBS=2` on every cargo invocation.
- **tmux: kill only your own session, never `kill-server`** — an agent did once
  and may have destroyed a colleague's work mid-run.
- **Leave no stray `mdless` processes.** Check `pgrep -f mdless` and kill only
  your own; other agents run theirs concurrently.

## 6. Where the detail lives

- **Design spec (the authority):** `docs/superpowers/specs/2026-08-08-mdless-design.md`.
- **Feature spec:** `docs/superpowers/specs/2026-08-09-wide-diagram-scrolling-design.md`
  (revision 2 — revision 1 is in git and is wrong in four ways).
- **Maintainer notes:** `docs/maintainer-notes.md` — now also carries the
  control-character contract and the terminal-width measurement table (which
  terminals cluster flags and ZWJ sequences correctly; tmux 3.4 does not).
- **QA:** `docs/qa/visual-review-3.md` is the best worklist. Its verdict was
  "no"; a fourth review after the wide-diagram work also said **no**, on the
  page-background claim (refuted, §4.17) plus contrast findings (fixed) — but
  its "what is genuinely good" section is the list of things not to break.
- Review and agent transcripts are subagent task outputs, not tracked files.

## 7. Open questions / pending decisions

1. **Nothing is blocked on the user.** They are reviewing output as it lands and
   raising findings; expect more.
2. **`fc-list` as the macOS icon probe** is untested there; detection falls back
   to plain, which is safe but pessimistic.
3. **Nested diagrams are never widened** (list, blockquote) while nested tables
   are — the same fence behaves differently indented two spaces. Recorded in
   the feature spec's "Out of scope".
4. **Nerd Fonts v2 patches now fail icon detection.** The task boxes moved to
   Material Design code points, so `nerdfont.rs`'s document-half probe moved
   from `0xf096` to `0xf0131`. A v2 patch that previously passed now falls back
   to plain Unicode. That is the safe direction of spec §2.1's rule and was a
   deliberate call, but it is a behaviour change for v2 users and is on the
   record only here.
5. **A copy made just before `q` still dies** on a desktop with no clipboard
   manager. Honest and documented; arboard's own warning now says so out loud
   after exit, which is the one piece of new user-visible noise.
6. Nothing has been pushed to any remote.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.**
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`.
- **Agents were running when this was written.** `git worktree list` and
  `git branch -a`; read each branch's log before assuming anything is unmerged.
- Re-run the gates:
  `export CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdless-lead && touch src/lib.rs && CARGO_BUILD_JOBS=2 cargo test`
  — and read the exit code, not the tail of a pipe (§4.2).
- Every width, ratio and glyph measurement in this file and in the QA reviews
  describes a past tree. §4.16 exists because that has already cost real work.
