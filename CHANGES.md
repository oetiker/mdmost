# Changes

## Unreleased

### New

### Changed

### Fixed

## 0.1.0 - 2026-08-09

### New

- A full-screen terminal pager for a single Markdown document: styled Unicode
  rendering, real table borders with negotiated column widths, syntax-highlighted
  code fences, and Mermaid diagrams laid out as box art.
- Rendering is a pure function of `(document, width, theme, options)`, so a resize
  reflows everything.
- Mouse support behind `--mouse`: the wheel scrolls, the scrollbar drags, contents
  entries jump, and a drag copies the Markdown *source* behind the selection.
- Section numbering, a FIGlet title banner, a contents pane, literal and regex
  search, themes, and a configuration file at `~/.config/mdmost/config.toml`.
