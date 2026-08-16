---
title: MDMOST
section: 1
header: mdmost manual
footer: mdmost
date: 2026-08-13
---

# NAME

mdmost - full-screen terminal pager for a single Markdown document

# SYNOPSIS

**mdmost** \[*OPTIONS*\] \[*FILE*\]

# DESCRIPTION

**mdmost** parses a Markdown document once and draws it as styled Unicode: tables
get real borders and negotiated column widths, fenced code is syntax-highlighted,
and Mermaid diagrams are laid out as box art rather than shown as source.

Rendering is a pure function of the document, the width, the theme and the
options. No layout decision is taken at parse time, so resizing the terminal
discards the canvas and renders again rather than patching what is on screen.
That is why everything reflows, and why the same table is drawn dense in a wide
terminal and spaced in a narrow one.

With no *FILE*, the document is read from standard input; the keyboard is then
read from */dev/tty*, so `cat notes.md | mdmost` stays interactive. When standard
output is not a terminal, `--render-once` is implied, so `mdmost doc.md | cat`
produces plain text rather than escape sequences.

# OPTIONS

- **`--render-once`** — Render one frame to standard output and exit. Needs no terminal.
  Truecolour goes to a terminal and plain text goes anywhere else, which is what makes
  it usable for scripting and snapshotting.

- **`--width N`** — Render the whole document at this width instead of the terminal's.

- **`--body-width N`** — Cap the prose body at N columns and centre it; `0` for no cap.

- **`--no-body-width`** — Let the body use the full terminal width.

- **`--theme NAME`** — The theme to start in.

- **`--icons`** — Use Nerd Font glyphs even if none appears to be installed.

- **`--no-icons`** — Use plain Unicode instead of Nerd Font glyphs, at the same display
  width.

- **`--mouse`** — Capture the mouse: the wheel scrolls, the scrollbar drags, clicks jump
  in the contents pane, and dragging over the document copies the Markdown source behind
  it.

- **`--toc`** — Start with the table-of-contents pane open.

- **`--config PATH`** — Read configuration from this file instead of the default.

- **`--licenses`** — Print the licences of the bundled syntax definitions and exit.

- **`-h`, `--help`** — Print help and exit.

- **`-V`, `--version`** — Print the version and exit.

There is no `--color` flag. The truecolour decision is made from whether standard
output is a terminal, which is the same question `--render-once` already answers.

# KEYS

Bindings are remappable; see `[keys]` under **CONFIGURATION**. The in-app help
overlay is generated from the same live binding table as this list, so the two
cannot drift apart, and the status bar always names the keys you have actually
bound rather than the defaults.

## Moving

- **`j`, `Down`** — Scroll down one line.

- **`k`, `Up`** — Scroll up one line.

- **`d`, `Ctrl-d`** — Scroll down half a screen.

- **`u`, `Ctrl-u`** — Scroll up half a screen.

- **`space`, `Ctrl-f`, `PgDn`** — Scroll down one screen.

- **`b`, `Ctrl-b`, `PgUp`** — Scroll up one screen.

- **`g`, `Home`** — Go to the top, and back to the left edge.

- **`G`, `End`** — Go to the bottom of the document.

- **`%`** — Jump N percent into the document, as in `50%`.

- **`Left`, `Right`** — Scroll content that is wider than the terminal, such as a wide
  table or a long code line. Neither is ever reflowed or mangled to fit.

## Structure

- **`[`** — Go to the previous heading.

- **`]`** — Go to the next heading.

- **`=`, `Ctrl-g`** — Report where you are.

- **`Tab`** — Show or hide the table of contents.

- **`f`** — Move the keyboard cursor to the next link or button in the document,
  scrolling to bring it into view.

- **`F`** — Move the keyboard cursor to the previous link or button in the document,
  scrolling to bring it into view.

- **`Enter`** — Jump to the selected heading, or follow the link or button under the
  keyboard cursor. The status bar shows the full URL under the cursor, the same as it
  does for a mouse hover, so `Enter` never sends you somewhere unseen.

## Searching

- **`/`** — Search forward.

- **`?`** — Search backward.

