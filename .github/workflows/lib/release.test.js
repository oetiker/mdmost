// repo-infra: workflow-lib v8
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

// --- D28 ---------------------------------------------------------------

const BUILT = [{ context: 'release-built', state: 'success', creator: { login: r.BOT } }];
const STALE = 'main moved after v1.2.0 was built; close this pull request and dispatch '
  + 'Create release PR again';

test('releaseTag reads the tag off a release branch', () => {
  assert.equal(r.releaseTag('release/v1.2.0'), 'v1.2.0');
});

// A fake Octokit for releaseModeVerdict: the head's statuses and how far main
// is ahead of it. `calls` records every request.
function modeGithub(statuses, behindBy) {
  const calls = [];
  return {
    calls,
    rest: { repos: {
      listCommitStatusesForRef: 'statuses',
      compareCommitsWithBasehead: async (a) => {
        calls.push(a.basehead);
        return { data: { behind_by: behindBy } };
      },
    } },
    paginate: async (route, a) => {
      calls.push(`${route}@${a.ref}`);
      return statuses;
    },
  };
}
const releasePr = (opts) => ({ ...pr('release/v1.2.0', opts),
  head: { ...pr('release/v1.2.0', opts).head, sha: 'h' }, base: { ref: 'main' } });
const mode = (statuses, behindBy, opts) => r.releaseModeVerdict(modeGithub(statuses, behindBy),
  { owner: 'oetiker', repo: 'mdmost', pr: releasePr(opts) });

test('release mode passes a built head that main has not left behind', async () => {
  const github = modeGithub(BUILT, 0);
  const v = await r.releaseModeVerdict(github, { owner: 'oetiker', repo: 'mdmost',
    pr: releasePr() });
  assert.equal(v.ok, true);
  assert.deepEqual(github.calls, ['statuses@h', 'main...h']);
});

test('release mode fails a head without the release-built status', async () => {
  assert.deepEqual(await mode([], 0), { ok: false, message: r.CHANGED_AFTER_BUILD });
});

test('release mode fails a built head that main moved past', async () => {
  assert.deepEqual(await mode(BUILT, 3), { ok: false, message: STALE });
});

test('release mode names the missing status first (Update branch)', async () => {
  assert.equal((await mode([], 2)).message, r.CHANGED_AFTER_BUILD);
});

test('release mode leaves a fork\'s or a person\'s release branch to the ordinary rules', async () => {
  for (const opts of [{ login: 'oetiker' }, { repo: 'fork/mdmost' }, { repo: null }]) {
    const github = modeGithub(BUILT, 0);
    assert.equal(await r.releaseModeVerdict(github, { owner: 'oetiker', repo: 'mdmost',
      pr: releasePr(opts) }), null);
    assert.deepEqual(github.calls, []); // a fork's head may not compare at all
  }
});

test('the Update-branch text is the D26 text, unchanged', () => {
  assert.equal(r.CHANGED_AFTER_BUILD, 'the release branch changed after it was built (the '
    + 'Update branch button does this); close this pull request and dispatch Create release '
    + 'PR again');
});

test('finish passes a release main has not moved past', () => {
  const v = r.finishVerdict({ behindBy: 0, tag: 'v1.2.0' });
  assert.equal(v.conclusion, 'success');
});

test('finish fails a release main moved past while it built, with the stale text', () => {
  assert.deepEqual(r.finishVerdict({ behindBy: 1, tag: 'v1.2.0' }), {
    conclusion: 'failure', title: 'main moved after v1.2.0 was built', summary: STALE,
  });
});

test('staleReleasePr marks a release pull request main moved past', () => {
  const p = (ref, sha) => ({ ...pr(ref), number: 1, head: { ...pr(ref).head, sha } });
  assert.deepEqual(r.staleReleasePr(p('release/v1.2.0', 'aaa'), 2), {
    number: 1, sha: 'aaa', title: 'main moved after v1.2.0 was built', summary: STALE,
  });
  assert.equal(r.staleReleasePr(p('release/v1.3.0', 'bbb'), 0), null);
});

