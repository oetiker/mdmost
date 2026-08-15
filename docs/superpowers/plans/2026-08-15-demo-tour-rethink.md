# Demo Tour Re-think Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-stage the mdmost demo tour onto ansidrama 0.4.0 so that a recording run which completes is a run in which every beat that could fail silently has been proven to have happened.

**Architecture:** The tour is a TOML scene script (`demo/mdmost.toml`) driving tmux in an embedded terminal. The old recorder paced itself by "wait for the terminal to go quiet"; the new one samples continuously and offers `await` — a scene declares the text its finished screen contains and the recorder waits for exactly that, aborting if it never appears. The work is: remove the one key that no longer parses, replace three raw wire-format escape sequences with the `move` scene that now exists, add `await` to every beat whose failure would otherwise be invisible, and restore the theme beat that was cut. Same seven acts, same story.

**Tech Stack:** ansidrama ≥ 0.4.0 (Rust, `record` subcommand), tmux, TOML, mdmost's own release binary.

**Spec:** `docs/superpowers/specs/2026-08-15-demo-tour-rethink-design.md`

## Global Constraints

- **ansidrama must be ≥ 0.4.0.** `move` and the reporting pointer do not exist before it. The binary currently on disk is a 0.3.0-era build (Aug 14) and Task 1 replaces it.
- **`settle_ms` and `react_ms` are gone.** A config carrying either fails to parse. Neither may be reintroduced anywhere.
- **Every cargo invocation is capped at 4 cores** (`-j 4`). This machine has 128 cores and is shared.
- **Coordinates are read off a rendered screen, never derived or adjusted by arithmetic.** A click that lands on nothing is silent.
- **An `await` pattern must be text that only the finished screen contains.** If the pattern also appears somewhere innocent — the document body, an earlier frame — it is rejected and a different pattern or a row scope is chosen. `row = -1` reaches the status bar.
- **tmux: use only the private socket `-L mdmost-demo`** (and `-L probe` for probes). Never touch a default-socket server; the owner is sitting in one.
- **Leave no stray `mdmost` processes** — but check a process's parent with `ps -o ppid=` before killing anything. The owner runs this pager himself on this machine.
- **All scratch output goes to `/scratch/oetiker/claude-tmp/`**, never `/tmp`.
- **Never delete a file you did not create.** `demo/mdmost.toml~` is the owner's editor backup and is not yours.
- **The hero image and the closing frame must both be dark.**

---

## File Structure

| File | Responsibility |
| --- | --- |
| `demo/mdmost.toml` | the scene script — the only file whose behaviour changes |
| `demo/tour.md` | the document being toured; one sentence changes (Task 3) |
| `docs/maintainer-notes.md` | the regeneration recipe and the recorder's timing model (Task 9) |
| `docs/demo/mdmost.webp` | the committed artifact (Task 10) |
| `demo/mdmost.trunc.toml` | **temporary**, created and removed within a task; a truncated copy used to record one act without paying for the whole tour |

### On testing, in a project with no test suite for this

There is no unit test for a recording. The `await` **is** the test: it asserts what the screen must say, and the run aborts if it does not. So the TDD cycle here is:

1. Add the `await` with the pattern you believe is right.
2. **Prove it can fail** — change the pattern to a string that cannot appear, run, and confirm the run aborts naming that pattern.
3. Restore the real pattern and confirm the run completes.

Step 2 is not ceremony. This project's own history has three separate cases of an assertion that passed for the wrong reason, and a gate that has quietly stopped gating looks identical to one that works if you only ever test the case where it passes.

**Cost cap, stated rather than hidden:** a full recording takes about five minutes. Fault injection is therefore done **once per act**, on that act's most load-bearing `await`, not on all ~18. The remaining awaits are verified only by the positive run. That is a deliberate reduction in coverage for time, and if any act later behaves oddly, its unfaulted awaits are the first suspects.

### The truncated-script technique, used from Task 4 onward

Recording only the acts you changed:

```bash
cp demo/mdmost.toml demo/mdmost.trunc.toml
# delete the [[scene]] blocks after the act under test, keeping everything before it
/home/oetiker/scratch/cargo-target/release/ansidrama record demo/mdmost.trunc.toml \
  -o /scratch/oetiker/claude-tmp/tour-trunc.webp \
  --dump-png /scratch/oetiker/claude-tmp/trunc-png
```

Two things make this work and are easy to get wrong:

- **Run from the repository root.** `launch` refers to `demo/tmux.conf`, `demo/config.toml` and `demo/tour.md` by repo-relative path.
- **Always pass `-o`.** The config's `out` key resolves relative to the config file's own directory, so a truncated copy left in `demo/` would otherwise overwrite the real `docs/demo/mdmost.webp`.