- **`n`, `Ctrl-Down`** — Go to the next match.

- **`N`, `Ctrl-Up`** — Go to the previous match.

- **`Ctrl-r`** — Switch between literal and regex search.

## Other

- **`t`** — Switch to the next theme. This cycles through the built-ins and anything you
  have defined.

- **`-`** — Show or hide code line numbers.

- **`S`** — Save the current settings for next time.

- **`h`, `F1`** — Show or hide the help overlay.

- **`Esc`** — Clear the search, or close the overlay or pane. It never quits.

- **`q`** — Quit.

## Notes on a few of these

Keys that take a count take it as a prefix: `10j`, `50%`.

`Esc` unwinds one step at a time. It clears a search, then a filter, then returns
focus from the contents pane, then closes it. It never quits; `q` does that.

`/` inside the contents pane filters the headings fuzzily instead of searching
the document.

While a search is live the status bar carries the query and which match you are
on out of how many there are, and, when there is more than one, the keys that
step between them, with the `Ctrl` alternatives beside them when the terminal is
wide enough. The current match is highlighted differently from the rest, and
reaching one scrolls sideways as well as down, so a hit inside a wide table or a
long code line is actually on screen when you arrive at it.

`S` writes the settings you can change from inside the pager --- theme, line
numbers, contents pane, body width --- back to the configuration file, and tells
you which file it wrote. It edits that file rather than regenerating it: your
comments, your ordering and any key a newer **mdmost** understands are all still
there afterwards, the previous version is kept as `config.toml.bak`, and a save
whose result would not read back identically is refused rather than guessed at.

# LINKS AND FOOTNOTES

Clicking a link opens it in your browser; clicking a `#heading` reference scrolls
that heading to the top of the viewport.

Both work with no `--mouse` flag. Press `f` (or `F` to go backward) to move a
keyboard cursor from one link or button to the next anywhere in the document,
scrolling to bring it on screen --- the same way `n` steps to the next search
hit --- and `Enter` to follow whatever it is on. The status bar shows the full
URL under the cursor exactly as it does for a mouse hover, so `Enter` never sends
you somewhere unseen. `Esc` puts the cursor away.

Only `http` and `https` links open. Anything else is shown as plain text and is
wholly inert.

Unlike the `[copy]` buttons described under **SELECTING AND COPYING**, links are
never hidden when the mouse was not captured. A link is content, not chrome, and
hiding it would mean hiding part of the document.

## Footnotes

A footnote marker opens the note in a box beside it, without moving the page.
The box is anchored to the marker and placed in whichever gap above or below it
has the room; on a screen too short for either it declines to open rather than
covering the line you are reading. It is at most 60 columns wide, because a note
that grows to fill a 200-column terminal stops reading as an aside.

The note is drawn by the ordinary renderer at the box's own width, so emphasis,
code spans, nested lists and even tables inside a footnote all work. The cursor
keys scroll the box while the document behind it holds still.

# SELECTING AND COPYING

With `--mouse` (or `mouse = true`) a left drag over the document selects, and
releasing puts the **Markdown source** behind the selection on the clipboard ---
not the glyphs on screen. Dragging over a rendered heading copies the `#` and its
text; over a bold word, `**bold**`; over a link, `[text](url)`; across a code
fence, the fence and its content verbatim. A drag that reflows across several
rows copies the source's own line breaks, not the renderer's. `Esc` clears the
highlight.

A code frame and a table each carry a `[copy]` in the right of their top edge.
Pressing it copies the **whole block** --- the code exactly as it is written, the
table as a grid of tab-separated cells that a spreadsheet splits into columns,
with an HTML flavour offered alongside where the local clipboard takes one. The
label reads `[copied]` for a moment and the status bar names what went out.

There is no key for the buttons and no setting. They appear only when the mouse
was actually captured, because a control nobody can press is worse than none.
They are dropped where they would not fit, and a table wider than the terminal
carries its button off to the right until you scroll to it.

Two things are worth knowing.

A selection over a Mermaid diagram has no source map to invert --- the renderer
records one for inline text and for code lines, not for box art --- so it copies
what is drawn instead, and the status bar says `rendered text` rather than
`Markdown source`.

