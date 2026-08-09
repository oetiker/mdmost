# mdless — design spec

Date: 2026-08-08
Status: approved

## 1. What it is

`mdless` is a full-screen terminal pager for a single Markdown document. It aims to be
as pleasant to look at as `btop` and as pleasant to use as `less`. It renders GitHub
Flavored Markdown including tables (with Markdown inside cells), syntax-highlighted
fenced code, and a useful subset of Mermaid diagrams drawn as Unicode art. It reflows
on terminal resize and offers a table-of-contents pane for navigation.

Non-goals: editing, a file-tree browser, following links to other documents, HTML
rendering, inline raster images, remote fetching.

## 2. Decisions (fixed)

| Decision | Choice |
|---|---|
| Language / TUI | Rust + `ratatui` + `crossterm` |
| Scope | Single-document pager; `mdless FILE` and stdin; usable as `$PAGER` |
| Mermaid | Unicode/box-art only, never raster |
| Mermaid families | flowchart, sequence, class, er, pie, gantt, state |
| Terminal floor | Truecolor + full Unicode; Nerd Font glyphs when detected, plain Unicode otherwise (§2.1) |
| Keys | less-compatible core, vim extras |
| Config | TOML at `~/.config/mdless/config.toml`, themeable |
| Images | Framed placeholder box showing alt text + target |
| HTML | Not supported. Not rendered, not passed through. |

### 2.1 Nerd Font glyphs are detected, not assumed

**Decision changed 2026-08-09 by the user.** This spec previously recorded "Nerd Font
glyphs are the default, `--no-icons` is the escape hatch" as settled, and it was settled
— explicitly, in the original design brief. It is being changed on the record here so
that the record is what a later reader finds, rather than the superseded version.

*What was wrong with it.* No terminal can be asked what font it is using. Assuming a
patched font means every first run on an unpatched terminal shows replacement boxes where
heading markers and task boxes should be, and the reader has to already know that
`--no-icons` exists to fix a screen that gives no hint of it. Two independent reviewers
raised this against the previous default; a third flipped it unilaterally and was
overruled on the grounds that the decision was already settled. That was the right call
procedurally and the wrong outcome, which is why this section exists.

*What replaces it.* When nobody has said, mdless asks whether an installed font covers
**every** private-use code point it can draw, and uses glyphs only if one does. The
question goes to fontconfig, and the code points are enumerated from the glyph tables
themselves, so a glyph added later is automatically a glyph the probe requires — the
detection list cannot drift from the drawing code because there is no second list.

*The asymmetry that decides every unclear case.* Falling back to plain on a machine that
could have drawn glyphs costs a little elegance. Drawing glyphs on a machine that cannot
costs a screen full of tofu, and — because the chrome is laid out by measuring glyphs
that are all supposed to be one column wide — possibly a misaligned status bar too. So
**detection answers "yes" only on positive evidence**, and treats "cannot tell" as "no":
no fontconfig, no terminal on stdout, `TERM=dumb` or `linux`, or an SSH session, where
the fonts on this machine describe the wrong computer entirely.

*Precedence*, lowest to highest: detection, `icons` in the config file, `MDLESS_ICONS`,
then `--icons` / `--no-icons`. The nearer answer wins outright rather than combining, so
`--no-icons` turns glyphs off for one run of a config that enables them. The env var
exists mainly for the SSH case, where detection is blind by design and the fix belongs in
the server's shell profile.

*Unchanged:* the two glyph sets remain the same shape and the same display width, so
which one is in force never affects layout (§9).

## 3. The central architectural rule

> Rendering is a pure function of `(AST, width, theme)`.

The document is parsed to an AST exactly once. **No layout decision may be taken at
parse time.** Every visual artifact — line breaks, table column widths, diagram
geometry, code-block framing — is computed by the renderer from the AST at the current
width. A terminal resize is handled by discarding the rendered output and calling the
renderer again with the new width.

Consequences that are binding on every module:

- No renderer may retain state between calls that depends on a previous width.
- Rendering is recursive over a *width budget*: a block renderer receives the width it
  is allowed to occupy and returns a `Canvas`. A table cell is a nested document
  rendered at its column's width budget. This is what makes Markdown-inside-tables
  work, and it must not be special-cased.
