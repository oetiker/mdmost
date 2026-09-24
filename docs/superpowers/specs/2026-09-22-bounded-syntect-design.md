# A bounded `syntect`

Design authority for the work that follows. Written 2026-09-22, after v0.3.4.

This is the first of two branches. The second, the viewport-driven highlighter, has its
own spec and depends on this one landing first.

## 1. What this changes

`syntect` is vendored at `vendor/syntect/`, carrying two patches: the loop fix from
upstream PR #706, and a new per-line parse budget that turns an unbounded parse into a
recoverable error.

After this, **no document content can make `mdmost` hang.** The guard added in PR #18 — a
worker thread whose only purpose was to be abandoned — becomes unnecessary and is removed.

Bounded is not the same as fast. A document can still spend seconds highlighting, and the
first paint still waits for it. Making that work incremental is the second branch's job,
and it is only safe to do on the main thread once this branch has made every parse finite.

## 2. Why vendor

A `.sublime-syntax` definition can drive `ParseState::parse_line` into a loop that never
returns. Two lines of JavaScript are enough. `docs/upstream/2026-09-21-javascript-syntax-hang.md`
holds the reproducer and the measurements; upstream PR #706 holds the root cause.

The loop cannot be escaped from outside. Rust has no thread cancellation. `pthread_cancel`
needs `unsafe`, acts only at cancellation points that a CPU-bound parse loop never
reaches, and can corrupt the process if it fires while the allocator lock is held. A
signal plus `longjmp` is undefined behaviour under unwinding. So the only ways out are a
separate process, which puts IPC in the highlighter's hot path, or a bound inside the
parser.

Four cheaper escapes were examined and all are closed; the evidence is in
`docs/upstream/2026-09-22-syntect-parse-line-step-budget.md`. In summary: the fixed
JavaScript definition does not load under 5.3.0, because the fix *is* a `branch` and 5.3.0
has no branch support; `syntect` master has branch support but is unreleased; the
`JavaScript (Babel)` definition that `bat` and docs.rs fall back to is excluded from the
`fancy-regex` build and exists only under `onig`, which needs a C toolchain; and PR #706
is unreviewed with no CI run, in a repository where nothing has merged since May 2026.

Vendoring is not a new move here. `vendor/pulldown-latex/` is 8,347 lines of someone
else's crate carried at a known commit with local patches. `syntect`'s `src/` is 11,020
lines, the same order of magnitude.

## 3. What is vendored

`syntect` 5.3.0, the version in `Cargo.lock` today.

**Not upstream master, although it has branch support.** `two-face` ships a *compiled
dump* of 937 syntax definitions built against a particular `syntect` version. A change to
the dump format would break deserialisation of the whole set. 5.3.0 is the version
`two-face` 0.5.2+bat-0.26.1 targets. Chasing branch support by taking master is a separate
question with a much larger blast radius, and it is out of scope here.

`vendor/syntect/VENDORED.md` records the source version, each patch, and which patches
are offered upstream — the same contract `vendor/pulldown-latex/VENDORED.md` follows.

**Nothing is deleted, and that is deliberate.** `FontStyle::from_bits_unchecked`
(`src/highlighting/style.rs:127`) is the only `unsafe` item in the tree, and it is
referenced only by `tests/snapshots/public-api.txt`, so removing it would let the vendored
crate carry `#![forbid(unsafe_code)]` like the mdmost crate. It stays anyway.

Removing it changes public API for one consumer's benefit, so upstream would not take it,
and a patch upstream will not take is a patch that keeps `vendor/` alive forever. The
property bought is cosmetic: `#![forbid(unsafe_code)]` in the mdmost crate has never
reached `vendor/`, so the guarantee mdmost actually makes is the same either way.

**Every patch in this vendor is therefore offered-upstream content**, which is what makes
§11's exit a procedure rather than a hope. `vendor/pulldown-latex` cannot say that: its
patch 5 widens two items' visibility that crates.io does not export, so deleting that
directory would stop mdmost compiling.

## 4. Patch 1 — break loops between non-consuming `set`s

Upstream PR #706 by `tontinton`, +73/−14 in `src/parsing/parser.rs`. Not our work; the
vendor notes say so.

