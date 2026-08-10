# Code block provenance, and copy buttons — design

*2026-08-10*

Cells drawn inside a fenced or indented code block carry no link back to the source
they came from. Two visible consequences follow from that one gap: search never matches
inside a fence, and a selection over code copies the rendered cells rather than the
Markdown. This design closes the gap, and then spends the mapping it creates on a
clickable `[copy]` in the frame's top edge — and on a second one over tables, which
copies the grid in a form a spreadsheet can paste.

## 1. The gap

`render::inline::wrap` records a `SearchSpan` for every run of inline text
(`src/render/inline.rs:183`), and `render::banner` does the same for its letters
(`src/render/banner.rs:519`). Those are the only two producers. `render_code_block`
draws with `Canvas::write_str` and produces none, so for every cell inside a code block:

* `search::segments_for` finds no span overlapping the hit and the hit is never drawn —
  the pager's search silently skips fences.
* `select::source_hull` finds no span, `select::extract` falls through to
  `rendered_text`, and the status bar honestly reports `copied N bytes of rendered
  text` (`src/tui/select.rs:203`).

The status bar is not lying and does not need fixing. The provenance does.

## 2. Where the mapping has to be built

Not in `render`. The renderer never sees the document source: an inline `Text` node's
origins are derived from that node's own `SourceSpan` plus offsets into its literal,
which works because a `Text` literal *is* a slice of the source. A code block's literal
is not. comrak strips the container prefix from every line — four spaces from an
indented block, `> ` from a fence inside a block quote, the item indent from a fence
inside a list — so no arithmetic on `node.source` alone recovers where a given code
line starts.

The mapping is therefore built in `doc::convert`, which holds the source string and the
`LineOffsets` table already.

**`NodeKind::CodeBlock` gains `lines: Vec<SourceSpan>`** — one entry per line of
`literal`, giving that line's byte range in the document source.

The construction walks the source lines the node's `Sourcepos` covers and matches each
literal line as a **suffix** of a source line. Suffix matching is the whole trick: it is
prefix-stripping run backwards, so it is correct for indented blocks, quoted fences and
list-nested fences without any of them being special-cased, and it is *verified* rather
than assumed, because the source text is right there to check against. Two rules keep it
honest:

* An empty literal line gets an empty span. There is nothing to select or search.
* A literal line that matches no source line gets an empty span, and the walk continues.
  A block whose lines cannot be located degrades to exactly today's behaviour — no
  provenance — rather than to a wrong offset. A wrong offset would put a search hit on
  the wrong cells and copy the wrong bytes, which is worse than copying rendered text.

`SourceSpan` is already the type `Node::source` uses; nothing new is introduced.

## 3. Spans on the canvas

`render::code::code_area` records one `SearchSpan` per drawn code line, mapping the
line's `SourceSpan` to the columns it occupies.

* **One span per line, not per highlight token.** Search slices a span by overlap
  (`search::segments_for`) and `select::byte_at_column` walks graphemes inside one, so a
  whole-line span is sufficient for both consumers. It is also the fewest records, and
  syntax highlighting is not a fact about the source worth duplicating into the metadata
  channel.
* **Only the visible columns.** Code never wraps; a line wider than the frame is
  clipped. The span covers the columns actually drawn, and the byte range is truncated to
  match, so a hit in the clipped tail is correctly reported as not on screen. The
  overflow marker `›` is chrome and carries no span.
* **The line-number gutter carries no span.** The numbers are not in the document.

Nothing in `canvas` changes. `Canvas::framed` blits its content one row and one column
in, and `blit` already translates spans, so a code block's spans survive framing,
padding, `indent` and `append` the same way a paragraph's do.

With this in place, search matches inside fences and a selection over code copies
Markdown source, reported as such. Sections 4 and 5 are the second feature.

## 4. The copy button

A `[copy]` label in the right of a code frame's top edge, clicked with the mouse:

```
╭ rust ─────────────────────────[copy] ╮
│ fn main() {                          │
│     println!("hello");               │
│ }                                    │
╰──────────────────────────────────────╯
```

**ASCII, unconditionally.** Not a Nerd Font glyph behind detection, and not a lone
Unicode symbol. This is the house rule that already governs bullets and task boxes: a
mark a reader has to *act on* is ASCII, so it looks the same in every terminal and
cannot arrive as tofu. The language label beside it keeps its detected icon, because
that one is decoration.

**Where it is drawn.** `framed_code` reserves the last 8 columns of the top edge, inside
the corner, and draws `[copy]` right-aligned in them. It is dropped entirely when the
frame is too narrow to hold it, and when it would collide with the language label or the
gutter junction that `join_gutter` places — the existing rule that the label and the
junction are never in competition extends to a third occupant, and the button is the one
that yields, because it is the only one of the three that is optional.

Eight columns, not six, because the flash in §5 is `[copied]`.

**Which blocks get one.** Fenced and indented code blocks. Not diagrams — a drawn
Mermaid diagram is not preformatted text and there is no code to copy. The Mermaid
*fallback* block does get one: it is a syntax-highlighted code block showing Mermaid
source, and that source is exactly what a reader who just saw the failure caption wants.

