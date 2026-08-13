# Documentation design — one manual, three audiences

Date: 2026-08-13
Status: proposed, awaiting owner review

## 1. The problem

The README is 492 lines and does four jobs badly at once: it sells the pager, it
installs it, it teaches it, and it is a reference for every key, option and config
field. The reference half is **duplicated in `man/mdmost.1`**, which already carries
all 45 key bindings, the options, environment, files and exit status. Two copies of
the same table drift, and one of them is roff that nobody wants to edit.

Two further faults, both found on 2026-08-13:

- **The static box-art samples do not survive GitHub's font stack.** A border line is
  made entirely of box-drawing characters, so the whole line falls back to a different
  font with a wider advance, while text lines — mostly ASCII with two `│` — keep the
  base advance. The rules overshoot the body. Nothing can be done in Markdown: GitHub
  strips `style` and CSS from READMEs, so the font stack is not ours to set.
- **Nothing tells a reader how to configure their terminal** to have the glyphs the
  renderer draws, which is the same root cause one layer down.

## 2. The shape

Three artifacts, one of them generated:

| Artifact | Audience | Source? |
| --- | --- | --- |
| `README.md` | someone deciding, in 30 seconds | authored |
| `docs/manual.md` | someone using it, on the web | **authored — the single source** |
| `man/mdmost.1` | someone using it, in a terminal | **built artifact, not in git** |

**`man/mdmost.1` is never committed.** It is generated in CI, and the stages that need
it consume it from there. `man/` is gitignored.

This was designed the other way first — committed output plus a CI staleness gate — and
that was worse. A generated file under version control can disagree with its source, so
it needs a gate, and the gate is a thing to maintain and a way to fail the build for a
reason no reader cares about. **A file that does not exist cannot be stale.** The
consumers turn out not to need it committed:

| Consumer | Where it reads the page | Needs it in git? |
| --- | --- | --- |
| Homebrew (`Formula/mdmost.rb:39`) | **our release tarball**, staged by `release.yml:143` | no |
| deb assets (`Cargo.toml:113`) | working tree during `build-binaries` | no |
| rpm assets (`Cargo.toml:126`) | working tree during `build-binaries` | no |
| release tarball (`release.yml:143`) | working tree during `build-binaries` | no |

All three working-tree consumers run inside the same `build-binaries` job, so **one
generate step at the top of that job serves all of them**.

The one snag is `cargo publish` (`release.yml:232`): a generated, untracked `man/` makes
the tree dirty and publishing would need `--allow-dirty`. The fix is to add `man/**` to
`Cargo.toml`'s `exclude` instead. Nothing is lost — **`cargo install` does not install
man pages**, so the page inside the `.crate` was inert bytes that no user could ever
reach.

**What this costs, stated plainly:** a git checkout has no man page until someone runs
`make man`, which needs pandoc. That affects contributors, and any distro packager who
builds from a git tag rather than from the release tarball or the crate. Given that
every packaging path this project actually ships — deb, rpm, Homebrew, tarball — is
built by our own CI, the trade is worth it.

## 3. Generation

```sh
pandoc --standalone --to man docs/manual.md -o man/mdmost.1
```

pandoc 3.1.3 is the version this is designed against. Its man writer turns definition
lists into `.TP`, which is the shape the current hand-written page already uses for its
45 keys, so the output stays idiomatic roff.

**The date comes from metadata, not from the build.** pandoc stamps the header with
today's date unless told otherwise, which would mean two builds of the same source
producing different pages — and would put a meaningless "date" on the page equal to
whenever the release happened to be cut. (With the page uncommitted this no longer
breaks any diff, as it would have under the rejected design; it is now purely about the
page telling the truth about when the manual was last revised.) `docs/manual.md` opens
with:

```yaml
---
title: MDMOST
section: 1
header: mdmost manual
footer: mdmost
date: 2026-08-13
---
```

`date` is edited by hand when the manual is substantively revised. **`footer` carries no
version number**, deliberately: a version there would make every release bump the `.1`,
which would couple the release workflow to doc regeneration for no reader benefit.