**The defect.** `parse_next_token`'s loop guard remembers only a non-consuming *push*, so
it can check the next *pop* for a loop. A `set` is neither, and leaves the stack depth
unchanged, so two contexts that `set` each other at the same byte spin forever. In the
JavaScript definition, `expression-statement-continuation` matches empty because `/` is in
its operator class, and the rule for "blanks and comments, then end of line" also matches
empty. They hand the position back to each other.

**The fix.** Record the `(byte, context stack)` each non-consuming `set` is taken from.
When the pair recurs, advance one character — which is what Sublime Text does for the
push/pop loop, so the remedy already has precedent in the same function.

**Two local refinements, both in one change.** Upstream keys on
`self.stack.iter().map(|lvl| lvl.context).collect()`, which allocates a `Vec` on every
non-consuming `set` and ignores the rest of each `StateLevel`. `StateLevel` holds
`context`, `prototypes` and `captures`, so two levels sharing a context but differing
otherwise are treated as identical. Replace the key with `(usize, u64)`, where the `u64` is
an incremental hash over each level's `context` **and** `prototypes`. That removes the
allocation and makes the key more precise than upstream's at the same time.

`captures` stays out of the key: it holds a `Region`, which is not hashable. A fingerprint
collision, or a state that differs only in `captures`, makes the guard treat that state as
a repeat and skip its `set`; the wrong stack can then persist for the rest of the parse.
The only guarantee is that the parser still advances, so it can never hang or panic.

**Measure before keeping the allocation change.** Non-consuming `set`s are rare in real
syntaxes, so the allocation may cost nothing measurable. If it does not show in the
highlight timings, keep the change only for the precision, and say so in `VENDORED.md`.

**If #706 merges upstream**, reconcile rather than re-apply: the upstream version and the
refinement touch the same lines.

## 5. Patch 2 — a parse budget

**What is unbounded.** Exactly one thing: the `while self.parse_next_token(...)` loop in
`parse_line`. A single regex match is already bounded — `fancy-regex` 0.11.0 defaults to a
backtrack limit of 1,000,000 (`src/lib.rs:396`) and `syntect` treats a regex error as a
non-match rather than propagating it (`src/parsing/regex.rs:225`, with the comment at 236
naming catastrophic backtracking). So capping the token loop is sufficient.

**The mechanism.** A new method beside `parse_line` takes the maximum number of tokens for
that line and returns a new `ParsingError` variant when it is exceeded. `parse_line` keeps
its signature and delegates with no limit, so no existing caller changes behaviour.
`ParsingError` is `#[non_exhaustive]`, so the variant is not a breaking change.

**A parameter, not a field on `ParseState`.** `ParseState` derives `Eq` and `PartialEq`, so
a limit stored on it would make two otherwise-identical parse states compare unequal — a
configuration value leaking into an identity. A parameter also matches what issue #202
asked for: "a version of the parse function that takes a timeout".

**A token count, not a `Duration`.** A count is deterministic, which is what lets a test
assert the limit fires exactly when it should; a wall-clock budget makes the same test
flaky on a loaded machine, and this project shares one machine with other work. A
`Duration` variant can be layered on a counter cheaply by checking the clock every N
tokens, and §9 offers upstream exactly that.

**The limit is measured, not guessed.** What has to be measured before a number is
written down: the maximum tokens per line produced by a legitimate line across the
languages in `docs/maintainer-notes.md`'s table, including a deliberately hostile
legitimate case — one long minified JavaScript line. The limit must sit far enough above
that ceiling that no real document loses colour. Record the measurement beside the
constant, not the reasoning.

`MAX_HIGHLIGHT_BYTES` (256 KiB) and `MAX_HIGHLIGHT_LINES` (10,000) stay as they are. They
are coarse pre-checks on block size and are orthogonal to a per-line token bound.

## 6. Wiring

`two-face` depends on `syntect`, so a plain path dependency would compile two copies whose
`SyntaxSet` types do not interoperate. `[patch.crates-io]` is therefore required:

```toml
[patch.crates-io]
syntect = { path = "vendor/syntect" }
```

`publish = false` in the manifest, so a path dependency blocks nothing. `vendor/syntect`
joins `workspace.members` beside `vendor/pulldown-latex`.

