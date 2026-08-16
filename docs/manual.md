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

`--render-once`

:   Render one frame to standard output and exit. Needs no terminal. Truecolour
    goes to a terminal and plain text goes anywhere else, which is what makes it
    usable for scripting and snapshotting.

`--width N`

:   Render the whole document at this width instead of the terminal's.

`--body-width N`

:   Cap the prose body at N columns and centre it; `0` for no cap.

`--no-body-width`

:   Let the body use the full terminal width.

`--theme NAME`

:   The theme to start in.

`--icons`

:   Use Nerd Font glyphs even if none appears to be installed.

`--no-icons`

:   Use plain Unicode instead of Nerd Font glyphs, at the same display width.

`--mouse`

:   Capture the mouse: the wheel scrolls, the scrollbar drags, clicks jump in the
    contents pane, and dragging over the document copies the Markdown source
    behind it.

`--toc`

:   Start with the table-of-contents pane open.

`--config PATH`

:   Read configuration from this file instead of the default.

`--licenses`

:   Print the licences of the bundled syntax definitions and exit.

`-h`, `--help`

:   Print help and exit.

`-V`, `--version`

:   Print the version and exit.

There is no `--color` flag. The truecolour decision is made from whether standard
output is a terminal, which is the same question `--render-once` already answers.

# KEYS

Bindings are remappable; see `[keys]` under **CONFIGURATION**. The in-app help
overlay is generated from the same live binding table as this list, so the two
cannot drift apart, and the status bar always names the keys you have actually
bound rather than the defaults.

## Moving

`j`, `Down`

:   Scroll down one line.

`k`, `Up`

:   Scroll up one line.

`d`, `Ctrl-d`

:   Scroll down half a screen.

`u`, `Ctrl-u`

:   Scroll up half a screen.

`space`, `Ctrl-f`, `PgDn`

:   Scroll down one screen.

`b`, `Ctrl-b`, `PgUp`

:   Scroll up one screen.

`g`, `Home`

:   Go to the top, and back to the left edge.

`G`, `End`

:   Go to the bottom of the document.

`%`

:   Jump N percent into the document, as in `50%`.

`Left`, `Right`

:   Scroll content that is wider than the terminal, such as a wide table or a
    long code line. Neither is ever reflowed or mangled to fit.

## Structure

`[`

:   Go to the previous heading.

`]`

:   Go to the next heading.

`=`, `Ctrl-g`

:   Report where you are.

`Tab`

:   Show or hide the table of contents.

`f`

:   Move the keyboard cursor to the next link or button in the document,
    scrolling to bring it into view.

`F`

:   Move the keyboard cursor to the previous link or button in the document,
    scrolling to bring it into view.

`Enter`

:   Jump to the selected heading, or follow the link or button under the keyboard
    cursor. The status bar shows the full URL under the cursor, the same as it
    does for a mouse hover, so `Enter` never sends you somewhere unseen.

## Searching

`/`

:   Search forward.

`?`

:   Search backward.

`n`, `Ctrl-Down`

:   Go to the next match.

`N`, `Ctrl-Up`

:   Go to the previous match.

`Ctrl-r`

:   Switch between literal and regex search.

## Other

`t`

:   Switch to the next theme. This cycles through the built-ins and anything you
    have defined.

`-`

:   Show or hide code line numbers.

`S`

:   Save the current settings for next time.

`h`, `F1`

:   Show or hide the help overlay.

`Esc`

:   Clear the search, or close the overlay or pane. It never quits.

`q`

:   Quit.

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

# ENVIRONMENT

`MDMOST_ICONS`

:   `1` or `0` forces Nerd Font glyphs on or off. It outranks the configuration
    file and is outranked by `--icons` and `--no-icons`. Exporting it in a
    profile is the natural thing to do on a server you always reach from the same
    well-equipped terminal.

`PAGER`

:   **mdmost** is usable as a pager; `export PAGER=mdmost` is the intended use.

# FILES

*~/.config/mdmost/config.toml*

:   Configuration, in TOML. A broken file never stops the program from starting:
    the problem is reported and the rest of the file still applies, so one bad
    key binding costs you that binding and nothing else. The platform's own
    configuration directory is used where it differs.

*config.toml.bak*

:   The previous configuration, kept beside it whenever `S` writes a new one.

# EXIT STATUS

`0`

:   Success, including a quit from the pager and a broken pipe.

`1`

:   The document could not be read, or the terminal could not be set up.

`2`

:   The command line could not be parsed.

# SEE ALSO

`less`(1), `bat`(1)

# AUTHOR

Tobias Oetiker
