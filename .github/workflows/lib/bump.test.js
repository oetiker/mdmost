// repo-infra: workflow-lib v5
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const bump = require('./bump.js');

const PLUGIN_SPEC = {
  path: '.claude-plugin/plugin.json',
  pattern: '"version"\\s*:\\s*"[^"]*"',
  replacement: '"version": "$VERSION"',
  verify: '"version"\\s*:\\s*"$VERSION"',
};

const CARGO_SPEC = {
  path: 'Cargo.toml',
  pattern: '^version = "[^"]*"',
  replacement: 'version = "$VERSION"',
  verify: '^version = "$VERSION"',
};

function memIO(files) {
  return {
    read: (p) => {
      if (!(p in files)) throw new Error(`ENOENT: ${p}`);
      return files[p];
    },
    write: (p, c) => { files[p] = c; },
  };
}

test('bumpFile rewrites the version', () => {
  const files = { '.claude-plugin/plugin.json': '{\n  "version": "0.0.0"\n}\n' };
  bump.bumpFile(PLUGIN_SPEC, '1.2.3', memIO(files));
  assert.match(files['.claude-plugin/plugin.json'], /"version": "1\.2\.3"/);
});

test('bumpFile leaves the rest of the file alone', () => {
  const files = {
    '.claude-plugin/plugin.json': '{\n  "name": "x",\n  "version": "0.0.0"\n}\n',
  };
  bump.bumpFile(PLUGIN_SPEC, '1.2.3', memIO(files));
  assert.match(files['.claude-plugin/plugin.json'], /"name": "x"/);
});

test('bumpFile anchors to the start of a line where the pattern says so', () => {
  // A Cargo.toml dependency also has a `version = "..."` line, but indented
  // under a table. The ^ anchor is what keeps the package version distinct.
  const files = {
    'Cargo.toml': '[package]\nversion = "0.1.0"\n\n[dependencies]\nanyhow = { version = "1.0" }\n',
  };
  bump.bumpFile(CARGO_SPEC, '0.2.0', memIO(files));
  assert.match(files['Cargo.toml'], /^version = "0\.2\.0"$/m);
  assert.match(files['Cargo.toml'], /anyhow = \{ version = "1\.0" \}/);
});

test('bumpFile fails loudly when the pattern does not match', () => {
  const files = { '.claude-plugin/plugin.json': '{\n  "name": "x"\n}\n' };
  assert.throws(
    () => bump.bumpFile(PLUGIN_SPEC, '1.2.3', memIO(files)),
    /no match for/,
  );
});

test('bumpFile fails when the write did not take', () => {
  // This is the whole reason the read-back exists. mdmost tagged v0.1.1 with
  // Cargo.toml at 0.1.1 and Cargo.lock still at 0.1.0 because a failed write
  // was swallowed; the publish died six minutes later.
  const files = { '.claude-plugin/plugin.json': '{\n  "version": "0.0.0"\n}\n' };
  const brokenIO = { read: (p) => files[p], write: () => { /* silently drops it */ } };
  assert.throws(
    () => bump.bumpFile(PLUGIN_SPEC, '1.2.3', brokenIO),
    /did not take/,
  );
});

test('bumpAll bumps every file', () => {
  const files = {
    '.claude-plugin/plugin.json': '{\n  "version": "0.0.0"\n}\n',
    'Cargo.toml': '[package]\nversion = "0.1.0"\n',
  };
  bump.bumpAll([PLUGIN_SPEC, CARGO_SPEC], '2.0.0', memIO(files));
  assert.match(files['.claude-plugin/plugin.json'], /"version": "2\.0\.0"/);
  assert.match(files['Cargo.toml'], /^version = "2\.0\.0"$/m);
});

test('verifyFile reports whether the file already carries the version', () => {
  const files = { '.claude-plugin/plugin.json': '{\n  "version": "1.2.3"\n}\n' };
  const io = memIO(files);
  assert.equal(bump.verifyFile(PLUGIN_SPEC, '1.2.3', io), true);
  assert.equal(bump.verifyFile(PLUGIN_SPEC, '1.2.4', io), false);
});

test('verifyFile is not fooled by a version that is a prefix of another', () => {
  const files = { '.claude-plugin/plugin.json': '{\n  "version": "1.2.30"\n}\n' };
  assert.equal(bump.verifyFile(PLUGIN_SPEC, '1.2.3', memIO(files)), false);
});
