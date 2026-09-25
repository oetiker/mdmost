# A step budget for `syntect`'s `parse_line`

Follow-up to `2026-09-21-javascript-syntax-hang.md`. That report establishes that a
`.sublime-syntax` definition can drive `ParseState::parse_line` into a loop that does not
return. This one records what a fix upstream would look like and what it would retire
here.

Nothing has been filed. Whether to file it, and where, is undecided.

## Prior art upstream

- Issue [#202](https://github.com/trishume/syntect/issues/202), open since 2018, asks for
  a highlighting timeout. `trishume` replied that he would accept a PR adding a version of
  the parse function taking a `std::time::Duration`, and that he is unlikely to write it
  himself. No PR followed in eight years.
- No general iteration cap on `parse_line` or `parse_next_token` has ever been proposed.
- Two bounded guards are merged, both narrower than this: PR #142 turns Oniguruma's
  match-limit error into a `Result` instead of a panic, and PR #597 caps context pushes
  at 100. Neither fires on a loop that consumes nothing and does not grow the stack.
- Targeted fixes for individual looping syntaxes are open and unreviewed: #706 (the
  `set`-to-`set` case that hangs `mdmost`), #691, #692, #705.

A budget and a rule fix are not exclusive. #706 fixes the case in hand; a budget bounds
every case not yet found, of which #460, #650 and #656 suggest there are several.

## The problem this solves downstream

A `parse_line` that does not return cannot be cancelled. Rust has no thread cancellation.
`pthread_cancel` acts only at cancellation points, which a CPU-bound parse loop does not
reach, needs `unsafe` and `libc` against this crate's `#![forbid(unsafe_code)]`, and can
corrupt the process if it fires while the allocator lock is held. A signal plus `longjmp`
is undefined behaviour in the presence of unwinding. Killing a separate process works,
but puts an IPC boundary and a serialisation of every styled line in the highlighter's
hot path.

So the thread is abandoned rather than stopped, and it spins until the process exits.
`mdmost` bounds the damage by counting abandoned threads and refusing to start more, which
is a cap on wasted cores, not a fix.

## Where it is

`syntect` 5.3.0, `src/parsing/parser.rs`:

- `parse_line` (line 212) returns `Result<Vec<(usize, ScopeStackOp)>, ParsingError>`.
- Its body drives `while self.parse_next_token(...)` (line 238). That loop has no
  iteration bound.
- The file's "Preventing loops" comment (line 140) documents the one case that *is*
  detected: a non-consuming push followed by a pop that would return to the same
  position. It mirrors what Sublime Text does. It does not cover the observed hang.

## Proposed shape

An optional maximum number of tokens per line on `ParseState`, defaulting to unlimited so
that no existing caller changes behaviour. On exceeding it, `parse_line` returns an error
rather than continuing.

`ParsingError` is `#[non_exhaustive]`, so adding a variant is not a breaking change.

The limit belongs on `ParseState` rather than on a `parse_line` argument: the existing
signature is the one every caller uses, including `HighlightLines`.

## Cheaper escapes, all closed (checked 2026-09-22)

Four ways to avoid the hang without a parser change were examined. None is available.

**Bundle the fixed `JavaScript.sublime-syntax` in `EXTRA_SYNTAXES`.** The mechanism would
work — `find_in_sets` probes the extra set first, so a definition there shadows the
bundled one. The file does not load. Today it is `extends: JavaScript (Plain)` plus
`meta_prepend` overrides with no `main` context, and `syntect` 5.3.0's `yaml_load.rs`
knows neither `extends` nor `version`, so it returns `ParseSyntaxError::MainMissing`.
Loading the parent instead fails *silently*: it holds 12 `branch_point`s, and 5.3.0's
`MatchOperation` has no `Branch` variant, so every branch rule becomes a zero-width no-op
with no error. The fix in Packages#3257 *is* a branch — the missing feature and the fix
are the same thing. It would also need three files, not one, and numeric `pop: N`, which
5.3.0 ignores.

**Depend on `syntect` master.** Branch support merged upstream as PR #614 on 2026-03-27,
six months after v5.3.0 (2025-09-27), and is unreleased. A git dependency would block
publication to crates.io.

**Map the tag to `JavaScript (Babel)`, which does not loop.** This is what `bat` and
docs.rs do, and it is why almost nobody sees the hang. `two-face` marks `JavaScript
(Babel)` as excluded from the `fancy-regex` build (`two-face` 0.5.2 `src/lib.rs:107`, the
`*` legend entry). It exists only in the `onig` set, which needs a C toolchain. Not
available to this project.

**Wait for `syntect` PR #706**, which fixes this exact loop. Open and unreviewed since
2026-09-12; no CI has run on it. Nothing in that repository has merged since May 2026,
though PRs merged in 1-9 days through April, and three collaborators still have write
access and comment. Attention continues; merging stopped.

So the guard is load-bearing, not precautionary.

## What it retires in `mdmost`

`highlight_with` already treats a parse error as "not highlightable" —
`state.parse_line(raw, set).ok()?` falls through to `plain`. So a hang that became an
error would need no new handling at the call site, and would make unnecessary:

- the worker thread whose only purpose is to be abandonable,
- the per-block time budget and its measured constants,
- the abandoned-thread cap,
- the frame caption that tells a reader why one block is not coloured.

## What is not known

Which rule in the JavaScript definition causes the loop. Whether a step budget is the
shape upstream would accept, or whether they would prefer the specific looping rule
found and fixed in the syntax definition instead. The two are not exclusive: a budget
bounds every future case, a rule fix addresses this one.
