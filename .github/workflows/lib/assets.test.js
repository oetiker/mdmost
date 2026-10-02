// repo-infra: workflow-lib v6
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const assets = require('./assets.js');

// The release smtp-proxy-rs's publish-deb-container job produces.
const RELEASE = [
  'smtp-proxy-0.1.0-x86_64-unknown-linux-musl',
  'smtp-proxy_0.1.0-1_amd64.deb',
];
const EXPECTED = ['*.deb', 'smtp-proxy-*-x86_64-unknown-linux-musl'];

test('a complete release is missing nothing', () => {
  assert.deepEqual(assets.missingAssets(RELEASE, EXPECTED), []);
});

test('the empty release names every expectation', () => {
  // The defect this guard exists for: finalize ran before the add-on job had
  // uploaded anything, because its `needs:` had been reverted.
  assert.deepEqual(assets.missingAssets([], EXPECTED), EXPECTED);
});

test('a half-uploaded release is still a failure', () => {
  // finalize racing an add-on that is mid-upload. The binary is there and the
  // .deb is not, which is exactly the release an operator must never see.
  const half = ['smtp-proxy-0.1.0-x86_64-unknown-linux-musl'];
  assert.deepEqual(assets.missingAssets(half, EXPECTED), ['*.deb']);
});

test('expecting nothing passes on a release with no assets', () => {
  // publish-crates-io attaches nothing to the GitHub release on purpose.
  assert.deepEqual(assets.missingAssets([], []), []);
});

test('an unexpected extra asset is not a failure', () => {
  // A repository that attaches something by hand is not broken. A guard that
  // went red on it would train people to route around finalize.
  assert.deepEqual(assets.missingAssets([...RELEASE, 'SHA256SUMS'], EXPECTED), []);
});

test('the version is what the star absorbs', () => {
  // The reason patterns exist at all: a literal name would be the second place
  // the version is written down, and would go red on the next release.
  const next = ['smtp-proxy_9.9.9-1_amd64.deb'];
  assert.deepEqual(assets.missingAssets(next, ['*.deb']), []);
});

test('a pattern anchors at both ends', () => {
  assert.equal(assets.matches('foo.deb.txt', '*.deb'), false);
  assert.equal(assets.matches('notes-foo.deb', '*.deb'), true);
});

test('the rest of a pattern is literal, not a regular expression', () => {
  // `.` in `*.deb` must mean a dot. Without escaping, `*.deb` would match
  // `mydeb` and a release with no package at all would sail through.
  assert.equal(assets.matches('mydeb', '*.deb'), false);
  assert.equal(assets.matches('smtp-proxy+1.deb', '*+1.deb'), true);
});

test('several stars in one pattern all match', () => {
  assert.equal(assets.matches('smtp-proxy_1.2.3-1_amd64.deb', 'smtp-proxy_*_*.deb'), true);
});

test('missing patterns keep their declared order', () => {
  // The job message names them in the order the config declares, so the reader
  // can find the one that is wrong.
  assert.deepEqual(assets.missingAssets(['x'], ['a', 'b', 'c']), ['a', 'b', 'c']);
});

test('unfinishedAssets names every asset whose state is not uploaded', () => {
  assert.deepEqual(assets.unfinishedAssets([
    { name: 'a.deb', state: 'uploaded' },
    { name: 'b.deb', state: 'starter' },
    { name: 'c.deb', state: 'open' },
  ]), ['b.deb', 'c.deb']);
  assert.deepEqual(assets.unfinishedAssets(undefined), []);
});

test('absent arguments are not a crash', () => {
  // finalize passes whatever the API returned. An empty release must produce a
  // failing assertion, never a TypeError that reads like an infrastructure bug.
  assert.deepEqual(assets.missingAssets(undefined, ['*.deb']), ['*.deb']);
  assert.deepEqual(assets.missingAssets(RELEASE, undefined), []);
});
