# Bounded `syntect` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every `syntect` parse finite, so no document content can hang `mdmost`, and remove the worker-thread guard that existed only because a parse could run forever.

**Architecture:** `syntect` 5.3.0 is vendored at `vendor/syntect/` and reached through `[patch.crates-io]` so that `two-face` resolves to the same copy. It carries two patches: upstream PR #706's fix for loops between non-consuming `set`s, and a new `parse_line_with_limit` that returns an error when a line exceeds a token budget. `highlight_with` already falls through to `plain` on a parse error, so the mdmost side is a deletion plus one new outcome value that lets a failed block caption its own frame.

**Tech Stack:** Rust 2024, `syntect` 5.3.0 with `default-fancy` (`fancy-regex`, no C toolchain), `two-face` 0.5.2+bat-0.26.1, `ratatui`. Cargo workspace with path-dependency vendoring.

**Spec:** `docs/superpowers/specs/2026-09-22-bounded-syntect-design.md`

## Global Constraints

- **No C toolchain in the build, ever.** `fancy-regex` only; never `syntect-onig`. This keeps the static musl builds working.
- **4-core cap on every cargo invocation.** Use `cargo test -j4`, `cargo build -j4`. This machine has 128 cores and is shared.
- **Every session here shares one 25 GiB memory cgroup.** Any test on unbounded or adversarial input runs under `systemd-run --user --scope -p MemoryMax=2G --`.
- **Give this work its own `CARGO_TARGET_DIR`**, e.g. `/scratch/oetiker/cargo-target-mdmost-syntect`.
- **The vendored crate's version stays `5.3.0`.** `two-face` requires `^5.3.0`; a pre-release such as `5.3.1-mdmost` does **not** satisfy a caret requirement, and `[patch.crates-io]` would be silently ignored, producing two `syntect` crates with incompatible `SyntaxSet` types.
- **Keep every `syntect` feature name.** `two-face` selects `dump-load`, `parsing` and `regex-fancy` with `default-features = false`; mdmost selects `default-fancy`, which expands to `parsing`, `default-syntaxes`, `default-themes`, `html`, `plist-load`, `yaml-load`, `dump-load`, `dump-create`, `regex-fancy`.
- **Nothing in `vendor/syntect/` may be a local-only change.** Every patch must be one upstream could take. This is what keeps the exit in spec §11 unblocked. The one tempting exception — deleting `FontStyle::from_bits_unchecked` — is explicitly **not** made.
- **The version is bumped only in a `Release vX` commit, never in feature work.**
- **`main` advances only through a PR merged on origin.** Never push local `main`.
- **Nothing may panic on document content.**
- **Commit messages end with:** `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`


## Amendments (Task 1, 2026-09-22)

Task 1's execution found defects in this plan. The rulings below override the task text
wherever they differ; the original text is left in place as the record of what was
argued, not as an instruction.

- **`[patch.crates-io]` is hyphenated.** The dotted `[patch.crates.io]` this plan
  originally wrote is a nested table cargo rejects. Corrected throughout, here and in the
  spec.
- **The duplicate guard is `cargo tree -p syntect`, not a grep.** Task 1 Step 1's and
  Step 8's `cargo tree --duplicates | grep -qv syntect` is inverted in both directions:
  `cargo tree --duplicates` already prints the word `syntect` today, on the dependent
  chains of unrelated duplicates, with one syntect compiled. `cargo tree -p <SPEC>` exits
  101 when a name matches several versions. `tests/vendoring.rs` asserts the exit status
  **and** that stdout names `vendor/syntect`, the second catching the patch table being
  ignored while the versions happen to agree.
- **The vendored `[lints]` tables carry real `allow` values**, following
  `vendor/pulldown-latex`'s pattern. Step 4's empty tables are inert, and the comment
  justifying them was wrong: `cargo clippy -p mdmost -- -D warnings` does reach
  `vendor/syntect`. `--no-deps` is the real scoping and is now in CI and the README. The
  verification list's clippy command gains `--no-deps`.