**`two-face` 0.5.2+bat-0.26.1 requires `syntect = "5.3.0"`**, which is `^5.3.0` — anything
from 5.3.0 up to but excluding 6.0.0. Vendoring at exactly `5.3.0` satisfies it.

**Never give the vendored crate a pre-release version.** `5.3.1-mdmost` would *not* satisfy
`^5.3.0`, because Cargo excludes pre-releases unless a requirement asks for one. The patch
would then be silently ignored and the build would carry two `syntect` crates whose
`SyntaxSet` types do not interoperate. Keep the version at `5.3.0`, or a plain `5.3.1`.

**Keep every feature name.** `two-face` selects `dump-load`, `parsing` and — through its own
`syntect-fancy` — `regex-fancy`, with `default-features = false`. `mdmost` selects
`default-fancy`, which expands to `parsing`, `default-syntaxes`, `default-themes`, `html`,
`plist-load`, `yaml-load`, `dump-load`, `dump-create` and `regex-fancy`. Renaming or
dropping any of these breaks the build as a feature-resolution error, which does not read
as a vendoring mistake.

`cargo tree -d` must show one `syntect`, asserted in CI.

## 7. What this retires in `mdmost`

`highlight_with` already treats a parse error as "not highlightable" —
`state.parse_line(raw, set).ok()?` falls through to `plain`. So a bounded parse needs no
new handling at the call site, and the following all go:

- `run_within` and the worker thread it spawns,
- `ABANDONED`, `MAX_ABANDONED` and `reset_abandoned_for_test`,
- `budget_for`, `RELEASE_FLOOR`, `MAX_HIGHLIGHT_BUDGET` and `DEBUG_BUDGET_MULTIPLIER`,
- the cold-compile budget arithmetic in `docs/maintainer-notes.md` that those constants
  required, and the warning that anyone retuning them must re-measure both build profiles,
- `HIGHLIGHT_GLOBALS_TEST_LOCK`'s `ABANDONED` half. The `CACHE` half stays until the
  second branch moves the memo out of a global.

`highlight()` keeps its signature and its blocking contract. Nothing outside
`src/highlight.rs` changes.

## 8. What the reader sees

A block whose parse trips the token-limit guard renders as plain themed text, with
`highlighting gave up` written into the bottom edge of its frame via the existing
`Canvas::framed_captioned`. A caption in the border costs no row, so the block's geometry
is unchanged.

**This needs `highlight` to say why a block is plain, which it currently cannot.** Today a
block with no language tag, a block whose tag resolves to no syntax, and a block that
failed all return plain lines and are indistinguishable. Only the last deserves a caption —
a fence tagged `text` is not a failure. So `highlight` reports an outcome alongside its
lines:

- `Highlighted` — a syntax was found and the parse finished.
- `Plain` — no tag, no syntax for the tag, or a parse error other than the token-limit
  guard. Draws no caption. The common case.
- `Failed` — the token-limit guard cut the parse short. Draws the caption.

The outcome is memoised with the lines, so a failed block is neither re-attempted nor
re-captioned on the next layout probe. `highlight`'s existing signature returns
`Vec<Line>`; the outcome rides alongside it rather than changing that return type, because
export and `--to-ansi` want the lines and nothing else.

This is per block, not per language and not per process. A construct breaks the
highlighter, not a language: upstream #460, #650 and #656 are three different syntaxes
looping for related reasons. Every other block in the document keeps its colour, and
nothing is silently disabled for the rest of the session.

## 9. Upstream

**The budget is offered upstream.** Issue #202 has been open since 2018 asking for a
highlighting timeout, and `trishume` replied there that he would accept a PR adding a
version of the parse function taking a `std::time::Duration`, and that he is unlikely to
write it himself. No PR followed in eight years, and no general iteration cap on
`parse_line` has ever been proposed.

The upstream commit is shaped for upstream, not for us: the token limit as the primitive,
a `Duration` layer over it answering #202 as asked, the limit defaulting to unlimited, and
its own tests. Filed as PR #708 (branch `oetiker:parse-line-token-limit`, commit
`f554c9a`, "Refs #202"); `VENDORED.md` records it under patch 2.