**`RenderOptions.copy_button`.** The pager sets it from whether mouse capture was
actually granted, so a terminal that refused the mouse shows no button rather than a
control that does nothing. `--render-once` leaves it off: a dump is text in a pipe and a
`[copy]` in it would be noise. The goldens render with default options and therefore do
not move.

## 5. Clicking it

A fourth canvas metadata channel, beside `Anchor`, `SearchSpan` and `Pin`:

```rust
pub struct Hotspot {
    pub row: usize,
    pub col: u16,
    pub cols: u16,
    /// The plain-text payload. Always present; the only thing OSC 52 can carry.
    pub text: String,
    /// An optional richer flavour for the local clipboard only.
    pub html: Option<String>,
}
```

It exists for the same reason the other three do: the pager needs to know something
about a region that only the renderer that drew it can know — here, that these cells are
a control, and what it puts on the clipboard.

**It carries payloads, not a byte range.** The source range of a quoted fence includes
the `> ` on every interior line; copying that is not what the button promises. The
literal is already the dedented code, so the hotspot carries it and the button is exactly
right in every container. This is deliberately *not* the mapping from §2 — that one
answers "which source byte is this cell", and it is the wrong answer to "what should this
button copy".

`html` is `None` for a code block and `Some` for a table (§6). The plain-text payload is
never optional, because the route that always works cannot carry anything else.

Like `Pin`, a hotspot is a claim about a region of a specific row, so it travels through
`append` and `indent` and is dropped by `blit` into a shared row. `framed_code` records
it on the framed canvas — after `Canvas::framed`, which is itself a `blit` — so the
button survives its way up to the document.

That leaves one case the channel's rule alone gets wrong. A code block inside a table
cell is blitted into a row it shares and loses its hotspot, but the drawn `[copy]` is
just cells and survives — a label with nothing behind it, which is the same dead control
the mouse gate in §4 exists to prevent. So the button is not drawn at all when
`ctx.table_depth > 0`. **The label and the hotspot are decided together, in one place,
and neither is ever emitted without the other.**

**Handling.** `tui::term` matches a hotspot hit before the `in_doc` arm that starts a
selection, so clicking `[copy]` never begins a drag. The column is translated by the same
horizontal offset the selection uses; the button is at the right of the row and is not
pinned, so it scrolls with the code. Delivery goes through the existing
`tui::clipboard::report`, and the status bar reads `copied N bytes of code` — a third
value beside `Markdown source` and `rendered text`, because it is neither: it is the
block, not the selection.

**The flash.** After a successful copy the label reads `[copied]` briefly, painted by
`tui::draw` over the reserved 8 columns.

This is drawn at draw time and never at render time, and that is the one architectural
constraint in this design worth stating plainly: **rendering is a pure function of
`(AST, width, theme, options)`**. "This block was copied 300 ms ago" is transient pager
state; letting it into the renderer would make the canvas cache a function of the clock
and would be the first crack in the property the whole design rests on. The reserved
region is what makes the overwrite possible without re-rendering, and the reason the
reservation is wider than the label it normally shows.

## 6. The table button

A table gets a `[copy]` too, and what it copies is a grid a spreadsheet understands.

### 6.1 Two flavours, and which one is load-bearing

**TSV is what makes Excel and Google Sheets split a paste into cells** — tab between
columns, newline between rows. Not HTML. HTML is an upgrade on top of it that carries
emphasis, alignment and links, and both applications prefer it when it is offered.

That ordering decides the architecture, because the two clipboard routes are not equal:

* **OSC 52** is the primary route and the one that survives SSH (`src/tui/clipboard.rs`
  §"Why OSC 52 first"). It is a single escape sequence carrying a single plain-text
  payload. It has no MIME flavours and never will.
* **The local clipboard** (`arboard`, behind the optional `clipboard` feature) can offer
  `text/html` and `text/plain` together.

So: **the TSV is always sent, on both routes; the HTML is offered only where a flavoured
clipboard exists.** A reader on a remote host pastes cells with no formatting, which is
the outcome they would have got from the plain design, rather than nothing. Nobody gets
a worse result than TSV-only, and a local reader gets a formatted table.

`clipboard::copy` grows a sibling that takes both payloads: OSC 52 is written from the
text exactly as today, and the local half uses `arboard`'s HTML setter with the TSV as
the plain-text alternate. `Delivery` is unchanged. The status bar reads `copied N bytes
of table`, counting the TSV — the payload every reader actually receives. It must not
report the HTML's size, which is larger and which a reader on a remote host did not get.

### 6.2 Generating them

A new module — `src/export/` — turns a table `Node` into TSV and into HTML. It depends
on `doc` and nothing else: not on `canvas`, not on `theme`, and above all not on `tui`.
It is a pure function from AST to string, which makes it the easiest thing in this
design to test exhaustively.

**TSV.** Each cell is flattened to a single line of plain text through the existing
`Node::plain_text`. Tabs and newlines *inside* a cell are replaced with a single space.
This is a deliberate choice over Excel's `"…"` quoting convention, which is fragile and
which Sheets and Excel disagree about: flattening cannot produce a grid that misaligns,
and a cell containing a newline is already being shown on one visual line by the pager.

