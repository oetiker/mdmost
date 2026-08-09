# Controller Handoff — mdless

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
Date: 2026-08-09   Reason: context budget
Worktree / branch: main checkout (`/home/oetiker/checkouts/mdless`) @ `main`
Trunk at time of writing: `main` @ `bd87674` — **reader: if trunk has moved, §2
is provisionally stale; if trunk now contains this branch's HEAD, this file is a
tombstone** (`git merge-base --is-ancestor HEAD main`). At that commit: 924
tests / 28 suites green, `fmt --check` clean, and `clippy --all-targets --
-D warnings` clean. **Re-derive anyway** (§8).
Sibling worktrees: 33 entries under `/scratch/oetiker/claude-worktrees/`, one per
`isolation: worktree` subagent, **almost all merged and dead**. Only
`icons-autodetect` (tombstone only) showed as unmerged;
`checkbox-double-indent` was merged into `main` as `bd87674` after this file was
first written, and its worktree still holds seven untracked scratch scripts. `git worktree list` + `git branch -a --no-merged main` is the only
authority. Four dead scratch worktrees from the original build (`mdless-gate`,
`-layout`, `-qa`, `-rendercheck`) the owner chose to leave alone. **A second
controller session was live in this repo during this one** — check `pgrep -af
mdless` and for other sessions' target dirs before assuming a surprise is yours.

## 1. Mission

`mdless` is a full-screen terminal pager for a single Markdown document — "as
pleasant to look at as btop, as pleasant to use as less". Rust + ratatui.
GFM including tables with Markdown inside cells, syntax-highlighted code, and
seven Mermaid families drawn as Unicode box art. Reflows on resize, TOC pane,
search, themes, mouse.

The mental model that matters: **rendering is a pure function of
`(AST, width, theme, options)`**. Parse once; no layout decision at parse time;
a resize discards the canvas and renders again.

**How the owner works, and it is not optional.** Everything is driven through
subagents; reviewers drive the real binary in tmux rather than trusting tests;
report when it is done, do not consult on mechanics. They review by *looking at
output* and their findings are consistently precise — this session they caught a
meter background, an oversized bullet and a checkbox gap by eye, and every one
was real. **When they ask a design question, answer it with rendered samples,
not prose.** They will also cut through your analysis when it has become
over-engineered (§4.9); treat that as direction, not as a request for more
options.

## 2. Where we are now

At `06dfc3a`, four owner findings from this session are fixed and merged:

- **List bullets are ASCII `* > + -`** at depths 1-4, in *both* glyph sets —
  they deliberately no longer vary with Nerd Font detection. This ended three
  rounds of glyph analysis (§4.9).
- **The task box is ASCII `[ ]`/`[x]`**, in both glyph sets, followed by two
  spaces (`TASK_GAP`, `src/render/block.rs`). The gap belongs to the *list*: any
  list containing a task widens its marker field so plain items keep the same
  text column. Same argument as the bullets, and it deleted a whole class of
  font-width fragility with it (§7.1).
- **The status-bar meter's trough is a space in a flat colour**, not a `░`
  dither, so the part-filled cell matches it by construction (§4.13). The meter
  looks unchanged. The class re-walk also fixed a latent pie-bar background bug
  and deleted false gantt comments.
- **Search match navigation is visible and actually reaches the match.** `n`/`N`
  were always bound but undiscoverable, and worse, `step_match` revealed only
  *vertically* — on a wide table `n` moved the counter and not the screen by a
  single character. `App::reveal_columns` fixes that; the status bar now carries
  `n/N next/prev` and `or Ctrl-↓/Ctrl-↑` chips generated from the live key
  table, and `ctrl-up`/`ctrl-down` are bound as aliases.