---

## Task 1: A recorder that has the machinery this plan assumes

**Files:**
- Modify: none (build only)

**Interfaces:**
- Produces: `/home/oetiker/scratch/cargo-target/release/ansidrama` at v0.4.0, and a current `mdmost` release binary at `$CARGO_TARGET_DIR/release/mdmost`. Every later task invokes both by these paths.

- [ ] **Step 1: Confirm the binary on disk is too old**

```bash
ls -la /home/oetiker/scratch/cargo-target/release/ansidrama
```

Expected: a timestamp of Aug 14 or earlier. v0.4.0 was released Aug 15, so this build predates `move`.

- [ ] **Step 2: Bring the ansidrama checkout to the released version**

The local checkout sits on the merged feature branch, not on main.

```bash
git -C /home/oetiker/checkouts/ansidrama status --porcelain
git -C /home/oetiker/checkouts/ansidrama fetch
git -C /home/oetiker/checkouts/ansidrama log --oneline -1 origin/main
```

Expected: a clean tree, and `origin/main` at `1963bf2 Release v0.4.0`.

**If the tree is not clean, stop and ask.** It is not yours to discard, and an abandoned branch in this project once held the only copy of a real fix.

```bash
git -C /home/oetiker/checkouts/ansidrama checkout main
git -C /home/oetiker/checkouts/ansidrama merge --ff-only origin/main
```

- [ ] **Step 3: Build it**

```bash
cd /home/oetiker/checkouts/ansidrama && cargo build --release -j 4
```

Run this in the foreground, pass `timeout: 600000`, and wait for it in the same turn. Do not end your turn while the build is running — the shell is killed with the turn and the result is lost.

- [ ] **Step 4: Verify the version has the new machinery**

```bash
grep -c '^## 0.4.0' /home/oetiker/checkouts/ansidrama/CHANGES.md
grep -n 'fn.*move\|"move"' /home/oetiker/checkouts/ansidrama/src/*.rs | head
```

Expected: the changelog has a 0.4.0 section, and the source contains a `move` scene action. The `--version` flag does not exist on this tool, so the source is the check.

- [ ] **Step 5: Build mdmost's release binary**

```bash
cd /home/oetiker/checkouts/mdmost && cargo build --release -j 4
```

Same rules: foreground, `timeout: 600000`, wait in the same turn.

- [ ] **Step 6: No commit**

This task changes no tracked file. Nothing to commit.

---

## Task 2: The baseline — delete the blocker and change nothing else

**Files:**
- Modify: `demo/mdmost.toml:23`

**Interfaces:**
- Consumes: the binaries from Task 1.
- Produces: a recorded baseline at `/scratch/oetiker/claude-tmp/baseline.webp` with frames dumped to `/scratch/oetiker/claude-tmp/baseline-png/`, and a written finding on whether the reporting pointer spoils any existing beat. Task 4 onward relies on that finding.

This task exists to answer one question and no others. Resist improving anything you notice.

- [ ] **Step 1: Confirm the script currently fails to parse**

```bash
/home/oetiker/scratch/cargo-target/release/ansidrama record demo/mdmost.toml \
  -o /scratch/oetiker/claude-tmp/should-not-exist.webp
```

Expected: a parse failure naming `settle_ms`. This is the failing test — the blocker, demonstrated rather than assumed.

- [ ] **Step 2: Delete the line**

Remove line 23 of `demo/mdmost.toml`:

```toml
settle_ms = 300
```

Also amend the comment directly above it, which currently explains a floor that no longer exists:

```toml
# tmux has to start, then `bash` and `mdmost` inside it. A short floor here races the
# first prompt and the demo types into nothing.
startup_ms = 2500
```

Leave `startup_ms` itself alone — it is still a real key and still needed.

- [ ] **Step 3: Record the baseline**

```bash
/home/oetiker/scratch/cargo-target/release/ansidrama record demo/mdmost.toml \
  -o /scratch/oetiker/claude-tmp/baseline.webp \
  --dump-png /scratch/oetiker/claude-tmp/baseline-png
```

Expected: the run completes. Takes about five minutes — foreground, `timeout: 600000`, wait for it in the same turn.

- [ ] **Step 4: Read the manifest before looking at any frame**

```bash
head -5 /scratch/oetiker/claude-tmp/baseline-png/manifest.tsv
wc -l /scratch/oetiker/claude-tmp/baseline-png/manifest.tsv
```

The columns are `frame`, `scene`, `input`, `kind`, `hold_cs`. This replaces the old run log's frame tally, and it is a lookup — do not do arithmetic on frame counts.

