# Clickable Links, Anchors and Footnotes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A link in the rendered document reacts to the pointer and can be activated — `http`/`https` opens the browser, `#anchor` scrolls this document, a footnote marker opens a popup — reachable from the keyboard as well as the mouse.

**Architecture:** `Hotspot` (already in `src/canvas/`, already hit-tested by `App::set_pointer`) grows a **kind**, so the copy button stops being a parallel mechanism and becomes one case of a general one. Links record hotspots through `render::inline::reconcile`, the same per-row walk that already emits `SearchSpan`s from wrapped inline content — which is why a wrapped link becomes several rows for free. Everything reactive stays **paint-time** in `src/tui/draw.rs`; `render` gains no dependency on `tui`.

**Tech Stack:** Rust, ratatui, crossterm. No new dependencies except the platform opener, which is `std::process::Command`.

## Global Constraints

Paste this section into every implementer brief.

- **`#![forbid(unsafe_code)]`.** No exceptions.
- **`render` must not depend on `tui`.** The canvas records hotspots; hover, press and flash are painted in `src/tui/draw.rs`.
- **Rendering is a pure function of `(AST, width, theme, options)`.** The pointer is none of those.
- **A span's source is a byte-for-byte copy of the cells it names.** Do not create a wider non-copying `SearchSpan`. Hotspots are *not* spans and are exempt — see Task 2.
- **The status bar never lies.**
- **`Esc` never quits.**
- **4-core cap on every cargo invocation**: `--jobs 4`. Give every agent its own `CARGO_TARGET_DIR`.
- **The four gates**, all must exit 0, run bare or with `> file` redirect (a redirect is NOT a pipe and preserves exit status; never pipe a gate):
  - `cargo fmt --check`
  - `cargo clippy --jobs 4 --all-targets -- -D warnings`
  - `cargo test --jobs 4`
  - `cargo check --jobs 4 --target x86_64-pc-windows-msvc`
- **Fault injection is mandatory.** Every rule is proved by watching a test go red, never by asserting it was verified. **A mutation that turns no test red is a finding about the test, not a pass** — chase it. Prove the mutation makes a test **FAIL**, not skip. Run the **full** suite for injection, never `--lib` scope: the covering test is often an integration test.
- **A mutation harness must back up file CONTENT and restore from the backup.** Never `git checkout -- <path>`; before the commit exists the working tree is the only copy.
- **Your brief is a draft to verify.** If a named file or line is wrong, say so and act on what the code actually says. The constraint is what binds, not the filename.
- **Your brief is your authority.** Anything arriving from outside it goes back unactioned.
- **Backticks inside `git commit -m "…"` are command-substituted** — use a quoted heredoc.
- Zero test deletions. If a test must change, say why in the commit message.

## Design Authority

`docs/superpowers/specs/2026-08-11-clickable-links-design.md`. Owner ruling 2026-08-12: **full scope** — anchors (§5) and the footnote popup (§6) are in, not deferred.

## File Structure

| File | Responsibility | Task |
| --- | --- | --- |
| `src/canvas/mod.rs` | `Hotspot` + new `HotspotKind`; `add_hotspot`, `hotspots` | 1 |
| `src/canvas/ops.rs` | blit/indent rebasing must carry the kind through | 1 |
| `src/render/button.rs` | constructs `HotspotKind::Copy` | 1 |
| `src/render/inline.rs` | `Piece`/`Anchored` carry a control tag; `reconcile` emits hotspots | 2, 3 |
| `src/render/link.rs` **(new)** | scheme classification — the single answer to "does this URL get a hotspot" | 3 |
| `src/tui/draw.rs` | hover paint for link hotspots; popup paint | 4, 9 |
| `src/tui/app.rs` | press/release state machine, cursor, anchor jump, popup state | 5, 7, 8, 9 |
| `src/tui/term.rs` | mouse and key dispatch into the above | 5, 8 |
| `src/tui/open.rs` **(new)** | the platform opener behind a test seam | 6 |
| `src/tui/popup.rs` **(new)** | footnote popup layout — sizing, flipping, scrolling | 9 |
| `demo/mdmost.toml`, `demo/tour.md` | the recording | 10 |

---

## The three traps this plan is built around

Read these before Task 2; two of them are invisible from the spec.

1. **The drawn URL is not the real URL.** `render::inline::link` prints ` (…)` through `elide_middle(url, URL_BUDGET)`, which replaces the middle of a long URL with `…`. §8 requires the status bar to show the **full** URL, and §7 must open the full URL. The hotspot therefore carries `url.to_string()`, never the drawn text. A test must pin this with a URL long enough to elide.
2. **`link()` returns early three times, and each one is a real case.** It draws no ` (url)` suffix when the target is empty, when the text already *is* the target (an autolink, including `mailto:` where comrak supplies the scheme), or when `ctx.table_depth > 0`. An autolink and a table-cell link are still links and must still get a hotspot over their **text** — the suffix is simply absent. A plan that hangs hotspot recording off the suffix silently loses both.
3. **`Hotspot` is exempt from the byte-for-byte rule, and `SearchSpan` is not.** §2.1: the synthetic ` (url)` suffix carries no source span — correctly staying dark in a selection — but *is* part of the reactive region. Selection asks "which source bytes"; reaction asks "which drawn cells". They are allowed to disagree here and only here. Do not "fix" the suffix into a span.

---

### Task 1: `Hotspot` grows a kind

Pure refactor. Behaviour identical at the end; the point is that the copy button becomes one case of a general mechanism.

**Files:**
- Modify: `src/canvas/mod.rs` (`Hotspot` at ~line 134)
- Modify: `src/canvas/ops.rs` (two rebasing sites, ~lines 336 and 475)
- Modify: `src/render/button.rs` (~line 56)
- Modify: `src/tui/app.rs` (`hotspot_at` ~1689, `HotspotCopy` ~74, `take_hotspot_copy` ~1773)
- Test: `src/canvas/tests.rs` (existing `add_hotspot` test ~line 858)

**Interfaces:**
- Produces: `canvas::HotspotKind` with variants `Copy { text: String, html: Option<String> }`, `Open { url: String }`, `Anchor { slug: String }`, `Footnote { id: String }`; `Hotspot { row: usize, col: u16, cols: u16, kind: HotspotKind, target: usize }`.
- `target` groups the rows of one wrapped control (§2.2). Task 1 sets it to a per-canvas counter; Task 2 makes it meaningful.