- Rendered output is cached keyed by `(document version, width, theme)`. Cache is a
  performance detail, never a correctness one; dropping the cache must change nothing
  visible.

### 3.1 Render options

Two user-facing settings are capabilities rather than colours, so they travel beside
the theme rather than inside it:

```rust
pub struct RenderOptions { pub icons: bool, pub line_numbers: bool }
pub fn render_document(doc: &Doc, width: u16, theme: &Theme, options: &RenderOptions) -> Canvas
```

`icons: false` — from `--no-icons`, from `icons = false`, or from detection declining to
promise a Nerd Font (§2.1) — substitutes plain Unicode for every Nerd Font glyph, at the
same display width so no layout shifts. Note that `RenderOptions::icons` is a plain
`bool`: by the time it is built the question has been answered, and no renderer ever has
to know how. `line_numbers` adds a themed gutter to
fenced code blocks, outside the horizontally scrollable region. Options are threaded
through every recursive render call, table cells included, and form part of the render
cache key alongside document version, width and theme.

### 3.2 The body width cap

**Added 2026-08-09 at the owner's request:** *"max body width should be configurable
independent of terminal width. Indenting left and right if the terminal is wider, with
special dispensation for tables and (pre)formatted content which would get full terminal
width."*

Prose past roughly a hundred columns is hard to read — the eye loses the start of the
next line on its way back from the end of this one — so the body has a cap, `body_width`
in the configuration file and `--body-width` on the command line, defaulting to **100
columns**. A hundred rather than eighty: the cost of a default is what it changes for
people who did not ask for it, and at 100 the cap does nothing whatsoever on a terminal
of 102 columns or fewer, which is the 80- and 100-column terminals almost everyone
reads in. It only bites on the wide terminals whose full-width prose is the complaint.
`0` (or `--no-body-width`) switches it off.

`--width` is a *different* setting and the two must not be confused: `--width` changes
the width the whole document is rendered at, tables and code included, and its surplus is
reached by scrolling. `--body-width` caps only the prose *within* whatever that width is.

**Where it lives.** The rule is applied in `tui::wide::render_scrollable` and nowhere
else, because that is the only renderer that assembles the document block by block and so
the only one that can give one block a different width than its neighbour. It is
therefore a property of the pager, not of `render_document`: `--render-once` produces a
full-width dump, on the grounds that its width was already chosen explicitly by the
caller (a `--width`, a terminal, or the 80-column fallback) and that its output is data
for a pipe or a snapshot rather than something anybody reads at 200 columns. It is
deliberately *not* a field of `RenderOptions`, which would put a setting `render_document`
ignores into the type every renderer is handed.

**Which blocks are exempt.** The dispensation is for content that cannot be reflowed;
content that cannot be reflowed is not made readable by being given less room, only
narrower and then cut.

| Block | Laid out at | Why |
|---|---|---|
| Table | the full body width, always | Squeezing a table wraps every cell. It costs nothing to exempt it: `distribute` returns the natural widths when they fit rather than padding columns out to the budget. |
| Mermaid fence | the full body width, always | A diagram is a figure, not prose, and `render::diagram` already answers with the *narrowest* width that draws, so it takes only what it needs. |
| Fenced/indented code | the cap, escalating to the full body width the moment a line would be cut | Its frame fills whatever budget it is given, so an outright exemption would blow every three-line snippet out to the terminal edge. Code is only *mangled* when it is cut, and being cut is exactly the escalation trigger. |
| Block quote, list, paragraph, heading, image placeholder | the cap, with the same escalation | Prose. A wide table or fence *nested* inside one still reaches the full width through the escalation, so nothing nested becomes less reachable than it was. |

HTML is not rendered at all (§2), so it has no width to speak of.

**Placement.** "Indenting left and right" is read as centring, and everything shares one
centre line. A block laid out at the cap takes the cap's own indent whatever it happens
to draw — centring a two-word paragraph or a short heading on *itself* would set the
document ragged — and a block that took the full width is centred on what it actually
drew. The two are the same arithmetic and agree exactly at an extent of one cap, so a
table slightly wider than the measure sits slightly wider than the prose rather than
jumping to the left margin. A block that fills the body, or overflows it, stays at the
margin. Left-aligning the exempt blocks at the margin instead was built first and looked
at in tmux at 200 columns: a 125-column table pinned to the left rail under centred prose
reads as a layout mistake, and the common centre line reads as a decision.