- [ ] **Step 5: Check the beats the reporting pointer could have changed**

This is the finding this task exists for. In 0.4.0 every `click` and `drag` glide now sends motion reports, and mdmost listens for motion. Use the manifest to find the frames belonging to each scene whose glide crosses the mdmost pane, and open them:

- the `click = { x = 76, y = 30 }` scenes (four of them) — the glide crosses the right pane to reach the bottom edge
- the `drag` scenes in act 2 — these already sent motion, so they should be unchanged
- the `drag = { from = [53, 7], to = [97, 11] }` in act 4

For each, record in your notes: does a link light up, does a URL appear in the status bar, and is it a distraction or an improvement?

- [ ] **Step 6: Check the four beats the spec listed as open**

Open the frames for: act 4's `keys = []` re-capture (is the paste complete in the frame *before* it?), act 5's three `g`s and the `Tab` after them (did the contents pane open?), act 6's hover frame (`keys = ["[<35;12;19M"]` — does it show a caret or an arrow?), and the closing frame (is it dark?).

- [ ] **Step 7: Settle the reproducibility claim**

`docs/maintainer-notes.md` claims successive runs produce the same frame count. Record a second time to a different path and compare:

```bash
/home/oetiker/scratch/cargo-target/release/ansidrama record demo/mdmost.toml \
  -o /scratch/oetiker/claude-tmp/baseline2.webp \
  --dump-png /scratch/oetiker/claude-tmp/baseline2-png
wc -l /scratch/oetiker/claude-tmp/baseline-png/manifest.tsv \
      /scratch/oetiker/claude-tmp/baseline2-png/manifest.tsv
```

Whichever way this lands is a fact Task 9 writes into the notes. Do not assert it in advance.

- [ ] **Step 8: Commit**

```bash
git add demo/mdmost.toml
git commit -F - <<'MSG'
demo: the recorder no longer has a settle window to configure

`settle_ms` was the whole of the 0.2.0 pacing model and ansidrama >=0.3.0
refuses to parse a config that still carries it. Nothing else changes here:
this is the baseline every later beat is measured against.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

Note the quoted heredoc. Backticks in `git commit -m` are command-substituted by the shell.

---

## Task 3: The prose edit, and every coordinate re-read after it

**Files:**
- Modify: `demo/tour.md:92`
- Modify: `demo/mdmost.toml` (coordinates only, if any moved)

**Interfaces:**
- Consumes: the baseline from Task 2.
- Produces: a `demo/tour.md` whose claims match what the recording shows, and a set of verified coordinates. Every later task's clicks depend on these.

This task comes third and not last because **editing `demo/tour.md` rewraps paragraphs, which moves the rows every click and hover in acts 4 and 6 is pinned to.**

- [ ] **Step 1: Read the sentence in context**

```bash
sed -n '85,95p' demo/tour.md
```

Line 92 currently ends: `The status bar says which, every time.`

At act 4's 49-column pane the status bar's width budget drops the copy notice, so the tour contradicts itself on screen.

- [ ] **Step 2: Soften the claim**

Replace `The status bar says which, every time.` with a sentence that says the status bar names the form copied, without promising it at every width. For example:

```
arrives as Markdown. The status bar names which, when there is room for it.
```

Keep the replacement close to the original length. A much shorter or longer sentence rewraps the paragraph and moves more rows than necessary.

- [ ] **Step 3: Check the width thresholds did not move**

The drag in act 2 works only because the pane passes through widths where content changes shape. Editing the file can move those thresholds silently.

```bash
for w in 48 50 51 59 60 64 100; do
  echo "=== $w ==="
  ${CARGO_TARGET_DIR:-target}/release/mdmost --render-once --width $w demo/tour.md | head -60
done
```

Expected, per `docs/maintainer-notes.md`: the three-column table has two-line cells up to 59 and single-line rows from 60; the small flowchart's labels wrap up to 50 and are single-line from 51; the five-column table needs 60 and `pipeline.mmd` declines to draw below 65. **These numbers drift on their own** — if one has moved past the drag's range, say so and stop, because act 2 would then be showing nothing.

- [ ] **Step 4: Re-read act 4's coordinates off a real screen**

```bash
tmux -L probe -f demo/tmux.conf new-session -d -s coord -x 100 -y 30 \
  "${CARGO_TARGET_DIR:-target}/release/mdmost --mouse --config demo/config.toml demo/tour.md"
