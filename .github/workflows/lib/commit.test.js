// repo-infra: workflow-lib v6
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const commit = require('./commit.js');

function fakeGithub() {
  const seen = { blobs: [], trees: [], commits: [], refs: [] };
  return {
    seen,
    rest: {
      git: {
        getCommit: async ({ commit_sha: sha }) => ({
          data: { sha, tree: { sha: `tree-of-${sha}` } },
        }),
        createBlob: async (params) => {
          seen.blobs.push(params);
          return { data: { sha: `blob-${seen.blobs.length}` } };
        },
        createTree: async (params) => {
          seen.trees.push(params);
          return { data: { sha: 'new-tree' } };
        },
        createCommit: async (params) => {
          seen.commits.push(params);
          return { data: { sha: 'new-commit' } };
        },
        createRef: async (params) => {
          seen.refs.push(params);
          return { data: { ref: params.ref } };
        },
      },
    },
  };
}

/// The API's answer when the branch is still there: a closed-but-undeleted
/// release PR, or a run that committed and then failed before opening one.
function refExistsError() {
  const e = new Error('Reference already exists');
  e.status = 422;
  return e;
}

function githubWithExistingRef() {
  const github = fakeGithub();
  github.rest.git.createRef = async (params) => {
    github.seen.refs.push(params);
    throw refExistsError();
  };
  github.seen.updates = [];
  github.rest.git.updateRef = async (params) => {
    github.seen.updates.push(params);
    return { data: { ref: params.ref } };
  };
  return github;
}

const ARGS = {
  owner: 'oposs',
  repo: 'repo-infra',
  branch: 'release/v1.2.3',
  baseSha: 'base-sha',
  message: 'Release v1.2.3',
  files: [
    { path: 'CHANGES.md', content: '# Changelog\n' },
    { path: '.claude-plugin/plugin.json', content: '{}\n' },
  ],
};

test('creates one blob per file, utf-8 encoded', async () => {
  const github = fakeGithub();
  await commit.commitFiles(github, ARGS);
  assert.equal(github.seen.blobs.length, 2);
  assert.equal(github.seen.blobs[0].content, '# Changelog\n');
  assert.equal(github.seen.blobs[0].encoding, 'utf-8');
});

test('the tree is based on the base commit tree, not the commit sha', async () => {
  // createTree's base_tree wants a tree SHA. Passing a commit SHA is the
  // mistake that silently produces a tree with no history behind it.
  const github = fakeGithub();
  await commit.commitFiles(github, ARGS);
  assert.equal(github.seen.trees[0].base_tree, 'tree-of-base-sha');
});

test('every entry is a normal file blob', async () => {
  const github = fakeGithub();
  await commit.commitFiles(github, ARGS);
  for (const entry of github.seen.trees[0].tree) {
    assert.equal(entry.mode, '100644');
    assert.equal(entry.type, 'blob');
  }
  assert.deepEqual(
    github.seen.trees[0].tree.map((e) => e.path),
    ['CHANGES.md', '.claude-plugin/plugin.json'],
  );
});

test('the commit has the base as its only parent', async () => {
  const github = fakeGithub();
  await commit.commitFiles(github, ARGS);
  assert.deepEqual(github.seen.commits[0].parents, ['base-sha']);
  assert.equal(github.seen.commits[0].tree, 'new-tree');
  assert.equal(github.seen.commits[0].message, 'Release v1.2.3');
});

test('the branch ref is created and the commit sha returned', async () => {
  const github = fakeGithub();
  const sha = await commit.commitFiles(github, ARGS);
  assert.equal(github.seen.refs[0].ref, 'refs/heads/release/v1.2.3');
  assert.equal(github.seen.refs[0].sha, 'new-commit');
  assert.equal(sha, 'new-commit');
});

test('an empty file list is refused', async () => {
  const github = fakeGithub();
  await assert.rejects(
    () => commit.commitFiles(github, { ...ARGS, files: [] }),
    /nothing to commit/,
  );
});

test('a branch left over from an earlier run is moved, not fatal', async () => {
  // Re-running the release workflow used to die here with an opaque 422.
  // The operator's only way out was deleting the branch by hand.
  const github = githubWithExistingRef();
  const sha = await commit.commitFiles(github, ARGS);
  assert.equal(sha, 'new-commit');
  assert.equal(github.seen.updates.length, 1);
  assert.equal(github.seen.updates[0].ref, 'heads/release/v1.2.3');
  assert.equal(github.seen.updates[0].sha, 'new-commit');
  assert.equal(github.seen.updates[0].force, true);
});

test('an error that is not "ref exists" still fails the run', async () => {
  const github = fakeGithub();
  github.rest.git.createRef = async () => {
    const e = new Error('Bad credentials');
    e.status = 401;
    throw e;
  };
  await assert.rejects(() => commit.commitFiles(github, ARGS), /Bad credentials/);
});
