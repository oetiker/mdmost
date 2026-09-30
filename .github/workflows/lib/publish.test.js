// repo-infra: workflow-lib v5
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { publishDecision, recordHeadWarning, validateBuildRecord } = require('./publish.js');

const TAG = 'v1.2.0';
const HEAD = 'a'.repeat(40);
let nextId = 1;
const rel = (draft, record) => ({
  id: nextId++, tag_name: TAG, draft,
  assets: record ? [{ id: 900, name: 'release-build.json' }, { id: 901, name: 'x.deb' }]
    : [{ id: 901, name: 'x.deb' }],
});
const decide = (tagCommit, releases) => publishDecision({ tag: TAG, tagCommit, releases });

// The whole state table of spec D26 "Publishing": tag present/absent x
// published/draft/none/several x build record present/absent.

test('case 1: tag, one published release, no record: done', () => {
  assert.deepEqual(decide(HEAD, [rel(false, false)]), { action: 'done' });
});

test('case 2: tag and one draft: resume at the tag commit (record or not)', () => {
  for (const record of [true, false]) {
    const d = rel(true, record);
    const want = { action: 'resume', releaseId: d.id, head: HEAD };
    if (record) want.recordAssetId = 900;
    assert.deepEqual(decide(HEAD, [d]), want);
  }
});

test('case 3: no tag and one draft with a record: create', () => {
  const d = rel(true, true);
  assert.deepEqual(decide(null, [d]), { action: 'create', releaseId: d.id, recordAssetId: 900 });
});

test('case 3 refused: no tag and one draft without a record', () => {
  const out = decide(null, [rel(true, false)]);
  assert.equal(out.action, 'fail');
  assert.match(out.message, /release-build\.json/);
});

test('case 3 refused: no tag and no draft names the count', () => {
  const out = decide(null, []);
  assert.equal(out.action, 'fail');
  assert.match(out.message, /found 0/);
});

test('case 3 refused: no tag and two drafts names the count', () => {
  const out = decide(null, [rel(true, true), rel(true, true)]);
  assert.equal(out.action, 'fail');
  assert.match(out.message, /found 2/);
});

test('case 4: tag but no release at all', () => {
  const out = decide(HEAD, []);
  assert.equal(out.action, 'fail');
  assert.match(out.message, /no release/);
});

test('case 4: a published release that still carries the record was published by hand', () => {
  for (const tagCommit of [HEAD, null]) {
    const out = decide(tagCommit, [rel(false, true)]);
    assert.equal(out.action, 'fail');
    assert.match(out.message, /by hand/);
  }
});

test('case 4: tag and several matching releases', () => {
  const out = decide(HEAD, [rel(true, false), rel(true, true)]);
  assert.equal(out.action, 'fail');
  assert.match(out.message, /2 drafts/);
});

test('case 4: tag, a published release and a draft', () => {
  assert.equal(decide(HEAD, [rel(false, false), rel(true, true)]).action, 'fail');
});

test('case 4: no tag but a published release', () => {
  assert.equal(decide(null, [rel(false, false)]).action, 'fail');
});

test('a valid record passes', () => {
  assert.equal(validateBuildRecord({ head: HEAD }, '1.2.0', { version: '1.2.0' }), null);
});

test('a record without a commit sha is refused', () => {
  assert.match(validateBuildRecord({}, '1.2.0', { version: '1.2.0' }), /head/);
  assert.match(validateBuildRecord({ head: 'main' }, '1.2.0', { version: '1.2.0' }), /head/);
});

test('a head whose CHANGES.md is at another version is refused', () => {
  assert.match(validateBuildRecord({ head: HEAD }, '1.2.0', { version: '1.1.0' }), /1\.1\.0/);
  assert.match(validateBuildRecord({ head: HEAD }, '1.2.0', null), /no released version/);
});

test('recovery hints name ways out that prepare does not refuse', () => {
  const noRelease = decide(HEAD, []).message;
  assert.match(noRelease, /carries every release_assets file/);
  const noRecord = decide(null, [rel(true, false)]).message;
  assert.match(noRecord, /Push v1\.2\.0 at the built commit/);
  assert.match(noRecord, /back under \[Unreleased\]/);
  assert.doesNotMatch(noRecord, /Dispatch/);
});

test('recordHeadWarning: silent when the tag is at the built commit, else names both', () => {
  assert.equal(recordHeadWarning(TAG, HEAD, HEAD), null);
  const other = 'b'.repeat(40);
  const w = recordHeadWarning(TAG, HEAD, other);
  assert.ok(w.includes(HEAD) && w.includes(other));
});
