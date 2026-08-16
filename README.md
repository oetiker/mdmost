# mdmost

like less but for Markdown

A full-screen terminal pager for one Markdown document. It parses the file once and
draws it as styled Unicode: tables get real borders and negotiated column widths,
fenced code is syntax-highlighted, and Mermaid diagrams are laid out as box art rather
than shown as source. Resize the terminal and everything reflows, because rendering is
a pure function of `(document, width, theme, options)`.

![less on the left, mdmost on the right, on the same file](https://raw.githubusercontent.com/oetiker/mdmost/main/docs/demo/mdmost.webp)

The same document in `less` and in `mdmost`. Dragging the divider is the whole argument:
a table renegotiates its column widths, a diagram re-lays its node boxes, and prose only
re-wraps. Content too wide to fold scrolls sideways rather than being mangled, and it
scrolls alone, while the prose around it holds still.

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

**Rust** — `cargo install mdmost`, or `cargo build --release` from a checkout. Rust 2024
edition; no system dependencies beyond a terminal that speaks ANSI truecolour. Pure Rust
all the way down — the build needs no C compiler, which is why the regex engine behind
the highlighter is `fancy-regex` rather than oniguruma.

Two honest caveats. The **macOS** tarball binaries are neither signed nor notarised, so
Gatekeeper will quarantine them; `brew install` is the path of least resistance. The
**Windows** build compiles and is checked on every push but has never been exercised in
anger: expect the mouse, the clipboard and font detection to be less well behaved there
than on Unix.

## Quick start

```sh
mdmost README.md              # open a document
mdmost                        # read standard input
cat notes.md | mdmost         # same, keyboard still works
export PAGER=mdmost           # use it as your pager
mdmost --mouse README.md      # wheel, drag-to-copy, clickable links
mdmost --render-once notes.md # print one frame and exit
```

Two things make the pipe cases work. When the input is a pipe the keyboard is read from
`/dev/tty`, so `cat x.md | mdmost` is still interactive; and when *stdout* is not a
terminal `--render-once` is implied, so `mdmost x.md | cat` produces plain text instead
of escape soup. That is what makes `mdmost --render-once --width 80 doc.md` usable for
scripting and snapshotting.

Every flag is in the
[manual](https://github.com/oetiker/mdmost/blob/main/docs/manual.md#options), or in
`man mdmost`.

## What makes it different

**Everything is width-driven.** No layout decision is taken at parse time, so a resize
re-renders rather than patching. Table columns are negotiated against the space
available, and a cell is itself a nested document, so Markdown inside a table cell works.

**A selection copies the source, not the screen.** Drag over a rendered heading and you
get `# Wide diagram` on the clipboard; over a bold word, `**bold**`; over a link,
`[text](url)`. Code frames and tables also carry a `[copy]` button — the table arrives as
tab-separated cells a spreadsheet will split into columns.

**Links, anchors and footnotes are live.** Clicking an `http` link opens it; a `#heading`
reference scrolls there. No mouse is needed: `f` walks a keyboard cursor from one control
to the next and `enter` follows it, with the full URL in the status bar first. A footnote
marker opens the note in a box beside it without moving the page.

**Mermaid becomes box art.** All seven families are drawn as Unicode rather than dumped
as source, and anything outside the supported subset degrades to a highlighted code block
with a caption saying why — a diagram never takes the document down with it.

## What it is not

The scope is deliberately narrow, and these are decisions rather than gaps:

- **Not an editor.** The document is read-only.
- **Not a file browser.** One document per invocation; there is no tree, no next-file.
- **No HTML.** Raw HTML in the source is skipped rather than rendered or shown.
- **No raster images.** An image becomes a captioned placeholder with its alt text and
  target — no sixel, no kitty protocol.

## Terminal setup

`mdmost` draws box-drawing, block and geometric characters. If your terminal font does
not cover them it falls back to one that does, and a fallback with a different advance
width makes a line of box characters a different width from the text around it — so the
frames shear. Nothing inside the pager can correct that.

The [manual's terminal setup
section](https://github.com/oetiker/mdmost/blob/main/docs/manual.md#terminal-setup)
lists exactly which Unicode blocks a font has to cover, gives a fontconfig fallback
chain you can paste, and names a font stack known to work. Nerd Font icons are detected
rather than assumed, and `--no-icons` turns them off at the same display width.

## Configuration

TOML, at `~/.config/mdmost/config.toml`. A broken file never stops the program from
starting: the problem is reported and the rest of the file still applies.

```toml
theme        = "dark"    # name of a built-in or a [themes.*] table
line_numbers = false     # line-number gutter in fenced code blocks
mouse        = false     # wheel, drag-to-copy, and [copy] buttons
body_width   = 72        # widest the prose body is laid out; 0 for no cap
section_numbers = true   # number headings when a document nests three levels or more
title_banner = false     # true sets a lone `#` title as a FIGlet banner
```

`S` writes the settings you changed back to that file, keeping your comments and
ordering. The full schema — every key, `[toc]`, `[keys]` and custom `[themes.*]` — is in
the
[manual](https://github.com/oetiker/mdmost/blob/main/docs/manual.md#configuration).

## Keys

| Keys | Action |
|---|---|
| `q` | Quit — `esc` never quits |
| `h`, `f1` | Show or hide the help overlay |
| `j`, `k` | Scroll one line |
| `space`, `b` | Scroll one screen |
| `g`, `G` | Top, bottom |
| `/`, `n`, `N` | Search, next match, previous |
| `tab` | Show or hide the contents pane |
| `f`, `enter` | Walk to the next link or button, then follow it |
| `t` | Next theme |
| `left`, `right` | Scroll wide content sideways |

All 45 bindings, and how to remap them, are in `man mdmost` or the
[manual](https://github.com/oetiker/mdmost/blob/main/docs/manual.md#keys). The in-app
help overlay is generated from the same live binding table, so it never drifts from what
you have actually bound.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
make man          # build man/mdmost.1 from docs/manual.md; needs pandoc
```

Snapshot tests use [`insta`](https://insta.rs); property tests use `proptest`. The man
page is generated and is not in git. Design specs live in `docs/superpowers/specs/`.

## License

MIT.

The syntax definitions compiled into the binary are third-party work, curated by the
[`bat` project](https://github.com/sharkdp/bat) and packaged by
[`two-face`](https://codeberg.org/CosmicHarper/two-face). Most are under Sublime's
permissive notice or the Unlicense; the MIT, BSD and Apache-2.0 ones among them require
their notices to be reproduced in binary distributions, and `mdmost --licenses` is where
they are — inside the binary, which is the artefact people actually receive. The TOML and
Dockerfile definitions in `assets/syntaxes/` are `mdmost`'s own and MIT like the rest.