- **`testdata/` is restored from upstream git tag `v5.3.0`.** The crates.io tarball
  excludes it (`exclude = ["testdata/*", ...]` in upstream's own manifest) and syntect's
  tests do not compile without it. **The re-sync baseline is therefore tag `v5.3.0` minus
  the four `.gitmodules` submodules minus the dropped directories — not the crates.io
  tarball**, which the verification list at the end of this plan still names.
- **The four submodules are not vendored.** `sublimehq/Packages` alone is ~17.9 MB that
  GitHub resolves as `NOASSERTION`. CI fetches all four at the commits
  `git ls-tree v5.3.0 testdata` pins and runs the full 127-test suite with no `--skip`;
  `.gitignore` refuses to track the result. The 16-name skip list is a documented local
  command only. Seven of those 16 are in `parsing::parser::tests`, the file Tasks 2 and 3
  patch, which is why CI fetches rather than lives with the list.
- **Task 3's token-limit check is a nested `if let`, not a let-chain**, because the
  vendored crate is edition 2021. Corrected in Task 3 Step 3.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `vendor/syntect/` (create) | The vendored crate. Read-only except for the two patches. |
| `vendor/syntect/VENDORED.md` (create) | The contract: source version, each patch, what was dropped, and the exit procedure. Modelled on `vendor/pulldown-latex/VENDORED.md`. |
| `vendor/syntect/src/parsing/parser.rs` (modify) | Both patches live here and nowhere else. |
| `Cargo.toml` (modify) | `workspace.members`, `[patch.crates-io]`, and the `syntect` dependency comment. |
| `.github/workflows/ci.yml` (modify) | One new test step for the vendored crate; one `cargo tree -d` assertion. |
| `src/highlight.rs` (modify) | Set the limit; add `Outcome`; delete the thread guard and its constants. |
| `src/render/bridge.rs` (modify) | Expose the outcome to the renderer alongside the lines. |
| `src/render/code.rs` (modify) | `framed` → `framed_captioned` when the outcome is `Failed`. |
| `docs/maintainer-notes.md` (modify) | Replace the removed budget arithmetic with the measured token ceiling. |
| `CHANGES.md` (modify) | One user-visible entry. |

**Why `parser.rs` and nothing else:** both patches are in the token loop. Keeping the vendor's diff to one file is what makes a future re-sync to a newer upstream a readable three-way merge instead of an archaeology exercise.

---

## Task 1: Vendor `syntect` 5.3.0 unchanged, and prove there is only one of it

The whole task is "the build is byte-for-byte equivalent, but the crate now comes from `vendor/`". No behaviour changes. Doing this alone first means that if anything breaks later, it is a patch and not the vendoring.

**Files:**
- Create: `vendor/syntect/` (the stripped upstream tree)
- Create: `vendor/syntect/VENDORED.md`
- Modify: `Cargo.toml:10-11` (the `[workspace]` table) and the `syntect` dependency near line 108
- Modify: `.github/workflows/ci.yml:76-83`

**Interfaces:**
- Consumes: nothing.
- Produces: a `syntect` 5.3.0 at `vendor/syntect` that every workspace member resolves to. No new Rust API.

- [ ] **Step 1: Write the failing test — the duplicate-crate guard**

This is the test that catches the trap in the Global Constraints. Create `tests/vendoring.rs`:

```rust
//! The vendored `syntect` must be the only `syntect` in the build.
//!
//! `two-face` depends on `syntect` too. If `[patch.crates-io]` ever stops applying — a
//! version bump that no longer satisfies `two-face`'s `^5.3.0`, a pre-release version,
//! a stray `[dependencies.syntect]` with a `version` that disagrees — cargo silently
//! compiles two copies. They are different crates to the type system, so
//! `two_face::syntax::extra_newlines()` would return a `SyntaxSet` that
//! `syntect::parsing::ParseState::new` cannot accept, and the error message names the
//! same type twice. This test fails first instead.

use std::process::Command;

#[test]
fn exactly_one_syntect_is_compiled() {
    let out = Command::new(env!("CARGO"))
        .args(["tree", "--duplicates", "--quiet"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree should run");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.contains("syntect"),
        "cargo tree --duplicates reported more than one syntect:\n{text}"
    );
}
```

- [ ] **Step 2: Run it to make sure it fails for the right reason**

Run: `cargo test -p mdmost --test vendoring -j4`

Expected: PASS at this point, because there is only one `syntect` today — it comes from crates.io. That is fine and expected: this test is a guard for the rest of the task, not a red-to-green cycle. **Confirm it passes now**, so that if it fails after Step 5 you know the vendoring caused it.

- [ ] **Step 3: Copy the upstream tree in, stripped**

The source is the crates.io tarball already unpacked in the local registry. Copy it, then remove what is not load-bearing.

```bash
SRC=~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/syntect-5.3.0
mkdir -p vendor/syntect
cp -R "$SRC"/src vendor/syntect/
cp -R "$SRC"/assets vendor/syntect/
mkdir -p vendor/syntect/tests
cp "$SRC"/tests/error_handling.rs vendor/syntect/tests/
cp "$SRC"/LICENSE.txt "$SRC"/Readme.md vendor/syntect/
cp "$SRC"/Cargo.toml vendor/syntect/Cargo.toml
```

**Keep `assets/`.** It is 748 KB of `.packdump` files reached by `include_bytes!` behind the `default-syntaxes` and `default-themes` features, which `default-fancy` enables. The crate does not compile without them.

**What is deliberately not copied, and why:**

| Not copied | Why |
| --- | --- |
| `benches/` | `criterion`. |
| `examples/` (8 files) | `getopts`, `rayon`, `regex`, `serde_json` as binaries; none is library code. |
| `tests/public_api.rs`, `tests/snapshots/` | Needs `public-api`, `rustdoc-json` and `rustup-toolchain`, which spawn a second toolchain. Not an offline test. |
| `Cargo.lock`, `Cargo.toml.orig`, `.cargo_vcs_info.json`, `.cargo-ok` | The workspace root's lockfile governs a path dependency. |
| `.github/`, `.gitignore`, `.gitattributes`, `.gitmodules`, `.git-blame-ignore-revs`, `CHANGELOG.md`, `DESIGN.md` | Serve a full upstream checkout; nothing in the library reads them. |

**What is kept, and why:** `tests/error_handling.rs` uses only `std` and `syntect`, so it costs no dev-dependency. The parser's real tests are inline in `src/parsing/parser.rs` — 41 `#[test]` functions — and that is where Task 2's and Task 3's tests go.

- [ ] **Step 4: Edit the vendored manifest**

Strip the removed targets' dev-dependencies and add the two local keys. Keep `pretty_assertions` (`src/lib.rs:26` has `#[cfg(test)] #[macro_use] extern crate pretty_assertions`), `rayon` (the `can_use_in_multiple_threads` test in `src/parsing/syntax_set.rs:1201`) and `serde_json` (both an optional real dependency at `[dependencies.serde_json]` and a dev-dependency).

Remove these `[dev-dependencies.*]` tables: `criterion`, `getopts`, `public-api`, `rustdoc-json`, `rustup-toolchain`, `regex`. Remove the `[[bench]]` and `[[example]]` tables. Then add:

```toml
publish = false

# Allows everything, on purpose. mdmost's gate names its package —
# `cargo clippy --all-targets -p mdmost -- -D warnings` — and trailing args reach only
# the selected package, so this table is insurance against a toolchain bump that
# introduces a lint nobody here is going to fix in someone else's frozen code. A
# `[lints]` table applies only to the package declaring it, so neither half can reach
# mdmost's own code. If this tree ever stops being read-only, delete this table rather
# than the scoping.
[lints.rust]
[lints.clippy]
```

Leave `version = "5.3.0"` exactly as it is. See the Global Constraints for what a pre-release would do.

- [ ] **Step 5: Wire the workspace and the patch table**

In the root `Cargo.toml`, extend the existing `[workspace]` table (currently at lines 10-11) and add the patch table immediately after it:

```toml
[workspace]
members = ["vendor/pulldown-latex", "vendor/syntect"]

# `two-face` depends on `syntect` as well, so a plain path dependency would compile two
# copies whose `SyntaxSet` types do not interoperate — `two_face::syntax::extra_newlines()`
# would return a type `ParseState::new` refuses, and the error would name `SyntaxSet`
# twice. `[patch]` redirects every requirement in the graph to one copy.
#
# The patch applies only while the vendored version satisfies what two-face asks for,
# `^5.3.0`. A pre-release version would not, and cargo would ignore this table in
# silence. `tests/vendoring.rs` fails if that ever happens.
[patch.crates-io]
syntect = { path = "vendor/syntect" }
```

Leave the `syntect = { version = "5.3.0", ... }` dependency line alone — the patch table redirects it, and keeping the version requirement is what documents which upstream release this tree came from.

- [ ] **Step 6: Verify one crate, and that nothing else moved**

```bash
cargo tree --duplicates
cargo test -p mdmost --test vendoring -j4
cargo test -p mdmost -j4
cargo test -p syntect -j4
cargo clippy --all-targets -p mdmost -j4 -- -D warnings
cargo fmt --check -p mdmost
```

Expected: `cargo tree --duplicates` prints nothing about `syntect`. The mdmost test count is **unchanged from main plus the one new vendoring test** — record both numbers. `cargo test -p syntect` passes; record its count too, because Tasks 2 and 3 add to it and a reviewer needs the baseline.

If the mdmost count moved by anything other than +1, stop: the vendoring changed behaviour, which it must not.

- [ ] **Step 7: Write `vendor/syntect/VENDORED.md`**

Follow `vendor/pulldown-latex/VENDORED.md`'s structure exactly — a reader who knows one should recognise the other. Sections: what is here (source version, upstream URL, the crates.io tarball it came from); the patches (empty for now, filled by Tasks 2 and 3); this vendor is temporary (copy spec §11's procedure); what was dropped from the upstream tree and why (Step 3's table, plus the capability lost: the public-API snapshot test, and the note that a re-sync should run the full suite in a real checkout of the fork first); and the manifest is not upstream's verbatim (Step 4).

State plainly that patches 1 and 2 are not this project's work in origin: patch 1 is upstream PR #706 by `tontinton`, and patch 2 answers upstream issue #202.

- [ ] **Step 8: Add the CI step**

In `.github/workflows/ci.yml`, after the existing `cargo test -p pulldown-latex` step:

```yaml
      # The vendored syntect's own suite, for the same reason pulldown-latex's runs: the
      # patches in vendor/syntect/src/parsing/parser.rs add their regression tests to the
      # inline `mod tests` there. If these go red, a patch has been lost.
      - name: Run the vendored syntect's tests
        run: cargo test -p syntect
```

And in the lint job, after the clippy step:

```yaml
      # `[patch.crates-io]` is ignored in silence if the vendored version stops satisfying
      # two-face's requirement. Two syntect crates compile and the failure surfaces as a
      # type error naming `SyntaxSet` twice.
      - name: One syntect only
        run: cargo tree --duplicates | grep -qv syntect
```

- [ ] **Step 9: Commit**

```bash
git add vendor/syntect Cargo.toml Cargo.lock tests/vendoring.rs .github/workflows/ci.yml
git commit  # message per the house style; no behaviour change, and say so
```

---

## Task 2: Patch 1 — break loops between non-consuming `set`s

**Files:**
- Modify: `vendor/syntect/src/parsing/parser.rs`
- Modify: `vendor/syntect/VENDORED.md`
- Test: `vendor/syntect/src/parsing/parser.rs` (the inline `mod tests`)

**Interfaces:**
- Consumes: the vendored tree from Task 1.
- Produces: `fn advance_one_char(line: &str, start: &mut usize) -> bool` and `type SetStates = HashSet<(usize, u64), BuildHasherDefault<FnvHasher>>`, both private to `parser.rs`. No public API change.

- [ ] **Step 1: Write the two failing tests**

Add both to the inline `mod tests` in `vendor/syntect/src/parsing/parser.rs`. The first is upstream PR #706's own test, kept verbatim so a future re-sync sees it as already present:

```rust
    #[test]
    fn can_parse_non_consuming_sets_that_would_loop() {
        let syntax = r#"
name: test
scope: source.test
contexts:
  main:
    # This makes us go into "a" without consuming any characters
    - match: (?=test)
      set: a
  a:
    # And this one into "b", still without consuming anything
    - match: (?=t)
      set: b
    - match: \w+
      scope: test.matched
  b:
    # Which sends us straight back to "a". Neither of the two is a push or a
    # pop, so the stack depth never moves and the push/pop loop check above
    # never sees a thing. ST stops taking a "set" that already fired at this
    # position and advances instead.
    - match: (?=t)
      set: a
    - match: \w+
      scope: test.matched
"#;

        let line = "test";
        let expect = ["<source.test>, <test.matched>"];
        expect_scope_stacks(line, &expect, syntax);
    }
```

The second is the input that started this work, so the vendor holds the real-world case and not only the synthetic one:

```rust
    /// The two lines from `docs/upstream/2026-09-21-javascript-syntax-hang.md`.
    ///
    /// Before patch 1 this did not return: `expression-statement-continuation` matches
    /// empty because the `/` of `/*` is in its operator class, and the "blanks and
    /// comments then end of line" rule also matches empty, so the two handed the position
    /// back to each other. Kept as a *parser* test rather than only as an mdmost one
    /// because the defect is here, and because a future re-sync to a newer upstream needs
    /// to fail loudly if the fix is lost.
    #[test]
    fn the_javascript_continuation_then_block_comment_terminates() {
        let ps = SyntaxSet::load_defaults_newlines();
        let syntax = ps
            .find_syntax_by_name("JavaScript")
            .expect("the default set has a JavaScript syntax");
        let mut state = ParseState::new(syntax);
        for line in ["  | { type: \"a\" }\n", "  /** x */\n"] {
            state
                .parse_line(line, &ps)
                .expect("a bounded parse, not a hang");
        }
    }