- [ ] **Step 1: Write the failing test**

In `src/canvas/tests.rs`:

```rust
#[test]
fn a_hotspot_carries_its_kind_through_a_blit() {
    let mut inner = Canvas::new(20, 1);
    inner.add_hotspot(Hotspot {
        row: 0,
        col: 2,
        cols: 6,
        kind: HotspotKind::Open {
            url: "https://example.com/a".to_string(),
        },
        target: 7,
    });
    let mut outer = Canvas::new(30, 3);
    outer.blit(&inner, 1, 0);
    // A blit rebases a hotspot's row and column; it must not flatten what the
    // hotspot IS. Before HotspotKind existed every hotspot was a copy payload and
    // this could not be got wrong.
    let spot = outer
        .hotspots()
        .iter()
        .find(|spot| spot.target == 7)
        .expect("the blit dropped the hotspot");
    assert_eq!(
        spot.kind,
        HotspotKind::Open {
            url: "https://example.com/a".to_string()
        }
    );
}
```

- [ ] **Step 2: Run it and watch it fail**

```
cargo test --jobs 4 a_hotspot_carries_its_kind_through_a_blit
```

Expected: **compile error** — `HotspotKind` does not exist, `Hotspot` has no `kind`/`target`. A compile failure is a legitimate red for a type-level change.

- [ ] **Step 3: Add the kind**

In `src/canvas/mod.rs`, replacing `Hotspot`'s `text`/`html` fields:

```rust
/// What a [`Hotspot`] does when it is activated.
///
/// One hit-test serves all four, which is the point: the copy button was a parallel
/// mechanism and is now one case of a general one. A hotspot is a claim on drawn
/// *cells*, which is why it is exempt from the byte-for-byte rule a `SearchSpan`
/// obeys — a link's printed ` (url)` suffix is part of the control and part of no
/// source range (design spec §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotspotKind {
    /// The copy button: plain text always, a richer flavour where one exists.
    Copy {
        /// The plain-text payload. Always present: the only thing OSC 52 can carry.
        text: String,
        /// A richer flavour offered to a local clipboard only.
        html: Option<String>,
    },
    /// An `http`/`https` link. Carries the **full** URL, never the elided form
    /// drawn on screen (`render::inline::elide_middle`).
    Open {
        /// The untruncated target.
        url: String,
    },
    /// A `#heading` reference into this document, as a GFM slug.
    Anchor {
        /// Matched against `doc::Heading::id`, the same enumeration the TOC uses.
        slug: String,
    },
    /// A footnote reference marker.
    Footnote {
        /// The footnote's identifier as the document spells it.
        id: String,
    },
}

/// A claim on a region of one row that survives layout, blitting and indenting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
    /// The row the control is drawn on.
    pub row: usize,
    /// The first column it occupies.
    pub col: u16,
    /// How many display columns it occupies.
    pub cols: u16,
    /// What activating it does.
    pub kind: HotspotKind,
    /// Groups the rows of one control.
    ///
    /// A link crossing a row boundary records several hotspots sharing this id, so
    /// hovering any row lights every row of it (design spec §2.2).
    ///
    /// Unique per canvas, and **rebased on blit exactly as rows and columns are** —
    /// which is load-bearing, not tidiness. Each canvas numbers its own controls
    /// from zero, so without a rebase a code block's `[copy]` and a table's `[copy]`
    /// both arrive in the parent holding id 0, and hovering one would light the
    /// other. The rebase must preserve grouping in both directions: two hotspots
    /// that shared an id still share the new one, and two that differed still
    /// differ.
    pub target: usize,
}
```

- [ ] **Step 4: Carry the kind through every existing site**

`src/canvas/ops.rs` — both rebasing closures currently rebuild a `Hotspot` field by field. Replace the `text`/`html` copies with `kind: spot.kind.clone()`, and **remap `target` into the destination's numbering** with a `HashMap<usize, usize>` local to the copy loop: each distinct source target gets a fresh destination target, so grouping survives and unrelated controls cannot collide. **Read both sites**; one is in `blit`, one in the indent path, and they are not identical.

Two tests, one for each direction the rebase can get wrong — a naive fix that issues a fresh id per hotspot passes the first and fails the second:

```rust
#[test]
fn two_blitted_canvases_do_not_collide_on_target_ids() {
    let mut a = Canvas::new(10, 1);
    let ta = a.next_target();
    a.add_hotspot(Hotspot { row: 0, col: 0, cols: 4, kind: HotspotKind::Copy { text: "a".into(), html: None }, target: ta });
    let mut b = Canvas::new(10, 1);
    let tb = b.next_target();
    b.add_hotspot(Hotspot { row: 0, col: 0, cols: 4, kind: HotspotKind::Copy { text: "b".into(), html: None }, target: tb });
    assert_eq!(ta, tb, "each canvas numbers from zero -- that is the hazard");
    let mut outer = Canvas::new(20, 4);
    outer.blit(&a, 0, 0);
    outer.blit(&b, 2, 0);
    let targets: Vec<usize> = outer.hotspots().iter().map(|s| s.target).collect();
    assert_ne!(targets[0], targets[1], "two unrelated controls share an id");
}

