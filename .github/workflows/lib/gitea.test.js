// repo-infra: workflow-lib v6
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const g = require('./gitea.js');

const CONFIG = { gitea_packages: { url: 'https://gitea.oetiker.ch/', owner: 'oposs' } };
const CFG = g.packageConfig(CONFIG);

test('the default layout is one channel', () => {
  assert.deepEqual(CFG, {
    url: 'https://gitea.oetiker.ch', owner: 'oposs',
    distribution: 'stable', component: 'main', group: '',
  });
});

test('a config without url or owner is refused', () => {
  assert.throws(() => g.packageConfig({}), /gitea_packages/);
  assert.throws(() => g.packageConfig({ gitea_packages: { url: 'https://x' } }), /owner/);
});

test('cargo-deb and cargo-generate-rpm names parse', () => {
  assert.deepEqual(g.parsePackageFile('mdmost_0.5.0-1_amd64.deb'), {
    kind: 'debian', name: 'mdmost', version: '0.5.0-1', arch: 'amd64',
    file: 'mdmost_0.5.0-1_amd64.deb',
  });
  assert.deepEqual(g.parsePackageFile('smtp-proxy-0.1.0-1.aarch64.rpm'), {
    kind: 'rpm', name: 'smtp-proxy', version: '0.1.0-1', arch: 'aarch64',
    file: 'smtp-proxy-0.1.0-1.aarch64.rpm',
  });
});

test('a pre-release deb version with ~ parses', () => {
  assert.equal(g.parsePackageFile('mdmost_1.0.0~rc.1-1_arm64.deb').version, '1.0.0~rc.1-1');
});

test('anything else is not a package', () => {
  for (const name of ['mdmost-0.5.0-x86_64-unknown-linux-musl.tar.gz', 'release-build.json',
    'mdmost.deb', 'x.rpm']) {
    assert.equal(g.parsePackageFile(name), null, name);
  }
});

test('upload urls follow the Gitea API, rpm is always signed', () => {
  assert.equal(g.uploadUrl(CFG, g.parsePackageFile('mdmost_0.5.0-1_amd64.deb')),
    'https://gitea.oetiker.ch/api/packages/oposs/debian/pool/stable/main/upload');
  assert.equal(g.uploadUrl(CFG, g.parsePackageFile('mdmost-0.5.0-1.x86_64.rpm')),
    'https://gitea.oetiker.ch/api/packages/oposs/rpm/upload?sign=true');
  assert.equal(g.uploadUrl({ ...CFG, group: 'el9' }, g.parsePackageFile('mdmost-0.5.0-1.x86_64.rpm')),
    'https://gitea.oetiker.ch/api/packages/oposs/rpm/el9/upload?sign=true');
});

test('the files url names type, package and version', () => {
  assert.equal(g.filesUrl(CFG, g.parsePackageFile('mdmost_1.0.0~rc.1-1_amd64.deb')),
    'https://gitea.oetiker.ch/api/v1/packages/oposs/debian/mdmost/1.0.0~rc.1-1/files');
});

test('missing credentials are named, both of them', () => {
  assert.deepEqual(g.missingCredentials({}), ['GITEA_PACKAGE_TOKEN', 'GITEA_PACKAGE_USER']);
  assert.deepEqual(g.missingCredentials({ GITEA_PACKAGE_TOKEN: 't', GITEA_PACKAGE_USER: '' }),
    ['GITEA_PACKAGE_USER']);
  assert.deepEqual(g.missingCredentials({ GITEA_PACKAGE_TOKEN: 't', GITEA_PACKAGE_USER: 'u' }), []);
});

const DEB = g.parsePackageFile('mdmost_0.5.0-1_amd64.deb');
const RPM = g.parsePackageFile('mdmost-0.5.0-1.x86_64.rpm');

test('a 409 for a deb with the same SHA-256 is success', () => {
  const v = g.conflictVerdict(DEB, 'abc', [{ name: DEB.file, sha256: 'abc' }]);
  assert.equal(v.ok, true);
  assert.match(v.message, /SHA-256/);
});

test('a 409 for a different deb under the same version fails', () => {
  const v = g.conflictVerdict(DEB, 'abc', [{ name: DEB.file, sha256: 'def' }]);
  assert.equal(v.ok, false);
  assert.match(v.message, /different/);
});

test('a 409 for an rpm matches by file name and says the content was not compared', () => {
  const v = g.conflictVerdict(RPM, 'abc', [{ name: RPM.file, sha256: 'signed' }]);
  assert.equal(v.ok, true);
  assert.match(v.message, /not compared/);
});

test('a 409 whose file Gitea does not list fails for either kind', () => {
  assert.equal(g.conflictVerdict(DEB, 'abc', []).ok, false);
  assert.equal(g.conflictVerdict(RPM, 'abc', [{ name: 'mdmost-0.5.0-1.aarch64.rpm' }]).ok, false);
});
