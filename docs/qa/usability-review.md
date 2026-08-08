# mdless — usability review (harsh)

Binary: /home/oetiker/scratch/cargo-target/{debug,release}/mdless @ 3e17fdc
Driven in tmux at 100x30, 100x24, 90x20, 60x16, 40x10, 10x20, 80x5, 80x2.

## BLOCKING

### B1. Horizontal scrolling does not work at all in normal use
Repro: `tmux new-session -d -s w -x 60 -y 16; mdless wide.md`, press `→` five times.
Content is clipped with `›` markers and nothing moves:

```
╭  rust ─────────────────────────────────────────────────╮█
│fn very_long_line() { let a = "aaaaaaaaaaaaaaaaaaaa"; le›│█
╰─────────────────────────────────────────────────────────╯█
╭─────────────────┬─────────────────┬──────────────────┬──›█
│ AlphaColumnOne  │ BetaColumnTwo   │ GammaColumnThree │ D›█
```
Identical pane after 5× `→`.

Cause (src/tui/app.rs:421-429): `overflow = canvas.width() - viewport_width()`. The canvas
is rendered *at* viewport width, so overflow is always 0 and `clamp()` resets `hscroll` to 0.
It only works when `--width N` forces a canvas wider than the terminal (verified: `--width 120`
in a 60-col terminal scrolls fine and shows `↔ 8/61`).

Why it matters: spec §7.3 and §8 promise wide tables and long code lines become horizontally
scrollable. The UI *tells the user content is cut off* (`›`) and then gives no way to reach it.
Any real README with a wide table is permanently unreadable. This alone is abandon-the-tool.

Should be: clipped rows must set the scrollable extent from the widest *unclipped* content,
not from the already-clipped canvas.

### B2. The TOC does not track the current section while you scroll
Repro: `mdless --toc normal.md`, `Esc` (release TOC focus), then `d` ×8 to the bottom.
```
╭  Contents ────────────────╮text
│▸ mdless Test Document      │
│    Section One             │ Deep 5.2
...
│      Deep 5.1              │text
 󰈙 normal.md  100% ████████   Deep 5.1                    h help
```
Status bar says `Deep 5.1`; the TOC marker `▸` is still parked on the first entry.
It only re-syncs when the pane is closed and reopened with `Tab`.

Spec §9 requires "the current section highlighted". A navigation pane that shows a stale,
wrong position is worse than no pane: with `--toc` (a shipped startup flag) the map lies
permanently. Should be: the highlight follows the viewport on every scroll.

### B3. `Esc` quits the pager, and it does so with the TOC pane still open
Repro A: `mdless normal.md`, `/needle`, `Enter`, `Esc` → **application exits**.
Repro B: `mdless normal.md`, `Tab`, `Esc` (releases TOC focus, pane stays visible), `Esc`
→ **application exits while the TOC is still on screen**.

Spec §10 says Esc "closes an overlay or TOC focus first, quits from a bare document". A
document with a visible TOC pane and/or an active search is not "bare". In `less`, Esc does
nothing. Esc is the key people press when they are unsure — here it destroys their position
in the document with no undo and no confirmation. Should be: Esc unwinds state (clear search
highlight → close TOC pane → nothing), never quits. Reserve quitting for `q`.

### B4. The help overlay is silently truncated and hides the quit key
Repro: exactly 100×30 (`tmux resize-window -x 100 -y 30`), press `h`. Bottom of overlay:
```
│  View                                                    │
│                  t  Switch to the next theme             │
│               h F1  Show or hide this help               │
 󰈙 normal.md    0% ╰────────────────────────────────────╯   h help
```
`Esc  Close the overlay, or quit` and `q  Quit` are gone. At 40×10 it is far worse — the user
sees only Movement up to `g Home`, and the overlay covers the status bar so even the `h help`
hint disappears:
```
╭  Keys ──────────────────────────────╮
│  Movement                            │
│                j ↓  Scroll down one  │
...
│             g Home  Go to the top o  │
╰──────────────────────────────────────╯
```
No scrollbar, no "…more", no paging keys, and rows are cut mid-word without an ellipsis.
For a tool whose discoverability story *is* the help overlay, an overlay that cannot tell a
trapped user how to quit is a blocker. Should be: the overlay scrolls (j/k/PgDn) and shows an
indicator, or it reflows to multiple columns / drops to a compact one-line-per-group form.

### B5. Startup is super-linear and the screen is blank while it happens
Release build, `--render-once --width 80` (files of `## Heading N` + paragraph pairs):

| size | time |
|---|---|
| 200 KB | 0.15 s |
| 800 KB | 0.79 s |
| 3.2 MB | 6.82 s |
| 12.8 MB | >90 s (timed out) |
| 20 MB | >120 s (timed out) |

Debug build (what a developer runs) is ~15× worse: 800 KB takes 21 s, 3.2 MB times out at 120 s.

