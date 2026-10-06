// repo-infra: workflow-lib v6
'use strict';

// What `finalize` checks before it flips a release from draft to public.
//
// Until this existed, the only thing between that flip and a release with no
// artifacts on it was finalize's own `needs:` list. A `needs:` list is a
// generated line in a generated file, and losing it does not fail: finalize
// simply stops waiting and publishes a release whose .deb was never uploaded.
// Nothing goes red, because nothing went wrong -- it just did not wait.
//
// Ordering cannot report its own absence. An assertion can, and it cannot pass
// while being wrong: either the asset is on the release or it is not.

// A pattern is an asset name in which `*` stands for any run of characters --
// `*.deb`, `smtp-proxy-*-x86_64-unknown-linux-musl`. Deliberately not a full
// glob: the only thing that varies between releases is the version, and `*` is
// enough for it. A literal name would be the second place the version is
// written down, and it would go red on every release that did not update it.
function patternToRegExp(pattern) {
  const body = pattern
    .split('*')
    .map((part) => part.replace(/[.+?^${}()|[\]\\]/g, '\\$&'))
    .join('.*');
  return new RegExp(`^${body}$`);
}

function matches(name, pattern) {
  return patternToRegExp(pattern).test(name);
}

// The expected patterns that nothing on the release satisfies, in the order
// they were declared. Empty means the release carries everything it should.
//
// Note what this does NOT do: it never complains about an asset it did not
// expect. A repository that attaches something extra by hand is not broken,
// and a guard that failed on it would train people to route around finalize.
function missingAssets(names, patterns) {
  const present = names || [];
  return (patterns || []).filter(
    (pattern) => !present.some((name) => matches(name, pattern)),
  );
}

// Names of attached assets whose upload did not finish. GitHub keeps an asset
// in state `starter` or `open` when the upload broke half way, and it still
// lists by name, so a name check alone lets a broken file go public.
function unfinishedAssets(attached) {
  return (attached || []).filter((a) => a.state !== 'uploaded').map((a) => a.name);
}

module.exports = { matches, missingAssets, unfinishedAssets };