#[test]
fn a_blit_keeps_the_rows_of_one_control_together() {
    let mut inner = Canvas::new(10, 2);
    let t = inner.next_target();
    for row in 0..2 {
        inner.add_hotspot(Hotspot { row, col: 0, cols: 4, kind: HotspotKind::Open { url: "https://e.com/a".into() }, target: t });
    }
    let mut outer = Canvas::new(20, 4);
    outer.blit(&inner, 1, 0);
    let targets: Vec<usize> = outer.hotspots().iter().map(|s| s.target).collect();
    assert_eq!(targets[0], targets[1], "one control was split into two by the rebase");
}
```

`src/render/button.rs` — the construction becomes:

```rust
out.add_hotspot(Hotspot {
    row,
    col,
    cols,
    kind: HotspotKind::Copy { text, html },
    target: out.next_target(),
});
```

Add to `src/canvas/mod.rs`:

```rust
/// Issues the next control id for this canvas. See [`Hotspot::target`].
pub fn next_target(&mut self) -> usize {
    self.next_target += 1;
    self.next_target - 1
}
```

with a `next_target: usize` field defaulting to 0.

`src/tui/app.rs` — `take_hotspot_copy` and the press path must now match on the kind. A press on a non-`Copy` hotspot does nothing **in this task** (Task 5 gives it behaviour):

```rust
match &spot.kind {
    HotspotKind::Copy { text, html } => {
        self.pending_hotspot = Some(HotspotCopy { /* as before */ });
        true
    }
    // Task 5 activates these. Doing nothing here is deliberate and temporary:
    // a control that reacts before its action exists is the "visible and dead"
    // failure the spec forbids (§1.1).
    _ => false,
}
```

- [ ] **Step 5: Run the full suite**

```
cargo test --jobs 4
```

Expected: **PASS**, and the count must be **+1** on 1175 (the new blit test). Any other movement means the refactor changed behaviour — investigate before continuing.

- [ ] **Step 6: Fault injection**

Back up `src/canvas/ops.rs` by content (`cp`, not git). In the blit rebasing site, replace `kind: spot.kind.clone()` with a hardcoded `kind: HotspotKind::Copy { text: String::new(), html: None }`. Run the **full** suite.

Expected: `a_hotspot_carries_its_kind_through_a_blit` **FAILS**. Restore from the backup and re-run to green. If nothing goes red, the test is not reaching the blit path — that is a finding, chase it.

- [ ] **Step 7: Commit**

```bash
git add src/canvas/mod.rs src/canvas/ops.rs src/canvas/tests.rs src/render/button.rs src/tui/app.rs
git commit -F - <<'EOF'
refactor: a hotspot says what it is, not just what it copies

Hotspot grows a kind and a target id. The copy button stops being a
parallel mechanism and becomes one case of a general one, which is what
lets a link, an anchor and a footnote marker share its hit-test.

No behaviour changes. A press on a non-Copy hotspot does nothing yet,
deliberately: nothing records one until the next commit.
EOF
```

---

### Task 2: Links record hotspots

The geometric heart of the feature. **Read the three traps above first.**

**Files:**
- Modify: `src/render/inline.rs` — `Piece` (~line 40), `flatten` (~191), `reconcile` (~240), `link` (~337)
- Test: `tests/link_hotspots.rs` (new)

**Interfaces:**
- Consumes: `canvas::{Hotspot, HotspotKind}` from Task 1.
- Produces: `render::inline` emits `Hotspot`s onto the canvas alongside `SearchSpan`s. Later tasks read them only through `canvas.hotspots()`.

- [ ] **Step 1: Write the failing tests**

Create `tests/link_hotspots.rs`. These cover §9's required list plus the two traps:

```rust
//! Hotspot geometry is render-time: assert on `canvas.hotspots()`, no terminal needed.

use mdmost::canvas::HotspotKind;

/// Renders `markdown` at `width` and returns its hotspots, cheapest path only.
fn hotspots(markdown: &str, width: u16) -> Vec<(usize, u16, u16, HotspotKind, usize)> {
    let doc = mdmost::Doc::parse(markdown);
    let canvas = mdmost::render::document(&doc, width, &mdmost::Theme::default_dark(), &Default::default());
    canvas
        .hotspots()
        .iter()
        .map(|s| (s.row, s.col, s.cols, s.kind.clone(), s.target))
        .collect()
}

#[test]
fn a_link_records_a_hotspot_over_its_text_and_its_printed_url() {
    let spots = hotspots("[docs](https://example.com/a)\n", 60);
    assert_eq!(spots.len(), 1, "one link, one row, one hotspot");
    let (row, col, cols, kind, _) = &spots[0];
    assert_eq!(*row, 0);
    assert_eq!(*col, 0);
    // "docs" is 4 columns, " (https://example.com/a)" is 24. The suffix is a
    // synthetic decoration carrying no source span, and it is still part of the
    // control (design spec §2.1).
    assert_eq!(*cols, 28);
    assert_eq!(
        *kind,
        HotspotKind::Open { url: "https://example.com/a".to_string() }
    );
}

#[test]
fn a_hotspot_carries_the_full_url_even_when_the_drawn_one_is_elided() {
    // elide_middle puts a `…` in the middle of a long URL for display. The status
    // bar must show the whole thing (§8) and the opener must receive the whole
    // thing (§7), so the hotspot may not carry what was drawn.
    let long = "https://example.com/a/very/long/path/that/will/certainly/be/elided/for/display/purposes/index.html";
    let spots = hotspots(&format!("[x]({long})\n"), 60);
    assert_eq!(
        spots[0].3,
        HotspotKind::Open { url: long.to_string() },
        "the hotspot carries the drawn, elided URL instead of the real one"
    );
}

#[test]
fn a_wrapped_link_is_several_hotspots_sharing_one_target() {
    // Narrow enough that the link's text and its suffix cannot share a row.
    let spots = hotspots("[a fairly long link label](https://example.com/somewhere)\n", 24);
    assert!(spots.len() >= 2, "expected the link to wrap, got {spots:?}");
    let target = spots[0].4;
    assert!(
        spots.iter().all(|s| s.4 == target),
        "every row of one link shares one target id, or it breaks in half under \
         the pointer (design spec §2.2)"
    );
    let rows: Vec<usize> = spots.iter().map(|s| s.0).collect();
    assert!(rows.windows(2).all(|w| w[0] != w[1]), "one hotspot per row");
}

#[test]
fn an_autolink_records_a_hotspot_although_it_prints_no_suffix() {
    // `link()` returns early when the text already is the target. The suffix is
    // absent; the link is not.
    let spots = hotspots("<https://example.com/a>\n", 60);
    assert_eq!(spots.len(), 1, "an autolink is still a link");
    assert_eq!(spots[0].2, 21, "the hotspot covers the drawn text");
}

#[test]
fn a_link_in_a_table_cell_records_a_hotspot_although_it_prints_no_suffix() {
    // `link()` returns early when table_depth > 0.
    let spots = hotspots("| a |\n| --- |\n| [go](https://example.com/a) |\n", 60);
    assert_eq!(spots.len(), 1, "a table-cell link is still a link");
    assert_eq!(
        spots[0].3,
        HotspotKind::Open { url: "https://example.com/a".to_string() }
    );
}

#[test]
fn a_link_in_a_block_quote_records_a_hotspot_at_its_drawn_column() {
    let spots = hotspots("> [go](https://example.com/a)\n", 60);
    assert_eq!(spots.len(), 1);
    assert!(
        spots[0].1 >= 2,
        "the quote bar and its gap sit left of the link, so the hotspot cannot \
         start at column 0; got {}",
        spots[0].1
    );
}