**Interaction with horizontal scrolling.** Nothing past the full body width changes: a
block that still does not fit is widened and reached with `←`/`→` exactly as before.
`scroll_reach` and `pinned_prefix` both measure the canvas *as drawn*, so a centred block
simply has a larger left extent; capped rows stay well inside the render width and
therefore keep an offset of zero, which is what stops one wide table dragging the prose
sideways. One thing this depends on: a block placed with the prose is **cropped to the
cap before it is indented**, because a canvas padded out to the full width and then
indented would push every row's extent past the render width and hand the whole document
to `scroll_reach` as a single over-wide run.

## 4. The `Canvas` contract

`Canvas` is the single currency between renderers and the viewport.

```rust
pub struct Cell { pub ch: char, pub style: Style, pub width: u8 } // width: 0,1,2
pub struct Canvas {
    pub width: u16,
    pub rows: Vec<Vec<Cell>>,
    pub anchors: Vec<Anchor>,   // heading ids -> row, for TOC jumps
    pub spans: Vec<SearchSpan>, // source-text offsets -> (row, col), for search
}
```

Rules:

- A `Canvas` is exactly `width` columns wide; renderers pad. Double-width characters
  (CJK, emoji) occupy one `Cell` with `width == 2` followed by one with `width == 0`.
- Every renderer — block, table, code, mermaid, image placeholder — returns a `Canvas`.
  The viewport does nothing but blit a vertical slice of the document `Canvas`.
- Because `Canvas` is the only shared type, modules can be built and reviewed
  independently.

Width measurement uses `unicode-width` throughout. Grapheme clusters (combining marks,
ZWJ emoji sequences, flags) are never split; wrapping operates on grapheme clusters via
`unicode-segmentation`.

## 5. Module map

Each module has one purpose, a narrow interface, and its own tests.

| Module | Responsibility | Public interface |
|---|---|---|
| `doc` | Owned AST: parse Markdown once, mark HTML nodes as skipped, assign heading ids and source offsets | `Doc::parse(&str) -> Doc` |
| `render::inline` | Inline spans → styled runs; grapheme-safe, width-aware wrapping | `wrap(&[Span], width) -> Vec<Line>` |
| `render::block` | Headings, paragraphs, lists, block quotes, rules, code blocks, image placeholders | `render_block(&Node, width, &Theme) -> Canvas` |
| `render::table` | Column-width negotiation, then recursive per-cell document render, then border drawing | `render_table(&Table, width, &Theme) -> Canvas` |
| `highlight` | Fenced code → styled lines, language detection, graceful unknown-language fallback | `highlight(lang, src, &Theme) -> Vec<Line>` |
| `mermaid::parse` | Mermaid source → typed `Diagram` enum; unknown syntax → recoverable error | `parse(&str) -> Result<Diagram, MermaidError>` |
| `mermaid::layout` | `Diagram` → `Canvas`; one trait, one impl per family | `trait DrawDiagram { fn draw(&self, width, &Theme) -> Canvas }` |
| `toc` | Heading tree, current-position tracking, fuzzy filter | `Toc::from(&Doc)` |
| `search` | Case-smart substring/regex search over source text, mapped to canvas positions | `Search::find(&Doc, &str)` |
| `config` | Load/merge TOML config, themes, key bindings | `Config::load() -> Config` |
| `theme` | Palette + semantic style lookup; built-in themes | `Theme::builtin(name)` |
| `tui` | ratatui app: panes, scroll, key dispatch, help overlay, status bar | `App::run(Doc, Config)` |
| `cli` | Argument parsing, stdin handling, `--render-once` dump mode | `main` |

A module that grows past roughly 400 lines is a signal to split it.

## 6. Mermaid subset — acceptance criteria

All families render as Unicode box art. Anything outside the stated subset must fail
*gracefully*: the block falls back to a syntax-highlighted code block with a dim
"unsupported mermaid syntax: <reason>" caption. A panic or a garbled canvas is a bug.

Directives, comments (`%%`), and `%%{init}%%` blocks are parsed and ignored.

