// repo-infra: workflow-lib v8
'use strict';

// publish-gitea-packages (D27): which release assets are packages, where
// Gitea wants them, and what a 409 on a re-run means.

function packageConfig(config) {
  const c = (config || {}).gitea_packages;
  if (!c || !c.url) {
    throw new Error('.github/repo-infra.json: "gitea_packages" needs a "url"');
  }
  if (!c.owner) {
    throw new Error('.github/repo-infra.json: "gitea_packages" needs an "owner"');
  }
  return {
    url: c.url.replace(/\/+$/, ''),
    owner: c.owner,
    distribution: (c.debian && c.debian.distribution) || 'stable',
    component: (c.debian && c.debian.component) || 'main',
    group: (c.rpm && c.rpm.group) || '',
  };
}

// name_version_arch.deb (cargo-deb) and name-version-release.arch.rpm
// (cargo-generate-rpm). Gitea's RPM version is the version-release pair.
const DEB = /^([a-z0-9][a-z0-9+.-]*)_([^_/]+)_([a-z0-9]+)\.deb$/;
const RPM = /^(.+)-([^-/]+)-([^-/]+)\.([A-Za-z0-9_]+)\.rpm$/;

function parsePackageFile(file) {
  let m = DEB.exec(file);
  if (m) return { kind: 'debian', name: m[1], version: m[2], arch: m[3], file };
  m = RPM.exec(file);
  if (m) return { kind: 'rpm', name: m[1], version: `${m[2]}-${m[3]}`, arch: m[4], file };
  return null;
}

const seg = encodeURIComponent;

function uploadUrl(cfg, pkg) {
  const base = `${cfg.url}/api/packages/${seg(cfg.owner)}`;
  if (pkg.kind === 'debian') {
    return `${base}/debian/pool/${seg(cfg.distribution)}/${seg(cfg.component)}/upload`;
  }
  // Gitea's generated .repo sets gpgcheck=1, so an unsigned rpm would not
  // install. Gitea signs with its per-owner key.
  return `${base}/rpm${cfg.group ? `/${seg(cfg.group)}` : ''}/upload?sign=true`;
}

function filesUrl(cfg, pkg) {
  // `~` is unreserved; encodeURIComponent leaves it alone.
  return `${cfg.url}/api/v1/packages/${seg(cfg.owner)}/${pkg.kind}/`
    + `${seg(pkg.name)}/${seg(pkg.version)}/files`;
}

function missingCredentials(env) {
  return ['GITEA_PACKAGE_TOKEN', 'GITEA_PACKAGE_USER'].filter((k) => !env[k]);
}

// Gitea answers 409 for a version it already has. On a re-run after a partial
// upload that is the file that went up the first time -- or a different file
// under the same version, which must fail.
function conflictVerdict(pkg, assetSha256, files) {
  const stored = (files || []).find((f) => f.name === pkg.file);
  if (!stored) {
    return { ok: false, message: `${pkg.file}: Gitea answered 409 but lists no such file` };
  }
  if (pkg.kind === 'debian') {
    return stored.sha256 === assetSha256
      ? { ok: true, message: `${pkg.file}: already uploaded, SHA-256 matches` }
      : { ok: false, message: `${pkg.file}: Gitea holds a different file under this version` };
  }
  // With ?sign=true Gitea stores the signed file, whose SHA-256 never equals
  // the unsigned asset's.
  return {
    ok: true,
    message: `${pkg.file}: already uploaded; name, version-release and architecture `
      + 'match; the content was not compared (Gitea stores the signed file)',
  };
}

module.exports = {
  packageConfig, parsePackageFile, uploadUrl, filesUrl, missingCredentials, conflictVerdict,
};
