# LaTeX math

Design authority for the work that follows. Written 2026-08-19, after v0.2.0.

## 1. What this adds

`$E = mc^2$` reads as `E = mc²` on the line. A `$$` block, or a ```` ```math ````
fence, is laid out in two dimensions — stacked fractions, raised scripts, limits over
an operator, matrices inside tall brackets — and blitted like any other block.

The subsystem is a sibling of `src/mermaid/`: source text in, `Canvas` out, no
knowledge of `render` or `tui`. Everything below follows from that plus eight
decisions taken during design, recorded here so a later reader does not have to
re-derive them.

| decision | taken |
|---|---|
| Display math is 2D, inline math is one row | Inline math must not give a paragraph line a variable height. Reflow is the property the whole renderer rests on. |
| Coverage is KaTeX-scale | Not core-plus-a-bit. Documents that use math use `\begin{array}` and `\newcommand`. |
| The front end is `pulldown-latex`, as a dependency | It is the expensive half, it has no required dependencies, and it is MIT. |
| Inline scripts are Unicode when complete, ASCII otherwise | Never a lookalike glyph. §5. |
| Tall delimiters are box drawing | The characters mdmost already draws with. §6.4. |
| Big operators are single characters | §6.3. |
| Two config keys, `math` and `math_inline` | §12. |
| A formula is atomic for selection | Which is not a new rule. §10. |

**Not in scope.** Terminal image protocols. Numbered equations and `\tag`. Anything
outside math mode — no `\section`, no `\begin{document}`, no LaTeX document rendering.
The unit of work is one formula.

## 2. The dependency

`pulldown-latex` 0.8.0, MIT, edition 2021, MSRV 1.74.1. Its `[dependencies]` section
is empty; its one optional `regex` feature stays off, so the crate adds no transitive
dependency and no C toolchain. That matters here for the reason recorded against
`syntect` in `Cargo.toml`: the build must stay a pure-Rust static musl build.

It supplies the lexer, the macro expander (`\newcommand`, `\def`), the symbol tables,
the environments and error recovery, as a pull parser over `Event`s. Two properties of
that event stream are load-bearing downstream:

- Symbols arrive **already resolved to a `char`**. There is no symbol table in this
  crate, and there must not be one — a second table would drift.
- `ScriptPosition` distinguishes `Right` from `AboveBelow` and `Movable`. That is the
  "limits over a `∑`, beside an `∫`" decision, made upstream and taken as given.

Two properties it does **not** have, both of which shape the design:

- `MacroContext` is private and built fresh per `Parser`. Macros do not cross
  formulas. §16.
- Events carry no source positions. `Content::Ordinary { content: char }` is a
  character with no byte range. §10.

## 3. Syntax and parsing

`Doc::parse` enables two comrak extensions when `math` is on (`src/doc/convert.rs:16`):

- `math_dollars` — `$…$` inline and `$$…$$` display
- `math_code` — `` $`…`$ `` inline and ```` ```math ```` fenced display

`NodeValue::Math` is currently flattened to `NodeKind::Text` at
`src/doc/convert.rs:496`. That line is replaced by a real
`NodeKind::Math { literal, display, dollars }`, carrying the node's `SourceSpan` as
every node does.

**Parsing is conditional on `math`, and only on `math`.** This is the one place the
document tree depends on configuration, and it is deliberate: `math = false` must mean
a document parses exactly as it does today, so that no reader who never wanted this can
have a `$` change the shape of anything. `math_inline` acts later, in the renderer, and
never changes the tree.

False positives are comrak's problem and comrak has already solved it, with Pandoc's
heuristics (`src/parser/inlines.rs:2131`): no space after an opening `$`, no space
before a closing `$`, and a closing `$` may not be followed by a digit. `costs $5 and
$10` is not math. `$PATH is under $HOME` is not math. No heuristic of our own is
written, and none should be added later without a corpus showing the need.

## 4. The box model

Three numbers per box, all in cells:

- `width` — display columns
- `above` — rows above the baseline
- `below` — rows below the baseline

The baseline is the row the surrounding text sits on. Composition is then arithmetic:

```
horizontal list   width = Σ widths     above = max(above)    below = max(below)
fraction          the rule row IS the baseline:
                    above = num.above + num.below + 1
                    below = den.above + den.below + 1
superscript       the script's lowest row sits one above the base's baseline
subscript         the script's highest row sits one below it
limits            stacked above and below the operator; the operator keeps the baseline
```

`\frac{-b \pm \sqrt{b^2-4ac}}{2a}` is `above = 2`, `below = 1`, and:

```
row -2    ──────────────
row -1    -b ± √b² - 4ac
row  0    ──────────────      baseline: the fraction rule
row +1          2a
```

**Inline mode is this same tree under one constraint**: `above == 0 && below == 0`. A
box that cannot meet it rewrites itself rather than being rendered by other code. There
is one engine, and `build.rs` consults one flag.

## 5. Inline mode

### 5.1 Scripts

A script group is written with Unicode superscripts or subscripts **only when every
character in that group has a real Unicode form**; otherwise the whole group falls back
to `^` or `_`, with braces kept where they disambiguate.

```
$x^2$      →  x²        $x_b$      →  x_b       no subscript b exists
$x_i$      →  xᵢ        $x^q$      →  x^q       no superscript q exists
$x^{n+1}$  →  xⁿ⁺¹      $a_{bc}$   →  a_{bc}    neither b nor c exists
$a_{ij}$   →  aᵢⱼ
```

All-or-nothing per group, never per character. The alternative — Unicode wherever it
fits — produces `a_b c` for `a_{bc}` and `x²q` for `x^{2q}`, which read as different
expressions. A rendering that can be read wrongly is worse than one that is plainly not
typeset.

The table of available scripts is asserted in the crate: every codepoint it claims must
exist and must measure one column under `unicode-width`. That is the `glyphs.rs` parity
rule applied to a new table, and the reason is the same one recorded there — a glyph
whose drawn width and measured width disagree will break the layout.

Nothing here is a font survey. Whether the reader's font *has* `ᵢ` is not knowable from
inside this process and is not asked; what is asserted is that the codepoint exists and
that Unicode says it is one column wide.

### 5.2 Structures

```
\frac{a}{b}        →  a/b, parenthesised when either part is not a single atom
\sqrt{x}           →  √(x), parentheses dropped for a single atom: √x
\sum_{i=1}^{n}     →  ∑ᵢ₌₁ⁿ  by §5.1; ∑_{i=1}^{n} when a character is missing
\begin{pmatrix}…   →  not representable in one row: §9
```

### 5.3 When inline math is off

`math_inline = false` renders the node's `SourceSpan` verbatim, dollars included, in
body style. `$E = mc^2$` shows as `$E = mc^2$`.

This costs one thing and it is documented rather than engineered around: the content
was still parsed as math, so it is no longer Markdown. `$a *b* c$` shows a literal
`*b*` and not an italic `b`. A reader who wants the old behaviour exactly sets
`math = false`, which is what that key is for.

## 6. Display mode

### 6.1 Fractions

The rule spans the wider part and is the baseline. Both parts are centred over it.

### 6.2 Radicals

An overline over the radicand. A one-row radicand takes `√` plus a rule; a taller one
grows a `╱` stroke, extended with `│` for each further row.

```
 ────────          ╱────
√ b² - 4ac        ╱  a
                 ╲  ───
                  ╲╱ b
```

### 6.3 Big operators

Single characters — `∑ ∏ ∫ ∮ ⋃ ⋂ ⋀ ⋁` — with limits stacked above and below according
to the event's `ScriptPosition`.

```
   n              ∞
   ∑  i²          ∫  e⁻ˣ dx
  i=1             0
```

Not a drawn multi-row `∑`. A drawn operator costs four rows, is right at exactly one
size, and puts a shape of our own invention where a standard character exists. The
single glyph sits on the math axis and is one column wide everywhere.

### 6.4 Tall delimiters

Built from box drawing, with light arcs standing in for round parentheses:

