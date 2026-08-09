# Publishing mdmost — design

*2026-08-09*

How `mdmost` reaches a user's machine: what CI checks, what a release builds, what
each platform installs, and the recording that shows why they would want it.

## 1. Scope

In scope: a `workflow_dispatch` release workflow modelled on `ansidrama`'s and
`edaptor`'s, binary artifacts for Linux (static musl), macOS and Windows, `.deb` and
`.rpm` packages, a Homebrew tap inside this repository, publication to crates.io, and an
`ansidrama` recording for the README.

Out of scope, decided rather than forgotten:

* **No apt/yum repository.** Packages are attached to the GitHub release and installed
  with `dpkg -i` / `rpm -i`. A repository hosted on GitHub Pages was considered and
  rejected: the published-site cap is 1 GB and the bandwidth allowance is a 100 GB/month
  *soft* limit, and package downloads are exactly the traffic that would test it. Moving
  the metadata to a redirect-capable host (Cloudflare, the `gh-release-apt` design)
  would work, but it puts a piece of load-bearing infrastructure outside this repository.
  The cost of the decision is real and should be stated in the README: there is no
  `apt upgrade` path, so users return to the releases page for a new version.
* **No GitHub Pages site.** GitHub renders `README.md` on the repository front page,
  demo included.
* **No notarised macOS build.** The tarball binaries are unsigned; Gatekeeper
  quarantines them. `brew install` is unaffected — Homebrew clears the quarantine
  attribute. Notarisation needs a paid Apple Developer account.
* **No container image.** `byonk` publishes a multi-arch image to GHCR, assembled from
  its already-built musl binaries. The mechanism is sound and would drop straight in,
  but `mdmost` is an interactive pager: running it in a container means
  `docker run --rm -it -v $PWD:/w -w /w …`, and mouse capture, OSC 52 clipboard writes
  and font detection all degrade across that boundary. The static musl binary already
  runs on any Linux, which is what a container would otherwise be solving.

## 2. Prerequisites in the tree

The release workflow cannot run against the repository as it stands.

| File | Why |
| --- | --- |
| `CHANGES.md` | The version job rewrites `## Unreleased` into `## <version> - <date>`; the release job extracts that section as the release notes. Seeded with an `Unreleased` block and a `0.1.0` entry. |
| `LICENSE-MIT` | `Cargo.toml` declares `license = "MIT"` and the README has a License section, but no licence file exists. Both packagers install it into `/usr/share/doc`. |
| `man/mdmost.1` | Written from the README's option and key tables. lintian and rpmlint want it, and it is what a user reaches for after installing a package. |
| `Cargo.toml` packaging metadata | `[package.metadata.deb]` and `[package.metadata.generate-rpm]`, after `ansidrama`'s: binary to `/usr/bin`, man page to `/usr/share/man/man1`, README and licence to `/usr/share/doc/mdmost/`. |
| `src/tui/term.rs` | One `#[cfg(unix)]` on the `SIGHUP` registration, so the Windows target compiles at all (§3). |

## 3. `ci.yml`

Push and pull request against `main`.

* **check** — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
* **test** — `cargo test`, with the cargo registry and `target` cached on `Cargo.lock`.
* **windows** — `cargo check` on `windows-latest`.

The Windows leg is not ceremony. `mdmost` had never been built for Windows, and when
first checked it did not compile:

```
error[E0425]: cannot find value `SIGHUP` in module `signal_hook::consts`
   --> src/tui/term.rs:188
```

That is the whole of it. `SIGHUP` has no Windows equivalent; `SIGTERM` and `SIGINT` do,
and `signal-hook` exports both there. With the one registration behind `#[cfg(unix)]`,
`cargo check --target x86_64-pc-windows-msvc --all-targets` passes clean — verified
locally against the real `x86_64-pc-windows-msvc` standard library, not assumed. The
rest of the portability was already deliberate: `/dev/tty` is behind
`cfg(target_os = "linux")` in `src/tui/term.rs`, the stderr capture behind `cfg(unix)`
in `src/tui/stderr.rs`, and `rustix` is a `cfg(unix)` dependency.

So implementation carries a one-line source change, and the CI leg exists to keep it
true. Note what the leg does *not* establish: `cargo check` is not a link, and neither
is a compile a run. Nobody has ever run `mdmost` on Windows — mouse capture, the
clipboard path and the alternate screen are unexercised there. The Windows artifact ships
as a build that compiles, and the README should not imply more.

## 4. `release.yml`

`workflow_dispatch` with `release_type: bugfix | feature | major`.

### version

Refuses to run off `main`. Derives the next version from the newest `v[0-9]*` tag,
rewrites `version` in `Cargo.toml`, rolls the `CHANGES.md` `Unreleased` block into a
dated section, commits, tags `v<version>`, pushes. Lifted from `ansidrama`'s workflow,
including its `perl -0777` block-boundary rewrite and the `sed '${/^## [0-9]/d}'` guard
that protects the last section in the file.

### build-binaries

| target | runner | build | artifacts |
| --- | --- | --- | --- |
| `x86_64-unknown-linux-musl` | ubuntu-latest | `cross` 0.2.5, `-C target-feature=+crt-static` | `tar.gz`, `.deb`, `.rpm` |
| `aarch64-unknown-linux-musl` | ubuntu-latest | `cross` 0.2.5, `-C target-feature=+crt-static` | `tar.gz`, `.deb`, `.rpm` |
| `x86_64-apple-darwin` | macos-latest | `cargo` | `tar.gz` |
| `aarch64-apple-darwin` | macos-14 | `cargo` | `tar.gz` |
| `x86_64-pc-windows-msvc` | windows-latest | `cargo` | `.zip` |

