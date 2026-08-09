# Visual review 2 — independent eyes

Reviewed tree: `2aa5d8a` ("test: the canvas contract is checked on assembled rows, not just cells"),
clean working tree, primary checkout `/home/oetiker/checkouts/mdmost`.
Built with a private target dir; driven live in tmux at 40 / 57 / 80 / 100 / 120 columns,
both built-in themes, `--icons` (default) and `--no-icons`.
Colours read from `tmux capture-pane -pe` with a state-carrying SGR parser.

## Verdict

**Yes** — I would be happy to look at this every day.

The everyday reading path (headings, paragraphs, lists, quotes, tables, code) is genuinely
well made, and the colour work in both themes is better than most terminal Markdown tools.
The defects below are real and several are ugly, but they cluster in diagrams and in one
one-column layout slip, not in the daily reading experience.

---

## Findings, worst first

### 1. State-diagram edge labels are bisected by crossing wires — MAJOR

Width-invariant: identical at 40, 57, 80 and 120 columns, both themes.
`probes/mermaid.md`, `stateDiagram-v2`, captured at 80 dark, `--no-icons`:

```
                      ╭───────╮              ╭────────╮
                      │ Ready │              │ Failed │
                      ╰─┬─┬─┬─╯              ╰────┬───╯
                        │ ▲ │                     │
                       ╭╯ ╰─┼╮                    │
                       │    ╰┼────────────────────┤
                       │press│/                   │quit
                       │     │escape              │
                       ▼     │                    │
                    ╭────────┴──╮                 ▼
                    │ Searching │                 ◉
                    ╰───────────╯
```

Column ruler (80-col frame): the wire runs down column 29. The label for
`Ready --> Searching : press /` is written starting at column 24 and the wire
overwrites it, so the rendered text is `press│/` — the label is cut in half and the
slash is orphaned on the far side of an unrelated line. One row below, `escape`
(the `Searching --> Ready` label) is printed hard against that same column-29 wire,
which is *not* its own wire, so it reads as belonging to the wrong edge.

Why it is wrong: a label that a wire runs through is not a label any more. The reader
has to reconstruct "press /" from two fragments separated by a vertical bar that means
something else. Everything else in the diagram families is legible; this is the one place
where the output is actually unreadable.

Severity: high. Any document with a state machine that has a back-edge hits it.

### 2. Two opposite-direction edges are drawn as one line — MAJOR

`tests/corpus/adversarial.md`, flowchart, 80 dark:

```
                                    ┌───────┐
                                    │ Start │
                                    └─┬───┬─┘
                                      │   ▲
                                      ╰─╮ │
                                        ├─╯
                                        │no
                                        ▼
                                   ╱────┴───╲
                                  │ Decision │
                                   ╲────┬───╱
```

`Start` has two ports, at columns 38 and 42. The outgoing edge leaves at 38 and jogs
right to column 40; the incoming edge arrives at 42 and jogs left to column 40. From
row `├─╯` downward, **both edges occupy column 40 for their entire length** — a single
`│` carrying one `▼` at the bottom and one label `no`. There is no way to see from the
picture that two edges exist, nor which one the `no` belongs to.

Why it is wrong: this does not merely look untidy, it misinforms. A diagram that shows
one edge where the source declares two is worse than showing the source. Contrast with
the `Ready`/`Failed` case above, where at least the trunk merge is visible.

Severity: high for correctness of reading, medium for how often it fires (needs a cycle).

### 3. Live TUI has no gutter on either side — MEDIUM, but you see it every session

Live pane, 120×12, dark, `probes/text.md`, columns 110–119 shown:

```
 0| Head...         █|
 1|━━━━━━...━━━━━━━━━│|
 3|Intro ...leanly at│|
 4|any wi...such as  │|
```

Content starts at column 0 and wraps at column 118; the scrollbar occupies column 119.
Row 3 ends `...cleanly at│` — the word `at` sits in column 117–118 and the scrollbar
glyph is in 119, with **zero blank column between them**. The same is true at 80
columns (`...without leaving orphaned█`). The H1 rule and every table/code-block border
also run flush from column 0 into the scrollbar.

