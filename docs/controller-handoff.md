# Controller Handoff — mdmost trunk, pre-public

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
Date: 2026-08-11   Reason: milestone — two plans finished, a third written and not started
Worktree / branch: main checkout `/home/oetiker/checkouts/mdmost` @ `main`
Trunk at time of writing: this **is** trunk. `main` @ `27aee88`.
Sibling worktrees: **none.** All five feature worktrees were merged and removed on
2026-08-11. One branch survives with no worktree — `worktree-code-provenance`, an
abandoned earlier run at the copy work, now holding nothing that is not on `main` (§7.6).
This line cannot see worktrees created later; check yourself.

## 1. Mission

`mdmost` is a full-screen terminal pager for a single Markdown document — "as
pleasant to look at as btop, as pleasant to use as less". Rust + ratatui. GFM
including tables with Markdown inside cells, syntax-highlighted code, and seven
Mermaid families drawn as Unicode box art.

The mental model that matters: **rendering is a pure function of
`(AST, width, theme, options)`**. Parse once; no layout decision at parse time;
a resize discards the canvas and renders again. Anything that depends on the
pointer — hover, the selection wash, the `[copied]` flash — is a **paint-time**
concern in `src/tui/draw.rs`, never a render-time one. That seam is load-bearing
and the next workstream leans on it hard.

**How the owner works, and it is not optional.** They review by *looking at
rendered output*, and their findings are consistently precise. **Answer a design
question with a rendered sample, never with prose.** They will cut through
analysis that has become over-engineered; treat that as direction. They report
bugs from real use, so expect renderer bug reports mid-plan. When they reframe a
question — "think how selection works in a web browser" — the reframe is the
design; stop defending the old framing and follow it.

## 2. Where we are now, as of the handoff commit

**Two plans are fully executed and merged.** Re-derive rather than inherit (§8).

- **`code-provenance` (nine tasks)** — code blocks carry a mapping back to the
  document source. Search and selection now see inside a fence (that was a
  long-standing defect, and it is gone); `src/export/` turns a table into TSV and
  HTML; the code frame and top-level tables offer a clickable `[copy]`; `Copied`
  replaced the `from_source` bit so the status bar names `Markdown source`,
  `rendered text`, `code` or `table`; a press is translated by the same
  `canvas_pos` a drag uses.
- **`publishing` (six tasks)** — CI, release workflow for five targets, packaging,
  man page, Homebrew formula, and now the demo: a five-act tmux recording at
  `docs/demo/mdmost.webp` (1,617,004 bytes), referenced from the README.
- **One ported fix** — `219a6dc`, a trailing-whitespace clip predicate that cost
  every such line the last character of its span (§4.5).

At `27aee88`: **1020 tests across 30 suites**, and `cargo fmt --check`, `cargo
clippy --jobs 4 --all-targets -- -D warnings`, `cargo test --jobs 4` and `cargo
check --jobs 4 --target x86_64-pc-windows-msvc` all exit 0. Verified on trunk
after the merge, not inherited from the branch.

**A third plan is written and has not been started**: semantic selection
(§6). Nothing of it is implemented.

## 3. Do this next

1. **Execute `docs/superpowers/plans/2026-08-11-semantic-selection.md`**, Task 1
   first. Ten tasks. It is TDD with mandatory fault injection and its global
   constraints block is written to be pasted into every implementer brief.
2. **Task 3 and Task 8 stop and show the owner rendered output** — the first
   selection across a table, and the three button states in every theme. Those
   are gates, not suggestions: the button colours are deliberately unnamed
   anywhere in the spec, because the owner settles them by looking.
3. **The demo needs re-recording when that lands** (plan Task 10) — it shows both
   the selection highlight and the buttons.

## 4. Lessons & traps ← the irreplaceable part

Carried forward and still true: **give every agent its own `CARGO_TARGET_DIR`**;
**never read a gate's result through a pipe**; the standing clippy gate is
`cargo clippy --all-targets -- -D warnings` because plain `cargo clippy` exits 0
on warnings; **verify a subagent's arithmetic, not its adjectives**; **do not
choose glyphs by measuring them**; **do not hand-resolve snapshot conflicts**;
**a rename is not a `sed`**; **when two code paths render "the same" thing, prove
it**; **measure box art in columns, not bytes** (every glyph is 3 bytes, 1
column; use `perl -CSD`); **prove a large snapshot churn is pure movement** by
stripping whitespace both sides, sorting and diffing; **backticks inside `git
commit -m "…"` are command-substituted — use a quoted heredoc**; **never merge
into a dirty worktree**; **tombstone every merged branch immediately**.

New this session, in rough order of what they cost:

1. **Three of four agents finished their work and went idle without reporting.**
   This is *not* the old stall — the tree was clean and the commit was made. Learn
   the two signatures, because the remedies are opposite: **dirty tree + no cargo
   process = stalled**, go run the gates and commit for it; **clean tree + commit
   present = finished silently**, go read the commit. Check before assuming
   either. A fourth agent was mid-flight when I checked (dirty tree *with* a live
   `cargo`), which is neither.
2. **`git status --porcelain` cannot see ignored files, and I destroyed the SDD
   ledgers with it.** I verified all five worktrees were clean before removing
   them; every ledger lived at `.superpowers/sdd/*/progress.md`, gitignored, and
   went with the worktrees. Nothing shipped was lost, but the record of which
   review found what, and which minors were deferred and why, is now only in
   commit messages and this file. **Use `git status --porcelain --ignored` before
   removing a worktree**, and expect gitignored working state to be exactly the
   thing worth saving.
3. **An abandoned branch can hold the only copy of a real fix.** The cleanup
   nearly deleted `worktree-code-provenance` as duplicate work. Two of its
   commits were fixes; one had been *rediscovered independently* and was already
   on trunk, the other never was and was live on `main` — a trailing-whitespace
   clip predicate that left the last character of such a line outside its own
   span, unreachable to search and to copy. It was recorded in the ledger as a
   "deferred minor", which reads like a rounding decision rather than a lost
   character. **Check by content, not by commit subject, and check every commit
   on a branch before deleting it.**
4. **Two agents cannot share one git index.** Task 7 and Task 8 ran in parallel
   with disjoint files, but a shared worktree means racing `git add`/`commit` and
   gate runs that see each other's half-finished edits. Task 8 got its own
   worktree. **Note its worktree was cut from `main`, not from the feature
   branch** — the agent noticed, merged the branch in itself and said so, which
   is the only reason it wasn't building on the wrong base. Check an isolated
   agent's base commit yourself.
5. **A test that goes *skipped* rather than red under mutation is a vacuous test
   in disguise.** The clipboard test written as its plan implied would skip when
   the alternate was dropped, because a read-back error read as "this display
   server cannot". It now probes the capability with a plain copy first, so it has
   no excuses left. **Prove the mutation makes a test FAIL, not skip.** This is a
   new species of the vacuous-test problem and the old checks do not catch it.
6. **The plan had three vacuous tests, unrunnable fixtures, and a nonexistent
   field, and agents caught all of them only because they were briefed to prove
   tests red.** `ctx.theme.table.frame` does not exist (it is `.border`); the
   plan's placement "after the clip" put the button in empty padding because
   `resize_width` pads to the viewport; and its own fixture (`| a | b |` at width
   40) negotiates a 9-column table that cannot hold `[copy]`, so its positive test
   asserted a button that correctly never appears. **A plan's test fixtures are as
   likely to be wrong as its code.**
7. **I re-ran the agents' fault injections myself and they held** — the nested
   table gate, the clipboard alternate, the clipped-off hotspot. Do this. On this
   project three earlier "verified red" claims did not survive it.
8. **Self-reviewing the plan I had just written found a whole-document
   selection bug.** `offset_at` searched only the cell's own row, so a drag inside
   a diagram — where no row carries a span — would resolve to `0..source.len()`.
   The fallbacks are now inverted from the obvious choice and the plan says so
   twice. **Review a plan against its spec before handing it over; the bug you
   find costs minutes, the one an implementer finds costs a task.**
9. **`git merge` does not accept `-F -`.** Write the message to a file. (`git
   commit -F -` does work.)
10. **`git worktree remove` is blocked by the permission classifier** and needs
    the owner's approval. Ask; do not reach for `rm -rf`, which is the workaround
    the block exists to prevent.

## 5. Don'ts & constraints

Carried forward and still binding: **no HTML rendering**; **Mermaid is Unicode box
art only**; **bullets and task boxes are ASCII and do not vary by font detection**;
**Nerd Font glyphs are detected, not defaulted on**; **`Esc` never quits**; **do
not widen the `NodeArt` seam**; **`render` must not depend on `tui`**;
**`#![forbid(unsafe_code)]`**; **the status bar never lies**; **no 1000-node golden
snapshot**; **4-core cap on every cargo invocation**; **tmux: kill only your own
session**; **leave no stray `mdmost` processes — but check a process's parent with
`ps -o ppid=` first**, because the owner runs this pager themselves on this machine.

Settled; do not relitigate:

- **There is no centring anywhere.** Every block anchors at the same left margin.
- **`src/export/` may depend only on `doc`** — not `canvas`, not `theme`, not `tui`.
- **TSV is what every reader receives**; HTML is an upgrade where a flavoured
  clipboard exists, because OSC 52 carries one flavour and is the route that
  survives SSH. Nobody ever gets less than TSV.