```

**Note on that second test:** it uses `load_defaults_newlines`, syntect's own 2016-vintage bundle, not `two-face`'s. Before running it, confirm the bundled `JavaScript` reproduces the hang. If it does not — the 2016 definition may predate the looping rules — replace the body with a minimal synthetic syntax that models the same rule pair, and say so in the doc comment. **Do not leave a test that passes for the wrong reason.**

- [ ] **Step 2: Run them to verify they fail**

```bash
timeout 60 cargo test -p syntect can_parse_non_consuming_sets_that_would_loop -j4
timeout 60 cargo test -p syntect the_javascript_continuation -j4
```

Expected: both **time out** rather than fail with an assertion — that is the defect. Use `timeout`, and never run these without it: an unbounded parse in a test binary is exactly the shape that has previously grown to 23 GB and taken a sibling session's session down with it. If either finishes and merely fails an assertion, stop and work out why before implementing.

- [ ] **Step 3: Implement the fix**

Three edits in `vendor/syntect/src/parsing/parser.rs`.

First, the imports and the two new items, beside the existing `SearchCache` type alias:

```rust
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hash, Hasher};
```

```rust
/// The `(byte, stack fingerprint)` pairs a non-consuming `set` was already taken from,
/// used to break `set` cycles. See [`ParseState::parse_next_token`].
type SetStates = HashSet<(usize, u64), BuildHasherDefault<FnvHasher>>;