### 6.1 flowchart / graph — first-class
- Directions `TD`, `TB`, `LR`, `RL`, `BT`.
- Node shapes: `[rect]`, `(round)`, `([stadium])`, `{rhombus}`, `((circle))`,
  `[[subroutine]]`, `[(cylinder)]`. Unsupported shapes degrade to rect.
- Edges: `-->`, `---`, `-.->`, `==>`, with `|label|` and `-- label -->` forms.
- `subgraph` … `end`, including nesting.
- Layout: layered (Sugiyama-style) — cycle breaking, layer assignment by longest path,
  crossing reduction by median heuristic, coordinate assignment on a character grid.
  Edges routed orthogonally with proper junction glyphs (`├ ┤ ┬ ┴ ┼`) and arrowheads.
- Out of scope: `click`, `style`/`classDef` colors, `linkStyle`.

### 6.2 sequenceDiagram — first-class
- `participant`/`actor` with `as` aliases; implicit participants from first use.
- Arrows: `->`, `-->`, `->>`, `-->>`, `-x`, `--x`, self-messages.
- `activate`/`deactivate` and `+`/`-` activation shorthand → activation bars.
- `Note left of|right of|over`.
- Blocks: `loop`, `alt`/`else`, `opt`, `par`/`and`, `critical`, with labels, drawn as
  labelled frames.
- Out of scope: `autonumber`, `box`, `link`, `rect` background regions.

### 6.3 classDiagram — reuses flowchart layout
- `class X { +field: T; +method(a) T }` with visibility markers `+ - # ~`,
  `<<interface>>`/`<<abstract>>` annotations.
- Relations: inheritance `<|--`, composition `*--`, aggregation `o--`, association
  `-->`, dependency `..>`, realization `..|>`, with cardinality labels `"1" -- "0..*"`.
- Node renderer draws the three-compartment class box; edge routing is the flowchart
  engine with mermaid-accurate arrow terminators.

### 6.4 erDiagram — reuses flowchart layout
- `ENTITY { type name PK "comment" }` attribute blocks.
- Crow's-foot cardinality `||--o{`, `}o--||`, `||--||`, `}|..|{` etc., with the
  relationship label. Terminators drawn as crow's-foot / bar / circle glyphs.

### 6.5 pie
- `pie title X` / `showData`, `"label" : value`. Rendered as a sorted horizontal bar
  chart with percentages and a legend (a circle in character cells reads badly; bars
  are the honest choice). Sub-cell precision via eighth-block glyphs.

### 6.6 gantt
- `title`, `dateFormat`, `axisFormat`, `section`, tasks with `id, after X, 3d` /
  explicit dates, `done`/`active`/`crit`/`milestone` tags.
- Rendered as a time axis with per-section bar rows; milestones as diamonds. Time scale
  is chosen from the available width.

### 6.7 stateDiagram-v2
- `[*] --> S`, `S --> T : label`, `state X { … }` composite states, `<<choice>>`,
  `<<fork>>`/`<<join>>`, `note left of`.
- Uses the flowchart layout engine with rounded state boxes and start/end markers.

## 7. Tables

1. Measure each cell's minimum width (longest unbreakable grapheme run) and natural
   width (unwrapped).
2. Distribute available width: satisfy minimums first, then grow columns toward natural
   width proportionally, then distribute slack.
3. If minimums exceed the terminal width, the table becomes horizontally scrollable
   rather than mangled; the status bar shows the horizontal offset.
4. Render each cell by recursing into the block renderer at the negotiated column width.
   Cells therefore support emphasis, code, links, lists, and even nested tables.
5. Draw borders with rounded box-drawing glyphs; honour GFM per-column alignment.
6. Row height is the tallest rendered cell; shorter cells are vertically top-aligned.

## 8. Syntax highlighting

- Fenced code blocks are highlighted by language tag. Unknown or absent tag → plain,
  themed, still framed.
- Implementation uses `syntect` with its default syntax set, themes mapped from the
  active `mdless` theme rather than syntect's own, so code sits inside the palette
  instead of clashing with it.
- Code blocks are framed with the language name in the frame's top edge, line numbers
  optional via config, and never wrap: long lines scroll horizontally with the table
  mechanism.
- A `mermaid` fence is routed to the mermaid renderer, not the highlighter.

