# mdmost — visual design review (harsh)

Binary: /home/oetiker/scratch/cargo-target/debug/mdmost
Reviewed at 3e17fdc, then re-verified at 6546f81 after sequence/pie/gantt mermaid landed.
B1, B2, B4, B5, B6, B7, B8 were all re-checked against the 6546f81 build and still reproduce.
Method: `--render-once` matrix at widths 40/60/80/100/120/200 × {icons,--no-icons} × {dark,light},
plus a real tmux session (`vis`) resized through 100×{10,24,30,50} and 60×30, captured with
`capture-pane -p` and `capture-pane -pe`.
Test material: docs/superpowers/specs/2026-08-08-mdmost-design.md, tests/corpus/adversarial.md,
and adversarial docs written for this review: headings.md, tables.md, blocks.md, code.md.

Caveat stated up front: this terminal has no Nerd Font. Every claim below about Nerd Font
glyphs is about *codepoints emitted* (verified by hexdump), never about how they paint.

Two brief items are not separately reported because the input format forbids them: a fenced
code block and a nested table **inside a table cell** are not expressible in GFM pipe-table
syntax (a `|` splits cells, a newline ends the row). The `<br>` proxy that people actually use
for multi-line cells *was* tested — see P5. Markdown that pipe tables do permit in cells
(emphasis, inline code, links, lists via `<br>`) was tested throughout.

`Esc` behaviour in P16 is flagged as arguable: spec §10 says Esc "quits from a bare document",
and whether a completed search makes the document non-bare is a judgement call.

---

## BLOCKING