A fifth landed after the handoff was first written, then was **superseded the same
day**. `bd87674` reserved two columns for the Nerd Font task boxes
(`Glyphs::task_cells`), because those boxes are *drawn* at twice an ASCII advance
while their private-use code points make `unicode-width` answer 1. The owner read
the result and called it: "hmmm it seems that whole business could be quite
fragile … so maybe instead of the fancy checkbox icon we should use `[ ]` and
`[x]`?" **The task box is now ASCII `[ ]`/`[x]` in both glyph sets**, and
`task_cells`, the reservation, and the parity exception it forced are all gone.
See §7.1 — the open question is closed, not merely answered.

Also: every "the shipping font is X" claim and every rasterised em-fraction
measurement is **deleted** — they were an early session's guess laundered into a
premise (§4.9). The check is
`git grep -il commitmono -- src/ tests/ README.md docs/superpowers/` , which must
stay empty; this handoff names the font on purpose, so do not grep the whole tree
and think you have found a survivor.

Integration state above is true as of the handoff commit only — **re-derive**
(§8).

## 3. Do this next

1. **Search does not match inside fenced code blocks at all.** `/word` over a
   ` ```text ` fence containing that word returns no match, while the same word
   in prose or a table cell is found. Pre-existing and apparently known (a test
   comment in `src/tui/tests.rs` says "unlike code, table cells carry search
   spans"), but for a pager aimed at code documents this reads as search being
   broken. Highest-value untouched defect.
2. **The light theme's heading ramp is flat and non-monotone** — last measured
   4.80 → 4.89 → 4.92 → 4.95 → 4.90 → 4.86:1, so it *rises* through H4. Dark
   steps correctly but every heading is dimmer than the body it introduces.
   More urgent since the prefix glyphs went: in light, H3/H4/H5 are separated by
   dash period alone. `tests/theme_contrast.rs` is where to pin it.
   **Re-measure before designing** (§4.17).

Then, in rough priority: the code palette has fewer distinct roles than names
(`code.line_number == code.operator`, `code.language == block.quote_bar`, in
both themes); remaining diagram-routing defects (edges entering a box's top
border as `┴`, a thick edge attaching at a corner, `classDiagram` leaking `*`/`$`
classifiers into return types, `stateDiagram` drawing `note right of` on the left
and duplicating an edge label); `journey` rejected as "not a diagram type" when
it is a real Mermaid family; `usability-review-2.md` findings 2-13, still
untouched and still cheap; and a widened CJK table rendering differently with and
without an active search (two agents hit it; neither settled whether it is
`highlight_matches` patching continuation cells or a tmux artifact — needs a real
terminal).

Housekeeping: 33 worktrees, nearly all dead. The owner was offered a cleanup and
has not answered. **Do not delete without asking.**

## 4. Lessons & traps ← the irreplaceable part

1. **Give every agent its own `CARGO_TARGET_DIR`.** Shared is the most expensive
   hazard here. `touch src/lib.rs && cargo build` before believing a surprise.
2. **Never read a gate's result through a pipe.** `cargo test 2>&1 | tail`
   returns tail's exit code and has hidden a red suite here.
3. **The standing clippy gate is `cargo clippy --all-targets -- -D warnings`.**
   Plain `cargo clippy` **exits 0 on warnings**, so a gate read only by exit code
   is blind to them — and "read the exit code, not a pipe" (§4.2) actively
   encourages that blindness. This bit twice in one session. First a merge landed
   a `doc_markdown` warning that four agents all truthfully reported as "clippy
   exit 0"; comparing the *warning count* to the known-clean baseline caught it.
   Then a branch cut from before that fix still carried the un-backticked text and
   **merging it would have silently reverted the fix** — its own agent sailed past
   the warning three times on exit code alone, and only found it by asking what
   the trunk commit it was missing actually did. **Gate on `-D warnings`, and when
   you are behind trunk, read what you are missing rather than trusting a green
   run on stale code.**
4. **Verify a subagent's arithmetic, not its adjectives.** Test COUNT is the
   check that catches a silently dropped test, because a test that stops running
   looks exactly like one that passes. Know the expected sum before merging
   (912 + 2 + 0 + 8 = 922 this session) and confirm `git grep -h "#\[test\]"`
   separately. A *flat* count can be legitimate — one agent renamed and rewrote a
   test rather than adding one — but it must be explained, never assumed.
5. **A green property test proves nothing about the run you didn't do.** When
   `tests/render_property.proptest-regressions` grows, commit the seed **with**
   its fix — a seed alone hands the next merge a permanently red suite.
6. **False doc comments are the most dangerous defect class.** The usual pattern
   is a comment true of one function and false of its caller. **Grep the prose
   when you change behaviour.** The worst variant is a *premise about the reader*
   rather than a claim about behaviour — see 9.
7. **Prove every behavioural test red before you fix.** Non-negotiable here.
8. **Ask reviewers to refute your diagnosis, not implement it.** Put the sentence
   "I would rather be corrected than have you implement around a wrong theory" in
   every brief where you are guessing. It keeps paying — most recently an agent
   reported a coverage finding that killed the very ladder it had been told to
   build, because the brief said to report and not resolve.
9. **Do not choose glyphs by measuring them — you do not control the reader's
   font.** Three rounds burnt. First a bullet justified by comparing em-fractions
   across *two different fonts*; corrected to same-font measurement, the owner cut
   deeper — "NO measuring is pointless ... you have no idea which font people will
   use!"; then a ladder of Unicode-named bullets died when a coverage survey found
   three of four absent from common faces. The owner ended it: "since lists are so
   important ... why play games at all ... how about we use *,>,+,-". **Coverage
   is the only legitimate font question, and the durable answer is characters that
   render everywhere.** The compounding failure: an early session *guessed* the
   owner used CommitMono, wrote it into a doc comment as "the shipping font", and
   every session after cited that comment back as authority. The owner asked "why
   do you think I use commit mono?" and there was nothing behind it. **A guess
   repeated in a doc comment becomes a false premise.**
10. **Distinguish what the terminal allots from what the font draws.** I
    escalated "the Nerd checkbox has double advance while `unicode-width` says 1"
    into a claimed accounting bug breaking wrapping, table cells, search offsets
    and drag-copy. Wrong. `unicode-width` reports 1 because PUA is
    East_Asian_Width **Ambiguous** — Unicode declines to define private-use code
    points at all — and *terminals compute from the same UAX #11 tables*, so
    mdless agrees with the terminal exactly. The real effect is narrow: the
    font's ink overflows a correctly-sized cell. **A width disagreement between
    font and table is a painting problem, not a counting problem, unless you have
    shown the terminal disagrees too.** The cure for such a problem is still a
    *layout* one — reserve an extra column so the overdraw lands on blank rather
    than on text — but understand it as deliberately diverging from the
    terminal's count to absorb ink, not as correcting a miscount. That
    distinction decides where the fix belongs and what it may safely assume.
    In this project the cure was applied and then *withdrawn* in favour of a
    glyph with no such disagreement (§7.1); prefer that when it is available.
11. **When you enumerate a class, enumerate it exhaustively.** The U+17D8 fix
    checked all 0x110000 scalars and closed the class; the bullet question was
    settled the same way (every code point named BULLET — there are 14).
12. **Fixing a class finds worse than the instance.** The raw-tab fix found ESC
    reaching the canvas. The meter fix found a latent pie-bar background bug.
13. **A partial glyph needs an explicit background — and that background must
    match what the neighbour *renders as*, not what its style says.** Four bugs,
    one shape: the zebra stripe at column rules, the help overlay's text in dark
    boxes, the meter's part-filled cell on a hole, and then that same cell given
    the track's *declared* colour when the track was a `░` dither showing only a
    quarter of it — a slab four times too heavy beside the trough it was meant to
    match. The durable fix is not to compensate but to remove the mismatch: a
    surface that a background must match should *be* a background. Grep for
    foregrounds drawn with no background, and distrust any background copied from
    a colour that reaches the screen through a glyph.
14. **An accepted trade-off should record the premise it rests on**, because a
    later change removes the premise silently. Twice now: the word-breaking
    ladder's "the counterfactual is a source dump" (killed by scrollable
    diagrams), and the meter fix's "the scrollbar's track is a thin rule on the
    page background" — that one was re-verified this session and *still holds*,
    which is exactly why writing it down was worth it.
15. **Two hostile reviewers, briefed differently and blind to each other, are
    worth far more than one.** Neither found the other's findings.
16. **A reviewer who cannot reproduce the objection they were briefed to make
    should say so** — one did, which is why LR diagram scrolling shipped.
17. **Reviews and measurements go stale within hours.** A review bisected a
    threshold at 92 columns; by the time it was designed the chart drew at 62.
    Re-derive before building.
18. **My own verification can be wrong in the same way a reviewer's is.** I
    "confirmed" the page background was never painted by reading `capture-pane -e`
    line by line. The SGR stream is *continuous across lines*, so every row after
    the first looks bare. **Parse escape streams statefully.**
19. **Merge conflicts here are usually semantic, not textual.** Several merged
    cleanly and did not compile; one hand-resolution silently dropped a `#[test]`
    attribute. **Delegate merges.** This session's three-branch merge was
    delegated with a written description of what each side wanted and landed
    clean — against the previous session's hand-merge, which burnt its context.
