# Usability review 2 — driving the real binary

**Verdict: yes — I would reach for `mdmost` instead of `less` tomorrow for reading Markdown.**

Method: release build from a private target dir
(`/scratch/oetiker/cargo-target-mdmost-qa-use2`), driven for real in a detached tmux
session at 120x32 (and 200x45, 70x24, 60x20, 40x12, 20x6, 10x3), with `send-keys` /
`capture-pane`. Every quoted block below is captured pane text. Corpus: a 60-section
generated document, a 14-column/335-column-wide document, a 1.7 MB / 4000-heading
document, an empty file, a heading-less file, and this repo's own `README.md`.

The reading experience is genuinely better than `less`: the feedback discipline (every
unbound key answers you, every failed action explains itself) is the best thing in the
program and is well ahead of what `less` does. The findings below are real, but with one
exception they live at the edges — config parsing and overlay corner cases — not in the
path you walk when you are actually reading a document.

---

## Findings by severity

### HIGH — 1. One unknown key discards the *entire* configuration, and the warning is invisible

`README.md` states:

> A broken configuration never stops the program from starting: the problem is reported
> and the rest of the file still applies, so one bad key binding costs you that binding
> and nothing else.

That is not what happens for an unknown **top-level** key. Config `c.toml`:

```toml
icons = false
bogus = 1
```

```
$ mdmost --config c.toml --render-once --width 30 h.md | cat -v
mdmost: …/c.toml:2: invalid config key `bogus`: unknown field `bogus`, expected one of
  `theme`, `icons`, `line_numbers`, `mouse`, `scroll_step`, `toc`, `keys`, `themes`
 M-oM-^HM-^Y H
```

`M-oM-^HM-^Y` is the Nerd Font glyph U+F019 — i.e. `icons = false` **on the line before
the error** was not applied. Baseline with `icons = false` alone renders `M-bM-^WM-^F`
(`◆`). Position does not matter: `bogus` first or second, the whole file is dropped.

This is not academic, because **the README's own configuration example triggers it.**
Copying the block from the README verbatim:

```
$ mdmost --config readme.toml --render-once --width 40 noheadings.md
mdmost: …/readme.toml:4: invalid config key `toc_open`: unknown field `toc_open`,
  expected one of `theme`, `icons`, `line_numbers`, `mouse`, `scroll_step`, `toc`, …
```

`toc_open` and `toc_width` are documented in `README.md` but do not exist — the real key
is `toc`, and there is no `toc_width` at all. So the *documented* starting configuration
costs the user their theme, their icons setting, their key bindings and their custom
`[themes.*]` table, all at once.

And interactively you never see why. Running `mdmost --config b.toml h.md` in tmux, the
document renders with Nerd icons (config dropped) and after quitting the warning is gone
— it went to stderr before the alternate screen was entered, and the restore wipes it:

```
$ tmux capture-pane -t qa -p -S -20 | grep "invalid config"   # after quitting
(no match)
```

Why it matters: this is the one finding that silently gives the user the wrong program.
Someone who sets `theme`, `mouse` and three key bindings, then typos one key, gets *none*
of it and no explanation. Ranked highest because it is silent, permanent-feeling, and
directly contradicted by the docs.

Scope note, for fairness: the graceful behaviour the README describes **does** hold for
the other two error classes. An unknown *action* costs only that binding, and an unknown
*theme* falls back with a message — both leave the rest of the file applied:

```
mdmost: …/d.toml:3: invalid config key `ctrl-n`: unknown action `no_such_action`
 M-bM-^WM-^F H          ← icons = false still applied
mdmost: …/e.toml:2: invalid config key `theme`: unknown theme `nope`, using `dark`
 M-bM-^WM-^F H          ← icons = false still applied
```

So the fix is narrow: treat an unknown top-level key like the other two, and correct the
README's `toc_open`/`toc_width`.

---

### MEDIUM — 2. The help overlay eats your next keypress, inconsistently and silently

The natural loop is: press `h`, read the key you needed, press it. That does not work.

```
h                       → overlay opens
/                       → overlay closes. No search prompt. No message.
                          status: " long.md 0% ░░░░░░░░   Long Document        h help"
```

