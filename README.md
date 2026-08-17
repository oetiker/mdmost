# mdmost

A full-screen terminal pager for one Markdown document

* resize the terminal and everything reflows
* mermaid diagrams -> rendered
* highlight with the mouse -> Markdown source copied
* table too wide -> side-scroll just the table

![less on the left, mdmost on the right, on the same file](https://raw.githubusercontent.com/oetiker/mdmost/main/docs/demo/mdmost.webp)

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

**Any Linux** — the tarballs are static musl builds and need nothing installed. The
archive carries the man page beside the binary, so install both:

```sh
tar xzf mdmost-*-x86_64-unknown-linux-musl.tar.gz
sudo install -Dm755 mdmost/mdmost       /usr/local/bin/mdmost
sudo install -Dm644 mdmost/man/mdmost.1 /usr/local/share/man/man1/mdmost.1
```

**Rust** — `cargo install mdmost`, or `cargo build --release` from a checkout. Neither
route installs a man page: `cargo install` does not handle man pages at all, and the page
is generated rather than shipped. Run `make man` in a checkout to build one; it needs
pandoc. Rust 2024 edition, and no system dependencies beyond a terminal that speaks ANSI
truecolour. The build needs no C compiler, which is why the regex engine behind the
highlighter is `fancy-regex` rather than oniguruma.

Two caveats. The **macOS** tarball binaries are neither signed nor notarised, so
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

Two rules make the pipe cases work. When the input is a pipe, the keyboard is read from
`/dev/tty`, so `cat x.md | mdmost` stays interactive. When *stdout* is not a terminal,
`--render-once` is implied, so `mdmost x.md | cat` produces plain text instead of escape
sequences. `mdmost --render-once --width 80 doc.md` is therefore usable for scripting and
snapshotting.

Every flag is in the
[manual](https://github.com/oetiker/mdmost/blob/main/docs/manual.md#options), or in
`man mdmost`.

## What makes it different

**Resizing re-renders.** No layout decision is taken at parse time, so a resize does not
patch what is on screen: it renders the document again at the new width. A table
renegotiates its columns, a diagram re-lays its node boxes, and prose re-wraps.

**Wide content scrolls sideways.** Some content will not fold at any width: a
five-column table, a diagram that wants 188 columns, a long line of code. It keeps its
shape, and `left` and `right` reach the rest. It scrolls on its own, while the prose
around it stays where it was. A cut line is marked at the edge, and a box's rules still
close with the corner they belong to.

**The mouse works.** With `--mouse` the wheel scrolls, the scrollbar drags, contents
entries jump, links light up under the pointer with their target in the status bar, and
code frames and tables grow a `[copy]` button. It is off by default, because capturing
the mouse takes away the terminal's own drag-select.

**A selection copies the source, not the screen.** Drag over a rendered heading and
`# Wide diagram` lands on the clipboard; over a bold word, `**bold**`; over a link,
`[text](url)`. A drag across reflowed rows copies the source's own line breaks. The
`[copy]` buttons take a whole block: code exactly as written, a table as tab-separated
cells a spreadsheet will split into columns.

**Links, anchors and footnotes are live.** Clicking an `http` link opens it, and a
`#heading` reference scrolls there. No mouse is needed: `f` walks a keyboard cursor from
one control to the next, `enter` follows it, and the full URL is in the status bar first.
A footnote marker opens the note in a box beside it without moving the page.

**Mermaid becomes box art.** All seven families are drawn as Unicode rather than dumped
as source. Anything outside the supported subset degrades to a highlighted code block
with a caption giving the reason.

## What it is not

The scope is narrow on purpose:

- **Not an editor.** The document is read-only.
- **No HTML.** Raw HTML in the source is skipped rather than rendered or shown.
- **No images.** An image becomes a captioned placeholder with its alt text and target.
  There is no sixel and no kitty protocol yet.

## Terminal setup

`mdmost` draws box-drawing, block and geometric characters. If your terminal font does
not cover them it falls back to one that does, and a fallback with a different advance
width makes a line of box characters a different width from the text around it, so the
frames shear. Nothing inside the pager can correct that.

The [manual's terminal setup
section](https://github.com/oetiker/mdmost/blob/main/docs/manual.md#terminal-setup)
lists exactly which Unicode blocks a font has to cover, gives a fontconfig fallback
chain you can paste, and names a font stack known to work. Nerd Font icons are detected
rather than assumed, and `--no-icons` turns them off at the same display width.

## Configuration

TOML, at `~/.config/mdmost/config.toml`. A broken file does not stop the program from
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
ordering. The full schema, with every key, `[toc]`, `[keys]` and custom `[themes.*]`, is
in the
[manual](https://github.com/oetiker/mdmost/blob/main/docs/manual.md#configuration).

## Keys

| Keys | Action |
|---|---|
| `q` | Quit; `esc` does not quit |
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
help overlay is generated from the same live binding table, so it shows the bindings in
effect rather than the defaults.

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
permissive notice or the Unlicense. The MIT, BSD and Apache-2.0 ones among them require
their notices to be reproduced in binary distributions, and `mdmost --licenses` prints
them. The TOML and Dockerfile definitions in `assets/syntaxes/` are `mdmost`'s own, MIT
like the rest.
