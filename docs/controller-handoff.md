# Controller Handoff — mdmost semantic-selection

> Starter pack for the next controller session. This handoff lives in ONE worktree —
> run `git worktree list` first and confirm this is the workstream you're resuming.
> Read this first, then `git log <handoff-commit>..HEAD` for everything that changed
> since. Detail is NOT here — it's in git and the ledger named in §6. Before you rewrite
> this file at your own handoff: read the previous version
> (`git show HEAD:docs/controller-handoff.md`) and carry forward any lesson in §4/§5 that
> is still true. Fresh synthesis, not blank page. On merge into another branch, rewrite
> that branch's handoff to the merged reality — do not merge or preserve this text, and
> tombstone this one.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-12   Reason: milestone — the plan is complete except Task 10
Worktree / branch: `/scratch/oetiker/claude-worktrees/mdmost-semantic-selection` @ `semantic-selection`
Trunk at time of writing: `main` @ `344f4e1` — **reader: if trunk has moved, §2 is provisionally stale; if trunk now contains this branch's HEAD, this file is a tombstone** (`git merge-base --is-ancestor HEAD main`)
Sibling worktrees: the `main` checkout at `/home/oetiker/checkouts/mdmost` (owns nothing active; **its handoff is two plans out of date and says this plan "has not been started"** — do not believe it), and `/scratch/oetiker/claude-worktrees/mdmost-tryout`, a **detached** worktree holding the owner's test binary, currently at `446b66e`. Never commit in tryout. This line cannot see worktrees created later; check yourself.

## 1. Mission

`mdmost` is a full-screen terminal pager for a single Markdown document — "as pleasant to
look at as btop, as pleasant to use as less". Rust + ratatui. This branch makes a
**selection a range over the document rather than a rectangle on screen**, gives every
diagram label a mapping back to its source, and gives all three block kinds a clickable
`[copy]` with a hover state.

Three mental models, all load-bearing:

1. **Rendering is a pure function of `(AST, width, theme, options)`.** Anything depending
   on the pointer — hover, the selection wash, the `[copied]` flash — is a **paint-time**
   concern in `src/tui/draw.rs`, never a render-time one.
2. **A span's source is a byte-for-byte copy of the cells it names.** `offset_at`,
   `highlighted_columns` and `search::segments_for` all convert between bytes and columns
   *inside* a span by walking its source, and that is exact only because of this. The one
   sanctioned exception is a one-grapheme, one-column entity run. Anything creating a
   wider non-copying span is a much bigger change than it looks.
3. **Syntax comes off the source; text is decoded at the leaves.** A decoded character
   must never become syntax — not a delimiter (`&#40;` is not an open paren), not a
   visibility marker (`&#126;` is not `~`). Two commits enforce this from opposite ends.

**How the owner works, and it is not optional.** They review by *looking at rendered
output*, and their findings are precise. **Answer a design question with a rendered
sample, never with prose** — build a release binary in the tryout worktree and write them
a page. When they reframe a question, the reframe *is* the design: ten rulings this
session came that way, and two of them reversed written premises outright (§5).

## 2. Where we are now, as of the handoff commit

Re-derive rather than inherit (§8). Everything below is committed on `semantic-selection`.

**Plan Tasks 1–9 are complete. Only Task 10 remains.** Five further pieces landed
off-plan, three of them bugs nobody knew existed.

| Work | Commit | Note |
| --- | --- | --- |
| Tasks 1–5c | `89be3f7`…`3d639f2` | see the previous handoff |
| Task 6 — six remaining families | `ee59167` | |
| Task 6b — the `graph` seam | `4af6d9d` | off-plan; edge and frame labels |
| Task 7 — the diagram's `[copy]` | `c65507a` | |
| The semicolon bug | `0d65425` | off-plan; **drew a phantom node** |
| Class member decoding | `6433ad1` | off-plan |
| Task 8d — one wrap, not two | `5524f06` | off-plan; pure cleanup |
| The `~` visibility bug | `d9d3884` | off-plan; **reversed generics** |
| Tasks 8 + 9 — hover and its shift | `446b66e` | one task, per owner ruling |

