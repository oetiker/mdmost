# Controller Handoff — mdmost trunk, LaTeX math specified and planned, nothing built

> Starter pack for the next controller session. This handoff lives in ONE worktree — run
> `git worktree list` first and confirm this is the workstream you're resuming. Read this
> first, then `git log <handoff-commit>..HEAD`. Detail is NOT here — it is in git and in
> the documents named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry forward any
> lesson in §4/§5 still true. Fresh synthesis, not blank page. On merge into another
> branch, rewrite that branch's handoff to the merged reality — do not preserve this text,
> and tombstone the branch you merged.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-19   Reason: milestone — a design spec and a twelve-task plan exist; no
implementation has begun, and that is exactly the state a fresh session misreads
Worktree / branch: main checkout `/home/oetiker/checkouts/mdmost` @ `main`
Trunk at time of writing: this **is** trunk. `main` @ `ddf50b1` plus this commit, **4
ahead of `origin/main` (`a3041b2`)**. Re-derive with `git status -sb`; do not trust this
line.
Sibling worktrees: **none** as of this commit — `git worktree list` shows only the main
checkout. This line cannot see anything created later; check yourself.

## 1. Mission

`mdmost` is a full-screen terminal pager for one Markdown document — "as pleasant to look
at as btop, as pleasant to use as less". Rust + ratatui. GFM including tables with
Markdown inside cells, syntax-highlighted code, and seven Mermaid families as Unicode box
art. **v0.2.0 is released, tagged and published** (Homebrew tap, deb, rpm, musl tarballs).

**The active workstream is LaTeX math**, specified but not built. The shape of it:

- Display math is laid out in **two dimensions** on the canvas; inline math is **one row**,
  because a variable-height paragraph line would break the reflow the whole renderer rests
  on.
- `pulldown-latex` is the front end — parser, macro expander, symbol tables — taken as a
  dependency. What we write is the part nobody else can supply: layout onto integer cells.
- `src/math/` is a **sibling of `src/mermaid/`**: source in, drawn output out, no knowledge
  of `render` or `tui`.

Load-bearing mental models, all still true:

1. **Rendering is a pure function of `(AST, width, theme, options)`.** Parse once; no
   layout decision at parse time; a resize discards the canvas and renders again. Anything
   depending on the pointer — hover, the selection wash, `[copied]` — is **paint-time** in
   `src/tui/`, never render-time. `render` must not depend on `tui`. *Math adds the single
   documented exception: `MathSyntax` makes parsing depend on configuration, so that
   `math = false` means a document parses exactly as it did before math existed.*
2. **A `SearchSpan`'s source is a byte-for-byte copy of the cells it names.** A `Hotspot`
   is deliberately exempt: it claims *drawn cells*, not source bytes. *Math adds the second
   exemption, `SearchSpan::copied = false` — see §4.2, and note it turned out not to be a
   new rule at all.*
3. **Syntax comes off the source; text is decoded at the leaves.**

**How the owner works, and it is not optional.** They review by *looking at rendered
output*, and their findings are consistently precise. **Answer a design question with a
rendered sample, never with prose** — this session that took the form of ASCII previews
inside `AskUserQuestion` options, and every one of the eight design decisions was settled
in a single round. They cut through over-engineering; treat that as direction. When they
reframe a question, the reframe *is* the design. **They will challenge a claim you have
not evidenced** (§4.4).

## 2. Where we are, as of the handoff commit

Re-derive rather than inherit (§8).

**Math: designed, planned, not started.** Three commits, all documentation:

- `60cf458` — the design spec.
- `810edd2` — global macros, and the `\(…\)` delimiters, after the owner challenged an
  unevidenced claim.
- `ddf50b1` — the twelve-task stage 1 plan.

**No `src/` file has been touched. `Cargo.toml` does not yet name `pulldown-latex`.** A
fresh session reading `git log` will see three confident commits about math and find no
code; that is correct, not a lost worktree.

**Eight design decisions are settled** and recorded in the spec's §1 table: 2D display /
one-row inline; KaTeX-scale coverage; `pulldown-latex` as a dependency; Unicode scripts
only when a whole group is expressible; box-drawing delimiters; single-character big
operators; three config keys; a formula is atomic for selection. Two more were added after
the owner's challenge: macros are global to the document, and `\(…\)`/`\[…\]` are read
behind a key that defaults off.

