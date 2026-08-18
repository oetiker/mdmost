---
title: MDMOST
section: 1
header: mdmost manual
footer: mdmost
date: 2026-08-17
---

# NAME

mdmost - full-screen terminal pager for a single Markdown document

# SYNOPSIS

**mdmost** \[*OPTIONS*\] \[*FILE*\]

# DESCRIPTION

**mdmost** renders one Markdown document in the terminal. Tables get borders and
negotiated column widths, fenced code is syntax-highlighted, and Mermaid diagrams
are drawn as box art instead of shown as source.

Layout follows from the document, the terminal width, the theme and the options.
A resize renders the document again rather than patching the screen, so
everything reflows and the same table is drawn dense in a wide terminal and
spaced in a narrow one.

With no *FILE*, the document is read from standard input and the keyboard is read
from */dev/tty*, so `cat notes.md | mdmost` stays interactive. When standard input
is a terminal there is no document on the way, so **mdmost** prints its help and
exits rather than waiting; pass `-` to read from a terminal anyway. When standard
output is not a terminal, `--render-once` is implied, so `mdmost doc.md | cat`
writes plain text rather than escape sequences.

# OPTIONS

- **`--render-once`** — Render one frame to standard output and exit. No terminal is
  needed. The output is truecolour to a terminal and plain text anywhere else.

- **`--width N`** — Render the whole document at width *N* instead of the terminal's.

- **`--body-width N`** — Cap the prose body at *N* columns and centre it; `0` removes the
  cap.

- **`--no-body-width`** — Lay the body out at the full terminal width.

- **`--theme NAME`** — Start in theme *NAME*.

- **`--icons`** — Use Nerd Font glyphs even when none is detected.

- **`--no-icons`** — Use plain Unicode instead of Nerd Font glyphs, at the same display
  width.

- **`--mouse`** — Capture the mouse: the wheel scrolls, the scrollbar drags, a click in
  the contents pane jumps, and a drag over the document copies the Markdown source
  behind it.

- **`--toc`** — Start with the contents pane open.

- **`--config PATH`** — Read the configuration from *PATH* instead of the default file.

- **`--licenses`** — Print the licences of the bundled syntax definitions and exit.

- **`-h`, `--help`** — Print a usage summary and exit.

- **`-V`, `--version`** — Print the version and exit.

There is no `--color` flag. Truecolour is used when standard output is a terminal.

# KEYS

Bindings are remappable; see `[keys]` under **CONFIGURATION**. The help overlay
and the status bar name the bindings in effect rather than the defaults.

## Movement

- **`j`, `Down`** — Scroll down one line.

- **`k`, `Up`** — Scroll up one line.

- **`d`, `Ctrl-d`** — Scroll down half a screen.

- **`u`, `Ctrl-u`** — Scroll up half a screen.

- **`space`, `Ctrl-f`, `PgDn`** — Scroll down one screen.

- **`b`, `Ctrl-b`, `PgUp`** — Scroll up one screen.

- **`g`, `Home`** — Go to the top, and back to the left edge.

- **`G`, `End`** — Go to the bottom of the document.

- **`%`** — Jump N percent into the document, as in `50%`.

- **`Left`, `Right`** — Scroll content wider than the terminal, such as a wide table or a
  long code line. Neither is reflowed.

## Structure

- **`[`** — Go to the previous heading.

- **`]`** — Go to the next heading.

- **`=`, `Ctrl-g`** — Report the current position.

- **`Tab`** — Show or hide the contents pane.

- **`f`** — Move the keyboard cursor to the next link or button in the document,
  scrolling to bring it into view.

- **`F`** — Move the keyboard cursor to the previous link or button in the document,
  scrolling to bring it into view.

- **`Enter`** — Jump to the selected heading, or follow the link or button under the
  keyboard cursor. The status bar shows the full URL under the cursor, as it does for a
  mouse hover.

## Search

- **`/`** — Search forward.

- **`?`** — Search backward.

- **`n`, `Ctrl-Down`** — Go to the next match.

- **`N`, `Ctrl-Up`** — Go to the previous match.

- **`Ctrl-r`** — Switch between literal and regex search.

## Display and session

- **`t`** — Switch to the next theme, over the built-ins and any theme defined in the
  configuration file.

- **`-`** — Show or hide code line numbers.

- **`S`** — Save the current settings for the next run.