/// A fingerprint of the context stack, for [`SetStates`].
///
/// A hash rather than the stack itself: the key is built on every non-consuming `set`,
/// and cloning a `Vec<ContextId>` each time allocates for nothing. `prototypes` is part
/// of it because two levels that share a context but differ in their prototypes are
/// genuinely different states, and conflating them would break a legitimate `set` one
/// character early. `captures` is left out because `Region` is not hashable; the cost of
/// that is bounded to one character of lost highlighting and can never hang or panic.
fn stack_fingerprint(stack: &[StateLevel]) -> u64 {
    let mut hasher = FnvHasher::default();
    for level in stack {
        level.context.hash(&mut hasher);
        level.prototypes.hash(&mut hasher);
    }
    hasher.finish()
}

/// Moves `start` past one character, the way Sublime Text breaks a match loop.
///
/// Byte indices, so `+= 1` would split a multi-byte character. Returns false at the end
/// of the line, where there is nothing to advance over and no point trying more patterns.
fn advance_one_char(line: &str, start: &mut usize) -> bool {
    match line[*start..].char_indices().nth(1) {
        Some((i, _)) => {
            *start += i;
            true
        }
        None => false,
    }
}
```

Second, in `parse_line`, create the set beside `non_consuming_push_at` and thread it through:

```rust
        // Used for detecting loops with push/pop, see long comment above.
        let mut non_consuming_push_at = (0, 0);
        // Used for detecting loops between non-consuming `set`s, see `parse_next_token`.
        let mut set_states = SetStates::default();
```

Add `&mut set_states,` to the `self.parse_next_token(...)` call and to that method's parameter list, immediately after `non_consuming_push_at`.

Third, in `parse_next_token`, replace the existing end-of-comment in the `if !consuming` block and add the new check. The existing `pop_would_loop` branch above it also collapses onto the new helper:

```rust
                return Ok(advance_one_char(line, start));
```

and inside `if !consuming`, after the existing `MatchOperation::Push(_)` arm:

```rust
                // A non-consuming "set" loops too, and the guard above cannot see it: a
                // "set" is neither the push that arms it nor the pop that trips it, and
                // the stack depth never changes, so two contexts that "set" each other
                // spin here forever. Taking a "set" from a byte and stack we already took
                // one from can only repeat the same work, so do what Sublime Text does
                // with the push/pop loop and move on by a character.
                if matches!(match_pattern.operation, MatchOperation::Set { .. }) {
                    let fingerprint = stack_fingerprint(&self.stack);
                    if !set_states.insert((*start, fingerprint)) {
                        return Ok(advance_one_char(line, start));
                    }
                }
```

Also delete the words `Otherwise leave the state, e.g. non-consuming "set" could also result in a loop.` from the comment above the `Push` arm — that comment documented the gap this patch closes.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
timeout 300 cargo test -p syntect -j4
```

Expected: PASS, including the 41 pre-existing parser tests. Watch `can_parse_non_consuming_set_after_consuming_push_that_does_not_loop` in particular — it exists to catch a loop-breaker that fires when it should not.

- [ ] **Step 5: Confirm the reproducer through mdmost too**

```bash
printf '```js\n  | { type: "a" }\n  /** x */\n```\n' > /tmp/../scratch/oetiker/claude-tmp/hang.md
timeout 30 cargo run -q -j4 -- --to-ansi /scratch/oetiker/claude-tmp/hang.md | head -20
```

Expected: it returns, and the code is coloured. Before this patch it did not return. (Use the project's own non-interactive export path rather than the pager, so this is scriptable.)

- [ ] **Step 6: Record the patch in `VENDORED.md`**

One entry under the patches section: what the defect was, that the fix is upstream PR #706 by `tontinton` and therefore not offered again, and the two local refinements with their reasons — the fingerprint instead of a cloned `Vec`, and `prototypes` included in the key. Note that if #706 merges upstream, the two touch the same lines and must be reconciled rather than re-applied.

- [ ] **Step 7: Measure whether the fingerprint change is worth keeping on its own merits**

Spec §4 asks for this. Non-consuming `set`s are rare, so the allocation may cost nothing measurable.

Interleaved A/B of two release binaries built in **separate** target dirs. `perf` is unusable here — `perf_event_paranoid` is 4 — and a shared target dir would make the second build overwrite the first.

```bash
# A: the fingerprint key, as implemented in Step 3.
CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-fp-a cargo build --release -j4
cp /scratch/oetiker/cargo-target-mdmost-fp-a/release/mdmost /scratch/oetiker/claude-tmp/mdmost-fingerprint

# B: revert stack_fingerprint to upstream's allocating key, build again, then restore.
#    SetStates becomes HashSet<(usize, Vec<ContextId>), ...> and the insert reads
#    `set_states.insert((*start, self.stack.iter().map(|l| l.context).collect()))`.
CARGO_TARGET_DIR=/scratch/oetiker/cargo-target-mdmost-fp-b cargo build --release -j4
cp /scratch/oetiker/cargo-target-mdmost-fp-b/release/mdmost /scratch/oetiker/claude-tmp/mdmost-collect

# Interleaved, five rounds each, against a document with many fences. --to-ansi so it
# terminates without a terminal.
DOC=tests/corpus/<the largest multi-fence corpus document>
for i in 1 2 3 4 5; do
  /usr/bin/time -f "fingerprint %e" /scratch/oetiker/claude-tmp/mdmost-fingerprint --to-ansi "$DOC" >/dev/null
  /usr/bin/time -f "collect     %e" /scratch/oetiker/claude-tmp/mdmost-collect     --to-ansi "$DOC" >/dev/null
done
```

Pick `$DOC` by size from `ls -S tests/corpus/`, and say which one in the notes — a timing without its input is not a measurement.

**Record the numbers either way.** If the difference does not show above the run-to-run spread, say exactly that in `VENDORED.md` and keep the change for the precision alone. A later reader must not have to guess whether this was measured or assumed. Delete the two throwaway target dirs afterwards.

- [ ] **Step 8: Commit**

```bash
git add vendor/syntect/src/parsing/parser.rs vendor/syntect/VENDORED.md
git commit
```

---

## Task 3: Patch 2 — a per-line token budget

**Files:**
- Modify: `vendor/syntect/src/parsing/parser.rs`
- Modify: `vendor/syntect/VENDORED.md`
- Test: `vendor/syntect/src/parsing/parser.rs` (the inline `mod tests`)

**Interfaces:**
- Consumes: Task 2's tree.
- Produces, both public:
  - `ParsingError::TokenLimitExceeded { limit: usize }` — a new variant on the existing `#[non_exhaustive]` enum.
  - `ParseState::parse_line_with_limit(&mut self, line: &str, syntax_set: &SyntaxSet, limit: Option<NonZeroUsize>) -> Result<Vec<(usize, ScopeStackOp)>, ParsingError>`.
  - `ParseState::parse_line` keeps its exact signature and delegates with `None`.

- [ ] **Step 1: Write the failing tests**

Three tests in the inline `mod tests`. They cover the boundary from both sides and the promise that the default is unchanged.

```rust
    /// A syntax whose `main` produces one token per character, so a line of `n`
    /// characters costs a predictable `n` tokens and the boundary can be asserted
    /// exactly rather than approximately.
    const ONE_TOKEN_PER_CHAR: &str = r#"
name: counter
scope: source.counter
contexts:
  main:
    - match: .
      scope: counter.char
"#;

    #[test]
    fn a_limit_above_the_line_s_cost_parses_normally() {
        let ps = counter_set();
        let syntax = ps.find_syntax_by_name("counter").expect("in the set");
        let mut state = ParseState::new(syntax);
        let ops = state
            .parse_line_with_limit("abcd\n", ps, NonZeroUsize::new(64))
            .expect("four characters cost far fewer than 64 tokens");
        assert!(!ops.is_empty());
    }

    #[test]
    fn a_limit_below_the_line_s_cost_is_an_error_naming_the_limit() {
        let ps = counter_set();
        let syntax = ps.find_syntax_by_name("counter").expect("in the set");
        let mut state = ParseState::new(syntax);
        let err = state
            .parse_line_with_limit("abcdefghij\n", ps, NonZeroUsize::new(2))
            .expect_err("ten characters cannot be parsed in two tokens");
        assert!(
            matches!(err, ParsingError::TokenLimitExceeded { limit: 2 }),
            "expected TokenLimitExceeded {{ limit: 2 }}, got {err:?}"
        );
    }

    /// The promise that makes the upstream offer honest: with no limit, this is 5.3.0.
    #[test]
    fn no_limit_is_the_unchanged_5_3_0_behaviour() {
        let ps = counter_set();
        let syntax = ps.find_syntax_by_name("counter").expect("in the set");
        let mut with_none = ParseState::new(syntax);
        let mut without_limit = ParseState::new(syntax);
        let line = "abcdefghij\n";
        assert_eq!(
            with_none.parse_line_with_limit(line, ps, None).unwrap(),
            without_limit.parse_line(line, ps).unwrap(),
        );
    }
```

Plus the helper, beside the other test helpers in that module:

```rust
    /// The counter syntax, in a leaked set.
    ///
    /// Returns the set and nothing else, and each test looks its syntax up. It must NOT
    /// return a `SyntaxReference` alongside a *clone* of the set: a `ContextId` is an
    /// index into one particular `SyntaxSet` (`ParseState::parse_line`'s own doc says so),
    /// so parsing with a reference from a different set gives wrong results or panics.
    ///
    /// Leaked on purpose. One small set per test binary, and `&'static` is what lets every
    /// test share it without a lazy static or a per-test rebuild.
    fn counter_set() -> &'static SyntaxSet {
        static SET: OnceLock<SyntaxSet> = OnceLock::new();
        SET.get_or_init(|| {
            let mut builder = SyntaxSetBuilder::new();
            builder.add(
                SyntaxDefinition::load_from_str(ONE_TOKEN_PER_CHAR, true, None)
                    .expect("the counter syntax should load"),
            );
            builder.build()
        })
    }
