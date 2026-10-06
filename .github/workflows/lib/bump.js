// repo-infra: workflow-lib v8
'use strict';

function escapeRegExp(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// The verify pattern is a template containing $VERSION. The version is escaped
// before substitution so its dots match literally: an unescaped '1.2.3' would
// also match '1x2x3', and (worse) '1.2.3' would match inside '1.2.30'.
function verifyPattern(spec, version) {
  return new RegExp(spec.verify.replace(/\$VERSION/g, escapeRegExp(version)) + '(?![0-9])', 'm');
}

function verifyFile(spec, version, io) {
  return verifyPattern(spec, version).test(io.read(spec.path));
}

function bumpFile(spec, version, io) {
  const before = io.read(spec.path);
  const locate = new RegExp(spec.pattern, 'm');

  if (!locate.test(before)) {
    throw new Error(
      `${spec.path}: no match for /${spec.pattern}/ - cannot set the version. `
      + 'Either the file changed shape or .github/repo-infra.json is wrong.',
    );
  }

  const after = before.replace(locate, spec.replacement.replace(/\$VERSION/g, version));
  io.write(spec.path, after);

  // Read back rather than trust the write. Without this a failed or swallowed
  // write produces a release that is internally inconsistent and looks fine
  // until something downstream refuses it.
  if (!verifyFile(spec, version, io)) {
    throw new Error(
      `${spec.path}: version ${version} did not take - the file does not match `
      + `/${spec.verify}/ after writing`,
    );
  }

  return after;
}

function bumpAll(specs, version, io) {
  for (const spec of specs) {
    bumpFile(spec, version, io);
  }
}

module.exports = { bumpFile, bumpAll, verifyFile };
