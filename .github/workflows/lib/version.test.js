// repo-infra: workflow-lib v6
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const version = require('./version.js');

test('parse accepts a release tag', () => {
  assert.deepEqual(version.parse('v1.2.3'), { major: 1, minor: 2, patch: 3 });
});

test('parse rejects anything that is not vX.Y.Z', () => {
  assert.equal(version.parse('v1.2'), null);
  assert.equal(version.parse('1.2.3'), null);
  assert.equal(version.parse('v1.2.3-rc1'), null);
  assert.equal(version.parse('nightly'), null);
});

test('latest sorts numerically, not lexically', () => {
  // The case a plain string sort gets wrong: '1.9.0' > '1.10.0' as text.
  assert.equal(version.latest(['v1.0.0', 'v1.9.0', 'v1.10.0', 'v1.2.0']), 'v1.10.0');
});

test('latest ignores tags that are not releases', () => {
  assert.equal(version.latest(['v1.0.0', 'nightly', 'v2.0.0', 'v2.0.0-rc1']), 'v2.0.0');
});

test('latest of no tags is the zero version', () => {
  assert.equal(version.latest([]), 'v0.0.0');
});

test('next increments the right component', () => {
  assert.equal(version.next('v1.2.3', 'bugfix'), '1.2.4');
  assert.equal(version.next('v1.2.3', 'feature'), '1.3.0');
  assert.equal(version.next('v1.2.3', 'major'), '2.0.0');
});

test('next resets lower components', () => {
  assert.equal(version.next('v1.9.7', 'feature'), '1.10.0');
  assert.equal(version.next('v1.9.7', 'major'), '2.0.0');
});

test('the first release of a fresh repo', () => {
  assert.equal(version.next('v0.0.0', 'feature'), '0.1.0');
});

test('next rejects an unknown release type', () => {
  assert.throws(() => version.next('v1.2.3', 'patch'), /unknown release type: patch/);
});