`cross` is pinned to the released 0.2.5 rather than tracking `main`, for the same reason
`ansidrama` pins it: the static-musl build cannot be verified locally before the first
release run, so it must not depend on a moving dependency. Packages are built with
`cargo deb --no-build --no-strip` and `cargo generate-rpm` against the already-built
target directory. Each archive carries the binary, the man page, `README.md` and
`LICENSE-MIT` under an `mdmost/` prefix.

Measured: the stripped x86_64 binary is 7.2 MB, so each package is roughly 3 MB
compressed.

### create-release

Downloads every artifact, extracts the release notes from `CHANGES.md`, and creates the
GitHub release with `fail_on_unmatched_files: true`.

### publish-crate

`cargo publish` gated on the `CRATES_IO_TOKEN` secret — the only secret this design
needs. It also claims the name, which matters here: this project is called `mdmost`
precisely because `mdless` was already taken.

### homebrew

Runs after `create-release`, computes the sha256 of the two macOS tarballs and the
`x86_64` Linux musl tarball, rewrites `Formula/mdmost.rb`, and commits it to `main`.
Ordering is load-bearing: the formula is written only once the artifacts it checksums
exist.

## 5. Homebrew tap, inside this repository

`Formula/mdmost.rb` — Homebrew searches a tap's `Formula/`, `HomebrewFormula/` and root,
so no separate repository is required. The formula branches on `on_macos` / `on_linux`
and `on_arm` / `on_intel` over the release tarball URLs.

Because the repository is not named `homebrew-mdmost`, the one-argument tap form does not
apply; users tap by explicit URL once:

```sh
brew tap oetiker/mdmost https://github.com/oetiker/mdmost
brew install mdmost
```

`brew upgrade mdmost` then behaves normally. The trade accepted here is two lines of
README instead of a `brew install oetiker/mdmost/mdmost` one-liner, in exchange for no
second repository and no cross-repository push token.

## 6. Install channels, as documented in the README

| Channel | Command |
| --- | --- |
| Debian/Ubuntu | download `mdmost_<ver>_<arch>.deb` from the release, `sudo dpkg -i` |
| Fedora/RHEL/openSUSE | download `mdmost-<ver>.<arch>.rpm`, `sudo rpm -i` |
| macOS, Linux | `brew tap oetiker/mdmost https://github.com/oetiker/mdmost && brew install mdmost` |
| Any Linux | untar the static musl tarball, `sudo install mdmost/mdmost /usr/local/bin/` |
| Windows | unzip, put `mdmost.exe` on `PATH` |
| Rust | `cargo install mdmost` |

The macOS tarball entry carries the Gatekeeper warning from §1 and points at `brew` as
the way around it.

## 7. The demo

`demo/tour.md` — a document chosen to be unkind to `less`: prose, a nested list, a task
list, a fenced Rust block, a table wider than the terminal, and a mermaid flowchart.

`demo/mdmost.toml` — an `ansidrama record` script; 100×30, `chrome.style = "macos"`,
output `docs/demo/mdmost.webp`. Two acts, 35-45 seconds:

1. Card — *"a Markdown file. and a pager."*
2. `less tour.md`, `space` twice: pipes, dashes, fence markers, and the mermaid block as
   unreadable source. `q`.
3. Card — *"the same file, in mdmost"*.
4. `mdmost --mouse tour.md` — the FIGlet banner and the styled body.
5. `space`, then `]` `]` — screen scroll, then heading to heading.
6. `tab` opens the contents pane; a click on an entry jumps to it.
7. Wheel scroll to the wide table, then `right` `right` `right` — the table scrolls
   sideways while the prose around it holds still.
8. A drag on the right-edge scrollbar; the document tracks the thumb.
9. A drag across a paragraph; on release the status bar reports the Markdown source was
   copied.
10. Card — *"mdmost"* with the install line.

`--mouse` is required for beats 6-9: mouse capture is opt-in.

The `.webp` is generated locally and committed, as `ansidrama` does with its own.
Rendering is deterministic, so regenerating produces identical bytes and does not churn
the diff. The regeneration command goes in `docs/maintainer-notes.md`.

## 8. Manual steps for the maintainer

1. Create the `oetiker/mdmost` repository and push; there is currently no git remote.
2. Add the `CRATES_IO_TOKEN` repository secret.
3. Allow GitHub Actions to write to the repository — the `version` and `homebrew` jobs
   push commits and tags.

## 9. Risks

* **Windows compiles but is unrun** (§3). The build error is known and one line to fix;
  the behaviour of the pager under `conhost`/Windows Terminal is untested by anyone.
  Mitigated by the CI leg and by not overclaiming in the README.
* **The first release run cannot be rehearsed locally.** `cross`, `cargo-deb` and
  `cargo-generate-rpm` are pinned, and the deb/rpm build can be exercised locally for
  `x86_64` before the first release to shake out metadata errors.
* **No upgrade path for deb/rpm users** (§1). Accepted; documented in the README rather
  than left for a user to discover.