- **`h`, `F1`** — Show or hide the help overlay.

- **`Esc`** — Clear the search, or close the overlay or pane. `Esc` does not quit.

- **`q`** — Quit.

## Notes

A key that takes a count takes it as a prefix: `10j`, `50%`.

`Esc` unwinds one step per press. It clears a search, then a filter, then returns
focus from the contents pane, then closes the pane. It does not quit; `q` does.

`/` in the contents pane filters the headings fuzzily instead of searching the
document.

While a search is live, the status bar carries the query and the current match
number out of the total. With more than one match it also carries the keys that
step between matches, and the `Ctrl` alternatives beside them when the terminal
is wide enough. The current match is highlighted differently from the rest.
Reaching a match scrolls sideways as well as down, so a match inside a wide table
or a long code line is on screen on arrival.

`S` writes the settings that can be changed from inside the pager, which are the
theme, the line numbers, the contents pane and the body width, back to the
configuration file, and reports which file it wrote. It edits that file rather
than regenerating it, so comments, key order and any key a newer **mdmost**
understands survive the write. The previous file is kept as `config.toml.bak`. A
save whose result would not read back identically is refused.

# LINKS AND FOOTNOTES

With the mouse captured, a click on a link opens it in the browser, and a click
on a `#heading` reference scrolls that heading to the top of the viewport.

Both kinds are also reachable from the keyboard, with no `--mouse` needed. Press
`f` to move a keyboard cursor to the next link or button anywhere in the
document, `F` to move it to the previous one, and `Enter` to follow the one under
it. The cursor scrolls the document to bring its target
into view, in the way that `n` steps to the next search match. The status bar
shows the full URL under the cursor, as it does for a mouse hover. `Esc` puts the
cursor away.

Only `http` and `https` links open. Any other scheme is shown as plain text and
is inert.

Links are shown whether or not the mouse was captured. The `[copy]` buttons
described under **SELECTING AND COPYING** are not.

## Footnotes

A footnote marker opens the note in a box beside it. The document does not move.

The box is anchored to the marker and placed in whichever gap above or below the
marker has the room. On a screen too short for either gap it does not open. It is
at most 60 columns wide.

The note is drawn by the ordinary renderer at the width of the box, so emphasis,
code spans, nested lists and tables inside a footnote all render. The cursor keys
scroll the box while the document behind it holds still.

# SELECTING AND COPYING

With `--mouse` (or `mouse = true`) a left drag over the document selects, and the
release puts the **Markdown source** behind the selection on the clipboard rather
than the glyphs on screen. A drag over a rendered heading copies the `#` and its
text; over a bold word, `**bold**`; over a link, `[text](url)`; across a code
fence, the fence and its content verbatim. A drag across rows that were reflowed
copies the line breaks of the source. `Esc` clears the highlight.

A code frame and a table each carry a `[copy]` button at the right of their top
edge. It copies the **whole block**: the code exactly as written, and the table as
a grid of tab-separated cells that a spreadsheet splits into columns. An HTML
flavour is offered alongside where the local clipboard takes one. The label reads
`[copied]` for a moment and the status bar names what was sent.

The buttons have no key binding and no setting. They appear only when the mouse
was captured. They are dropped where they would not fit, and a table wider than
the terminal carries its button off to the right, where horizontal scrolling
reaches it.

Two points apply to what is copied.

A selection over a Mermaid diagram copies what is drawn rather than the source,
and the status bar reports `rendered text` instead of `Markdown source`. The
renderer records a source map for inline text and for code lines, not for box
art.

The copy goes out as OSC 52 first, which works over `ssh` but which the terminal
does not acknowledge. When that is the only route that ran, the status bar
reports `sent … (unconfirmed)` instead of `copied`. `tmux` needs
`set -g set-clipboard on` to pass it along, and `xterm` needs `allowWindowOps`.
On a local display server the `arboard` fallback runs as well and the report
becomes `copied`.

Capturing the mouse takes away the terminal's own drag-select for as long as
**mdmost** runs.

# CONFIGURATION

The configuration file is TOML, at *~/.config/mdmost/config.toml*, or in the
platform's own configuration directory where that differs. `--config PATH`
overrides it. See **FILES**.

A broken configuration file does not stop the program from starting. The problem
is reported and the rest of the file still applies, so one bad key binding costs
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
settings exist only in the file. `title_banner` is `false` unless set: with
`title_banner = true`, a document whose first block is its one and only `#`
heading is drawn with a FIGlet banner. `section_numbers` is `true` by default.
Both are described under **RENDERING**.

