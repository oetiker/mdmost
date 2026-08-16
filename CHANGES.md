# Changes

## Unreleased

### New

### Changed

### Fixed

## 0.1.0 - 2026-08-16

### New

- A full-screen terminal pager for a single Markdown document: styled Unicode
  rendering, real table borders with negotiated column widths, syntax-highlighted
  code fences, and Mermaid diagrams laid out as box art.
- Rendering is a pure function of `(document, width, theme, options)`, so a resize
  reflows everything.
- Section numbering, a FIGlet title banner, a contents pane, literal and regex
  search, themes, and a configuration file at `~/.config/mdmost/config.toml`.
- Mouse support behind `--mouse`: the wheel scrolls, the scrollbar drags, and
  contents entries jump.
- A selection is a range over the document rather than a rectangle of cells. It hugs
  the text, opens and closes mid-line, survives a reflow because it is anchored in
  source offsets, and reaches inside a code fence and a table cell. A drag copies the
  Markdown *source* behind it.
- Code blocks and tables carry a `[copy]` button. A table arrives as tab-separated
  values, and as HTML where the clipboard carries flavours; a code block arrives
  without its fences. The status bar names which of the four it gave you.
- Links react and can be followed. Hovering lights the whole control however many rows
  it wrapped across, and the status bar names the host before you commit to it. A
  click opens `http` and `https` in your browser; every other scheme is inert. A
  `#fragment` scrolls to the heading it names, folded through the same slug rule
  headings use.
- A footnote marker opens the note in a box beside it, scrolled with the cursor keys
  without moving the page.
- A keyboard cursor reaches every control without a mouse: `f` and `F` walk the
  document's links, buttons and footnote markers, scrolling each into view, and
  `enter` follows the one it stopped on.
- Prebuilt binaries for Linux (static musl, x86_64 and aarch64), macOS (Intel and
  Apple silicon) and Windows, with `.deb` and `.rpm` packages, a Homebrew tap in this
  repository, and publication to crates.io.

### Changed

- The documentation is one manual with three faces. `docs/manual.md` is the single
  source for every key, option and config field; `man/mdmost.1` is generated from
  it by `make man` and is no longer kept in git; the README is a 30-second pitch
  that links to the rest. A new terminal-setup section says which Unicode blocks
  your font has to cover, and a test keeps that list matching the renderer.
- The licence file is named `LICENSE` rather than `LICENSE-MIT`, which is the name
  forges, packagers and licence scanners look for. Both packages now install it to
  `/usr/share/doc/mdmost/LICENSE`.

### Fixed
