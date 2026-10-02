# Mermaid colours from `classDef`, `class` and `style`

Design authority for the work that follows. Written 2026-10-03 on branch
`worktree-fix-mermaid-subgraph-edges`, after v0.6.0.

## 1. What this changes

Flowchart, state, class and ER diagrams read the colour lines Mermaid allows in them
(`classDef`, `class`, `cssClass`, `A:::name`, `style`). Until now all four parsers
skipped those lines and every node was drawn in the theme's one diagram colour.

mdmost does not draw the author's colours as written. A Mermaid stylesheet is made for
a white page: `fill:#e3f4fb,color:#000` is a pale box with black text, which turns into
a bright block with unreadable text on a dark terminal. What the colours carry is the
grouping: nodes of one class look alike and differ from nodes of another class. mdmost
keeps that grouping and draws it in the active theme's own hues, so every theme passes
the contrast floors of `tests/theme_contrast.rs`.

Diagrams without colour lines render exactly as before.

The driving example is the overview diagram in the user's
`hin-mbox-mgr/docs/design2026/design.md`, which ends with:

```
classDef access fill:#e3f4fb,stroke:#2a8bb5,color:#000
classDef comm fill:#fdf0e1,stroke:#d4831f,color:#000
classDef part fill:#fbd9a8,stroke:#b8650a,color:#000
class airlock,idm,credmgr access
class zimbra,ldap,mgw comm
class zmcfgapi,webui,zmmgmt,ldapmgmt part
style mbox fill:#fff7ee,stroke:#b8650a,stroke-width:3px,color:#000
```

## 2. Settled rulings

Decided with the owner on 2026-10-02 and 2026-10-03 and not reopened here.

1. Colours carry the distinction, not the exact shade. Each author colour snaps to a
   hue of the active theme.
2. Two different author colours that snap to the same hue are kept apart: the later
   one moves to the nearest free hue.
3. The theme offers 16 hues: its 8 named hues plus the midpoint between each pair of
   neighbours, derived from the palette. No new palette keys.
4. Snapping uses fixed nominal angles, not the active theme's measured hues, so a
   diagram resolves to the same slots in every theme.
5. All four node families: flowchart, state, class, ER. `linkStyle` and `click` stay
   out of scope. Two edges share a junction cell and a cell has one colour, so an edge
   colour cannot be kept along the whole edge.
6. A subgraph or composite state `fill` tints its whole area at half strength.
7. Node text always uses the theme's text colour. `color` is ignored.
8. The author's values live in the AST. Resolution to theme colours happens at draw
   time. Parsers stay theme-free.

## 3. Parsing

### 3.1 The `Paint` type

A new AST type holds what the author wrote for one node, after merging:

```rust
/// `Color` is the existing truecolor type in `theme::style`.
pub struct Paint {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    /// `stroke-width` of 3px or more.
    pub heavy: bool,
}
```

`Option<Paint>` is added to `FlowNode`, `Group` (flowchart subgraph), `StateNode`
(plain and composite states), `Class` and `Entity`. `None` means the node draws with
the theme default.

### 3.2 Lines each family reads

| family | lines |
|---|---|
| flowchart | `classDef`, `class`, `A:::name`, `style` |
| state | `classDef`, `class`, `A:::name`, `style` |
| class | `classDef`, `cssClass`, `class A:::name`, `style` |
| ER | `classDef`, `class`, `A:::name`, `style` |

`class` and `cssClass` take a comma-separated node list. `classDef` may name several
classes at once (`classDef a,b fill:#f00`). In a flowchart, `style` and `class` may name
a subgraph by its key.

### 3.3 Property lists

One shared helper parses a property list such as
`fill:#e3f4fb,stroke:#2a8bb5,stroke-width:3px` (separators `,` and `;`, whitespace
ignored) into a partial `Paint`. It reads:

- `fill`, `stroke`: `#rgb`, `#rrggbb`, `#rrggbbaa` (alpha ignored) and the CSS basic
  colour names plus `orange`.
- `stroke-width`: a number with optional `px`. 3 or more sets `heavy`.

Any other property (`color`, `stroke-dasharray`, `font-weight`, ...) and any value in
another form (`rgb()`, `hsl()`, `var()`) is ignored.

### 3.4 Merge order

For each node: `classDef default`, then the node's classes in the order they were
assigned, then every `style` line for the node in source order. A later value replaces
an earlier one, property by property. A class may be defined after its use; merging
happens once the whole diagram is read.

### 3.5 Errors

An unknown class, an unknown node and an unreadable value never fail the diagram. The
affected property is dropped, as unknown lines are dropped today.

## 4. Resolution

A pure function in a new module `src/mermaid/paint.rs` maps the diagram's `Paint`
values to slots. It needs no theme.

### 4.1 Slots

16 slots at fixed nominal angles. The named hues sit where both built-in themes put
them (measured 2026-10-02, dark and light agree within 5 degrees):

| red | orange | yellow | green | cyan | blue | purple | magenta |
|---|---|---|---|---|---|---|---|
| 0 | 30 | 45 | 150 | 180 | 220 | 260 | 315 |

