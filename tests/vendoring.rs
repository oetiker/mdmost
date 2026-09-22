//! The vendored `syntect` must be the only `syntect` in the build, it must be the
//! vendored one, and it must not pull in a C toolchain.
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
fn exactly_one_syntect_is_compiled_and_it_is_the_vendored_one() {
    // Not `cargo tree --duplicates` with a substring check on "syntect": this repo's
    // graph already carries unrelated duplicates today (foldhash, syn, thiserror,
    // hashbrown, ...), and `--duplicates` prints each one's dependents inverted up to
    // the workspace root. Several of those dependent chains pass through `syntect`
    // (hashbrown -> indexmap -> plist -> syntect -> mdmost), so the literal string
    // "syntect" is present in that output even when only one syntect is compiled —
    // a false positive, confirmed by running it before this test existed.
    //
    // `cargo tree -p <name>` instead asks cargo itself to resolve one package by name.
    // If cargo's graph holds two differently-versioned packages by that name, `-p`
    // is ambiguous and cargo refuses with a non-zero exit, naming every match. That
    // catches a second *version* -- but not two packages both named `syntect` at the
    // same version `5.3.0` from different *sources*, which is what a dropped or
    // mistyped `[patch.crates-io]` produces while `vendor/syntect` stays a workspace
    // member (the more likely typo of the two). `cargo tree -p` prints the resolved
    // package's source on its first line, e.g. `syntect v5.3.0 (/path/to/vendor/syntect)`,
    // so the second assertion below checks that the resolved copy is the vendored one.
    let out = Command::new(env!("CARGO"))
        .args(["tree", "--package", "syntect", "--quiet"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree should run");
    assert!(
        out.status.success(),
        "cargo tree -p syntect did not resolve to exactly one syntect -- an ambiguous \
         package specification means more than one *version* is compiled:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("vendor/syntect"),
        "cargo tree -p syntect resolved to a syntect that is not vendor/syntect -- \
         [patch.crates-io] is missing or not taking effect:\n{text}"
    );
    // "No C toolchain in the build, ever" is a load-bearing decision in the root
    // Cargo.toml -- mdmost's musl static build depends on it. mdmost's own dependency
    // line already pins `default-features = false, features = ["default-fancy"]`, so
    // this never reached the product build, but the vendored manifest's own `default`
    // named `default-onig` until this was caught by review: a bare `cargo test -p
    // syntect` -- CI's own invocation -- built `onig` -> `onig_sys`, whose
    // build-dependencies are `cc` and `pkg-config`. `default` in the manifest is now
    // `default-fancy`, so the same `cargo tree -p syntect` output above must carry no
    // trace of the onig engine.
    assert!(
        !text.to_lowercase().contains("onig"),
        "cargo tree -p syntect pulled in onig (a C toolchain via onig_sys) -- the \
         vendored manifest's [features] default must stay default-fancy:\n{text}"
    );
}