- **The title banner is opt-in**; **`--render-once` may emit lines wider than
  `--width`**; the README must not promise exact width.
- **The table gap-row threshold is 30 display columns.**
- **The copy button follows what mouse capture actually did**, not what the config
  asked for — a button nobody can click is worse than no button. This is why
  `--render-once` shows none, which is correct and not a bug to work around.
- **No apt/yum repository, no GitHub Pages site, no container image, no macOS
  notarisation.** All four considered and rejected; reasons in the publishing spec §1.
- **Do not push.** Creating the remote is the owner's step (§7.1).

## 6. Where the detail lives

- **Plan to execute:** `docs/superpowers/plans/2026-08-11-semantic-selection.md`
- **Its design authority:** `docs/superpowers/specs/2026-08-11-semantic-selection-design.md`
  — §2 the model, §2.1 endpoint resolution, §3 diagram provenance, §4 the button.
- **Renderer design authority:** `docs/superpowers/specs/2026-08-08-mdmost-design.md`
- Finished plans, for context on why code looks as it does:
  `docs/superpowers/plans/2026-08-10-code-provenance.md`,
  `docs/superpowers/plans/2026-08-09-publishing.md`
- **The SDD ledgers no longer exist** (§4.2). Commit messages are the record.
- **Demo:** `demo/tour.md`, `demo/mdmost.toml` (the ansidrama script),
  `demo/tmux.conf`, `demo/config.toml`; regeneration recipe in
  `docs/maintainer-notes.md`. Reference repo: `~/checkouts/ansidrama`.
- Key files: `src/tui/select.rs` (`source_hull`, `columns_on` — the geometry the
  next plan replaces), `src/tui/draw.rs:350` (`copied_flash`, the paint-time seam
  hover reuses), `src/tui/draw.rs:662` (`highlight_selection`),
  `src/mermaid/ast.rs:35` (`Label`, shared by all seven families),
  `src/render/code.rs:338` (the clip predicate, and the rebasing to imitate),
  `src/render/button.rs:35` (`place`), `src/canvas/ops.rs:127` (the `is_blank`
  clip rule).

## 7. Open questions / pending decisions

1. **The owner's manual steps, all still outstanding:** create
   `github.com/oetiker/mdmost`, add `CRATES_IO_TOKEN`, grant Actions write
   permission. Both CI workflows are inert until then, so the local
   `cargo check --target x86_64-pc-windows-msvc` is the only Windows detector.
2. **The release workflow has never executed.** The changelog rewrite, the formula
   marker rewrite and the deb/rpm builds were rehearsed locally; the matrix builds,
   crates.io publish, GitHub release and Homebrew commit are unexercised. The first
   real run is the first test.
3. **Windows compiles but has never been run.** Mouse, clipboard and alternate
   screen are all unexercised there — and the next plan adds motion-event handling.
4. **The button colours are deliberately unnamed** (spec §8). Plan Task 8 renders
   them for the owner. The light theme's heading ramp is separately known to be
   flat and non-monotone (measured 4.80 → 4.89 → 4.92 → 4.95 → 4.90 → 4.86:1);
   re-measure before designing, it may have moved.
5. **The banner's internal band centring was never ruled on.** A multi-line FIGlet
   banner still centres its bands relative to each other, though the block anchors
   left like everything else. Worth rendering now that nothing else is centred.
6. **`worktree-code-provenance` is now redundant** — both of its unique fixes are
   accounted for (§4.3). Safe to delete; left alone because it costs nothing
   without a worktree.
7. **~14.3 GB of orphaned cargo target dirs** under `/scratch/oetiker/` from the
   removed worktrees (`cargo-target-mdmost-*`). `/scratch` was at 55% with 299G
   free, so there is no pressure. Ask before deleting.
8. Carried forward and still open: the RPM payload is unverified (`rpm(1)` is not
   installed here); `fc-list` as the macOS icon probe is untested there; nested
   diagrams are never widened while nested tables are; Nerd Fonts v2 patches fail
   icon detection (deliberate); the banner fixture is self-referential because
   `figlet`'s `Small` font is not installed on this machine.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** Whether this branch
  is merged, pushed or superseded is not knowable from this file:
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`. If this branch is merged, stop reading and go
  to the successor's handoff.
- **Sibling worktrees / other workstreams may exist that this file cannot name** —
  anything started after the handoff commit is invisible here.
- The 1020-test count and the four green gates are as of the handoff commit.
  Re-run them; a count that moved without an explanation is the signal.
- **"Nothing has been pushed and there is no remote" rots the moment the owner
  creates the repository**, which is step one of their manual list. Check
  `git remote -v` rather than believing §7.1.
- The semantic-selection plan is written against the code as of `27aee88`. Its
  line references and its claim that `columns_on` still exists are true only until
  someone starts Task 3.
