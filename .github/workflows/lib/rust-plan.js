// repo-infra: workflow-lib v5
'use strict';

// Which workspace members ci-rust lints and tests (D24).
//
// A bare `cargo test` runs the default members only. With a root [package],
// or with `default-members`, that is not every member, and the others' tests
// stop running without an error. So a workspace like that must say, in
// .github/repo-infra.json, which crates are its own and where each one's
// tests run. This function is the whole rule; the rust-plan job only feeds it
// `cargo metadata --no-deps` and turns its answer into matrix entries.
//
// An entry '' means "no -p": today's workspace-wide command.

function names(metadata, ids) {
  const byId = new Map(metadata.packages.map((p) => [p.id, p.name]));
  return ids.map((id) => byId.get(id) || id);
}

function list(rust, key) {
  const value = rust[key];
  if (value === undefined) return undefined;
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error(
      `.github/repo-infra.json: "rust"."${key}" must list at least one crate`
      + (key === 'test' ? '; "no tests" is not a configuration the standard offers' : ''),
    );
  }
  return value;
}

function rustPlan(config, metadata) {
  if (!Array.isArray(metadata.workspace_default_members)) {
    throw new Error(
      'cargo metadata reports no workspace_default_members; cargo 1.71 or later is needed',
    );
  }
  const members = names(metadata, metadata.workspace_members).sort();
  const defaults = names(metadata, metadata.workspace_default_members).sort();
  const rust = (config || {}).rust;

  if (rust === undefined) {
    const skipped = members.filter((m) => !defaults.includes(m));
    if (skipped.length > 0) {
      throw new Error(
        `a bare cargo test runs ${defaults.join(', ')} only and skips `
        + `${skipped.join(', ')}. Add a "rust" key to .github/repo-infra.json `
        + 'with "lint", "test" and "tested_elsewhere" (references/conventions.md).',
      );
    }
    return { lint: [''], test: [''] };
  }

  const test = list(rust, 'test');
  if (test === undefined) {
    throw new Error('.github/repo-infra.json: "rust" needs a "test" list');
  }
  const lint = list(rust, 'lint');
  const elsewhere = rust.tested_elsewhere || [];

  const unknown = [...(lint || []), ...test, ...elsewhere]
    .filter((name) => !members.includes(name));
  if (unknown.length > 0) {
    throw new Error(
      `.github/repo-infra.json "rust" names ${[...new Set(unknown)].join(', ')}, `
      + `which is not a workspace member (members: ${members.join(', ')})`,
    );
  }

  const untested = members.filter((m) => !test.includes(m) && !elsewhere.includes(m));
  if (untested.length > 0) {
    throw new Error(
      `${untested.join(', ')} is in neither "rust"."test" nor `
      + '"rust"."tested_elsewhere" in .github/repo-infra.json; its tests would not run',
    );
  }

  return { lint: lint || [''], test };
}

module.exports = { rustPlan };