```

Each of the three tests then starts:

```rust
        let ps = counter_set();
        let syntax = ps
            .find_syntax_by_name("counter")
            .expect("the counter syntax should be in the set");
        let mut state = ParseState::new(syntax);
```

and passes `ps` where the tests above write `&ps`. Add `use std::sync::OnceLock;` to the test module.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p syntect token_limit no_limit a_limit -j4`

Expected: FAIL to compile — `parse_line_with_limit` and `ParsingError::TokenLimitExceeded` do not exist.

- [ ] **Step 3: Implement the variant and the method**

Add to `ParsingError` (it is `#[non_exhaustive]`, so this is not a breaking change):

```rust
    /// A line needed more tokens than the caller allowed. See
    /// [`ParseState::parse_line_with_limit`].
    #[error("Line exceeded its limit of {limit} tokens")]
    TokenLimitExceeded { limit: usize },
```

Rename the body of `parse_line` into `parse_line_with_limit`, and have `parse_line` delegate:

```rust
    pub fn parse_line(
        &mut self,
        line: &str,
        syntax_set: &SyntaxSet,
    ) -> Result<Vec<(usize, ScopeStackOp)>, ParsingError> {
        self.parse_line_with_limit(line, syntax_set, None)
    }

    /// Parses a single line, giving up after `limit` tokens.
    ///
    /// `None` is unlimited and behaves exactly as [`Self::parse_line`] always has.
    ///
    /// A `.sublime-syntax` definition can drive the token loop below without ever
    /// consuming a character or changing the stack depth, and neither the push/pop guard
    /// nor the non-consuming-`set` guard catches every shape of that. This limit is the
    /// backstop: it turns a parse that would not return into a
    /// [`ParsingError::TokenLimitExceeded`], which a caller can degrade on.
    ///
    /// A token count rather than a wall-clock budget, because a count is deterministic
    /// and a test can assert the exact boundary. A `Duration` can be layered on top by
    /// checking the clock every N tokens.
    pub fn parse_line_with_limit(
        &mut self,
        line: &str,
        syntax_set: &SyntaxSet,
        limit: Option<NonZeroUsize>,
    ) -> Result<Vec<(usize, ScopeStackOp)>, ParsingError> {
        // 5.3.0's `parse_line` body, moved here verbatim — the `stack.is_empty()` check,
        // `match_start`, `res`, the `first_line` block, `regions`, `search_cache`,
        // `non_consuming_push_at`, Task 2's `set_states` — down to but not including its
        // `while self.parse_next_token(...)` loop, which the next block replaces. Nothing
        // in it changes. Move it; do not retype it.
    }
```

Replace the loop at the end of the body:

```rust
        let mut tokens = 0usize;
        while self.parse_next_token(
            line,
            syntax_set,
            &mut match_start,
            &mut search_cache,
            &mut regions,
            &mut non_consuming_push_at,
            &mut set_states,
            &mut res,
        )? {
            tokens += 1;
            if let Some(limit) = limit {
                if tokens >= limit.get() {
                    return Err(ParsingError::TokenLimitExceeded { limit: limit.get() });
                }
            }
        }
```

Add `use std::num::NonZeroUsize;` to the imports.

**A nested `if let`, not a let-chain.** `vendor/syntect/Cargo.toml` is `edition = "2021"`
and let-chains need edition 2024. Raising the vendored crate's edition would be a
local-only change, which the Global Constraints forbid because it blocks the spec §11
exit.