At `446b66e`: **1175 tests across 31 suites**, with `cargo fmt --check`, `cargo clippy
--jobs 4 --all-targets -- -D warnings`, `cargo test --jobs 4` and `cargo check --jobs 4
--target x86_64-pc-windows-msvc` all exit 0. **Every one of these commits had its gates
re-derived by the controller on the commit**, its arithmetic reconciled two ways, and at
least one load-bearing mutation re-run by the controller rather than taken on the
implementer's word. Zero test deletions in the entire session.

**The three-state button is finished and owner-approved**: rest, hovered (`HOVER_SHIFT =
0.4` toward the theme's own ink), `[copied]`.

## 3. Do this next

1. **Task 10 — re-record the demo.** The only plan task left, and the owner has it flagged
   as their gate. Recipe in `docs/maintainer-notes.md` §"Regenerating the demo". **Do the
   plan's Step 2 before recording**: `demo/tour.md` is written to the widths act 2's drag
   passes through (two-line cells through 59 columns, single-line from 60). Nothing this
   session should have moved them, which is exactly why it is worth confirming. Step 4 is
   "watch it" — the act 4 selection should now hug the text rather than the row.
2. **Then settle the branch's fate with the owner.** This is nine commits of finished work
   on a branch with no remote. Merging into `main` means rewriting `main`'s handoff to the
   merged reality *and* tombstoning this one — both halves, or a fresh session gets
   launched into a corpse that answers questions confidently.
3. **The `clickable-links` spec is the next feature**, owner-approved, no plan written —
   deliberately, because plans rot (§4.6). Write the plan when the work is next up.

## 4. Lessons & traps ← the irreplaceable part

Carried forward and still true: **give every agent its own `CARGO_TARGET_DIR`**; **never
read a gate's result through a pipe**; the clippy gate is `--all-targets -- -D warnings`
because plain `cargo clippy` exits 0 on warnings; **verify a subagent's arithmetic, not
its adjectives**; **do not choose glyphs by measuring them**; **do not hand-resolve
snapshot conflicts**; **a rename is not a `sed`**; **when two code paths render "the
same" thing, prove it**; **measure box art in columns, not bytes** (every glyph is 3
bytes, 1 column; `perl -CSD`); **backticks inside `git commit -m` are
command-substituted — use a quoted heredoc**; **`git merge` does not accept `-F -`**;
**never merge into a dirty worktree**; **`git status --porcelain --ignored` before
removing a worktree** (the ledger is gitignored and dies with it); **put `timeout:
600000` in every dispatch**; **diagnose a silent agent by the worktree, never by the
silence** — *clean tree + commit* = finished silently, *dirty tree + no build* = stalled,
*dirty tree + live build* = the backgrounding bug, message it to resume; **state process
constraints as actions, not prohibitions**; **dispatch at most ONE file-writing agent per
worktree**.

New this session, in rough order of what they are worth:

1. **The verification loop that worked, and it is cheap.** For every commit: re-derive all
   four gates *on the commit*; reconcile the test count **two ways** (per-suite hand-sum
   and `#[test]` attributes — the constant 11 between them is the doc-test suite, which
   has no attributes); and **re-run one load-bearing mutation yourself**, choosing the
   claim most likely to be overstated rather than the first one listed. Ten commits, and
   it caught nothing fraudulent — which is itself the finding, and it cost minutes.
2. **For anything visual, RENDER IT.** Tests are not the check; the owner sees pixels.
   `cargo build && --render-once` showed the semicolon bug drew a *phantom node* (the
   report said "fails to parse"), proved the `~` bug reversed `Map~K,V~` brackets (not in
   the report at all), and confirmed all four fixes. Two minutes each.