#[test]
fn a_link_in_a_list_item_records_a_hotspot_past_the_marker() {
    let spots = hotspots("- [go](https://example.com/a)\n", 60);
    assert_eq!(spots.len(), 1);
    assert!(spots[0].1 >= 2, "the bullet sits left of the link");
}

#[test]
fn ordinary_prose_records_no_hotspot() {
    // The coverage asymmetry §9.1 warns about: test that a non-hotspot cell does
    // NOT react, not only that a hotspot does.
    assert!(hotspots("just some words\n", 60).is_empty());
}
```

- [ ] **Step 2: Run them and watch every one fail**

```
cargo test --jobs 4 --test link_hotspots
```

Expected: all fail — nothing records a link hotspot yet. `ordinary_prose_records_no_hotspot` will **pass** trivially; that is fine and expected, it guards the other direction from here on.

- [ ] **Step 3: Tag the pieces**

`Piece` gains a control tag. In `src/render/inline.rs`:

```rust
/// The control a piece belongs to, if any.
///
/// Parallel to `Origin`, and deliberately not part of it: `Origin` answers "which
/// source bytes did this draw", and a link's printed suffix belongs to the control
/// while belonging to no source bytes at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Control {
    /// Which control — one id per link, shared by all its pieces.
    id: usize,
    /// What activating it does.
    kind: HotspotKind,
}
```

with `control: Option<Control>` on `Piece` (defaulting to `None` in `synthetic`, `transcribable` and every other constructor) and on `Anchored`.

`flatten` propagates it: every branch already builds `Anchored`; add `control: piece.control.clone()` to each of the three.

- [ ] **Step 4: Emit the hotspots in `reconcile`**

`reconcile` already walks per-row and already merges contiguous runs. Give it a second accumulator alongside `run`, obeying the same close-on-discontinuity discipline:

```rust
// A control run grows while the cluster belongs to the SAME control and stays
// column-contiguous. It closes at a row end exactly as a search run does, which
// is what makes a wrapped link several hotspots sharing one target — the shape
// design spec §2.2 asks for, for free.
let mut control: Option<(usize, Hotspot)> = None;
```

and, per cluster, after the existing `match`:

```rust
match entry.and_then(|entry| entry.control.clone()) {
    Some(Control { id, kind }) => match control.as_mut() {
        Some((open, spot)) if *open == id && spot.col + spot.cols == col => {
            spot.cols += cols;
        }
        _ => {
            if let Some((_, spot)) = control.take() {
                spots.push(spot);
            }
            control = Some((
                id,
                Hotspot { row, col, cols, kind, target: id },
            ));
        }
    },
    None => {
        if let Some((_, spot)) = control.take() {
            spots.push(spot);
        }
    }
}
```

closing `control` at the end of each row alongside `run`, and returning `spots` for the caller (~line 184) to `add_hotspot` in the same loop that adds spans.

- [ ] **Step 5: Tag the link's pieces**

In `link()` — and note this must happen on **all four paths**, including the three early returns (trap 2):

```rust
fn link(node: &Node, url: &str, style: Style, ctx: Ctx<'_>, out: &mut Vec<Piece>) {
    let theme = ctx.theme;
    let before = out.len();
    collect(&node.children, style.patch(theme.text.link), ctx, out);
    let text: String = out[before..].iter().map(|piece| piece.text.as_str()).collect();
    let target = url.trim();

    // Task 3 replaces this with the scheme classifier. Until then every non-empty
    // target is Open, which is wrong for `mailto:` and is why Task 3 exists.
    let control = (!target.is_empty()).then(|| Control {
        id: ctx.next_control_id(),
        // The FULL url, never the elided form the suffix draws (trap 1).
        kind: HotspotKind::Open { url: url.to_string() },
    });
    for piece in &mut out[before..] {
        piece.control = control.clone();
    }

    if target.is_empty()
        || text.trim() == target
        || text.trim() == target.trim_start_matches("mailto:")
    {
        return;
    }
    if ctx.table_depth > 0 && !text.trim().is_empty() {
        return;
    }
    let mut suffix = Piece::synthetic(
        format!(" ({})", elide_middle(url, URL_BUDGET)),
        style.patch(theme.text.link_url),
    );
    // §2.1: the suffix carries no source span and IS part of the control.
    suffix.control = control;
    out.push(suffix);
}
```

`Ctx` needs `next_control_id()` — a `&Cell<usize>` counter threaded through the render context, incremented per link. Ids must be unique **per canvas**; reconcile passes them straight through to `Hotspot::target`.

- [ ] **Step 6: Run the tests**

```
cargo test --jobs 4 --test link_hotspots
```

Expected: all PASS. Then the full suite: `cargo test --jobs 4`. Investigate any pre-existing test that moves — a link that now records a hotspot must not have changed a single drawn cell.

- [ ] **Step 7: Fault injection, three mutations**

Back up by content first. Run the **full** suite for each.

1. Drop the suffix's `control` assignment (`suffix.control = None`). Expect `a_link_records_a_hotspot_over_its_text_and_its_printed_url` red on `cols`.
2. Use the drawn text instead of the full URL (`elide_middle(url, URL_BUDGET)` into the `kind`). Expect `a_hotspot_carries_the_full_url_even_when_the_drawn_one_is_elided` red.
3. Give each row a fresh id (`target: out_target_counter()` inside reconcile instead of `id`). Expect `a_wrapped_link_is_several_hotspots_sharing_one_target` red.

Any mutation that turns nothing red is a finding about the test. Restore from backup between each.

- [ ] **Step 8: Commit**

```bash
git add src/render/inline.rs tests/link_hotspots.rs
git commit -F - <<'EOF'
feat: a link claims the cells it draws

Links record hotspots through reconcile, the per-row walk that already
emits search spans from wrapped inline content -- so a wrapped link is
several hotspots sharing one target id, and hovering any row will light
every row of it.

