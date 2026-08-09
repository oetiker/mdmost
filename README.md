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
| `--width N` | Render the whole document at this width instead of the terminal's. |
| `--body-width N` | Cap the prose body at N columns and centre it; `0` for no cap. |
| `--no-body-width` | Let the body use the full terminal width. |
| `--theme NAME` | The theme to start in. |
| `--no-icons` | Use plain Unicode instead of Nerd Font glyphs, at the same display width. |
| `--icons` | Use Nerd Font glyphs even if none appears to be installed. |
| `--mouse` | Capture the mouse: wheel scrolls, clicks select in the contents pane. |
| `--toc` | Start with the table-of-contents pane open. |
| `--config PATH` | Read configuration from this file instead of the default. |

Exit codes: `0` success, `1` unreadable input, `2` bad arguments. There is no `--color`
flag — the truecolour decision is made from whether stdout is a terminal.

## Nerd Fonts

Headings, list bullets, code fences and the status bar are drawn with Nerd Font glyphs
when a Nerd Font is available, and with plain Unicode equivalents **of the same display
width** when it is not — so the difference is what the markers look like, never where
anything sits. Nothing shifts, nothing reflows, and no feature is lost either way.

**`mdless` works out which to use, and errs towards plain.** No terminal can be asked
what font it is using, so mdless asks fontconfig whether an installed font covers every
glyph it would draw, and uses glyphs only if one does. It picks plain whenever it cannot
establish that — in particular when `fc-list` is unavailable, when output is not going to
a terminal, on `TERM=dumb` or the Linux console, and **over SSH**, where the fonts on the
machine running mdless say nothing about the terminal drawing the pixels. Guessing wrong
towards plain costs a little elegance; guessing wrong towards glyphs fills the screen
with replacement boxes, so the tie does not go to the prettier answer.

To decide for yourself, in increasing order of authority:

| | |
|---|---|
| `icons = true` / `false` in the configuration | settles it for this machine |
| `MDLESS_ICONS=1` / `0` in the environment | settles it for this shell — the natural thing to export in the profile on a server you always reach from the same well-equipped terminal |
| `--icons` / `--no-icons` | settles it for this run |

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
| `g`, `home` | Go to the top, and back to the left edge |
| `G`, `end` | Go to the bottom of the document |
| `%` | Jump N percent into the document (`50%`) |
| `left` | Scroll left (wide content) |
| `right` | Scroll right (wide content) |

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
| `S` | Save the current settings for next time |

Notes on a few of these:

- Keys that take a count take it as a prefix: `10j`, `50%`.
- `Esc` unwinds one step at a time — it clears a search, then a filter, then returns
  focus from the contents pane, then closes it. It never quits; `q` does that.
- `/` inside the contents pane filters the headings fuzzily instead of searching the
  document.
- `←` / `→` scroll content that is wider than the terminal, such as a wide table or a
  long code line. Neither is ever reflowed or mangled to fit.
- `S` writes the settings you can change — theme, line numbers, contents pane, body
  width — back to the configuration file, and tells you which file it wrote. It edits
  that file rather than regenerating it: your comments, your ordering and any key a
  newer mdless understands are all still there afterwards, the previous version is kept
  as `config.toml.bak`, and a save whose result would not read back identically is
  refused rather than guessed at.

## Configuration

TOML, at `~/.config/mdless/config.toml` (or the platform's configuration directory —
`--config PATH` overrides it). A broken configuration never stops the program from
starting: the problem is reported and the rest of the file still applies, so one bad key
binding costs you that binding and nothing else.

```toml
theme        = "dark"    # name of a built-in or a [themes.*] table
icons        = true      # Nerd Font glyphs; false is plain Unicode; omit to detect
line_numbers = false     # line-number gutter in fenced code blocks
mouse        = false     # wheel scrolls, clicks select in the contents pane
scroll_step  = 3         # document lines per mouse-wheel notch
body_width   = 100       # widest the prose body is laid out; 0 for no cap

[toc]
open  = false            # start with the contents pane open
width = 32               # width of the contents pane, in columns

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

## Line length

Prose is capped at 100 columns by default and centred when the terminal is wider,
because a line that runs the full width of a wide terminal is hard to come back from —
the eye loses the start of the next one. Set `body_width` (or `--body-width`) to taste,
or `0` / `--no-body-width` to switch the cap off. On a terminal of 102 columns or fewer
the default cap does nothing at all.

The cap is about text that can be reflowed, so it does not apply to everything:

- **Tables and Mermaid diagrams ignore the cap** and are laid out at the full terminal
  width. Both stop at their natural width — a table does not stretch its columns to fill
  the room, and a diagram is drawn at the narrowest width that works — so this costs
  nothing when they are small. Wherever it ends up, a block is centred on the same axis
  as the prose rather than stranded at the left edge; only something as wide as the
  terminal starts at the margin.
- **Everything else takes the full width as soon as the cap would cut it short.** That
  is what a fenced code block gets: a short snippet sits with the prose, and a block with
  a long line takes the whole terminal. The same applies to a wide table or fence nested
  inside a block quote or list item.
- Content wider than the terminal itself is unaffected by any of this: it is still laid
  out at the width it needs and reached with `←` / `→`.

`--width` is a different setting and does not replace this one: it changes the width the
whole document is rendered at, including tables and code. `--body-width` caps only the
prose within whatever that width is.

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
  terminal keeps its shape and is reached with `←`/`→`. A cut line is marked with a `›`
  at the edge — or a `‹` once you have scrolled — while a box's own rules close with the
  corner they belong to, so the frame still reads as a box that continues.

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