## Themes

A `[themes.<name>]` table inherits from `base`, which is `"dark"` or `"light"`,
so a custom theme can override a single colour rather than a full palette. The
overridable colours are `bg`, `surface`, `overlay`, `fg`, `muted`, `border`,
`accent`, `red`, `orange`, `yellow`, `green`, `cyan`, `blue` and `purple`.
`dark = true|false` tells the renderer which way the palette leans. `t` cycles
through the built-ins and any theme defined here.

## Icon settings

Three settings answer the same question. Each outranks the one before it:

- **`icons = true` / `false`** — Settles it for this machine.

- **`MDMOST_ICONS=1` / `0`** — Settles it for this shell. See **ENVIRONMENT**.

- **`--icons` / `--no-icons`** — Settles it for this run.

With none of the three set, **mdmost** detects whether to use icons. The
detection is described under **TERMINAL SETUP**.

# RENDERING

## Title banner

With `title_banner = true`, a document whose first block is its one and only `#`
heading opens with that title in the FIGlet *Small* font, wrapped between words
over as many lines as it needs and centred.

The setting is **off by default**. Even when it is on, an ordinary heading is
drawn instead when the title is not plain ASCII, or when a single word is too
wide for the measure to break.

## Section numbers

A document that nests three or more section levels gets section numbers, `1`,
`1.1` and `1.1.1`, in front of its headings and in the contents pane. They are
drawn in a grey of their own, so they are distinguishable from numbers the author
wrote.

A lone `#` title is not a section: it stays unnumbered, and its `##` headings are
numbered `1`, `2`, `3`. A document with fewer than three levels gets no numbers.
`section_numbers = false` turns them off.

Heading levels are also told apart by the rule underneath them: heavy, then
light, then dashed. Headings carry no marker in front of them.

## Syntax highlighting

Fenced code is highlighted from the syntax definitions curated by the `bat`
project. A little over two hundred languages are compiled into the binary, so
there is nothing to install and nothing to configure. The set includes TypeScript
and TSX, Kotlin, Swift, Zig, Nix, TOML, Dockerfile, Terraform/HCL, Elixir, Dart,
Julia, Protobuf, GraphQL, Vue, Svelte, Sass and SCSS, F#, CMake, Solidity, Nim,
x86-64 assembly, `.env` files, `go.mod`, `nginx.conf` and `.gitignore`.

**PowerShell** and **ARM assembly** are absent. They need regex features the
pure-Rust engine cannot compile, and the build uses that engine so that no C
toolchain is required. They render as plain text.

The fence tag is matched against every syntax name and every file extension, so
`rs`, `py`, `yml`, `sh`, `ts`, `tsx`, `kt`, `c++` and `hcl` all resolve. A short
table of aliases covers the rest, among them `golang`, `console`, `jsonc`,
`csharp`, `fsharp`, `objc` and `plaintext`. Only the first word of the info
string is read, so a fence tagged `rust,no_run` is highlighted as Rust. **An
unrecognised tag is not an error**: the block is drawn as plain themed text,
still framed and still with its label.

Colours never come from the syntax definitions. Each scope is mapped to a
semantic slot, which is one of keyword, string, number, comment, type, namespace
and escape, and the slot takes its colour from the active theme.

TOML and Dockerfile use definitions written for **mdmost** rather than the
bundled ones. The bundled TOML gives a `[table.header]` no scope at all, and the
bundled Dockerfile emits a whole `RUN` line as one span.

## Line length

Prose is capped at 72 columns by default and centred when the terminal is wider.
Set `body_width`, or `--body-width`, to change the cap, and `0` or
`--no-body-width` to switch it off. The default of 72 is inside the readable band
rather than at the top of it, so the cap also bites on an 80-column terminal.

The cap is about text that can be reflowed, so it does not apply to everything:

- **Tables and Mermaid diagrams ignore the cap** and are laid out at the full
  terminal width. Both stop at their natural width: a table does not stretch its
  columns to fill the room, and a diagram is drawn at the narrowest width that
  works. Wherever a block ends up, it is centred on the same axis as the prose;
  only a block as wide as the terminal starts at the margin.