### B1. The theme background is never painted. `--theme light` is unusable.
`base()` in src/theme/mod.rs:367 returns `self.text.body`, which carries a foreground only.
Consequence: the document area inherits the *terminal's* background, while elements that set
their own bg (headings, code blocks, status bar) paint the theme's.

    tmux new-session -d -s vis -x 100 -y 30 ; tmux set-option -t vis window-size manual
    tmux resize-window -t vis -x 100 -y 30
    mdmost --no-icons --theme light headings.md
    tmux capture-pane -t vis -pe

    ^[[1m^[[38;2;26;111;212m^[[48;2;253;252;249m◆ H1 Alpha …          <- H1 row: bg #FDFCF9 (light)
    ^[[38;2;43;47;56mBody under H1. This paragraph …       <- body: fg #2B2F38, NO bg

On a dark terminal the light theme gives you a white stripe under every heading and
near-black body text on black — i.e. invisible. Code blocks stay readable (they paint
`#F1EFE9`), so the page reads as islands of light floating in the dark.
Even with the *dark* theme on a non-matching terminal you get banding: heading rows are
`#11141B`, body rows are whatever the user's terminal is.
**Should be:** fill the whole viewport (and the TOC pane, and the overlay) with `palette.bg`
every frame. A theme that only paints half its elements is not a theme.

### B2. Every heading prefix glyph is painted in one fixed accent, not the heading's colour.
src/render/block.rs:141 uses `theme.block.heading_prefix` — a single Style for all six levels.

    mdmost --no-icons h3.md ; tmux capture-pane -t vis -pe

    ^[[1m^[[38;2;100;181;255m^[[48;2;17;20;27m▸ ^[[38;2;118;215;160mH3 first
    ^[[1m▹ ^[[38;2;242;208;107mH4                      <- ▹ inherits #64B5FF
    • ^[[38;2;255;166;87mH5                        <- • inherits #64B5FF
    · ^[[38;2;185;155;248mH6                        <- · inherits #64B5FF

The bullet is blue #64B5FF while the text next to it is green / yellow / orange / purple.
The one visual element that is supposed to encode level encodes nothing.
**Should be:** the prefix takes the heading's own style (or a systematically derived tint of it).

### B3. Heading markers and list markers collide — the glyph vocabulary is not disjoint.
src/render/glyphs.rs:36 / :50.

    PLAIN  heading = ["◆","◈","▸","▹","•","·"]   bullets = ["•","◦","‣","·"]
    NERD   heading = [f0c8,f096,f111,f10c,f0da,f105]
           bullets = [f192,f1db,f0da,f105]   task_unchecked = f096

`•` is both H5 and a top-level list bullet; `·` is both H6 and a depth-4 bullet. In the Nerd
set it is worse: `f096` is *both* the H2 prefix and the unchecked-task box, and `f0da`/`f105`
are H5/H6 *and* list depths 3/4. Verified side by side:

    mdmost --render-once --width 80 --no-icons headings.md  ->  "• H5 Echo — deeper still"
    mdmost --render-once --width 80 --no-icons blocks.md    ->  "• level 1"

Identical marker, opposite meaning. **Should be:** two disjoint families — filled/geometric
for headings, small/round for bullets — and *nothing* shared with task boxes.

### B4. Horizontal scrolling does not work, but the UI promises it does.
`hscroll_max()` (src/tui/app.rs:309) = `canvas.width() - viewport_width()`, and the canvas is
rendered *at* viewport width. So it is always 0 unless `--width N` exceeds the terminal.

    mdmost --no-icons tables.md      # 100×30 tmux
    /Column number <Enter>           # jumps to the over-wide table
    <Right> ×10                      # nothing moves

    ╭────────────────────────────────────┬──────────────────────────────┬─────────────────────────────›│
    │ Column number one with a long      │ Column number two with a     │ Column number three with a l›│
    │ header                             │ long header                  │                             ›│

The `›` markers advertise content that is permanently unreachable. Spec §7.3 explicitly
requires the table to become horizontally scrollable instead of being mangled; it is mangled.

### B5. The horizontal-offset indicator never appears.
Even in the one case where hscroll *does* move (`--width 200` on a 100-col terminal), the
status bar shows nothing, contradicting spec §10 and src/tui/chrome.rs:190.

    mdmost --no-icons --width 200 tables.md ; /Column number <Enter> ; <Right>×12
    (content visibly shifts from "Column number one" to "number two with a long")
     ▤ tables.md │  25% ██       │ § Empty-ish table                       ⌕ Column number 1/5 │ h help
                                   ^ no  → 40/100  chip anywhere

There is also no `‹` marker on the left edge once scrolled, so both the fact and the direction
of the offset are invisible.

### B6. The help overlay silently truncates. At 100×30 you cannot see `q`.
    mdmost --no-icons docs/superpowers/specs/2026-08-08-mdmost-design.md   # 100×30
    h

    │  View                                                    │
    │                  t  Switch to the next theme             │
    │               h F1  Show or hide this help               │
     ▤ 2026-08-08-mdmost╰──────────────────────────────────────╯    h help

The box is cut at the screen edge. `Esc` and `q` — the two keys a stuck user needs — are below
the fold, with no scroll indicator, no paging, no "more" hint. At 100×10 it shows 8 of ~32 rows.
It only fits at height ≥ 50. **Should be:** the overlay scrolls (and says so), or reflows to a
two-column layout, or drops to a one-line key strip when the height is small.

### B7. The help overlay has no scrim; its borders fuse with the document behind it.
Same repro, at 100×50:

    │ Mermaid families  │              Enter  Jump to the selected heading         │ate               ││
    │ Terminal floor    │                  t  Switch to the next theme             │ith --no-icons    ││
    ╰───────────────────│             Ctrl-r  Switch literal / regex search        │──────────────────╯│
                        │                                                          │                   │
    ◈ 3. The central arc│  View                                                    │                   │

The document's table borders run straight into the overlay's border and out the other side.
The overlay reads as a hole punched in a broken frame, not as a floating panel.
**Should be:** dim the backdrop (reduce to a single muted fg, or overlay a bg wash) and give the
panel a 1-cell shadow or margin.

### B8. Footnote references are all rendered `[1]`.
    mdmost --render-once --width 80 --no-icons blocks.md

    Here is a footnote reference[1] and another[1].      <- source was [^1] and [^long]
    …
    [1] The first footnote.
    [long] A much longer footnote …

Two different references collapse to the same label, and the `[long]` definition has no
reference pointing at it. Numbering is not just ugly, it is wrong.

---

## POLISH (still embarrassing, individually cheap)

### P1. Nothing has a margin. Everything is jammed against the scrollbar.
At any width, body text, table borders and code frames end at column `width-1` and the
scrollbar occupies column `width`. There is no left margin either — text starts at column 0.

    │ 日本語のテキストが入っている長いセルです                     │ mixed 幅 test                    │█
    │ 短い                                                         │ ok                               │█

btop breathes. This does not. One column of gutter on each side would do most of the work.

### P2. Code-block content has no left padding; tables have it on both sides.
    mdmost --render-once --width 80 --no-icons code.md

    │use std::collections::HashMap;                                                │
    │    let mut m: HashMap<&str, i32> = HashMap::new();                           │

vs a table row `│ Left       │`. The right side *is* padded (content stops one cell short of the
border), so the asymmetry is visible as a lean. Turning line numbers on accidentally fixes it:

    mdmost --render-once --width 80 --no-icons --config ln.toml code.md   # line_numbers = true
    │ 1 │ use std::collections::HashMap;                                           │

so the intended look already exists — it is just not applied in the default case. Also the
line-number gutter `│` does not join the frame (`┬`/`┴` missing at top and bottom).

### P3. Tables always expand to the full width, however little they contain.
    mdmost --render-once --width 80 --no-icons tables.md

    ╭──────────────────────────────────────────────────────────────────────────────╮
    │ Only                                                                         │
    ├──────────────────────────────────────────────────────────────────────────────┤
    │ a                                                                            │
    │ b                                                                            │
    ╰──────────────────────────────────────────────────────────────────────────────╯

A one-column table with the content "a"/"b" becomes an 80-column void. Spec §7.2 says grow
columns *toward* natural width — natural width here is 4. **Should be:** cap at natural width,
grow to full width only when the content asks for it.

### P4. An empty table renders as a mouth with no teeth.
    ╭───────────────────────────────────────┬──────────────────────────────────────╮
    │ A                                     │ B                                    │
    ╰───────────────────────────────────────┴──────────────────────────────────────╯

Header rule dropped when there are no body rows, so the header and the bottom border touch.
Combined with P3 it is 80×3 of nothing. Either draw the header rule anyway, or render a
compact "(no rows)" line inside.

### P5. `<br>` — the commonest thing in a GFM table cell — becomes a visible `⟨html⟩` chip.
    │ list      │ - one⟨html⟩- two                                                 │
    │ block     │ ⟨html⟩see below                                                  │

No spacing, no dimming, and it butts straight into the surrounding words. Spec §2 says HTML is
"not rendered, not passed through" — a literal `⟨html⟩` token *is* passing it through, badly.
Either drop it silently or render it as a dim, spaced marker.

### P6. Column negotiation is visibly lopsided.
    ╭───────────┬──────────────────────────────────────────────────────────────────╮
    │ feature   │ detail                                                           │
    │ nested    │ see the doc                                                      │
    │ table     │                                                                  │

"nested table" (12 cols natural) got 9 and wrapped, while the neighbour column carries dozens
of trailing blanks on every row. In the alignment table the three columns come out 26/25/25
with identical content in each — a 1-column stagger you can see.

### P7. Link URLs are dumped in full inside cells.
    │ link      │ example                                                          │
    │           │ (https://example.com/a/very/long/url/that/keeps/going/and/going) │

One link eats an entire table row. Elide the middle, or suppress the URL in narrow contexts.

### P8. TOC entries are hard-cut mid-word with no ellipsis, and have no right padding.
    ╭ ≡ Contents ────────────────╮
    │▸ mdmost — design spec      │
    │    3. The central architect│
    │      6.2 sequenceDiagram — │

The document area uses `›` for truncation; the TOC uses nothing. Text touches the border.
Indent step is 4 for level 2 then 2 for level 3 — inconsistent. The pane is a fixed ~30
columns (30 % of a 100-col terminal) and there is no gap between its border and the document:
`╮◆ mdmost — design spec`.

### P9. The status-bar meter has no trough and punches a different background into the bar.
    2%    ▤ file.md │   2% ▏        │ § …
    46%   ▤ file.md │  46% ███▊     │ § …
    100%  ▤ file.md │ 100% ████████ │ § …

Credit: the eighth-block sub-cell precision spec §9 asks for is there (`▏`, `▊`). Two problems
remain. There is no track glyph, so at low percentages the unfilled cells are blank — at 0 % the
meter is eight empty cells and reads as a hole, not a gauge. And those cells are painted
`#11141B` (page bg) inside a `#222836` status bar, so the gauge is a visibly darker rectangle
punched into the bar rather than a component of it.

### P10. `h help` at the far right of the status bar has no background.
    ^[[38;2;242;208;107mh help          <- fg only; bg falls back to the terminal's

Six columns of the bar are a different colour from the rest, in both themes. In the light theme
it is `#8A6D00` on the terminal's own background.

### P11. The status bar duplicates the match count and truncates mid-word when narrow.
    ▤ tables.md │   0%          │ match 1/13                    re  ⌕ c.l 1/13 │ h help

`match 1/13` and `⌕ c.l 1/13` say the same thing twice, 30 columns apart, with a `re` chip and a
double space between. At width 60 the whole tail is chopped without an ellipsis:

    ▤ 2026-08-08-mdmost-design.md │  11% ▉        │ § 3. The ce

### P12. The mermaid fallback caption is a stray log line, not a caption.
    ╰──────────────────────────────────────────────────────────╯
    unsupported mermaid syntax: unsupported diagram type `flowchart`

Dim italic `#7C869B` is right; the placement is not. Flush at column 0, glued to the bottom
border, wider vocabulary repeated twice in one sentence ("unsupported … unsupported"), and for
genuine garbage it echoes the first word of the input: ``unsupported diagram type `this` ``.
Put it inside the frame's bottom edge (where the language label sits on top), or indent it to
the box interior, and write a message that means something.

### P13. The image placeholder prints "image" twice when alt text is empty.
    ╭ image ───────────────────────────────────────────────────────────────────────╮
    │image                                                                         │
    │empty-alt.png                                                                 │
    ╰──────────────────────────────────────────────────────────────────────────────╯

### P14. Thematic breaks and the H2 rule are the same glyph at the same width.
`---` renders as a full-bleed `─` run; so does the rule under every H2. You cannot tell a
section rule from a thematic break. Inset the thematic break, or centre a `◈`/`···` on it.

### P15. Deep block quotes spend 8 columns on gutters.
    ▌ ▌ ▌ ▌ level four quote

At width 40 that is 20 % of the line. Tint the bar by depth instead of stacking bars.

### P15b. The TOC filter is sticky, and its results are unadorned.
    Tab ; / ; type "merm"

    ╭ ⌕ merm ────────────────────╮
    │▸   6. Mermaid subset — acce│
    │                            │
    │  … 27 more empty rows …    │
    ╰────────────────────────────╯

Working, and retitling the pane to `⌕ merm` is a nice touch. But: the matched characters are not
highlighted (it is sold as a *fuzzy* filter — show what matched); the entry keeps its stale
tree indentation in a now-flat list; there is no "1 of 25" count; the pane does not shrink, so
one result sits above 27 rows of empty box; and the filter **persists across `Tab` close/reopen**
with no visible way to clear it — a user who filters once has a permanently one-item TOC.
Also: `Enter` while the filter prompt is still open confirms the filter instead of jumping; a
second `Enter` is needed. Without a filter, `j`/`k` + `Enter` jump correctly, and `n`/`N` step
matches correctly.

### P15c. Status-bar segments collide with no separator.
    ▤ 2026-08-08-mdmost-design.md │  46% ███▊     │ § 6. Mermaid subset — acceptance criteria⌕ Canvas 3

The heading segment runs straight into the search chip — no space, no `│` — and `h help` is
pushed off the end entirely.

### P16. Search does not centre the match, and `Esc` quits instead of clearing the search.
`/Column number <Enter>` left the match on the *last* visible row with zero following context.
Pressing `Esc` afterwards exited the program rather than dismissing the search state.

### P17. `--render-once` prints errno spam and exits 1 on SIGPIPE.
    mdmost --render-once --width 80 tests/corpus/adversarial.md | head -3
    …
    mdmost: Broken pipe (os error 32)      # exit 1

`mdmost x.md | head` is the normal `$PAGER` idiom. Exit 0 quietly.

### P18. Icons are on by default with no capability detection.
The default look on a terminal without a Nerd Font (i.e. most terminals):

    mdmost blocks.md

     Blocks
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Deep list nesting
    ─────────────────────────────────────────
     level 1
       level 2
         level 3
     󰈙 blocks.md    0%            Blocks   h help

Every heading marker, every bullet, every task box and every status-bar separator is a hole.
A nested list becomes indentation with no markers at all; done and not-done tasks become
identical. The codepoints *are* emitted (hexdump confirms `ef 83 88` = U+F0C8), so this is a
font problem, not a rendering bug — but shipping it as the default, with no `NERD_FONT`/
terminal check and no fallback, means most first runs look broken. Separately: several of the
chosen glyphs are rendered double-width by some Nerd Font patches, which would shift every
following column; the code assumes width 1.

---

## MERMAID (re-reviewed at 6546f81, after sequence/pie/gantt landed)

Repro for all of this: `mdmost --render-once --width 80 --no-icons mermaid.md`, and the same
file in tmux at 100×40 with `capture-pane -pe`.

### Pie — the best-looking thing in the program. Two nits.
```
                              Where the time goes

● Rendering        █████████████████████████████████████████████████████   45.0%
● Parsing          █████████████████████████████▌                          25.0%
● Layout           ███████████████████████▌                                20.0%
● Everything else  ███████████▊                                            10.0%
  ──────────────────────────────────────────────────────────────────────────────
  Total                                                                   100.0%
```
Sorted descending (verified with deliberately unsorted input), eighth-block sub-cell precision
(`▌`, `▊`), legend dot colour matches its bar (`#64B5FF`/`#76D7A0`/`#FFA657`/`#B99BF8`), long
labels elided with `…`. This is what the rest of the program should look like.

- **M1.** The one-slice pie drops the rule and the `Total` row that every other pie has. In a
  document with several pies that inconsistency is visible. Keep the chrome or drop it always.
- **M2.** The `Total` rule is indented 2 on the left and full-bleed on the right. Same
  no-margin problem as P1.
- **M3.** The chart title is bold `#D6DBE5` — the exact colour of body text. It does not read
  as a title, it reads as a bold sentence.
- **M4.** Elision uses `…` here but the TOC hard-cuts mid-word (P8) and code/tables use `›`.
  Three different truncation idioms in one program.

### Sequence — structurally right, tonally wrong.
```
     ╭───────╮                  ╭─────╮                          ╭───────╮
     │ Alice │                  │ Bob │                          │ Carol │
     ╰───────╯                  ╰─────╯                          ╰───────╯
         ┆        Hello Bob        ┆                                 ┆
         ┆─────────────────────────▶                                 ┆
         ┆        Hi Alice         ┃
         ◀╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┃
         ┆                         ┆  No                             ┆
         ✗╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┆
         ┆                        ┌───────────────────────────────────┐
         ┆                        │ A note spanning two participants  │
         ┆                        └───────────────────────────────────┘
       ╭─ loop [Every minute] ─────────────────────────────────────────╮
       │ ┆          Ping           ┆                                 ┆ │
       ╰───────────────────────────────────────────────────────────────╯
       ╭─ alt [is well] ───────────────────────────────────────────────╮
       │ ┆                         ┆              Good               ┆ │
       ├╌ else [is not well] ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┤
       ╰───────────────────────────────────────────────────────────────╯
```
Credit first: lifelines hold their columns (9/35/69) **inside and outside** the frames — I
checked programmatically, expecting a jog, and there is none. Activation bars (`┃`), the `-x`
terminator (`✗`), the dashed `╌` for `-->>`, the `├╌ else ╌╌┤` divider, and repeating the
participant boxes at the foot are all correct and mermaid-faithful.

- **M5. Three box vocabularies in one diagram.** Participant boxes and block frames are rounded
  (`╭╮╰╯`); note boxes are square (`┌┐└┘`). Spec §7.5 says rounded. Pick one.
- **M6. Message labels have no alignment rule.** `Hello Bob` is centred over its span; `No` is
  parked two cells right of Bob's lifeline; `Perhaps` is flush against a lifeline with no space
  (`┆Perhaps`); `One message` is flush against the source lifeline. Four labels, four rules.
- **M7. The labels are the faintest ink in the diagram.** Message text is `#7C869B`, the
  message line is `#707784`, and the lifelines are `#39414F` — dimmer than the H1 rule I
  already flagged as invisible. In a sequence diagram the labels *are* the content; here they
  are quieter than the lines they sit on. Invert it: labels at body weight, lines dim.
- **M8. Two different blues on one diagram.** Participant boxes are `#7AA2F7` (the code-block
  *function* colour) while arrowheads are `#64B5FF`, with cyan `#5FD7D7` activation bars. That
  reads as an accident, not a decision.
- **M9. Self-messages are cramped.** `┆──╮` / `┆  │ Talking to myself` / `┆◀─╯` — a 2-cell loop
  with the label hanging outside it at an arbitrary offset.

### Gantt — the weakest of the three.
```
           01-01    01-08     01-15      01-22      01-29      02-05     02-12
          ├───────────┬─────────┬──────────┬──────────┬──────────┬─────────┬────
Design    │
  Spec    │░░░░░░░░░░░░░░░
  Review  │               ████████
          │
Build     │
  Renderer│                       ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
  Tests   │                                                      ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒
  Ship    │                                                                    ◆
```
- **M10. BLOCKING-grade: state is double-encoded as colour *and* ink density, and the density
  fights the colour.** done = `░` green `#76D7A0`, active = `█` blue `#64B5FF`, crit = `▓` red
  `#FF6B7F`, plain = `▒` grey `#707784`, milestone = `◆` bold yellow. Colour alone already says
  everything. Varying the fill makes a *completed* task look washed out and makes the **default**
  task state (`▒` at 25 % fill, mid-grey, on a near-black page) the lowest-contrast element on
  the screen. Use solid `█` for every bar and let colour carry state.
- **M11. No legend.** Four bar states and a milestone glyph, and nothing anywhere says what
  they mean. The pie ships a legend; the gantt does not.
- **M12. The longest section label kisses the axis.** `  Renderer│` — zero gap, while
  `  Spec    │` has four. The gutter is sized to the longest label with no padding added. Also
  the gutter width is computed per chart, so two gantts in one document start their axes at
  different columns.
- **M13. The axis rule is unterminated on the right** — `├──┬──┬──┬────` with no `┤` or end cap,
  and it does not reach the full width. The left end has `├`.
- **M14. A single short task produces a nonsense axis.**
```
         00:00     12:00       00:00       12:00      00:00       12:00    00:00
        ├────────────┬───────────┬───────────┬──────────┬───────────┬──────────┬
Only    │
  A task│▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒
```
  Source is `dateFormat YYYY-MM-DD`, start `2026-03-01`, duration `3d`. The axis went *hourly*,
  repeats `00:00` four times, and shows no date anywhere — you cannot tell which day any tick
  is. The bar also fills the chart edge to edge, conveying nothing. Should keep a date-bearing
  axis whenever the span crosses a day boundary, and pad the span so a lone task isn't 100 %.
- **M15. Inconsistent section spacing** — a blank row before `Build`, none before `Design`.

### The remaining fallback (flowchart/class/ER/state) — as requested, judged not reported as missing.
```
╭ mermaid ─────────────────────────────────────────────────────────────────────╮
│flowchart TD                                                                  │
│  A[Start] --> B{Choice}                                                      │
╰──────────────────────────────────────────────────────────────────────────────╯
unsupported mermaid syntax: unsupported diagram type `flowchart`
```
The fallback *shape* is fine — a real code block, syntax-framed, honest about what it is. What
is wrong is the caption (P12): flush at column 0, glued to the bottom border with no indent and
no gap, saying "unsupported" twice in one sentence, and for genuine garbage input echoing the
first word of the nonsense (``unsupported diagram type `this` ``). Put it in the frame's bottom
edge — mirroring the language label on the top edge — and write a message that means something.
Also `╭ mermaid ─` as a label is redundant with a caption that already says "mermaid".

---

## COLOUR VERDICT

The dark palette is competent where it is deliberate. Syntax highlighting is genuinely good —
keywords `#B99BF8`, functions `#7AA2F7`, strings `#76D7A0`, numbers `#FFA657`, comments dim
italic `#7C869B`, all on a `#181C25` panel that lifts one step off the `#11141B` page. Code sits
*inside* the palette, exactly as spec §8 asks. Search matches (`#FFA657` bg for current,
`#F2D06B` for others) are legible and distinguishable. Credit where due.

The document palette is not competent. Headings run
**H1 blue #64B5FF → H2 cyan #5FD7D7 → H3 green #76D7A0 → H4 yellow #F2D06B → H5 orange #FFA657
→ H6 purple #B99BF8**. That is a rainbow, not a hierarchy: H4 and H5 are *more* saturated and
more attention-grabbing than H3, and H6 introduces a brand-new hue at the bottom of the tree.
A hierarchy should lose salience with depth — a single hue family desaturating and dimming, or
at most two hues. Right now the eye is dragged to the deepest headings.

Meanwhile `#64B5FF` is doing six jobs at once: H1 text, *all* heading prefixes (B2), table
header text, scrollbar thumb, filename in the status bar, and the search cursor. When one
colour means six things it means nothing.

Two more collisions: `#FFA657` is both the current-search-match background and the `›`
overflow marker; the H1 rule `#39414F` is *dimmer than body text* on the page background, so
the signature rule under the signature heading is the least visible thing on the line.

## RESIZE

The one thing that is unambiguously right. `tmux resize-window -t vis -x 60 -y 30` mid-document
reflowed correctly and kept the reader's paragraph pinned at the top of the viewport, with no
flicker and no lost place. Spec §3 is honoured. The status bar truncation at narrow widths
(P11) is the only casualty.

---

## OVERALL VERDICT

**No. I would not show this to someone whose taste I respect.**

Not because it fails to render — it renders a lot, correctly, and the architecture underneath
(pure render, canvas contract, correct reflow, real syntax highlighting, genuine CJK/emoji
width handling) is clearly sound. It fails because it has not been *looked at*. The tells are
the ones an experienced eye finds in ten seconds: no margins anywhere, text welded to the
scrollbar, code flush against its own frame while tables are padded, an 80-column box drawn
around the letter "a", a rainbow of heading colours that inverts the hierarchy it is supposed
to express, and a help overlay that cuts off before it can tell you how to quit.

btop earns its reputation on exactly the things missing here: a disciplined two-or-three-hue
palette, consistent padding, panels that are visibly panels, and chrome that degrades
gracefully instead of being chopped.

To get to yes, in order:

1. Paint the theme background across the entire viewport (B1). Until then the light theme is
   not shippable and the dark theme depends on the user's luck.
2. Fix the heading system: prefix glyph takes the heading's colour (B2), glyph families made
   disjoint from list markers (B3), and replace the six-hue rainbow with one hue family that
   *dims* with depth. Make the H1 rule brighter than body text, not dimmer.
3. Give the layout a gutter — one column left, one column right, everywhere, and pad code
   block interiors the way tables and the line-number variant already do (P1, P2).
4. Make the help overlay fit at any height and dim what is behind it (B6, B7).
5. Either implement horizontal scrolling or stop drawing `›` markers that lie (B4, B5).
6. Size tables to their content (P3, P4, P6).
7. Decide about icons: detect, or default to `--no-icons`, or ship a first-run notice. The
   current default makes the common case look like a font failure (P18).
8. Gantt: solid bars, colour-only state encoding, and a legend (M10, M11). It is the one new
   renderer that is worse than no renderer at its default task state.

Items 1–3 are the difference between "a competent Markdown renderer" and "something I would
put in a screenshot". Nothing here is architecturally hard; it is all taste applied late.