**`NonZeroUsize`, not `usize`:** a limit of zero would reject every line including empty ones, which is never what a caller means, and the type makes that unrepresentable rather than a runtime check.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
timeout 300 cargo test -p syntect -j4
timeout 600 cargo test -p mdmost -j4
```

Expected: PASS. The mdmost count must not move — nothing there passes a limit yet.

- [ ] **Step 5: Record the patch in `VENDORED.md`**

What it adds, that it answers upstream issue #202 where the owner offered to accept exactly this, that the default is unlimited so no existing caller changes, and that the `Duration` layer §9 of the spec promises upstream is **not** in the vendor — the vendor needs only the count.

- [ ] **Step 6: Commit**

```bash
git add vendor/syntect/src/parsing/parser.rs vendor/syntect/VENDORED.md
git commit
```

---

## Task 4: Measure the limit, set it, and delete the thread guard

**Files:**
- Modify: `src/highlight.rs` — the limit, and the deletions
- Modify: `src/highlight/tests.rs` — rewrite the hang test
- Modify: `docs/maintainer-notes.md` — replace the removed arithmetic with the measurement

**Interfaces:**
- Consumes: `ParseState::parse_line_with_limit` and `ParsingError::TokenLimitExceeded` from Task 3.
- Produces: `pub const MAX_TOKENS_PER_LINE: NonZeroUsize` in `src/highlight.rs`. Removes `MAX_HIGHLIGHT_BUDGET`, `RELEASE_FLOOR`, `DEBUG_BUDGET_MULTIPLIER`, `MAX_ABANDONED`, `budget_for`, `run_within`, `ABANDONED` and `reset_abandoned_for_test`.

- [ ] **Step 1: Measure the token ceiling of legitimate lines**

The constant must not be guessed. Write a throwaway test that bisects the limit using only the public API — no instrumentation, so a reviewer can re-run it.

```rust
    /// Smallest limit at which `line` parses, found by doubling. Throwaway: used to pick
    /// `MAX_TOKENS_PER_LINE`, then deleted. Uses only the public API on purpose.
    fn tokens_needed(lang: &str, line: &str) -> usize {
        let (set, syntax) = resolve_syntax(Some(lang)).expect("a known language");
        let mut limit = 1usize;
        loop {
            let mut state = ParseState::new(syntax);
            match state.parse_line_with_limit(line, set, NonZeroUsize::new(limit)) {
                Ok(_) => return limit,
                Err(_) => limit *= 2,
            }
        }
    }
```

Measure across the languages in `docs/maintainer-notes.md`'s cold-compile table — TypeScript, JavaScript, makefile, python, rust, yaml — using the longest realistic line of each, **and one deliberately hostile but legitimate case: a single minified JavaScript line of several thousand characters.** Minified JS is the real ceiling; ordinary prose code is nowhere near it.

Record every number. The constant goes far enough above the highest that no real document loses colour.

- [ ] **Step 2: Write the failing tests**

Two. The first replaces `the_javascript_hang_degrades_to_plain_text`, whose meaning has inverted — it asserted the degradation, and the input now highlights:

```rust
/// The two lines from `docs/upstream/2026-09-21-javascript-syntax-hang.md`.
///
/// This test used to be `the_javascript_hang_degrades_to_plain_text` and asserted that
/// the pager survived by giving up. Patch 1 in `vendor/syntect` fixes the loop, so the
/// block is now highlighted like any other. Kept rather than deleted: it is the only
/// test naming the input that caused the vendoring, and if a re-sync loses the patch
/// this is what says so.
#[test]
fn the_javascript_continuation_then_block_comment_is_highlighted() {
    let theme = Theme::default();
    let src = "  | { type: \"a\" }\n  /** x */\n";
    let lines = highlight(Some("js"), src, &theme);
    assert_eq!(lines.len(), 2);
    assert_ne!(
        lines,
        plain(src, &theme.code),
        "the block should be highlighted, not degraded"
    );
}
```

The second protects the constant from being tightened later:

```rust
/// A legitimate long line keeps its colour.
///
/// `MAX_TOKENS_PER_LINE` is a backstop against a syntax that will not terminate, not a
/// size policy. If someone tightens it far enough to degrade real code, this fails
/// instead of a reader's document quietly losing colour.
#[test]
fn a_long_minified_line_is_still_highlighted() {
    let theme = Theme::default();
    let src = format!("{}\n", MINIFIED_JS_LINE);
    let lines = highlight(Some("js"), &src, &theme);
    assert_ne!(lines, plain(&src, &theme.code));
}
```

`MINIFIED_JS_LINE` is the measured hostile case from Step 1, as a `const &str` in the test module. **Paste the real line, not a placeholder.**

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p mdmost the_javascript_continuation a_long_minified -j4`

Expected: the first FAILS on the `assert_ne!` (the block still degrades, because `highlight_with` has not been given the limit yet and the old guard still abandons it); the second may pass already. **A test that passes before the change is not evidence** — if the second passes, note why and keep it as a regression guard.

- [ ] **Step 4: Pass the limit, and delete the guard**

In `src/highlight.rs`, add the constant with its measurement recorded beside it, replace the `state.parse_line(raw, set)` call in `highlight_with` with `state.parse_line_with_limit(raw, set, Some(MAX_TOKENS_PER_LINE))`, and simplify `highlight` to:

```rust
pub fn highlight(lang: Option<&str>, src: &str, theme: &Theme) -> Vec<Line> {
    if let Some(hit) = cache_get(lang, src, theme) {
        return hit;
    }
    let lines = highlight_uncached(lang, src, &theme.code);
    cache_put(lang, src, theme, &lines);
    lines
}
```

Then delete `run_within`, `ABANDONED`, `MAX_ABANDONED`, `reset_abandoned_for_test`, `budget_for`, `RELEASE_FLOOR`, `DEBUG_BUDGET_MULTIPLIER` and `MAX_HIGHLIGHT_BUDGET`, their tests, and the `ABANDONED` half of `HIGHLIGHT_GLOBALS_TEST_LOCK`'s doc comment. **Keep the lock itself** — its `CACHE` half is still real, and the memo does not move out of a global until branch B.