tmux -L probe split-window -h -t coord
tmux -L probe capture-pane -p -t coord.0
```

Reproduce act 4's state: a 49-column pane, `Space` pressed four times, then read the fenced block's `[copy]` row and column off the captured text. The current script uses `click = { x = 94, y = 1 }` and the comment records that this button moved from row 16 to row 1 between recordings for no reason but a renderer change.

Do the same for the table's `[copy]`, currently `click = { x = 93, y = 2 }`.

- [ ] **Step 5: Re-read act 6's coordinates**

Act 6 runs at full width. Search for `Where a link` and read off the rows of the three controls — the `http` link, the `#heading` reference and the footnote marker — currently rows 19, 22 and 24 of a hundred-column frame, with the link at columns 5–20.

- [ ] **Step 6: Update any coordinate that moved, and kill the probe**

Edit `demo/mdmost.toml` only where a number actually changed. Then:

```bash
tmux -L probe kill-server
ps -ef | grep -c '[m]dmost'
```

Check any surviving `mdmost` process's parent with `ps -o ppid=` before touching it. The owner runs this pager himself.

- [ ] **Step 7: Commit**

```bash
git add demo/tour.md demo/mdmost.toml
git commit -F - <<'MSG'
demo: the tour stops promising something a narrow pane cannot show

At act 4's 49-column pane the status bar drops the copy notice to fit, so
"every time" was a claim the recording contradicted on screen. Coordinates
re-read off a live pane after the rewrap.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 4: Act 6 — a pointer that hovers instead of a wire format typed by hand

**Files:**
- Modify: `demo/mdmost.toml` (act 6, currently lines 304–417)

**Interfaces:**
- Consumes: verified act 6 coordinates from Task 3.
- Produces: three `move` scenes replacing three raw escape sequences, and the row-scoped `await` idiom that Tasks 5–8 reuse.

Act 6 goes first among the re-stagings because it is where 0.4.0's headline feature lands and where the payoff is visible in the artifact: the hover frame currently draws a **text caret** instead of a pointer, because the only way to deliver a motion report used to be typing the wire format into a `keys` scene.

- [ ] **Step 1: Replace the hover with a `move` scene**

Delete the eleven-line comment block and the scene at line 332–334:

```toml
[[scene]]
keys = ["[<35;12;19M"]
hold_cs = 240
```

Replace with:

```toml
# The pointer glides to the link and rests on it. ansidrama >=0.4.0 reports its motion
# to an application in any-event tracking, which mdmost is (`src/tui/term.rs:249`), so
# the link lights and the status bar names its host with a real arrow on screen.
[[scene]]
move = { x = 12, y = 19 }
await = { find = "github.com", row = -1 }
hold_cs = 240
```

**The `row = -1` is load-bearing, not tidiness.** `github.com` also appears in the document body, so a whole-screen `await` would match with no hover having happened. This exact mistake was caught in ansidrama's own test suite.

- [ ] **Step 2: Prove the await can fail**

This is act 6's fault injection. Temporarily change the pattern to something that cannot appear:

```toml
await = { find = "zzz-no-such-host", row = -1 }
```

Record a truncated script ending just after this scene (see "The truncated-script technique" above).

Expected: the run **aborts**, naming the pattern and showing the last screen. If it completes, the await is not gating and nothing below is trustworthy — stop and investigate.

- [ ] **Step 3: Restore the real pattern and confirm it passes**

Put `github.com` back, re-record the truncated script.

Expected: the run completes. Then open the hover frame via `manifest.tsv` and confirm two things by eye: an **arrow** rests on the link (not a text caret), and the link is lit.

- [ ] **Step 4: Replace the un-hover**

The scene at line 340–342 exists because hover is sticky — a pointer that never reported leaving would strand the URL in the status bar over an unrelated footnote.

```toml
[[scene]]
keys = ["[<35;28;24M"]
hold_cs = 60
```

becomes:

```toml
# The pointer moves off the link to the footnote marker. Without this the URL would
# stand in the status bar for the rest of the recording, over a footnote it has
# nothing to do with.
[[scene]]
move = { x = 28, y = 24 }
hold_cs = 60
```

No `await` here: the assertion worth making is that the URL has *gone*, and `await` waits for text to appear, not to vanish. The following click's `await` covers this beat's outcome.

- [ ] **Step 5: Add an await to the footnote click**

```toml
[[scene]]
click = { x = 28, y = 24 }
await = "↓ 8 more"
hold_cs = 300
```

`↓ 8 more` is drawn in the note box's bottom edge and appears nowhere else, so a whole-screen match is safe here. **Verify that string against a real frame before trusting it** — read it off the baseline dump rather than copying it from this plan, because the count depends on the note's length.

- [ ] **Step 6: Replace the dismissal move**

```toml
[[scene]]
keys = ["[<35;90;6M"]
hold_cs = 40
```

becomes:

```toml
# (90, 6) is blank canvas right of the code frame, clear of both the box and any
# control. A click outside is what design spec §6 gives for dismissal, and unlike
# `Escape` it cannot lay a bare ESC in front of the next keystroke.
[[scene]]
move = { x = 90, y = 6 }
hold_cs = 40
```

Leave the `click = { x = 90, y = 6 }` scene that follows it in place.

- [ ] **Step 7: Add an await to the third `F` of the keyboard walk**

The third `F` reaches `the project page`, and the status bar naming its host with no pointer on screen is the entire point of the beat.

```toml
[[scene]]
keys = ["F"]
await = { find = "github.com", row = -1 }
hold_cs = 300
```

Leave the surrounding comment block — it explains why the walk runs backwards and why it takes three presses, and all of that is still true.

- [ ] **Step 8: Confirm no raw escapes remain in act 6**

```bash
grep -n '\\u001B' demo/mdmost.toml
```

Expected: no output.

- [ ] **Step 9: Record the truncated script through act 6 and check the frames**

Open the hover frame, the note-open frame, the note-scrolled frame, and the third-`F` frame. Confirm the counter walks `8 → 7 → 6 → 5` and the document stays at 75%.

- [ ] **Step 10: Remove the truncated script and commit**

Delete `demo/mdmost.trunc.toml` — a file you created, so this is safe, but confirm the path before running any removal.

```bash
git add demo/mdmost.toml
git commit -F - <<'MSG'
demo: act 6 hovers with a pointer instead of a hand-typed wire format