## 9. Look and feel

- Signature dark theme plus a light theme built in; further themes definable in TOML as
  `[themes.<name>]` with `base = "dark" | "light"`, an optional `dark = bool`, and
  per-slot palette colour overrides — so a custom theme can be a two-line tweak rather
  than a full fifteen-colour palette. Config themes derive their semantic styles through
  `Theme::from_palette`, the same single implementation the built-ins use, so they
  cannot drift.
- Nerd Font glyphs for heading bullets, list markers, code-fence language icons, and
  the status bar, used when a Nerd Font is detected (§2.1). `--no-icons` and
  `icons = false` substitute plain Unicode of the same display width, as does detection
  failing or being unable to tell.
- Headings are visually distinct by level (colour, weight, prefix glyph, rules under
  H1/H2), not merely by size-that-doesn't-exist.
- Status bar: file name, position percentage with a fine-grained scrollbar, current
  heading, search state, key hint.
- TOC pane toggled with `Tab`, docked left, showing the heading tree with the current
  section highlighted; `/` inside the TOC filters fuzzily; `Enter` jumps.
- Smooth, quiet transitions; no flashing on resize.

## 10. Keys

| Key | Action |
|---|---|
| `j` `k` `↓` `↑` | line down/up |
| `d` `u` `PgDn` `PgUp` `Space` `b` | half/full page |
| `g` `G` `Home` `End` | top / bottom |
| `/` `?` | search forward / backward |
| `n` `N` | next / previous match |
| `Tab` | toggle TOC pane |
| `Enter` | (in TOC) jump to heading |
| `t` | cycle theme |
| `[` `]` | previous / next heading |
| `←` `→` | scroll horizontally (wide tables, long code lines) |
| `%` | jump to a percentage of the document |
| `=` `Ctrl-G` | report where you are |
| `-` | toggle code line numbers |
| `S` | save the current settings to the configuration file (§12.1) |
| `Ctrl-R` | switch literal / regex search |
| `Ctrl-D` `Ctrl-U` `Ctrl-F` `Ctrl-B` | movement variants of `d` `u` `Space` `b` |
| `h` `F1` | help overlay |
| `q` | quit, unconditionally |
| `Esc` | cancel, never quit: unwinds count → search → TOC filter → TOC focus → TOC pane, then says so |
| mouse wheel | scroll; click in TOC jumps |

`Esc` never exits. It is the key people press when unsure, and losing your place in a
long document with no undo is a hostile answer to uncertainty; when there is nothing left
to cancel it says `nothing to cancel — press q to quit`. Quitting is `q` alone.

Counts prefix motions (`10j`), and the whole table is remappable from config. The help
overlay and the README key map are both generated from this same binding table, so the
three cannot drift.

When the content is scrolled horizontally the status bar shows a horizontal offset
indicator, so a reader who bumps `→` can see why the text moved.

Bindings are remappable in config; the help overlay is generated from the live binding
table so it can never drift.

## 11. CLI and I/O

```
mdless [FILE]              # file, or stdin when FILE is absent or "-"
  --render-once            # render one frame to stdout and exit (no TTY needed)
  --width N                # force render width in BOTH modes; in the TUI the surplus
                           # is reachable by horizontal scrolling
  --body-width N           # cap the prose body at N columns and centre it (§3.2);
                           # 0 means no cap. Distinct from --width, and TUI-only
  --no-body-width          # the same as --body-width 0
  --theme NAME
  --icons / --no-icons     # Nerd Font glyphs on or off; detected when unset, per §2.1
  --mouse / --no-mouse     # mouse capture; off leaves native drag-select working
  --toc                    # start with TOC pane open
  --config PATH
```

- When input is stdin, the process reopens `/dev/tty` for keyboard input so
  `cat x.md | mdless` and `export PAGER=mdless` both work.
- When stdout is not a TTY, `--render-once` behaviour is implied so `mdless x.md | cat`
  produces sensible output instead of escape soup. `--render-once` emits ANSI truecolour
  to a TTY and plain text otherwise, which is also what makes headless snapshotting
  trivial. There is no `--color` flag.
- Exit codes: 0 success, 1 unreadable input, 2 bad arguments.

## 12. Error handling

