# Controller Handoff — mdmost, code provenance and copy buttons

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
Date: 2026-08-10   Reason: context budget, after retiring an 8-hour agent (§4.11)
Worktree / branch: `/scratch/oetiker/claude-worktrees/mdmost-provenance` @ `code-provenance`
Trunk at time of writing: `main` @ `7693b00` — **reader: if trunk has moved, §2
is provisionally stale; if trunk now contains this branch's HEAD, this file is a
tombstone** (`git merge-base --is-ancestor HEAD main`). At `7693b00`: 959 tests
across 30 suites, fmt / clippy / test / `cargo check --target
x86_64-pc-windows-msvc` all exit 0. **Re-derive anyway** (§8).
Sibling worktrees: `mdmost-publishing` @ `publishing`, `mdmost-tablezebra` @
`table-zebra`, `mdmost-entities` @ `mermaid-entities` — **all three are merged
into `main` and tombstoned; do not work in them.** This line cannot see
worktrees created later; check yourself.

## 1. Mission

`mdmost` is a full-screen terminal pager for a single Markdown document — "as
pleasant to look at as btop, as pleasant to use as less". Rust + ratatui. GFM
including tables with Markdown inside cells, syntax-highlighted code, and seven
Mermaid families drawn as Unicode box art.

The mental model that matters: **rendering is a pure function of
`(AST, width, theme, options)`**. Parse once; no layout decision at parse time;
a resize discards the canvas and renders again.

This branch is executing `docs/superpowers/plans/2026-08-10-code-provenance.md`:
give code-block cells a link back to the document source so search and copy work
inside fences, then spend that mapping on a clickable `[copy]` in code frames and
tables, the table copying a grid that Excel and Sheets paste as cells.

**How the owner works, and it is not optional.** They review by *looking at
rendered output*, and their findings are consistently precise. **Answer a design
question with a rendered sample, never with prose** — every design call this
session was settled in one exchange by rendering the options. They will cut
through analysis that has become over-engineered; treat that as direction. They
report bugs from real use, so expect renderer bug reports to arrive mid-plan.

## 2. Where we are now

**Tasks 1-5 of nine are done and reviewed.** 13 commits ahead of `main` as of the
handoff commit; re-derive (§8). At HEAD: **994 tests across 30 suites**, fmt and
clippy clean.

- Task 1 — the source-line mapping on `NodeKind::CodeBlock`, built in
  `doc::convert` by suffix-matching literal lines against the real source.
- Task 2 — one `SearchSpan` per drawn code line, so search and copy see inside a
  fence.
- Task 3 — the `Hotspot` canvas channel (`src/canvas/`), carrying clipboard
  payloads alongside `Pin`.
- Task 4 — `src/render/button.rs` and the code frame's `[copy]`.
- Task 5 — `src/export/`, a pure AST-to-string module producing TSV and HTML.

**`main` was merged into this branch at `da4e0bb`**, bringing three finished
workstreams. That matters more than it sounds:

- **Everything is left-aligned. Block centring was removed entirely** — narrow
  content stops early on the right, wide content runs past. 108 snapshots moved.
- **`src/render/table.rs` was rewritten**: the zebra stripe stops at the vertical
  rules, a cell past 30 display columns earns the table gap rows even with no
  wrapping, and inline code is a colour with no background.
- Mermaid labels decode HTML entities and mermaid's `#…;` codes; a `;` inside a
  `|…|` edge label is label text; an edge label is drawn once, not once per rank.

**A fresh agent (`provenance2`) is running Tasks 6-9** as of the handoff commit.
Task 8 modifies `render_table_node`, which the merge rewrote — **the plan's Task 8
text predates that and its line references will not match.**

## 3. Do this next

1. **Watch `provenance2` for the stall pattern in §4.1** — check
   `git log`/`git status` in its worktree every ~10 minutes. If the tree is dirty
   with no `cargo` process running, it has deadlocked: run the three gates
   yourself, send it the numbers, and tell it to commit. That converts a
   45-minute stall into two minutes and has been necessary five times.
2. **When Tasks 6-9 land: merge `code-provenance` into `main`**, rewrite `main`'s
   handoff to the merged reality, and **tombstone this branch** (§4.12).
3. **Then the demo** — publishing Task 5, the only unexecuted task of the
   publishing plan. It was deliberately deferred because it records a copy of a
   table as TSV and a fence as its source, which Tasks 6-8 deliver. The brief is
   already rewritten and committed; hand it over as-is.

## 4. Lessons & traps ← the irreplaceable part

Carried forward from the previous handoff and still true: **give every agent its
own `CARGO_TARGET_DIR`**; **never read a gate's result through a pipe**; the
standing clippy gate is `cargo clippy --all-targets -- -D warnings` because plain
`cargo clippy` exits 0 on warnings; **verify a subagent's arithmetic, not its
adjectives**; **do not choose glyphs by measuring them**; **do not hand-resolve
snapshot conflicts**; **a rename is not a `sed`** (box art and the FIGlet
fixture); **when two code paths render "the same" thing, prove it**; **flipping a
default breaks the tests that used it as a convenience**. Read
`git show 7693b00:docs/controller-handoff.md` for the full prior text.

