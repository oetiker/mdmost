# mdless

A full-screen terminal pager for a single Markdown document — `less`, but it knows what
Markdown means.

It parses the document once, then draws it as styled Unicode: tables get real borders
and negotiated column widths, fenced code is syntax-highlighted, and Mermaid diagrams
are laid out as box art rather than shown as source. Resize the terminal and everything
reflows, because rendering is a pure function of `(document, width, theme, options)`.

```
 ╭──────────┬───────────┬─────────────╮
 │ Language │ Extension │ Highlighted │
 ├──────────┼───────────┼─────────────┤
 │ Rust     │    .rs    │         yes │
 │ Python   │    .py    │         yes │
 │ TOML     │   .toml   │         yes │
 ╰──────────┴───────────┴─────────────╯

 ╭ rust ────────────────────────────────────────────────────╮
 │ fn main() {                                              │
 │     println!("hello");                                   │
 │ }                                                        │
 ╰──────────────────────────────────────────────────────────╯
```

## What it is not

The scope is deliberately narrow, and these are decisions rather than gaps:

- **Not an editor.** The document is read-only.
- **Not a file browser.** One document per invocation; there is no tree, no next-file.
- **No link following.** Links are shown and styled; nothing opens a browser.
- **No HTML.** Raw HTML in the source is skipped rather than rendered or shown.
- **No raster images.** An image becomes a captioned placeholder with its alt text and
  target — no sixel, no kitty protocol.

## Install

```sh
cargo build --release
install -m755 target/release/mdless ~/.local/bin/
```

Rust 2024 edition; no system dependencies beyond a terminal that speaks ANSI truecolour.

## Quick start

```sh
mdless README.md              # open a document
mdless                        # read standard input
cat notes.md | mdless         # same, keyboard still works (see below)
export PAGER=mdless           # use it as your pager
mdless --render-once notes.md # print one frame and exit
```

Two things make the pipe cases work. When the input is a pipe, the keyboard is read from
`/dev/tty`, so `cat x.md | mdless` is still interactive. When *stdout* is not a terminal,
`--render-once` is implied, so `mdless x.md | cat` produces plain text instead of escape
soup — and `--render-once` emits truecolour to a terminal and plain text otherwise, which
is what makes it usable for scripting and snapshotting.

```sh
mdless --render-once --width 80 --no-icons doc.md > snapshot.txt
```

### Options

| Option | Meaning |
|---|---|
| `--render-once` | Render one frame to stdout and exit. Needs no terminal. |
| `--width N` | Render at this width instead of the terminal's. |
| `--theme NAME` | The theme to start in. |
| `--no-icons` | Use plain Unicode instead of Nerd Font glyphs. **This is the default.** |
| `--icons` | Use Nerd Font glyphs. Needs a terminal font that has them. |
| `--mouse` | Capture the mouse: wheel scrolls, clicks select in the contents pane. |
| `--toc` | Start with the table-of-contents pane open. |
| `--config PATH` | Read configuration from this file instead of the default. |

Exit codes: `0` success, `1` unreadable input, `2` bad arguments. There is no `--color`
flag — the truecolour decision is made from whether stdout is a terminal.

## Nerd Fonts are optional

**Plain Unicode is the default.** `mdless` ships two glyph vocabularies of identical
display width, so turning icons on or off never changes the layout — only the glyphs.
Nerd Font icons are opt-in via `--icons` or `icons = true`, because there is no way to
ask a terminal whether it has a patched font, and guessing wrong fills the screen with
replacement boxes.

## Keys

Bindings are remappable, and the in-app help overlay (`h` or `F1`) is generated from the
same live binding table as the list below, so the two cannot drift apart.

#### Help and exit

| Keys | Action |
|---|---|
| `h`, `f1` | Show or hide this help |
| `esc` | Clear the search, close the overlay or pane |
| `q` | Quit |

#### Movement

| Keys | Action |
|---|---|
| `j`, `down` | Scroll down one line |
| `k`, `up` | Scroll up one line |
| `d`, `ctrl-d` | Scroll down half a screen |
| `u`, `ctrl-u` | Scroll up half a screen |
| `space`, `ctrl-f`, `pgdn` | Scroll down one screen |
| `b`, `ctrl-b`, `pgup` | Scroll up one screen |
| `g`, `home` | Go to the top of the document |
| `G`, `end` | Go to the bottom of the document |
| `%` | Jump N percent into the document (`50%`) |
| `left` | Scroll left (wide tables and code) |
| `right` | Scroll right (wide tables and code) |

#### Navigation

| Keys | Action |
|---|---|
| `[` | Go to the previous heading |
| `]` | Go to the next heading |
| `=`, `ctrl-g` | Report where you are |
| `tab` | Show or hide the table of contents |
| `enter` | Jump to the selected heading |

#### Search

| Keys | Action |
|---|---|
| `/` | Search forward |
| `?` | Search backward |
| `n` | Go to the next match |
| `N` | Go to the previous match |
| `ctrl-r` | Switch literal / regex search |

#### View

| Keys | Action |
|---|---|
| `t` | Switch to the next theme |
| `-` | Show or hide code line numbers |

Notes on a few of these:

- Keys that take a count take it as a prefix: `10j`, `50%`.
- `Esc` unwinds one step at a time — it clears a search, then a filter, then returns
  focus from the contents pane, then closes it. It never quits; `q` does that.
