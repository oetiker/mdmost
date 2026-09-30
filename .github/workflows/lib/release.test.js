// repo-infra: workflow-lib v5
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const r = require('./release.js');

const REPO = 'oetiker/mdmost';
const pr = (ref, { repo = REPO, login = r.BOT } = {}) => ({
  number: 7, head: { ref, repo: repo && { full_name: repo } }, user: { login },
});
const release = (tag, draft, assets = [r.BUILD_RECORD], id = 1) => ({
  id, tag_name: tag, draft, assets: assets.map((name, i) => ({ id: 100 + i, name })),
});

test('a release pull request is a bot branch release/* in this repository', () => {
  assert.equal(r.isReleasePr(pr('release/v1.2.0'), REPO), true);
});

test('a fork naming a branch release/x is not a release pull request', () => {
  assert.equal(r.isReleasePr(pr('release/x', { repo: 'someone/mdmost' }), REPO), false);
});

test('a person pushing release/x is not a release pull request', () => {
  assert.equal(r.isReleasePr(pr('release/x', { login: 'oetiker' }), REPO), false);
});

test('a deleted fork has no head repo and is not a release pull request', () => {
  assert.equal(r.isReleasePr(pr('release/x', { repo: null }), REPO), false);
});

test('blockingReleasePr finds the one that blocks', () => {
  const prs = [pr('feature/a'), pr('release/v1.2.0')];
  assert.equal(r.blockingReleasePr(prs, REPO).head.ref, 'release/v1.2.0');
  assert.equal(r.blockingReleasePr([pr('feature/a')], REPO), null);
});

test('the latest CHANGES.md release without a tag is reported', () => {
  const latest = { version: '1.2.0', date: '2026-09-29' };
  assert.equal(r.untaggedRelease(latest, ['v1.1.0']), '1.2.0');
  assert.equal(r.untaggedRelease(latest, ['v1.1.0', 'v1.2.0']), null);
  assert.equal(r.untaggedRelease(null, []), null);
});

test('a draft with a build record, no tag, not the latest release is stale', () => {
  const releases = [
    release('v1.2.0', true, undefined, 1), // abandoned pull request
    release('v1.1.1', true, undefined, 2), // merged, publish not finished
    release('v1.0.0', true, ['notes.txt'], 3), // someone's own draft
    release('v1.1.0', false, [], 4), // published
    release('v0.9.0', true, undefined, 5), // tagged
  ];
  const stale = r.staleDrafts(releases, { tags: ['v0.9.0', 'v1.1.0'], latestVersion: '1.1.1' });
  assert.deepEqual(stale.map((x) => x.id), [1]);
});

test('ownDrafts picks this version\'s drafts that carry a build record', () => {
  const releases = [release('v1.2.0', true, undefined, 1), release('v1.2.0', true, ['x'], 2),
    release('v1.3.0', true, undefined, 3)];
  assert.deepEqual(r.ownDrafts(releases, '1.2.0').map((x) => x.id), [1]);
});

test('ownDrafts also picks a bot draft without a build record (partial finish)', () => {
  const draft = (tag, author, id) => ({ ...release(tag, true, [], id), author });
  const releases = [
    draft('v1.2.0', { login: 'github-actions[bot]' }, 1),
    draft('v1.2.0', { login: 'alice' }, 2),
    draft('v1.3.0', { login: 'github-actions[bot]' }, 3),
    draft('v1.2.0', null, 4),
  ];
  assert.deepEqual(r.ownDrafts(releases, '1.2.0').map((x) => x.id), [1]);
});

const VERSION_FILES = [{ path: 'Cargo.toml' }];

test('a formula path is an acceptable release file', () => {
  assert.deepEqual(r.refusedReleaseFiles(['Formula/mdmost.rb'], VERSION_FILES), []);
});

for (const [entry, why] of [
  ['CHANGES.md', /CHANGES\.md/],
  ['./CHANGES.md', /CHANGES\.md/],
  ['Formula/../CHANGES.md', /CHANGES\.md/],
  ['Cargo.toml', /version file/],
  ['.github/repo-infra.json', /\.github/],
  ['.github//workflows/ci.yml', /\.github/],
  ['/etc/passwd', /absolute/],
  ['../outside', /outside/],
  ['', /empty/],
]) {
  test(`release file ${JSON.stringify(entry)} is refused`, () => {
    const refused = r.refusedReleaseFiles([entry], VERSION_FILES);
    assert.equal(refused.length, 1);
    assert.match(refused[0].reason, why);
  });
}

test('undeclared build output is named after normalising both sides', () => {
  assert.deepEqual(
    r.undeclaredReleaseFiles(['Formula/mdmost.rb', 'CHANGES.md'], ['./Formula/mdmost.rb']),
    ['CHANGES.md'],
  );
});

test('decodeText accepts UTF-8 and refuses anything else', () => {
  assert.equal(r.decodeText(Buffer.from('class Mdmost < Formula\n')), 'class Mdmost < Formula\n');
  assert.equal(r.decodeText(Buffer.from([0xff, 0xfe, 0x00])), null);
});

test('releaseBuilt needs a successful release-built status from the bot', () => {
  const s = (context, state, login) => ({ context, state, creator: { login } });
  assert.equal(r.releaseBuilt([s('release-built', 'success', r.BOT)]), true);
  assert.equal(r.releaseBuilt([s('release-built', 'success', 'oetiker')]), false);
  assert.equal(r.releaseBuilt([s('release-built', 'failure', r.BOT)]), false);
  assert.equal(r.releaseBuilt([s('ci-passed', 'success', r.BOT)]), false);
  assert.equal(r.releaseBuilt([]), false);
});