The copy goes out as OSC 52 first, which works over SSH but which the terminal
never acknowledges. If that is the only route that ran, the status bar says
`sent … (unconfirmed)` rather than `copied`. `tmux` needs
`set -g set-clipboard on` to pass it along, and `xterm` needs `allowWindowOps`.
On a local display server the `arboard` fallback runs too and the report becomes
`copied`.

Turning the mouse on is a trade: capturing it takes away the terminal's own
drag-select, which outlives the pager and which your fingers already know.

# CONFIGURATION

TOML, at *~/.config/mdmost/config.toml*, or the platform's own configuration
directory where that differs. `--config PATH` overrides it. See **FILES**.

A broken configuration never stops the program from starting: the problem is
reported and the rest of the file still applies, so one bad key binding costs you
that binding and nothing else.

```toml
theme        = "dark"    # name of a built-in or a [themes.*] table
icons        = true      # Nerd Font glyphs; false is plain Unicode; omit to detect
line_numbers = false     # line-number gutter in fenced code blocks
title_banner = false     # off; true sets a lone `#` title as a wrapped FIGlet banner
section_numbers = true   # number headings when a document nests three levels or more
mouse        = false     # wheel scrolls, scrollbar drags, TOC clicks jump, drag copies
                         # source, and code frames and tables get a [copy] button
scroll_step  = 3         # document lines per mouse-wheel notch
body_width   = 72        # widest the prose body is laid out; 0 for no cap

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

Most command-line options have a configuration-file counterpart, and a few
settings exist only there. In particular `title_banner` is `false` unless asked
for: set `title_banner = true` to have a document whose first block is its one
and only `#` heading drawn as a FIGlet banner. Section numbering,
`section_numbers`, is on by default; the banner is not. Both are described under
**RENDERING**.

## Themes

A `[themes.<name>]` table inherits from `base` --- `"dark"` or `"light"` --- so a
custom theme can be a two-line tweak rather than a full palette. The overridable
colours are `bg`, `surface`, `overlay`, `fg`, `muted`, `border`, `accent`, `red`,
`orange`, `yellow`, `green`, `cyan`, `blue` and `purple`, plus
`dark = true|false` to tell the renderer which way the palette leans. `t` cycles
through the built-ins and anything you have defined.

## Whether icons are used

Three settings answer the same question, in increasing order of authority:

- **`icons = true` / `false`** — Settles it for this machine.

- **`MDMOST_ICONS=1` / `0`** — Settles it for this shell. See **ENVIRONMENT**.

- **`--icons` / `--no-icons`** — Settles it for this run.

Omit all three and **mdmost** decides for itself. How it decides, and why it errs
towards plain, is under **TERMINAL SETUP**.

# RENDERING

## Title banner

Set `title_banner = true` and a document whose first block is its one and only
`#` heading opens with that title in the FIGlet *Small* font, wrapped between
words over as many lines as it needs and centred.

It is **off by default**: art in place of someone else's title is a decoration,
and a default is the wrong place to hold that opinion. Turned on, it still
declines --- and draws an ordinary heading --- when the title is not plain ASCII,
or when a single word is too wide for the measure to break.

## Section numbers

A document that nests three or more section levels gets section numbers --- `1`,
`1.1`, `1.1.1` --- in front of its headings and in the contents pane, drawn in a
quiet grey of their own so it is obvious they are **mdmost**'s and not the
author's.

A lone `#` title is not a section: it stays unnumbered and its `##`s are numbered
`1`, `2`, `3`. A flat document gets nothing, because it needs nothing.
`section_numbers = false` turns them off.

Heading levels are also told apart by the rule underneath them --- heavy, light,
then dashed. Headings have no marker in front of them.

## Syntax highlighting