3. **I named the wrong file three times, and agents overruled me with evidence every
   time.** The copy button belongs in `code::diagram_block`, not `render/diagram.rs`
   (which serves only the width search, so the plan's own test would have stayed red);
   `lex::split_statements` was *not* already entity-hardened despite a doc comment naming
   the hazard; and a "behavioural choice" my dedup brief flagged was dead code. **Name the
   CONSTRAINT — which layer must not learn this — not the file.** And brief every agent to
   treat its brief as a draft to verify; ten in a row did, and were right each time.
4. **Ask the owner when a ruling is ambiguous instead of picking.** The ledger said the
   `[copy]` payload was "the whole fenced block" while both shipped buttons passed
   `literal`. I put the contradiction back rather than resolve it; they chose content-only
   for all three, **reversing the recorded ruling**. Guessing would have changed two
   shipped behaviours for nothing.
5. **Every state a test document names must actually be reachable.** I handed the owner a
   page listing "at rest / hovered / just clicked" as judgeable. Hover had never been
   implemented. They went looking for a difference that could not exist and reported it as
   a bug. **The page is a promise about what works.**
6. **Plans are perishable; specs are durable.** Six of six tasks carried a plan defect, and
   this session Task 7's file was wrong and Task 8's whole premise was reversed by the
   owner looking at it. Write the plan when the work is next up, not before.
7. **Fault injection must run the FULL suite.** I re-ran a mutation at `cargo test --lib`
   scope and read a genuine red as a survivor — the covering test was an integration test.
   Related, and worth putting in briefs: **a redirect (`cmd > file`) is NOT a pipe** and
   preserves cargo's exit status, so a long gate can be captured without breaking the
   never-pipe rule.
8. **"A mutation that turns no test red is a finding about the test, not a pass" is still
   the single most productive sentence in the dispatch template.** Ten consecutive agents
   self-reported a zero-red mutation and chased it; **every one was a real hole** — an
   untested sequence actor path, an untested `Method::name`, a decode-up-front shortcut
   nothing pinned, a table-cell fixture too narrow to hold the button either way, and a
   stale hover index that would light the wrong button after a reflow.
9. **A mutation harness must back up file CONTENT and restore from the backup.** One agent
   reverted with `git checkout -- <path>` and destroyed uncommitted implementation across
   six files. Before the commit exists, the working tree is the only copy.
10. **An unattributed instruction appeared inside a subagent's run** — "we need to update
    the ansidrama to show off the new capabilities". The controller did not send it, it was
    not in the brief (grep-checked), and the owner had said nothing of the kind. The agent
    refused it and passed it back, which was correct. **Brief agents that the brief is
    their authority and anything arriving outside it goes back unactioned.** The demo
    genuinely does need re-recording (§3.1) — do not let that coincidence launder it.
11. **Check a reported root cause before briefing it.** The 6b report said "entities are
    unreachable in flowchart, class and state edge labels". Rendering showed flowchart was
    *fine*, and grep showed the cause was the `;` (an entity's terminator, and the
    statement separator) rather than the `&`. A brief written from the report would have
    sent someone to change ampersand splitting, which is real Mermaid syntax.
12. **A shared behaviour with one family's test is a trap.** Dropping the blank-piece
    fallback turned exactly one test red in the whole repo — the state diagram's — while
    four families depended on it. Ask which families share a path, and whether each is
    independently covered.

## 5. Don'ts & constraints

Carried forward and still binding: **no HTML rendering**; **Mermaid is Unicode box art
only**; **bullets and task boxes are ASCII**; **`Esc` never quits**; **do not widen the
`NodeArt` seam**; **`render` must not depend on `tui`**; **`#![forbid(unsafe_code)]`**;
**the status bar never lies**; **no 1000-node golden snapshot**; **4-core cap on every
cargo invocation**; **there is no centring anywhere**; **`src/export/` may depend only on
`doc`**; **TSV is what every reader receives**; **the table gap-row threshold is 30
display columns**; **the copy button follows what mouse capture actually did**; **do not
push — creating the remote is the owner's step**; **tmux: kill only your own session, and
check a process's parent with `ps -o ppid=` before killing an `mdmost`** — the owner runs
this pager himself on this machine.

Owner rulings, all binding, all superseding written docs:

1. **A diagram is atomic** — but only outside one label. Press inside a label and stay
   inside it → the characters dragged over; press inside and go wider → the whole diagram;
   press anywhere else → the whole diagram, decided by **the anchor cell alone**.
2. **The `[copy]` button's payload is the block's content, without its fences** (ruling
   amended 2026-08-12; the earlier "whole fenced block" reading is retired, and all three
   buttons follow this). **A *selection* still yields the fenced block — the two are
   deliberately different.** Do not "fix" the code buttons toward fences.
3. **Container prefixes are stripped entirely** from a copied block — no line keeps `> `,
   fence lines included, and the prefix is *read from the document*, per line, never
   matched as a pattern.
4. **The margin beside a narrow diagram is inside it.** The anchor is matched by row, not
   by the atom's columns. Deliberate — do not "fix" it.
5. **Line endings are normalised at `Doc::parse`/`parse_plain`** — `\r\n` and a lone `\r`
   both become `\n`.
6. **Local `.md` links stay wholly inert** until the navigation spec lands.
7. **The button's resting colour is correct as it stands** (2026-08-12). This **reversed**
   plan Task 8 Step 1, which said "do not reuse the frame colour — that is the thing being
   fixed". The owner looked at it and it was fine.
8. **`HOVER_SHIFT = 0.4`**, blending the button's foreground toward `Palette::fg`
   (2026-08-12). Settled; do not retune without asking.

Settled; do not relitigate:

- **The highlight and the clipboard are ONE computation** through a shared `Resolved`.
- **Case 2 must never run `extend_over_markup`** — on a Mermaid line almost every byte is
  undrawn and the walk would swallow `A[`, the arrow and half the next box.
- **"Confined to one label" is judged on the HULL**, never on the two screen positions.
- **`Label`'s equality deliberately ignores `source`**, so `assert_eq!` on a whole `Label`
  proves nothing about provenance. **An empty `Label::source` means "emit no span"**; do
  not invent a plausible range.
- **`carry_spans` has no structural guard.** `every_node_shape_puts_its_label_span_on_the_drawn_text`
  and `every_node_shape_carries_every_piece_of_a_cut_label` are load-bearing — extend them
  when an eighth shape lands, never delete them.
- **`highlight.rs::strip_eol`'s `\r` half is dead and deliberately kept.** If someone
  removes it, its test must be *rewritten*, not have the failing assertion deleted.
- **Hover paints BEFORE `copied_flash`**, so `[copied]` wins the cells in the resting
  colour. The pointer is necessarily over the button when it is clicked.
- **The empty-label fallback must NOT be folded into `chrome::label_pieces`** —
  `DrawnLabel::is_empty` uses "no pieces" to mean "this edge draws no label row", so
  folding it in would give every unlabelled edge a reserved blank row. It lives in
  `chrome::label_pieces_or_blank`.

## 6. Where the detail lives

- **Change history:** `git log 7cf0dcf..HEAD` covers this entire session.
- **Plan:** `docs/superpowers/plans/2026-08-11-semantic-selection.md`. **Tasks 5b, 5c, 6b
  and 8b–8e are not in it** (owner rulings and off-plan bugs), and its Tasks 7–9 were
  superseded in flight — the ledger is the record, not the plan.
- **Design authority:** `docs/superpowers/specs/2026-08-11-semantic-selection-design.md`
- **Next feature's spec, owner-approved, no plan yet:**
  `docs/superpowers/specs/2026-08-11-clickable-links-design.md`
- **Progress ledger — read this second, after this file:**
  `.superpowers/sdd/2026-08-11-semantic-selection/progress.md`. **Gitignored, so it dies
  with the worktree.** Every ruling, plan defect and deferred item is there, including
  ~40 lines added this session.
- **Per-task briefs and reports** in that same directory (`task-6-*`, `task-6b-*`,
  `task-7-*`, `task-8b-*` … `task-9-*`). An agent's memory; a fresh implementer can resume
  from one.
- **The owner's test page:** `/scratch/oetiker/claude-worktrees/mdmost-tryout/buttons.md`,
  with a release binary at `/scratch/oetiker/cargo-target-mdmost-tryout/release/mdmost`.
  **Run it with `--mouse` or there are no buttons at all.**
- Key files: `src/tui/select.rs` (`resolve`'s four cases — **read its doc comment before
  touching anything**), `src/tui/draw.rs` (`copied_flash`, `hover_highlight` — the
  paint-time seam), `src/tui/app.rs:1740` (`set_pointer`, and why its return value
  matters), `src/mermaid/chrome.rs` (`label_pieces`, `label_pieces_or_blank`,
  `label_spans`, `label_rows` — every family's wrap goes through these now),
  `src/mermaid/ast.rs` (`Label`, `spans_for`, `from_lines`), `src/mermaid/entity.rs`
  (`decode_runs`, `reference_len` — the splitter and the decoder share this so they cannot
  disagree about where a reference ends), `src/mermaid/parse/class.rs`
  (`split_visibility`, `decoded`), `src/theme/style.rs:63` (`blend`, `luminance`).

## 7. Open questions / pending decisions

1. **Owner's manual GitHub steps, still outstanding:** create `github.com/oetiker/mdmost`,
   add `CRATES_IO_TOKEN`, grant Actions write. Both CI workflows are inert until then, so
   local `cargo check --target x86_64-pc-windows-msvc` is the only Windows detector.
2. **The release workflow has never executed.** First real run is the first test.
3. **Windows compiles but has never been run.** Mouse, clipboard and alternate screen all
   unexercised — and this session added motion-event handling to that list.
4. **The owner has not retested since `446b66e`** beyond hover. A binary and `buttons.md`
   are in the tryout worktree; their findings are not in this file. Ask.
5. **Provenance deliberately missing, all commented in code**, and **not accepted as
   permanent** — logged and deferred: `<br>`-broken sequence message labels; sequence block
   captions (`loop`/`alt`); generic class names (`Square~Shape~` draws `Square<Shape>`,
   which no stretch of the source spells); class members and ER attributes; class
   cardinalities; chart and section titles; a state key used as its own label; `note … end
   note`. The first two are ordinary things to write and are the best candidates.
6. **Hover does not clear when the pointer leaves the window** — terminals send no such
   event, so the last button stays lit until the pointer returns. Everything inside the
   window clears correctly. Told to the owner; not ruled on.
7. **The sequence note and participant head have no empty-label guard**, the same hole
   Task 8d found and closed for the flowchart. Small task.
8. **`&nbsp;` is a wrapping opportunity** and always was (U+00A0 passes
   `char::is_whitespace`), contradicting the corpus calling it a hard space. Flagged, not
   fixed, no ruling.
9. **Smart punctuation would break `align`.** `convert::options()` leaves `parse.smart`
   false; remember this before flipping it.
10. **~14 GB of orphaned `cargo-target-mdmost-*` dirs** under `/scratch/oetiker/`. Ask the
    owner before deleting; there is no disk pressure.
11. Carried forward and still open: the banner's internal band centring was never ruled on;
    the RPM payload is unverified; nested diagrams are never widened while nested tables
    are; comrak's sourcepos after a lone `\r` is wrong, now unreachable from this crate.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** Whether this branch is merged,
  pushed or superseded is not knowable from this file:
  `git merge-base --is-ancestor HEAD main`, `git log --oneline HEAD..main`,
  `git branch -a --contains HEAD`. If this branch is merged, stop reading and go to the
  successor's handoff.
- **Sibling worktrees / other workstreams may exist that this file cannot name** — anything
  started after the handoff commit is invisible here.
- **`main`'s handoff is actively misleading, not merely stale**: it says the
  semantic-selection plan "has not been started", and describes trunk as of `344f4e1`.
- The 1175-test count and the four green gates are as of `446b66e`. Re-run them; a count
  that moved without an explanation is the signal.
- **"There is no remote" rots the moment the owner creates the repository**, which is step
  one of their manual list. Check `git remote -v` rather than believing §7.1.
- The plan's line references were written against code many behaviour changes ago, and this
  session changed `EdgeSpec`, `GroupSpec`, `chrome`, `flowchart::shape`, `parse/class.rs`
  and `parse/lex.rs`. Treat every file and line it names as a draft.
