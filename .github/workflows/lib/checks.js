// repo-infra: workflow-lib v5
'use strict';

// Everything that reported on the commit, whatever workflow produced it. The
// previous version polled listWorkflowRuns for a hardcoded 'test.yml', which
// saw one workflow and broke whenever a repo named its CI something else.
const PASSING = new Set(['success', 'neutral', 'skipped']);

async function checkState(github, { owner, repo, ref }, opts = {}) {
  const all = await github.paginate(github.rest.checks.listForRef, {
    owner, repo, ref, per_page: 100,
  });

  // A job that waits for the checks on its own commit is itself one of those
  // checks, so without this the caller waits for the job doing the waiting.
  // Ids are the Actions job ids: a check run's id and its job id are the same
  // number, so listJobsForWorkflowRun(context.runId) yields exactly this set.
  const ignore = new Set(opts.ignoreCheckRunIds || []);
  const runs = all.filter((r) => !ignore.has(r.id));

  const pending = runs.filter((r) => r.status !== 'completed');
  const failed = runs.filter(
    (r) => r.status === 'completed' && !PASSING.has(r.conclusion),
  );

  return {
    total: runs.length,
    pending,
    failed,
    // Zero checks is not success. Releasing a commit that nothing tested is
    // exactly the state this guard exists to prevent.
    ok: runs.length > 0 && pending.length === 0 && failed.length === 0,
  };
}

async function waitForChecks(github, params, opts = {}) {
  const intervalMs = opts.intervalMs ?? 15000;
  const timeoutMs = opts.timeoutMs ?? 15 * 60 * 1000;
  const sleep = opts.sleep ?? ((ms) => new Promise((r) => { setTimeout(r, ms); }));
  const now = opts.now ?? (() => Date.now());

  const started = now();
  for (;;) {
    const state = await checkState(github, params, opts);
    if (state.failed.length > 0) return state;
    if (state.pending.length === 0) return state;
    if (now() - started >= timeoutMs) return { ...state, timedOut: true };
    await sleep(intervalMs);
  }
}

// The workflow file name out of GITHUB_WORKFLOW_REF, which looks like
//   owner/repo/.github/workflows/release-pr.yml@refs/heads/main
function workflowFile(workflowRef) {
  const m = /\.github\/workflows\/([^@]+)/.exec(workflowRef || '');
  return m ? m[1] : null;
}

// Every check run on this commit that this workflow produced, in ANY of its
// runs -- which is the set a guard waiting on its own commit must ignore.
//
// Ignoring only the current run is not enough, and the failure it causes is
// permanent. Seen on oetiker/smalti's first release: attempt 1 died on a
// permissions 403, leaving a failed check run on the commit; attempt 2 saw
// that dead run, said "Failing checks on this commit: Prepare the release
// pull request", and refused. Attempt 3 saw two. Check runs cannot be
// deleted, so the commit could never be released, and deleting the release
// branch did not help -- the block is attached to the commit, not the branch.
//
// A check run's id is its Actions job id, so the job lists of this workflow's
// runs on this commit are exactly the ids to drop. Reading them needs the
// `actions: read` permission on the calling workflow.
async function guardIgnoreIds(github, {
  owner, repo, ref, workflowRef, runId,
}) {
  const jobsOf = async (run_id) => github.paginate(
    github.rest.actions.listJobsForWorkflowRun,
    { owner, repo, run_id, per_page: 100 },
  );

  const ids = new Set();
  const file = workflowFile(workflowRef);
  if (file) {
    const runs = await github.paginate(github.rest.actions.listWorkflowRuns, {
      owner, repo, workflow_id: file, head_sha: ref, per_page: 100,
    });
    for (const run of runs) {
      (await jobsOf(run.id)).forEach((j) => ids.add(j.id));
    }
  }
  // Always this run too. A run that has only just started can be missing from
  // listWorkflowRuns for a moment, and losing our own job id there is the
  // original deadlock: the guard waits for the job doing the waiting.
  if (runId) (await jobsOf(runId)).forEach((j) => ids.add(j.id));
  return [...ids];
}

module.exports = { checkState, waitForChecks, guardIgnoreIds };
