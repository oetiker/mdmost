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

Pushback reviews raised the points behind rulings 9 to 15 (2026-10-03) and 16 to 20
(2026-10-04).

1. Colours carry the distinction, not the exact shade. Each author colour snaps to a
   hue of the active theme.
2. Two different author colours that snap to the same hue are kept apart: the later
   one moves to a free hue (§4.4).
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
9. A heavy frame stays dashed: `╍` and `╏` with heavy corners.
10. CSS colour names map to their slot by name. Hex values snap by angle.
11. A colliding colour skips the two slots next to the slot it lost (§4.4).
12. Slot order is the source order of the `classDef` or `style` line. A paint that
    reaches no node takes no slot.
13. A `fill` without a hue does not tint.
14. `classDef default` applies to nodes only, not to subgraphs or composite states.
15. `style X` or `class X c` for an undeclared `X` is ignored. It does not create a node.
16. Contrast is repaired on the slot inks, not on the tints: a weak ink moves a little
    toward the text colour, and the tints keep their strength (§5.3).
17. Notes inside a filled composite state keep the page background.
18. A heavy frame keeps the round corners of a light frame: `╭╍╮ ╏ ╰╍╯`.
19. The remaining `:::` misreadings are fixed before the colour work (§3.7).
20. Two colours are the same when their RGB values are equal: `red` and `#ff0000` are
    one colour.

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
    /// Byte offset of the `classDef` or `style` line that supplied the colour picking
    /// the hue (§4.2), for the order of §4.4.
    pub origin: Option<usize>,
}
```

`Option<Paint>` is added to `FlowNode`, `Group` (flowchart subgraph), `StateNode`
(plain and composite states), `Class` and `Entity`. `None` means the node draws with
the theme default. Start and end markers (`[*]`), choice, fork and join states and notes
take no paint: a class or style aimed at them is ignored.

The paint also records the source position of the line that decided its hue colour, for
the ordering of §4.4.

### 3.2 Statements each family reads

| family | statements |
|---|---|
| flowchart | `classDef`, `class`, `style`, `:::` |
| state | `classDef`, `class`, `style`, `:::` |
| class | `classDef`, `cssClass`, `style`, `:::` |
| ER | `classDef`, `class`, `style`, `:::` |

- `classDef a,b props`: defines one or more classes. `default` is a class name that
  applies to every node of §3.1, and only to nodes.
- `class A,B c` and `cssClass "A,B" c`: assign class `c`. Quotes around the list are
  stripped. Several classes may be assigned to one node, also as `c1,c2`.
- `style A props`: paints one node. In a flowchart `style` and `class` may name a
  subgraph by its key. In a state diagram they may name a composite state.
- A class name is `[A-Za-z0-9_-]+`.

### 3.3 Where `:::` may appear

`:::name` attaches class `name` to the node it follows. It binds tighter than `:`, so
`A:::c : text` in a state diagram is class `c` plus the description `text`.

- flowchart: after a node reference anywhere one may stand: alone (`A:::c`), after a
  shape (`A[text]:::c`), at either end of an edge, inside an `&` group. After a subgraph
  key: `subgraph one:::c` and `subgraph one:::c [Title]`.
- state: after a state name, alone or at either end of a transition. After `[*]` it is
  read and ignored. In `state A:::c {` it applies to the composite state. In
  `note left of A:::c : text` it applies to `A`, not to the note.
- class: `class A:::c` and `class A:::c {`. At either end of a relation
  (`Animal:::c <|-- Dog`) and before a member (`Animal:::c : +int age`).
- ER: after an entity name, alone, before `{`, or at either end of a relationship.

Commits `c7d24c4` to `89e5581` added the shared reader (`lex::split_class_suffix`,
`lex::split_label_colon`). The forms of §3.7 are fixed with it before the colour work.

### 3.4 Property lists

One shared helper parses a property list such as
`fill:#e3f4fb,stroke:#2a8bb5,stroke-width:3px` into a partial `Paint`. Properties are
separated by `,` only. `;` ends the statement, as in Mermaid: the statement splitter
splits there, and a fragment after it that reads as `word:value` is dropped instead of
being parsed as a node or edge. The splitter does not split at a `;` that closes an
all-digit hex value (`#333;`, `#000;`), because it reads that as a character reference;
so the property reader itself also stops at the first `;`.

- `fill`, `stroke`: `#rgb`, `#rrggbb` or `#rrggbbaa` (alpha ignored). The `#` is
  required. CSS colour names, case-insensitive: `red`, `maroon`, `orange`, `yellow`,
  `olive`, `green`, `lime`, `teal`, `cyan`, `aqua`, `blue`, `navy`, `purple`, `fuchsia`,
  `magenta`, and the neutrals `black`, `white`, `gray`, `grey`, `silver`.
- `stroke-width`: a number with optional `px`. 3 or more sets `heavy`.

Any other property (`color`, `stroke-dasharray`, `font-weight`, ...) and any value in
another form (`#rgba`, `rgb()`, `hsl()`, `var()`) is ignored.

### 3.5 Merge order

For each node: `classDef default`, then the node's classes in the order they were
assigned, then every `style` line for the node in source order. A later value replaces
an earlier one, property by property. A class may be defined after its use; merging
happens once the whole diagram is read.

### 3.6 Errors

An unknown class, an undeclared node and an unreadable value never fail the diagram.
The affected statement or property is dropped.

### 3.7 Parse failures fixed first

These forms break diagrams in v0.6.0. They are fixed in their own commits before the
colour work, by reading `:::name` and `;`-separated style fragments as §3.3 and §3.4
describe and discarding the result.

Fixed in `c7d24c4` to `89e5581`:

- flowchart: `A:::foo --> B` and `style A fill:#e3f4fb;stroke:#2a8bb5` fail the diagram.
- state: `[*] --> A:::foo` draws a transition labelled `::foo`.
- class: `class Animal:::foo` draws a second class named `Animal:::foo`.
- ER: `CUSTOMER:::foo ||--o{ ORDER : places` fails with "unknown attribute key".
- state and class: a style line with `;` draws an extra box named `stroke`.

Fixed next:

- class: `Animal:::c <|-- Dog` and `Animal:::c : +int age` fail with "empty member";
  `Animal <|-- Dog:::c` draws the relation labelled `::c`.
- state: `note left of A:::c : hi` draws the note text `::c : hi`; `state A:::c {`
  keeps `A:::c` as the composite's name.
- flowchart: `subgraph one:::c` keeps `one:::c` as key and title.
- `BorderSet::HEAVY` (`src/canvas/border.rs`) draws `┛` as its bottom-left corner and
  the double-line `╣` and `╬` as its left tee and cross. The gantt critical task frame
  uses it today; §6.2 needs it correct.

## 4. Resolution

A pure function in a new module `src/mermaid/paint.rs` maps the diagram's `Paint`
values to slots. It needs no theme.

### 4.1 Slots

16 slots at fixed nominal angles (HSL hue). The named hues sit near where both built-in
themes draw them (measured 2026-10-02):

| | red | orange | yellow | green | cyan | blue | purple | magenta |
|---|---|---|---|---|---|---|---|---|
| nominal | 0 | 30 | 45 | 150 | 180 | 220 | 260 | 315 |
| dark | 352 | 28 | 45 | 146 | 180 | 221 | 259 | 317 |
| light | 6 | 31 | 47 | 150 | 180 | 220 | 260 | 312 |

Midpoint slots sit halfway between neighbours: 15, 37.5, 97.5, 165, 200, 240, 287.5,
337.5. A midpoint slot draws as the RGB blend of its neighbours, whose hue can be some
way off the nominal angle (yellow-green draws near 84). The nominal angle is used only to
pick the slot.

Distance between two angles is the shorter way round the circle.

### 4.2 The colour that picks the hue

`stroke` if it has a hue, otherwise `fill` if it has a hue. A colour has no hue when its
HSL saturation is below 0.2, or its HSL lightness is below 0.06 or above 0.97. Pastel
fills keep their hue: `#e3f4fb` has saturation 0.75. A `Paint` with no hued colour gets
no slot; its `heavy` still applies.

### 4.3 CSS names

A CSS name goes straight to its slot: `red`, `maroon` to red; `orange` to orange;
`yellow` to yellow; `olive` to yellow-green; `green`, `lime` to green; `teal`, `cyan`,
`aqua` to cyan; `blue`, `navy` to blue; `purple` to purple; `fuchsia`, `magenta` to
magenta. The neutrals have no hue. Two names on the same slot (`green` and `lime`) are
two colours and collide as in §4.4.

### 4.4 Collisions

Each distinct hue-picking colour is one unit. Two colours are the same when their RGB
values are equal; a CSS name has its CSS RGB value, so `red` and `#ff0000` are one unit.
Units are taken in the source order of the `classDef` or `style` line that supplies them
(`Paint::origin`). A colour that decides no node's hue takes no slot.

A unit's angle is its hue angle, except that a unit first written as a CSS name takes
the nominal angle of that name's slot (§4.3). A later hex value with the same RGB joins
that unit and its slot.

1. The unit takes its nearest slot S if S is free or held by the same colour.
2. Otherwise it takes the nearest free slot that is neither S nor one of the two slots
   next to S.
3. If there is none, the nearest free slot.
4. If no slot is free, it shares S.

A tie in distance goes to the slot with the larger nominal angle (counting 0 as 360 when
comparing with 337.5).

The driving example resolves to:

| unit | angle | nearest | slot |
|---|---|---|---|
| `access` `#2a8bb5` | 198 | 200 | 200 cyan-blue |
| `comm` `#d4831f` | 33 | 30 | 30 orange |
| `part` `#b8650a` | 31 | 30, held | 45 yellow; 15 and 37.5 are skipped, 0 is 31 away |
| `mbox` `#b8650a` | 31 | same colour as `part` | 45 yellow |

## 5. Theme

### 5.1 Where the slots live

`Theme` gets a public field `diagram_slots: [SlotInk; 16]`, computed in
`Theme::from_palette`. `SlotInk` is `Copy` and holds three colours: `ink`, `full_tint`
and `half_tint`. Index order is the nominal angle order of §4.1, starting at red.

### 5.2 Colours

- `ink`: a named slot uses the palette hue of that name. A midpoint uses the 50/50
  `Color::blend` of its neighbours.
- `full_tint`: `bg.blend(ink, 0.15)`.
- `half_tint`: `bg.blend(ink, 0.075)`.

The tints are computed from the palette ink of §5.2 and do not change when §5.3 repairs
the ink. The ratio 0.15 is the start value.

Measured 2026-10-04 at 0.15 with unrepaired inks: the dark theme passes every floor of
§7 (lowest 5.81:1). The light theme fails 72 of the 256 "ink on half tint" pairs: orange
(4.10 to 4.19), orange-yellow (4.20 to 4.30), yellow (4.27 to 4.37), cyan (4.40 to 4.49),
red-orange (4.44 to 4.49) and green-cyan (4.50 on magenta). Its orange ink on the page
measures 4.60:1, so any tint under it falls short. §5.3 repairs this.

### 5.3 Contrast repair

The WCAG 2 contrast function moves from `tests/theme_contrast.rs` into `src/theme`
(`Color::contrast`), and the test uses it from there.

The steps run in this order, each over all 16 slots before the next starts:

1. Ink. While a slot's `ink` misses 4.5:1 on `bg` or on any slot's `half_tint`, or 3:1
   on its own `full_tint`, blend it 0.05 further toward `fg`. At most 20 steps. In the
   light theme this moves orange, orange-yellow and yellow by 0.10 and red-orange,
   green-cyan and cyan by 0.05, with hue shifts under 2 degrees; the dark theme needs no
   step.
2. Fallback. A slot whose `ink` still misses after step 1 draws nodes in
   `diagram.node_border` and frames in `diagram.group_border` with titles in
   `diagram.group_title`, and uses `bg` as both tints.
3. Tints. While a §7 pair whose ink is a fixed theme style (text, labels, lines, borders)
   misses its floor on a slot's `full_tint` or `half_tint`, lower that tint's ratio by
   0.015 toward `bg`. At most 10 steps; at ratio 0 the tint is `bg`. A tint that moves
   toward `bg` keeps every ink check of step 1 valid, because each ink already clears
   both `bg` and the stronger tint.

Every loop is bounded, so a palette whose own `fg` misses 4.5:1 on `bg` ends at step 2
instead of looping.

## 6. Drawing

### 6.1 Resolved styles

At draw time each node with a paint gets a resolved style: border colour, inner
background and `heavy`. Border colour is the slot `ink`, or the theme default when the
paint has no slot: `diagram.node_border` for a node, `diagram.group_border` and
`diagram.group_title` for a frame. The inner background is the slot's `full_tint` when the paint has a
`fill` with a hue and a slot; otherwise the page background. The node drawing code
(flowchart `shape.rs`, state `shape.rs`, `record.rs` for class and ER) takes this
resolved style instead of reading `theme.diagram.node_border` directly. A node without a
paint gets the theme default, which is today's output.

The border takes the slot colour even when the slot came from `fill`.

### 6.2 Shapes

"Inside" means every cell between the border cells: padding, text, emphasis spans and
the cells of inner rules. Inside cells get the inner background. Border cells keep the
page background. Inner rules (subroutine bars, cylinder lid and base rules, class and ER
dividers) are drawn in the border colour.

Box Drawing has heavy straight lines and square corners but no heavy arcs or diagonals.
Heavy applies as follows:

| shape | light | heavy |
|---|---|---|
| rectangle, class, ER | `┌─┐ │ └─┘` | `┏━┓ ┃ ┗━┛` |
| round, state box | `╭─╮ │ ╰─╯` | `╭━╮ ┃ ╰━╯`, arcs stay light |
| stadium | `╭─╮ ( ) ╰─╯` | `╭━╮ ( ) ╰━╯`, arcs and parentheses stay light |
| circle | `╭─╮ (( )) ╰─╯` | `╭━╮ (( )) ╰━╯`, arcs and parentheses stay light |
| rhombus | ` ╱─╲ │ ╲─╱` | ` ╱━╲ ┃ ╲━╱`, diagonals stay light |
| subroutine | `┌┬─┬┐ ││ ││ └┴─┴┘` | `┏┯━┯┓ ┃│ │┃ ┗┷━┷┛`, inner bars stay light |
| cylinder | `╭─╮ ├─┤ │ ├─┤ ╰─╯` | `╭━╮ ┠─┨ ┃ ┠─┨ ╰━╯`, lid rules stay light |
| class/ER divider | `├─┤` | `┠─┨` |
| frame | `╭╌╮ ╎ ╰╌╯` | `╭╍╮ ╏ ╰╍╯`, arcs stay light |

The heavy forms need two new border sets (heavy with light arcs, and heavy dashed with
light arcs). Both join `BorderSet::ALL`. The rectangle uses `BorderSet::HEAVY` after the
§3.7 fix.

`╍` and `╏` join the Thick list of `stroke_of` in `layout/graph/glyph.rs`, so an edge
crossing a heavy frame gets the mixed junction the table there already holds. A light
edge meeting a heavy border uses the same table (for example `┯`).

### 6.3 Subgraphs and composite states

`stroke` and `heavy` apply to the frame. The title takes the frame's slot `ink`.

With `fill`, a pass after the graph is drawn sets the slot's `half_tint` as background
on every cell inside the frame that still carries the page background: empty space,
edge lines, edge labels, inner frames' borders and titles, nodes without their own fill.
Nodes with their own fill keep their full tint. Notes keep the page background: their
text uses `diagram.note`, which misses 4.5:1 on the light half tints (4.27 to 4.37).
Nested frames are painted innermost first, so an inner tint is not overwritten by an
outer one.

The layout does not keep frame rectangles today: frames are wrapped level by level, and
the parent level draws the edges into a subgraph afterwards with the page background
(`layout/graph.rs`, the frame wrapping around line 619). So the layout records each
frame's rectangle in final canvas coordinates, with its paint and nesting depth, and the
pass runs once on the finished canvas.

## 7. Contrast floors

Added to `tests/theme_contrast.rs` for every theme in `themes()`, after §5.3 has run.
`themes()` stays as it is. A low-contrast palette would fail the twelve existing tests
that loop over it, so the repair is tested on its own list of palettes (§8).

For each slot X:

- on `bg`: X `ink` 4.5:1 (frame title; this also covers the border's 3:1).
- on X `full_tint`: theme text, `diagram.stereotype` and `diagram.edge_label` 4.5:1;
  X `ink` 3:1 (inner rules).
- on X `half_tint`: theme text, `diagram.edge_label`, `diagram.group_title` 4.5:1;
  `diagram.line`, `diagram.arrow`, `diagram.node_border`, `diagram.group_border` 3:1;
  and for every slot Y, Y `ink` 4.5:1 (an inner frame title or a painted node's border
  inside X).

Before repair, the light theme fails the "Y `ink` on X `half_tint`" pairs listed in
§5.2. After §5.3 step 1 all pairs pass in both built-in themes; the implementation
records the resulting ink blends in the theme comment. Notes never sit on a tint
(§6.3), so `diagram.note` has no tint floor.

## 8. Tests

- Unit, `paint.rs`: property list parsing, colour names and their slots, `#` required,
  merge order, hue detection, every rule of §4.4 including the example table, all 16
  slots held.
- Parse, per family: every form of §3.2 and §3.3 attaches the expected `Paint`; undeclared
  nodes and unknown classes are dropped; every failure of §3.7 parses.
  `tests/mermaid_parse_robustness.rs` and the property tests cover the new statements.
- Theme: on a separate list of low-contrast palettes (one whose `fg` misses 4.5:1 on
  `bg`, one whose hues sit close to `bg`), §5.3 terminates, and every slot either passes
  §7 or has fallen back by step 2.
- Render: a coloured flowchart checks cell styles (border colour, inner background,
  heavy glyphs per shape of §6.2, subgraph half tint, inner node full tint, nested frame
  order). One state, one class and one ER diagram the same way. A light edge on a heavy
  border draws `┯`, `┷`, `┠` or `┨`. An edge crossing a heavy frame draws a mixed
  junction. The snapshot corpus stays unchanged, which pins the uncoloured output.

## 9. Documentation

- `docs/manual.md`, Mermaid section: the supported statements and properties, and that
  colours are drawn in the theme's hues. `click` and `linkStyle` stay listed as out of
  scope.
- `docs/superpowers/specs/2026-08-08-mdmost-design.md` §6.1, §6.3, §6.4, §6.7: point to
  this spec instead of listing the colour lines as out of scope.
- `docs/maintainer-notes.md`: why author colours are snapped instead of drawn as
  written (§1), and why collisions skip neighbouring slots (ruling 11).
- `CHANGES.md`: one entry under Unreleased for the colours, and one per fix of §3.7.

## 10. Out of scope

`linkStyle`, `click`, `color`, `stroke-dasharray`, font properties, `#rgba`, `rgb()` and
`hsl()` values, colour in sequence, gantt and pie diagrams, paint on markers, pseudo
states and notes.
