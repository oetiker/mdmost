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

No rule inside the `.sublime-syntax` definition has been identified as the cause. Nobody
has gone looking, and this report does not speculate about which rule it is.
