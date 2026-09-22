# `syntect`, vendored

This directory is a copy of someone else's crate. It is **not** mdmost code, it is not
maintained here, and it is meant to be deleted rather than developed. Read this file
before changing anything under it.

## What is here

- Upstream: <https://github.com/trishume/syntect>.
- Version: **5.3.0**. This must stay exactly `5.3.0` — `two-face` requires `^5.3.0` of
  `syntect`, and the root `Cargo.toml`'s `[patch.crates-io]` table only applies while the
  vendored version satisfies that. A pre-release or any other version makes cargo ignore
  the patch table in silence and compile two incompatible `syntect` crates; `tests/vendoring.rs`
  exists to catch that.
- **The reference to diff against is upstream git tag `v5.3.0`
  (commit `e4670846ecf16d8832db6c43d531bec466214e27`), not the crates.io tarball.**
  The tarball is close — `src/` and `Cargo.toml.orig` are byte-identical to the tag,
  confirmed with `diff -rq` — but the tarball's publisher-side `exclude` list drops
  `testdata/*` entirely, and this tree needs part of `testdata/` to run syntect's own
  tests (see below). A future re-sync should start from the git tag for the release
  being picked up, not from `~/.cargo/registry/src/.../syntect-<version>/`.
- Licence: MIT. `LICENSE.txt` is copied with the code because the licence requires it.

## The patches

None yet. This task (vendoring) adds no patches — the point of doing it first is that if
anything breaks in a later task, it is provably a patch and not the vendoring. Two are
coming:

- **Patch 1** answers upstream PR [#706](https://github.com/trishume/syntect/pull/706) by
  `tontinton`. Not this project's own work in origin.
- **Patch 2** answers upstream issue
  [#202](https://github.com/trishume/syntect/issues/202). Also not this project's own
  work in origin.

Each will get its own row here, in the same shape as `vendor/pulldown-latex/VENDORED.md`'s
patch table, when it lands.

## This vendor is temporary

Per the design spec's exit condition (spec §11): when upstream cuts a release that
carries both patches (or otherwise removes the need for the step-budget guard), the exit
procedure is:

1. Delete `vendor/syntect/` entirely.
2. Restore a plain version requirement in the root `Cargo.toml`:
   `syntect = { version = "…", default-features = false, features = ["default-fancy"] }`,
   and drop the `[patch.crates-io]` table and the `"vendor/syntect"` workspace member.
3. Delete `tests/vendoring.rs` and the CI steps that reference it
   (`.github/workflows/ci.yml`: "Run the vendored syntect's tests", "One syntect only").
4. Confirm both patches are actually present in the picked-up release before deleting —
   grep the new dependency's source for the guarded step count and the bounded-recursion
   check `parser.rs` carries once Tasks 2 and 3 land, don't just trust the version number.

## What was dropped from the upstream tree, and why

Kept: `src/`, `assets/` (748 KB of `.packdump` files reached by `include_bytes!` behind
`default-syntaxes` and `default-themes`, which `default-fancy` enables — the crate does
not compile without them), `LICENSE.txt`, `Readme.md`, `tests/error_handling.rs`, and
part of `testdata/` (below).

| Dropped | Why |
| --- | --- |
| `benches/` | `criterion`. |
| `examples/` (8 files) | `getopts`, `rayon`, `regex`, `serde_json` as binaries; none is library code. |
| `tests/public_api.rs`, `tests/snapshots/` | Needs `public-api`, `rustdoc-json` and `rustup-toolchain`, which spawn a second toolchain. Not an offline test. |
| `Cargo.lock`, `Cargo.toml.orig`, `.cargo_vcs_info.json`, `.cargo-ok` | The workspace root's lockfile governs a path dependency. |
| `.github/`, `.gitignore`, `.gitattributes`, `.git-blame-ignore-revs`, `CHANGELOG.md`, `DESIGN.md` | Serve a full upstream checkout; nothing in the library reads them. |
| `.gitmodules` and the four submodules it declares (`testdata/Packages` from `sublimehq/Packages`, `testdata/Solarized`, `testdata/spacegray`, `testdata/InspiredGitHub.tmtheme`) | Real Sublime Text package and theme definitions from independently-changing third-party repositories under their own, separate licences — `sublimehq/Packages` alone is ~18 MB across dozens of packages, and GitHub resolves its aggregate licence as `NOASSERTION` (no single identifiable licence). Committing that into a directory this vendoring's own exit plan deletes within a year (see above) is a poor trade at any size. Hand-picking just the files the affected tests open was tried and rejected too: `.sublime-syntax` grammars reference each other by scope, not by the file path a test names, so a fixture that names no submodule path in its own source can still fail to resolve a scope that only exists inside one — confirmed by `html::tests::tricky_test_syntax`, below. The empty submodule directories the tarball checkout leaves behind were removed rather than kept as placeholders: an empty directory that looks like missing content is worse than an absent one. **Not vendored, but not skipped either — see "Two ways to run syntect's tests" below: CI fetches these same four repositories at runtime, at the exact commits `v5.3.0` pins them to, and nothing is redistributed by this repository.** |

**The capability at risk, and why it isn't actually lost:** 16 of syntect's own inline
`#[test]` functions need content from those four submodules. Seven of them are in
`parsing::parser::tests` — `can_parse_simple`, `can_parse_includes`, `can_parse_backrefs`,
`can_parse_issue25`, `can_parse_preprocessor_rules`, `can_parse_yaml`,
`can_compare_parse_states` — which is **the exact file both syntect patches modify**.
Leaving those 16 permanently skipped would darken the regression net around the two
patches' most important file, which is not acceptable. So CI does not skip them: it
fetches the four submodules fresh on every run (below) and runs the complete, unmodified
suite. The skip list exists only as the *local* fallback for a working tree that hasn't
fetched them — see the next section for both invocations and which is which.

### Two ways to run syntect's tests

**In CI:** fetch each submodule at the exact commit `v5.3.0`'s own tree pins it to
(`git ls-tree v5.3.0 testdata` in a checkout of the tag — the `160000` entries are the
submodules, and the SHA on each line is the pin), then run the suite with no `--skip` at
all:

```sh
cd vendor/syntect/testdata
for spec in \
  "Packages https://github.com/sublimehq/Packages fa6b8629c95041bf262d4c1dab95c456a0530122" \
  "Solarized https://github.com/braver/Solarized.git bcd6234b4f5f96d3fd27db079268b5757053072a" \
  "spacegray https://github.com/kkga/spacegray.git 2703e93f559e212ef3895edd10d861a4383ce93d" \
  "InspiredGitHub.tmtheme https://github.com/sethlopezme/InspiredGitHub.tmtheme.git 18ddb271179e118cfc2dd83abf88b915b7328a25"
do
  set -- $spec
  mkdir "$1" && cd "$1" && git init -q && git fetch --depth 1 "$2" "$3" && git checkout -q FETCH_HEAD && rm -rf .git && cd ..
done
cd -
cargo test -p syntect   # 106 lib + 8 error_handling + 13 doctests = 127 passed, 0 failed
```

A branch name would move; the pinned commit will not. If any fetch fails, this step
fails and stays failed — there is deliberately no fallback to the skip list, so a broken
mirror or a network outage is loud rather than a silent drop to a weaker regression net.
The exact shape of this runs in `.github/workflows/ci.yml`'s "Fetch syntect's test
fixtures" and "Run the vendored syntect's tests" steps.

**Locally, with no setup:** the same command with the 16 named explicitly, since without
the fetch above they cannot pass:

```sh
cargo test -p syntect -- \
  --skip highlighting::highlighter::tests::can_parse \
  --skip highlighting::highlighter::tests::can_parse_with_highlight_state_from_cache \
  --skip highlighting::highlighter::tests::test_ranges \
  --skip highlighting::theme_set::tests::can_parse_common_themes \
  --skip html::tests::strings \
  --skip html::tests::tricky_test_syntax \
  --skip parsing::parser::tests::can_compare_parse_states \
  --skip parsing::parser::tests::can_parse_backrefs \
  --skip parsing::parser::tests::can_parse_includes \
  --skip parsing::parser::tests::can_parse_issue25 \
  --skip parsing::parser::tests::can_parse_preprocessor_rules \
  --skip parsing::parser::tests::can_parse_simple \
  --skip parsing::parser::tests::can_parse_yaml \
  --skip parsing::syntax_set::tests::can_load \
  --skip dumps::tests::can_dump_and_load \
  --skip dumps::tests::dump_is_deterministic
# 90 lib + 8 error_handling + 13 doctests = 111 passed, 16 skipped, 0 failed
```

**This local skip list is a convenience for a working tree without the fixtures, not
this project's position on those 16 tests — CI's full run is.** A future re-syncer should
treat the CI invocation as authoritative and this one as what to reach for only when
offline or iterating quickly.

**The skip list below was derived by running the suite and reading its failures, not by
reading the tests' source, and a re-sync must redo it the same way.** `.sublime-syntax`
grammars reference each other by scope, and a test's own source does not always say
which scope it needs: `html::tests::tricky_test_syntax` opens a local fixture,
`testdata/testing-syntax.testsyntax`, which names no submodule path at all — but that
fixture's grammar extends `text.html.basic`, which exists only inside the (unvendored)
`Packages/HTML/` submodule, so the test fails with `UnresolvedContextReference` rather
than a missing-file error. Reading the test source here would have missed the
dependency entirely; only running it against a tree with the submodules absent, and
reading what broke, found it.

### The 16 skipped tests (locally; CI runs all of them)

| Test | Fixture it needs | Why it fails here |
| --- | --- | --- |
| `highlighting::highlighter::tests::can_parse` | `Packages/Ruby on Rails` | `find_syntax_by_name("Ruby on Rails")` on an empty set |
| `highlighting::highlighter::tests::can_parse_with_highlight_state_from_cache` | `Packages/Python` | `find_syntax_by_scope(source.python)` on an empty set |
| `highlighting::highlighter::tests::test_ranges` | `Packages/…` (via `testdata::PACKAGES_SYN_SET`) | same shared static, poisoned once any of its users panics during init |
| `highlighting::theme_set::tests::can_parse_common_themes` | `testdata/spacegray/base16-ocean.dark.tmTheme` | file does not exist |
| `html::tests::strings` | `testdata/Packages/Rust/Cargo.sublime-syntax` | file does not exist |
| `html::tests::tricky_test_syntax` | `Packages/HTML/HTML.sublime-syntax` (`text.html.basic`) | the *local* fixture `testdata/testing-syntax.testsyntax` extends this scope; it is not named in the test's own source |
| `parsing::parser::tests::can_compare_parse_states` | `testdata::PACKAGES_SYN_SET` | as above |
| `parsing::parser::tests::can_parse_backrefs` | `testdata::PACKAGES_SYN_SET` | as above |
| `parsing::parser::tests::can_parse_includes` | `testdata::PACKAGES_SYN_SET` | as above |
| `parsing::parser::tests::can_parse_issue25` | `testdata::PACKAGES_SYN_SET` | as above |
| `parsing::parser::tests::can_parse_preprocessor_rules` | `testdata::PACKAGES_SYN_SET` | as above |
| `parsing::parser::tests::can_parse_simple` | `testdata::PACKAGES_SYN_SET` | as above |
| `parsing::parser::tests::can_parse_yaml` | `testdata::PACKAGES_SYN_SET` | as above |
| `parsing::syntax_set::tests::can_load` | `Packages/Rust/Rust.sublime-syntax` (via `testdata::PACKAGES_SYN_SET`) | as above |
| `dumps::tests::can_dump_and_load` | `testdata::PACKAGES_SYN_SET` | as above |
| `dumps::tests::dump_is_deterministic` | `testdata::PACKAGES_SYN_SET` | as above |

`testdata::PACKAGES_SYN_SET` (`src/utils.rs`) is a `LazyLock<SyntaxSet>` built once per
test run from `SyntaxSet::load_from_folder("testdata/Packages")`. With that directory
absent, the load itself fails and the `LazyLock`'s initializer panics; every other test
that touches the same static afterward fails too, with "previously been poisoned" rather
than its own error. That is why 10 of the 16 name the same fixture: they are not 10
independent gaps, they are one missing directory reached from 10 call sites.

### Third-party content kept anyway, and its licence

Two files under `testdata/` are real third-party source, kept as syntax-highlighting
fixtures because upstream ships them and dropping them was not asked for:

- `testdata/jquery.js` — jQuery JavaScript Library v2.1.4, Copyright jQuery Foundation,
  Inc. and other contributors. MIT licensed (header in the file).
- `testdata/parser.rs` — an old rustc `libsyntax` parser, Copyright The Rust Project
  Developers. Dual Apache-2.0/MIT licensed (header in the file).

Neither is executed; both are read as text by syntax-highlighting tests exercising real
source code.

## The manifest is not upstream's verbatim

`Cargo.toml` here is upstream's with the benches, the eight examples, the `public_api`
test target and the dev-dependencies those needed stripped (`criterion`, `getopts`,
`public-api`, `regex`, `rustdoc-json`, `rustup-toolchain`), plus:

- `publish = false`, in `[package]` — a path dependency cannot be published, and this
  tree is not ours to publish anyway.
- `[features] default`, changed from upstream's `default-onig` to **`default-fancy`**.
  `default-onig` pulls in `onig` → `onig_sys`, whose build-dependencies are `cc` and
  `pkg-config` — a C toolchain. "No C toolchain in the build, ever" is a load-bearing
  decision in the root `Cargo.toml` (mdmost's own musl static build depends on it), and
  mdmost's product dependency line already pins `default-features = false, features =
  ["default-fancy"]`, so the product build was never affected. But this manifest's own
  default still named `default-onig` until review caught it: a bare `cargo test -p
  syntect` — the exact command CI runs and this file tells a developer to run — built
  `onig` regardless of what mdmost itself asks for, because features requested by a
  dependent add to a crate's own defaults rather than replacing them, and nothing in
  `cargo test -p syntect` names any feature at all. Changed the manifest rather than
  adding `--no-default-features --features default-fancy` at each call site, for three
  reasons: it closes the hole for every invocation, including one nobody wrote a flag
  for; it makes syntect's own suite test the exact configuration mdmost ships, which is
  a better test and not merely a safer one; and this section already exists for
  documenting exactly this kind of change. `tests/vendoring.rs` now asserts `cargo tree
  -p syntect`'s output carries no `onig`, so a re-sync that reverts this reintroduces a
  failing test rather than a silent regression.

  **This has a real runtime cost, measured, not estimated.** `syntect`'s own lib tests
  (`cargo test -p syntect --lib`, fixtures fetched, the CI path) took **~4s under
  `default-onig`** and **~119s under `default-fancy`**, on this machine, in this
  session, `CARGO_TARGET_DIR` shared with the rest of Task 1's runs. The two numbers are
  sequential before/after runs, not an interleaved A/B comparison, and this machine runs
  many concurrent sessions sharing 128 cores, so treat the ~30x gap as the right order
  of magnitude rather than a precise ratio a re-run would reproduce exactly. It is
  `fancy-regex` (pure Rust) against `oniguruma` (C, `onig_sys`) on the same suite — most
  of that suite loads and matches real Sublime `.sublime-syntax` regexes across hundreds
  of files, exactly the workload the two engines differ most on — and it is not a
  correctness change: the CI path's test count was 127 passed, 0 failed either way. It
  is also CI step time on this vendored crate's own suite, not mdmost's runtime: nothing
  in mdmost's own product code path changed, since mdmost's dependency line already
  pinned `default-fancy` before this fix existed.
- A `[lints]` table that allows everything:

  ```toml
  [lints.rust]
  warnings = "allow"

  [lints.clippy]
  all = "allow"
  ```

  Insurance against a toolchain bump that introduces a lint nobody here is going to fix
  in someone else's frozen code. It is not what keeps mdmost's gate off this crate: a
  lint level named on the command line (`-D warnings`) overrides a crate's own `[lints]`
  table rather than the other way around, so `cargo clippy -p mdmost -- -D warnings`
  still fails on this crate's code without `--no-deps` — confirmed directly against
  `yaml_load.rs:818`'s `mismatched_lifetime_syntaxes` lint on the current toolchain,
  which this table alone did not silence once `-D warnings` was on the command line.
  `--no-deps` is what mdmost's gate actually relies on (`.github/workflows/ci.yml`,
  `README.md`). This table's real effect: a plain `cargo clippy -p syntect`, with no
  extra flags, stays silent. If this tree ever stops being read-only, delete the table
  rather than the `--no-deps` scoping.

Kept as dev-dependencies, deliberately: `pretty_assertions` (`src/lib.rs` has
`#[cfg(test)] #[macro_use] extern crate pretty_assertions`), `rayon` (the
`can_use_in_multiple_threads` test in `src/parsing/syntax_set.rs`) and `serde_json` (both
an optional real dependency and a dev-dependency upstream declares separately).