There is no configuration knob for this: `README.md`'s Configuration section lists
`theme`, `icons`, `line_numbers`, `toc_open`, `toc_width`, `mouse`, `scroll_step` and the
`[themes.*]` colours — no margin or padding, and `grep -niE "margin|padding|gutter"
src/config.rs src/config/keys.rs` only finds the code-block line-number gutter.

Related and worth knowing: `--render-once --width 120` renders *with* a one-column left
margin, live rendering does not.

```
--- render-once 120, rows 0-4, cols 0-6 and 110+ ---
 0|  Hea...|
 1| ━━━━━...━━━━━━━━━|
 3| Intro...cleanly|
```

So the headless dumps the team reviews are not a pixel-faithful preview of the pane the
user gets; the two differ by exactly the left margin. Goldens will never show this defect.

Why it is wrong: for a tool that names btop as its bar, text kissing the scrollbar is the
single most "unfinished" thing in the frame, and it is present on every wrapped paragraph
of every document.

Note in fairness: **resize is pure.** Launching at 120 and resizing the live window to 57
produces a pane byte-identical to launching fresh at 57 (`diff` clean). That claim in the
README holds.

### 4. Heading levels 4, 5 and 6 are indistinguishable in the default (icon) mode — MEDIUM

Live 80 dark, `probes/text.md`, foreground RGB and attributes per heading row:

```
L00 (100,181,255)[bold] ' Heading level one'     + ━━━ rule
L08 (104,173,239)[bold] ' Heading level two'     + ─── rule
L11 (108,166,223)[bold] ' Heading level three'
L13 (112,158,207)       ' Heading level four'
L15 (115,151,191)       ' Heading level five'
L17 (119,143,175)       ' Heading level six'
```

H1–H3 are fine: rule, thinner rule, bold-no-rule. Below that the *only* cues are
(a) a colour ramp stepping 3–4 units per channel — H4 `(112,158,207)` vs H5
`(115,151,191)` vs H6 `(119,143,175)` are not separable by eye on a terminal — and
(b) the leading glyph, which in Nerd Font is `` (chevron-right), ``
(angle-double-right), `` (angle-right): three near-identical small right-pointing
chevrons. There is no indentation change and no weight change across H4–H6.

The light theme has the same structure with a slightly wider ramp
(`65,112,172` / `78,113,158` / `91,113,145`) — still too close.

Ironically `--no-icons` is *better*: `◆ ◈ ◇ ▸ ▹ ❯` are actually six distinct marks.
The default mode is the weaker one.

Severity: medium. Bites on any specification or API doc that nests past H3.

### 5. Degradation policy at narrow widths is inconsistent — MEDIUM

At 40 columns the LR flowchart refuses honestly and shows the source with a reason:

```
 ╭ mermaid ───────────────────────────╮
 │ flowchart LR                       │
 │     src[Markdown source] --> pars› │
 │     layout --> theme[Theme]        │
 │     theme --> draw                 │
 ╰ needs more than 38 columns to draw ╯
```

That is the right behaviour. But class and ER diagrams at the same width happily draw
themselves into rubble instead:

```
                     ┌──────────┐
                     │ Document │
                     ├──────────┤
                     │ +blocks… │
                     │ +title:… │
                     ├──────────┤
                     │ +parse(… │
                     │ -normal… │
                     └─────┬────┘
...
       ┌──────────┐  ┌──────────┐
       │  Table   │  │ CodeBlo… │
       ├──────────┤  ├──────────┤
       │ +rows: … │  │ +lang: … │
       ├──────────┤  ├──────────┤
       │ +widths… │  │ +highli… │
       └──────────┘  └──────────┘
```

Every member is elided to 8 characters plus `…`. `+blocks…`, `+parse(…`, `+widths…`
carry no information — you cannot tell `+parse(...)` from `+parseAll(...)`. The ER
diagram is the same (`DELIVER…`, `LINE_IT…`, `string …`, `int    …`).