Keep `MAX_HIGHLIGHT_BYTES` and `MAX_HIGHLIGHT_LINES`: coarse pre-checks on block size, orthogonal to a per-line token bound.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
timeout 600 cargo test -p mdmost -j4
cargo clippy --all-targets -p mdmost -j4 -- -D warnings
```

Expected: PASS. The count drops by however many tests the deleted guard had — **name them in the commit message, with the total before and after.** A count that falls without an accounting is indistinguishable from a test silently lost.

- [ ] **Step 6: Update `docs/maintainer-notes.md`**

Delete the highlight-budget arithmetic, the cold-compile budget reasoning that depended on `budget_for`, and the warning that anyone retuning those constants must re-measure both build profiles. **Keep the cold-compile table itself** — it is a measured fact about syntect and still true. Add Step 1's token measurements in its place, with the method (bisection through the public API) so they can be re-run.

- [ ] **Step 7: Commit**

```bash
git add src/highlight.rs src/highlight/tests.rs docs/maintainer-notes.md
git commit
```

---

## Task 5: Say why a block is plain

**Files:**
- Modify: `src/highlight.rs` — the `Outcome` enum, memoised
- Modify: `src/render/bridge.rs` — expose it to the renderer
- Modify: `src/render/code.rs:366-375` — `framed` → `framed_captioned`
- Test: `src/highlight/tests.rs`, `src/render/tests.rs`

**Interfaces:**
- Consumes: Task 4's `src/highlight.rs`.
- Produces:
  - `pub enum Outcome { Highlighted, Plain, Failed }`, deriving `Clone, Copy, Debug, PartialEq, Eq`.
  - `pub fn outcome(lang: Option<&str>, src: &str, theme: &Theme) -> Outcome` — reads the memo, so it must be called after `highlight` for the same key. Returns `Outcome::Plain` for a key the memo does not hold.
  - `pub(crate) fn bridge::outcome(language: Option<&str>, src: &str, theme: &Theme) -> Outcome`.
  - `#[cfg(test)] fn highlight_with_limit(lang: Option<&str>, src: &str, theme: &Theme, limit: NonZeroUsize) -> Vec<Line>` in `src/highlight.rs` — `highlight` with the token limit overridden, so a test can drive a real `Failed` on a real syntax instead of stubbing the parser. It memoises exactly as `highlight` does, which is what lets `outcome` be asserted straight after it.
  - `fn caption(text: &str, ctx: Ctx<'_>) -> Line` in `src/render/code.rs`, private, modelled on the existing `title` helper at `src/render/code.rs:565`:

    ```rust
    /// The label drawn into the frame's bottom edge: what happened to this block.
    ///
    /// Styled like the overflow marker rather than the language label, because it is a
    /// report about the block and not part of the block's identity.
    fn caption(text: &str, ctx: Ctx<'_>) -> Line {
        let mut line = Line::empty();
        line.push(Span::new(text, ctx.theme.code.overflow_marker));
        line
    }
    ```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn an_untagged_block_is_plain_not_failed() {
    let theme = Theme::default();
    let src = "just text\n";
    highlight(None, src, &theme);
    assert_eq!(outcome(None, src, &theme), Outcome::Plain);
}

#[test]
fn an_unknown_tag_is_plain_not_failed() {
    let theme = Theme::default();
    let src = "just text\n";
    highlight(Some("no-such-language"), src, &theme);
    assert_eq!(outcome(Some("no-such-language"), src, &theme), Outcome::Plain);
}

#[test]
fn a_highlighted_block_says_so() {
    let theme = Theme::default();
    let src = "fn main() {}\n";
    highlight(Some("rust"), src, &theme);
    assert_eq!(outcome(Some("rust"), src, &theme), Outcome::Highlighted);
}
```

For `Failed`, drive a real token-limit error rather than a stub, so the test exercises the path a reader would hit. Use the minified line from Task 4 with a deliberately tiny limit — which means `highlight_uncached` needs the limit injectable for tests. Add a `#[cfg(test)]` seam rather than making the constant mutable:

```rust
#[test]
fn a_block_that_exceeds_its_token_budget_is_failed() {
    let _guard = HIGHLIGHT_GLOBALS_TEST_LOCK.lock();
    let theme = Theme::default();
    let src = format!("{}\n", MINIFIED_JS_LINE);
    let lines = highlight_with_limit(Some("js"), &src, &theme, NonZeroUsize::new(2).unwrap());
    assert_eq!(lines, plain(&src, &theme.code));
    assert_eq!(outcome(Some("js"), &src, &theme), Outcome::Failed);
}
```

And two render-side tests, in `src/render/tests.rs`. The first is the caption; the second is the geometry promise branch B will depend on.

```rust
/// A block whose highlight failed says so in its frame's bottom edge.
///
/// The memo is primed through `highlight_with_limit` rather than by stubbing the parser,
/// so this exercises the path a reader actually reaches. The globals lock is held because
/// priming the memo under one theme races any other test that switches theme —
/// `cache_put` clears the whole map on a theme change, keyed on the theme and not on the
/// entry.
#[test]
fn a_failed_block_captions_its_frame() {
    let _guard = crate::highlight::HIGHLIGHT_GLOBALS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let theme = Theme::default_dark();
    let src = format!("{}\n", crate::highlight::tests::MINIFIED_JS_LINE);
    crate::highlight::highlight_with_limit(
        Some("js"),
        &src,
        &theme,
        std::num::NonZeroUsize::new(2).expect("two is non-zero"),
    );

    let node = code_block("js", &src);
    let canvas = render_block(&node, 60, &theme, &BUTTONS);
    canvas.check_invariants().expect("contract holds");

    let bottom = canvas.row_text(canvas.height() - 1);
    assert!(
        bottom.contains(" highlighting timed out "),
        "the failed block should caption its bottom edge:\n{bottom}"
    );
}

/// The caption costs no row.
///
/// Branch B patches colour onto an already-rendered canvas, which is only sound while a
/// plain block and a highlighted one occupy the same cells. A caption that added a row
/// would break that silently, so it is pinned here rather than in a comment.
#[test]
fn a_caption_does_not_change_a_block_s_height() {
    let _guard = crate::highlight::HIGHLIGHT_GLOBALS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let theme = Theme::default_dark();
    let src = format!("{}\n", crate::highlight::tests::MINIFIED_JS_LINE);
    let node = code_block("js", &src);

    crate::highlight::highlight(Some("js"), &src, &theme);
    let uncaptioned = render_block(&node, 60, &theme, &BUTTONS).height();

    crate::highlight::highlight_with_limit(
        Some("js"),
        &src,
        &theme,
        std::num::NonZeroUsize::new(2).expect("two is non-zero"),
    );
    let captioned = render_block(&node, 60, &theme, &BUTTONS).height();

    assert_eq!(uncaptioned, captioned);
}
```

**Two things to check while writing these, because they are assumptions and not facts:**

