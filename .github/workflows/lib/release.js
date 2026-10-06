// repo-infra: workflow-lib v8
//
// Purpose: The JavaScript the release flow and the publish pieces run (D8).
// Choose: Always.
// Supplies: Nothing.

'use strict';

// Decisions of the release flow that builds and tests a release before its
// pull request exists (D28). Each is a pure function over what the API
// returned, so every state the workflows can meet is a line in
// release.test.js rather than a hope in YAML.

const path = require('path');

const BUILD_RECORD = 'release-build.json';
const BOT = 'github-actions[bot]';

// A fork can name a branch release/x, and a person can push one. Neither is
// the pull request `Create release PR` opened, and neither may block it or
// be judged by the release-built status.
function isReleasePr(pr, fullName) {
  return pr.head.ref.startsWith('release/')
    && Boolean(pr.head.repo) && pr.head.repo.full_name === fullName
    && pr.user.login === BOT;
}

function blockingReleasePr(prs, fullName) {
  return prs.find((pr) => isReleasePr(pr, fullName)) || null;
}

// A merged release whose publish has not finished (or failed). Dispatching
// again would compute the same version from the tags, and `roll` would
// write a second heading for it into CHANGES.md.
function untaggedRelease(latest, tags) {
  if (!latest) return null;
  return tags.includes(`v${latest.version}`) ? null : latest.version;
}

const carriesRecord = (release) => release.assets.some((a) => a.name === BUILD_RECORD);

// Drafts a closed release pull request left behind. Only ours (they carry the
// build record), only untagged, and never the latest release in CHANGES.md:
// that one belongs to a merged release publish has not finished.
function staleDrafts(releases, { tags, latestVersion }) {
  return releases.filter((release) => release.draft && carriesRecord(release)
    && !tags.includes(release.tag_name)
    && release.tag_name !== `v${latestVersion}`);
}

// This version's drafts from an earlier attempt. A finish that failed while
// uploading leaves a draft by the bot without the build record, so a draft
// counts as ours when it carries the record or the bot created it. A
// record-less draft by anyone else is left alone.
function ownDrafts(releases, version) {
  return releases.filter((release) => release.draft
    && release.tag_name === `v${version}`
    && (carriesRecord(release) || release.author?.login === BOT));
}

function normalise(entry) {
  return path.posix.normalize(entry).replace(/\/+$/, '');
}

// The build job is read-only. Its one way to write the repository is the
// release-files artifact, which `finish` commits. These paths would turn that
// channel into a way to rewrite the changelog, a version or a workflow.
// `repo_infra.check.refused_release_files` (used by `check`) enforces the same
// rule; both must change together.
function refusedReleaseFiles(entries, versionFiles) {
  const versions = new Set((versionFiles || []).map((f) => normalise(f.path)));
  const refused = [];
  for (const entry of entries) {
    const reason = (() => {
      if (typeof entry !== 'string' || entry === '') return 'is empty';
      if (path.posix.isAbsolute(entry)) return 'is an absolute path';
      const p = normalise(entry);
      if (p === '..' || p.startsWith('../')) return 'points outside the repository';
      if (p === 'CHANGES.md') return 'is CHANGES.md, which the release pull request rolls';
      if (versions.has(p)) return 'is a version file, which the release pull request bumps';
      if (p === '.github' || p.startsWith('.github/')) return 'is under .github/';
      return null;
    })();
    if (reason) refused.push({ path: entry, reason });
  }
  return refused;
}

function undeclaredReleaseFiles(paths, declared) {
  const allowed = new Set(declared.map(normalise));
  return paths.filter((p) => !allowed.has(normalise(p)));
}

// The commit library writes strings. A binary file would be mangled on the
// way into the release branch, so it is refused instead.
function decodeText(buffer) {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(buffer);
  } catch (error) {
    return null;
  }
}

// Anyone who can push can set a status of any name, so this guards against
// the Update branch button, not against people with write access.
function releaseBuilt(statuses) {
  return statuses.some((s) => s.context === 'release-built'
    && s.state === 'success' && s.creator && s.creator.login === BOT);
}