Three things the spec does not say and the code does. The hotspot carries
the FULL url, not the elided form drawn on screen. An autolink and a
table-cell link print no ` (url)` suffix and are still links, so the tag
goes on before those early returns, not after. And the suffix belongs to
the control while belonging to no source bytes -- a hotspot is a claim on
cells, which is the one place it and a search span are allowed to
disagree.
EOF
```

---

### Task 3: The scheme allowlist, and the URL in the status bar

Security, not scope. **Only `http` and `https` get a hotspot**, so a document the reader did not write cannot make the pager launch an arbitrary desktop handler (§8).

**Files:**
- Create: `src/render/link.rs`
- Modify: `src/render/inline.rs` (`link`, from Task 2), `src/render/mod.rs` (`mod link;`)
- Modify: `src/tui/draw.rs` (status bar)
- Test: `src/render/link.rs` unit tests; `tests/link_hotspots.rs` additions

**Interfaces:**
- Produces: `render::link::classify(url: &str) -> Option<HotspotKind>` — the single answer to "does this URL get a hotspot, and what kind".

- [ ] **Step 1: Write the failing tests**

In `src/render/link.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_and_https_are_the_only_schemes_that_open() {
        assert_eq!(
            classify("https://example.com"),
            Some(HotspotKind::Open { url: "https://example.com".to_string() })
        );
        assert_eq!(
            classify("http://example.com"),
            Some(HotspotKind::Open { url: "http://example.com".to_string() })
        );
    }

    #[test]
    fn every_other_scheme_is_inert() {
        // A hotspot here would let a document the reader did not write choose which
        // desktop handler the pager launches (design spec §8).
        for url in [
            "mailto:a@b.c",
            "ftp://example.com/x",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "vscode://file/etc/passwd",
            "HTTPS://example.com/../../x", // case handled, but see below
        ] {
            if url.starts_with("HTTPS") {
                continue;
            }
            assert_eq!(classify(url), None, "{url} must record no hotspot");
        }
    }

    #[test]
    fn a_scheme_is_matched_case_insensitively() {
        // `HTTPS://` is a legal URL and a reader would expect it to work; more to
        // the point, a case-sensitive check is an allowlist with a hole in it.
        assert!(matches!(classify("HTTPS://example.com"), Some(HotspotKind::Open { .. })));
    }

    #[test]
    fn a_fragment_is_an_anchor() {
        assert_eq!(
            classify("#some-heading"),
            Some(HotspotKind::Anchor { slug: "some-heading".to_string() })
        );
    }

    #[test]
    fn a_local_markdown_link_is_wholly_inert() {
        // Not "lights up and declines" -- a control that appears live and refuses is
        // worse than one never offered (design spec §1.1). Until the navigation
        // spec lands this records nothing at all.
        assert_eq!(classify("./other.md"), None);
        assert_eq!(classify("other.md"), None);
        assert_eq!(classify("/abs/other.md"), None);
    }
}
```

- [ ] **Step 2: Run and watch them fail**

```
cargo test --jobs 4 --lib render::link
```

Expected: compile error — `classify` does not exist.

- [ ] **Step 3: Implement the classifier**

```rust
//! Which link targets become controls, and which stay inert.
//!
//! An allowlist, and deliberately a short one. Only `http` and `https` get a
//! hotspot: this is a security decision as much as a scope one, because a document
//! the reader did not write must not be able to choose which desktop handler the
//! pager launches (design spec §8). Everything unrecognised is inert, so a scheme
//! nobody has thought of yet fails closed.

use crate::canvas::HotspotKind;

/// The control a link target earns, or `None` for an inert link.
pub(crate) fn classify(url: &str) -> Option<HotspotKind> {
    let target = url.trim();
    if let Some(slug) = target.strip_prefix('#') {
        return (!slug.is_empty()).then(|| HotspotKind::Anchor {
            slug: slug.to_ascii_lowercase(),
        });
    }
    let scheme = target.split_once("://").map(|(scheme, _)| scheme)?;
    matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
        .then(|| HotspotKind::Open { url: target.to_string() })
}
```

Then in `link()`, replace Task 2's placeholder with `classify(url).map(|kind| Control { id: ctx.next_control_id(), kind })`.

- [ ] **Step 4: Add the render-level guard**

In `tests/link_hotspots.rs`:

```rust
#[test]
fn an_inert_link_records_no_hotspot_but_still_draws() {
    let spots = hotspots("[mail](mailto:a@b.c) and [doc](./other.md)\n", 60);
    assert!(spots.is_empty(), "inert schemes record nothing: {spots:?}");
}
```

- [ ] **Step 5: Show the full URL in the status bar on hover**

**The status bar is `chrome::draw_status` (`src/tui/chrome.rs:202`), not `draw.rs`** — the plan said `draw.rs` and was wrong.

It is not a line you append to. It is two `Vec<Segment>` (left and right), where every segment carries a `Drop` priority: when the bar is too narrow, **whole segments are dropped, cheapest first**, and only the filename is allowed to lose characters. `Drop::Never` segments (the quit hint) never go.

So the hovered URL is a **new `Segment`**, and its priority is a real decision:

- §8 makes the visible URL the safeguard that stands in for a confirmation prompt. A URL silently dropped because the terminal was narrow is that safeguard failing exactly when the reader needed it. Give it a priority **above** the breadcrumb, the search chip and the meter — all of which are restated somewhere the reader can already see, which is why they are cheap.
- It may still not fit. Elide it **at the end** (`crate::text::ellipsize`), never in the middle: `elide_middle` is right for the *drawn* suffix, where both ends carry meaning, and wrong here, because the thing the reader is checking before they commit is the **host**, which lives at the front.
- The status bar never lies: an elided URL must show its `…`, and hovering a non-`Open` hotspot must show no URL segment at all rather than a stale one.

Test in `src/tui/tests.rs`:

```rust
#[test]
fn hovering_a_link_shows_its_full_url_in_the_status_bar() {
    let mut app = app_with("[x](https://example.com/a/path)\n", 60, 10);
    app.set_pointer(0, 0);
    let status = status_line(&draw_to_canvas(&app));
    assert!(
        status.contains("https://example.com/a/path"),
        "the status bar must show where the link goes; it said {status:?}"
    );
}
```

- [ ] **Step 6: Full suite, then fault injection**

Mutations, full suite each, restore from a content backup between:

1. Widen the allowlist to any scheme (`Some(HotspotKind::Open { .. })` unconditionally). Expect `every_other_scheme_is_inert` **and** `an_inert_link_records_no_hotspot_but_still_draws` red.
2. Make the scheme check case-sensitive. Expect `a_scheme_is_matched_case_insensitively` red.
3. Put the elided URL in the status bar. Expect `hovering_a_link_shows_its_full_url_in_the_status_bar` red.

- [ ] **Step 7: Commit**

```bash
git add src/render/link.rs src/render/mod.rs src/render/inline.rs src/tui/draw.rs src/tui/tests.rs tests/link_hotspots.rs
git commit -F - <<'EOF'
feat: only http and https become controls, and hover says where they go

