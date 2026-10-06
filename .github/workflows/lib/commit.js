// repo-infra: workflow-lib v6
'use strict';

// Commits go through the Git Data API rather than `git commit && git push`.
// Two reasons: no `git config user.email` dance repeated in every repository,
// and the commit lands atomically instead of as a sequence that can half-fail.
async function commitFiles(github, {
  owner, repo, branch, baseSha, message, files,
}) {
  if (!files || files.length === 0) {
    throw new Error('nothing to commit - the file list is empty');
  }

  // createTree's base_tree takes a *tree* SHA. Resolve the base commit to its
  // tree rather than passing the commit SHA and hoping it is dereferenced.
  const { data: base } = await github.rest.git.getCommit({
    owner, repo, commit_sha: baseSha,
  });

  const tree = [];
  for (const file of files) {
    const { data: blob } = await github.rest.git.createBlob({
      owner, repo, content: file.content, encoding: 'utf-8',
    });
    tree.push({
      path: file.path, mode: '100644', type: 'blob', sha: blob.sha,
    });
  }

  const { data: newTree } = await github.rest.git.createTree({
    owner, repo, base_tree: base.tree.sha, tree,
  });

  const { data: newCommit } = await github.rest.git.createCommit({
    owner, repo, message, tree: newTree.sha, parents: [baseSha],
  });

  // Create, then move on "already exists". A release branch survives a
  // closed-but-undeleted PR, and a run that committed before failing to open
  // one leaves the branch behind too -- in both cases createRef answers 422
  // and the next run used to die with an opaque API error that only a manual
  // branch delete cleared. Create first rather than checking first: the check
  // would be a race, and 422 is the authoritative answer.
  try {
    await github.rest.git.createRef({
      owner, repo, ref: `refs/heads/${branch}`, sha: newCommit.sha,
    });
  } catch (e) {
    if (e.status !== 422) throw e;
    // Forced because the leftover branch holds an older release commit that
    // nothing is building on: this run's commit is the one to keep.
    await github.rest.git.updateRef({
      owner, repo, ref: `heads/${branch}`, sha: newCommit.sha, force: true,
    });
  }

  return newCommit.sha;
}

module.exports = { commitFiles };