test('the untagged refusal no longer starts with re-run and names the tree comparison', () => {
  const m = r.untaggedMessage('1.2.0');
  assert.ok(m.startsWith('v1.2.0 is in CHANGES.md on main but has no tag. '));
  assert.match(m, /failed for another reason than the tree comparison/);
  assert.match(m, /push v1\.2\.0 by hand/);
  assert.match(m, /back under \[Unreleased\]/);
  assert.doesNotMatch(m, /^v1\.2\.0[^.]*\. Re-run/);
});

const merged = (ref, sha, opts = {}) => ({ ...pr(ref, opts), merged_at: opts.mergedAt
  === undefined ? '2026-10-01T10:00:00Z' : opts.mergedAt, merge_commit_sha: sha });

test('releasePrMergeCommit finds the merged release pull request of this version', () => {
  const prs = [
    merged('release/v1.2.0', 'closed1', { mergedAt: null }), // an abandoned attempt
    merged('release/v1.1.0', 'old'), // another version
    merged('release/v1.2.0', 'person', { login: 'oetiker' }),
    merged('release/v1.2.0', 'mmm'),
  ];
  assert.equal(r.releasePrMergeCommit(prs, { fullName: REPO, tag: 'v1.2.0' }), 'mmm');
  assert.equal(r.releasePrMergeCommit([], { fullName: REPO, tag: 'v1.2.0' }), null);
});

test('releasePrMergeCommit takes the release merged last, in any listing order', () => {
  // v1.2.0 abandoned once and released again: two merged pull requests, one branch.
  const first = merged('release/v1.2.0', 'abandoned', { mergedAt: '2026-10-01T10:00:00Z' });
  const again = merged('release/v1.2.0', 'again', { mergedAt: '2026-10-02T09:00:00Z' });
  const open = merged('release/v1.2.0', 'open', { mergedAt: null });
  for (const prs of [[first, again, open], [open, again, first]]) {
    assert.equal(r.releasePrMergeCommit(prs, { fullName: REPO, tag: 'v1.2.0' }), 'again');
  }
});

test('treeVerdict passes equal trees, also after a squash merge', () => {
  assert.equal(r.treeVerdict({ tag: 'v1.2.0', head: 'h', mergeSha: 'squash', mergeTree: 't',
    headTree: 't' }), null);
});

test('treeVerdict names main, the head and the way out when the trees differ', () => {
  assert.equal(r.treeVerdict({ tag: 'v1.2.0', head: 'h', mergeSha: 'm', mergeTree: 't1',
    headTree: 't2' }), 'main at m does not match the release built from h; merge a pull '
    + 'request that moves the v1.2.0 entries in CHANGES.md back under [Unreleased], then '
    + 'dispatch Create release PR again');
});

test('treeVerdict refuses when no release branch pull request was merged', () => {
  const m = r.treeVerdict({ tag: 'v1.2.0', head: 'h', mergeSha: null, mergeTree: null,
    headTree: 't' });
  assert.match(m, /no merged release pull request from release\/v1\.2\.0,/);
  assert.match(m, /Nothing was tagged/);
});

const run = (id, branch, opts = {}) => ({
  id, event: opts.event || 'pull_request', head_branch: branch,
  status: opts.status || 'completed', conclusion: opts.conclusion === undefined
    ? 'action_required' : opts.conclusion,
  head_repository: opts.repo === null ? null : { full_name: opts.repo || REPO },
  actor: { login: opts.actor || r.BOT },
  ...(opts.jobCount === undefined ? {} : { jobCount: opts.jobCount }),
});