Why it is wrong: the LR flowchart already establishes the correct answer — say you can't
and show the source. Rendering an unreadable box is strictly worse than both drawing it
properly and refusing, because it looks like the diagram *is* the information.

### 6. Gantt chart at narrow widths: label touches the axis, legend silently drops entries — MEDIUM

`probes/mermaid.md` at 40, dark, `--no-icons`:

```
                 01-01  02-01
                ├──┬──────┬───────────┤
                │
 Core           │
   Parser       │  ████
   Layout engine│      ███████
   Theme system │             ███
                │
 Polish         │
   Mermaid fami…│            ██████
   Docs         │                  ██
   Ship         │                    ◆

                 █ done   █ active
```

Two problems in one frame:

- `Layout engine│` and `Mermaid fami…│` abut the axis rail with no separator, while
  `Parser       │` and `Docs         │` have space. The task-name column is sized to the
  longest label with zero right padding, so the longest label always collides with the
  axis. At 80 columns there is a space, so this only shows up when it is already cramped.
- The legend is truncated to `█ done   █ active`. The chart still contains grey
  *planned* bars and a `◆` milestone, and their keys have been dropped without any
  ellipsis or hint. The `◆` on the `Ship` row is now unexplained.

### 7. Flowchart edge ports are noticeably off-centre — LOW/MEDIUM

`probes/mermaid.md`, flowchart TD, 80 dark, columns measured:

```
                     ┌────────────────┐   ┌──────────────┐
                     │ Parse document │   │ Report error │
                     └──────────────┬─┘   └───────┬──────┘
                                    ▼             │
                                  ╭───────────╮   │
                                  ├───────────┤   │
                                  │ Cache AST │   │
                                  ├───────────┤   │
                                  ╰─────────┬─╯   │
                                            ╰─╮   ╰──╮
                                              ▼      ▼
                                          ┌──────────────┐
                                          │ Render frame │
```

`Parse document` spans columns 21–38 (centre 29.5); its exit tick `┬` is at column 36.
`Cache AST` spans 34–46 (centre 40); its exit tick is at column 44. Compare `Start`
higher up, which gets a perfectly centred `└───┬───┘`. The result is that mid-graph
edges leave and enter boxes near a corner and then dog-leg (`╰─╮`, `╰──╮`), which reads
as sloppy next to the crisp top and bottom of the same diagram. Also, the two arrowheads
into `Render frame` land at columns 46 and 53 in a box spanning 42–57 — neither centred
nor symmetric.

Not wrong, just visibly less tidy than the diagram is capable of being.

### 8. Gantt axis ends with a tick jammed against the end cap — LOW

80 dark, columns marked:

```
                        01-01    01-15   01-29    02-12   02-26    03-12  03-26
                     ├────┬────────┬───────┬────────┬───────┬────────┬───────┬┤
```

Ticks sit at columns 26, 35, 43, 52, 60, 69, 77, and the end cap `┤` at 78 — a
one-column final segment reading as `┬┤`. Segment lengths otherwise alternate 9/8/9/8,
which is fine rounding, but the last one is a wart. The labels themselves *are* correctly
centred on their ticks (verified: `01-01` occupies 24–28 centred on 26, `03-26` occupies
75–79 centred on 77) — but `03-26` therefore ends in column 79, flush against the
scrollbar (see finding 3).

### 9. Empty fence renders as a hollow two-line box — LOW

```
 ╭ rust ──────────────────────────────────────────────────────────────────────╮
 ╰────────────────────────────────────────────────────────────────────────────╯
```

An empty ` ```rust ` block produces a language-tagged frame with no interior row at all.
It reads as a rendering failure rather than as "this block is empty".

### 10. The invalid-mermaid fallback truncates the one sentence that helps — LOW

`tests/corpus/adversarial.md` at 80:

```
 ╭ mermaid ───────────────────────────────────────────────────────────────────╮
 │ this is not valid mermaid at all                                           │
 ╰ not a diagram type — mdmost draws flowchart, sequenceDiagram, classDiagra… ╯