Same for `Tab`, for a digit, and for an unbound key:

```
h then Z   → overlay closes, no "Z is not bound" message
h then Tab → overlay closes, TOC does not open
h then 5   → overlay closes, no count started
```

But not for everything, and that is the worse half:

```
h then G   → nothing at all. Overlay stays open, document stays at 0%,
             status unchanged: " long.md 0% ░░░░░░░░   Long Document   h help"
             (G does not even scroll the overlay to its own bottom)
h then q   → quits the program
h then j/k → scrolls the overlay (correct, and the footer says so:
             "╰ ↓ 8 more — j k scroll" / "╰ ↑ k scrolls back")
```

So four different behaviours for four keys, none of them announced. Why it matters:
"look it up, then do it" is *the* reason a help overlay exists, and it is the one flow
that fails. Either dismiss-and-execute, or dismiss-and-say-so — the current mixture
teaches the user that the overlay is unreliable.

---

### MEDIUM — 3. `Esc` on a pending count does the right thing and reports the opposite

```
g                → top
12               → status: " long.md 0% ░░░░░░░░  12…            h help"
Esc              → status: " long.md 0% ░░░░░░░░  nothing to cancel — press q to quit"
j                → moves 1 line (same as the plain-j baseline)
```

The count *was* cancelled — `j` moved one line, not twelve — but the message says there
was nothing to cancel. Why it matters: the Esc ladder is otherwise exactly per spec (see
"works well"), and this message is the one place it lies to you. A user who reads it will
press `Esc` again, or worse, distrust the ladder.

---

### MEDIUM — 4. `n` / `N` with no active search are completely silent

```
(fresh document, no search performed)
n   → status unchanged, no movement, no message
N   → status unchanged, no movement, no message
/   then Enter with an empty pattern → nothing, no message
```

Why it matters: this program's defining virtue is that *nothing* is silent — `y is not
bound — press h for help`, `no match for \`zzzznotfound\``, `no further heading`,
`nothing to cancel — press q to quit`, `filter cleared`, `theme: light`. These three are
the holes in an otherwise perfect record, and they are exactly the case a `less` user
hits (`n` before `/`, or `/`+Enter to repeat the last search, which `less` supports).

---

### MEDIUM — 5. The search prompt has no line editing

```
/abc            → " /  abc█"
BSpace          → " /  ab█"     (works)
Left            → " /  ab█"     (no cursor movement, nothing)
Ctrl-u          → " /  ab█"     (does not clear the line)
```

Backspace is the only editing operation. There is no cursor movement, no word-erase, no
kill-line, and no recall of the previous pattern. Why it matters: regex mode invites long
patterns (`re/  needle[0-9]+`), and a typo in character three of a thirty-character
pattern means twenty-seven backspaces. `Ctrl-u` is a particularly sharp miss because it
*is* bound globally (half-page up), so the user has every reason to expect it to mean
something here.

---

### LOW — 6. Nothing returns horizontal scroll to the left margin

On the 335-column-wide document at 120 columns:

```
→ ×221          → " wide.md   All ████████  ↔ 215/215   Wide         h help"  (clamps, good)
←               → "↔ 207/215"                                   (8 columns per press)
g               → "↔ 199/215"   — top of document, horizontal offset untouched
Home            → "↔ 199/215"   — same
10←             → "↔ 119/215"   — counts do work here
```

`g`/`Home` scroll vertically only, which is defensible, but there is then no single key
for "back to column 0" — it is `←` ×27 or a count you have to compute from the `↔` readout.
Why it matters: the `↔ N/M` indicator and the `‹`/`›` edge markers make horizontal
scrolling unusually legible; the missing complement is the way out of it.

---

### LOW — 7. The invalid-regex message is truncated before the useful part

```
Ctrl-r  (status: "search: literal text" → toggles to regex)
/[unclosed   Enter
→ " long.md 4% ▎░░░░░░░   invalid pattern: regex parse error:            h help"
```

The message ends at `regex parse error:` — the sentence that says *what* is wrong
(unclosed character class) never arrives. Compare the same input in literal mode, which
is correct and complete: `no match for \`[unclosed\``.