An allowlist that fails closed: an unrecognised scheme records no hotspot,
so a document the reader did not write cannot choose which desktop handler
the pager launches. mailto, ftp, file and every local .md link stay wholly
inert -- they do not light up and then decline, which would be worse than
never offering the control.

Hovering shows the FULL url in the status bar, elided at the end if it
must be, so the host stays visible. That is the safeguard standing in for
a confirmation prompt.
EOF
```

---

### Task 4: Links react to the pointer — OWNER GATE

**Stop at Step 4 and show the owner.** The colours are deliberately unnamed in the spec (§3.1); the owner settles them by looking, as they did for the copy button.

**Files:**
- Modify: `src/tui/draw.rs`
- Test: `src/tui/tests.rs`, `tests/theme_contrast.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn hovering_a_link_shades_every_row_of_it() {
    // A wrapped link is several hotspots sharing one target; hovering any row must
    // light all of them, or the link visibly breaks in half under the pointer.
    let mut app = app_with("[a fairly long link label](https://example.com/x)\n", 24, 10);
    app.set_pointer(0, 0);
    let canvas = draw_to_canvas(&app);
    let lit: Vec<usize> = (0..3)
        .filter(|row| row_has_hovered_style(&canvas, *row))
        .collect();
    assert!(lit.len() >= 2, "only row(s) {lit:?} lit; the link wraps");
}

#[test]
fn ordinary_prose_under_the_pointer_is_not_shaded() {
    // The §9.1 asymmetry, stated as a test: prove the non-hotspot direction too.
    let mut app = app_with("just some words\n", 60, 10);
    app.set_pointer(2, 0);
    assert!(!row_has_hovered_style(&draw_to_canvas(&app), 0));
}
```

- [ ] **Step 2: Run and watch them fail**

```
cargo test --jobs 4 hovering_a_link_shades_every_row_of_it
```

- [ ] **Step 3: Paint it**

`draw.rs` already applies `Theme::hovered` to the hovered copy button by index. Generalise: hover lights **every hotspot sharing the hovered one's `target`**, not the single index. That one change is what makes §2.2 visible.

`HOVER_SHIFT` is 0.6 and settled (owner ruling 2026-08-12) — reuse `Theme::hovered`, do not add a second constant.

- [ ] **Step 4: OWNER GATE — render every state in every theme**

Build a release binary in the tryout worktree and write a page exercising: an inert link, a live link at rest, the same link hovered, a wrapped link hovered, an anchor link, and a link in a table cell. **Every state the page names must actually be reachable** — that is a promise about what works, and a page listing an unimplemented state sends the owner hunting for a difference that cannot exist.

Show the owner. Do not proceed until they have looked.

- [ ] **Step 5: Extend the contrast test**

Add the hovered link to `tests/theme_contrast.rs` on the same terms as the copy button: it clears the 3:1 non-text floor in every theme, and it moves *away* from the page (lighter on dark, darker on light). Re-measure and record the numbers in the doc comment rather than asserting they were verified.

- [ ] **Step 6: Fault injection**

Light only the hovered index rather than every hotspot with that target. Expect `hovering_a_link_shades_every_row_of_it` red. Invert the blend towards `palette.bg`; expect the contrast test red.

- [ ] **Step 7: Commit**

---

### Task 5: The click state machine

**A click is press and release on the same hotspot with no intervening drag.** **Selection wins every tie** — no gesture that works today changes behaviour.

**Files:**
- Modify: `src/tui/app.rs` (press/release; `pending_hotspot` neighbours), `src/tui/term.rs` (~lines 384, 399, 403)
- Test: `src/tui/tests.rs`

**Interfaces:**
- Produces: `App::press_hotspot(x, y) -> bool`, `App::release_hotspot(x, y) -> Option<Activation>`, `Activation { kind: HotspotKind }`.

- [ ] **Step 1: Write the failing tests — a pure state machine, no mouse needed**

```rust
#[test]
fn press_then_release_on_one_hotspot_fires() {
    let mut app = app_with("[x](https://example.com/a)\n", 60, 10);
    assert!(app.press_hotspot(0, 0));
    let fired = app.release_hotspot(0, 0);
    assert!(matches!(fired, Some(a) if a.kind == HotspotKind::Open {
        url: "https://example.com/a".to_string()
    }));
}

#[test]
fn press_drag_release_fires_nothing_and_yields_a_selection() {
    // Selection wins every tie (design spec §3).
    let mut app = app_with("[x](https://example.com/a) and more text here\n", 60, 10);
    app.press_hotspot(0, 0);
    app.on_drag(30, 0);
    assert!(app.release_hotspot(30, 0).is_none(), "a drag is not a click");
    assert!(app.selection().is_some(), "the gesture became a selection");
}

#[test]
fn press_on_one_hotspot_and_release_on_another_fires_nothing() {
    let mut app = app_with("[a](https://example.com/a) [b](https://example.com/b)\n", 80, 10);
    app.press_hotspot(0, 0);
    let second = 30;
    assert!(app.release_hotspot(second, 0).is_none());
}