20. **Do not hand-resolve snapshot conflicts.** Take either side, regenerate with
    `INSTA_UPDATE=always`, read every diff, and check that *both* sides' changes
    are present — a diff showing only one side means something was lost.

## 5. Don'ts & constraints

- **No HTML.** Not rendered, not passed through.
- **Mermaid is Unicode box art only**, never raster.
- **Bullets are ASCII and do not vary by font detection.** Settled by the owner;
  do not relitigate (§4.9).
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
  as what is known ("sent, unconfirmed"), never as success. **On-screen key hints
  must come from the live key table, never hardcoded** — a rebound key must show
  the user's own binding.
- **No 1000-node golden snapshot** (spec §13.2); prefer several named fixtures.
- 4-core cap: `CARGO_BUILD_JOBS=2` on every cargo invocation.
- **tmux: kill only your own session, never `kill-server`** — an agent did once
  and may have destroyed a colleague's work mid-run.
- **Leave no stray `mdless` processes.** Check `pgrep -f mdless` and kill only
  your own; other agents and other sessions run theirs concurrently.
- **Nothing has ever been pushed to any remote.** Do not push without asking.

## 6. Where the detail lives

- **Design spec (the authority):** `docs/superpowers/specs/2026-08-08-mdless-design.md`.
- **Feature spec:** `docs/superpowers/specs/2026-08-09-wide-diagram-scrolling-design.md`
  (revision 2 — revision 1 is in git and is wrong in four ways).