// --- D28: one release flow, tested before the merge ------------------------

const CHANGED_AFTER_BUILD = 'the release branch changed after it was built (the Update '
  + 'branch button does this); close this pull request and dispatch Create release PR again';

function releaseTag(ref) {
  return ref.replace(/^release\//, '');
}

function staleMessage(tag) {
  return `main moved after ${tag} was built; close this pull request and dispatch `
    + 'Create release PR again';
}

function judgeReleaseHead({ statuses, behindBy, tag }) {
  if (!releaseBuilt(statuses)) return { ok: false, message: CHANGED_AFTER_BUILD };
  if (behindBy > 0) return { ok: false, message: staleMessage(tag) };
  return {
    ok: true,
    message: `${tag}: this head was built and tested, and main has not moved since.`,
  };
}

// ci-passed and changelog-updated on a pull request from a release/* branch.
// null: not the pull request Create release PR opened (a fork's, a person's),
// and the ordinary rules apply. It takes the pull request rather than its
// statuses, so no caller can judge a fork's branch by the release rules; and
// it reads the API only for a release pull request, since comparing a fork's
// head can fail. The test results do not count: finish built and tested
// exactly the head that carries release-built, and the up-to-date rule needs
// it not to be behind main. The answer must equal finishVerdict's and
// staleReleasePr's, or an approved parked run would turn their red check green.
async function releaseModeVerdict(github, { owner, repo, pr }) {
  if (!isReleasePr(pr, `${owner}/${repo}`)) return null;
  // The head commit's statuses, not the merge commit's: finish sets
  // release-built on the head it built.
  const statuses = await github.paginate(github.rest.repos.listCommitStatusesForRef, {
    owner, repo, ref: pr.head.sha, per_page: 100,
  });
  const { data: compared } = await github.rest.repos.compareCommitsWithBasehead({
    owner, repo, basehead: `${pr.base.ref}...${pr.head.sha}`,
  });
  return judgeReleaseHead({
    statuses, behindBy: compared.behind_by, tag: releaseTag(pr.head.ref),
  });
}

// finish opens the pull request first and judges it afterwards, so a release
// that went stale while it built is visible and says why.
function finishVerdict({ behindBy, tag }) {
  if (behindBy > 0) {
    return { conclusion: 'failure', title: `main moved after ${tag} was built`,
      summary: staleMessage(tag) };
  }
  return { conclusion: 'success', title: `${tag} is built and tested`,
    summary: `Create release PR built and tested ${tag} on the current main.` };
}

// release-pr-current: the failed check for a release pull request main has
// moved past, or null. The caller lists the open pull requests and keeps the
// ones isReleasePr accepts before it compares each with main.
function staleReleasePr(pr, behindBy) {
  if (!(behindBy > 0)) return null;
  const tag = releaseTag(pr.head.ref);
  return { number: pr.number, sha: pr.head.sha,
    title: `main moved after ${tag} was built`, summary: staleMessage(tag) };
}

// A re-run helps only when publish failed for a reason other than the tree
// comparison; after that one, abandoning is the way out.
function untaggedMessage(version) {
  return `v${version} is in CHANGES.md on main but has no tag. If its Publish release run `
    + 'failed for another reason than the tree comparison, re-run its failed jobs; if it '
    + `is already out under another tag, push v${version} by hand; to abandon it, merge a `
    + 'pull request that moves its entries back under [Unreleased].';
}

// The release pull request whose merge put this version on main, found by its
// release branch (pulls.list). Not by the recorded head: for a commit off the
// default branch GitHub lists only open pull requests. Not context.sha: a failed
// first publish followed by an ordinary merge starts a new run on a later commit.
// A version abandoned and released again leaves two merged pull requests from
// one branch; the one merged last put this version on main. The listing order
// does not decide it.
function releasePrMergeCommit(prs, { fullName, tag }) {
  const found = prs
    .filter((pr) => isReleasePr(pr, fullName) && pr.head.ref === `release/${tag}`
      && pr.merged_at)
    .sort((a, b) => Date.parse(b.merged_at) - Date.parse(a.merged_at));
  return found.length ? found[0].merge_commit_sha : null;
}

// With the up-to-date rule on, the merge commit's tree is the built head's
// tree (also after a squash or rebase). A mismatch means the rule was off.
function treeVerdict({ tag, head, mergeSha, mergeTree, headTree }) {
  if (!mergeSha) {
    return `${tag}: no merged release pull request from release/${tag}, so publish cannot `
      + 'compare main with the release that was built. Nothing was tagged.';
  }
  if (mergeTree === headTree) return null;
  return `main at ${mergeSha} does not match the release built from ${head}; merge a pull `
    + `request that moves the ${tag} entries in CHANGES.md back under [Unreleased], then `
    + 'dispatch Create release PR again';
}

// The pull_request runs of a release branch park for an approval nobody needs
// to give. A run someone approved ran, and is kept. The bot opened the pull
// request, so it is the run's actor, also after an approval (the approver is
// the triggering actor). A person's own release/x whose run failed with no
// jobs (an invalid workflow file) is not ours to delete. While the pull request is
// open a parked run reads status `completed`, conclusion `action_required`.
// Once it merges or closes, GitHub turns the same run into conclusion
// `failure` with no jobs (seen on oetiker/repo-infra-spike, run 36740994466),
// so fetchParkedRuns counts the jobs of each failed run into `jobCount`.
const isParked = (run) => run.status === 'action_required'
  || run.conclusion === 'action_required'
  || (run.conclusion === 'failure' && run.jobCount === 0);

function parkedRuns(runs, { fullName, branch = null, keep = [] }) {
  return runs.filter((run) => run.event === 'pull_request' && isParked(run)
    && Boolean(run.actor) && run.actor.login === BOT
    && Boolean(run.head_repository) && run.head_repository.full_name === fullName
    && typeof run.head_branch === 'string' && run.head_branch.startsWith('release/')
    && (branch === null || run.head_branch === branch)
    && !keep.includes(run.head_branch));
}

// The parked runs of this repository's release branches, or of one branch.
// Prepare deletes those of abandoned branches, publish those of the branch it
// released; both need `actions: write` for that, and the listing here needs
// `actions: read`. Only the failed runs of release branches have their jobs
// counted, one request each.
//
// GitHub returns at most 1000 runs for one listing, so listing every
// pull_request run of a busy repository would never reach its old parked
// runs. The listing asks for the bot's runs with one of the two conclusions a
// parked run has, which leaves the runs of release pull requests only.
async function fetchParkedRuns(github, { owner, repo, branch = null }) {
  const listed = [];
  for (const status of ['action_required', 'failure']) {
    listed.push(...await github.paginate(github.rest.actions.listWorkflowRunsForRepo, {
      owner, repo, event: 'pull_request', actor: BOT, status,
      ...(branch === null ? {} : { branch }), per_page: 100,
    }));
  }
  const runs = [];
  for (const run of listed) {
    if (!(run.head_branch || '').startsWith('release/')) continue;
    if (run.conclusion !== 'failure') {
      runs.push(run);
      continue;
    }
    const { data } = await github.rest.actions.listJobsForWorkflowRun({
      owner, repo, run_id: run.id, per_page: 1,
    });
    runs.push({ ...run, jobCount: data.total_count });
  }
  return parkedRuns(runs, { fullName: `${owner}/${repo}`, branch });
}

module.exports = {
  BUILD_RECORD, BOT, isReleasePr, blockingReleasePr, untaggedRelease,
  staleDrafts, ownDrafts, refusedReleaseFiles, undeclaredReleaseFiles,
  decodeText, releaseBuilt,
  CHANGED_AFTER_BUILD, releaseTag, staleMessage, releaseModeVerdict, finishVerdict,
  staleReleasePr, untaggedMessage, releasePrMergeCommit, treeVerdict, parkedRuns,
  fetchParkedRuns,
};
