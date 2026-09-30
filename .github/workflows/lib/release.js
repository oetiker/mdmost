// repo-infra: workflow-lib v5
'use strict';

// Decisions of a release pull request that builds its release (D26). Each is
// a pure function over what the API returned, so every state the workflows
// can meet is a line in release.test.js rather than a hope in YAML.

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
// `repo_infra.state.refused_release_files` (used by `check`) enforces the same
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

module.exports = {
  BUILD_RECORD, BOT, isReleasePr, blockingReleasePr, untaggedRelease,
  staleDrafts, ownDrafts, refusedReleaseFiles, undeclaredReleaseFiles,
  decodeText, releaseBuilt,
};