New this session, in rough order of what they cost:

1. **The background-test deadlock, five times, ~2 hours.** An implementer starts
   `cargo test` in the background, arms a monitor or `until` loop, ends its turn
   — and the turn that would consume the result never fires. **Gates must run in
   the foreground.** The instruction only works if it is in the *implementer's
   brief*; I put it in the parent's context after the second stall and it never
   propagated, so stalls three, four and five were my briefing failure, not three
   independent agent mistakes. **Put the rule verbatim at the top of every brief
   you write.**
2. **Four vacuous tests. A test written from a *description* of the behaviour
   asserts the right property in the wrong place.** Only a test derived from an
   *observed failure* is anchored to what actually breaks. The worst example was
   doubly vacuous: its fixture used inline code where a fence was needed, and its
   assertion was a disjunction that held when nothing was drawn — guarding
   precisely the rule the design invented that seam for. **Fault injection is the
   cure and is now standard here**: revert the mechanism, confirm *that* test
   fails. Do it even when a report claims a red proof — three such claims did not
   survive the mutation they supposedly demonstrated.
3. **Measure box art in columns, not bytes — I got this wrong and sent an agent
   after a phantom.** `perl -ne 'index($_, $needle)'` on UTF-8 returns a *byte*
   offset; every box-drawing glyph is 3 bytes and 1 column, so the number runs
   ahead by 2 per glyph to the left. I reported a "28-column misalignment" that
   did not exist and told a correct test it was vacuous. **Use `perl -CSD`**, or
   count with `chars()`. The tell I ignored: the offset scaled with the amount of
   box art to the left, which is exactly what a byte/column confusion looks like.
4. **Plans have holes exactly where they say "the same way X is handled".** Task
   3's step said "implement it the same way pins are handled" while the test list
   omitted the pin's coverage; Task 1's six tests missed the empty-literal case;
   Task 2's missed tabs. **When a brief delegates a behaviour by analogy, check
   that the analogy's tests came across too.**
5. **The per-task review earned its cost on this plan and should be kept.** It
   caught: a byte-offset bug that copied wrong bytes from any tabbed code block; a
   phantom entry for empty fences; a silent *total* provenance loss on every CRLF
   document; and the vacuous tests. Every one was invisible to a green suite.
6. **Cross-agent messages get lost. Do not assume delivery.** One agent asked for
   a ruling I had already sent twice; another's reviewer woke with an empty
   context and answered "what would you like me to work on?". If an agent's
   behaviour implies it never received something, **re-send consolidated into one
   message** rather than referring back to the earlier one.
7. **Check a process's parent before killing it.** A 45-minute-old `mdmost
   README.md` looked exactly like the stray the house rules forbid. Its PPID was
   an interactive `-bash` on `pts/0` — the owner's own shell, with the pager open.
   One `ps -o ppid=` was the difference between hygiene and closing a document
   someone was reading.
8. **Prove a large snapshot churn is what you think it is.** The left-alignment
   change rewrote 108 files, 3254 insertions and 3254 deletions. Strip leading and
   trailing whitespace from both sides, sort, diff: identical means *pure
   horizontal movement*, nothing added or lost. That check takes one command and
   is the only thing standing between "moved left" and "quietly mangled".
9. **Backticks inside `git commit -m "…"` are command-substituted.** It ate a
   symbol name out of a merge message before I noticed. Use a quoted heredoc
   (`-F - <<'EOF'`) for every commit message; the whole repo's style is full of
   backticked identifiers.
10. **Never merge into a dirty worktree.** Wait for the agent to commit. Twice I
    wanted to land a port while a task was in flight and had to hold.
11. **Retire long-running agents; do not nurse them.** The Tasks 1-5 agent ran ~8
    hours, and its context held the plan, nine briefs, every report and every
    review. Every turn re-sent all of it, so a wake producing one `git status`
    cost as much input as writing a feature — and it was stalling anyway. A fresh
    agent with a 200-line brief does the same work at a fraction of the cost and
    starts with the rules already in it. **Watch for the symptom: an agent that
    looks like it is "just sitting there" burning tokens.**
12. **Tombstone every merged branch immediately** and say so in its handoff. Three
    dead worktrees are on disk right now with tombstoned handoffs; without them a
    fresh session can be launched into a corpse that answers questions
    confidently.

## 5. Don'ts & constraints

Carried forward and still binding: **no HTML rendering**; **Mermaid is Unicode box
art only**; **bullets and task boxes are ASCII and do not vary by font
detection**; **Nerd Font glyphs are detected, not defaulted on**; **`Esc` never
quits**; **do not widen the `NodeArt` seam**; **`render` must not depend on
`tui`**; **`#![forbid(unsafe_code)]`**; **the status bar never lies**; **no
1000-node golden snapshot**; **4-core cap on every cargo invocation**; **tmux:
kill only your own session**; **leave no stray `mdmost` processes** (but §4.7).

New or changed this session:

- **There is no centring anywhere.** Every block anchors at the same left margin;
  longer content sticks out to the right. The owner was asked explicitly whether
  the title banner and heading rules were exceptions and said *"no, everything
  moved left"*. Do not reintroduce a centred measure.
- **`src/export/` may depend only on `doc`** — not `canvas`, not `theme`, and
  above all not `tui`. That is what makes it exhaustively testable.
- **TSV is what every reader receives**; HTML is an upgrade where a flavoured
  clipboard exists. Nobody gets less than TSV, because OSC 52 carries only one
  flavour. Serialising the AST into a clipboard payload is **not** the "no HTML"
  rule, which is about rendering markup from a document.
- **Nothing has ever been pushed and there is no remote** *as of the handoff
  commit* — `git remote -v`. Creating `github.com/oetiker/mdmost`, adding
  `CRATES_IO_TOKEN` and setting workflow write permissions are the owner's steps.
  **Do not push without asking.**
- **The title banner is opt-in and stays opt-in.** **`--render-once` may emit
  lines wider than `--width`** where a table or diagram earns it, and the README
  must not promise exact width. Both settled; do not relitigate.
- **No apt/yum repository, no GitHub Pages site, no container image, no macOS
  notarisation.** All four considered and rejected with reasons in the publishing
  spec §1. The Pages site was dropped by the owner *after* being designed.
- **The table gap-row threshold is 30 display columns.** The owner was shown a
  28-column near-miss rendered both ways and did not ask for it to change.

## 6. Where the detail lives

- **Plan being executed:** `docs/superpowers/plans/2026-08-10-code-provenance.md`
  (Tasks 6-9 remain).
- **Design authority for it:** `docs/superpowers/specs/2026-08-10-code-provenance-design.md`
  — §4 the button, §5 the Hotspot channel and payloads.
- **Publishing plan, one task left:** `docs/superpowers/plans/2026-08-09-publishing.md`
  Task 5, the demo. Its design is §7 of
  `docs/superpowers/specs/2026-08-09-publishing-design.md` — rewritten this
  session into a five-act tmux split-screen recording at 100 columns.
- **Renderer design authority:** `docs/superpowers/specs/2026-08-08-mdmost-design.md`
- **Ledger:** `.superpowers/sdd/2026-08-10-code-provenance/progress.md` (gitignored)
- Key files: `src/doc/convert.rs` (`code_lines`, the suffix match and the CRLF
  strip), `src/render/code.rs:311` (the span byte end and its `.min(origin.end)`
  clamp), `src/canvas/ops.rs` (`merge_hotspots`), `src/export/`,
  `src/render/document.rs` (`placed`, where centring used to be),
  `src/render/table.rs` (rewritten), `src/tui/term.rs` (Tasks 7 and 9 edit it).
- Reference repo for the demo work: `~/checkouts/ansidrama`.

## 7. Open questions / pending decisions

1. **The banner's internal band centring was never ruled on.** A multi-line FIGlet
   banner still centres its bands relative to each other. The block now anchors
   left like everything else, but its internal composition was left alone
   deliberately. Worth rendering for the owner now that nothing else is centred.
2. **The RPM payload is unverified** — `rpm(1)` is not installed on this machine,
   so the `.rpm` was built and sized but never listed. Twice recorded.
3. **The release workflow has never executed.** The changelog rewrite, the formula
   marker rewrite, the deb/rpm builds and the Windows check were rehearsed
   locally; the matrix builds, crates.io publish, GitHub release and Homebrew
   commit are unexercised. The first real run is the first test.
4. **Windows compiles but has never been run.** `cargo check --target
   x86_64-pc-windows-msvc` is green and there is a CI leg for it, inert until a
   remote exists. The mouse, clipboard and alternate screen are unexercised there.
5. **The banner fixture is self-referential on this machine** — `figlet`'s `Small`
   font is not installed, so the reference art came from our own layout.
6. Carried forward and still open: `fc-list` as the macOS icon probe is untested
   there; nested diagrams are never widened while nested tables are; Nerd Fonts v2
   patches fail icon detection (deliberate); a copy made just before `q` still dies
   on a desktop with no clipboard manager.
7. **The light theme's flat heading ramp** (measured 4.80 → 4.89 → 4.92 → 4.95 →
   4.90 → 4.86:1, non-monotone) is untouched and inherited from two handoffs ago.
   **Re-measure before designing** — it may have moved under the theme changes.
   Note the *other* long-standing defect, search not matching inside fenced code,
   was fixed by Task 2 of this plan.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** Whether this branch
  is merged, pushed or superseded is not knowable from this file:
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`. If this branch is merged, stop reading and go
  to the successor's handoff.
- **Sibling worktrees / other workstreams may exist that this file cannot name** —
  anything started after the handoff commit is invisible here.
- **`provenance2` was mid-Task-6 when this was written.** Tasks 6-9 may be done,
  half-done, or stalled. `git log` in its worktree is the truth, not this file.
- The 994-test count and the clean gates are as of the handoff commit. Re-run
  them; a count that moved without an explanation is the signal.
- The "no remote, nothing pushed" claim rots the moment the owner creates the
  repository, which is step one of the publishing plan's manual steps.
