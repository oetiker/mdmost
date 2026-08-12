# Controller Handoff — mdmost semantic-selection

> Starter pack for the next controller session. This handoff lives in ONE
> worktree — run `git worktree list` first and confirm this is the workstream
> you're resuming. Read this first, then `git log <handoff-commit>..HEAD` for
> everything that changed since. Detail is NOT here — it's in git and the ledger
> named in §6. Before you rewrite this file at your own handoff: read the
> previous version (`git show HEAD:docs/controller-handoff.md`) and carry forward
> any lesson in §4/§5 that is still true. Fresh synthesis, not blank page. On
> merge into another branch, rewrite that branch's handoff to the merged
> reality — do not merge or preserve this text.

Handoff commit: the last commit touching this file — `git log -1 -- docs/controller-handoff.md`
Date: 2026-08-12   Reason: context budget — controller rolling over with Task 6 unstarted
Worktree / branch: `/scratch/oetiker/claude-worktrees/mdmost-semantic-selection` @ `semantic-selection`
Trunk at time of writing: `main` @ `344f4e1` — **reader: if trunk has moved, §2 is provisionally stale; if trunk now contains this branch's HEAD, this file is a tombstone** (`git merge-base --is-ancestor HEAD main`)
Sibling worktrees: the `main` checkout at `/home/oetiker/checkouts/mdmost` (owns nothing active; its handoff is superseded for this workstream), and `/scratch/oetiker/claude-worktrees/mdmost-tryout`, a **detached** throwaway used only to build owner test binaries — never commit there. This line cannot see worktrees created later; check yourself.

## 1. Mission

Make a selection a **range over the document** rather than a rectangle on screen, give
every diagram label a mapping back to its source, and give all three block kinds a muted
three-state `[copy]` button. Plan: `docs/superpowers/plans/2026-08-11-semantic-selection.md`.

The mental model that matters: **rendering is a pure function of `(AST, width, theme,
options)`**. Anything depending on the pointer — hover, the selection wash, the `[copied]`
flash — is a **paint-time** concern in `src/tui/draw.rs`, never a render-time one.

The second model, learned this session and now load-bearing: **a span's source is a
byte-for-byte copy of the cells it names.** `offset_at`, `highlighted_columns` and
`search::segments_for` all convert between bytes and columns *inside* a span by walking its
source, and that is exact only because of this property. The single sanctioned exception is
a one-grapheme, one-column run (an entity reference), because it has no interior position to
resolve. Anything that would create a wider non-copying span is a much bigger change than it
looks.

**How the owner works, and it is not optional.** They review by *looking at rendered
output*, and their findings are precise. **Answer a design question with a rendered sample,
never with prose** — build a release binary in the tryout worktree and write them a test
document. When they reframe a question, the reframe *is* the design. Eight rulings this
session came that way and every one was better than what I had drafted; one of them (CRLF,
§4.4) was better than both options I was choosing between.

## 2. Where we are now, as of the handoff commit

Re-derive rather than inherit (§8). Everything below is committed on `semantic-selection`.

| Task | Commit | State |
| --- | --- | --- |
| 1–5b (see prior handoff) | `89be3f7`…`ab79c98` | complete, reviewed |
| 5b fix round 1 | `be3b5d8`, `3a3dedb` | complete — **but round 1 shipped a defect, §4.1** |
| 5b fix round 2 | `5a8a883` | complete, both blockers fixed |
| soft-break span (owner bug) | `5361411` | complete |
| escape/entity spans (owner ruling 7) | `e330ebe` | complete |
| clickable-links **spec** | `e63cfd3` | approved by owner; **no plan written, deliberately (§4.9)** |
| CRLF normalisation (owner ruling 6) | `0a3f81f` | complete |
| 5c partial selection (owner ruling 5) | `3d639f2` | complete |

**1103 tests / 30 suites**; `cargo fmt --check`, `cargo clippy --jobs 4 --all-targets -- -D
warnings` and `cargo test --jobs 4` all exit 0 at `3d639f2`, **re-derived by the controller
on the commit, not inherited from any agent**. Across the session: zero test deletions in
any test file; every count reconciled two ways (per-suite hand-sum *and* `#[test]`
attributes).

