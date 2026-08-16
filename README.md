# mdmost

like less but for Markdown

A full-screen terminal pager for one Markdown document — one you can resize, point at
and copy out of while you are reading it. Drag the edge of the window and the table
you are looking at renegotiates its columns to fit. Drag across a paragraph and what
lands on the clipboard is the Markdown that produced it, not the glyphs on screen.

It renders properly too: real table borders with negotiated widths, syntax-highlighted
code, and Mermaid drawn as box art rather than dumped as source. What will not fit at
any width is not mangled to make it fit — a wide table or diagram scrolls sideways by
itself, while the prose around it stays put.

![less on the left, mdmost on the right, on the same file](https://raw.githubusercontent.com/oetiker/mdmost/main/docs/demo/mdmost.webp)

One file, `less` on the left and `mdmost` on the right. Watch the divider move: the
table renegotiates its columns and the diagram re-lays its boxes, live, at every width
in between. At the end, three copies cross into the other pane — a table as TSV, a
fenced block as its own source, a paragraph as Markdown.

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

**Resizing is the point, not a feature.** No layout decision is taken at parse time, so
a resize does not patch what is on screen — it renders the document again at the new
width. A table renegotiates its columns, a diagram re-lays its node boxes, and prose
re-wraps.

**Wide things scroll sideways instead of being mangled.** Some content will not fold at
any width: a five-column table, a diagram that wants 188 columns, a long line of code.
It keeps its shape and you reach the rest with `left` and `right` — and **it scrolls on
its own**, while the prose around it stays exactly where it was. A cut line is marked at
the edge, and a box's rules still close with the corner they belong to, so a frame that
continues off-screen still reads as a frame.

**The mouse works properly.** With `--mouse` the wheel scrolls, the scrollbar drags,
contents entries jump, links light up under the pointer with their target in the status
bar, and code frames and tables grow a `[copy]` button. It is off by default because
capturing the mouse takes away the terminal's own drag-select, and that is your call to
make rather than mine.

**A selection copies the source, not the screen.** Drag over a rendered heading and you
get `# Wide diagram` on the clipboard; over a bold word, `**bold**`; over a link,
`[text](url)`. A drag that reflowed across several rows copies the source's own line
breaks. The `[copy]` buttons take a whole block: code exactly as written, a table as
tab-separated cells a spreadsheet will split into columns.

**Links, anchors and footnotes are live.** Clicking an `http` link opens it; a `#heading`
reference scrolls there. No mouse is needed: `f` walks a keyboard cursor from one control
to the next and `enter` follows it, with the full URL in the status bar first. A footnote
marker opens the note in a box beside it without moving the page.

**Mermaid becomes box art.** All seven families are drawn as Unicode rather than dumped
as source, and anything outside the supported subset degrades to a highlighted code block
with a caption saying why — a diagram never takes the document down with it.

## How it compares

Checked in August 2026 against `mdcat` 2.15.0, `glow` 2.1.2, `frogmouth` 0.9.1,
`bat` 0.26.1 and `less` 590. `part` means the tool does some of it, or does it in a
way that is not really the same thing — the footnotes say which.

| | mdmost | mdcat | glow | frogmouth | bat | less |
|---|---|---|---|---|---|---|
| Reflows on resize | yes | no | yes | yes | no | no |
| Table borders | box | rules | rules | rules | no | no |
| Markdown in table cells | yes | yes | yes | part | no | no |
| Mermaid as a diagram | yes | yes | no | no | no | no |
| Highlighted code | yes | yes | yes | yes | yes | no |
| In-document search | yes | part | no | no | yes | yes |
| Contents pane | yes | part | no | yes | no | no |
| Keyboard link following | yes | part | no | yes | no | no |
| Copy Markdown source | yes | no | part | no | n/a | n/a |
| Inline images | no | yes | no | no | no | no |
| More than one file | no | part | yes | yes | part | part |

The three rows that this was written for are resizing, the mouse and copying. Drag the
divider and the table renegotiates its columns while you watch; drag across the text
and what lands on the clipboard is the Markdown that produced it. Those only mean
anything in a program that owns the screen for as long as you are reading, which is
why `mdcat` and `bat` are a different kind of tool rather than a worse one: they
render once, print, and exit.

`mdcat` is worth knowing about anyway, because it does things this does not. It draws
Mermaid, negotiates table widths, styles markup inside cells, and compiles the same
`bat` syntax definitions — and it puts **real images** on your terminal, which mdmost
will never do. The widely known `swsnr/mdcat` was archived in June 2026; the live fork
is `BIRSAx2/mdcat`. `frogmouth` is the only genuine file browser here, though it has
had no release since 2023.

Smaller print, so the table is not read for more than it says. `glow` reflows and
styles table cells just as this does; what it does not do is negotiate column widths
into a closed box or draw diagrams, and its `c` key copies the whole document rather
than a selection. `mdcat`'s contents is a list it prints, not a pane; its links are
OSC 8, so your terminal follows them rather than `mdcat`; and searching means the
`$PAGER` it hands off to. For `bat` and `less` the screen already *is* the Markdown
source, so copying it is not a feature they implement — hence `n/a` rather than a tick
they did not earn.

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
