# Demo tour re-think — design

Status: proposed, awaiting owner review
Date: 2026-08-15
Supersedes nothing. Depends on ansidrama ≥ 0.4.0.

## 1. What this is

The demo tour is re-staged onto ansidrama 0.4.0's machinery. **Same seven acts, same
story, same features shown.** This is not a narrative rewrite, and no beat is added or
removed except the theme beat, which is restored to what it was before it was cut.

The tour was authored against ansidrama 0.2.0, whose timing model was "send the next key
once the terminal has been quiet for `settle_ms`". That model is gone. 0.3.0 replaced it
with continuous sampling plus `await` — a scene declares what its finished screen looks
like and the recorder waits for that — and 0.4.0 added a reporting pointer and a `move`
scene. Several beats in the current script are workarounds for the old model, and one line
of it no longer parses at all.

The result to aim for: **a run that completes is a run where every gated beat matched.**
Today a completed run means only that the recorder did not crash.

## 2. Why now

`demo/mdmost.toml:23` carries `settle_ms = 300`, and ansidrama ≥ 0.3.0 fails to parse a
config carrying it. **The tour cannot be recorded at all until that line goes.** Every
other item here is elective; this one is a blocker.

## 3. Scope

Four files. Only one changes behaviour.

| File | Change |
| --- | --- |
| `demo/mdmost.toml` | the work: delete `settle_ms`, add `await`, replace raw SGR escapes with `move`, restore the theme beat |
| `demo/tour.md` | one sentence softened (§6) |
| `docs/maintainer-notes.md` | the timing model and the theme section rewritten from 0.2.0 to 0.4.0 truth (§7) |
| `docs/demo/mdmost.webp` | the regenerated artifact |

Out of scope: the narrative, the seven acts, `demo/config.toml`, `demo/tmux.conf`, and any
change to mdmost's own source.

## 4. Order of work, and why it is not negotiable

**Editing `demo/tour.md` moves the coordinates.** Every click and hover in acts 4 and 6 is
a row read off a rendered screen; rewrapping a paragraph above them shifts those rows, and
a shifted click lands on document text and copies nothing — silently. So the prose edit
cannot come last, and the coordinates cannot be adjusted by arithmetic afterwards.

1. **Delete `settle_ms`. Change nothing else. Record.** The baseline. It answers one
   question — does the existing tour still record truthfully on 0.4.0? — and makes
   everything after it attributable.
2. **Edit `demo/tour.md`, then re-read every coordinate off a real screen.** Not derived,
   not adjusted. Read (§8).
3. **Re-stage act by act**, recording a truncated script after each.
4. **Restore the theme beat last**, when the acts before it are settled.

### The new risk in step 1

In 0.4.0 the pointer **reports its motion**, and mdmost listens for motion (`?1003h`,
`src/tui/term.rs:249`). Every existing `click` and `drag` previously glided in silently;
now the glide lights whatever it crosses. Links will highlight, the status bar will show
URLs, and the hover wash will appear in frames that never had it.

Some of that is an improvement. Some of it is a distraction crossing the screen at the
wrong moment. The baseline recording changes nothing else precisely so this effect can be
seen isolated, and any beat it spoils is re-staged in step 3 like the rest.

## 5. The re-staging

The rule behind every `await`: **the pattern must be text that only the finished screen
contains.** A hover probe in ansidrama once awaited `github.com`, which also sits in the
document body — an unscoped match would have passed with no hover happening. Row scoping
(`row = -1` reaches the status bar) is what makes these tests rather than decorations.

Every pattern below is a **target**. The literal string is proven against a real capture
during implementation, and any pattern that also appears somewhere innocent is rejected.

| Act | Beat | Change |
| --- | --- | --- |
| 1 | launch `less` | `await` on text only the loaded file shows |
| 2 | the four drags | already sent real motion; `await` on the widened state |
| 3 | `Right` × 6 | `await` on the `↔` readout — the count of columns still off the edge is the one string proving the table moved and the prose did not |
| 4 | three `[copy]` clicks | `await` on the pasted text arriving in the nano pane, row-scoped. **Not** on the copy notice: at 49 columns the status bar drops it (§6) |
| 4 | the extra `keys = []` | delete it and see. It was a workaround for nano redrawing a paste in several writes; continuous sampling should retire it. If the paste still freezes half-drawn, it returns |
| 5 | three sacrificial `g`s | replaced by an `await` on the full-width screen — **uncertain, see below** |
| 6 | the hover | `move = { x = 12, y = 19 }` with a row-scoped `await` on the URL. Retires eleven lines of comment and the raw SGR escape hatch, and the frame finally shows an arrow resting on the link instead of a text caret |
| 6 | the un-hover, and the dismissal at (90, 6) | two more `move` scenes, same treatment |
| 6 | the `F F F f Enter` walk | `await` on the status bar URL at the third `F` — the press whose whole purpose is proving a mouseless reader is shown where a control leads |
| 7 | the theme beat | restored: `t`, `await "theme: light"`, `t`, `await "theme: dark"`. The `await` is necessary and **not sufficient** (§8) |