#[test]
fn moving_off_the_pressed_cell_and_back_does_not_resurrect_the_candidate() {
    // "Moving off cancels" must mean cancelled, not suspended.
    let mut app = app_with("[x](https://example.com/a) tail\n", 60, 10);
    app.press_hotspot(0, 0);
    app.on_drag(40, 0);
    app.on_drag(0, 0);
    assert!(app.release_hotspot(0, 0).is_none());
}
```

- [ ] **Step 2: Run and watch them fail.**

- [ ] **Step 3: Implement.** A `pressed: Option<usize>` (a target id) on `App`. Press records it; any drag event clears it; release fires only if the released hotspot has the same target.

- [ ] **Step 4: Wire `src/tui/term.rs`.** The `Down`/`Drag`/`Up` arms already exist and already branch on selection state. The hotspot candidate must be recorded **before** the selection path claims the press, and cleared by the existing drag arm.

- [ ] **Step 5: Full suite.** Every existing selection and copy-button test must still pass untouched. If one moves, the tie went the wrong way.

- [ ] **Step 6: Fault injection.** Make release fire without comparing targets — expect `press_on_one_hotspot_and_release_on_another_fires_nothing` red. Stop clearing on drag — expect the drag tests red.

- [ ] **Step 7: Commit.**

---

### Task 6: The opener, behind a seam

**Files:**
- Create: `src/tui/open.rs`
- Modify: `src/tui.rs` (`pub mod open;`), `src/tui/app.rs`
- Test: `src/tui/open.rs` unit tests

**Interfaces:**
- Produces: `open::open(url: &str) -> Outcome`, mirroring the clipboard seam. **Read `src/tui/clipboard.rs` first and mirror its shape** — it already solves "assert what would have happened without doing it", and a second, different answer to the same problem is the duplication this project keeps removing.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_command_is_built_with_a_direct_argv_and_no_shell() {
    // Nothing in a URL may be interpolated into a command line. A URL is attacker
    // -controlled data: `; rm -rf ~` is a legal thing to find in a document.
    let argv = command_for("https://example.com/a;%20rm%20-rf%20~");
    assert_eq!(argv.1.len(), 1, "exactly one argument, the url itself");
    assert_eq!(argv.1[0], "https://example.com/a;%20rm%20-rf%20~");
    assert!(
        !argv.0.contains("sh") && !argv.0.contains("cmd"),
        "no shell may be involved on unix; got {}",
        argv.0
    );
}

#[test]
fn a_url_that_looks_like_a_flag_is_still_one_argument() {
    // `--foo` as a URL must not be read as an option by the opener.
    let argv = command_for("https://example.com/--version");
    assert_eq!(argv.1.len(), 1);
}
```

- [ ] **Step 2: Run and watch them fail.**

- [ ] **Step 3: Implement.** `xdg-open` on Linux, `open` on macOS, `cmd /c start` on Windows — the Windows form needs its documented empty-title argument, and that is the one platform where an extra argv entry is correct. Spawn **detached**; the UI never blocks on the child. Keep stderr off the alternate screen through the existing `stderr` module. Report failure in the status bar.

- [ ] **Step 4: Run.** Then wire `Activation { kind: Open { url } }` to it in `App`.

- [ ] **Step 5: Fault injection.** Route through `sh -c` instead of a direct argv — expect `the_command_is_built_with_a_direct_argv_and_no_shell` red. This is the security-load-bearing test; if it does not go red, it is not testing what it claims.

- [ ] **Step 6: Commit.**

---

### Task 7: Anchors

**Files:**
- Modify: `src/tui/app.rs`
- Test: `src/tui/tests.rs`

Slugs come from `doc::Heading::id` — **the same enumeration the TOC uses** (`src/doc/slug.rs`, `Slugger`). One source of truth, so an anchor and the TOC cannot disagree about what a heading is called. Do not write a second slugger.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn an_anchor_scrolls_its_heading_to_the_top_row() {
    let doc = format!("[go](#target)\n\n{}\n## Target\n\nbody\n", "filler\n\n".repeat(40));
    let mut app = app_with(&doc, 60, 10);
    app.activate(HotspotKind::Anchor { slug: "target".to_string() });
    assert_eq!(heading_text_at_row(&app, 0), "Target");
}

#[test]
fn a_duplicate_heading_resolves_to_the_right_one() {
    // Slugger suffixes duplicates -1, -2. `#setup-1` is the SECOND "Setup".
    let doc = format!("## Setup\n\n{}\n## Setup\n\nsecond\n", "filler\n\n".repeat(40));
    let mut app = app_with(&doc, 60, 10);
    app.activate(HotspotKind::Anchor { slug: "setup-1".to_string() });
    assert!(visible_text(&app).contains("second"));
}

#[test]
fn an_unknown_anchor_reports_and_does_not_move() {
    let mut app = app_with("# A\n\nbody\n", 60, 10);
    let before = app.scroll();
    app.activate(HotspotKind::Anchor { slug: "nope".to_string() });
    assert_eq!(app.scroll(), before, "it must scroll nowhere");
    assert!(
        status_line(&draw_to_canvas(&app)).contains("nope"),
        "the status bar never lies -- it must say the anchor matched nothing"
    );
}
```

- [ ] **Steps 2–6:** run red, implement against `Doc::headings()`, run green, inject (resolve to the *first* matching heading rather than the exact slug — expect the duplicate test red; make an unknown anchor scroll to 0 — expect the third red), commit.

---

### Task 8: The keyboard cursor

This **resolves** the gating rule rather than inheriting it (§4). Copy buttons hide when mouse capture fails; links must not, because a keyboard cursor makes them reachable over SSH and in terminals without mouse support. Links are therefore **never hidden**.

**Binding, chosen against the existing table in `README.md` §keys:** `f` cycles forward through the hotspots on screen ("follow", and free — `ctrl-f` is scroll, plain `f` is unbound), `F` cycles backward, `enter` activates, `esc` drops the cursor. Verify `f` is still free before implementing; if it is not, say so rather than colliding.

**Files:** `src/tui/app.rs`, `src/tui/term.rs`, `README.md`, `man/`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn f_cycles_the_cursor_through_the_hotspots_on_screen() {
    let mut app = app_with("[a](https://e.com/a) [b](https://e.com/b)\n", 80, 10);
    app.on_key(key('f'));
    let first = app.cursor_target().expect("f put the cursor on the first control");
    app.on_key(key('f'));
    assert_ne!(app.cursor_target(), Some(first), "f must advance");
}

#[test]
fn the_cursor_wraps_at_the_end() {
    let mut app = app_with("[a](https://e.com/a) [b](https://e.com/b)\n", 80, 10);
    app.on_key(key('f'));
    let first = app.cursor_target();
    app.on_key(key('f'));
    app.on_key(key('f'));
    assert_eq!(app.cursor_target(), first);
}

#[test]
fn enter_activates_the_control_under_the_cursor() {
    let mut app = app_with("[a](https://e.com/a)\n", 60, 10);
    app.on_key(key('f'));
    assert!(app.on_key(key_enter()).fired_activation());
}

#[test]
fn the_cursor_survives_no_mouse_capture() {
    // The whole point: links are reachable without a mouse, which is what earns
    // them the right never to be hidden (design spec §4).
    let mut app = app_without_mouse("[a](https://e.com/a)\n", 60, 10);
    app.on_key(key('f'));
    assert!(app.cursor_target().is_some());
}

#[test]
fn a_copy_button_is_still_hidden_without_mouse_capture() {
    // The gating rule is RESOLVED for links, not repealed for buttons.
    let app = app_without_mouse("```rust\nfn a() {}\n```\n", 60, 10);
    assert!(!visible_text(&app).contains("[copy]"));
}
```