```
 ╭      ╮      ┌      ┐      │      │      ╭      ╮
 │ 1  0 │      │ 1  0 │      │ a  b │      ┤ x, y ├
 ╰      ╯      └      ┘      │ c  d │      ╰      ╯
```

These are the characters the frames, tables and quote bars already draw with, so they
inherit that coverage and need no separate argument. The Unicode bracket-piece block
(U+239B–U+23AD) was considered and rejected: it is the block designed for the job, and
it is thinner on the ground in terminal fonts than box drawing is.

`\left` and `\right` size to the enclosed box. A one-row content takes the plain
character, not a one-row box: `(x)`, never `╭x╮`.

### 6.5 Grids

Matrices, `cases`, `align`, `alignat`, `gathered` and `array` are one mechanism:
columns with per-column alignment, negotiated widths, an optional vertical rule where
`array`'s column specification asks for `|`, then a delimiter wrapped round the
outside. The width negotiation is the one `src/render/table.rs` already performs.

```
 ╭          ╮        ╭ x²   if x > 0
 │ 1  0   0 │        ┤
 │ 0  cos θ │        ╰ 0    otherwise
 ╰          ╯
```

## 7. Width, centring and overflow

Display math does not line-break. TeX will not break a formula and neither will this;
a wide formula is a wide block.

- **Fits the measure** → centred.
- **Wider than the measure** → left-aligned at its natural width, and side-scrolled by
  the mechanism `src/render/diagram.rs` already provides. `MathError::TooWide` plays
  the part `MermaidError::TooNarrow` plays for diagrams, and the caller's `Limits`
  ceiling applies unchanged: past it, the source dump wins, because scrolling a reader
  a hundred columns to the right is not reading.

One difference from diagrams, and it is a simplification. A diagram can be re-laid out
narrower, so `diagram.rs` searches for the narrowest width that works. **A formula has
exactly one width.** The math path measures once and never probes, so `bridge::math`
needs no counterpart to `MERMAID_LAYOUTS`.

## 8. Module layout

```
src/math/
  mod.rs        render_math(src, width, &Theme, Mode) -> Result<Canvas, MathError>
  boxes.rs      the box model of §4
  build.rs      pulldown-latex events -> box tree; owns the inline constraint
  atoms.rs      symbols to cells; the script table of §5.1
  delim.rs      tall delimiters
  grid.rs       §6.5
  draw.rs       box tree -> Canvas
```

`src/render/bridge.rs` gains one function beside `mermaid()`. It stays the only place
`render` calls a foreign renderer.

## 9. Failure

Nothing about math is fatal and nothing about it may panic the pager.

| where | what the reader sees |
|---|---|
| display | The source as a framed, syntax-highlighted code block with the reason in its bottom edge — the path at `src/render/code.rs:610`, reused unchanged. |
| inline | The verbatim source with its dollars: §5.3's rendering, reached for a second reason. |

`ParserError` carries a position, so a caption can name what and where. An inline
formula that is well-formed but not representable in one row — a matrix — is a failure
of this kind, not a special case: it takes the inline fallback.

## 10. Selection, search and copy

**A formula is a diagram with no labels**, and the rules of `2026-08-11-semantic-selection-design.md`
§2.2 then decide everything without amendment.

