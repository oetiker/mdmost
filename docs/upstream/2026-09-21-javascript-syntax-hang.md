# A JavaScript fence that never finishes highlighting

## Reproducer

```
  | { type: "a" }
  /** x */
```

Two lines, fenced as `js` (or any tag `mdmost` resolves to the same syntax — see
below). Parsing the second line does not return.

## Versions

- `syntect` 5.3.0, `default-fancy` feature (the pure-Rust `fancy-regex` backend;
  `mdmost` never links `syntect-onig`, which needs a C toolchain and is out of scope for
  this project's static musl builds).
- `two-face` 0.5.2+bat-0.26.1, `syntect-fancy` feature.

Both are the exact versions locked in this repository's `Cargo.lock` at the time of
writing.

## What was measured

Under the `fancy-regex` backend `mdmost` ships, calling `ParseState::parse_line` on the
reproducer's second line does not return: 180 s of CPU time, resident memory flat at
2 MB, no observable progress. The hang is inside that one call — nothing checked between
lines is ever reached, and the two-line input is far too small for a size-based guard to
have caught it.

The same two lines were also tried against a `syntect` build linked with `oniguruma`
(`syntect-onig`) — a separate, standalone build outside `mdmost`, since `mdmost` does not
build against that backend at all — selecting the syntax by name. It hangs there too.
That is why this is attributed to the syntax definition itself and not to `fancy-regex`:
two independent regex engines both fail to finish on the same input, driven by the same
`.sublime-syntax` rules.

`two-face` ships two curated syntax sets, one per backend, from the same upstream
curation. `mdmost`'s own token resolution (`highlight::resolve_syntax`, no alias for
`js`) resolves the bare tag `js` to the syntax named `JavaScript` in the `fancy` set that
`mdmost` actually links. `two-face`'s own crate documentation lists a second, separate
syntax, `JavaScript (Babel)`, and marks it as excluded from the `fancy-regex` build
specifically (present only under `onig`). This confirms the two backends' catalogues are
not identical for JavaScript-family syntaxes; which of the two entries the `onig`-linked
build's token lookup returns for the bare tag `js` was not independently checked as part
of this report — that check would need building against `oniguruma`, which this project
does not do.

## What was not measured

No rule inside the `.sublime-syntax` definition was identified as the cause by this
report. See the section below, added later.

## Root cause, found upstream (2026-09-22)

`syntect` PR [#706](https://github.com/trishume/syntect/pull/706), open and unreviewed
since 2026-09-12, reports the same hang from a reproducer of the same shape (`x /*`) and
diagnoses it: `expression-statement-continuation` matches empty, because the `/` of `/*`
is in its operator class, and the rule for "rest of the line is blanks and comments, then
end of line" also matches empty. The two hand the position back to each other.

The loop is not caught because `parse_next_token`'s guard remembers only a non-consuming
**push**. A `set` leaves the stack depth unchanged, so it neither arms nor trips the
guard. `parser.rs` says as much in the comment beside the guard. That is engine-
independent, which is why this report's two backends both hang.

It also explains why the merged stack-depth cap (PR #597, a limit of 100 pushes) does not
fire here: this loop never grows the stack.

Related upstream reports of the same class, all open: issue #460 (the same JavaScript
symptom, reported 2023-06-21, never root-caused), #650 (Perl POD), #656 (non-consuming
multi-context push).

This report's reproducer was posted upstream on 2026-09-22, on PR #706
([comment](https://github.com/trishume/syntect/pull/706#issuecomment-5774175118)) and on
issue #460 ([comment](https://github.com/trishume/syntect/issues/460#issuecomment-5774175313)),
the second linking #460 to #706 as its likely root cause. No new issue was opened, and
nothing about a step budget was raised there — that belongs on issue #202. Do not post
this again.
