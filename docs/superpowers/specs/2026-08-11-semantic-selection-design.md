# Semantic selection, diagram provenance and a three-state button

Design authority for the work that follows. Written 2026-08-11, after the
`code-provenance` plan landed and before the project goes public.

## 1. The defect this starts from

`Selection::columns_on` (`src/tui/select.rs`) answers a geometric question. For any
row in the middle of a drag it returns `0..width` — every cell of the row, including
the table's vertical rules, the code block's line-number gutter, cell padding and the
blank tail past the end of the text. The highlight is line-shaped.

The copy is not. `extract` walks `SearchSpan`s backwards to source bytes, and a table's
frame carries no spans, so borders never reach the clipboard. **What the reader sees
selected and what they get when they release the button are computed by two different
rules, and the display is the one that lies.** That is the same defect class as a status
bar that names the wrong thing, and this project has now caught it three times.

The fix is not to trim the rectangle. It is to stop drawing a rectangle.

## 2. The model: a selection is a range over the document

A selection is a hull of source bytes — it already is, for copying — and the highlight
is wherever that hull happens to be drawn. Anchor and cursor each resolve to a source
offset through the span they land on; the selection is `lo..hi` between them; the
highlight paints **every span intersecting `lo..hi`, clipped to the covered part**.

One computation feeds both the clipboard and the display, so they cannot disagree. This
is the browser's model: a selection is a range in the document, and what lights up is
whatever the layout put on screen for that range.

Everything else in this section follows from that sentence rather than being a rule of
its own:

- **Chrome never highlights.** Borders, the gutter, cell padding and the frame carry no
  spans because nothing in the document produced them. No special case is written.
- **A wrapped table cell follows its own text.** Selecting into a cell that wraps to two
  lines continues down that cell's lines and then hands over to the *next cell in
  document order* — not to whatever happens to sit beside it on the same screen row.
  Row-major is the source's own order.
- **The highlight stops at the end of content**, because there is no span past it.
- **Dragging across only chrome selects nothing.** No span touched, no hull, no copy,
  and the status bar reports nothing copied. A browser behaves the same over a table
  border.

### 2.1 Resolving an endpoint that lands on chrome

A drag that begins or ends on a border, the gutter or padding has no span under it. The
endpoint resolves to the **nearest span in document order in the direction of the
drag** — a press on the left rule of a cell takes the start of that cell's text, a
release past the end of a line takes the end of the last span on that row. An endpoint
with no span anywhere in that direction clamps to the document's start or end. This is
the only place a coordinate is interpreted rather than looked up, and it is where the
tests should be sharpest.

### 2.2 A diagram is atomic

*Amended 2026-08-11, after task 5 landed and the first drag over a real chart was
looked at. Amended again the same day, after the review found the last see/get
divergence left inside a diagram: the third case below.*

A drag **confined to one box** — and only such a drag; the two cases after this one take
precedence — selects that label's whole source range, whichever part of the box it
touched. Partial selection inside a label would need a byte mapping that survives entity
decoding and `<br>` splitting (`Label::parse`, `src/mermaid/ast.rs`), which buys nothing
a reader wants. The unit of selection in a diagram is the box, which is what a reader
points at.