Fenced code is highlighted from the syntax definitions curated by the `bat`
project --- a little over two hundred languages, compiled into the binary, so
there is nothing to install and nothing to configure. That includes the ones a
2020s README actually contains: TypeScript and TSX, Kotlin, Swift, Zig, Nix,
TOML, Dockerfile, Terraform/HCL, Elixir, Dart, Julia, Protobuf, GraphQL, Vue,
Svelte, Sass and SCSS, F#, CMake, Solidity, Nim, x86-64 assembly, `.env` files,
`go.mod`, `nginx.conf` and `.gitignore`.

Two definitions are missing on purpose. **PowerShell** and **ARM assembly** need
regex features the pure-Rust engine cannot compile, and **mdmost** uses that
engine so the build needs no C toolchain. They render as plain text, like any tag
we do not know.

The fence tag is matched against every syntax name and every file extension, so
`rs`, `py`, `yml`, `sh`, `ts`, `tsx`, `kt`, `c++`, `hcl` and their friends all
land where you would expect; a short table of aliases covers the rest (`golang`,
`console`, `jsonc`, `csharp`, `fsharp`, `objc`, `plaintext`, and so on). Only the
first word of the info string is read, so a fence tagged `rust,no_run` highlights
as Rust. **A tag nobody recognises is never an error** --- the block is drawn as
plain themed text, still framed, still with its label.

Colours never come from the syntax definitions. Each scope is mapped to a
semantic slot --- keyword, string, number, comment, type, namespace, escape ---
and the slot is filled from the active **mdmost** theme, so code sits inside the
palette instead of fighting it.

TOML and Dockerfile use definitions written for **mdmost** rather than the
bundled ones, because the bundled TOML gives a `[table.header]` no scope at all
and the bundled Dockerfile emits a whole `RUN` line as one undifferentiated span.

## Line length

Prose is capped at 72 columns by default and centred when the terminal is wider,
because a line that runs the full width of a wide terminal is hard to come back
from --- the eye loses the start of the next one. Set `body_width` (or
`--body-width`) to taste, or `0` / `--no-body-width` to switch the cap off.
Seventy-two is inside the readable band rather than at the top of it, so the cap
bites on an 80-column terminal too --- but only on prose.

The cap is about text that can be reflowed, so it does not apply to everything:

- **Tables and Mermaid diagrams ignore the cap** and are laid out at the full
  terminal width. Both stop at their natural width --- a table does not stretch
  its columns to fill the room, and a diagram is drawn at the narrowest width
  that works --- so this costs nothing when they are small. Wherever it ends up,
  a block is centred on the same axis as the prose rather than stranded at the
  left edge; only something as wide as the terminal starts at the margin.
- **Everything else takes the full width as soon as the cap would cut it short.**
  That is what a fenced code block gets: a short snippet sits with the prose, and
  a block with a long line takes the whole terminal. The same applies to a wide
  table or fence nested inside a block quote or list item.
- Content wider than the terminal itself is unaffected by any of this. It is
  still laid out at the width it needs and reached with `Left` and `Right`.

`--width` is a different setting and does not replace this one: it changes the
width the whole document is rendered at, including tables and code.
`--body-width` caps only the prose within whatever that width is.

## Mermaid

Fenced `mermaid` blocks are parsed and drawn as Unicode box art. All seven
families are supported; anything outside the supported subset degrades to a
syntax-highlighted code block with a dim caption saying why, so a diagram never
takes the document down with it.

- **`flowchart` / `graph`** — Directions `TD`/`TB`/`LR`/`RL`/`BT`; shapes `[rect]`,
  `(round)`, `([stadium])`, `{rhombus}`, `((circle))`, `[[subroutine]]`, `[(cylinder)]`;
  edges `-->`, `---`, `-.->`, `==>` with `|label|` and `-- label -->`; nested
  `subgraph`. Out of scope: `click`, `style`/`classDef`, `linkStyle`.

- **`sequenceDiagram`** — `participant`/`actor` with `as`; arrows `->`, `-->`, `->>`,
  `-->>`, `-x`, `--x`; self-messages; `activate`/`deactivate` and `+`/`-`; `Note left
  of|right of|over`; `loop`, `alt`/`else`, `opt`, `par`/`and`, `critical`. Out of scope:
  `autonumber`, `box`, `link`, `rect`.