Midpoint slots sit halfway between neighbours: 15, 37.5, 97.5, 165, 200, 240, 287.5,
337.5.

### 4.2 The colour that picks the hue

`stroke` if it has a hue, otherwise `fill` if it has a hue. A colour has no hue when its
HSL saturation is below 0.2, or its HSL lightness is below 0.06 or above 0.97. Pastel
fills keep their hue: `#e3f4fb` has saturation 0.75. A `Paint` with no hued colour gets
no slot; its `heavy` still applies.

### 4.3 Collisions

Each distinct hue-picking colour is one unit, taken in source order of its first use.
A unit takes the slot nearest its angle. If another, different colour holds that slot,
it takes the nearest free slot (ties go clockwise). The same colour used twice takes the
same slot. Once all 16 slots are held, the nearest slot is shared.

The driving example resolves to:

| source | angle | slot |
|---|---|---|
| `access` `#2a8bb5` | 198 | 200 (cyan-blue) |
| `comm` `#d4831f` | 33 | 30 (orange) |
| `part` `#b8650a` | 31 | 37.5 (orange-yellow), 30 is held |
| `mbox` `#b8650a` | 31 | 37.5, same colour as `part` |

## 5. Theme

### 5.1 Slot colours

A named slot draws in the palette hue of that name. A midpoint slot draws in the 50/50
blend of its two neighbours.

### 5.2 Tints

- Full tint: the page background blended toward the slot colour by a fixed ratio
  (start value 0.15; the contrast tests set the final value).
- Half tint: half that ratio.

### 5.3 Contrast repair

A derived colour that misses its floor in some palette is blended toward the palette
text colour in small steps until it passes. This keeps custom `[themes.<name>]`
palettes readable without new keys.

## 6. Drawing

### 6.1 Resolved styles

At draw time each node with a slot gets a resolved style: border colour, inner
background (full tint when `fill` is set, page background otherwise) and `heavy`. The
node drawing code (flowchart `shape.rs`, state `shape.rs`, `record.rs` for class and ER)
takes this resolved style instead of reading `theme.diagram.node_border` directly. A node
without `Paint` gets the theme default, which is today's output.

The border takes the slot colour even when the slot came from `fill`.

### 6.2 Fill

Every cell inside the border gets the full tint as its background: padding, text and
emphasis spans. Border cells keep the page background.

### 6.3 Heavy borders

Box Drawing has heavy straight lines and square corners but no heavy arcs or diagonals.

- Rectangle, subroutine, class box, ER box, subgraph and composite state frame: fully
  heavy (`┏━┓`).
- Round, stadium and state boxes: heavy straight sides, light arc corners.
- Rhombus and cylinder: heavy on their straight runs only.

Where a light edge meets a heavy border, the mixed-weight table in
`layout/graph/glyph.rs` supplies the junction (for example `┯`). Glyphs missing from that
table are added, and `tests/glyph_inventory.rs` lists them.

### 6.4 Subgraphs and composite states

`stroke` and `heavy` apply to the frame. The title takes the frame's slot colour.

With `fill`, a pass after the graph is drawn sets the half tint as background on every
cell inside the frame that still carries the page background: empty space, edge lines,
edge labels, nodes without their own fill. Nodes with their own fill keep the full tint.
Nested frames are painted innermost first, so an inner tint is not overwritten by an
outer one.

## 7. Contrast floors

Added to `tests/theme_contrast.rs`, for both built-ins and the palette-derived theme, as
the existing assertions are:

- Each slot colour on the page background: 3:1 (border) and 4.5:1 (frame title).
- Theme text on each full tint and each half tint: 4.5:1.
- `diagram.line` on each half tint: 3:1.
- `diagram.edge_label` on each half tint: 4.5:1.

## 8. Tests

- Unit, `paint.rs`: property list parsing, colour names, merge order, hue detection, the
  slot table of §4.3, collision with all 16 slots held.
- Parse, per family: each line form of §3.2 attaches the expected `Paint`; unknown class
  and node names are dropped. `tests/mermaid_parse_robustness.rs` and the property tests
  cover the new lines.
- Render: a coloured flowchart checks cell styles (border colour, inner background,
  heavy glyphs, subgraph half tint, inner node full tint). The same for one state, one
  class and one ER diagram. The snapshot corpus stays unchanged, which pins the
  uncoloured output.
- `tests/glyph_inventory.rs` for new junction glyphs.

## 9. Documentation

- `docs/manual.md`, Mermaid section: the supported lines and properties, and that
  colours are drawn in the theme's hues. `click` and `linkStyle` stay listed as out of
  scope.
- `docs/superpowers/specs/2026-08-08-mdmost-design.md` §6.1, §6.3, §6.4, §6.7: point to
  this spec instead of listing the colour lines as out of scope.
- `docs/maintainer-notes.md`: why author colours are snapped instead of drawn as
  written (§1).
- `CHANGES.md`: one entry under Unreleased.

## 10. Out of scope

`linkStyle`, `click`, `color`, `stroke-dasharray`, font properties, `rgb()`/`hsl()`
values, colour in sequence, gantt and pie diagrams.
