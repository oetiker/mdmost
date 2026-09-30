// repo-infra: workflow-lib v5
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { rustPlan } = require('./rust-plan.js');

// cargo metadata --no-deps for a workspace shaped like mdmost's: a root
// [package] plus two vendored members. The root [package] makes mdmost the
// only default member, so a bare `cargo test` skips the other two.
function metadata(members, defaults = members) {
  const id = (name) => `path+file:///w/${name}#${name}@1.0.0`;
  return {
    packages: members.map((name) => ({ name, id: id(name) })),
    workspace_members: members.map(id),
    workspace_default_members: defaults.map(id),
  };
}
const MDMOST = metadata(['mdmost', 'pulldown-latex', 'syntect'], ['mdmost']);
const RUST = {
  lint: ['mdmost'],
  test: ['mdmost', 'pulldown-latex'],
  tested_elsewhere: ['syntect'],
};

test('a single crate without the key runs the workspace-wide commands', () => {
  assert.deepEqual(rustPlan({}, metadata(['one'])), { lint: [''], test: [''] });
});

test('a virtual workspace without default-members needs no key', () => {
  assert.deepEqual(rustPlan({}, metadata(['a', 'b'])), { lint: [''], test: [''] });
});

test('default members that differ from members fail without the key', () => {
  // The silent gap D24 closes: bare `cargo test` would run mdmost alone.
  assert.throws(() => rustPlan({}, MDMOST), (e) => {
    assert.match(e.message, /pulldown-latex, syntect/);
    assert.match(e.message, /"rust"/);
    return true;
  });
});

test('the declared lists become the matrix', () => {
  assert.deepEqual(rustPlan({ rust: RUST }, MDMOST), {
    lint: ['mdmost'],
    test: ['mdmost', 'pulldown-latex'],
  });
});

test('lint may be left out and then lints the whole workspace', () => {
  const { lint, ...rest } = RUST;
  assert.deepEqual(rustPlan({ rust: rest }, MDMOST).lint, ['']);
});

test('a name that is not a workspace member fails and is named', () => {
  const rust = { ...RUST, test: ['mdmost', 'pulldown-latx'] };
  assert.throws(() => rustPlan({ rust }, MDMOST), /pulldown-latx/);
});

test('a misspelt tested_elsewhere entry fails too', () => {
  const rust = { ...RUST, tested_elsewhere: ['syntec'] };
  assert.throws(() => rustPlan({ rust }, MDMOST), /syntec\b/);
});

test('a member in neither test nor tested_elsewhere fails and is named', () => {
  const rust = { ...RUST, tested_elsewhere: [] };
  assert.throws(() => rustPlan({ rust }, MDMOST), (e) => {
    assert.match(e.message, /syntect/);
    assert.match(e.message, /neither/);
    return true;
  });
});

test('a missing test list fails', () => {
  const { test: _t, ...rest } = RUST;
  assert.throws(() => rustPlan({ rust: rest }, MDMOST), /"test"/);
});

test('an empty test list fails', () => {
  assert.throws(() => rustPlan({ rust: { ...RUST, test: [] } }, MDMOST), /"test"/);
});

test('an empty lint list fails', () => {
  assert.throws(() => rustPlan({ rust: { ...RUST, lint: [] } }, MDMOST), /"lint"/);
});

test('a cargo without workspace_default_members fails with the version it needs', () => {
  const old = { ...metadata(['one']) };
  delete old.workspace_default_members;
  assert.throws(() => rustPlan({}, old), /1\.71/);
});