Three raw SGR motion reports become `move` scenes, so the hover frame shows an
arrow resting on the link rather than a text caret. The awaits are row-scoped
to the status bar: `github.com` is also in the document body, and an unscoped
match would pass with no hover having happened.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 5: Acts 1–3 — the launch, the drags, the sideways scroll

**Files:**
- Modify: `demo/mdmost.toml` (acts 1–3, currently lines 65–134)

**Interfaces:**
- Consumes: the row-scoped await idiom from Task 4.
- Produces: gated acts 1–3.

- [ ] **Step 1: Gate the `less` launch**

```toml
[[scene]]
keys = ["Enter"]
await = "<text only the loaded file shows>"
hold_cs = 260
```

Derive the pattern rather than guessing it: open the baseline frame for this scene and pick text that `less` draws and the empty prompt does not. **Do not pick text that also appears in the mdmost pane** — the same file is open on both sides, which makes almost every candidate ambiguous. A `less`-specific artifact such as its filename prompt on the bottom row (`row = -1`) is the safer choice.

- [ ] **Step 2: Gate the widening drag**

The drag's point is that 49 columns become 64 and the content changes shape. Pick a pattern from a frame at the wide width:

```toml
[[scene]]
drag = { from = [51, 15], to = [37, 15] }
await = "<text the widened pane shows and the narrow one does not>"
hold_cs = 200
```

Task 3 Step 3 measured the thresholds; use a table row or flowchart label that is single-line only above one of them. Gate only the two widening drags, not the two that return — the narrowing direction restores a screen already seen, so any pattern for it would have matched earlier too.

- [ ] **Step 3: Gate the sideways scroll**

This is the best await in the script, because the `↔` readout is a number that exists only after the table has moved:

```toml
[[scene]]
keys = ["Right", "Right", "Right", "Right", "Right", "Right"]
type_cs = 14
await = { find = "↔", row = -1 }
hold_cs = 240
```

Read the actual readout off the baseline frame. If it shows a column count, **include the number** — `↔ 13` proves the table moved six steps, while a bare `↔` may be drawn before any scrolling at all. Check the frame before the scroll to see which.

- [ ] **Step 4: Fault-inject act 3's await**

Change the `find` to a string that cannot appear, record truncated through act 3, confirm the abort, restore, re-record, confirm it completes.

- [ ] **Step 5: Commit**