---

### LOW — 8. Smart-case search is implemented well and documented nowhere

It behaves the way a modern tool should, on `README.md`:

```
/options → " README.md   21% █▋░░░░░░   Quick start        options 2/2  h help"
/Options → " README.md   21% █▋░░░░░░   Quick start        Options 1/1  h help"
/OPTIONS → " README.md   21% █▋░░░░░░   no match for `OPTIONS`  OPTIONS 0  h help"
```

The source contains exactly one `Options` and one `options`, so the all-lowercase query
matching both and the capitalised query matching only one is smart case, not coincidence.

Neither the help overlay nor `README.md` mentions it, so the user's model is "search is
case-sensitive, and sometimes surprisingly not". One line in the Search section would
convert a hidden behaviour into a selling point.

---

### LOW — 9. Search wrap-around is not announced

Pressing `n` eight times through eight matches:

```
needle 2/8 … needle 7/8 … needle 8/8 … needle 1/8
```

The counter rolling over is the only signal; `less` prints "search hit BOTTOM, continuing
at TOP". Given how carefully everything else in this program narrates itself, the silence
here is out of character. The counter does make it recoverable, hence LOW.

---

### LOW — 10. Help labels for `n`/`N` don't match their direction-relative behaviour

Help says `n  Go to the next match` / `N  Go to the previous match`. After a backward
search the directions invert (which is the correct, `less`-compatible behaviour):

```
50%  ?needle  Enter → " long.md 32% ██▋░░░░░  … needle 3/8"
n                   → " long.md 14% █▏░░░░░░  … needle 2/8"   (backward)
N                   → " long.md 32% ██▋░░░░░  … needle 3/8"   (forward)
```

The behaviour is right; only the wording ("next"/"previous" rather than
"repeat"/"reverse") is misleading.

---

### LOW — 11. The help overlay omits the contents-pane keys and count prefixes

The overlay's 25 rows are a faithful list of the global bindings, but nothing in it tells
you that `/` inside the contents pane filters headings, that `Enter` on a filtered list
jumps and hands focus back to the document, or that counts are a prefix (`10j`, `50%` —
`%` is listed, `10j` is not). `README.md` covers all of these in prose; the overlay,
which is what you actually have in front of you, does not.

---

### LOW — 12. Top of document reads `0%`, bottom reads `End`

```
g → " long.md    0% ░░░░░░░░ "
G → " long.md   End ████████ "
(short document) → " noheadings.md   All ████████ "
```

`End` and `All` are words; `0%` is a number. `less` says `Top`. Cosmetic, but the
asymmetry is visible every time you press `g`.

---

### LOW — 13. `--mouse` is off by default