1. `code_block(lang, src)` — this file builds nodes through helpers; find the existing one for a fenced block (the `table_with_cell("```rust\n…")` pattern near `src/render/tests.rs:2726` shows how a fence is parsed into a node) and use it rather than adding another.
2. `MINIFIED_JS_LINE` lives in `src/highlight`'s test module. Reaching it from `src/render/tests.rs` needs it `pub(crate)`. If that is awkward, move the constant to a shared test-support module rather than duplicating the literal in two files — a copy that drifts makes one of the two tests assert nothing.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mdmost outcome_ failed_block untagged_block unknown_tag -j4`

Expected: FAIL to compile — `Outcome`, `outcome` and `highlight_with_limit` do not exist.

- [ ] **Step 3: Implement**

Add the enum, store it in the cache `Entry` beside `lines`, set it in `highlight_uncached` (`Plain` when `resolve_syntax` returns `None` or a size pre-check trips, `Failed` when `highlight_with` returns `None`, `Highlighted` otherwise), and add the two reader functions. `highlight`'s return type does **not** change — export and `--to-ansi` want the lines and nothing else.

In `src/render/code.rs`, `framed_code` currently calls `inner.framed(BorderSet::ROUNDED, theme.code.frame, title.as_ref(), theme.code.background)`. Give it a caption when the outcome is `Failed`:

```rust
    let note = (bridge::outcome(language, literal, theme) == highlight::Outcome::Failed)
        .then(|| caption("highlighting timed out", ctx));
    let mut out = inner.framed_captioned(
        BorderSet::ROUNDED,
        theme.code.frame,
        title.as_ref(),
        note.as_ref(),
        theme.code.background,
    );
```

Add `highlight_with_limit` beside `highlight`, sharing its body through a private helper so the two cannot drift:

```rust
#[cfg(test)]
fn highlight_with_limit(
    lang: Option<&str>,
    src: &str,
    theme: &Theme,
    limit: NonZeroUsize,
) -> Vec<Line> {
    highlight_capped(lang, src, theme, limit)
}
```

where `highlight_capped` is what `highlight` itself now calls with `MAX_TOKENS_PER_LINE`.

`framed_captioned` already exists (`src/canvas/ops.rs:653`) and writes the caption into the bottom edge, so **no row is added and the block's geometry does not change** — which is what branch B's canvas patching will depend on. Style the caption like the existing `title` helper at `src/render/code.rs:560-575`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
timeout 600 cargo test -p mdmost -j4
cargo clippy --all-targets -p mdmost -j4 -- -D warnings
```

Expected: PASS. Check that no existing render snapshot moved: a caption appears only for `Failed`, which no existing fixture produces. If a snapshot did move, find out why before updating it.

- [ ] **Step 5: Add the CHANGES.md entry**

Under the unreleased section, in the house style — user-visible, no internals:

```markdown
### Fixed
- A code block whose language definition sent the highlighter into a loop no longer
  freezes the pager. The block is shown without colour and says so in its frame.
```

No version bump: that happens only in a `Release vX` commit.

- [ ] **Step 6: Commit**

```bash
git add src/highlight.rs src/highlight/tests.rs src/render/bridge.rs src/render/code.rs src/render/tests.rs CHANGES.md
git commit
```

---

## Task 6: Offer the budget upstream

Outward-facing, so it ends at a draft. **Do not open the PR without the owner's approval of the wording.**

**Files:**
- Create: a branch in a *separate clone* of `trishume/syntect`, not in this repo
- Modify: `vendor/syntect/VENDORED.md` — record the PR number once filed

**Interfaces:**
- Consumes: Task 3's implementation.
- Produces: no code in this repository.

- [ ] **Step 1: Clone syntect and branch from master**

Not from 5.3.0 — upstream wants the patch against master, which has moved (branch support landed as PR #614 in March 2026). Expect the token loop to have changed shape.

- [ ] **Step 2: Port Task 3's change, shaped for upstream not for us**

Differences from the vendored copy, all deliberate:

- **Add the `Duration` layer.** Issue #202 asked for a timeout; the count is the primitive. Layer `parse_line_with_timeout(line, set, Duration)` over it by checking `Instant::now()` every N tokens — N a private constant, so the clock cost is amortised. This is what the issue asked for, and offering only a count invites a review round asking for it.
- **Keep Task 3's three tests**, plus one for the `Duration` path.
- **Do not include patch 1**; it is already PR #706.
- **Do not include the `[lints]` table, `publish = false`, or any stripping.** Those are vendoring artefacts.

- [ ] **Step 3: Run upstream's full suite, including what the vendor dropped**

```bash
cargo test --all-features
```

This runs `tests/public_api.rs`, which the vendor drops — and a new public method **will** change that snapshot. Update it, and expect a reviewer to look at that diff first.

- [ ] **Step 4: Draft the PR text and stop**

Reference issue #202 and quote the owner's offer. Say the count is the primitive and the `Duration` is layered over it, and why: determinism for tests. State that the default is unlimited so no existing caller changes behaviour, and that `ParsingError` is `#[non_exhaustive]` so the variant is not a breaking change. Mention that a second project hit the motivating hang, linking the #706 comment already posted.

**Present the draft for approval. Do not push.**

- [ ] **Step 5: After filing, record the number**

Add the PR number and URL to `vendor/syntect/VENDORED.md` under patch 2, and to spec §11's "what to watch", so the exit condition names a specific PR rather than "the budget".

```bash
git add vendor/syntect/VENDORED.md docs/superpowers/specs/2026-09-22-bounded-syntect-design.md
git commit
```

---

## Verification before the branch is offered for review

Re-derive these rather than trusting the task steps — a green number quoted from an earlier run is not evidence.

- [ ] `cargo test -p mdmost -j4` — record passed / failed / ignored, and account for the delta against `main` (Task 1 adds 1, Task 4 removes the guard's tests, Task 5 adds 4 or 5).
- [ ] `cargo test -p syntect -j4` — record the count; it is Task 1's baseline plus 5.
- [ ] `cargo test -p pulldown-latex -j4` — must be unchanged.
- [ ] `cargo clippy --all-targets -p mdmost -j4 -- -D warnings` — no warnings.
- [ ] `cargo fmt --check -p mdmost`.
- [ ] `cargo tree --duplicates` — says nothing about `syntect`.
- [ ] The two-line reproducer highlights rather than hanging, run through the built binary.
- [ ] A musl static build, since that is the reason `fancy-regex` exists here.
- [ ] `cargo check --all-targets -p mdmost` on Windows, via CI.
- [ ] `grep -rn "run_within\|ABANDONED\|budget_for\|RELEASE_FLOOR\|DEBUG_BUDGET_MULTIPLIER" src/` returns nothing.
- [ ] `vendor/syntect/` differs from the crates.io 5.3.0 tarball only in `src/parsing/parser.rs`, `Cargo.toml`, `VENDORED.md`, and the dropped files. Diff it and check.
