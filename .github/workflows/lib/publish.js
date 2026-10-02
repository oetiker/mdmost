// repo-infra: workflow-lib v6
'use strict';

// What the publish job does for a repository with release_build set (D26),
// decided from the tag and the releases together. Every late review of the
// spec found a release-stranding state in this machine, so it lives here,
// with one test per state, and the workflow only acts on the answer.
//
// Once the tag exists, the tag is the record of what was built; the
// release-build.json asset matters only before that (case 3).

const { BUILD_RECORD } = require('./release.js');

const fail = (message) => ({ action: 'fail', message });

function publishDecision({ tag, tagCommit, releases }) {
  const drafts = releases.filter((r) => r.draft);
  const published = releases.filter((r) => !r.draft);
  const record = (r) => r.assets.find((a) => a.name === BUILD_RECORD);

  if (published.some(record)) {
    return fail(`${tag}: a published release still carries ${BUILD_RECORD}. It was `
      + 'published by hand, which skips every publish add-on; check what the '
      + 'add-ons would have done, then delete the asset.');
  }

  if (tagCommit) {
    if (published.length === 1 && drafts.length === 0) return { action: 'done' };
    if (published.length === 0 && drafts.length === 1) {
      const carried = record(drafts[0]);
      return {
        action: 'resume',
        releaseId: drafts[0].id,
        head: tagCommit,
        ...(carried ? { recordAssetId: carried.id } : {}),
      };
    }
    if (releases.length === 0) {
      return fail(`${tag} exists but no release matches it (was a draft deleted after `
        + 'tagging?). Create a draft release for the tag that carries every '
        + 'release_assets file, then re-run this workflow.');
    }
    return fail(`${tag}: several releases match it: ${published.length} published, `
      + `${drafts.length} drafts. Delete the extra ones, then re-run.`);
  }

  if (published.length > 0) {
    return fail(`${tag} does not exist but a published release names it; `
      + 'this workflow does not know which commit it describes.');
  }
  if (drafts.length !== 1) {
    return fail(`${tag}: expected exactly one draft release built by the release `
      + `pull request, found ${drafts.length}.`);
  }
  const asset = record(drafts[0]);
  if (!asset) {
    return fail(`${tag}: the draft release carries no ${BUILD_RECORD}, so the built `
      + `commit is unknown. Push ${tag} at the built commit and re-run this workflow, `
      + 'or abandon the release with a pull request that moves its entries back '
      + 'under [Unreleased].');
  }
  return { action: 'create', releaseId: drafts[0].id, recordAssetId: asset.id };
}

// A hand-pushed tag on another commit than the built one resumes anyway (the
// tag is the record), so say so instead of doing it silently.
function recordHeadWarning(tag, tagCommit, recordHead) {
  if (recordHead === tagCommit) return null;
  return `${tag} points at ${tagCommit}, but ${BUILD_RECORD} names ${recordHead}. `
    + 'Publishing continues from the tag.';
}

function validateBuildRecord(record, version, latestAtHead) {
  if (!record || typeof record.head !== 'string' || !/^[0-9a-f]{40}$/.test(record.head)) {
    return `${BUILD_RECORD} names no head commit`;
  }
  if (!latestAtHead) {
    return `CHANGES.md at ${record.head} has no released version`;
  }
  if (latestAtHead.version !== version) {
    return `CHANGES.md at ${record.head} releases ${latestAtHead.version}, not ${version}`;
  }
  return null;
}

module.exports = { publishDecision, recordHeadWarning, validateBuildRecord };