**Everything the previous handoff listed as pending has shipped.** v0.2.0 is out; the
release workflow has now executed four times (`v0.1.0`, `v0.1.1`, `v0.1.2`, `v0.2.0`); the
18 commits it worried about were pushed long ago. **Do not act on that file's §3.**

## 3. Do this next

1. **Get the execution mode ruled.** The plan is written and the owner was asked
   subagent-driven versus inline, and answered "let's do a handoff first". That question is
   still open — ask it again before dispatching anything.
2. **Push, or ask.** Three documentation commits sit unpushed. The owner's call.
3. **Task 1 is safe to start the moment the mode is ruled.** It is pure data — the Unicode
   script tables — with no dependency on anything else in the plan, so it is a good first
   subagent and a good first review.

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
IS A WRITER** — own a verify worktree for gate runs, never an implementer's tree; **read
coordinates off the screen, never derive them**; **a missing test is only a gap if you can
name a mutation that survives without it**; **a gate is tested by the case where it does
NOT fire**; **prove a mutation makes a test FAIL, not skip**; **before discarding a dirty
tree, read the diff**; **integration state changes UNDER you, not just between sessions**;
**test, don't infer**; **verify an absence before reporting it** (a pattern that found
nothing is not proof); **reviewers finish and then end the turn without sending — chase
with `SendMessage`, never re-dispatch**; **give a reviewer the specific question, not just
the diff**.

New this session. The first three are the ones that will save real time:

1. **Read the tree before designing against it — three times the obvious mechanism was
   the wrong one.** `Origin::Transcribed` in `src/render/inline.rs` looks exactly like the
   home for inline math and is not: it is documented as one grapheme drawing one column,
   and that width rule is what keeps `select`'s column arithmetic walkable. `Atom` in
   `src/canvas/mod.rs` looks like the home for an indivisible formula and is not: it is a
   claim on *rows a block owns outright*, dropped by `Canvas::blit`, so it cannot serve
   something sharing a row with prose. And `search::segments_for` would not have panicked
   on a math span — it does `span.cols.checked_sub(offset)?` and **declines silently**,
   which is worse, because math would simply have stopped highlighting and nothing would
   have said so.
2. **Look for the existing rule before writing a new one.** The plan nearly carried "math
   needs a documented exemption from the byte-for-byte span rule". It does not.
   `2026-08-11-semantic-selection-design.md` §2.2's third case — *a drag pressed outside
   any label takes the whole diagram immediately* — already decides it, because **a formula
   is a diagram with no labels**, so every press is that case. The container-prefix rule
   came free with it. One reading of an existing spec removed a rule, an exemption and a
   paragraph of justification.
3. **The glyph inventory extends rather than exempts.** `tests/glyph_inventory.rs`
   subtracts the source's own non-ASCII characters on the principle that they are the
   document's, not mdmost's. Math fits: **an author who writes `\alpha` asked for `α` as
   surely as one who typed it.** Only the structure — fraction rule, delimiters, radical
   strokes — is ours, and it is box drawing that is mostly listed already. Had this gone
   the other way the manual would have needed a list of two thousand codepoints.
4. **The owner will challenge an unevidenced claim, and they will be right.** The spec
   said "documents do rely on defining macros once at the top". They asked "are you sure
   this is the way people are using math in markdown?" They were not sure and neither was
   I. One web search: GitHub shipped cross-block `\newcommand` and **withdrew** it, so
   documents *cannot* rely on it; what is true is that people keep asking and being
   refused, and the two renderers that have it named it (KaTeX `globalGroup`, Typora).
   **The corrected argument was stronger than the invented one.**
5. **Searching to answer the asked question found a second, unasked one.** The same search
   surfaced that `\(…\)` and `\[…\]` are how math arrives in a file somebody pasted an AI
   assistant's answer into — common enough that tools exist solely to convert it. That
   matters for a pager and not for GitHub, and it would not have been found by reasoning.