- **Everything else takes the full width as soon as the cap would cut it short.**
  A short fenced snippet sits with the prose, and a block with a long line takes
  the whole terminal. The same applies to a wide table or fence nested inside a
  block quote or a list item.
- Content wider than the terminal itself is unaffected. It is laid out at the
  width it needs and reached with `Left` and `Right`.

`--width` is a different setting and does not replace this one. It changes the
width the whole document is rendered at, including tables and code.
`--body-width` caps the prose within that width.

## Mermaid

Fenced `mermaid` blocks are parsed and drawn as Unicode box art. All seven
families are supported. Anything outside the supported subset degrades to a
syntax-highlighted code block with a dim caption stating the reason.

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
  percentages, because a circle in character cells reads badly.

- **`gantt`** — `title`, `dateFormat`, `axisFormat`, `section`, tasks with `after X`,
  durations or explicit dates, and `done`/`active`/`crit`/`milestone` tags. The time
  scale is chosen from the available width.

Directives, `%%` comments and `%%{init}%%` blocks are parsed and ignored.

## Layout rules

**Layout is width-driven.** No layout decision is taken at parse time, so a
resize renders the document again rather than patching it. Table columns are
negotiated against the available width. A cell is itself a nested document, so
Markdown inside a table cell renders.

**A table whose rows wrap gets air between them.** While every row fits on one
line, the table is drawn dense. As soon as one row wraps, a blank line goes
between every pair of rows. The zebra stripe is carried through the gap by a half
block, so the shading still groups the lines of each row. The decision is made at
the width in use, so the same table is dense in a wide terminal and spaced in a
narrow one.

**Widths are display columns**, never bytes and never characters. Combining
marks, ZWJ emoji sequences and regional-indicator flags are never split, and
every row is padded to exactly the width of its pane.

**Wide content scrolls.** A table or code line too wide for the terminal keeps
its shape and is reached with `Left` and `Right`. A cut line is marked at the
edge, with one marker for content continuing to the right and another once the
view has scrolled, while a box's own rules close with the corner they belong to.

**List bullets and task boxes are ASCII.** The bullets are `*`, `>`, `+` and `-`,
one per nesting level, and the task boxes are `[ ]` and `[x]`. Both are ASCII
whether or not a Nerd Font is present.

**HTML and raster images are not rendered.** Raw HTML in the source is skipped
rather than rendered or shown. An image becomes a captioned placeholder carrying
its alt text and its target. There is no sixel support and no kitty graphics
protocol.

# TERMINAL SETUP

**mdmost** draws characters from the Unicode blocks below. The terminal font, or
a fallback behind it, has to cover them. Any font or font chain with that
coverage will do.

- **Box Drawing (U+2500-U+257F)** — Every table border, code frame and diagram box.

- **Block Elements (U+2580-U+259F)** — Zebra stripes, the scrollbar, gantt bars.

- **Geometric Shapes (U+25A0-U+25FF)** — Heading marks, diagram node shapes, arrowheads.

- **General Punctuation (U+2000-U+206F)** — The elision marker.

- **Mathematical Operators (U+2200-U+22FF)** — Class-diagram relations.

- **Misc Mathematical Symbols-A (U+27C0-U+27EF)** — Class-diagram generics.

- **Dingbats (U+2700-U+27BF)** — The marker on a degraded diagram's caption.

- **Latin-1 Supplement (U+0080-U+00FF)** — The scrollbar track, and whatever HTML
  entities the document decodes to.

- **Specials (U+FFF0-U+FFFF)** — The replacement character, drawn in place of one that
  cannot be represented.

- **Private Use Area (U+E000-U+F8FF)** — Code-fence language icons, drawn **only when
  icons are on**.

The Private Use Area row is the only optional one, and `--no-icons` removes it.
Every row above it is drawn regardless.

The interface adds one block that no document needs: **Arrows**
(U+2190-U+21FF), for the `Ctrl` key hints beside a live search, and for the
search indicator itself.

The text of the document is not on this list and cannot be. Covering the language
of the document is the font's business in any program.

## Symptoms of missing glyphs

A missing glyph is usually not a blank box. The terminal falls back to another
font, and the advance width of that font need not match the base font's. A line
made *entirely* of box characters then comes out at a different width from the
text lines around it, and the frame stops lining up. A table's rules overshoot
its contents, and a diagram's boxes shear.

**mdmost** cannot correct this from the inside. Every row it draws is padded to
exactly the width of the pane, measured in display columns. What the terminal
then does with those columns belongs to the font stack.