In the TUI it is worse than slow, it is *silent*. Measured time from launch to the first
non-blank frame, release build, 100×24 tmux pane, by polling `capture-pane`:

- `s8000.md` (800 KB): **1.9 s** of blank alternate screen.
- `s32000.md` (3.2 MB): **15.9 s** of blank alternate screen. (Polling steals some CPU;
  `--render-once` on the same file is 6.8 s, so treat this as 7–16 s.)

No filename, no "Loading…", no progress bar, no way to tell it apart from a hang. Separately,
a resize re-lays-out the whole document: resizing the 800 KB file from 100 to 88 columns took
**0.91 s** before the reflowed frame appeared, and on the 3.2 MB file a resize gave another
multi-second freeze. `less` opens a 100 MB file instantly.

Should be: render the first screen before parsing/laying out the rest (or at minimum show a
progress line and stay interruptible), and debounce resize re-layout.

## POLISH (still real)

### P1. Status bar collides with the key hint
At 60 columns:
```
 󰈙 normal.md    0%           mdless Test Documenth help
```
The heading is truncated with no separator and butts into `h help`, producing the nonsense word
`Documenth`. Needs a reserved gap and an ellipsis.

### P2. Status bar loses the help hint exactly when it is most needed
At 40 columns the bar reads ` 󰈙 normal.md    0%          mdless` — `h help` is dropped. The
narrower the terminal, the less likely the user knows how to get help or quit. The hint should
be the last thing dropped, not the first.

### P3. Every unknown key is a silent no-op
Pressed and verified dead, with zero feedback: `10j`, `50%`, `Ctrl-G`, `=`, `m`, `'`, `v`, `F`,
`&`. (`-N` line-number toggle is also absent from the binding table; it exists only as a config
flag `line_numbers`.) A `less` user typing `50%` sees a frozen-looking screen and no clue why.
Should be: unrecognised input flashes `unknown key — press h for help` in the status bar.

### P4. Repeat counts and percentage-seek are missing
`10j`, `5d`, `50%`, `100g` are core `less`/vi muscle memory. Their absence is the single
biggest "this is not less" gap after B1.

### P5. TOC needs two Enters, and cannot be re-focused without closing it
Repro: `Tab`, `/`, type `dp`, `Enter` → filter commits, **document does not move**. A second
`Enter` jumps. Spec §9 says "`Enter` jumps."
Also: after a jump, focus silently returns to the document. `j` then scrolls the document. The
only way back into the TOC is `Tab` `Tab` — which also *preserves the stale filter* (pane title
still reads ` dp` with no way to clear it, since `Esc` quits — see B3).

### P6. A failed search leaves permanent stale noise in the status bar
After a zero-match search the bar keeps `dp 0` forever, through TOC toggles and scrolling.
There is no discoverable way to clear it (`Esc` quits). Should auto-clear or be clearable.

### P7. Search state is shown twice and redundantly
` 󰈙 normal.md   93% ███████▍  match 3/4        needle 3/4  h help` — `match 3/4` and
`needle 3/4` say the same thing, in a bar that is already dropping the help hint at 40 cols
(P2). Drop one.

### P8. Mouse capture is on by default, which breaks copy/paste
`config.mouse` defaults to true and `EnableMouseCapture` is issued unconditionally. In most
terminals that disables native drag-select, so a *read-only viewer* — the one kind of program
where selecting and copying text is the primary secondary action — makes copying require
Shift-drag. `less` does not do this. The escape hatch (`mouse = false` in TOML) appears in
neither `--help` nor the help overlay.

### P9. Mermaid fallback message is wrong and unhelpful
```
╰────────────────────────────────────────────────╯
unsupported mermaid syntax: unsupported diagram type `flowchart`
```
Three problems: (a) `flowchart` *is* a spec'd family (§2), so the message tells the user
mdless can't do flowcharts rather than "not implemented yet"; (b) the wording is redundant
("unsupported … unsupported"); (c) for genuinely broken input it echoes a random word from the
user's typo as a diagram type — `unsupported diagram type \`this\`` for the line
`this is not valid mermaid at all !!!`. The caption is also unstyled body text sitting outside
the frame, so it reads as document content. Should be framed as a caption and say
"mermaid flowchart rendering is not available yet — showing the source".

### P10. `--config PATH` pointing at a missing file is silently ignored
`mdless --config /nonexistent/cfg.toml normal.md` → exit 0, no message, defaults used.
An explicitly-named config that does not exist is a typo, not an optional file. (The *default*
config being absent is correctly silent.)

### P11. One bad colour discards the whole custom theme, with a misleading second message
```
mdless: th2.toml:3: invalid config key `accent`: in theme `mine`: invalid colour `not-a-color`
mdless: unknown theme `mine`, using the dark theme
```
The theme *is* defined; only one slot is bad. Spec §9's whole selling point is "a custom theme
can be a two-line tweak". Should keep the theme and fall back on the single bad slot; and the
second line should not claim the theme is unknown.