6. **The owner's ruling pattern, which is predictive.** Four choices went to the *most*
   ambitious option — KaTeX-scale coverage, global macros — and four went to the *most*
   conservative — box drawing over the Unicode bracket-piece block, single-character big
   operators, Unicode-when-complete-else-ASCII, backslash delimiters behind a default-off
   key. The rule behind it: **ambitious about capability, conservative about anything that
   can render wrong on somebody else's terminal.** Use this to predict stage 2 and 3
   rulings rather than asking twice.
7. **Verify a dependency's event payloads, not its feature list.** `pulldown-latex` has
   everything the coverage decision needed, and two absences that shaped the design and are
   invisible from the README: its events carry **no source positions** (`Content::Ordinary`
   is a `char` with no byte range), which is why a formula is atomic rather than
   per-symbol; and `MacroContext` is **private and rebuilt per `Parser`**, which is why
   global macros need a collected preamble rather than an API call.
8. **comrak already solved the currency problem.** `math_dollars` uses Pandoc's heuristics
   — no space after the opening `$`, none before the closing `$`, no digit after it — so
   `costs $5 and $10` and `$PATH is under $HOME` are not math. **Do not add a heuristic of
   our own.** The plan pins this with a test so a later change to `options()` cannot start
   eating prose quietly.
9. **Previews in `AskUserQuestion` did the work prose could not.** Eight design questions,
   eight single-round answers, each option carrying an ASCII rendering of what the reader
   would actually see. This is the same principle as "answer with a rendered sample",
   applied before any code exists to render with.

## 5. Don'ts & constraints

Carried forward and binding: **no HTML rendering**; **Mermaid is Unicode box art only**;
**bullets and task boxes are ASCII**; **Nerd Font glyphs are detected, not defaulted on**;
**`Esc` never quits**; **do not widen the `NodeArt` seam**; **`render` must not depend on
`tui`**; **`#![forbid(unsafe_code)]`**; **the status bar never lies**; **no 1000-node golden
snapshot**; **4-core cap on every cargo invocation**; **no C toolchain in the build, ever**
— it is what keeps the static musl builds working, and it is why `syntect` uses
`fancy-regex`; **leave no stray `mdmost` processes** — the owner runs this pager himself
here.

Settled; do not relitigate: **there is no centring anywhere** *(display math is the one
exception, and only when it fits — spec §7)*; **`src/export/` may depend only on `doc`**;
**the title banner is opt-in**; **`--render-once` may emit lines wider than `--width`**;
**the copy button follows what mouse capture actually did**; **only `http`/`https` become
controls**; **activation is on the RELEASE edge**; **`blit` carries hotspots, drops `Pin`
and `Atom`**; **no apt/yum repo, no GitHub Pages, no container image, no macOS
notarisation**.

Settled on math, this session — the spec's §1 table is the authority, but these are the
ones most likely to be second-guessed:

- **No symbol table of our own, ever.** `pulldown-latex` resolves symbols to characters. A
  second table would drift, and drift means a formula that renders differently here than
  everywhere else.
- **Big operators are single characters** (`∑ ∏ ∫`), not drawn multi-row art. A drawn
  operator costs four rows, is right at exactly one size, and invents a shape where a
  standard character exists.
- **Tall delimiters are box drawing with light arcs for parentheses.** The Unicode
  bracket-piece block (U+239B–U+23AD) was considered and rejected for font coverage.
- **Inline scripts are all-or-nothing per group.** `a_{bc}` stays `a_{bc}`; it never
  becomes `a_b c`, which reads as a different expression.
- **The backslash scan never rewrites the source.** Spans are subdivided, never shifted. A
  pre-pass turning `\(` into `$` would move every offset after it and break provenance,
  search and the clipboard together.
- **No `theme.math` slot in stage 1.** Half of `MathStyles`, in both built-in themes, used
  by nothing, is debt. Stage 2 adds both entries, both themes and `tests/theme_contrast.rs`
  as one change.
- **Display math is not laid out in stage 1** — it shows its framed source, which is the
  *permanent* failure path reached for a temporary reason.

## 6. Where the detail lives

- **The math authority:** `docs/superpowers/specs/2026-08-19-math-design.md`. Sixteen
  sections; §1's table is the decision record, §10 is the selection reasoning, §16 is
  global macros.
- **The math plan:** `docs/superpowers/plans/2026-08-19-math-stage-1.md`. Twelve tasks,
  each with test code, exact file paths and a commit message. Its Self-Review section lists
  the spec sections deferred to stages 2 and 3.