**HTML.** A `<table>`, with the header row as `<th>` and the rest as `<td>`, and
`align="right"` / `align="center"` from the table's declared alignments. The inline
subset is exactly what a table cell can contain and no more:

| Node | HTML |
| --- | --- |
| `Text` | escaped text |
| `Strong` / `Emph` / `Strikethrough` | `<strong>` / `<em>` / `<del>` |
| `Code` | `<code>` |
| `Link` | `<a href="…">`, see below |
| `Image` | its alt text, as text |
| `LineBreak` | `<br>` |
| `SoftBreak` | a space |
| anything else | its `plain_text()` |

Two rules are load-bearing rather than cosmetic:

* **Escaping is unconditional.** `&`, `<`, `>` in text and `"` additionally in attribute
  values. A document is untrusted input, and a cell containing `<script>` must arrive at
  the clipboard as four escaped characters. This is the one part of the module where a
  bug is a real problem rather than an ugly paste, so it gets its own tests.
* **Only `http`, `https` and `mailto` links keep their `href`.** Any other scheme — and
  in particular `javascript:` — is dropped and the link renders as its text. The payload
  is being handed to another application to interpret; narrowing the schemes costs
  nothing and removes the whole question.

**"No HTML" is not violated.** The standing rule is that `mdmost` does not *render*
HTML — it will not interpret markup in a document and paint it. Serialising an AST the
pager already parsed into a clipboard flavour is the opposite direction, it never
re-enters the renderer, and no HTML from the document is ever passed through: everything
in the output is generated here and escaped here.

### 6.3 Where it sits

**Top-right of the table's top rule**, in the same 8 reserved columns as the code
button, so the control is in one place across the whole pager and a reader learns it
once.

```
╭──────────┬──────────┬─[copy] ╮
│ Name     │ Role     │ Since  │
├──────────┼──────────┼────────┤
│ Ada      │ Design   │   1843 │
╰──────────┴──────────┴────────╯
```

The known cost, recorded so it is not rediscovered as a bug: a code frame is rendered to
the body width and is never wider than the viewport, but **a table can be**, and then its
right edge — and its button — is off screen until the reader scrolls right. Consistency
was chosen over reachability here deliberately. If that becomes annoying in use, the fix
is not to move the button: it is to hold the top rule's trailing columns still, the
mirror of what `Pin` already does for a code block's leading gutter columns. That is a
new capability in the pin channel and is not part of this design.

The button is dropped for a table too narrow to hold it beside its own corner, and — as
in §5 — the label and the hotspot are decided together, so a table that cannot carry a
hotspot does not show a label.

It is gated by the same `RenderOptions.copy_button` as the code button, for the same
reasons, and like it the label and the hotspot are emitted together or not at all.

## 7. Testing

* `doc::convert` — the mapping, per container: a top-level fence, an indented block, a
  fence inside a block quote, a fence inside a list item, a block with blank lines, and a
  block whose lines cannot be matched (empty spans, no panic, no wrong offset).
* `render::code` — a span per drawn line; a clipped line's span covers only the drawn
  columns; no span on the gutter or the overflow marker; the button appears, is dropped
  when narrow, and yields to the label and the junction.
* `search` — a hit inside a fence is found and lands on the right cells, including
  inside a quoted fence, and a hit in a clipped tail is not drawn.
* `tui::select` — a selection over code reports `from_source: true` and yields the
  Markdown.
* `tui` — a click on the hotspot copies the literal and does not start a selection; a
  click one column outside it does start one.
* `export` — the TSV grid, including a cell containing a tab and one containing a
  newline; the HTML for each inline node; alignment attributes; the header row; and
  escaping, with `<script>`, `&`, and a quote inside an `href`, plus a `javascript:` URL
  that must lose its `href` and keep its text.
* `tui::clipboard` — the dual-flavour copy sends the TSV over OSC 52 unchanged, and the
  local half is offered both payloads. Tests must not call the real `copy`: it writes an
  escape sequence to standard output, which is why the module already separates the
  local half (`src/tui/clipboard.rs:303`).

Every behavioural test is proven red before the change that makes it green.

## 8. Out of scope

* **No keyboard route.** No `y`-copies-the-block-at-the-cursor key. It needs a
  definition of "the current block", a key-table entry and a status-bar hint, and the
  keyboard already has a route to the same bytes: select and copy, which §3 makes yield
  real Markdown for the first time.
* **No hover highlight.** The pager does not track mouse motion today, and adding a
  redraw on every mouse move to light up six columns is not worth it.
* **The button is not a general widget.** Two controls, two places, both spelled out
  above. `Hotspot` is the channel a third would use, but nothing here is built for a
  caller that does not yet exist.
* **No CSV, and no choice of format.** One button, one payload pair. A format menu is a
  second decision to make at the moment a reader wants one click.
* **No copy button on diagrams.** A drawn Mermaid diagram is box art; there is no grid
  and no code. The fallback block, which shows Mermaid source, keeps its code button
  (§4).
