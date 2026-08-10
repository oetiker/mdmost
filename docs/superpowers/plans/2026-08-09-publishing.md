# mdmost Publishing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `mdmost` to users — CI on every push, a one-click release that builds
Linux/macOS/Windows binaries plus `.deb`/`.rpm`, a Homebrew tap in this repository,
a crates.io publish, and a recording in the README that shows why anyone would want it.

**Architecture:** Two GitHub Actions workflows. `ci.yml` gates every push and PR on the
same three commands the maintainer runs locally, plus a Windows compile check. `release.yml`
is `workflow_dispatch`-triggered and runs five jobs in dependency order: `version` bumps
and tags, `build-binaries` fans out over a five-target matrix, `create-release` collects
the artifacts, `publish-crate` pushes to crates.io, and `homebrew` rewrites the formula
with checksums of the artifacts that now exist. Packaging metadata lives in `Cargo.toml`
so `cargo deb` and `cargo generate-rpm` can be run locally and in CI identically.

**Tech Stack:** GitHub Actions, `cross` 0.2.5 (static musl), `cargo-deb` 3.x,
`cargo-generate-rpm` 0.21, `softprops/action-gh-release`, Homebrew formula DSL,
`ansidrama` (for the demo recording), roff (man page).

## Global Constraints

- The binary, crate, tap and packages are all named **`mdmost`**. Never `mdless`, never `mdmst`.
- Repository is **`https://github.com/oetiker/mdmost`**; no git remote is configured yet.
- Pinned tool versions, exactly: `cross` **0.2.5** (`--locked`), `cargo-deb` **`^3.7`**,
  `cargo-generate-rpm` **`^0.21`**.
- Action versions, matching `~/checkouts/ansidrama/.github/workflows/`:
  `actions/checkout@v7`, `actions/cache@v6`, `actions/upload-artifact@v7`,
  `actions/download-artifact@v8`, `dtolnay/rust-toolchain@stable`,
  `softprops/action-gh-release@v3`.
- Release targets, exactly five: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`
  (both `cross`, both `RUSTFLAGS="-C target-feature=+crt-static"`), `x86_64-apple-darwin`,
  `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`.
- `.deb` and `.rpm` are built for the two musl targets only.
- The only repository secret is **`CRATES_IO_TOKEN`**.
- No GitHub Pages, no apt/yum repository, no container image, no macOS notarisation.
  If a task seems to want one, it is out of scope — see the spec's §1.
- The local gate for every task is `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
  All three must pass before a commit. Never use more than 4 build jobs: `--jobs 4`.
- Commit messages: lowercase `type: subject`, body explaining *why*, wrapped at 76 columns.

---

### Task 1: Make Windows compile, and gate it in CI

**Files:**
- Modify: `src/tui/term.rs:188`
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: nothing.
- Produces: a repository that compiles for `x86_64-pc-windows-msvc`, which Task 4's
  release matrix depends on.

- [ ] **Step 1: Reproduce the failure**

The Windows standard library is already installed on this machine. Run:

```bash
cargo check --jobs 4 --target x86_64-pc-windows-msvc --all-targets 2>&1 | tail -20
```

Expected: FAIL with

```
error[E0425]: cannot find value `SIGHUP` in module `signal_hook::consts`
   --> src/tui/term.rs:188:62
```

If the target is missing, `rustup target add x86_64-pc-windows-msvc` first.

- [ ] **Step 2: Gate the one registration that has no Windows counterpart**

`SIGTERM` and `SIGINT` exist on Windows and `signal-hook` exports both; only `SIGHUP`
does not. In `src/tui/term.rs`, change:

```rust
    let _ = signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&terminate));
```

to:

```rust
    // `SIGHUP` is the terminal-went-away signal and has no Windows counterpart;
    // `SIGTERM` and `SIGINT` do, and `signal-hook` exports both there.
    #[cfg(unix)]
    let _ = signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&terminate));
```

Then update the module doc comment at `src/tui/term.rs:8`, which currently promises a
`SIGTERM`/`SIGHUP`/`SIGINT` flag unconditionally, to say `SIGHUP` is unix-only.

- [ ] **Step 3: Verify the Windows check now passes**

```bash
cargo check --jobs 4 --target x86_64-pc-windows-msvc --all-targets 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 4: Verify the unix build is untouched**

```bash
cargo fmt --check && cargo clippy --jobs 4 --all-targets -- -D warnings && cargo test --jobs 4 2>&1 | tail -3
```

Expected: no warnings, all tests pass.

- [ ] **Step 5: Write `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check & Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo registry
        uses: actions/cache@v6
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-

      - name: Check formatting
        run: cargo fmt --check

      - name: Run Clippy
        run: cargo clippy --all-targets -- -D warnings

  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo registry
        uses: actions/cache@v6
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-

      - name: Run tests
        run: cargo test

  windows:
    name: Windows compile
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v7

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      # A compile, not a run: nobody has exercised the pager on Windows. This leg
      # exists so the `cfg` gates in `tui::term` and `tui::stderr` stay honest.
      - name: Cargo check
        run: cargo check --all-targets
```

- [ ] **Step 6: Validate the YAML parses**

```bash
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('ok')"
```

Expected: `ok`

- [ ] **Step 7: Commit**

```bash
git add src/tui/term.rs .github/workflows/ci.yml
git commit -m "ci: gate every push, and make the Windows target compile

SIGHUP has no Windows counterpart, so registering it put the whole crate out
of reach of x86_64-pc-windows-msvc; SIGTERM and SIGINT do exist there and
signal-hook exports both. One cfg(unix) is the entire difference.

The CI workflow runs the three commands the README already documents, plus a
Windows cargo check so the cfg gates stay honest. A check is not a run: the
pager has never been launched on Windows, and nothing here claims otherwise."
```

---

### Task 2: Packaging prerequisites — licence, changelog, man page, metadata

**Files:**
- Create: `LICENSE-MIT`
- Create: `CHANGES.md`
- Create: `man/mdmost.1`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: nothing.
- Produces: `mdmost_<version>_<arch>.deb` and `mdmost-<version>-1.<arch>.rpm` build
  locally; `CHANGES.md` has the exact `## Unreleased` / `### New` / `### Changed` /
  `### Fixed` heading shape that Task 3's `version` job rewrites and whose section
  Task 3's `create-release` job extracts.