- [ ] **Steps 2–6:** run red, implement reusing the TOC pane's cursor pattern, run green, inject (make the cursor skip hotspots off-screen incorrectly; make `f` not wrap), update `README.md`'s key table and the man page, commit.

---

### Task 9: The footnote popup

The largest task. A bordered box adjacent to the marker, sized to content up to a cap, flipping above/below and left/right to stay on screen.

**The footnote renders through the ordinary renderer at the popup's width.** Rendering is already a pure function of width, so a popup is another width, not a second rendering path — formatting, code spans and lists inside a footnote work for free. **Do not write a second renderer.**

**Files:**
- Create: `src/tui/popup.rs`
- Modify: `src/tui/app.rs` (popup state), `src/tui/draw.rs` (paint), `src/render/inline.rs` (footnote markers record `HotspotKind::Footnote`)
- Test: `tests/footnote_popup.rs` (new)

- [ ] **Step 1: Write the failing tests — layout is testable on a canvas, no terminal**

```rust
#[test]
fn a_popup_sizes_itself_to_its_content_up_to_the_cap() { /* … */ }

#[test]
fn a_popup_near_the_bottom_flips_above_the_marker() { /* … */ }

#[test]
fn a_popup_near_the_right_edge_flips_left() { /* … */ }

#[test]
fn a_footnote_with_a_list_in_it_renders_through_the_ordinary_renderer() {
    // The proof that this is "another width", not a second rendering path.
    let app = open_footnote("[^1]\n\n[^1]: intro\n\n    - one\n    - two\n");
    let text = popup_text(&app);
    assert!(text.contains("one") && text.contains("two"));
    assert!(text.contains('•') || text.contains('-'), "the list drew its markers");
}

#[test]
fn a_long_footnote_scrolls_inside_the_popup() { /* … */ }

#[test]
fn links_inside_a_popup_are_inert() {
    // Design spec §1.1.
    let app = open_footnote("[^1]\n\n[^1]: see [x](https://example.com/a)\n");
    assert!(app.popup_hotspots().is_empty());
}

#[test]
fn esc_dismisses_the_popup_and_does_not_quit() {
    let mut app = open_footnote("[^1]\n\n[^1]: note\n");
    app.on_key(key_esc());
    assert!(app.popup().is_none());
    assert!(!app.should_quit(), "Esc never quits");
}

#[test]
fn scrolling_the_document_dismisses_the_popup() { /* … */ }

#[test]
fn a_click_outside_dismisses_the_popup() { /* … */ }
```

Fill every `/* … */` with a real body before starting — a placeholder here is a plan failure. Each follows the shape of the first: build an app, open the footnote, assert on the drawn canvas.

- [ ] **Steps 2–7:** run red; implement layout in `popup.rs` (pure geometry, unit-testable); render the footnote's nodes through `render::document` at the popup's inner width; paint in `draw.rs`; wire dismissal to `Esc`, an outside click and any document scroll; run green; inject (remove the bottom flip, remove the right flip, let popup links record hotspots — each must turn its own test red); commit.

---

### Task 10: The demo

**Files:** `demo/mdmost.toml`, `demo/tour.md`, `docs/demo/mdmost.webp`, `docs/maintainer-notes.md`

Owner requirements, recorded 2026-08-12:

1. **The demo must show a footnote popup click.**
2. **The first and final panel hold 3× longer**, so they can be read. *(Confirm this reading with the owner before recording — it was given alongside a comment that was later withdrawn.)*
3. The recording must show the new capabilities generally: link hover, a click that opens, and the selection that now hugs text rather than the row.

- [ ] **Step 1: Re-read the recipe** — `docs/maintainer-notes.md` §"Regenerating the demo". Reference repo `~/checkouts/ansidrama`.

- [ ] **Step 2: Check the widths still hold.** `demo/tour.md` is written to the widths act 2's drag passes through: the three-column table has two-line cells through 59 columns and single-line rows from 60. Verify before recording — much has changed since it was written.

- [ ] **Step 3: Add the new beats to `demo/mdmost.toml`** — a link hover, a footnote popup click, and the timing change from requirement 2. `demo/tour.md` needs a footnote and a live link if it has none.

- [ ] **Step 4: Record.** Foreground. Then check the byte size: lossless WebP, under about 2 MB. If heavier, trim act 5's tour beats — never the act 2 drags or the act 4 copies.

- [ ] **Step 5: Watch it.** The act 4 selection should hug the text, not the row. The popup should be legible and on screen.

- [ ] **Step 6: Gates and commit.** All four. No source changed in this task, so the test count must match exactly.

---

## Self-review

Run against the spec before dispatching Task 1.

**Spec coverage.** §1 Tasks 2–9 · §1.1 inert local/other schemes Task 3, inert popup links Task 9, no OSC 8 (nothing emits it; no task adds it) · §2 kind Task 1 · §2.1 whole link incl. suffix Task 2 · §2.2 shared target Tasks 2 and 4 · §2.3 paint-time seam Tasks 4 and 9 · §3 Task 5 · §3.1 Task 4 (owner gate) · §4 Task 8 · §5 Task 7 · §6 Task 9 · §7 Task 6 · §8 Task 3 · §9 testing throughout · §9.1 both directions — `ordinary_prose_records_no_hotspot` (Task 2) and `ordinary_prose_under_the_pointer_is_not_shaded` (Task 4) · §10 sequencing: this plan lands after the selection work, which is complete.

**Known gap, deliberate.** `--render-once` records no hotspots (§4 last line). No task asserts this. **Add it to Task 2 Step 1** as `render_once_records_no_hotspots` rather than trusting that it falls out — the copy button's equivalent rule is enforced by an explicit check, not by accident.

**Type consistency.** `HotspotKind` variants are spelled identically in Tasks 1, 2, 3, 5, 6, 7 and 9. `Hotspot::target` is `usize` throughout and is the same number as `Control::id`. `classify` returns `Option<HotspotKind>` and is the only producer of a link's kind after Task 3.

**Risk to watch.** Task 2 threads a counter through `Ctx`, which is shared by every inline renderer. If `Ctx` is `Copy`, a plain `usize` field will silently give each subtree its own numbering and every link will collide on id 0 — hence the `&Cell<usize>`. The wrapped-link test is what catches this, and it is the reason that test asserts *shared* ids rather than merely counting hotspots.