```

The caption exists precisely to tell the author which types are supported, and it is cut
off at the third one. Either wrap it to a second caption row or shorten the lead-in.

### 11. Light theme `h help` hint is under-contrast — LOW

Status bar, light theme: `(138,109,0)` on `(227,224,215)` = **3.7:1**. Below the 4.5:1
bar for text this small. Every other light-theme pairing I measured is comfortable —
body `(43,47,56)` on `(253,252,249)` is ~15:1, H6 `(91,113,145)` is ~4.9:1, the gantt
*planned* grey `(104,105,106)` is ~5.1:1.

### 12. TOC selected row has no left padding — LOW

```
╭  Contents ────────────────╮
│▸ mdmost                    │
│    What it is not          │
│    Install                 │
```

The `▸` marker on the top-level entry sits in column 1, directly against the pane border,
while every other row is inset four columns. And the TOC's right border abuts the
document text with no separating column (`│    Install                 │Quick start`
style adjacency at column 29/30) — the same missing-gutter issue as finding 3.

---

## Areas I checked and found nothing wrong

Stated explicitly so they are not re-litigated:

- **Paragraph wrapping.** Correct at 40/57/80/120. Unbreakable tokens are hard-broken at
  the margin rather than overflowing; long URLs likewise. No line in any of my 96 rendered
  frames exceeded its declared width (measured with an East-Asian-width-aware counter).
- **List indentation.** Bullet text and continuation lines align exactly. Top level:
  glyph at column 1, text at column 3, continuation at column 3. Nested: text at 5,
  continuation at 5. Four levels step by 2 consistently.
- **Ordered-list renumbering.** `10.` and `100.` render as `3.` and `4.` — that is
  CommonMark-correct (only the start number is honoured), not a bug.
- **Block quotes.** Marker on every wrapped line, nesting to three levels reads clearly,
  and quoted code fences/lists/links inside a quote all keep their marker column.
- **Task lists.** Unchecked `` in muted grey `(124,134,155)`, checked `` in
  green `(118,215,160)` — distinct by both glyph and colour, and the checkbox column
  aligns on wrapped rows.
- **Search highlighting.** Dark: `(17,20,27)` on `(255,166,87)` bold. Light:
  `(253,252,249)` on `(179,92,0)` bold. Both high-contrast and unmissable.
- **Full-pane background painting.** Both themes paint every cell of the pane, including
  rows past the end of a short document — verified against a control that confirmed
  `capture-pane -pe` carries SGR state across line boundaries. (I nearly filed a false
  "light theme inherits the terminal background" bug here; it is not one.)

---

## What genuinely looks good — do not break this

- **Tables.** The best thing in the tool. Column-width negotiation is sensible, alignment
  markers are honoured (`:---`/`:--:`/`---:` all correct), Markdown inside cells renders
  styled, and cells wrap inside their column rather than blowing out the table:

```
 ╭───────────┬───────────────────────────────────────────────────────┬────────╮
 │ Construct │ Example                                               │ Note   │
 ├───────────┼───────────────────────────────────────────────────────┼────────┤
 │ long      │ a cell whose content is long enough that the column   │ wraps? │
 │           │ must either wrap it or the table must overflow        │        │
 │           │ horizontally                                          │        │
 ╰───────────┴───────────────────────────────────────────────────────┴────────╯
```

  CJK and emoji width handling is correct — a genuinely hard thing done right:

```
 ╭──────────────────┬───────╮
 │ Text             │ Width │
 ├──────────────────┼───────┤
 │ 日本語のテキスト │ wide  │
 │ 🎉 emoji 🎉      │ mixed │
 │ combining é vs é │ equal │
 ╰──────────────────┴───────╯