**Entry point.** The repository has no Makefile and no `scripts/`. Add a minimal
`Makefile` with one target, and nothing else — broader targets are out of scope here:

- `make man` — build `man/mdmost.1` from `docs/manual.md`.

Contributors run it when they want to read the page locally; CI runs the same target, so
there is exactly one command and no second code path that could drift from it.

**Where CI builds it.**

- `.github/workflows/release.yml`, job `build-binaries`: `make man` as the first step,
  before `cargo deb`, `cargo generate-rpm` and the tarball staging at line 143. One
  step, three consumers.
- `.github/workflows/ci.yml`: a `docs` job that installs pandoc and runs `make man`.
  Nothing consumes the output there — its job is to fail the build when `docs/manual.md`
  stops converting, so a broken manual is caught on the pull request that broke it and
  not at release time.

There is **no staleness check**, because there is nothing that can be stale.

## 4. What the README becomes

Target: roughly 150 lines. Order:

1. `# mdmost`, tagline, three sentences of what it is.
2. **The demo movie** — the only visual. Both static box-art blocks (README:37-51 and
   README:434-450) are **deleted**, which is what disposes of §1's font problem.
3. **Install** — Homebrew tap, release binaries, `cargo install`, deb/rpm.
4. **Quick start** — open a file, `--mouse`, the six keys that matter.
5. **What makes it different** — short prose, no tables: rendering is a pure function of
   `(document, width, theme, options)` so everything reflows; a selection copies the
   Markdown *source*; links, anchors and footnotes are live; Mermaid becomes box art.
6. **What it is not** — kept, it is short and it sets expectations.
7. **Terminal setup** — §6 below, in its short form, linking to the manual's long form.
8. **Configuration** — a six-line example, then a link to the manual.
9. **Keys** — a ten-row cheat sheet, then `man mdmost` and a link to the manual.
10. **Development**, **License**.

Everything else moves into `docs/manual.md`. Nothing is deleted except the two box-art
blocks and the reference tables that the manual now owns.

**Links must be absolute.** `Cargo.toml` excludes `docs/**` from the published crate, so
on crates.io a relative link to `docs/manual.md` or to the hero image at
`docs/demo/mdmost.webp` has nothing to resolve against. Every link and image in the
README that points into `docs/` uses a full
`https://github.com/oetiker/mdmost/...` (or `raw.githubusercontent.com` for the image)
URL. This pins them to a branch, which is accepted: a broken hero image on the crate
page is worse than a branch-pinned URL.

## 5. What the manual contains

`docs/manual.md`, in this order. Sections marked *(from man)* already exist in
`man/mdmost.1` and are translated, not rewritten; sections marked *(from README)* move
out of the README largely intact.

1. **Name**, **Synopsis** *(from man)*
2. **Description** *(from man)* — expanded with the rendering model.
3. **Options** *(from man)* — every flag.
4. **Keys** *(from man)* — all 45, in the existing four groups: Moving, Structure,
   Searching, Other.
5. **Links and footnotes** *(from README)* — what becomes a control, what is inert and
   why, anchors, the footnote popup, the keyboard cursor.
6. **Selecting and copying** *(from README)* — the four payload flavours and what the
   status bar names.
7. **Configuration** *(from README)* — the full schema: every key, `[toc]`,
   `[themes.*]`, defaults.
8. **Rendering** *(from README)* — syntax highlighting, line length and `body_width`,
   the rules worth knowing, Mermaid families.
9. **Terminal setup** — new, §6.
10. **Environment**, **Files**, **Exit status** *(from man)*
11. **See also**, **Author** *(from man)*

## 6. The terminal setup section

This is new content and the reason the owner asked for it. It is written **coverage
first, font names second** — the requirement is which Unicode blocks the terminal font
must cover, and named fonts are examples that satisfy it, never an instruction to
install a particular font.

Structure:

- **What mdmost draws**: the block list, derived empirically (§7), not guessed.
- **What goes wrong without coverage**: the fallback font's advance width differs from
  the base font's, so a line made entirely of box characters is a different width from
  the text lines around it and the frame stops lining up. This is exactly what happens
  to these samples on GitHub, and it is worth saying so — the reader may have just seen
  it.