- [ ] **Step 1: Write `LICENSE-MIT`**

```
MIT License

Copyright (c) 2026 Tobias Oetiker

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 2: Write `CHANGES.md`**

The heading shape is load-bearing: the `version` job matches `^## Unreleased` and the
`create-release` job matches `^## <version>`. Do not reorder or rename the subsections.

```markdown
# Changes

## Unreleased

### New

### Changed

### Fixed

## 0.1.0 - 2026-08-09

### New

- A full-screen terminal pager for a single Markdown document: styled Unicode
  rendering, real table borders with negotiated column widths, syntax-highlighted
  code fences, and Mermaid diagrams laid out as box art.
- Rendering is a pure function of `(document, width, theme, options)`, so a resize
  reflows everything.
- Mouse support behind `--mouse`: the wheel scrolls, the scrollbar drags, contents
  entries jump, and a drag copies the Markdown *source* behind the selection.
- Section numbering, a FIGlet title banner, a contents pane, literal and regex
  search, themes, and a configuration file at `~/.config/mdmost/config.toml`.
```

- [ ] **Step 3: Write `man/mdmost.1`**

Create the directory and the page. Content is derived from the README's option table
(`README.md:85-95`) and key tables (`README.md:173-218`).

```roff
.TH MDMOST 1 "2026-08-09" "mdmost" "User Commands"
.SH NAME
mdmost \- full-screen terminal pager for a single Markdown document
.SH SYNOPSIS
.B mdmost
.RI [ OPTIONS ]
.RI [ FILE ]
.SH DESCRIPTION
.B mdmost
parses a Markdown document once and draws it as styled Unicode: tables get real
borders and negotiated column widths, fenced code is syntax-highlighted, and Mermaid
diagrams are laid out as box art rather than shown as source. Resizing the terminal
reflows everything, because rendering is a pure function of the document, the width,
the theme and the options.
.PP
With no
.IR FILE ,
the document is read from standard input; the keyboard is then read from
.I /dev/tty
so the pager stays interactive. When standard output is not a terminal,
.B \-\-render\-once
is implied, so
.B mdmost doc.md | cat
produces plain text rather than escape sequences.
.SH OPTIONS
.TP
.B \-\-render\-once
Render one frame to standard output and exit. Needs no terminal.
.TP
.BI \-\-width " N"
Render the whole document at this width instead of the terminal's.
.TP
.BI \-\-body\-width " N"
Cap the prose body at N columns and centre it;
.B 0
for no cap.
.TP
.B \-\-no\-body\-width
Let the body use the full terminal width.
.TP
.BI \-\-theme " NAME"
The theme to start in.
.TP
.B \-\-icons
Use Nerd Font glyphs even if none appears to be installed.
.TP
.B \-\-no\-icons
Use plain Unicode instead of Nerd Font glyphs, at the same display width.
.TP
.B \-\-mouse
Capture the mouse: the wheel scrolls, the scrollbar drags, clicks jump in the
contents pane, and dragging over the document copies the Markdown source behind it.
.TP
.B \-\-toc
Start with the table-of-contents pane open.
.TP
.BI \-\-config " PATH"
Read configuration from this file instead of the default.
.TP
.B \-\-licenses
Print the licences of the bundled syntax definitions and exit.
.TP
.B \-h ", " \-\-help
Print help and exit.
.TP
.B \-V ", " \-\-version
Print the version and exit.
.SH KEYS
.SS Moving
.TP
.BR j ", " Down
Scroll down one line.
.TP
.BR k ", " Up
Scroll up one line.
.TP
.BR d ", " Ctrl\-d
Scroll down half a screen.
.TP
.BR u ", " Ctrl\-u
Scroll up half a screen.
.TP
.BR space ", " Ctrl\-f ", " PgDn
Scroll down one screen.
.TP
.BR b ", " Ctrl\-b ", " PgUp
Scroll up one screen.
.TP
.BR g ", " Home
Go to the top, and back to the left edge.
.TP
.BR G ", " End
Go to the bottom of the document.
.TP
.B %
Jump N percent into the document, as in
.BR 50% .
.TP
.BR Left ", " Right
Scroll wide content sideways.
.SS Structure
.TP
.B [
Go to the previous heading.
.TP
.B ]
Go to the next heading.
.TP
.BR = ", " Ctrl\-g
Report where you are.
.TP
.B Tab
Show or hide the table of contents.
.TP
.B Enter
Jump to the selected heading.
.SS Searching
.TP
.B /
Search forward.
.TP
.B ?
Search backward.
.TP
.BR n ", " Ctrl\-Down
Go to the next match.
.TP
.BR N ", " Ctrl\-Up
Go to the previous match.
.TP
.B Ctrl\-r
Switch between literal and regex search.
.SS Other
.TP
.B t
Switch to the next theme.
.TP
.B \-
Show or hide code line numbers.
.TP
.BR h ", " F1
Show or hide the help overlay.
.TP
.B Esc
Clear the search, or close the overlay or pane.
.TP
.B q
Quit.
.SH ENVIRONMENT
.TP
.B MDMOST_ICONS
.B 1
or
.B 0
forces Nerd Font glyphs on or off. It outranks the configuration file and is
outranked by
.BR \-\-icons / \-\-no\-icons .
.TP
.B PAGER
.B mdmost
is usable as a pager;
.B export PAGER=mdmost
is the intended use.
.SH FILES
.TP
.I ~/.config/mdmost/config.toml
Configuration, in TOML. A broken file never stops the program from starting: the
problem is reported and the rest of the file still applies. The platform's own
configuration directory is used where it differs.
.PP
Most command-line options have a configuration-file counterpart, and a few settings
exist only there. In particular
.B title_banner
is
.B false
unless asked for: set
.B title_banner = true
to have a document whose first block is its one and only
.B #
heading drawn as a FIGlet banner. Section numbering,
.BR section_numbers ,
is on by default; the banner is not.
.SH EXIT STATUS
.TP
.B 0
Success, including a quit from the pager and a broken pipe.
.TP
.B 1
The document could not be read, or the terminal could not be set up.
.SH SEE ALSO
.BR less (1),
.BR bat (1)
.SH AUTHOR
Tobias Oetiker
```

