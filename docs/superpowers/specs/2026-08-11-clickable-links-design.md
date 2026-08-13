# Clickable links and footnotes — design

Status: approved in outline 2026-08-11. Successor spec (document navigation) not yet written.

## 1. What this is

A link in the rendered document reacts to the pointer and can be activated. Activating an
`http`/`https` link opens the browser; activating a `#anchor` scrolls this document to that
heading; activating a footnote marker opens a small popup holding the footnote.

Everything is reachable from the keyboard as well as the mouse.

### 1.1 What this is not

- **Local `.md` links are inert.** They record no hotspot, so they offer no control and give
  no reaction. Opening another document makes the pager a browser — history, a way back,
  missing-file handling, and what becomes of scroll position, search state and the TOC when
  the document underneath changes. That is a navigation subsystem and gets its own spec.
  Deliberately, a local link does *not* light up and then refuse; a control that appears live
  and declines is worse than one that was never offered.
- **Other schemes are inert** — `mailto:`, `ftp:`, anything else. See §8.
- **Links inside a footnote popup are inert.**
- **No OSC 8 hyperlink emission.** The pager handles the click itself, because it must give
  the visual reaction; handing clicks to the terminal would take that away.

## 2. The model

`Hotspot` already exists in `src/canvas/`: a claim on a region of one row that survives
layout, blitting and indenting, currently carrying the copy button's payload. It grows a
**kind**:

    Copy { text, html }   the existing copy button
    Open { url }          http/https
    Anchor { slug }       #heading in this document
    Footnote { id }       a footnote reference marker

One hit-test serves all four. The copy button stops being a parallel mechanism and becomes
one case of a general one.

### 2.1 The reactive region is the whole link

A link draws as its text plus a printed ` (url)` suffix. **Both are one region.** The suffix
is a synthetic decoration carrying no source span — which is why it correctly stays dark in a
*selection* — but it is part of the link as drawn, so it is part of what reacts.

Selection and reaction answer different questions and are allowed to disagree here:
selection asks "which source bytes", reaction asks "which drawn cells belong to this
control".

### 2.2 A wrapped link is one control

A link crossing a row boundary records several hotspots sharing one target id. Hovering any
row lights every row of that link. Without this a wrapped link visibly breaks in half under
the pointer.

### 2.3 Which seam

Rendering stays a pure function of `(AST, width, theme, options)`: the canvas records
hotspots and nothing else. Hover, press and the activation flash are **paint-time** concerns
in `src/tui/draw.rs`, alongside `copied_flash`, which already owns this seam. Pointer state
lives on `App`. `render` gains no dependency on `tui`.

## 3. Activation

**A click is press and release on the same hotspot with no intervening drag.**

- Press on a hotspot records a candidate and paints the pressed state.
- Moving off the pressed cell cancels the candidate; the gesture becomes an ordinary
  selection.
- Release on the same hotspot fires the action.

**Selection wins every tie.** No gesture that works today changes behaviour.

### 3.1 Visual states

Idle, hover, pressed, plus a brief post-activation flash reusing `copied_flash`.

**The colours are deliberately unnamed in this spec.** They are settled by rendering all
states in every theme and showing the owner, as with the copy button. Note the light theme's
heading ramp is known to be flat and non-monotone; re-measure before designing against it.

## 4. The keyboard cursor

A key cycles a cursor through the hotspots on the current screen; Enter activates the one
under it, with the same visual reaction the pointer gives. Reuses the TOC pane's cursor
pattern. The specific key binding is left to the plan, against the existing bindings.

This **resolves** the gating rule rather than inheriting it. Copy buttons are hidden when
mouse capture is not granted, because a control nobody can click is worse than none. A
keyboard cursor makes links reachable over SSH and in terminals without mouse support, so
links are never hidden — the principle is satisfied by a different route.

~~`--render-once` records no hotspots, as it draws no copy buttons.~~ **Struck by owner
ruling, 2026-08-13.** It contradicted the paragraph above it, and the paragraph above it
is the one that is right. The line was written about *buttons*, which are drawn chrome; a
hotspot draws nothing. `--render-once` sets `copy_button: false`, and that is the **same**
flag the pager sets when mouse capture is refused and when stdout is not a terminal — so
gating link hotspots on it would blank every link in every mouseless terminal, which is
the exact population this section exists to serve. Hotspots are recorded, and a test pins
that behaviour.

## 5. Anchors

`#some-heading` resolves against heading slugs by GFM rules: lowercase, spaces to dashes,
punctuation dropped, duplicates suffixed `-1`, `-2`. The target heading scrolls to the top
row.

**Slugs come from the same enumeration the TOC uses.** One source of truth, so an anchor and
the TOC cannot disagree about what a heading is called.

An anchor matching nothing says so in the status bar and scrolls nowhere. The status bar
never lies.

## 6. The footnote popup

A bordered box adjacent to the marker, sized to its content up to a cap, flipping
above/below and left/right to stay on screen.

The footnote renders through the **ordinary renderer at the popup's width**. Rendering is
already a pure function of width, so a popup is another width, not a second rendering path.
Formatting, code spans and lists inside a footnote therefore work for free.

Long footnotes scroll: the wheel over the popup, or the cursor keys while it is open.

Dismissed by `Esc` (which never quits, so it is free for this), a click outside, or scrolling
the document.

## 7. Opening

The platform opener (`xdg-open`, `open`, `cmd /c start`) is spawned **detached, with a direct
argv and no shell**, so nothing in a URL can be interpolated into a command. Its stderr is
kept off the alternate screen through the existing `stderr` module. Failure is reported in
the status bar.

The UI never blocks on the child.

## 8. Why the scheme allowlist is a safety feature

Only `http` and `https` get a hotspot. This is a security decision as much as a scope one: a
document the reader did not write cannot make the pager launch an arbitrary desktop handler.

**Hovering shows the full URL in the status bar** — the "see where it goes before you commit"
safeguard, and the reason there is no confirmation prompt.

## 9. Testing

- **Hotspot geometry** is render-time: assert on `canvas.hotspots()`, no terminal needed.
  Cover a wrapped link, a link at a row boundary, a link inside a table cell, a link inside a
  block quote, and a link in a list item.
- **Activation** is a pure state machine over press / move / release on `App`. Testable
  without a mouse. Cover: press-release fires; press-drag-release does not and yields a
  selection; press on one hotspot and release on another fires nothing.
- **The opener sits behind a seam**, so tests assert "would have opened *X*" without
  launching a browser — the same shape as the existing clipboard test double.
- **Popup layout** is tested by rendering to a canvas: flipping at each screen edge, sizing
  to content, and a footnote long enough to scroll.
- **Anchors**: a matching slug scrolls, a duplicate heading resolves to the right one, an
  unknown anchor reports and does not move.
- **Fault injection is mandatory**, per project convention: every rule is proved by watching
  a test go red, never by asserting it was verified. A mutation that turns no test red is a
  finding about the test.

### 9.1 A known coverage asymmetry to avoid repeating

Task 3's review checked that chrome stays *unwashed*; nothing checked that body text stays
*washed*, and a regression shipped through it — reverting the later fix across 1062 tests
turned exactly one test red. This spec adds a second thing that paints over text. Test both
directions: that a hotspot reacts, **and** that a non-hotspot cell does not.

## 10. Sequencing

This lands on the same paint-time seam as the selection work and touches the same files. It
is sequenced after partial selection (owner ruling 5) and Task 6, not in parallel with them.