- **How to fix it, by platform**: a fontconfig fallback chain on Linux
  (`~/.config/fontconfig/fonts.conf`), and the terminal's own font-fallback setting
  elsewhere. Concrete, copy-pasteable.
- **A stack known to work**, borrowed from ansidrama, which bundles exactly this trio
  and consults it in order: a text font (JetBrains Mono), **Symbols Nerd Font** for the
  Private Use Area icons, and **JuliaMono** for Unicode's symbol blocks — arrows,
  geometric shapes, dingbats, braille. Presented as "this combination is known to cover
  everything mdmost draws", not as a requirement.
- **Icons are detected, not assumed** — `icons` in the config, and why the default is
  detection.

## 7. Keeping the font advice true — and what this does NOT test

**It tests against no font whatsoever, and cannot.** It compares two things that are
both inside this repository: the codepoints the renderer emits, and the list of blocks
the manual claims to draw. It says nothing about whether any font on any machine has
glyphs for them. Checking that would mean rasterising a font and asserting something
about the reader's system — an assumption this project does not make in code, and one
that would be wrong on the next machine.

So the claim is narrow: **§6's block list cannot silently stop matching the renderer.**
Judge it on that. If that is not worth a test file, the alternative is to derive the
list once by hand during implementation and accept that it may rot.

A test, `tests/glyph_inventory.rs`, renders `tests/corpus/` **and a fixture per Mermaid
family** — the corpus has `diagrams.md` and `pipeline.mmd` but does not exercise all
seven, and a family whose glyphs are never rendered is a family the inventory cannot
see. It collects every non-ASCII codepoint emitted and asserts that set is a subset of a
documented inventory checked into the test itself, grouped by Unicode block.

Adding a glyph the manual does not mention fails the test with the new codepoint named.
This is what stops §6 from rotting: the alternative is prose that was true when written
and silently false two features later, which is the failure this project has already
paid for elsewhere. The test pins the *inventory*, not the appearance, so it does not
constrain the renderer — only the honesty of the documentation.

## 8. Non-goals

- **No GitHub Pages site.** Rejected in the publishing spec §1 and still rejected.
- **No `mdmost(5)`.** The config schema lives in the one manual; a second man page means
  a second generated artifact in all four packaging paths.
- **No automated font installation**, and no font detection beyond the `icons` probe
  that already exists.
- **No rewrite of the man page's content** where it is already correct. Translation
  first; improvement only where the README's version is better.
- **The demo is not touched by this work.**

## 9. Acceptance

- `make man` produces `man/mdmost.1` from a clean checkout with nothing but pandoc
  installed, and `man/` is gitignored and absent from a fresh clone.
- `man ./man/mdmost.1` renders without roff warnings, and the four groups of keys appear
  as `.TP` entries.
- `cargo package --list` contains no `man/` entry, and `cargo publish` needs no
  `--allow-dirty`.
- A release run produces a tarball containing `man/mdmost.1`, and deb and rpm packages
  that install it to `/usr/share/man/man1/`.
- README contains no box-drawing characters at all.
- Every README link into `docs/` is absolute.
- `tests/glyph_inventory.rs` passes, and fails when a new non-ASCII glyph is introduced.
- The four packaging consumers of `man/mdmost.1` are untouched.

## 10. Open questions for the owner

1. **The tagline.** The README currently opens with "`less`, but it knows what Markdown
   means"; the demo card was changed on 2026-08-13 to "less but moreso and it knows
   Markdown"; `Cargo.toml`'s description is a third, flatter wording. One of the three
   should win in all three places.
2. ~~**The demo could show icons.**~~ **Answered 2026-08-13: `icons = true`.** The old
   justification — "ansidrama's bundled JetBrains Mono, which has no Nerd Font glyphs" —
   was a claim about someone else's bundled font that stopped being true when they added
   two more. `demo/config.toml` now pins icons ON, and says why it is pinned rather than
   detected. **The demo must be re-recorded against an ansidrama new enough to carry
   Symbols Nerd Font**, which the owner is releasing; until then the recording on disk
   does not match its own config. This is demo work, tracked here only because this spec
   is where the error surfaced.