The same misalignment in a web browser has the same cause one layer up. GitHub
strips CSS from Markdown, so the font stack there is not settable either.

## Font fallback configuration

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

On macOS and Windows, fontconfig is not involved and the terminal decides. Set
the fallback in the terminal's own font settings. iTerm2 has a separate font for
non-ASCII text, Windows Terminal takes a font fallback list in its
*settings.json*, and Kitty and WezTerm both take an explicit fallback list in
their configuration files.

## A known-good font stack

These three fonts, consulted in this order, cover everything **mdmost** draws:

- **JetBrains Mono** — The text font.

- **Symbols Nerd Font** — The Private Use Area icons.

- **JuliaMono** — Unicode's symbol blocks: arrows, geometric shapes, dingbats and
  braille.

## Icon detection

A terminal cannot be asked which font it is using. **mdmost** therefore asks
fontconfig whether an installed font covers every glyph it would draw, and uses
icons only when one does.

It uses plain glyphs whenever it cannot establish that: when `fc-list` is
unavailable, when output is not going to a terminal, on `TERM=dumb` and on the
Linux console, and over `ssh`, where the fonts on the machine running **mdmost**
say nothing about the terminal drawing the pixels.

Plain and icon glyphs occupy **the same display width**, so nothing shifts and
nothing reflows either way, and no feature depends on icons. To settle the choice
by hand instead, see **CONFIGURATION**.

# DEFAULT MARKDOWN VIEWER

Two unrelated mechanisms can hand a Markdown file to **mdmost**. Which one
applies depends on where the file is clicked.

- **The desktop's file association** — What a file manager consults when *notes.md*
  is double-clicked. It is a registry belonging to the operating system, and on Linux
  it is an open standard that any program may register with.

- **The terminal's own link handling** — What runs when a `file://` link in terminal
  output is clicked. The terminal settles this itself, before the operating system is
  consulted, so it is configured once per terminal and behaves the same on Linux and
  on macOS.

On macOS only the second is available to **mdmost**. Launch Services binds a file
type to an *application bundle*, and a program that draws in a terminal has no
bundle to register. The macOS instructions below are therefore per terminal
rather than system-wide.

Neither mechanism makes **mdmost** the pager that other command-line programs
reach for. For that, see `PAGER` under **ENVIRONMENT**.

## Desktop file association on Linux

The registered media type for Markdown is `text/markdown`. Describe **mdmost** to
the desktop in *~/.local/share/applications/mdmost.desktop*:

```ini
[Desktop Entry]
Type=Application
Name=mdmost
Comment=Read a Markdown document in the terminal
Exec=mdmost %f
Terminal=true
MimeType=text/markdown;
Categories=Utility;TextEditor;ConsoleOnly;
```

Then refresh the lookup cache and claim the type:

```sh
update-desktop-database ~/.local/share/applications
xdg-mime default mdmost.desktop text/markdown
```

`xdg-mime query default text/markdown` then answers `mdmost.desktop`.

Three properties of that file matter:

- **`Terminal=true` is the weak part** — **mdmost** needs a terminal, so the desktop
  has to find a terminal emulator to run it in. Which one it settles on, and whether
  it settles on one at all, differs between desktops and versions. A double-click that
  appears to do nothing has this cause, and the per-terminal setup below is the more
  dependable answer.

- **`text/x-markdown` needs no line of its own** — The shared-mime-info database
  declares it an alias of `text/markdown`, so it resolves through it.

- **`Exec` is resolved on `PATH`** — A bare `mdmost` is correct here, and it is what a
  packaged copy of this file has to say.

The `.deb` and the `.rpm` install that entry to */usr/share/applications*, so on
those there is nothing to write: **mdmost** is offered for a Markdown file to
every user on the machine, and only the `xdg-mime` line above is left. Write the
file by hand after a Homebrew or tarball installation, or to register **mdmost**
for one account rather than all of them.

No package settles which application wins. A package declares that **mdmost**
*can* open Markdown; the `xdg-mime` line above sets the default. The freedesktop
specification reserves a system-wide *mimeapps.list* for the desktop
environment, so a package that shipped one would take the association away from
every editor on the machine.

## Terminal file:// link handling