Wheel and click both work well when enabled (evidence in "works well" below), but a
newcomer's first reflex in a modern terminal — spin the wheel — does nothing. `--help`
explains the tradeoff honestly ("Off by default because capturing takes the terminal's own
drag-select away"), so this is a recorded decision rather than an oversight; noted only
because it is the first thing a mouse-using newcomer will try.

---

## What works well — do not break these

**The unbound-key answer.** Every one of `y e F { } v & : ' m z w H L M | \ * # ^ $`
produced `y is not bound — press h for help`. This single behaviour is why the newcomer
pass never got stuck: you cannot press a dead key without being told, and being told
points you at help. It is better than `less`, and it is the thing this program should be
known for.

**The `Esc` ladder — exactly as specified, and it never quits.** Built up count + search
+ TOC + TOC focus + TOC filter, then pressed `Esc` five times:

```
toc / sec   (filter being typed)
Esc → filter entry cancelled, search still live:  "… Long Document   needle 1/8"
Esc → "search cleared"
Esc → focus returns to the document (pane still open)
Esc → pane closes
Esc → "nothing to cancel — press q to quit"
```

Five distinct steps, correct order, correct terminal message. Only the pending-count case
mis-reports (finding 3).

**The contents pane.** `Tab` opens it *and* focuses it, so `j`/`k`/`g`/`G` drive the list
immediately — no hunting for a focus key. `▸` tracks the reading position and the list
auto-scrolls to keep it visible:

```
│    Section 30
│▸     Subsection 30.1
│    Section 31
 long.md   53% ████▎░░░   Subsection 30.1                       h help
```

`/` filters fuzzily with a live counter in the border (`╭  zeb 1/68 ──────╮`), `Enter`
jumps and hands focus back to the document, long entries ellipsise
(`Rendering rules worth …`), and a heading-less document says `no headings` instead of
showing an empty box.

**Search.** Match counter (`needle 4/8`), highlighted matches (verified in the raw escape
stream: `48;2;255;166;87` background on the matched run), smart case, a `re` badge when
regex mode is on, `Ctrl-r` toggling both inside the prompt (` /  ` → ` re/  `) and outside
it (`search: literal text`), counts on `n` (`3n` → `Section 4/60`), correct
direction-relative repeat, and a clear `no match for \`zzzznotfound\``.

**Themes.** `t` cycles and names each one in the status bar (`theme: light`,
`theme: dark`). A custom theme really is a two-line tweak and really does join the cycle —
with a config containing only `[themes.midnight] base = "dark"`, `t` gave
`theme: light` → `theme: midnight` → `theme: dark`. `--theme light` and
`--theme midnight` both start in that theme; `--theme nosuch` prints
`mdmost: unknown theme \`nosuch\`, using the dark theme` and carries on, exit 0.

**Every key the overlay advertises was pressed and does what it says**, including the ones
easy to leave untested: `F1` (opens the overlay identically to `h`), `Ctrl-f` (0% → 8%),
`PgDn` (8% → 17%), `Ctrl-b` (17% → 8%), `Ctrl-d`/`Ctrl-u`, `Home`/`End`, `[`/`]`,
`Ctrl-g`, `-`, `%`, `←`/`→`. No advertised binding was dead.

**`=` / `Ctrl-g`** gives a real `less`-style report:
`long.md lines 182-212 of 393 (50%) — Section 29`.

**The help overlay really is generated from the live binding table.** This is the
README's strongest claim and it holds. With
`[keys] "ctrl-n" = "line_down"`, `"t" = "none"`, `"x" = "quit"`, the overlay redrew as:

```
│                q x  Quit
│         j Ctrl-n ↓  Scroll down one line
```

— the `t` theme row was gone entirely and the footer went from `↓ 8 more` to `↓ 7 more`.
Pressing `t` then answered `t is not bound — press h for help`. Verified, not assumed.

**Horizontal scrolling** keeps shape rather than mangling: `‹`/`›` edge markers on every
overflowing row, an `↔ 168/215` readout, correct clamping at both ends, and counts
(`10←`) supported.

**Resizing.** Reflow is clean at 200x45, 70x24, 40x12 and 20x6; the contents pane
auto-closes when the terminal gets too narrow and the status bar degrades gracefully
(`   89%      h help`). Even 10x3 renders a sensible frame and recovers on the way back up:

```
◆ mdmost ▄
──────────
 │  h help
```

**Performance.** 1.7 MB / 4000 headings: `--render-once` in 1.45 s, interactive open under
a second, `G`, `/Chapter 3999` and `Tab` all with no perceptible delay.

**Command-line hygiene.** `nonexistent.md` → clear message, exit 1. A directory → `Is a
directory`, exit 1. Bad flag → clap error, exit 2. `mdmost long.md | head -2` → two lines,
no SIGPIPE panic. `mdmost wide.md | cat` → plain text (`--render-once` implied).
`cat long.md | mdmost` → fully interactive with `(standard input)` in the status bar, `G`
and `Tab` both working from `/dev/tty`. `Ctrl-c` exits 0 with the terminal left sane.

**Degenerate documents.** An empty file shows `(empty document)` with `no headings` in the
pane and answers `no match for \`foo\`` on search; a heading-less file answers
`no further heading` on `]`. Nothing panics, nothing shows an empty frame.

**`%` handling.** `999%` clamps to `100%` and lands on `End`; `0%` and a bare `%` go to the
top; `50%` is reported back as `50%`.