- [ ] **Step 4: Add packaging metadata to `Cargo.toml`**

Insert after the `[package]` block's `repository` line, keeping the existing keys:

```toml
readme = "README.md"
keywords = ["markdown", "pager", "terminal", "tui", "mermaid"]
categories = ["command-line-utilities", "text-processing"]
# The demo recording and the design docs are for readers of the repository, not for
# anyone building the crate, and the `.webp` is the largest file in the tree.
exclude = ["docs/**", "demo/**", "tests/corpus/**", "tests/snapshots/**"]
```

Then append at the end of the file:

```toml
[package.metadata.deb]
maintainer = "Tobias Oetiker <tobi@oetiker.ch>"
copyright = "2026, Tobias Oetiker <tobi@oetiker.ch>"
license-file = ["LICENSE-MIT", "0"]
extended-description = """\
A full-screen terminal pager for a single Markdown document. Tables get real \
borders and negotiated column widths, fenced code is syntax-highlighted, and \
Mermaid diagrams are laid out as box art rather than shown as source. Rendering \
is a pure function of the document, width, theme and options, so a resize \
reflows everything. A single static binary."""
section = "utils"
priority = "optional"
assets = [
    ["target/release/mdmost", "usr/bin/", "755"],
    ["man/mdmost.1", "usr/share/man/man1/", "644"],
    ["README.md", "usr/share/doc/mdmost/README.md", "644"],
    ["LICENSE-MIT", "usr/share/doc/mdmost/LICENSE-MIT", "644"],
]

[package.metadata.generate-rpm]
summary = "Full-screen terminal pager for a single Markdown document"
license = "MIT"
# The musl builds are statically linked, so there is nothing to depend on — and the
# host's find-requires cannot inspect a cross-built aarch64 binary anyway.
auto-req = "no"
assets = [
    { source = "target/release/mdmost", dest = "/usr/bin/mdmost", mode = "755" },
    { source = "man/mdmost.1", dest = "/usr/share/man/man1/mdmost.1", mode = "644", doc = true },
    { source = "README.md", dest = "/usr/share/doc/mdmost/README.md", mode = "644", doc = true },
    { source = "LICENSE-MIT", dest = "/usr/share/doc/mdmost/LICENSE-MIT", mode = "644", doc = true },
]
```

`cargo-deb` and `cargo-generate-rpm` both rewrite a `target/release/` asset path to
`target/<triple>/release/` when `--target` is passed, which is why the paths above are
written without a triple.

- [ ] **Step 5: Prove the man page is valid roff**

```bash
man --warnings -E UTF-8 -l -Tutf8 man/mdmost.1 >/dev/null
```

Expected: no output. Any `mandoc`/`groff` warning printed here is a real defect — fix it.

Then eyeball it: `man -l man/mdmost.1`.

- [ ] **Step 6: Build a real .deb and .rpm locally**

Both tools are already installed on this machine. This is the rehearsal the CI run
cannot give us:

```bash
cargo build --release --jobs 4
cargo deb --no-build --no-strip
cargo generate-rpm
```

Expected: a `.deb` under `$CARGO_TARGET_DIR/debian/` and an `.rpm` under
`$CARGO_TARGET_DIR/generate-rpm/`. Resolve the target directory with
`cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])'`
— this repository uses a shared `CARGO_TARGET_DIR`, so `./target` does not exist.

- [ ] **Step 7: Inspect what the packages actually contain**

```bash
TD=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
dpkg-deb --contents "$TD"/debian/*.deb
rpm -qlp "$TD"/generate-rpm/*.rpm
```

Expected, in both: `/usr/bin/mdmost`, `/usr/share/man/man1/mdmost.1`,
`/usr/share/doc/mdmost/README.md`, `/usr/share/doc/mdmost/LICENSE-MIT`. If `rpm` is not
installed, skip the second command and note it — the CI job is the backstop.

- [ ] **Step 8: Run the local gate**

```bash
cargo fmt --check && cargo clippy --jobs 4 --all-targets -- -D warnings && cargo test --jobs 4 2>&1 | tail -3
```

Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add LICENSE-MIT CHANGES.md man/mdmost.1 Cargo.toml
git commit -m "packaging: a licence file, a changelog, a man page and metadata

Cargo.toml claimed MIT and the README had a License section, but no licence
file existed for a package to install. CHANGES.md is what the release workflow
reads: it rewrites the Unreleased block into a dated section and extracts that
section as the release notes, so the heading shape is load-bearing.

The man page is what someone reaches for after apt install, and both packagers
want one at /usr/share/man/man1. The deb and rpm were built and their contents
listed locally before any of this reached CI."
```

---

### Task 3: The release workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `CHANGES.md` heading shape and the `Cargo.toml` packaging metadata from Task 2.
- Produces: release assets named exactly
  `mdmost-<version>-<target>.tar.gz`, `mdmost-<version>-x86_64-pc-windows-msvc.zip`,
  `mdmost_<version>_<arch>.deb`, `mdmost-<version>-1.<arch>.rpm`. Task 4's formula
  hardcodes the three `tar.gz` names.

- [ ] **Step 1: Write the `version` job**

Create `.github/workflows/release.yml` starting with:

```yaml
name: Release

on:
  workflow_dispatch:
    inputs:
      release_type:
        description: 'Release type'
        required: true
        type: choice
        options:
          - bugfix
          - feature
          - major

env:
  CARGO_TERM_COLOR: always