### Act 5 is not promised

The other beats fail because the app was slow to draw, and `await` is built for that. Act 5
fails differently: `bash` exits, and for a moment the keystroke reaches a pane tmux has not
yet closed — the key goes to the *wrong application*. Recorded that way once, and every
beat of act 5 after it was a different demo.

An `await` on the full-width screen makes that miss loud instead of silent, which is worth
having on its own. Whether it also makes it stop happening depends on whether the
recorder's wait survives a pane disappearing underneath it. **If it does not, the three
`g`s stay — with an `await` behind them.** Act 5 improves either way; step 3 finds out
which.

## 6. The act 4 contradiction

`demo/tour.md` claims the status bar "says which, every time". At act 4's 49-column pane
the status bar's width budget drops the copy notice, so the tour contradicts itself on
screen. This is pre-existing and by design in the renderer — something has to be dropped.

**Ruling: soften the claim in `demo/tour.md`.** One line of prose. Act 4 is not re-staged
wider, the renderer's drop priorities are not touched, and no coordinate moves for this
reason. (The prose edit may still move coordinates by rewrapping — that is §4's ordering
constraint, and applies to any edit of that file.)

## 7. Documentation

`docs/maintainer-notes.md` describes a recorder that no longer exists. Two sections are
rewritten rather than patched:

- **The timing model.** Everything about `settle_ms`, the quiet-window pacing, and "the run
  log counts frames, it does not number them" is 0.2.0 truth and is now wrong. It is
  replaced by continuous sampling, `await`, and `manifest.tsv` as the frame → scene → input
  lookup that retired the frame arithmetic.
- **"The theme beat is cut, and why".** Becomes "the theme beat, and how to verify it". The
  bisect table stays — it is the evidence, and it is what makes the restoration explicable
  rather than hopeful.

The regeneration recipe gains the verification steps in §8 as numbered steps, not prose.

## 8. Verification

Most verification is now automatic, and that inversion is the deliverable: a completed run
is a run where every gated beat matched. Two things stay manual, and both are recipe steps.

**The theme frame, checked by eye.** Record once with `--dump-png` into a scratch
directory. `manifest.tsv` maps every frame to its scene and input, so locating the theme
scene is a lookup. Open the frame and confirm the body is actually light — `#fdfcf9`, not
`#11141b`.

This check cannot be an `await`, and the reason is the whole history of this beat: the
broken frame carried a **correct** caption — `theme: light` — over a dark body. `await`
matches text, never colour, so it would have matched the broken frame happily. A green
recording is not evidence that this beat is fixed.

**Coordinates, re-read after any edit to `demo/tour.md` or to the renderer.** Put a
49-column pane on the file, press `Space` four times, and read the row off
`tmux capture-pane -p`. The existing comment in the script says this already; it stays,
because it is still true and still the thing that has actually gone wrong.

### One inherited claim to re-derive, not copy

`docs/maintainer-notes.md` says successive runs produce the same frame count and the same
loop. Under continuous sampling, frame counts are partly app-driven, so this may no longer
hold. The spec asserts nothing either way — the second baseline recording settles it, and
whichever way it lands is a fact the notes should then carry.

## 9. Success criteria

1. `ansidrama record demo/mdmost.toml` parses and completes.
2. No raw SGR escape sequence remains in `demo/mdmost.toml`.
3. Every beat that can fail silently carries an `await` (§5).
4. The theme beat is present, and its frame has been opened and confirmed light.
5. `docs/maintainer-notes.md` describes 0.4.0's recorder, not 0.2.0's.
6. `demo/tour.md` no longer claims something the recording contradicts.
7. The hero image and the closing frame are both dark, as they are today.

## 10. Open, and deliberately not settled here

- Whether act 5's `await` retires the three `g`s (§5).
- Whether the `keys = []` re-capture in act 4 is still needed (§5).
- Whether the reporting pointer improves or spoils any existing beat (§4).
- Whether frame counts are still reproducible run to run (§8).

Each is answered by a recording, and each is cheap to answer in the order §4 sets. None of
them changes the shape of this design.