- **`classDiagram`** — Three-compartment boxes, visibility `+ - # ~`, `$`/`*`
  classifiers, generics, `<<interface>>`/`<<abstract>>` and other stereotypes; relations
  `<|--`, `*--`, `o--`, `-->`, `..>`, `..|>` with `"1"`/`"0..*"` cardinalities.

- **`erDiagram`** — Entities with attribute tables (`type name PK "comment"`, including
  `PK`/`FK`/`UK`), aliases, crow's-foot cardinalities `||--o{`, `}o--||`, `||--||`,
  `}|..|{`, and relationship labels.

- **`stateDiagram-v2`** — `[*]` start and end markers per scope, `S --> T : label`,
  composite `state X { … }`, `<<choice>>`, `<<fork>>`/`<<join>>`, `note left of` and
  `note right of`.

- **`pie`** — `title`, `showData`, `"label" : value`. Drawn as a sorted bar chart with
  percentages --- a circle in character cells reads badly, so bars are the honest
  choice.

- **`gantt`** — `title`, `dateFormat`, `axisFormat`, `section`, tasks with `after X`,
  durations or explicit dates, and `done`/`active`/`crit`/`milestone` tags. The time
  scale is chosen from the available width.

Directives, `%%` comments and `%%{init}%%` blocks are parsed and ignored.

## Rules worth knowing

**Everything is width-driven.** No layout decision is taken at parse time, so a
resize re-renders rather than patching. Table columns are negotiated against the
available width; a cell is itself a nested document, so Markdown inside a table
cell works.

**A table whose rows wrap gets air between them.** While every row fits on one
line the table is drawn dense. As soon as one row wraps, a blank line goes
between every pair of rows, because multi-line rows packed edge to edge read as
one block of prose. The zebra stripe is carried through the gap by a half block,
so the shading still groups each row's lines. This is decided at the width you
are reading at, so the same table is dense in a wide terminal and spaced in a
narrow one.

**Grapheme-safe throughout.** Widths are display columns, never bytes or
characters. Combining marks, ZWJ emoji sequences and regional-indicator flags are
never split, and every row the pager draws is padded to exactly the width of the
pane.

**Wide content scrolls, it does not mangle.** A table or code line too wide for
the terminal keeps its shape and is reached with `Left` and `Right`. A cut line
is marked at the edge --- one marker for content continuing to the right, another
once you have scrolled --- while a box's own rules close with the corner they
belong to, so the frame still reads as a box that continues.

**Lists and task boxes are ASCII.** The bullets (`*`, `>`, `+`, `-`, one per
nesting level) and the task boxes (`[ ]`, `[x]`) are ASCII whether or not a Nerd
Font is present. Lists turn up in nearly every document, so their markers are the
things on the page that can least afford to be invisible --- and most of these
are the literal Markdown the author typed.

**No HTML, and no raster images.** Raw HTML in the source is skipped rather than
rendered or shown. An image becomes a captioned placeholder with its alt text and
target; there is no sixel and no kitty protocol.

# TERMINAL SETUP

**mdmost** draws with characters from the Unicode blocks below. Your terminal
font, or a fallback behind it, has to cover them. This is a statement about
coverage, not about which font you should use: any font or chain that covers
these will do.

- **Box Drawing (U+2500-U+257F)** — Every table border, code frame and diagram box.

- **Block Elements (U+2580-U+259F)** — Zebra stripes, the scrollbar, gantt bars.

- **Geometric Shapes (U+25A0-U+25FF)** — Heading marks, diagram node shapes, arrowheads.

- **General Punctuation (U+2000-U+206F)** — The elision marker.

- **Mathematical Operators (U+2200-U+22FF)** — Class-diagram relations.

- **Misc Mathematical Symbols-A (U+27C0-U+27EF)** — Class-diagram generics.

- **Dingbats (U+2700-U+27BF)** — The marker on a degraded diagram's caption.

- **Latin-1 Supplement (U+0080-U+00FF)** — Whatever HTML entities the document decodes
  to.

- **Specials (U+FFF0-U+FFFF)** — The replacement character, drawn in place of one that
  cannot be represented.