jobs:
  version:
    name: Bump Version
    runs-on: ubuntu-latest
    permissions:
      contents: write
    outputs:
      version: ${{ steps.version.outputs.version }}
      tag: ${{ steps.version.outputs.tag }}
    steps:
      - uses: actions/checkout@v7
        with:
          fetch-depth: 0

      - name: Verify main branch
        run: |
          if [ "${{ github.ref }}" != "refs/heads/main" ]; then
            echo "::error::Releases must be created from the main branch"
            exit 1
          fi

      - name: Calculate new version
        id: version
        run: |
          LATEST=$(git tag -l 'v[0-9]*.[0-9]*.[0-9]*' | sort -V | tail -1 || echo "v0.0.0")
          if [ -z "$LATEST" ]; then
            LATEST="v0.0.0"
          fi
          MAJOR=$(echo "$LATEST" | sed 's/v\([0-9]*\)\.\([0-9]*\)\.\([0-9]*\)/\1/')
          MINOR=$(echo "$LATEST" | sed 's/v\([0-9]*\)\.\([0-9]*\)\.\([0-9]*\)/\2/')
          PATCH=$(echo "$LATEST" | sed 's/v\([0-9]*\)\.\([0-9]*\)\.\([0-9]*\)/\3/')
          case "${{ inputs.release_type }}" in
            major)   NEW_VERSION="$((MAJOR+1)).0.0" ;;
            feature) NEW_VERSION="${MAJOR}.$((MINOR+1)).0" ;;
            bugfix)  NEW_VERSION="${MAJOR}.${MINOR}.$((PATCH+1))" ;;
          esac
          echo "version=${NEW_VERSION}" >> $GITHUB_OUTPUT
          echo "tag=v${NEW_VERSION}" >> $GITHUB_OUTPUT
          echo "New version: ${NEW_VERSION}"

      - name: Update Cargo.toml version
        run: |
          sed -i 's/^version = ".*"/version = "${{ steps.version.outputs.version }}"/' Cargo.toml
          # Keep Cargo.lock's own record of the version in step, so the release commit
          # is not immediately dirty for anyone who builds it.
          cargo update --workspace --offline || cargo update -p mdmost --offline || true

      - name: Update CHANGES.md
        run: |
          DATE=$(date +%Y-%m-%d)
          VERSION="${{ steps.version.outputs.version }}"
          # Move the `## Unreleased` block (everything up to the next `## <version>`
          # heading, or EOF) into a new `## <version> - <date>` section and reset
          # Unreleased to empty subsections. Block-boundary match, so blank lines
          # before ### Changed/### Fixed and a missing trailing version section are
          # both handled.
          perl -i -0777 -pe '
            s{^\#\# Unreleased\n(.*?)(?=\n\#\# |\z)}{
              "## Unreleased\n\n### New\n\n### Changed\n\n### Fixed\n\n" .
              "## '"$VERSION"' - '"$DATE"'\n$1"
            }gmse' CHANGES.md

      - name: Commit and tag
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Cargo.toml Cargo.lock CHANGES.md
          git commit -m "Release ${{ steps.version.outputs.tag }}"
          git tag -a "${{ steps.version.outputs.tag }}" -m "Release ${{ steps.version.outputs.tag }}"
          git push origin main --tags