**Task 6 is the only plan task left, and it has never been started.** Tasks 7–10 follow it.

`selection-review.html` is an untracked owner artifact from Task 3. Leave it.

## 3. Do this next

1. **Task 6 — the six remaining Mermaid families.** Its brief must carry four things the
   plan cannot know:
   - **`Label::spans_for(index, at, text)` is the shared entry point** 5c built for exactly
     this. Every family wraps its label, locates each drawn piece in its line, and emits one
     `SearchSpan` per returned run with `unit: Some((source.start, source.end))`. **Do not
     emit one span naming the whole label** — that is the shape 5c removed, and `resolve`
     now reads two such spans from two labels the same way it reads one.
   - **14 of 15 threaded parse sites have no test.** The shared `lex::label_at` makes them
     *look* covered; design §6 risk 4 warns a shared helper does not make seven families one
     behaviour.
   - **Flowchart edge labels and subgraph titles still carry no spans.** `flowchart::edge`
     and `group` flatten a `Label` into `Vec<String>` at the shared `graph` seam every
     graph-drawn family uses. Whoever fixes it should thread the `Label` through rather than
     the strings, so `spans_for` is reachable. 5c did **not** touch that seam.
   - **`Label::from_lines`** (new, for the state diagram's `note … end note`) yields a label
     with no raw text, and `spans_for` declines for it. If a state note should be selectable
     character by character, that constructor is the thing to replace, not `spans_for`.
2. **Task 7's button payload is the block's CONTENT, not its fences** — owner ruling
   amended 2026-08-12, which *retires* the "whole fenced block" reading recorded here as
   ruling 1c. All three copy buttons carry the content: the diagram button carries the
   mermaid source, and the code frame and the Mermaid-error fallback keep carrying
   `literal` exactly as they already do. Spec §4 and plan Task 7 Step 3 are therefore
   correct as written and nothing shipped changes. The plan's own test passes under
   *either* reading, so Task 7 pins the ruling in both directions
   (`a_diagram_button_carries_the_content_and_not_the_fences`) — done.
3. **Tasks 8 and 10 are owner gates**: button colours, and re-recording the demo. Stop and
   show rendered output; do not choose colours yourself.

After the plan: the **clickable-links spec** (§6), then its successor navigation spec.

## 4. Lessons & traps ← the irreplaceable part

Carried forward and still true: **give every agent its own `CARGO_TARGET_DIR`**; **never
read a gate's result through a pipe**; the clippy gate is `--all-targets -- -D warnings`
because plain `cargo clippy` exits 0 on warnings; **verify a subagent's arithmetic, not its
adjectives**; **do not choose glyphs by measuring them**; **do not hand-resolve snapshot
conflicts**; **measure box art in columns, not bytes**; **backticks inside `git commit -m`
are command-substituted — use a quoted heredoc**; **never merge into a dirty worktree**;
**tombstone every merged branch immediately**; **`git status --porcelain --ignored` before
removing a worktree** (the ledger is gitignored and dies with it); **`git merge` does not
accept `-F -`**; **put `timeout: 600000` in every dispatch prompt**, and diagnose a silent
agent by the worktree, never by the silence — *clean tree + commit* = finished silently,
*dirty tree + no build* = stalled, *dirty tree + live build* = the backgrounding bug, message
it to resume; **state process constraints as actions, not prohibitions**.

New this session, in rough order of what they cost:

1. **A fix round can fix the case named first and leave the case named second.** 5b round 1
   addressed one owner ruling and half of another, and the half it missed was the *commonest
   gesture on a diagram*. Two independent reviewers caught it. **A "fix round complete"
   claim is not a verdict — re-review it against the ruling's own acceptance criterion**, in
   the ruling's words, not the implementer's.
2. **Two reviewers reaching the same defect from opposite directions is confirmation, not
   two opinions.** A scoped re-review (does this meet the ruling?) and an adjudication (is
   this specific claim true?) found one root cause, two reachable drag shapes, the same
   responsible lines. That pairing is cheap and it is the strongest signal this loop
   produces. **Asking a reviewer to adjudicate ONE named claim, with the stakes stated and
   "verify rather than assume either party is right", keeps producing the best findings** —
   it found the entity-in-a-label population, the rhombus/cylinder span drop, and this one.
3. **Dispatch at most ONE file-writing agent per worktree.** I ran two reviewers into the
   branch worktree; one's throwaway probes made the tree dirty mid-review, and a gate run in
   that window would have read 1065 instead of 1058. The other noticed, cut its own detached
   worktree, and timestamped its numbers — and it *misattributed* the probes to an agent that
   had already finished. A shared worktree makes agents misread each other's work as their
   peer's. Read-only agents are fine; writers are not.
4. **The owner's own suggestion beat both engineering options.** CRLF: I was choosing between
   a copy-side clamp and a narrower anchor rule. They said "turn `\r\n` into `\n` on read",
   which killed three downstream rules with one boundary rule and made a fourth dead. **When
   they propose a mechanism, cost it seriously before defending yours.**
5. **Live testing found two real bugs the entire review apparatus had missed**, one of them a
   regression this branch shipped through a clean review. **Build the owner a release binary
   and a test document at every natural milestone** — tryout worktree, `cargo build
   --release`, a Markdown file that walks the new behaviour and *names what is knowingly
   broken* so they don't report it twice. This is the highest-yield thing the controller does.
6. **The suite tested one direction only, and that is how the regression shipped.** Task 3's
   review checked that chrome stays *unwashed*; nothing checked that body text stays
   *washed*. Reverting the fix across 1062 tests turned exactly ONE test red — the new one.
   **Require both directions in every brief that touches painting or spans.**
7. **A one-dimensional fixture set passes a half-done implementation.** Every prefix test
   dragged label→label, the one shape where the defect is masked. The ruling anticipated a
   half-done implementation along the *lines* axis and the tests covered that axis; nothing
   covered the *drag-shape* axis. **Ask what axes a rule can be wrong along, and name them in
   the brief.**
8. **Name the CONSTRAINT, not the file.** I briefed the escape/entity fix into
   `render/inline.rs`. It cannot live there — the alignment needs the source bytes and
   `render` has none. It went to `doc/convert.rs`, following a precedent already in the crate.
   My real constraint was "this is a rendering fact, not a `tui` one", and that was
   respected. Four agents in a row correctly overruled part of their brief with evidence;
   **brief them to treat the brief as a draft to verify, and they will**.
9. **Plans are perishable; specs are durable.** This project's plans have carried a defect in
   six of six tasks, and the plan's Tasks 6–10 are already drafts because they were written
   against code many behaviour changes ago. I deliberately wrote the clickable-links **spec**
   and no plan: the spec is behaviour and seams, the plan would name files that Task 6 is
   about to change. **Write the plan when the work is next up.**
10. **Four agents in a row self-reported a mutation that turned ZERO tests red, and chased
    it.** Every one found a real hole: an unguarded verification half, an unpinned trim rule
    that real Mermaid reaches, an entity-at-the-end case that a mid-label fixture cannot
    expose. **Put "a mutation that turns no test red is a finding about the test, not a pass"
    in every brief, and ask "if the code took the lazy shortcut here, would anything catch
    it?"** This is the single most productive sentence in the dispatch template.
11. **A failure signature that looks like a provenance bug may be a test slicing the wrong
    string.** `Doc::source()` no longer returns what the caller passed in. Three tests failed
    with spans "slid one byte left per line" and the production mapping was correct
    throughout. **If you see that signature, ask WHICH STRING is being sliced first.** ~100
    tests still slice the fixture literal; harmless while it equals the source, deliberately
    not swept.
12. **Setting a task's `owner` field re-notifies an agent that already delivered.** Set owner
    at dispatch time only.

## 5. Don'ts & constraints

Carried forward and still binding: **no HTML rendering**; **Mermaid is Unicode box art
only**; **bullets and task boxes are ASCII**; **`Esc` never quits**; **do not widen the
`NodeArt` seam**; **`render` must not depend on `tui`**; **`#![forbid(unsafe_code)]`**; **the
status bar never lies**; **no 1000-node golden snapshot**; **4-core cap on every cargo
invocation**; **there is no centring anywhere**; **`src/export/` may depend only on `doc`**;
**TSV is what every reader receives**; **the table gap-row threshold is 30 display columns**;
**the copy button follows what mouse capture actually did**; **do not push — creating the
remote is the owner's step**; **tmux: kill only your own session, and check a process's
parent with `ps -o ppid=` before killing an `mdmost`** — the owner runs this pager himself on
this machine.

Owner rulings, all binding, all superseding written docs:

1. **A diagram is atomic** — but only outside one label. Three cases: press inside a label
   and stay inside it → **the characters dragged over** (ruling 5, was "the whole label");
   press inside a label and go wider → the whole diagram; press anywhere else inside a
   diagram → the whole diagram immediately, wherever released, decided by **the anchor cell
   alone**, never by comparing the drag's rectangle to cells.
2. **The `[copy]` button's payload is the block's content, without its fences** (ruling
   amended 2026-08-12; the earlier "whole fenced block" reading is retired). A *selection*
   still yields the fenced block — the two are deliberately different.
3. **Container prefixes are stripped entirely** from a copied block — no line keeps `> `,
   fence lines included, and the prefix is *read from the document*, per line, never matched
   as a pattern.
4. **The margin beside a narrow diagram is inside it** (ruling 8). The anchor is matched by
   row, not by the atom's columns. Deliberate — do not "fix" it.
5. **Line endings are normalised at `Doc::parse`/`parse_plain`** — `\r\n` and a lone `\r`
   both become `\n`. Copy-paste always gets clean `\n`.
6. **Local `.md` links stay wholly inert** until the navigation spec lands — no hotspot, no
   reaction. A control that lights up and then declines is worse than one never offered.

Settled; do not relitigate:

- **The highlight and the clipboard are ONE computation** through a shared `Resolved` with
  private fields and a private `range()`. Two paths that merely agree today is the defect 5b
  exists to remove. A mutation test guards this.
- **Case 2 must never run `extend_over_markup`** — on a Mermaid line almost every byte is
  undrawn and the walk would swallow `A[`, the arrow and half the next box. Pinned by
  `a_drag_to_the_edge_of_a_label_stops_at_the_label`.
- **"Confined to one label" is judged on the HULL, never on the two screen positions.** A
  drag from inside a label onto the arrow beside it stays confined. Making it a crossing
  would need the screen-shaped rectangle rule §2.2 refuses.
- **A non-copying span may be one grapheme drawing one column, and nothing wider.** An emoji
  entity declines its origin and leaves one dark cell rather than hand the column walks a
  body they cannot walk. Fixing that means teaching `select`'s two column walks that a span
  may be atomic — a `tui` change with its own review.
- **`Label`'s equality deliberately ignores `source`**, so `assert_eq!` on a whole `Label`
  proves nothing about provenance. **An empty `Label::source` means "emit no span"**; do not
  invent a plausible range.
- **`carry_spans` has no structural guard.** `every_node_shape_puts_its_label_span_on_the_drawn_text`
  and its new sibling `every_node_shape_carries_every_piece_of_a_cut_label` are load-bearing —
  extend them when an eighth shape lands, never delete them.
- **`highlight.rs::strip_eol`'s `\r` half is dead and deliberately kept.** If someone removes
  it, its test must be *rewritten*, not have the failing assertion deleted.

## 6. Where the detail lives

- **Plan:** `docs/superpowers/plans/2026-08-11-semantic-selection.md` (Tasks 5b and 5c are
  not in it — owner rulings)
- **Design authority, amended repeatedly this session:**
  `docs/superpowers/specs/2026-08-11-semantic-selection-design.md` §2.2, §3, §6 risk 1
- **Next feature's spec, owner-approved, no plan yet:**
  `docs/superpowers/specs/2026-08-11-clickable-links-design.md`
- **Progress ledger — read this second, after this file:**
  `.superpowers/sdd/2026-08-11-semantic-selection/progress.md`. Gitignored, so it dies with
  the worktree. Every ruling, plan defect and deferred minor is in it.
- **Per-task reports** in the same directory: `task-5b-rereview.md`,
  `adjudication-prefix-strip.md`, `task-5b-fix2-report.md`, `diagnosis-word-separators.md`,
  `softbreak-span-report.md`, `escape-entity-spans-report.md`, `crlf-normalise-report.md`,
  `task-5c-report.md`. These are an agent's memory — a fresh implementer can resume from one.
- Key files: `src/tui/select.rs` (`resolve`'s four cases — **read its doc comment before
  touching anything**, `Resolved`, `atom_text`, `pressed_on_chrome_of`),
  `src/mermaid/ast.rs` (`Label`, `raw`, `spans_for`, `from_lines`),
  `src/mermaid/entity.rs` (`decode_runs`), `src/mermaid/layout/flowchart/shape.rs`
  (`carry_spans`, `wrap`), `src/doc/mod.rs` (`normalise_line_endings`),
  `src/doc/convert.rs` (`code_lines`, `align`, `LineOffsets`), `src/render/inline.rs`
  (`Piece::anchored` / `synthetic` / `transcribable`).

## 7. Open questions / pending decisions

1. **Owner's manual GitHub steps, still outstanding:** create `github.com/oetiker/mdmost`,
   add `CRATES_IO_TOKEN`, grant Actions write. Both CI workflows are inert until then, so
   local `cargo check --target x86_64-pc-windows-msvc` is the only Windows detector.
2. **The release workflow has never executed.** First real run is the first test.
3. **Windows compiles but has never been run.** Mouse, clipboard and alternate screen all
   unexercised; Task 9 adds motion-event handling.
4. **Button colours are deliberately unnamed** (spec §8). Task 8 renders them for the owner.
   The light theme's heading ramp is separately known to be flat and non-monotone —
   re-measure before designing.
5. **`&nbsp;` is a wrapping opportunity** and always was (U+00A0 passes
   `char::is_whitespace`, so `wrap::tokenize` breaks there), contradicting the corpus calling
   it a hard space. Flagged to the owner, not fixed, no ruling yet.
6. **Smart punctuation would break `align`.** `convert::options()` leaves `parse.smart` at
   false. If it is ever turned on, `--` → en dash is a 2-byte-to-3-byte transcription
   starting with neither `\` nor `&`, and every such paragraph fails closed to no
   provenance. Safe, but remember it before flipping that flag.
7. **comrak's sourcepos after a lone `\r` is wrong** (`11..11` in an 11-byte document). Now
   unreachable from this crate as a side effect of ruling 5. Upstream-worthy; not reported.
8. **`a_crlf_fence_with_a_blank_line_still_maps_the_lines_around_it` is not CRLF coverage**
   despite its name; it is sensitive only to a different mutation. Name kept deliberately to
   preserve the link to the defect it was written for.
9. Carried forward and still open: the banner's internal band centring was never ruled on;
   the RPM payload is unverified; nested diagrams are never widened while nested tables are;
   the demo needs re-recording once the selection work lands (plan Task 10).
10. **`BASH_DEFAULT_TIMEOUT_MS` / `BASH_MAX_TIMEOUT_MS` were NOT set** — the settings edit
    was blocked by the permission classifier. Until the owner applies it, every dispatch must
    carry `timeout: 600000` explicitly.
11. **~14 GB of orphaned `cargo-target-mdmost-*` dirs** under `/scratch/oetiker/`, plus this
    branch's own, plus `-tryout`, `-rereview5b` and `-adjudicate` created this session.
    `/scratch` was at 58% with 282G free. Ask the owner before deleting.

## 8. Staleness watch

- **Integration state must be re-derived, never inherited.** Whether this branch is merged,
  pushed or superseded is not knowable from this file: `git merge-base --is-ancestor HEAD
  main`, `git log --oneline HEAD..main`, `git branch -a --contains HEAD`. If this branch is
  merged, stop reading and go to the successor's handoff.
- **Sibling worktrees / other workstreams may exist that this file cannot name** — anything
  started after the handoff commit is invisible here.
- The 1103-test count and the three green gates are as of `3d639f2`. Re-run them; a count
  that moved without an explanation is the signal.
- **§3's Task 6 guidance is written against 5c's design and is the freshest thing here**, but
  the plan's own Task 6 text was written against code many changes ago — treat its file lists
  and sample code as drafts.
- The owner had **not yet re-tested `3d639f2`** when this was written. A binary and
  `retest2.md` were handed over; their findings are not in this file. Ask them.