test('parkedRuns keeps only parked pull_request runs of this repository\'s release branches', () => {
  const runs = [
    run(1, 'release/v1.2.0'), // parked
    run(2, 'release/v1.2.0', { status: 'action_required', conclusion: null }), // parked, other spelling
    run(3, 'release/v1.2.0', { conclusion: 'success' }), // someone approved it; it ran
    run(4, 'feature/x'), // not a release branch
    run(5, 'release/v1.2.0', { repo: 'fork/mdmost' }), // a fork's
    run(6, 'release/v1.2.0', { event: 'push' }),
    run(7, 'release/v1.1.0'),
    run(8, 'release/v1.2.0', { conclusion: 'failure', jobCount: 0 }), // parked, PR merged
    run(9, 'release/v1.2.0', { conclusion: 'failure', jobCount: 2 }), // approved, ran, failed
    run(10, 'release/v1.2.0', { conclusion: 'failure' }), // jobs not counted: not parked
    run(11, 'release/v1.2.0', { conclusion: 'failure', jobCount: 0, actor: 'oetiker' }),
    // a person's release/x with an invalid workflow file
  ];
  assert.deepEqual(r.parkedRuns(runs, { fullName: REPO }).map((x) => x.id), [1, 2, 7, 8]);
  assert.deepEqual(r.parkedRuns(runs, { fullName: REPO, branch: 'release/v1.2.0' })
    .map((x) => x.id), [1, 2, 8]);
  assert.deepEqual(r.parkedRuns(runs, { fullName: REPO, keep: ['release/v1.1.0'] })
    .map((x) => x.id), [1, 2, 8]);
});

// A fake Octokit for fetchParkedRuns: paginate lists the runs whose status or
// conclusion matches the `status` filter, as GitHub does, and
// listJobsForWorkflowRun answers each run's job count from `jobs`.
function runsGithub(runs, jobs = {}) {
  const calls = { listed: [], counted: [] };
  return {
    calls,
    rest: { actions: {
      listWorkflowRunsForRepo: 'listWorkflowRunsForRepo',
      listJobsForWorkflowRun: async (a) => {
        calls.counted.push(a.run_id);
        return { data: { total_count: jobs[a.run_id] || 0 } };
      },
    } },
    paginate: async (route, params) => {
      if (route !== 'listWorkflowRunsForRepo') throw new Error(`unexpected route ${route}`);
      calls.listed.push(params);
      return runs.filter((x) => x.status === params.status || x.conclusion === params.status);
    },
  };
}

test('fetchParkedRuns counts the jobs of failed release runs and keeps the parked ones', async () => {
  const github = runsGithub([
    run(1, 'release/v1.2.0'), // parked, pull request open
    run(2, 'release/v1.2.0', { conclusion: 'failure' }), // parked, pull request closed
    run(3, 'release/v1.2.0', { conclusion: 'failure' }), // approved, ran, failed
    run(4, 'feature/x', { conclusion: 'failure' }), // not a release branch: not counted
    run(5, 'release/v1.1.0', { conclusion: 'success' }),
  ], { 3: 2 });
  const parked = await r.fetchParkedRuns(github, { owner: 'oetiker', repo: 'mdmost' });
  assert.deepEqual(parked.map((x) => x.id), [1, 2]);
  assert.deepEqual(github.calls.counted, [2, 3]);
});

test('fetchParkedRuns lists only the bot\'s parked and failed pull_request runs', async () => {
  // GitHub stops a listing at 1000 runs; all pull_request runs of a busy
  // repository would hide the old parked ones.
  const github = runsGithub([]);
  await r.fetchParkedRuns(github, { owner: 'oetiker', repo: 'mdmost' });
  const common = { owner: 'oetiker', repo: 'mdmost', event: 'pull_request',
    actor: 'github-actions[bot]', per_page: 100 };
  assert.deepEqual(github.calls.listed, [{ ...common, status: 'action_required' },
    { ...common, status: 'failure' }]);
});

test('fetchParkedRuns for one branch lists that branch only', async () => {
  const github = runsGithub([
    run(1, 'release/v1.2.0', { conclusion: 'failure' }),
    run(2, 'release/v1.1.0'),
  ]);
  const parked = await r.fetchParkedRuns(github, {
    owner: 'oetiker', repo: 'mdmost', branch: 'release/v1.2.0',
  });
  assert.deepEqual(parked.map((x) => x.id), [1]);
  assert.deepEqual(github.calls.listed.map((x) => x.branch),
    ['release/v1.2.0', 'release/v1.2.0']);
});

test('fetchParkedRuns does not write the job count into the listed runs', async () => {
  const runs = [run(1, 'release/v1.2.0', { conclusion: 'failure' })];
  await r.fetchParkedRuns(runsGithub(runs), { owner: 'oetiker', repo: 'mdmost' });
  assert.equal('jobCount' in runs[0], false);
});
