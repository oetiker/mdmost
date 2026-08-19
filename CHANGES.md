# Changes

## Unreleased

### Breaking

No format for this existed in this file before now; entries here are API breaks a
`cargo publish` consumer of the library crate would feel, and are why this release is a
minor bump rather than a patch.

- `render_block_numbered` and `render_blocks` each gained a `source: &str` parameter, so
  that a formula which cannot be drawn can fall back to its own verbatim bytes, delimiters
  included (see New, below), wherever either function is the caller's entry point — the
  document body and the footnote popup. `render_block` and `render_table` keep their old
  signatures; neither has a call site in this binary, and a caller using either instead
  falls back to the formula's bare LaTeX with no surrounding `$…$`, since there is no
  whole-document source to slice the delimiters from.

### New

- `$E = mc^2$` reads as `E = mc²` on the line, wherever inline math appears in a
  document: a paragraph, a table cell, a list item, a footnote. Scripts are Unicode
  where a full raised or lowered form exists and written flat (`x^q`) where it does
  not, `\frac{a}{b}` reads `a/b`, `\sqrt{x}` reads `√x`, and a big operator such as
  `\sum` or `\int` carries its limits as a subscript and superscript on the one
  character. A `$$…$$` block or a ```` ```math ```` fence is display math; it is not
  laid out in this version and is shown as its own framed, syntax-highlighted source
  instead, the same as an unsupported Mermaid diagram. `\(…\)` and `\[…\]` are read as
  well behind `math_backslash`, off by default. Three configuration keys and their
  matching `--math`/`--no-math`, `--math-inline`/`--no-math-inline` and
  `--math-backslash`/`--no-math-backslash` flags control this; `math = false` parses
  `$` as ordinary text, exactly as before this existed.

### Fixed

- A Mermaid diagram's degraded-code caption is no longer corrupted where the
  line-number gutter's bottom-edge junction crosses it — "not a diagram type" no
  longer comes out "no┴ a diagram type". This shipped in v0.2.0 for every caption long
  enough to reach the junction column with line numbers on, and was invisible to the
  existing test because that test renders without line numbers, the one configuration
  the bug cannot appear in. A caption that collides with the junction is now
  re-ellipsized against the room left after the shift, rather than hard-truncated a
  second time.

## 0.2.0 - 2026-08-18

### New

- Code fences carry a language icon for 50 languages instead of 18. Perl, Lua, Swift,
  Kotlin, Scala, Haskell, Elixir, Erlang, Clojure, OCaml, Julia, R, Dart, Zig, Nim,
  Crystal, Groovy, Prolog, C#, F#, PowerShell, assembly, Vim script, XML, CSV, TeX,
  Make, Terraform, GraphQL, Vue and Svelte are now named, along with more spellings of
  the languages that were already there. An unnamed language still gets the generic
  code icon.

### Changed

- The scrollbar track is a column of dots rather than an unbroken line. The thumb moves
  in half cells, so its end lands mid-cell on every second scroll step; against a solid
  rule the line alternately met the thumb and stood a half cell clear of it, and the
  flicker at that join was more visible than the smoothness it came from.

### Fixed

- Bold, italic, struck-through and linked text inside a striped table row keeps the
  stripe. Those styles were defined as body text plus a mark, and body text names the
  page background, so each run repainted the band it stood on — a page-coloured box
  around the letters for exactly as long as the run. Inline code already avoided this;
  now no inline style carries a background at all. Bold link text also keeps the link
  colour instead of reverting to body ink.

- The release workflow builds its Intel bottle on `macos-15-intel`. It asked for
  `macos-13`, which GitHub retired in December 2025, and a retired image does not fail a
  job — it leaves it queued for the 24 hours GitHub waits before cancelling it. The
  bottle-publishing step waits on the whole matrix, so nothing was published at all:
  v0.1.2 shipped with an empty bottle block and no bottles on the release, despite the
  arm64 one having built successfully. Intel bottles now cover macOS 15 and later, which
  is as far back as the last Intel runner image reaches.

## 0.1.2 - 2026-08-18

### New

- Releases now ship Homebrew bottles for macOS. Without one, Homebrew treats the
  formula as a source build even though nothing in it is compiled, and refuses to
  install on a Mac whose Command Line Tools are older than its macOS. One bottle per
  architecture covers later macOS releases too.

### Changed

- The Homebrew instructions now include `brew trust --formula oetiker/mdmost/mdmost`.
  Homebrew 6.0 stopped loading formulae from third-party taps until they are trusted, so
  `brew install mdmost` alone no longer finds the formula.

### Fixed

- `mdmost` with no arguments at a terminal prints its help and exits instead of hanging.
  It read standard input whenever no file was named, so bare `mdmost` sat waiting on a
  terminal nobody was typing into, which is indistinguishable from a hang. `mdmost -`
  still reads standard input on purpose, and `cat x.md | mdmost` is unchanged.
- The release no longer tags a commit whose `Cargo.toml` and `Cargo.lock` disagree
  about the version, which is what stopped `v0.1.1` from reaching crates.io. The
  version bump refreshed the lock with `cargo update --offline`, but that job never
  fetches the registry, so the resolve failed on the first dependency it looked up —
  and a trailing `|| true` swallowed it. `cargo publish` then re-resolved, rewrote
  `Cargo.lock` itself, and refused to publish a dirty tree. The lock update now runs
  online, is not allowed to fail quietly, and is checked before the commit is made;
  `cargo publish --locked` names the lock if it is ever stale again. `Cargo.lock` is
  brought up to `0.1.1` here, so the tree is consistent again.

## 0.1.1 - 2026-08-17

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
- Configuration fragments in `integrations/` that make **mdmost** the thing a click
  opens a Markdown file with: a WezTerm Lua module, a kitty `open-actions.conf`
  stanza, and an XDG desktop entry. The terminal fragments hook the terminal's own
  link handling rather than the operating system's file association, so they need
  nothing registered and work the same on Linux and macOS — which is the only route
  open on macOS, where Launch Services binds a file type to an application bundle
  and a terminal program has none. The `.deb` and the `.rpm` install the desktop
  entry to `/usr/share/applications`, so a system-wide install offers **mdmost** for a
  Markdown file to every user on the machine — declaring what it can open, while
  leaving which application wins to `xdg-mime`. The terminal fragments cannot be
  installed that way, because neither WezTerm nor kitty has a drop-in directory and
  each reads one file owned by the user, so those ship as examples in
  `/usr/share/doc/mdmost/examples/`. Homebrew keeps all three under `pkgshare`: its
  prefix is not on a desktop session's `XDG_DATA_DIRS`, so a desktop entry installed
  there would never be found.

### Changed

- The documentation is one manual with three faces. `docs/manual.md` is the single
  source for every key, option and config field; `man/mdmost.1` is generated from
  it by `make man` and is no longer kept in git; the README is a 30-second pitch
  that links to the rest. A new terminal-setup section says which Unicode blocks
  your font has to cover, and a test keeps that list matching the renderer. A
  **DEFAULT MARKDOWN VIEWER** section covers the two separate mechanisms that can
  hand a file to **mdmost** — the desktop's file association and the terminal's own
  link handling — with the configuration for each.
- The licence file is named `LICENSE` rather than `LICENSE-MIT`, which is the name
  forges, packagers and licence scanners look for. Both packages now install it to
  `/usr/share/doc/mdmost/LICENSE`.

### Fixed