```

  Overflow at 57 columns clips with a `›` marker on every row including borders, and
  `←`/`→` scroll it. That is the right answer.

- **Pie charts.** Sorted descending, fractional block glyphs for sub-cell precision,
  five clearly distinct hues, legend dot colour matching its bar, right-aligned
  percentages and a total rule. Nothing to change:

```
 ● Layout           ███████████████████████████████████████████████████   42.0%
 ● Highlighting     ██████████████████████████████▍                       25.0%
 ● Parsing          ████████████████████▋                                 17.0%
 ● Terminal I/O     ██████████▉                                            9.0%
 ● Everything else  ████████▌                                              7.0%
   ──────────────────────────────────────────────────────────────────────────
   Total                                                                 100.0%
```

- **Gantt semantics.** Bar positions are arithmetically right (Parser 20d = 12 columns
  from the `01-01` tick; `after p1` starts exactly where Parser ends), status colours are
  coded (green done / blue active / grey planned / yellow `◆` milestone) and the legend
  matches at widths where it fits. Section names in bold purple read well against plain
  task rows.

- **Sequence diagrams.** The strongest family. Dotted return arrows distinct from solid
  calls, `loop`/`alt`/`else` frames drawn as labelled boxes with a dashed `else`
  separator, self-messages as a proper little hook, participants repeated at the foot,
  and it survives 40 columns by eliding message text rather than collapsing:

```
              ╭─ alt [document changed] ───────────────────────────╮
              │ ┆          ┆        invalidate        ┆            │
              │ ┆          ┆──────────────────────────▶            │
              ├╌ else [unchanged] ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┤
              │ ┆          ┆──╮                       ┆            │
              │ ┆          ┆  │ reuse                 ┆            │
              │ ┆          ┆◀─╯                       ┆            │
              ╰────────────────────────────────────────────────────╯
```

- **Syntax highlighting.** A real palette, not a token soup: keyword `(185,155,248)`,
  type `(95,215,215)`, attribute `(242,208,107)`, comment `(124,134,155)` *italic*,
  punctuation deliberately dimmed to `(94,103,121)`. Code panels get their own tint
  (`(24,28,37)` dark, `(241,239,233)` light) inside the fence border, and the language
  tag rides the top border in accent colour. Unknown languages degrade to plain text
  without complaint.

- **The light theme is a real theme.** Not the dark palette on a white background — every
  hue is independently re-derived to a darker, more saturated variant (pie blue
  `(100,181,255)` → `(26,111,212)`, green `(118,215,160)` → `(31,122,77)`, orange
  `(255,166,87)` → `(179,92,0)`). Contrast holds throughout. This is more care than most
  TUIs take.

- **Chrome.** The status bar (icon, filename, powerline separator, percentage plus a
  proportional `░░░` gauge, current heading, `h help`) is compact and informative. The
  help overlay is well organised and admits when it is scrolled
  (`╰ ↓ 8 more — j k scroll ─╯`). The TOC pane shows nesting by indent with a `▸` marker
  and a white-on-blue selection.

- **Inline styles.** Emphasis italic, strong bold, both combined, strikethrough with a
  dimmed colour, links underlined in blue with the URL trailing in muted grey and
  middle-elided when long (`https://example.c…/that/might/wrap`) — that elision keeps both
  the host and the tail visible, which is the right choice.

- **Images.** Rendered as a captioned `╭ image ─╮` panel with alt text above target,
  degrading to target-only when there is no alt. Honest and tidy.

- **Resize purity.** Confirmed by diff, not by claim.

---

## Suggested fix order

1. State-diagram label placement (finding 1) — route labels clear of crossing wires.
2. Separate co-linear opposite-direction edges (finding 2).
3. Add one column of gutter left and right in the live renderer, and make
   `--render-once` agree with it so goldens show what users see (finding 3).
4. Give H4–H6 a real differentiator in icon mode — indent, weight, or genuinely
   different glyphs (finding 4).
5. Extend the "needs more than N columns" refusal to class and ER diagrams (finding 5).
6. Gantt: pad the label column, and never silently drop legend entries (finding 6).