- **Maintainer notes:** `docs/maintainer-notes.md` — carries the
  control-character contract and the terminal-width measurement table (which
  terminals cluster flags and ZWJ sequences correctly; tmux 3.4 does not).
- **QA:** `docs/qa/visual-review-3.md` is the best worklist; its "what is
  genuinely good" section is the list of things not to break.
- Key files: `src/render/glyphs.rs` (bullets + the glyph-choice lesson),
  `src/render/block.rs` (`TASK_GAP`, `marker_field`), `src/tui/chrome.rs`
  (`track_surface`/`TRACK_INK`, status-bar chips), `src/config/keys.rs`
  (bindings + the chord round-trip test), `src/tui/app.rs` (`reveal_columns`).
- Review and agent transcripts are subagent task outputs, not tracked files.

## 7. Open questions / pending decisions

1. **The Nerd Font checkbox width — the one thing the owner has open.** The
   patched boxes have double advance (241 vs 121 at the same em) while
   `unicode-width` reports 1, so `TASK_GAP = 2` leaves **one** visible space with
   icons and **two** with the plain `☐`/`☑`. Options put to the owner: (a) treat
   the Nerd boxes as width 2 and use a one-space gap — correct for the normal
   variant, wrong for the `Mono` variant; (b) always draw `☐`/`☑`, which are
   unambiguously single-width — the "why play games at all" answer that settled
   the bullets; (c) keep the current compensation and accept the discrepancy.
   **No runtime probe can resolve this**: Nerd Fonts ships both normal
   (double-advance) and `Mono` (single-advance) variants of the same family, and
   the deciding information lives in the font file, not in Unicode. Nothing is
   visually broken today, so it can wait. Read §4.10 before re-diagnosing.

   **CLOSED — the owner took option (b), one better.** Option (a) was built and
   merged first (`bd87674`, `Glyphs::task_cells`), and it worked, but it cost a
   hand-maintained width, a marker field that ignored its own measurement, and a
   documented exception to the parity rule that forced two long-standing tests to
   be weakened. The owner looked at that and said the business was too fragile,
   choosing not `☐`/`☑` but **ASCII `[ ]` and `[x]`** — the "why play games at
   all" answer, applied to checkboxes exactly as it was to bullets.

   Everything option (a) added is gone: no `task_cells`, no reservation, no
   exception. `task_field` measures the box again, both glyph sets draw the same
   three ASCII columns, and `icons_change_the_glyphs_but_never_the_layout` and the
   `render_property` sweep are back on the **unfiltered** corpus asserting the
   absolute. `TASK_GAP` stays 2 and now yields two *visible* spaces in both modes,
   which is what the original request asked for and had never actually got.

   The premise this once rested on — both the proportional and `Mono` faces
   measuring 241 vs 121 advance — no longer matters to anything that ships, since
   no Nerd Font glyph is involved in a task list at all. It is recorded only
   because it explains why the pictograph was a losing position: **a glyph whose
   drawn width and measured width disagree costs more than it is worth.**

   *Detection consequence, and the one thing to check if this area moves again:*
   the task box used to be the renderer's representative in the coverage probe.
   The renderer is now represented by the code-fence language icons alone
   (`0xe7a8` in `nerdfont`'s test), because they are the only thing it draws that
   icons still change. A Nerd Fonts v2 patch still fails detection, but *solely*
   because the chrome's file marker is five-digit Material — pinned deliberately
   by an added assertion rather than left to follow from something that moves.
2. **`fc-list` as the macOS icon probe** is untested there; detection falls back
   to plain, which is safe but pessimistic.
3. **Nested diagrams are never widened** (list, blockquote) while nested tables
   are — the same fence behaves differently indented two spaces. Recorded in the
   feature spec's "Out of scope".
4. **Nerd Fonts v2 patches fail icon detection.** The task boxes moved to Material
   Design code points, so `nerdfont.rs`'s probe moved from `0xf096` to `0xf0131`.
   A v2 patch that previously passed now falls back to plain Unicode — the safe
   direction of spec §2.1, a deliberate call, on the record only here.
5. **A copy made just before `q` still dies** on a desktop with no clipboard
   manager. Honest and documented; arboard's warning now says so after exit.
6. **33 worktrees await a cleanup decision.** Offered, not yet answered.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** Whether this branch
  is merged, pushed or superseded is not knowable from this file:
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`. If this branch is merged, stop reading and go
  to the successor's handoff.
- **Sibling worktrees / other workstreams may exist that this file cannot name** —
  anything started after the handoff commit is invisible here. A *second
  controller session* was active in this repo while this was written; assume
  concurrency rather than sole ownership.
- Re-run the gates. Clippy MUST be run as `-- -D warnings` (§4.3); a bare
  `cargo clippy` exit code is blind to the warnings that have twice slipped
  through here:
  `export CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdless-lead && touch src/lib.rs && CARGO_BUILD_JOBS=2 cargo test && CARGO_BUILD_JOBS=2 cargo clippy --all-targets -- -D warnings && cargo fmt --check`
  — and read the exit code, not the tail of a pipe (§4.2). Expect 924 tests and
  913 `#[test]` attributes at the handoff commit.
- Every width, ratio and contrast measurement in this file and in the QA reviews
  describes a past tree. §4.17 exists because that has already cost real work.
- The §3 backlog below the top three is inherited from the previous handoff and
  was only partly re-verified this session. Confirm a defect still reproduces
  before fixing it.