**A drag wider than one label takes the whole diagram.** Crossing from one label into
another, or starting inside the diagram and ending outside it, expands the highlight to
the diagram's whole rectangle and copies the **whole fenced block**, ```` ```mermaid ````
opener and ```` ``` ```` closer included. A drag that continues past the diagram
contributes the block first and then whatever else it covered, in document order.

**A drag pressed outside any label takes the whole diagram immediately.** If the button
goes down on box art, an arrow, a box's blank interior or the padding inside the
rectangle, the reader has taken hold of the drawing rather than of anything written in
it, and the answer is the whole diagram wherever the drag is released — the block on the
clipboard, the rectangle washed. Before this case such a drag touched no label at all: it
resolved to no source range, so the clipboard fell back to the drawn cells (§3.1) while
the highlight stayed empty. The reader copied something and saw nothing, which is §1.

This third case is decided by the **press**, on the single cell the button went down on,
and never by comparing the drag's cells against the rectangle. A geometric rule over the
drag would be a second rule able to disagree with the source-range one; a lookup of which
atom, if any, holds the anchor decides before there is a range to disagree with. It is
also the reader's own model: the press is where they choose what they are taking.

**A block is copied verbatim, line one included.** The fenced block's recorded extent
begins at the ```` ``` ```` and the container prefix of that first line sits before it, so
copying the extent alone yields a diagram in a block quote as a bare ```` ```mermaid ````
followed by `> flowchart LR`. The copy therefore reaches back to the start of the opener's
line. An ordinary quoted fence keeps `> ` on every line it hands over; a diagram in one
does the same, and a diagram indented into a list item keeps its indent.

This is not a refinement of §2's markup rule but a replacement for it inside a diagram.
That rule extends a selection over every byte nothing drew, which in prose is a pair of
asterisks; on a Mermaid line it is nearly the whole line, so a drag over `Read` in
`    A[Read] --> B[Draw]` lit one word and copied `    A[Read] --> B[` — a token cut in
half, and precisely the see/get divergence of §1. It cannot be fixed from the highlight
side: `A[` is never drawn, so there are no cells to light.

**The whole-diagram wash covers the entire rectangle — box art, arrows and interior
blanks included. This is a deliberate exception to "chrome never highlights" (§2), and
the only one.** It is what makes the two states legible: either one label is lit and one
label is copied, or the diagram is a solid block and the block is what the clipboard
gets. Lighting only the labels while copying forty bytes of Mermaid would be §1 again,
wearing the chrome rule as a disguise.

The implementation keeps a single predicate for both: the diagram records its drawn
rectangle and its block's source range as a `Canvas::Atom`, and `select::resolve`
answers the clipboard and the highlight from one decision. "Confined to one label" is
judged on the resolved source hull, not on the two screen positions, so an endpoint that
landed on a border or an arrow — already resolved to a text offset by §2.1 — is judged
by the same rule as one the reader dragged over directly.

## 3. Diagram provenance

Diagrams carry no `SearchSpan`s today, which is why a drag over one falls back to
handing over the drawn box art (`select.rs` decision 3). That fallback is what makes a
diagram the one place selection is shaped by the screen. Removing it requires labels to
know where they came from.

**`Label` gains a source range.** It is a single shared type used by every family
(`src/mermaid/ast.rs:35`), so provenance is added in one place rather than seven.
`Label::parse` does not know its own offset, so parse sites pass it in; the range covers
the raw label text as it appeared in the mermaid source, before entity decoding and
`<br>` splitting.

**Each layout family emits a span where it draws a label.** `render_mermaid_with`
already returns a `Canvas`, and `Canvas::add_span` and `merge_metadata` already
translate spans through every `blit`, `indent`, `append` and `slice_rows`. The channel
exists; the families use it.

The seven, from `Diagram` (`src/mermaid/ast.rs:115`): **Flowchart** (`flowchart` and
`graph` both parse to it), **Sequence**, **Class**, **Er**, **Pie**, **Gantt**,
**State**. Their layout code is *not* all in one directory — `src/mermaid/layout/`
holds class, er, flowchart, graph, record and state, while gantt lives in
`src/mermaid/gantt/`, and sequence and pie are elsewhere again. Find each by its
`Diagram` variant rather than by directory listing, and note that what counts as a
"label" differs per family: a node's text in a flowchart, a participant and a message in
a sequence, a task name in a gantt, a slice's name in a pie. Each is a `Label` (except
where the AST holds a bare `String`, `ast.rs:742` — those need the same treatment or an
explicit note saying why not).

**The bridge rebases mermaid-local offsets onto document offsets.** A span emitted
inside the diagram is relative to the fenced block's own text; the document's spans are
absolute. `src/render/code.rs` already does exactly this rebasing for fenced code lines,
and it is the single highest-risk piece of arithmetic in this design. It is also where
CRLF and leading-whitespace handling have already bitten this project once each.

Once labels carry spans, a drag over a diagram yields **mermaid source** and the status
bar says `Markdown source`, like everything else. Which source is §2.2's answer: one
label for a drag confined to one box, the whole fenced block for anything wider and for
anything pressed outside a label. The
earlier draft of this paragraph promised the source *between* the first and last box
touched — `A[Parse] --> B[Layout]` — which is what the implementation did and what the
amendment above replaces; the string it produced in practice was truncated at the last
label it could map.

### 3.1 What remains of the rendered-cells fallback

It stays, and it should be rare. Content that genuinely has no source mapping — a table's
frame, a thematic break, a family whose layout has not yet been given spans — still falls
back to the drawn cells with `from_source: false`, and the status bar still says `rendered
text`. The fallback is not the design; it is the honest answer when the mapping is
absent.

A diagram's box art is no longer one of its cases. It used to be the example, and it was
the one place the fallback produced the §1 shape rather than an honest answer: the cells
were copied and nothing lit up. §2.2's third case claims the whole rectangle, so the
fallback is reached inside a diagram only where no atom was recorded at all — a layout
that failed, which draws no rectangle to press on either.

## 4. The copy button

Three block kinds now offer one: the code frame and the top-level table (shipped), and
the diagram (new).

**The diagram's button floats.** It has no frame rule to sit in, so it is placed over
the diagram's top-right rather than embedded in a border, and it steps aside rather than
stamping over box art — `button::place` already yields when it would collide, which is
the gutter-junction case it was built for.

**It is placed at render time.** Position is a function of `(AST, width, theme,
options)` like everything else, and it obeys the same gate as the other two: drawn only
when mouse capture actually succeeded, because a button nobody can click is worse than
no button.

**It copies the diagram's whole mermaid source**, matching what a code fence's `[copy]`
does.

### 4.1 Three appearances, none of them in the canvas

The control's *position* is decided at render time. Its *appearance* is decided at paint
time, from pointer and flash state:

| state | when | style |
| --- | --- | --- |
| at rest | always | muted — available, not announcing itself |
| hovered | the pointer is inside its region | brighter |
| copied | for `FLASH_FOR` ms after a click | the `[copied]` label |

`draw.rs` already repaints a button's cells from app state — `copied_flash` at
`src/tui/draw.rs:350` does it through the same `Offsets` as every other overlay. Hover is
that seam with a different trigger. **Render stays pure**; hover and flash are paint-time
concerns exactly like the search wash and the selection highlight.

**The muted style is a change to what already shipped.** `[copy]` currently takes the
frame's own colour on the code frame and the table, which makes it read as structure. A
lighter token, applied uniformly to all three. This is a theme question in every theme,
including the light one, whose heading ramp is separately known to be flat and
non-monotone.

### 4.2 Hover without a redraw storm

`EnableMouseCapture` turns on any-motion reporting, so `MouseEventKind::Moved` is
delivered today — `src/tui/term.rs` simply has no arm for it. A motion event fires per
cell crossed, so the state to track is **which hotspot is hovered, not where the pointer
is**, and a redraw happens only when that identity changes. A pointer moving within one
button, or across dead space, costs nothing.

## 5. Constraints this must not break

Carried from the renderer's design authority and still binding:

- **Rendering is a pure function of `(AST, width, theme, options)`.** Hover must not
  reach into it.
- **`render` must not depend on `tui`**; `src/export/` may depend only on `doc`.
- **Mermaid is Unicode box art only**; no HTML rendering.
- **There is no centring anywhere** — every block anchors at the same left margin.
- **The status bar never lies.** A selection that fell back to drawn cells says so.
- **`Esc` never quits**; `#![forbid(unsafe_code)]`; 4-core cap on every cargo
  invocation.
- The table gap-row threshold is 30 display columns; the title banner is opt-in.

## 6. Risks, in the order they are likely to bite

1. **The mermaid offset rebasing.** New arithmetic over a fence's literal text, where
   this project has already been bitten by CRLF (provenance was silently lost on every
   CRLF document) and by measuring an expanded line against a source offset. Tests must
   include a CRLF document and a diagram indented inside a list.
2. **Endpoint resolution on chrome** (§2.1) — the one interpreted coordinate.
3. **Snapshot churn.** The muted button style and the highlight change will move
   goldens. Prove any large churn is what it looks like: strip leading and trailing
   whitespace from both sides, sort, diff — identical means pure style movement with
   nothing added or lost.
4. **Seven families, seven chances to emit a span at the wrong origin.** Each needs its
   own test; a shared helper does not make them one behaviour.
5. **The demo recording shows selection highlighting** and will need re-recording once
   this lands. `docs/maintainer-notes.md` says how.

## 7. How this is tested

- **Every test derives from an observed failure, not from a description.** This project
  has produced four vacuous tests whose assertions held no matter what the code did.
- **Fault injection is mandatory**: revert the mechanism, confirm *that* test fails,
  restore. A test that goes *skipped* rather than red under mutation is a vacuous test
  in disguise — the clipboard tests can do this when no display server is present.
- **Measure box art in columns, not bytes.** Every box-drawing glyph is 3 bytes and 1
  column; `perl -CSD`, or count with `chars()`.
- The highlight is asserted **on the canvas**, not on the selection geometry — the
  defect being fixed is precisely that those two disagree.

## 8. Open question, to be settled by looking

**The exact colours of the three button states, in every theme.** Not named here on
purpose. The owner reviews by looking at rendered output, so the implementer renders all
three states in each theme, side by side, and asks — rather than picking a token and
declaring it done. The light theme deserves particular attention.