```

- [ ] **Step 2: Append the `build-binaries` job**

```yaml
  build-binaries:
    name: Build ${{ matrix.target }}
    needs: version
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
            cross: true
            package: true
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
            cross: true
            package: true
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-14
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            windows: true
    steps:
      - uses: actions/checkout@v7
        with:
          ref: ${{ needs.version.outputs.tag }}

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install cross
        if: matrix.cross
        # Pinned to the released version, not `main`: the static-musl build cannot be
        # rehearsed locally, so it must not rest on a moving dependency.
        run: cargo install cross --version 0.2.5 --locked

      - name: Build binary
        run: |
          if [ "${{ matrix.cross }}" = "true" ]; then
            RUSTFLAGS="-C target-feature=+crt-static" cross build --release --target ${{ matrix.target }}
          else
            cargo build --release --target ${{ matrix.target }}
          fi
        shell: bash

      - name: Create tarball
        if: '!matrix.windows'
        run: |
          mkdir -p dist staging/mdmost/man
          BINARY="target/${{ matrix.target }}/release/mdmost"
          ARCHIVE="mdmost-${{ needs.version.outputs.version }}-${{ matrix.target }}.tar.gz"
          cp "$BINARY" staging/mdmost/
          cp man/mdmost.1 staging/mdmost/man/
          cp README.md LICENSE-MIT staging/mdmost/
          tar -czvf "dist/${ARCHIVE}" -C staging mdmost
        shell: bash

      - name: Create zip (Windows)
        if: matrix.windows
        run: |
          mkdir dist
          mkdir staging/mdmost
          $ARCHIVE = "mdmost-${{ needs.version.outputs.version }}-${{ matrix.target }}.zip"
          Copy-Item "target/${{ matrix.target }}/release/mdmost.exe" staging/mdmost/
          Copy-Item README.md staging/mdmost/
          Copy-Item LICENSE-MIT staging/mdmost/
          Compress-Archive -Path staging/mdmost -DestinationPath "dist/$ARCHIVE"
        shell: pwsh

      - name: Build deb and rpm
        if: matrix.package
        run: |
          cargo install cargo-deb --version '^3.7'
          cargo install cargo-generate-rpm --version '^0.21'
          cargo deb --no-build --no-strip --target ${{ matrix.target }}
          cargo generate-rpm --target ${{ matrix.target }}
          cp target/${{ matrix.target }}/debian/*.deb dist/
          cp target/${{ matrix.target }}/generate-rpm/*.rpm dist/
        shell: bash

      - name: Upload artifact
        uses: actions/upload-artifact@v7
        with:
          name: mdmost-${{ matrix.target }}
          path: dist/*
```

- [ ] **Step 3: Append `create-release`, `publish-crate` and `homebrew`**

```yaml
  create-release:
    name: Create GitHub Release
    needs: [version, build-binaries]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v7
        with:
          ref: ${{ needs.version.outputs.tag }}

      - name: Download all artifacts
        uses: actions/download-artifact@v8
        with:
          path: artifacts
          pattern: mdmost-*
          merge-multiple: true

      - name: Extract release notes
        run: |
          VERSION="${{ needs.version.outputs.version }}"
          # Print this version's section up to the next `## <version>` heading. Drop
          # that trailing heading, but only if the last printed line IS one: for the
          # final section in the file the range runs to EOF, where a blind `$d` would
          # delete real content.
          sed -n "/^## ${VERSION}/,/^## [0-9]/p" CHANGES.md | sed '${/^## [0-9]/d}' > release-notes.md
          echo "Release notes:"; cat release-notes.md

      - name: List artifacts
        run: ls -la artifacts/

      - name: Create Release
        uses: softprops/action-gh-release@v3
        with:
          tag_name: ${{ needs.version.outputs.tag }}
          name: mdmost ${{ needs.version.outputs.tag }}
          body_path: release-notes.md
          files: artifacts/*
          fail_on_unmatched_files: true
          draft: false
          prerelease: false

  publish-crate:
    name: Publish to crates.io
    needs: [version, create-release]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
        with:
          ref: ${{ needs.version.outputs.tag }}

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Publish
        run: cargo publish --token "${{ secrets.CRATES_IO_TOKEN }}"

  homebrew:
    name: Update Homebrew formula
    needs: [version, create-release]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      # The formula is committed to `main`, not to the tag: a tap is read from the
      # default branch. It runs after create-release because it checksums artifacts
      # that must already exist.
      - uses: actions/checkout@v7
        with:
          ref: main

      - name: Download tarballs
        uses: actions/download-artifact@v8
        with:
          path: artifacts
          pattern: mdmost-*
          merge-multiple: true

      - name: Rewrite the formula
        run: |
          VERSION="${{ needs.version.outputs.version }}"
          sha() { sha256sum "artifacts/mdmost-${VERSION}-$1.tar.gz" | cut -d' ' -f1; }
          MAC_ARM=$(sha aarch64-apple-darwin)
          MAC_X86=$(sha x86_64-apple-darwin)
          LINUX_X86=$(sha x86_64-unknown-linux-musl)
          LINUX_ARM=$(sha aarch64-unknown-linux-musl)
          sed -i \
            -e "s/^  version \".*\"/  version \"${VERSION}\"/" \
            -e "s/MAC_ARM_SHA_[0-9a-f]*\|sha256 \"[0-9a-f]*\" # mac-arm/sha256 \"${MAC_ARM}\" # mac-arm/" \
            Formula/mdmost.rb
          # Each sha256 line carries a trailing marker comment naming the artifact it
          # belongs to, which is what makes this rewrite unambiguous.
          sed -i \
            -e "s|sha256 \"[0-9a-f]*\" # mac-arm|sha256 \"${MAC_ARM}\" # mac-arm|" \
            -e "s|sha256 \"[0-9a-f]*\" # mac-x86|sha256 \"${MAC_X86}\" # mac-x86|" \
            -e "s|sha256 \"[0-9a-f]*\" # linux-x86|sha256 \"${LINUX_X86}\" # linux-x86|" \
            -e "s|sha256 \"[0-9a-f]*\" # linux-arm|sha256 \"${LINUX_ARM}\" # linux-arm|" \
            Formula/mdmost.rb
          cat Formula/mdmost.rb

      - name: Commit
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Formula/mdmost.rb
          git commit -m "Homebrew formula for ${{ needs.version.outputs.tag }}"
          git push origin main
```

Note: the first `sed -i` in "Rewrite the formula" above is redundant with the second —
delete it and keep only the four marker-based substitutions.

- [ ] **Step 4: Validate the YAML parses and the job graph is what you meant**

```bash
python3 - <<'EOF'
import yaml
wf = yaml.safe_load(open('.github/workflows/release.yml'))
jobs = wf['jobs']
print(sorted(jobs))
for name, job in jobs.items():
    print(name, '<-', job.get('needs'))
targets = [m['target'] for m in jobs['build-binaries']['strategy']['matrix']['include']]
print(targets)
assert len(targets) == 5, targets
assert sorted(jobs) == ['build-binaries', 'create-release', 'homebrew', 'publish-crate', 'version']
print('ok')
EOF
```

Expected: `ok`, five targets, and `build-binaries <- version`,
`create-release <- ['version', 'build-binaries']`, `publish-crate` and `homebrew` both
`<- ['version', 'create-release']`.

- [ ] **Step 5: Dry-run the CHANGES.md rewrite against the real file**

The `perl` rewrite is the step most likely to silently mangle the changelog, and it is
easier to test now than after a release:

```bash
cp CHANGES.md /tmp/CHANGES.test.md
VERSION=0.2.0 DATE=2026-08-09 perl -i -0777 -pe '
  s{^\#\# Unreleased\n(.*?)(?=\n\#\# |\z)}{
    "## Unreleased\n\n### New\n\n### Changed\n\n### Fixed\n\n" .
    "## $ENV{VERSION} - $ENV{DATE}\n$1"
  }gmse' /tmp/CHANGES.test.md
cat /tmp/CHANGES.test.md
sed -n "/^## 0.2.0/,/^## [0-9]/p" /tmp/CHANGES.test.md | sed '${/^## [0-9]/d}'
```

Expected: a fresh empty `## Unreleased`, a `## 0.2.0 - 2026-08-09` section holding what
Unreleased held, the `## 0.1.0` section intact below it, and the final `sed` printing the
0.2.0 section only. Delete `/tmp/CHANGES.test.md` afterwards.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: one-click release for five targets

Bumps the version, rolls CHANGES.md, tags, builds static musl binaries for
x86_64 and aarch64 plus macOS and Windows, packages deb and rpm for the two
musl targets, publishes the crate and rewrites the Homebrew formula with the
checksums of artifacts that by then exist.

cross is pinned to 0.2.5 rather than tracking main. The static-musl build
cannot be rehearsed on a developer machine, so the one thing it must not do is
change underneath a release. The changelog rewrite was dry-run against the
real CHANGES.md before this landed."
```

---

### Task 4: Homebrew formula and the README install section

**Files:**
- Create: `Formula/mdmost.rb`
- Modify: `README.md:50-59` (the `## Install` section)

**Interfaces:**
- Consumes: the artifact names Task 3 produces, and the marker comments
  `# mac-arm`, `# mac-x86`, `# linux-x86`, `# linux-arm` that Task 3's `homebrew` job
  substitutes on.
- Produces: a tap installable with
  `brew tap oetiker/mdmost https://github.com/oetiker/mdmost && brew install mdmost`.

- [ ] **Step 1: Write `Formula/mdmost.rb`**

The four `sha256` lines each carry a marker comment; the release workflow rewrites them
by marker. The placeholder digests below are the sha256 of the empty string, so a stale
formula fails loudly at download rather than installing something unexpected.

```ruby
# Homebrew formula for mdmost. This repository is its own tap:
#
#   brew tap oetiker/mdmost https://github.com/oetiker/mdmost
#   brew install mdmost
#
# The version and the four sha256 lines are rewritten by .github/workflows/release.yml
# after the release artifacts exist. The trailing marker comments are what that
# rewrite matches on — do not remove them.
class Mdmost < Formula
  desc "Full-screen terminal pager for a single Markdown document"
  homepage "https://github.com/oetiker/mdmost"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" # mac-arm
    end
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" # mac-x86
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" # linux-x86
    end
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" # linux-arm
    end
  end

  def install
    bin.install "mdmost"
    man1.install "man/mdmost.1"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mdmost --version")
    (testpath/"doc.md").write("# Title\n\nBody text.\n")
    assert_match "Body text", shell_output("#{bin}/mdmost --render-once --width 40 #{testpath}/doc.md")
  end
end
```

- [ ] **Step 2: Check the formula is valid Ruby**

```bash
ruby -c Formula/mdmost.rb
```

Expected: `Syntax OK`. If `ruby` is not installed, note it and rely on the `brew` check
below; do not skip both.

- [ ] **Step 3: Verify the marker rewrite actually matches**

Prove the workflow's `sed` hits all four lines before trusting it in a release:

```bash
cp Formula/mdmost.rb /tmp/f.rb
for m in mac-arm mac-x86 linux-x86 linux-arm; do
  sed -i "s|sha256 \"[0-9a-f]*\" # $m|sha256 \"deadbeef\" # $m|" /tmp/f.rb
done
grep -c 'deadbeef' /tmp/f.rb
```

Expected: `4`. Then `rm /tmp/f.rb`.

- [ ] **Step 4: Rewrite the README `## Install` section**

Replace the current section (`README.md:50-59`, the `cargo build --release` snippet and
the paragraph under it) with:

````markdown
## Install

**Homebrew** (macOS and Linux) — this repository is its own tap:

```sh
brew tap oetiker/mdmost https://github.com/oetiker/mdmost
brew install mdmost
```

**Debian, Ubuntu** — download `mdmost_<version>_amd64.deb` (or `_arm64.deb`) from the
[releases page](https://github.com/oetiker/mdmost/releases):

```sh
sudo dpkg -i mdmost_*_amd64.deb
man mdmost
```

**Fedora, RHEL, openSUSE** — download the matching `.rpm`:

```sh
sudo rpm -i mdmost-*.x86_64.rpm
```

There is no apt or yum repository, so `apt upgrade` will not find new versions: come
back to the releases page for those.

**Any Linux** — the tarballs are static musl builds and need nothing installed:

```sh
tar xzf mdmost-*-x86_64-unknown-linux-musl.tar.gz
sudo install mdmost/mdmost /usr/local/bin/
```

**macOS without Homebrew** — the tarball binaries are neither signed nor notarised, so
Gatekeeper will quarantine them; `brew install` above is the path of least resistance.

**Windows** — unzip `mdmost-<version>-x86_64-pc-windows-msvc.zip` and put `mdmost.exe`
on your `PATH`. The Windows build compiles and is checked on every push, but it has not
been exercised in anger: expect the mouse, the clipboard and font detection to be less
well behaved there than on Unix.

**Rust** —

```sh
cargo install mdmost
```

**From source** —

```sh
cargo build --release
install -m755 target/release/mdmost ~/.local/bin/
```

Rust 2024 edition; no system dependencies beyond a terminal that speaks ANSI truecolour.
Pure Rust all the way down — the build needs no C compiler, which is why the regex engine
behind the highlighter is `fancy-regex` rather than oniguruma.
````

- [ ] **Step 5: Confirm the README still renders in the pager it documents**

```bash
cargo run --release --jobs 4 -- --render-once --width 100 README.md | head -60
```

Expected: the Install section renders with its fenced blocks intact.

- [ ] **Step 6: Run the local gate and commit**

```bash
cargo fmt --check && cargo clippy --jobs 4 --all-targets -- -D warnings && cargo test --jobs 4 2>&1 | tail -3
git add Formula/mdmost.rb README.md
git commit -m "packaging: a Homebrew tap in this repository, and install docs

The repository is not named homebrew-mdmost, so the one-argument tap form does
not reach it and users tap by explicit URL once; after that brew upgrade works
normally. That trade buys us no second repository and no cross-repository push
token.

The four sha256 lines carry marker comments because that is what the release
workflow rewrites them by, and the placeholders are the digest of the empty
string so a formula that never got rewritten fails at download instead of
installing a surprise. The README now says plainly that there is no apt
upgrade path and that the Windows build has never been run."
```

---

### Task 5: The demo recording

**Files:**
- Create: `demo/tour.md`
- Create: `demo/config.toml` (mdmost's own config for the recording)
- Create: `demo/mdmost.toml` (the ansidrama script)
- Create: `docs/demo/mdmost.webp` (generated)
- Modify: `README.md` (insert the demo after the opening paragraphs)
- Modify: `docs/maintainer-notes.md` (how to regenerate)

**Interfaces:**
- Consumes: a release `mdmost` binary, and `less` from the host.
- Produces: `docs/demo/mdmost.webp`, referenced from the README.

- [ ] **Step 1: Build ansidrama**

```bash
cargo build --release --jobs 4 --manifest-path ~/checkouts/ansidrama/Cargo.toml
~/scratch/cargo-target/release/ansidrama --help | head -20
```

If `ansidrama`'s target directory differs, find it with `cargo metadata` as in Task 2.
Read `~/checkouts/ansidrama/README.md` §"Scenes & frames" before writing the script —
the field names and the one-action-per-scene rule are exact.

- [ ] **Step 2: Write `demo/tour.md`**

The document must be one `less` visibly fails at: a wide table, a fenced block, a
Mermaid diagram, and enough headings for the contents pane to be worth opening.

```markdown
# Field Notes

A pager that knows what Markdown means. This paragraph exists so there is prose to
select, and so the body-width cap has something to centre.

## Why box art beats source

- Tables get **real borders** and negotiated column widths.
- Fenced code is syntax-highlighted, in the theme's own palette.
- Mermaid diagrams are laid out, not printed.

1. Parse once.
2. Draw at the current width.
3. Resize, and it reflows.

- [x] read the document
- [ ] believe the screenshots

## A table wider than the terminal

| Component | Responsibility | Input | Output | Notes |
| --- | --- | --- | --- | --- |
| `doc` | parse Markdown into a tree | source text | `Doc` | comrak, once per document |
| `render` | tree to canvas at a width | `Doc`, width, theme | `Canvas` | pure function, no I/O |
| `mermaid` | lay diagrams out as box art | fence body | `Canvas` | flowchart, sequence, class, ER, state, pie, gantt |
| `tui` | event loop, panes, search | `Canvas` | frames | ratatui and crossterm |
| `theme` | colours and glyph choices | config | `Theme` | truecolour, contrast-checked |

## Some code

```rust
pub fn render(doc: &Doc, width: u16, theme: &Theme) -> Canvas {
    let mut canvas = Canvas::new(width);
    for node in doc.root().children.iter() {
        canvas.push(block(node, width, theme));
    }
    canvas
}
```

## And a diagram

```mermaid
flowchart LR
    A["Markdown source"] --> B["parse"]
    B --> C["render at width"]
    C --> D["canvas"]
    D --> E["frame"]
```

## The end

Scroll back up, or press `q`.
```

- [ ] **Step 3a: Write `demo/config.toml`**

`title_banner` is opt-in and defaults to `false`, so without this the recording opens
on a plain `# Field Notes` heading and design §7 beat 4 — "the FIGlet banner and the
styled body" — never happens. The recording must not depend on whatever is in the
maintainer's `~/.config/mdmost/config.toml` either, so mdmost is launched with
`--config config.toml` and this file pins everything the demo relies on:

```toml
# mdmost's own configuration for the recording. Passed with `--config config.toml`
# so the demo does not inherit the maintainer's ~/.config/mdmost/config.toml.
title_banner = true
```

- [ ] **Step 3b: Write `demo/mdmost.toml`**

Paths are relative to the config file's directory. `PATH` is prefixed so the recorded
shell finds the release binary being demonstrated; replace `/abs/path/to/target/release`
with the real target directory before recording.

```toml
# The README trailer: `less` and `mdmost` on the same document, back to back.
# Regenerate with the command in docs/maintainer-notes.md.
launch  = "PS1='$ ' PATH=/abs/path/to/target/release:$PATH bash --norc --noprofile -i"
cols    = 100
rows    = 30
font_px = 18
card_font_px = 40
out     = "../docs/demo/mdmost.webp"
env     = { COLORTERM = "truecolor", TERM = "xterm-256color" }
settle_ms = 900
quit_keys = ["C-c"]

[chrome]
style   = "macos"
title   = "mdmost"
padding = 12

[[scene]]
card    = { lines = ["a Markdown file.", "and a pager."], fg = "#fef9c3" }
hold_cs = 260

# Act one: the file as `less` shows it.
[[scene]]
text    = "less tour.md"
hold_cs = 40
[[scene]]
keys    = ["Enter"]
hold_cs = 200
[[scene]]
keys    = ["space"]
hold_cs = 200
[[scene]]
keys    = ["space"]
hold_cs = 240
[[scene]]
keys    = ["q"]
hold_cs = 80

[[scene]]
card    = { text = "the same file, in mdmost", fg = "#fef9c3" }
hold_cs = 260

# Act two.
[[scene]]
text    = "mdmost --mouse --config config.toml tour.md"
hold_cs = 40
[[scene]]
keys    = ["Enter"]
hold_cs = 320

[[scene]]
keys    = ["space"]
hold_cs = 180
[[scene]]
keys    = ["]", "]"]
hold_cs = 200

# The contents pane, opened and then clicked.
[[scene]]
keys    = ["Tab"]
hold_cs = 200
[[scene]]
click   = { x = 12, y = 6 }
hold_cs = 220
[[scene]]
keys    = ["Tab"]
hold_cs = 150

# The wide table, scrolled sideways while the prose holds still.
[[scene]]
scroll  = { x = 50, y = 15, dir = "down", n = 4 }
hold_cs = 200
[[scene]]
keys    = ["Right", "Right", "Right"]
hold_cs = 260

# The scrollbar, dragged.
[[scene]]
drag    = { from = [99, 6], to = [99, 22] }
hold_cs = 240

# A drag across prose copies the Markdown source behind it.
[[scene]]
drag    = { from = [8, 8], to = [60, 10] }
hold_cs = 320

[[scene]]
card    = { lines = ["mdmost", "brew install mdmost"], fg = "#fef9c3" }
hold_cs = 320
```

- [ ] **Step 4: Record it**

```bash
mkdir -p docs/demo
cd demo && ~/scratch/cargo-target/release/ansidrama record mdmost.toml; cd ..
ls -lh docs/demo/mdmost.webp
```

Expected: a `.webp` is written. If a scene errors, ansidrama says which one.

- [ ] **Step 5: Look at it — do not skip this**

Coordinates in the script (the TOC click at 12,6; the scrollbar drag at column 99; the
prose drag) are guesses about where things land at 100×30. They are almost certainly
wrong on the first pass. Dump frames and look:

```bash
cd demo && ~/scratch/cargo-target/release/ansidrama record mdmost.toml --dump-png /tmp/frames; cd ..
ls /tmp/frames | head
```

Then use the Read tool on individual PNGs — at minimum one frame from each of: the
`less` act, the mdmost banner, the open contents pane, the sideways-scrolled table, the
scrollbar drag, and the copy. Check that the click actually hit a contents entry, that
the table really moved sideways, and that the status bar says `copied` or
`sent … (unconfirmed)` after the drag. Adjust the coordinates and re-record until each
beat lands. If `--dump-png` is not supported by `record`, use `--dump-png` on an
`encode` config, or fall back to checking the final `.webp` in an image viewer and say
so.

- [ ] **Step 6: Make the config reproducible**

The absolute `PATH` in `launch` must not be committed. Replace it with a relative one
that works when recording from `demo/`:

```toml
launch  = "PS1='$ ' PATH=../target/release:$PATH bash --norc --noprofile -i"
```

This repository uses a shared `CARGO_TARGET_DIR`, so `../target/release` will not exist.
Record with a symlink or an explicit `PATH` export in the shell instead, and document
whichever you used in the next step rather than leaving a path that only worked once.

- [ ] **Step 7: Put the demo in the README**

After the opening two paragraphs of `README.md` (immediately before the paragraph
beginning "It parses the document once"), insert:

```markdown
![less, then mdmost, on the same document](docs/demo/mdmost.webp)
```

- [ ] **Step 8: Document the regeneration in `docs/maintainer-notes.md`**

Append a section:

```markdown
## Regenerating the demo

`docs/demo/mdmost.webp` is recorded with [ansidrama](https://github.com/oetiker/ansidrama)
from `demo/mdmost.toml`, against `demo/tour.md`. The pager is launched with
`--config demo/config.toml` so the recording shows the opt-in title banner and does not
inherit the maintainer's own configuration. Rendering is deterministic, so
re-recording an unchanged script produces identical bytes.

```sh
cargo build --release
PATH="$(cargo metadata --format-version 1 --no-deps \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/release:$PATH"
cd demo && ansidrama record mdmost.toml
```

The scene coordinates — the contents-pane click, the scrollbar drag, the selection drag
— are tied to the 100×30 grid and to where the document's headings fall. Change
`tour.md` and they need checking again; `ansidrama record --dump-png <dir>` writes every
frame out for that.
```

- [ ] **Step 9: Commit**

```bash
git add demo/tour.md demo/config.toml demo/mdmost.toml docs/demo/mdmost.webp README.md docs/maintainer-notes.md
git commit -m "docs: a recording of less and mdmost on the same document

Two acts on one file. First `less`, where a wide table is pipes and dashes and
a Mermaid block is unreadable source; then mdmost on the same document, with
the banner, the contents pane, a table scrolled sideways while the prose holds
still, the scrollbar dragged, and a selection that copies Markdown source
rather than glyphs.

tour.md is built to be unkind to a plain pager, which is the point. The scene
coordinates are tied to the 100x30 grid, so maintainer-notes says how to dump
frames and check them after any change."
```

---

### Task 6: Final verification and the maintainer's checklist

**Files:**
- Modify: `docs/maintainer-notes.md`
- Modify: `CHANGES.md`

**Interfaces:**
- Consumes: everything from Tasks 1-5.
- Produces: nothing further depends on this.

- [ ] **Step 1: Re-run the whole local gate from a clean slate**

```bash
cargo fmt --check
cargo clippy --jobs 4 --all-targets -- -D warnings
cargo test --jobs 4 2>&1 | grep -E "^test result" | sort | uniq -c
cargo check --jobs 4 --target x86_64-pc-windows-msvc --all-targets 2>&1 | tail -2
```

Expected: no formatting diff, no clippy warnings, every `test result` line `ok`, and the
Windows check `Finished`.

- [ ] **Step 2: Confirm no stale names survive**

```bash
grep -rin "mdless\|mdmst" --exclude-dir=.git --exclude-dir=target . | wc -l
```

Expected: `0`.

- [ ] **Step 3: Confirm the packages still build after every change**

```bash
TD=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cargo build --release --jobs 4
cargo deb --no-build --no-strip && dpkg-deb --contents "$TD"/debian/*.deb | awk '{print $NF}'
cargo generate-rpm && rpm -qlp "$TD"/generate-rpm/*.rpm
```

Expected: `/usr/bin/mdmost`, the man page, the README and the licence in both.

- [ ] **Step 4: Write the release checklist into `docs/maintainer-notes.md`**

Append:

```markdown
## Releasing

Releases are cut by the `Release` workflow, run from the Actions tab with a release type
of `bugfix`, `feature` or `major`. It must be run from `main`, and it refuses otherwise.

Before the first release:

1. Create `https://github.com/oetiker/mdmost` and push `main`.
2. Add the `CRATES_IO_TOKEN` repository secret (Settings → Secrets and variables →
   Actions). It is the only secret this project uses.
3. Settings → Actions → General → Workflow permissions: allow read and write. The
   `version` job pushes a commit and a tag, and the `homebrew` job pushes the rewritten
   formula.

Each release:

1. Put what changed under `## Unreleased` in `CHANGES.md`. The workflow moves that block
   into a dated section and uses it verbatim as the release notes — nothing else writes
   them.
2. Run the workflow. It bumps `Cargo.toml`, tags, builds five targets, packages `.deb`
   and `.rpm` for the two musl targets, publishes the crate, and rewrites
   `Formula/mdmost.rb` with the new checksums.
3. `git pull` afterwards: the workflow has pushed two commits and a tag to `main`.

What is deliberately not automated, and why, is in
`docs/superpowers/specs/2026-08-09-publishing-design.md` §1 — there is no apt/yum
repository, no Pages site, no container image and no macOS notarisation.
```

- [ ] **Step 5: Record the work in `CHANGES.md`**

Under `## Unreleased` → `### New`, add:

```markdown
- Prebuilt binaries for Linux (static musl, x86_64 and aarch64), macOS (Intel and
  Apple silicon) and Windows, with `.deb` and `.rpm` packages, a Homebrew tap in this
  repository, and publication to crates.io.
```

- [ ] **Step 6: Commit**

```bash
git add docs/maintainer-notes.md CHANGES.md
git commit -m "docs: what to do before and during a release

The three one-time setup steps are the ones a release fails on if they are
missed: the repository has no remote yet, the crate publish needs a token, and
both the version and homebrew jobs push to main, which default workflow
permissions forbid.

Also says where the deliberate omissions are written down, so the next person
to wonder where the apt repository went finds the reasoning instead of
assuming it was forgotten."
```

---

## Notes for the implementer

**Things that will bite:**

1. **`CARGO_TARGET_DIR` is shared and outside the repository.** `./target` does not
   exist. Every command that reaches for a build artifact must resolve the real path
   with `cargo metadata` first. This trips up `cargo deb` inspection, the demo's `PATH`,
   and anything that greps a binary.

2. **Snapshot tests cover rendered box art.** If any change alters what the pager draws,
   `cargo test` fails on `insta` snapshots. Regenerate deliberately with
   `INSTA_FORCE_UPDATE=1 cargo test --jobs 4` and *read the diff* — never regenerate to
   make red go away. Nothing in this plan should change rendering; if a snapshot moves,
   that is a finding, not a chore.

3. **Never exceed 4 build jobs.** This machine has 128 cores and other tenants.

4. **The release workflow cannot be tested by running it.** The parts that *can* be
   rehearsed locally — the changelog rewrite, the formula rewrite, the deb and rpm
   builds, the Windows check — have explicit steps above. Do them; they are the only
   evidence available before a real release.

5. **`cargo publish` is irreversible.** A published version can be yanked but never
   replaced. It runs only inside the release workflow, gated on a secret; do not run it
   by hand while testing.
