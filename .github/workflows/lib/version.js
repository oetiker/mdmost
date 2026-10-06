// repo-infra: workflow-lib v8
'use strict';

// Release tags are exactly vX.Y.Z. Pre-release and build suffixes are deliberately
// not matched: the release flow has no concept of them, and quietly accepting one
// would let it compute a "next" version from a tag it cannot reproduce.
const TAG_RE = /^v(\d+)\.(\d+)\.(\d+)$/;

function parse(tag) {
  const m = TAG_RE.exec(tag);
  if (!m) return null;
  return { major: Number(m[1]), minor: Number(m[2]), patch: Number(m[3]) };
}

function compare(a, b) {
  const pa = parse(a);
  const pb = parse(b);
  if (!pa) throw new Error(`not a release tag: ${a}`);
  if (!pb) throw new Error(`not a release tag: ${b}`);
  return (pa.major - pb.major) || (pa.minor - pb.minor) || (pa.patch - pb.patch);
}

function latest(tags) {
  const releases = tags.filter((t) => TAG_RE.test(t));
  if (releases.length === 0) return 'v0.0.0';
  return releases.sort(compare)[releases.length - 1];
}

function next(latestTag, releaseType) {
  const v = parse(latestTag);
  if (!v) throw new Error(`not a release tag: ${latestTag}`);
  switch (releaseType) {
    case 'major': return `${v.major + 1}.0.0`;
    case 'feature': return `${v.major}.${v.minor + 1}.0`;
    case 'bugfix': return `${v.major}.${v.minor}.${v.patch + 1}`;
    default: throw new Error(`unknown release type: ${releaseType}`);
  }
}

module.exports = { parse, compare, latest, next };