### P12. `mdless x.md | head` prints `mdless: Broken pipe (os error 32)`
Pagers and CLI tools exit silently on EPIPE. Cosmetic but it looks broken.

### P13. Position readout is confusing when the document fits on screen
A document shorter than the viewport shows `100% ████████` while the cursor is at the top.
`less` shows `(END)`. Also `G` reports the heading at the *top* of the viewport
(`Section Four`) while the visible text is Sections Five–Seven — technically consistent, but at
the bottom of the document it reads as simply wrong.

### P14. Empty document shows a bare blank alternate screen
`mdless empty.md` → 23 blank rows, a scrollbar, `empty.md 100%`. No "(empty document)".

### P15. TOC entries truncate mid-word with no ellipsis, producing duplicates
At 40 cols: `│      Subsection 1│` appears twice — 1.1 and 1.2 are indistinguishable. Needs an
ellipsis and/or a smarter narrow-pane layout.

### P16. `Tab` is a silent no-op below ~40 columns
At 10 columns the TOC never appears and nothing is said. Fine to suppress; not fine to be silent.

### P17. `$PAGER` on non-Markdown is a mess
`git log | mdless` — commit headers get reflowed into paragraphs (line breaks lost), emails
become `(mailto:…)` links, indented commit bodies become framed code blocks:
```
commit 2041ef3b… Author: Tobias Oetiker tobi@oetiker.ch
(mailto:tobi@oetiker.ch) Date:   Sat Aug 8 21:34:28 2026 +0200
╭──────────────────────────────────────────────────╮
│feat: sequence, pie and gantt renderers wired…    │
```
§2/§11 sell "usable as `$PAGER`", which users read as `export PAGER=mdless`. That setting
makes `git log`, `--help` output and man pages unreadable. Either narrow the claim in the docs
to Markdown-specific pager slots, or add a plain-text passthrough mode.

## What works well (so the criticism lands where it should)

- Rendering quality is genuinely good: headings, rules, rounded table borders, GFM alignment,
  framed code fences with a language chip, blockquote bars, nested list glyphs.
- Unicode is solid — CJK double-width, ZWJ family emoji, flags, combining marks all measured
  correctly, no panics down to `--width 1`.
- Terminal restoration is correct in every path tested: `q`, `Esc`, `Ctrl-C`, `SIGTERM`, and
  after a 2-row terminal. `stty` clean, alternate screen exited, shell responsive.
- Exit codes are right: 0 success, 1 unreadable/permission/directory, 2 bad arguments
  (`--width 0` → 2 with a clear message). Error text names the file and the OS reason.
- Config diagnostics are excellent: `file:line: invalid config key \`x\`: <reason>` with the
  list of accepted fields, and it still starts.
- The stdin/`$PAGER` keyboard path works — `cat x.md | mdless` and `mdless -` both accept keys.
- Smart case search, regex mode via `Ctrl-r` with a `re/` prompt, match counts, `n`/`N` with
  silent wrap-around, `?` backward — all correct.
- Search-during-resize and 80×2 terminals survive without artifacts.
- **Every binding the help overlay claims actually works.** Verified individually:
  `j k ↓ ↑ d u Ctrl-d Ctrl-u Space b Ctrl-f Ctrl-b PgDn PgUp g G Home End [ ] Tab Enter
  / ? n N Ctrl-r t h F1 Esc q`. No drift between the overlay and reality (§10's claim holds).
- Config key remapping works: `[keys]\n"x" = "quit"` makes `x` quit, and an unknown action name
  is reported as `k.toml:3: invalid config key \`zzz\`: \`zzz\` is not a key I recognise`.

## Verdict

**No. It is not yet as pleasant to use as `less`, and two of the gaps are disqualifying.**

The rendering is the strong part and it is already better than anything `less` can do. The
*pager* is the weak part. Three things a `less` user does constantly are broken or missing:

1. **Reaching content that is off-screen to the right.** `less` has `→`/`ESC-)`; mdless shows
   you the truncation marker and then refuses to move (B1).
2. **Knowing where you are.** `less` has `Ctrl-G`. mdless has a TOC that stops updating the
   moment you scroll (B2), a status heading that disagrees with it, and no `Ctrl-G`, no `=`,
   no `50%`, no repeat counts (P3, P4).
3. **Getting out safely.** `Esc` — the universal "undo whatever I just did" key — kills the
   session and your reading position (B3), and the help overlay that would warn you about it
   is truncated above the line that mentions `q` (B4).

Add the startup stall (B5): a 3 MB document means seven seconds of blank screen with no
feedback, repeated on every resize. `less` is instant on any file at any size, and that
instantaneousness is a large part of why it is pleasant.

To reach the stated bar, in order: fix horizontal scroll, make the TOC track, make `Esc`
non-destructive, make the help overlay scroll, add repeat counts + `50%` + `Ctrl-G`, give
unknown keys feedback, and get first-paint off the critical path of full-document layout.