**Patch 1 is not offered** — it is already PR #706. Our comment on that PR
(2026-09-22) supplies an independent reproducer. The local refinement in §4 was offered
as a comment on #706, at the owner's choice rather than waiting for #706 to merge:
<https://github.com/trishume/syntect/pull/706#issuecomment-5811758916>.

**Nothing is local-only.** See §3: the one change that would have been — deleting the
crate's single `unsafe` item — is deliberately not made, so that §11's exit has no blocking
step.

## 10. Testing

**The reproducer becomes a real test, not an ignored one.** `mdmost`'s existing
`the_javascript_hang_degrades_to_plain_text` asserts the *degradation*. After this branch
the two lines must parse and highlight normally, because #706 fixes the loop. The test
changes meaning and must be rewritten, not deleted: it is the only test that names the
input that started all of this.

- **`syntect`'s own suite runs in the vendor**, including its syntax tests. A vendored
  crate whose tests are not run is a fork nobody can update.
- **The budget fires exactly once at the boundary**: a synthetic syntax and a line
  producing a known token count, asserted at the limit and one below it.
- **A legitimate long line keeps its colour** — the hostile-but-real minified JavaScript
  case from §5, asserted against the chosen limit, so a future tightening of the constant
  breaks a test instead of a reader's document.
- **The default is unlimited**: a test asserting that a `ParseState` with no limit set
  behaves exactly as 5.3.0 did, which is what makes the upstream offer honest.
- **`cargo tree -d` shows one `syntect`** — asserted in CI, not by hand, because a silent
  second copy would produce two incompatible `SyntaxSet` types.
- **The musl static build and the Windows compile must both pass.** The vendored crate is
  pure Rust with no build script, so this is a check, not a risk.

## 11. This vendor is temporary

Not a prediction — a procedure, and the conditions it waits on are named so that a later
reader can check them instead of guessing.

**Delete `vendor/syntect/` when a released `syntect` carries both patches.** Both, not
either: #706 alone leaves the budget unavailable, and the budget alone leaves the
JavaScript loop unfixed. Check with `cargo add syntect@<new> --dry-run` and by reading the
release notes for a `ParsingError` variant covering the token limit.

Then:

1. Delete `vendor/syntect/` entirely. **No step blocks this** — every patch is
   offered-upstream content, and nothing in `src/` depends on an item a crates.io `syntect`
   does not export. §3 is what buys that, and any future local patch must either keep it
   true or amend this section to say what it broke.
2. Restore a plain version requirement in the root `Cargo.toml`:
   `syntect = { version = "…", default-features = false, features = ["default-fancy"] }`.
3. Drop the `[patch.crates-io]` table and remove `vendor/syntect` from
   `workspace.members`.
4. Set the token limit through whatever API upstream shipped, which may not be the one
   §5 proposes. If upstream took a `Duration` instead of a count, the determinism argument
   in §5 has to be re-made against the tests that relied on it, not quietly dropped.
5. Re-run the gates, including `cargo tree -d`, the musl static build and the Windows
   compile.

**What to watch, as of 2026-09-24.** PR #706 open and unreviewed since 2026-09-12, no CI
run. PR #708, filed against issue #202, open and awaiting review. Nothing merged in the
repository since May 2026, though PRs merged in 1-9 days through April, and three
collaborators with write access still comment. Release cadence is roughly annual: 5.3.0
2025-09-27, 5.2.0 2024-02-07, 5.1.0 2023-07-31, 5.0.0 2022-05-04.

**So the realistic wait is a year or more, and may be indefinite.** That is a reason to
keep the exit cheap, not a reason to pretend the vendor is permanent.

## 12. Out of scope

- **Branch support, and the newer syntax definitions it would unlock.** That means
  vendoring master, which risks `two-face`'s compiled dump. Separate question.
- **Finding which rule in the JavaScript definition loops.** #706 identifies it; fixing
  the definition is Sublime's business and was done upstream in 2022.
- **Bundling any third-party syntax definition.** `EXTRA_SYNTAXES` holds definitions
  written under this project's own licence, and that stays true.
- **The viewport-driven highlighter.** Second branch, own spec. This branch removes the
  thread; it does not make highlighting incremental.
- **`oniguruma`.** No C toolchain in the build, ever.