- **Other design authorities:** `2026-08-08-mdmost-design.md` (renderer),
  `2026-08-11-semantic-selection-design.md` (**read §2.2 before touching selection**),
  `2026-08-11-clickable-links-design.md`, `2026-08-13-documentation-design.md`
  (**still unruled since 2026-08-13**).
- **The SDD ledgers** for older plans live at `/scratch/oetiker/mdmost-ledgers/` — outside
  git by design, and **nothing backs this directory up.**
- Key files for math work: `src/render/inline.rs` (`Origin`, `Piece`, `collect`),
  `src/canvas/mod.rs:60` (`SearchSpan`) and `:225` (`Atom`), `src/search.rs:372`
  (`segments_for`), `src/render/bridge.rs` (the foreign-renderer seam),
  `src/render/diagram.rs` (the width search math will *not* need), `src/doc/convert.rs:16`
  (comrak options) and `:496` (the `NodeValue::Math` arm), `src/config.rs:577`
  (`KNOWN_KEYS`), `tests/glyph_inventory.rs`.
- **The dependency's source** is unpacked at
  `~/.cargo/registry/src/index.crates.io-*/pulldown-latex-0.8.0/`. `src/event.rs` is the
  event model; `src/parser/primitives.rs` lists every environment it supports.

## 7. Open questions / pending decisions

1. **Execution mode for the math plan is unruled** — subagent-driven or inline (§3.1).
2. **Three documentation commits are unpushed** as of this commit (§3.2).
3. **Stages 2 and 3 have no plans yet.** The spec covers them; only stage 1 is planned.
4. **The documentation design spec is still unruled**, now six days and several sessions
   old: `docs/superpowers/specs/2026-08-13-documentation-design.md`.
5. **`mdmost.mp4` is untracked in the working tree** and is new since the last handoff.
   Unknown whether it is meant to be committed, is a byproduct, or is the owner's. Ask;
   it is not yours to delete or add.
6. **`README.md~` and `demo/mdmost.toml~` are untracked editor backups.** Not yours to
   delete.
7. Carried and still open: Windows compiles but has never been run; two unguarded `Span`
   sites (`chrome::highlighted`, the status-bar breadcrumb); a language icon on the code
   frame's label (owner's idea, 2026-08-13, needs its own brainstorm); "a hotspot never
   claims a cell the canvas does not have" is held by argument at three call sites, not by
   construction; the ledger archive is unbacked; RPM payload unverified; `fc-list` untested
   on macOS; nested diagrams never widened while nested tables are; `&nbsp;` as a wrapping
   opportunity is unruled; the light theme's heading ramp is flat and non-monotone —
   re-measure before designing against it.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** `git merge-base --is-ancestor
  HEAD main`, `git log --oneline HEAD..main`, `git branch -a --contains HEAD`,
  `git status -sb`. **This file says 4 commits unpushed; that is the fastest-rotting line
  in it.**
- **Sibling worktrees created after this commit are invisible here.** `git worktree list`.
  There were none at this commit — which is itself unusual for this project and worth
  re-checking rather than assuming.
- **The previous handoff (`6c8b3b0`, 2026-08-16) is superseded and several of its
  statements are now false** — it says the release workflow has never run and no tags
  exist; there are four tags and v0.2.0 is published. Do not act on its §3.
- **The test suite was NOT run this session.** No `src/` file was touched, so the inherited
  "green" is untested but also unthreatened. Run the gates rather than quoting a number.
- **No math code exists.** If `git log` shows math commits and `src/math/` is absent, that
  is this state, not a lost worktree.
- **The plan's file-path line references were read at `ddf50b1`.** `src/search.rs:372`,
  `src/canvas/mod.rs:60`, `src/doc/convert.rs:496` and `src/config.rs:577` move whenever
  those files are edited — grep for the symbol, do not trust the line number.
- **`pulldown-latex` 0.8.0 was the current release at this commit** and was inspected at
  that version. Check for a newer one before adding the dependency; the API notes in §4.7
  are read from 0.8.0.
- **~14 GB of orphaned cargo target dirs** under `/scratch/oetiker/`. No disk pressure. Ask
  before deleting, and never `cargo sweep --stamp`/`--file` with a shared target dir.