```bash
git add demo/mdmost.toml
git commit -F - <<'MSG'
demo: acts 1-3 wait for the screen they claim to produce

The sideways scroll gates on the `↔` readout's own count, which exists only
once the table has actually moved; the drag gates on content that is single-line
only above a width threshold.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 6: Act 4 — three copies, each proven to have crossed the split

**Files:**
- Modify: `demo/mdmost.toml` (act 4, currently lines 136–238)

**Interfaces:**
- Consumes: act 4 coordinates verified in Task 3.
- Produces: gated copy beats, and a ruling on whether the `keys = []` re-capture is still needed.

Act 4 is where a silent miss is most dangerous. A click that lands on document text copies nothing, and the `prefix ]` two scenes later pastes the *previous* buffer a second time — which looks almost right. Three of the four takes behind the current recording had a silently broken act in them.

- [ ] **Step 1: Gate each paste, not each copy**

The obvious gate — the status bar's copy notice — **is not available**, because at 49 columns the status bar drops it. That is the contradiction Task 3 softened rather than fixed. So gate the paste instead, on the text arriving in the nano pane:

```toml
[[scene]]
keys = ["C-b", "]"]
await = { find = "<text unique to the pasted form>", row = <a row inside nano's pane> }
hold_cs = 260
```

**The innocent-occurrence trap is acute here.** The table is on screen in mdmost while its TSV is being pasted into nano, so the cell text appears twice. Two ways out, and you need one of them for each of the three pastes:

- scope to a row that only nano's pane occupies, read off the baseline frame; or
- pick text that differs between the two forms — TSV joins cells with tabs, so the pasted line's *spacing* differs from the rendered table's.

Prefer the row scope. It is the technique that turned ansidrama's own hover probe from a test that could not fail into one that could.

- [ ] **Step 2: Do the same for the second and third pastes**

The second is the fenced block's Rust source; the third is the dragged paragraph's Markdown. Each gets its own pattern, derived from its own baseline frame. Do not reuse the first pattern — each paste must prove *it* happened, and the buffer from the previous copy is exactly what a failure would show.

- [ ] **Step 3: Try deleting the `keys = []` re-capture**

The scene at line 236–238 is a 0.2.0 workaround: nano redrew the largest paste in more than one write, and a single capture could freeze the pane one paste short of the truth.

```toml
[[scene]]
keys = []
hold_cs = 200
```

Delete it. Continuous sampling captures what the app draws *between* inputs, which is precisely the case this worked around.

- [ ] **Step 4: Record truncated through act 4 and check the paste is whole**

Open the last frame of act 4 and confirm all three pastes are present and complete.

**If the paste is half-drawn, put the scene back** — and say so, because that is a finding about the recorder, not a defeat. If it stays, give it the same `await` as Step 1's third paste so it is gated rather than hopeful.

- [ ] **Step 5: Fault-inject the third paste's await**

The third is the largest and the one the deleted workaround protected. Change its pattern to something impossible, confirm the abort, restore, confirm it passes.

- [ ] **Step 6: Commit**

```bash
git add demo/mdmost.toml
git commit -F - <<'MSG'
demo: each of act 4's three copies proves it crossed the split

A miss here was silent and looked almost right: the click landed on document
text, copied nothing, and `prefix ]` pasted the previous buffer twice. The
awaits are scoped to nano's own rows, because the table is on screen in mdmost
while its TSV is arriving next door.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 7: Act 5 — the pane closes, and the key reaches the right application

**Files:**
- Modify: `demo/mdmost.toml` (act 5, currently lines 240–302)

**Interfaces:**
- Consumes: nothing new.
- Produces: a gated act 5, and a ruling on the three sacrificial `g`s.

**This is the task whose outcome the spec does not promise.** Every other beat fails because the app was slow to draw, and `await` is built for that. Act 5 fails differently: `bash` exits, and for a moment the keystroke reaches a pane tmux has not yet closed — the key goes to the *wrong application*. Recorded that way once, and every beat of act 5 after it was a different demo.

- [ ] **Step 1: Gate the pane close**

```toml
[[scene]]
keys = ["Enter"]
await = "<text visible only once mdmost has all 100 columns>"
hold_cs = 300
```

The wide `pipeline.mmd` diagram declines to draw below 65 columns, so something it draws only at full width is the natural pattern. Read it off a baseline frame from after the pane closed.

- [ ] **Step 2: Try removing the three sacrificial `g`s**

Delete this scene entirely:

```toml
[[scene]]
keys = ["g", "g", "g"]
type_cs = 30
hold_cs = 60
```

- [ ] **Step 3: Record truncated through act 5, several times**

Record **at least three times**. This beat is a race, and a race that passes once has not been shown to be fixed. Each run: confirm the `Tab` opened the contents pane, the four `Down`s walked the list rather than the document, and `Enter` selected a heading.

- [ ] **Step 4: Rule on it, and write the ruling into the script**

**If all three runs pass:** the `g`s stay deleted. Replace their long comment with a short one recording that the wait now survives the pane transition, and that the `await` in Step 1 is what makes a regression loud.

**If any run fails:** put the `g`s back, keep the `await`, and rewrite their comment to say what is now true — that the `await` catches the miss but does not prevent it, and the three keystrokes still buy the quiet the pane transition needs. This is a good outcome, not a failure: act 5 improves either way, because the failure mode stops being silent.

Either way the old comment must go. It explains a `settle_ms` mechanism that no longer exists.

- [ ] **Step 5: Commit**

Write the commit subject to match what actually happened — do not copy a subject from this plan that asserts the opposite of your result.

```bash
git add demo/mdmost.toml
git commit -F - <<'MSG'
demo: act 5 waits for the full-width screen instead of guessing at it

<Say here whether the three sacrificial `g`s survived, and why. If they stayed,
say that the await makes the miss loud without preventing it.>

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 8: Act 7 — the theme beat comes back

**Files:**
- Modify: `demo/mdmost.toml` (act 7, currently lines 419–447)

**Interfaces:**
- Consumes: a working acts 1–6.
- Produces: the restored theme beat, verified by eye.

The beat was cut because the recording showed a **dark** screen carrying `theme: light` in its status bar, held for 2.8 seconds on the hero image. A status bar that names a theme the screen is not wearing is the one thing this project says a status bar may never do.

0.3.0 fixed the likely cause: output still draining from the previous input could end the current input's wait, and the grace is now measured on real grid changes rather than PTY bytes. That matches the 2026-08-13 bisect exactly — only the script with a five-repaint keyboard walk before the `t` failed.

- [ ] **Step 1: Restore the beat before the closing hold**

```toml
# The theme beat, restored. It was cut on 2026-08-13 because the first `t` recorded a
# dark screen captioned `theme: light`; ansidrama 0.3.0 fixed the capture race that
# caused it. The `await` below is necessary and NOT sufficient — see step 3.
[[scene]]
keys = ["t"]
await = { find = "theme: light", row = -1 }
hold_cs = 280

[[scene]]
keys = ["t"]
await = { find = "theme: dark", row = -1 }
hold_cs = 280
```

Read the exact status bar strings off a live pane before trusting `theme: light` and `theme: dark` verbatim.

- [ ] **Step 2: Record the FULL script, not a truncated one**

```bash
/home/oetiker/scratch/cargo-target/release/ansidrama record demo/mdmost.toml \
  -o /scratch/oetiker/claude-tmp/theme-check.webp \
  --dump-png /scratch/oetiker/claude-tmp/theme-png
```

**Truncation is not acceptable for this task.** The bisect established that the trigger is act 6's keyboard walk *preceding* the `t`. A truncated script is the configuration that always worked, so a green truncated run proves nothing.

- [ ] **Step 3: Open the frame and look at it**

Find the light-theme scene's frames in `manifest.tsv`, open the PNG, and confirm the body background is actually light — `#fdfcf9`, not `#11141b`.

**This step cannot be replaced by the `await`, and that is the whole history of this beat.** The broken frame carried a *correct* caption over a wrong colour. `await` matches text, never colour, so it would have matched the broken frame happily. A green recording is not evidence.

If the frame is dark: the beat is cut again, the `await` scenes are reverted, and the finding goes into `docs/maintainer-notes.md` as a second entry in the bisect table — this time against 0.4.0.

- [ ] **Step 4: Confirm the closing frame is still dark**

The tour must open and close in dark, because those are the two frames a looping viewer lands on. The second `t` returns to dark before the closing hold, so check the final frame explicitly.

- [ ] **Step 5: Commit**

```bash
git add demo/mdmost.toml
git commit -F - <<'MSG'
demo: the theme beat returns, and its frame was looked at

Cut on 2026-08-13 for recording a dark screen captioned `theme: light`.
ansidrama 0.3.0 measures its grace on real grid changes rather than PTY bytes,
which is the mechanism the bisect pointed at. The await cannot verify this beat
-- it matches text, and the broken frame's text was correct -- so the frame was
opened and its background confirmed light.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 9: The maintainer notes describe the recorder that exists

**Files:**
- Modify: `docs/maintainer-notes.md` (the timing-model section around lines 205–235, and the theme section from line 240)

**Interfaces:**
- Consumes: every finding from Tasks 2–8.
- Produces: a regeneration recipe a maintainer who is not you can follow.

Two sections are **rewritten, not patched**. They describe a recorder that no longer exists.

- [ ] **Step 1: Replace the timing-model passage**

Everything about `settle_ms`, the quiet-window pacing, and the two consequences that "have bitten this script" is 0.2.0 truth. Replace with:

- the recorder samples the terminal grid continuously and assembles frames from that log;
- a scene declares its finished screen with `await`, and a pattern that never matches **aborts** the run naming the pattern;
- an `await` pattern must be text only the finished screen contains, and `row = -1` reaches the status bar;
- `hold_cs` is still a duration written into the WebP, not a sleep — **this part survives**, and so does the `Escape` consequence, because a bare ESC in front of the next keystroke is still read as `M-<key>`.

- [ ] **Step 2: Delete the frame-counting passage and replace it with the manifest**

"The run log counts frames, it does not number them" and the off-by-one story it carries are retired: 0.3.0 no longer prints that tally, and `--dump-png` writes `manifest.tsv` mapping `frame`, `scene`, `input`, `kind`, `hold_cs`.

Keep the lesson that outlives the mechanism: **when a beat looks wrong, check the app in a live pane before changing the script.** The `tmux -L probe` recipe stays verbatim — it is still how these are settled in a minute.

- [ ] **Step 3: Correct the reproducibility claim with Task 2's measurement**

The notes say successive runs produce the same frame count. Task 2 Step 7 measured this on the new recorder. Write down what it actually found, whichever way it went.

- [ ] **Step 4: Rewrite the theme section**

`### The theme beat is cut, and why — restore it when ansidrama is fixed` becomes a section about a beat that is present and how to verify it. **Keep the bisect table** — it is the evidence, and it is what makes the restoration explicable rather than hopeful.

State plainly that `await` cannot verify this beat and why.

- [ ] **Step 5: Add the verification steps to the recipe as numbered steps**

Not prose. A maintainer regenerating the demo must do, in order:

1. rebuild both binaries;
2. check the width thresholds with `--render-once` at 48, 50, 51, 59, 60, 64, 100;
3. re-read every coordinate off a live 49-column pane;
4. record with `--dump-png`;
5. open the theme frame and confirm the background is light;
6. confirm the closing frame is dark;
7. only then replace `docs/demo/mdmost.webp`.

- [ ] **Step 6: Commit**

```bash
git add docs/maintainer-notes.md
git commit -F - <<'MSG'
docs: the notes describe the recorder that exists, not the one they were written for

The settle window, the quiet-window pacing and the run log's frame tally are all
gone from ansidrama; `await` and `manifest.tsv` replace them. The theme section
becomes a beat to verify rather than a beat to mourn, and the bisect table stays
as its evidence.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

---

## Task 10: The artifact

**Files:**
- Modify: `docs/demo/mdmost.webp`

**Interfaces:**
- Consumes: everything.
- Produces: the committed recording.

- [ ] **Step 1: Confirm the working tree is clean apart from the artifact**

```bash
git status --short
```

`demo/mdmost.toml~` will appear as untracked. **It is the owner's editor backup. Leave it.**

- [ ] **Step 2: Record to the real output path**

```bash
/home/oetiker/scratch/cargo-target/release/ansidrama record demo/mdmost.toml \
  --dump-png /scratch/oetiker/claude-tmp/final-png
```

No `-o` this time: the config's own `out` key resolves to `docs/demo/mdmost.webp`.

- [ ] **Step 3: Walk the success criteria against the frames**

From the spec §9, each checked against `/scratch/oetiker/claude-tmp/final-png` via `manifest.tsv`:

1. the run parsed and completed;
2. `grep -n '\\u001B' demo/mdmost.toml` returns nothing;
3. every beat that can fail silently carries an `await`;
4. the theme frame is open on screen and its background is light;
5. `docs/maintainer-notes.md` describes 0.4.0;
6. `demo/tour.md` claims nothing the recording contradicts;
7. the hero image and the closing frame are both dark.

- [ ] **Step 4: Check the size is sane**

```bash
ls -la docs/demo/mdmost.webp
```

The previous recording was about 1.7 MB across 906 frames. A wildly different size is a signal worth explaining before committing — but note that frame counts are now partly app-driven, so a moderate difference is expected rather than alarming.

- [ ] **Step 5: Commit**

```bash
git add docs/demo/mdmost.webp
git commit -F - <<'MSG'
demo: re-recorded on a recorder that waits for the screen

Same seven acts. Every beat that could fail silently now declares what its
finished screen says, so a run that completes is a run where each one happened.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
MSG
```

- [ ] **Step 6: Report what is left open**

Anything the tasks above ruled differently from the spec's expectation — act 5's `g`s, act 4's re-capture, the reporting pointer's effect, the reproducibility claim — is reported to the owner, not silently absorbed.

---

## Notes for whoever executes this

- **Never end a turn with a recording or a build still running.** Pass `timeout: 600000`, keep it in the foreground, and wait for it in the same turn. A backgrounded shell is killed with the turn and its result is lost — this has hit five of six agents on this project.
- **Read the log, not the exit code.** A parse failure and an abort-on-await look different in the log and identical in an exit status.
- **A recording is not cheap.** Five minutes each. The truncated-script technique exists so you are not paying full price to check one act; use it everywhere except Task 8, which specifically may not.
- **Every pattern in this plan is a target, not a literal.** Derive each one from a real captured frame and reject any that also appears somewhere innocent. That check is the difference between a test and a decoration.