That section's third case reads: *a drag pressed outside any label takes the whole
diagram immediately.* A math canvas has no labels at all, because `pulldown-latex`
events carry no source positions and so no drawn atom can name the bytes that produced
it. Every press is therefore that case. A drag anywhere inside a formula copies the
whole thing — `$$` fences, ```` ```math ```` fences or `$` delimiters included — and
washes the whole rectangle.

Consequences, all inherited rather than invented:

- The formula carries **one** `SearchSpan`, its own source range, with `unit` set to
  the same range.
- The container-prefix rule applies: a `$$` block inside a block quote copies as clean
  LaTeX, with `> ` removed from every line.
- A search hit anywhere in the LaTeX highlights the whole formula. Searching `alpha`
  lights the formula containing `\alpha`, whose drawn cell says `α`.

This is not an exemption from the byte-for-byte span rule. It is the atom case, which
that rule already carves out, applied to an atom with no interior.

## 11. Theme

A `math: MathStyles` slot beside `diagram: DiagramStyles`, with two entries and not
thirteen:

- `atom` — the symbols, following body text
- `rule` — fraction bars, delimiters, radical strokes, the overline

The split is the argument `heading_number` makes at `src/theme/mod.rs:352`. The symbols
are what the author wrote; the structure is what mdmost drew, and a reader has to be
able to tell at a glance. Both built-in themes and `tests/theme_contrast.rs` gain the
two entries.

## 12. Configuration

| key | flag | default | acts at |
|---|---|---|---|
| `math` | `--math` / `--no-math` | `true` | `Doc::parse`. Off means the comrak extensions are never enabled and `$` is ordinary text. |
| `math_inline` | `--math-inline` / `--no-math-inline` | `true` | the renderer. Off means §5.3; `$$` blocks and ```` ```math ```` fences still render. |

Two keys and not one because comrak's `math_dollars` covers `$` and `$$` together, so
"inline off, display on" cannot be a parser setting. Both are plain `bool` — neither
has the tri-state `icons` needs, because neither is detected.

Both names go in the key list at `src/config.rs:577` and in the generated file written
by `src/config/write.rs`. Named `math_inline` rather than `inline_math` so the pair
sorts together in that file.

## 13. The glyph inventory

`tests/glyph_inventory.rs` pins every non-ASCII character the renderer *adds*, against
the manual's terminal-setup list, by subtracting the characters already present in the
source. Its principle is that a character the author supplied is the document's, not
mdmost's.

Math fits that principle and extends it: **an author who writes `\alpha` asked for `α`
just as surely as one who typed `α`.** So the symbols are the document's, and only the
structure is ours — the fraction rule, the delimiter pieces, the radical strokes, the
overline, a handful of box-drawing characters most of which are already listed.

The test changes in one way: the subtraction becomes "characters produced from a math
node", not only "characters present verbatim in the source". The manual then claims
*blocks* for math — Greek and Coptic, Mathematical Operators, Supplemental
Mathematical Operators, Letterlike Symbols, superscripts and subscripts — and
codepoints for everything else, as it does now.

## 14. Testing

| test | what it pins |
|---|---|
| `tests/corpus/math.md` through `insta` | the rendering itself, as diagrams are pinned |
| `check_invariants()` after every math canvas | every row is exactly `width` columns |
| proptest over arbitrary input to `render_math` | never panics, never violates an invariant |
| the script table of §5.1 | every claimed codepoint exists and measures one column |
| `tests/glyph_inventory.rs` | §13's amended subtraction |
| a `math = false` snapshot | a document with `$` in it renders byte-identically to today |

The last one is the regression that matters most to a reader who did not ask for this.

## 15. Phasing

Three implementation plans against this one spec, each shippable alone.

1. **Plumbing and inline.** The two config keys, the comrak options, `NodeKind::Math`,
   `atoms.rs`, the script table, one-row layout, §5.3's fallback, §9's failure path,
   the proptest. Ships `E = mc²`.
2. **Display core.** `boxes.rs`, `build.rs`, `draw.rs`, fractions, radicals, scripts,
   big operators, tall delimiters, centring, §7's overflow and side-scroll, the framed
   source fallback, §10's span, §11's theme slots.
3. **Grids and the tail.** `grid.rs`: matrices, `cases`, `align`, `alignat`,
   `gathered`, `array` column specifications. Then `substack`, accents,
   over- and underbraces, colour, and §16's macro preamble.

## 16. Open item

**`\newcommand` does not cross formulas.** `MacroContext` is private and built fresh
per `Parser`, so a macro defined in an early block is unknown to a later one. Documents
do rely on defining macros once at the top.

The fix needs no upstream change: collect `\newcommand` and `\def` from earlier math
nodes in document order and prepend them as a preamble to each later formula's source.
It costs one owned `String` per formula. It belongs in stage 3, and the ordering rule —
a macro is visible to formulas after the one that defines it, and not before — has to
be written down where a reader will find it, because it is not TeX's rule.