- Malformed Markdown is not an error; CommonMark always parses.
- Malformed Mermaid degrades to a captioned code block (§6).
- Unreadable file → clean stderr message, exit 1, no partial TUI.
- Panics are caught at the top level and the terminal is always restored (raw mode off,
  alternate screen left) before the message is printed. A panic that leaves a wrecked
  terminal is treated as a release blocker.
- Config errors report file, line, and the offending key, then fall back to defaults
  rather than refusing to start.

### 12.1 Saving settings

**Added 2026-08-09 at the owner's request:** *"settings should be storeable in a config
file on request so that the next session comes up with the settings."*

`S` (action `save_config`) writes the settings the reader can change — theme, icons when
they were stated rather than detected, line numbers, mouse, scroll step, body width, and
the contents pane's state and width — back to the configuration file: `--config PATH`
when one was named, otherwise `Config::default_path()`, created along with its
directories. The key table and `[themes.*]` are never written; nothing in the pager
changes them. The outcome always reaches the status bar, naming the file.

It is a binding rather than a CLI flag because the settings worth saving are the ones
that were changed at run time, and because a binding is picked up by the help overlay and
the README key map automatically (§10). Being data-driven, it is rebindable like any
other.

**Not autosave, and not destructive.** The pager cannot ask "overwrite?" — there is no
such level of interaction — so the protection has to be structural rather than a prompt:

1. The file is **edited, not regenerated**. Lines the writer has no opinion about are
   copied through byte for byte, trailing comments on the lines it does have an opinion
   about included; a setting with no line yet is inserted after its section's last real
   content, so a comment block introducing the *next* section keeps introducing it.
   Comments, ordering, `[keys]`, `[themes.*]` and keys from a newer version — which the
   loader keeps with a warning rather than discarding the file — all survive.
2. The writer **checks its own work before touching the disk**: it parses the text it is
   about to write with the ordinary loader and compares it, setting by setting, with what
   it meant to save. A mismatch means the edit would have changed the file's meaning, and
   the answer is to write nothing and say so (`ConfigError::RoundTrip`). This has already
   earned its keep: it refuses a save whose in-memory settings did not come from the file
   on disk, which would otherwise have silently dropped the reader's key bindings.
3. The previous file is kept as `config.toml.bak` and the new one arrives by **rename**,
   so an interrupted save cannot leave half a configuration behind.

`tests/config_save.rs` asserts the round trip from the outside as well: write, re-read,
compare.

## 13. Testing

Complete suite, all of it required before the project is considered done:

1. **Unit tests** per module: wrapping (CJK, emoji, ZWJ, combining marks, zero-width),
   table column negotiation, each mermaid parser, each layout engine's invariants
   (no overlapping nodes, no edges through nodes, deterministic output).
2. **Golden snapshot tests** of `--render-once` at widths 40, 80, and 120 over a corpus
   of adversarial documents: nested tables, Markdown inside cells, deep lists, mixed
   scripts, every Mermaid family, degenerate cases (empty table, single-node graph).

   Deliberately **not** in the goldens: very large graphs. A 1000-node diagram produces a
   snapshot diff no reviewer can read, so it would be rubber-stamped on every future
   change and manufacture false confidence. Scale is covered by the property tests
   (§13.3) and by the engine's own hundred-node case, both of which assert invariants
   rather than exact output. A golden nobody reads is worse than no golden.
3. **Property tests** (`proptest`): rendering never panics; every output line is exactly
   `width` display columns; rendering is idempotent; text content survives round-trip.
4. **Integration tests**: stdin path, `$PAGER` path, non-TTY stdout, resize sequences
   driven through a pty.
5. **Visual/usability review**: tmux-driven agents capture panes at multiple sizes and
   review against the standard in §9, critically.

CI-equivalent gate: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`,
all green. Builds and tests are limited to 4 parallel jobs.

## 14. Code standards

Idiomatic, DRY Rust: no `unwrap` outside tests, errors via `thiserror` at library
boundaries and `anyhow` at the binary edge, no duplicated layout logic between mermaid
families (shared graph-layout crate module), no `unsafe`, public items documented,
`#![warn(missing_docs)]` on library modules. Any duplication found between the table
renderer, the code renderer, and the mermaid renderers is a defect to be factored into
the shared `Canvas` layer.