- `/` inside the contents pane filters the headings fuzzily instead of searching the
  document.
- `←` / `→` scroll content that is wider than the terminal, such as a wide table or a
  long code line. Neither is ever reflowed or mangled to fit.

## Configuration

TOML, at `~/.config/mdless/config.toml` (or the platform's configuration directory —
`--config PATH` overrides it). A broken configuration never stops the program from
starting: the problem is reported and the rest of the file still applies, so one bad key
binding costs you that binding and nothing else.

```toml
theme        = "dark"    # name of a built-in or a [themes.*] table
icons        = false     # Nerd Font glyphs
line_numbers = false     # line-number gutter in fenced code blocks
toc_open     = false     # start with the contents pane open
toc_width    = 32        # width of the contents pane, in columns
mouse        = false     # wheel scrolls, clicks select in the contents pane
scroll_step  = 3         # document lines per mouse-wheel notch

[keys]
"ctrl-n" = "line_down"   # bind a chord to an action
"t"      = "none"        # "none" removes a default binding

[themes.midnight]
base   = "dark"          # inherit everything unspecified from a built-in
accent = "#ff5f87"
green  = "#a6e3a1"
```

A `[themes.<name>]` table inherits from `base` (`"dark"` or `"light"`), so a custom theme
can be a two-line tweak rather than a full palette. Overridable colours: `bg`, `surface`,
`overlay`, `fg`, `muted`, `border`, `accent`, `red`, `orange`, `yellow`, `green`, `cyan`,
`blue`, `purple`, plus `dark = true|false` to tell the renderer which way the palette
leans. `t` cycles through the built-ins and anything you have defined.

## Mermaid

Fenced ```` ```mermaid ```` blocks are parsed and drawn as Unicode box art. All seven
families are supported; anything outside the supported subset degrades to a
syntax-highlighted code block with a dim caption saying why, so a diagram never takes the
document down with it.

| Family | Supported |
|---|---|
| `flowchart` / `graph` | Directions `TD`/`TB`/`LR`/`RL`/`BT`; shapes `[rect]`, `(round)`, `([stadium])`, `{rhombus}`, `((circle))`, `[[subroutine]]`, `[(cylinder)]`; edges `-->`, `---`, `-.->`, `==>` with `\|label\|` and `-- label -->`; nested `subgraph`. Out of scope: `click`, `style`/`classDef`, `linkStyle`. |
| `sequenceDiagram` | `participant`/`actor` with `as`; arrows `->`, `-->`, `->>`, `-->>`, `-x`, `--x`; self-messages; `activate`/`deactivate` and `+`/`-`; `Note left of\|right of\|over`; `loop`, `alt`/`else`, `opt`, `par`/`and`, `critical`. Out of scope: `autonumber`, `box`, `link`, `rect`. |
| `classDiagram` | Three-compartment boxes, visibility `+ - # ~`, `$`/`*` classifiers, generics, `<<interface>>`/`<<abstract>>` and other stereotypes; relations `<\|--`, `*--`, `o--`, `-->`, `..>`, `..\|>` with `"1"`/`"0..*"` cardinalities. |
| `erDiagram` | Entities with attribute tables (`type name PK "comment"`, including `PK`/`FK`/`UK`), aliases, crow's-foot cardinalities `\|\|--o{`, `}o--\|\|`, `\|\|--\|\|`, `}\|..\|{`, and relationship labels. |
| `stateDiagram-v2` | `[*]` start/end markers per scope, `S --> T : label`, composite `state X { … }`, `<<choice>>`, `<<fork>>`/`<<join>>`, `note left of`/`right of`. |
| `pie` | `title`, `showData`, `"label" : value`. Drawn as a sorted bar chart with percentages — a circle in character cells reads badly, so bars are the honest choice. |
| `gantt` | `title`, `dateFormat`, `axisFormat`, `section`, tasks with `after X` / durations / explicit dates, and `done`/`active`/`crit`/`milestone` tags. The time scale is chosen from the available width. |

Directives, `%%` comments and `%%{init}%%` blocks are parsed and ignored.

```
                                      ┌───────┐
                                      │ Start │
                                      └───┬───┘
                                          ▼
                                        ╱───╲
                                       │ OK? │
                                        ╲─┬─╱
                                ╭─────────┤
                                │yes      │no
                                │         ▼
                                ▼     ╭───────╮
                             ┌────┐   ├───────┤
                             │ Go │   │ Store │
                             └────┘   ├───────┤
                                      ╰───────╯
```

## Rendering rules worth knowing

- **Everything is width-driven.** No layout decision is taken at parse time, so a resize
  re-renders rather than patching. Table columns are negotiated against the available
  width; a cell is itself a nested document, so Markdown inside a table cell works.
- **Grapheme-safe throughout.** Widths are display columns, never bytes or `char`s.
  Combining marks, ZWJ emoji sequences and regional-indicator flags are never split, and
  every rendered row is exactly the requested width.
- **Wide content scrolls, it does not mangle.** A table or code line too wide for the
  terminal keeps its shape and is reached with `←`/`→`, marked with a `›` at the edge.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Snapshot tests use [`insta`](https://insta.rs); property tests use `proptest`. The design
spec lives in `docs/superpowers/specs/`, and QA reports in `docs/qa/`.

## License

MIT.
