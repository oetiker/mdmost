# `pulldown-latex`, vendored

This directory is a copy of someone else's crate. It is **not** mdmost code, it is not
maintained here, and it is meant to be deleted rather than developed. Read this file
before changing anything under it.

## What is here

- Upstream: <https://github.com/carloskiki/pulldown-latex> (the manifest still names the
  account's old spelling, `Carlosted`; GitHub redirects).
- Our fork: <https://github.com/oetiker/pulldown-latex>.
- Vendored from branch `mdmost-integration`, commit
  **`cf0b98d138e6ce06a083aa9db25acb4d500bcb55`**, 2026-08-24. That is `v0.7.1-83-gcf0b98d`:
  upstream `main` at `1067fd2` (2026-08-21), which is past the 0.8.0 release the version
  field claims, plus the four merges below.
- Licence: MIT. `LICENSE` is copied with the code because the licence requires it.

## Patches 1-4: defects, each an upstream branch

Each is an independent branch off upstream `main`, merged into `mdmost-integration`.
Every one of them is a defect mdmost hit with real documents.

| Branch | Commit | Upstream PR | What it fixes |
| --- | --- | --- | --- |
| `fix/argument-recursion-depth` | `6e10f81` | [#73](https://github.com/carloskiki/pulldown-latex/pull/73) — **closed unmerged** | A chain of unbraced control-sequence arguments (`\sqrt\sqrt…x`) recurses without bound and **aborts the process** with a stack overflow. That is not a panic, so `catch_unwind` never sees it. A depth counter in `handle_primitive` refuses first. |
| `fix/next-tail-recursion` | `49effe3` | [#74](https://github.com/carloskiki/pulldown-latex/pull/74) — open | Same abort, different route: a run of tokens that emit no event (`\relax`, `%` comments) recurses through `Parser::next` and `lex::token`. Both are driven by a loop instead. |
| `fix/optional-argument-extent` | `357ce1d` | [#75](https://github.com/carloskiki/pulldown-latex/pull/75) — open | An unbraced optional argument loses its extent, so `\sqrt[n+1]{x}` draws as the nth root, plus one. The radical index is emitted as a group. |
| `fix/newcommand-optional-arg-count` | `74b8219` | [#76](https://github.com/carloskiki/pulldown-latex/pull/76) — open | `\newcommand{\R}{\mathbb{R}}` is rejected — valid LaTeX, mdmost's design spec §16.3's own example, and the spelling every author writes — because the `[n]` parameter count is mandatory where LaTeX defaults it to zero. Read as 0 when absent. |

**#73 was closed unmerged on 2026-08-22, with no comment and no review.** #74, #75 and #76
were still open when this was vendored. Treat none of the four as certain to land.

Two of the four **change what parses or renders**, so this tree is load-bearing for
mdmost's own test suite, not merely a hardening measure: `src/math/build.rs`'s
`\sqrt[n+1]{x}` assertion and `src/math/tests.rs`'s bare `\newcommand{\R}{\mathbb{R}}`
case hold only with `fix/optional-argument-extent` and
`fix/newcommand-optional-arg-count` applied.

## Patch 5: two visibility widenings, local, no upstream PR

**Added 2026-09-11, Task 15b. This one is not a defect and is not on a branch of the
fork** — it is two words of `pub` in this tree, made by hand, and it must be re-applied by
hand if this directory is ever re-synced.

| File | Item | Was | Is |
| --- | --- | --- | --- |
| `src/mathml.rs` | `Font::map_char` | private | `pub` |
| `src/event.rs` | `Grouping::is_math_env` | `pub(crate)` | `pub` |

**No body was touched, moved or rewritten**, and the three `unsafe` blocks in this tree are
untouched. Each item gained a doc comment saying the `pub` is ours and pointing here.

Why: mdmost draws `\mathbb{R}` as `ℝ` on a terminal cell. `map_char` is the table that
says which code point that is — the same table this crate's own MathML writer uses — and
the alternative was a second copy of it in `src/math/build.rs`, which would drift. The
owner ruled that the parser's table is the source (plan Task 15b). `is_math_env` is the
same argument for the *scoping*: a font change applies to the end of its group, and a math
environment is the one group that does not inherit one, which is a fact only this crate
knows.

Neither changes what parses or what this crate renders; `cargo test -p pulldown-latex`
is unaffected by them. **No upstream PR was opened**, per the plan: widening visibility for
one consumer's convenience is a different kind of ask from the four defect fixes above, and
whether to make it is the owner's call rather than this task's.

`MAX_COMMAND_RUN` and `MAX_SOURCE_BYTES` in `src/math/build.rs` stay regardless of what
happens here. They refuse before the parser runs, so they do not depend on which parser
is underneath.

## This vendor is temporary

When upstream cuts a release carrying these fixes:

0. **Patch 5 blocks this step until it is answered.** `src/math/build.rs` calls
   `Font::map_char` and `Grouping::is_math_env`, and a crates.io `pulldown-latex` exports
   neither. Deleting `vendor/` therefore stops mdmost compiling until either upstream makes
   them public or `build.rs` grows a table of its own — which the owner ruled against.
1. Delete `vendor/` entirely.
2. Restore a plain version requirement in the root `Cargo.toml`:
   `pulldown-latex = { version = "…", default-features = false }`.
3. Re-scope the gate and the CI steps that name `-p mdmost` (see below) back to plain
   workspace-wide invocations, and drop the `[workspace]` table.
4. The crates.io question reopens — a path dependency is the reason mdmost cannot be
   published, and that reason disappears with this directory. Whether to publish again
   is the owner's call, not a consequence of this step.

## What was dropped from the upstream tree, and why

Kept: `src/`, `docs/usage.md` (`src/lib.rs` pulls it in with `include_str!`, so the crate
does not compile without it), `LICENSE`, `README.md`, and three test files.

| Dropped | Why |
| --- | --- |
| `tests/cross-browser.rs` | Drives a real browser through `fantoccini` + `tokio` and a live WebDriver. Not an offline test. |
| `tests/{wikipedia,mozilla,latexml,fonts,miscellaneous}.rs`, `tests/common/`, `tests/out/`, `styles.css` | Corpus round-trip suites built on `harness = false` plus `libtest-mimic`, `inventory`, `heck` and `anyhow`, whose other half is emitting HTML pages for a human to look at. Keeping them would add five dev-dependencies (and, through `libtest-mimic`, `clap` and the `anstyle` family) to this workspace for suites that assert only "parses, and renders without error". |
| `benches/` | `criterion`. |
| `fuzz/` | A separate `cargo-fuzz` crate with its own manifest and toolchain requirement. |
| `site/`, `font/`, `examples/`, `CHANGELOG.md`, `.github/`, `.cargo/`, `.gitignore` | Serve a full upstream checkout; nothing in the library reads them. |
| `Cargo.lock` | The workspace root's lockfile governs a path dependency. |

**The capability lost is the corpus smoke net** — several thousand Wikipedia, Mozilla and
LaTeXML formulas parsed and rendered on every run. That was the broadest check on
`fix/next-tail-recursion`, which rewrote the control flow of `Parser::next` and
`lex::token`. What replaces it: the three kept test files carry the regression tests the
four patches themselves added (`tests/errors.rs` and `tests/mathml.rs` are where every one
of them landed), and mdmost's own math suite exercises this parser through `src/math/`.
If this tree is ever re-synced to a newer upstream, run the full suite in a checkout of
the fork before copying — that is one `git clone` away, and it is the right place for it.

## The manifest is not upstream's verbatim

`Cargo.toml` here is upstream's with the benches, the example, the six removed test
targets and their dev-dependencies stripped, plus two additions: `publish = false`, and a
`[lints]` table that allows everything.

On the lints table, the honest version: **it fixes nothing today.** Measured on
2026-08-24 with rustc/clippy 1.96.0, this tree is clean under
`cargo clippy --all-targets --workspace -- -D warnings` with the table removed — upstream
runs clippy in its own CI too. The table is insurance against the toolchain bump that
introduces a lint nobody here is going to fix in someone else's frozen code.

What is load-bearing is the other half: mdmost's gate names its package,
`cargo clippy --all-targets -p mdmost -- -D warnings`, and trailing args reach only the
selected package. Were the gate workspace-wide, `-D warnings` would override this table
and lint upstream's code under our settings anyway. **Neither half can reach mdmost's own
code** — a `[lints]` table applies to the package that declares it. If this tree ever
stops being read-only, delete the `[lints]` table rather than the scoping.