- **Private Use Area (U+E000-U+F8FF)** — Code-fence language icons --- **only when icons
  are on**.

The Private Use Area row is the one you can opt out of, with `--no-icons`.
Everything above it is drawn whatever you do.

The interface adds one block the document never needs: **Arrows**
(U+2190-U+21FF), for the `Ctrl` key hints the status bar shows beside a live
search, and the search indicator itself.

The document's own text is not on this list and cannot be. A font that does not
cover the language you are reading was already a problem before **mdmost**
opened it.

## What goes wrong without it

A missing glyph is not usually a blank box. The terminal falls back to another
font, and that font's advance width need not match the base font's --- so a line
made *entirely* of box characters comes out a different width from the text lines
around it, and the frame stops lining up. A table's rules overshoot its contents;
a diagram's boxes shear.

This is not something **mdmost** can correct from the inside. Every row it draws
is padded to exactly the width of the pane, measured in display columns; what the
terminal then does with those columns is the font stack's business.

If you have seen these frames misalign in a web browser, that is the same fault
one layer up. GitHub strips CSS from Markdown, so the font stack there is not
ours to set either.

## Fixing it

On Linux, fontconfig decides. Put a fallback chain in
*~/.config/fontconfig/fonts.conf* and run `fc-cache -f`:

```xml
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <alias>
    <family>monospace</family>
    <prefer>
      <family>JetBrains Mono</family>
      <family>Symbols Nerd Font</family>
      <family>JuliaMono</family>
    </prefer>
  </alias>
</fontconfig>
```

On macOS and Windows fontconfig is not in play and the terminal decides. Set the
fallback in the terminal's own font settings --- iTerm2 has a separate font for
non-ASCII text, Windows Terminal takes a font fallback list in its
*settings.json*, and Kitty and WezTerm both take an explicit fallback list in
their configuration.

## A stack known to work

This trio, consulted in that order, covers everything **mdmost** draws:

- **JetBrains Mono** — The text font.

- **Symbols Nerd Font** — The Private Use Area icons.

- **JuliaMono** — Unicode's symbol blocks --- arrows, geometric shapes, dingbats,
  braille.

It is named because it is known to work, not because **mdmost** wants it.

## Icons are detected, not assumed

No terminal can be asked what font it is using. So **mdmost** asks fontconfig
whether an installed font covers every glyph it would draw, and uses icons only
if one does.

It picks plain whenever it cannot establish that: when `fc-list` is unavailable,
when output is not going to a terminal, on `TERM=dumb` or the Linux console, and
**over SSH**, where the fonts on the machine running **mdmost** say nothing about
the terminal drawing the pixels.

Guessing wrong towards plain costs a little elegance; guessing wrong towards
icons fills the screen with replacement boxes. The tie does not go to the
prettier answer.

Plain and icon glyphs occupy **the same display width**, so nothing shifts and
nothing reflows either way --- the difference is what the markers look like,
never where anything sits, and no feature is lost. To decide for yourself rather
than let it detect, see **CONFIGURATION**.

# ENVIRONMENT

- **`MDMOST_ICONS`** — `1` or `0` forces Nerd Font glyphs on or off. It outranks the
  configuration file and is outranked by `--icons` and `--no-icons`. Exporting it in a
  profile is the natural thing to do on a server you always reach from the same
  well-equipped terminal.

- **`PAGER`** — **mdmost** is usable as a pager; `export PAGER=mdmost` is the intended
  use.

# FILES

- ***~/.config/mdmost/config.toml*** — Configuration, in TOML. A broken file never stops
  the program from starting: the problem is reported and the rest of the file still
  applies, so one bad key binding costs you that binding and nothing else. The
  platform's own configuration directory is used where it differs.

- ***config.toml.bak*** — The previous configuration, kept beside it whenever `S` writes
  a new one.

# EXIT STATUS

- **`0`** — Success, including a quit from the pager and a broken pipe.

- **`1`** — The document could not be read, or the terminal could not be set up.

- **`2`** — The command line could not be parsed.

# SEE ALSO

`less`(1), `bat`(1)

# AUTHOR

Tobias Oetiker