A terminal turns `file:///path/to/notes.md` in its output into a link and hands
it to the operating system's opener when it is clicked. The terminals below can
be told to run something else instead, which keeps the whole business inside the
terminal: nothing to register, and the same configuration on both platforms.

Two points apply to all of them:

- **Name mdmost by absolute path** — These terminals run the program directly rather
  than through a shell, so it is looked up on the terminal's own `PATH`, and a terminal
  launched from the macOS Dock inherits a bare `PATH` with no Homebrew prefix in it.
  Use the output of `command -v mdmost`.

- **A link stops at whitespace** — The usual link patterns match runs of non-space
  characters, so a path holding a literal space is recognised only as far as the space.
  Percent-encode it as `%20`.

### WezTerm

WezTerm emits an `open-uri` event before it hands a link to the operating system,
and a handler that returns `false` suppresses that hand-off. In
*~/.config/wezterm/wezterm.lua*:

```lua
local wezterm = require 'wezterm'

local MDMOST = '/opt/homebrew/bin/mdmost'
local MARKDOWN = {
  md = true, markdown = true, mkd = true, mdown = true,
}

wezterm.on('open-uri', function(window, pane, uri)
  -- Split host from path, then drop any query or fragment. A
  -- '#' that belongs to the filename arrives percent-encoded,
  -- so a literal one here is a delimiter.
  local host, path = uri:match '^file://([^/]*)(/[^?#]*)'
  if not path or (host ~= '' and host ~= 'localhost') then
    return
  end
  path = path:gsub('%%(%x%x)', function(hex)
    return string.char(tonumber(hex, 16))
  end)
  local ext = path:match '%.(%a+)$'
  if not ext or not MARKDOWN[ext:lower()] then
    return
  end
  local dir = path:match '^(.*)/[^/]*$'
  if dir == nil or dir == '' then
    dir = '/'
  end
  window:perform_action(
    wezterm.action.SpawnCommandInNewWindow {
      args = { MDMOST, path },
      cwd = dir,
    },
    pane
  )
  return false
end)
```

Returning nothing for every other URI keeps `https` links going to the browser
and keeps a remote host's `file://` out of a pager that has no such file to open.
Do not use `wezterm.glob` to locate the binary: it is an async function, and
calling it from a required module raises *attempt to yield across a C-call
boundary*, which looks exactly like the binary not being installed.

### Kitty

Kitty reads *~/.config/kitty/open-actions.conf*, at that same path on Linux and
on macOS. A stanza is match lines followed by action lines:

```
protocol file
ext md,markdown,mkd,mdown
action launch --type=os-window /opt/homebrew/bin/mdmost ${FILE_PATH}
```

`${FILE_PATH}` arrives decoded and unquoted, so there is nothing to unescape. Add
`--title ${FILE}` to name the window after the document. Omit `--type=os-window`
for a new tab in the window that was clicked in.

### iTerm2

iTerm2 has no per-scheme hook. It has Semantic History, under **Settings ->
Profiles -> Advanced**: set it to **Run command...**, where `\1` stands for the
path. Semantic History is the coarser instrument. It fires on a Cmd-click over a
filename rather than over a `file://` link, and it fires for every file rather
than for Markdown alone, so the command has to be a script that inspects the
extension and hands anything else to an editor.

### Terminal.app

There is nothing to configure. Terminal.app passes every link it recognises to
Launch Services and offers no way to intervene.

# ENVIRONMENT

- **`MDMOST_ICONS`** — `1` or `0` forces Nerd Font glyphs on or off. It outranks the
  configuration file and is outranked by `--icons` and `--no-icons`. Export it in a
  profile on a server that is always reached from the same well-equipped terminal.

- **`PAGER`** — **mdmost** can serve as the pager for other programs:
  `export PAGER=mdmost`.

# FILES

- ***~/.config/mdmost/config.toml*** — Configuration, in TOML. A broken file does not
  stop the program from starting: the problem is reported and the rest of the file
  still applies, so one bad key binding costs that binding and nothing else. The
  platform's own configuration directory is used where it differs.

- ***config.toml.bak*** — The previous configuration, kept beside the file whenever `S`
  writes a new one.

# EXIT STATUS

- **`0`** — Success, including a quit from the pager and a broken pipe.

- **`1`** — The document could not be read, or the terminal could not be set up.

- **`2`** — The command line could not be parsed.

# SEE ALSO

`less`(1), `bat`(1)

# AUTHOR

Tobias Oetiker
