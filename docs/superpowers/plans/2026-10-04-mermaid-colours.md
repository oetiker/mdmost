# Mermaid Colours Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flowchart, state, class and ER diagrams read `classDef`, `class`, `cssClass`, `:::` and `style`, and draw each class in one of 16 theme hues (border, interior tint, heavy outline, frame tint) while every uncoloured diagram renders exactly as before.

**Architecture:** Parsers stay theme-free: a shared `Sheet` collects the colour statements and attaches a merged `ast::Paint` to each node, subgraph and composite state. A pure resolver (`mermaid::paint`) maps the diagram's paints to 16 fixed-angle slots. The theme carries 16 contrast-repaired `SlotInk`s. At draw time a `Painter` turns paint plus slot into a `NodeStyle` (nodes) or a `FrameStyle` (frames); the graph engine records tinted frame rectangles and tints them in one pass on the finished canvas.

**Tech Stack:** Rust 2024 (crate `mdmost`), `insta` snapshots, `proptest`.

**Spec:** `docs/superpowers/specs/2026-10-03-mermaid-colours-design.md` (approved; read it beside this plan, section numbers below are its own).

## Global Constraints

- Never more than 4 cores: every cargo command takes `-j4`, every test run `-- --test-threads=4`.
- Every cargo command runs under `systemd-run --user --scope -q -p MemoryMax=8G --`, for example `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 -- --test-threads=4`. Pass `timeout: 600000` on every long Bash call and never end a turn while a background command runs.
- Gates at the end of every task, all three green:
  - `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 -- --test-threads=4` (baseline at `af91a4c`: 1840 passed, 0 failed; the count only grows).
  - `systemd-run --user --scope -q -p MemoryMax=8G -- cargo clippy -j4 --all-targets -p mdmost --no-deps -- -D warnings` (the workspace-wide form fails on vendored syntect; that is known and not ours).
  - `cargo fmt --check`.
- `#![warn(missing_docs)]` and `clippy::doc_markdown` are on: every `pub` item needs a doc comment, and identifiers in doc comments go in backticks (`` `classDef` ``).
- The snapshot corpus must not change. An `insta` snapshot diff is a stop-and-report event: do not accept it, report it.
- No em dashes in prose, comments or commit messages. English identifiers and comments. Comments explain why, in the dense house style of the surrounding file.
- Never write to `/tmp`; scratch files go to `/scratch/oetiker/claude-tmp/...`. Never run `find /home/oetiker`.
- Commit each task with explicit paths (`git add <paths>`), never `git add -A`. Messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push.
- The only binary that runs in the sandbox is `/home/oetiker/scratch/cargo-target/debug/mdmost`, called straight from bash.
- `render_mermaid` returns `Err(TooNarrow)` at small widths: render tests use width 100 or more.
- New glyphs (`┯ ┷ ┠ ┨ ╍ ╏ ┿ ╂`) are all in Box Drawing (U+2500-U+257F), which `tests/glyph_inventory.rs` already allows. If that test fails anyway, report it.
- Colour constants (spec, copied): slot nominal angles `0, 15, 30, 37.5, 45, 97.5, 150, 165, 180, 200, 220, 240, 260, 287.5, 315, 337.5`; no hue when HSL saturation `< 0.2` or lightness `< 0.06` or `> 0.97`; `full_tint = bg.blend(ink, 0.15)`, `half_tint = bg.blend(ink, 0.075)`; ink repair `0.05` per step, at most 20; tint repair `0.015` per step, at most 10; heavy when `stroke-width >= 3`.

## Review Focus

1. Painting a diagram must not move anything: the same diagram with and without its colour lines has the same geometry, the same characters (apart from light versus heavy line glyphs) and the same selection spans, at every width. Test: Task 10, `paint_never_changes_layout_or_spans`.
2. Garbage in a colour line (`fill:`, `fill:#zzz`, `stroke-width:1e999px`, `classDef` with no name, `class` with no list, `:::` with no name, non-ASCII) never fails the diagram and never panics. Test: Task 5, `colour_lines_with_garbage_never_fail_the_diagram`, plus new robustness samples.
3. An edge must not attach where a heavy box draws an inner rule or bar (`┠ ┨ ┯ ┷`), the same way it keeps off `├ ┤ ┬ ┴` today. Test: Task 9, `ports_keep_off_heavy_rules`.
4. A diagram with more distinct colours than slots still draws every node; the extra colours share a slot. Test: Task 3, `a_seventeenth_colour_shares_its_nearest_slot`, and Task 10, `more_colours_than_slots_still_draw`.
5. In the light theme a painted border uses the repaired ink, not the raw palette hue (orange draws `#a55806`, not `#b35c00`). Test: Task 10, `the_light_theme_draws_repaired_inks`.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/theme/style.rs` | `Color::relative_luminance`, `Color::contrast` (WCAG 2), moved from the test. |
| `src/theme/slots.rs` (new) | `SlotInk` and the derivation and repair of the 16 slot inks (§5). |
| `src/theme/builtin.rs`, `src/theme/mod.rs` | `Theme::diagram_slots`, filled by `from_palette`. |
| `src/mermaid/ast.rs` | `Paint`, `PaintColor`; `paint` field on `FlowNode`, `Group`, `StateNode`, `Class`, `Entity`. |
| `src/mermaid/paint.rs` (new, `pub mod`) | Property lists, CSS names, merge, hue detection, slot resolution (§3.4, §3.5, §4). Theme-free. |
| `src/mermaid/parse/sheet.rs` (new) | `Sheet`: reads `classDef`/`class`/`cssClass`/`style`, records `:::`, merges per key (§3.2, §3.5, §3.6). |
| `src/mermaid/parse/{flowchart,state,class,er}.rs` | Feed the sheet; attach paints at the end of parsing. |
| `src/canvas/border.rs` | `BorderSet::ROUNDED_HEAVY`, `BorderSet::DASHED_HEAVY`. |
| `src/mermaid/layout/graph/glyph.rs`, `graph.rs` | `╍ ╏` are thick; heavy rule tees keep ports off. |
| `src/mermaid/layout/painted.rs` (new) | `Painter`, `NodeStyle`: paint plus theme to resolved styles (§6.1). |
| `src/mermaid/layout/flowchart/shape.rs`, `state/shape.rs`, `record.rs` | Draw with a `NodeStyle` (§6.2). |
| `src/mermaid/layout/graph/spec.rs`, `graph.rs` | `FrameStyle` on `GroupSpec`, `NodeArt::keeps_page`, frame rectangles, half-tint pass (§6.3). |
| `tests/theme_contrast.rs` | Uses `Color::contrast`; §7 floors; repair palettes. |
| `tests/mermaid_parse_families.rs`, `tests/mermaid_parse_robustness.rs` | Parse tests per family. |
| `tests/mermaid_colours.rs` (new) | Render tests: cell styles and glyphs. |

---

### Task 1: WCAG contrast moves into the theme

**Files:**
- Modify: `src/theme/style.rs` (add two methods after `luminance`, line 78; add tests in the `tests` module)
- Modify: `tests/theme_contrast.rs:22-45` (delete `channel`, `relative_luminance`, `contrast`; call `Color::contrast`)

**Interfaces:**
- Produces: `Color::relative_luminance(self) -> f32`, `Color::contrast(self, other: Color) -> f32` (WCAG 2 ratio in `1.0..=21.0`).

- [ ] **Step 1: Write the failing test** in `src/theme/style.rs` `mod tests`:

```rust
    #[test]
    fn contrast_is_the_wcag_ratio() {
        let black = Color::hex(0x000000);
        let white = Color::hex(0xffffff);
        assert!((black.contrast(white) - 21.0).abs() < 1e-4);
        assert!((white.contrast(black) - 21.0).abs() < 1e-4, "symmetric");
        assert!((white.contrast(white) - 1.0).abs() < 1e-6);
        // The light theme's orange on its page, the tightest ink-on-page pair of the
        // colour spec §5.2.
        let ratio = Color::hex(0xb35c00).contrast(Color::hex(0xfdfcf9));
        assert!((ratio - 4.60).abs() < 0.01, "{ratio}");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --lib theme::style -- --test-threads=4`
Expected: FAIL, "no method named `contrast`".

- [ ] **Step 3: Implement** after `luminance()` in `impl Color`:

```rust
    /// WCAG 2 relative luminance in `0.0..=1.0`.
    ///
    /// Not [`Color::luminance`], which is the BT.601 weighting the palette derivation
    /// shades by. That one answers "which of these is lighter"; a contrast ratio is
    /// defined against this one only.
    pub fn relative_luminance(self) -> f32 {
        fn channel(value: u8) -> f32 {
            let c = f32::from(value) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// The WCAG 2 contrast ratio between two colours, in `1.0..=21.0`.
    ///
    /// Lives here rather than in a test because the theme itself repairs diagram slot
    /// inks against it (colour spec §5.3).
    pub fn contrast(self, other: Color) -> f32 {
        let (x, y) = (self.relative_luminance(), other.relative_luminance());
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }
```

In `tests/theme_contrast.rs` delete `channel`, `relative_luminance` and `contrast` (lines 22-45) and their doc comments; replace every `contrast(a, b)` call with `a.contrast(b)` (`at_least` at line 74 and the calls at lines 284-296, 332-334, 395-396, 459-460, 512). Keep the explanation of why BT.601 is not used: it now lives on `relative_luminance`.

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --lib theme::style -- --test-threads=4` then `... cargo test -j4 --test theme_contrast -- --test-threads=4`
Expected: PASS, theme_contrast unchanged in count.

- [ ] **Step 5: Gates and commit**

Run the three gates. Then:

```bash
git add src/theme/style.rs tests/theme_contrast.rs
git commit -m "refactor: move the WCAG contrast ratio into Color

The colour work repairs diagram slot inks against WCAG contrast inside
Theme::from_palette, so the function the contrast test kept to itself
moves to Color::contrast and the test uses it from there.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `Paint`, property lists, CSS names, hue detection, merge

**Files:**
- Modify: `src/mermaid/ast.rs` (new section after `Direction`, around line 372)
- Create: `src/mermaid/paint.rs`
- Modify: `src/mermaid/mod.rs:9-17` (add `pub mod paint;`)

**Interfaces:**
- Produces (in `mermaid::ast`):

```rust
pub struct PaintColor { pub rgb: Color, pub named: Option<usize> }
pub struct Paint { pub fill: Option<PaintColor>, pub stroke: Option<PaintColor>, pub heavy: bool, pub origin: Option<usize> }
```
- Produces (in `mermaid::paint`): `SLOT_COUNT: usize = 16`; `struct Props { fill: Option<(PaintColor, usize)>, stroke: Option<(PaintColor, usize)>, heavy: Option<bool> }`; `fn parse_props(text: &str, base: usize) -> Props`; `fn css_color(name: &str) -> Option<PaintColor>`; `fn hue_of(color: Color) -> Option<f32>`; `fn hue_color(paint: &Paint) -> Option<PaintColor>`; `fn merge<'a>(layers: impl IntoIterator<Item = &'a Props>) -> Option<Paint>`.

Two decisions the spec leaves open, fixed here: `PaintColor::named` records that a colour was written as a CSS name, because §4.4 gives such a unit the nominal angle of its name and the bare `Color` of §3.1 cannot tell `green` from `#008000`. `origin` is the byte offset of the property that supplied the hue-picking colour, not of its line: that is the line's order for colours on different lines, and it orders `fill` and `stroke` of one line deterministically.

- [ ] **Step 1: Add the AST types** to `src/mermaid/ast.rs` after `enum Direction` (they have no users yet, so nothing fails; the unit tests come next):

```rust
// ---------------------------------------------------------------------------
// Colour lines (colour spec, docs/superpowers/specs/2026-10-03-mermaid-colours-design.md)
// ---------------------------------------------------------------------------

/// A colour as written in a `classDef` or `style` line (colour spec §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaintColor {
    /// The RGB value. A CSS name has its CSS value, so `red` and `#ff0000` compare
    /// equal (colour spec ruling 20).
    pub rgb: crate::theme::Color,
    /// The slot a CSS colour name goes to by name (colour spec §4.3), as an index into
    /// the 16 slots. `None` for a hex value and for the neutral names.
    pub named: Option<usize>,
}

/// What the author wrote for one node, frame or composite state, after merging
/// (colour spec §3.1, §3.5).
///
/// The author's values, not theme colours: resolution to theme hues happens at draw
/// time, so the parsers stay theme-free (colour spec ruling 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Paint {
    /// `fill`.
    pub fill: Option<PaintColor>,
    /// `stroke`.
    pub stroke: Option<PaintColor>,
    /// `stroke-width` of 3px or more.
    pub heavy: bool,
    /// Byte offset of the property that supplied the colour picking the hue
    /// (colour spec §4.2), which orders the slot assignment of §4.4. `None` when no
    /// colour has a hue.
    pub origin: Option<usize>,
}
```

- [ ] **Step 2: Write the failing unit tests** at the bottom of the new `src/mermaid/paint.rs` (create the file with only the module doc and this test module first, so the run fails on missing items):

```rust
// SPDX-License-Identifier: MIT
//! Colour lines: property lists, CSS names, merging and slot resolution.
//!
//! Theme-free on purpose (colour spec ruling 8): this module turns what the author
//! wrote into [`Paint`] values and decides which of the theme's 16 slots each colour
//! takes. What a slot looks like is the theme's business (`theme::SlotInk`).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    fn hex(value: u32) -> PaintColor {
        PaintColor { rgb: Color::hex(value), named: None }
    }

    #[test]
    fn reads_fill_stroke_and_width() {
        let props = parse_props("fill:#e3f4fb,stroke:#2a8bb5,stroke-width:3px", 100);
        assert_eq!(props.fill, Some((hex(0xe3f4fb), 100)));
        assert_eq!(props.stroke, Some((hex(0x2a8bb5), 113)));
        assert_eq!(props.heavy, Some(true));
        assert_eq!(parse_props("stroke-width:2.5px", 0).heavy, Some(false));
        assert_eq!(parse_props("stroke-width: 4", 0).heavy, Some(true));
    }

    #[test]
    fn reads_short_long_and_alpha_hex_but_requires_the_hash() {
        assert_eq!(parse_props("fill:#f9f", 0).fill.map(|f| f.0), Some(hex(0xff99ff)));
        assert_eq!(parse_props("fill:#11223344", 0).fill.map(|f| f.0), Some(hex(0x112233)));
        assert_eq!(parse_props("fill:#1234", 0).fill, None, "#rgba is ignored");
        assert_eq!(parse_props("fill:e3f4fb", 0).fill, None, "# is required");
        assert_eq!(parse_props("fill:rgb(1,2,3)", 0).fill, None);
        assert_eq!(parse_props("fill:#zzz", 0).fill, None);
    }

    #[test]
    fn ignores_other_properties_and_stops_at_a_semicolon() {
        let props = parse_props("color:#000,font-weight:bold,stroke-dasharray:5 5", 0);
        assert_eq!(props, Props::default());
        // `#333;` is not split by the statement splitter, so the reader stops itself.
        let props = parse_props("fill:#f9f,stroke:#333;", 0);
        assert_eq!(props.stroke.map(|s| s.0), Some(hex(0x333333)));
        let props = parse_props("fill:#000;stroke:#123", 0);
        assert_eq!(props.fill.map(|f| f.0), Some(hex(0x000000)));
        assert_eq!(props.stroke, None);
    }

    #[test]
    fn css_names_carry_their_slot_and_value() {
        for (name, rgb, slot) in [
            ("red", 0xff0000, Some(0)),
            ("Maroon", 0x800000, Some(0)),
            ("orange", 0xffa500, Some(2)),
            ("yellow", 0xffff00, Some(4)),
            ("olive", 0x808000, Some(5)),
            ("green", 0x008000, Some(6)),
            ("lime", 0x00ff00, Some(6)),
            ("teal", 0x008080, Some(8)),
            ("cyan", 0x00ffff, Some(8)),
            ("aqua", 0x00ffff, Some(8)),
            ("blue", 0x0000ff, Some(10)),
            ("navy", 0x000080, Some(10)),
            ("purple", 0x800080, Some(12)),
            ("fuchsia", 0xff00ff, Some(14)),
            ("MAGENTA", 0xff00ff, Some(14)),
            ("black", 0x000000, None),
            ("white", 0xffffff, None),
            ("gray", 0x808080, None),
            ("grey", 0x808080, None),
            ("silver", 0xc0c0c0, None),
        ] {
            assert_eq!(
                css_color(name),
                Some(PaintColor { rgb: Color::hex(rgb), named: slot }),
                "{name}"
            );
        }
        assert_eq!(css_color("rebeccapurple"), None);
    }

    #[test]
    fn hue_detection_follows_saturation_and_lightness() {
        let hue = |v| hue_of(Color::hex(v));
        assert!((hue(0x2a8bb5).expect("hued") - 198.1).abs() < 0.2);
        assert!((hue(0xd4831f).expect("hued") - 33.2).abs() < 0.2);
        assert!((hue(0xb8650a).expect("hued") - 31.4).abs() < 0.2);
        assert!(hue(0xe3f4fb).is_some(), "a pastel keeps its hue (s = 0.75)");
        assert!(hue(0xfff7ee).is_some(), "l = 0.967 is still under 0.97");
        assert_eq!(hue(0xffffff), None);
        assert_eq!(hue(0x0d0000), None, "too dark");
        assert_eq!(hue(0x808080), None, "no saturation");
        assert_eq!(hue(0x8a7f7a), None, "saturation under 0.2");
    }

    #[test]
    fn merge_replaces_property_by_property_in_layer_order() {
        let default = parse_props("fill:#ffffff,stroke-width:3px", 0);
        let class = parse_props("stroke:#2a8bb5", 50);
        let style = parse_props("fill:#e3f4fb", 90);
        let paint = merge([&default, &class, &style]).expect("painted");
        assert_eq!(paint.fill, Some(hex(0xe3f4fb)));
        assert_eq!(paint.stroke, Some(hex(0x2a8bb5)));
        assert!(paint.heavy);
        assert_eq!(paint.origin, Some(50), "the stroke picks the hue");
        let thin = parse_props("stroke-width:1px", 99);
        assert!(!merge([&default, &thin]).expect("painted").heavy);
    }

    #[test]
    fn the_hue_comes_from_the_stroke_unless_it_has_none() {
        let paint = merge([&parse_props("fill:#e3f4fb,stroke:#000000", 7)]).expect("painted");
        assert_eq!(hue_color(&paint), Some(hex(0xe3f4fb)));
        assert_eq!(paint.origin, Some(7));
        let grey = merge([&parse_props("fill:#ffffff,stroke:#808080", 0)]).expect("painted");
        assert_eq!(hue_color(&grey), None);
        assert_eq!(grey.origin, None);
    }

    #[test]
    fn nothing_readable_is_no_paint() {
        assert_eq!(merge([&parse_props("color:#000", 0)]), None);
        assert_eq!(merge(std::iter::empty()), None);
        assert_eq!(merge([&parse_props("stroke-width:1px", 0)]), None);
    }
}
```

- [ ] **Step 3: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --lib mermaid::paint -- --test-threads=4`
Expected: FAIL to compile, `parse_props` etc. not found. (Add `pub mod paint;` to `src/mermaid/mod.rs` first so the module is compiled.)

- [ ] **Step 4: Implement** above the test module in `src/mermaid/paint.rs`:

```rust
use crate::mermaid::ast::{Paint, PaintColor};
use crate::theme::Color;

/// How many slots the theme offers: its 8 named hues and the 8 midpoints between
/// neighbours (colour spec §4.1). Index order is nominal angle order from red.
pub const SLOT_COUNT: usize = 16;

/// What one `classDef` or `style` line sets, property by property.
///
/// Each colour keeps the byte offset it was written at, so that a merge can tell which
/// line decided a node's hue (colour spec §4.4). `heavy` is `Some(false)` for a thin
/// `stroke-width`, which must replace a heavy one from an earlier layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Props {
    /// `fill`, and where its value was written.
    pub fill: Option<(PaintColor, usize)>,
    /// `stroke`, and where its value was written.
    pub stroke: Option<(PaintColor, usize)>,
    /// Whether `stroke-width` was 3 or more, when it was readable.
    pub heavy: Option<bool>,
}

/// Reads a property list such as `fill:#e3f4fb,stroke:#2a8bb5,stroke-width:3px`.
///
/// `base` is the byte offset of `text` in the Mermaid source. Properties are separated
/// by `,` only, and the list ends at the first `;`: the statement splitter keeps
/// `#333;` whole as a character reference, so it cannot be relied on to have cut there
/// (colour spec §3.4). Anything unreadable is dropped, never an error (§3.6).
pub fn parse_props(text: &str, base: usize) -> Props {
    let mut props = Props::default();
    let list = text.split(';').next().unwrap_or_default();
    let mut at = 0;
    for part in list.split(',') {
        let offset = base + at + (part.len() - part.trim_start().len());
        at += part.len() + 1;
        let Some((name, value)) = part.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "fill" => props.fill = color(value).map(|c| (c, offset)).or(props.fill),
            "stroke" => props.stroke = color(value).map(|c| (c, offset)).or(props.stroke),
            "stroke-width" => props.heavy = width(value).map(|w| w >= 3.0).or(props.heavy),
            _ => {}
        }
    }
    props
}

/// A `fill` or `stroke` value: `#rgb`, `#rrggbb`, `#rrggbbaa` or a CSS name.
fn color(value: &str) -> Option<PaintColor> {
    let Some(digits) = value.strip_prefix('#') else {
        return css_color(value);
    };
    if !digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    // Alpha is read and dropped: a terminal cell has no transparency.
    let rgb = match digits.len() {
        3 | 6 => Color::parse(digits).ok()?,
        8 => Color::parse(&digits[..6]).ok()?,
        _ => return None,
    };
    Some(PaintColor { rgb, named: None })
}

/// A `stroke-width` value: a number with optional `px`.
fn width(value: &str) -> Option<f32> {
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    number.parse::<f32>().ok().filter(|w| w.is_finite() && *w >= 0.0)
}

/// The CSS colour names the colour spec accepts (§3.4), with their slot (§4.3).
const CSS_NAMES: [(&str, u32, Option<usize>); 20] = [
    ("red", 0xff0000, Some(0)),
    ("maroon", 0x800000, Some(0)),
    ("orange", 0xffa500, Some(2)),
    ("yellow", 0xffff00, Some(4)),
    ("olive", 0x808000, Some(5)),
    ("green", 0x008000, Some(6)),
    ("lime", 0x00ff00, Some(6)),
    ("teal", 0x008080, Some(8)),
    ("cyan", 0x00ffff, Some(8)),
    ("aqua", 0x00ffff, Some(8)),
    ("blue", 0x0000ff, Some(10)),
    ("navy", 0x000080, Some(10)),
    ("purple", 0x800080, Some(12)),
    ("fuchsia", 0xff00ff, Some(14)),
    ("magenta", 0xff00ff, Some(14)),
    ("black", 0x000000, None),
    ("white", 0xffffff, None),
    ("gray", 0x808080, None),
    ("grey", 0x808080, None),
    ("silver", 0xc0c0c0, None),
];

/// A CSS colour name, case-insensitive, as its RGB value and its slot by name.
pub fn css_color(name: &str) -> Option<PaintColor> {
    CSS_NAMES
        .iter()
        .find(|(css, _, _)| css.eq_ignore_ascii_case(name))
        .map(|&(_, rgb, named)| PaintColor {
            rgb: Color::hex(rgb),
            named,
        })
}

/// The HSL hue of `color` in degrees, or `None` when it has none (colour spec §4.2).
///
/// A colour has no hue below saturation 0.2 or outside lightness 0.06..=0.97. Pastel
/// fills keep theirs: Mermaid stylesheets are written for a white page, and `#e3f4fb`
/// is how such a sheet says "blue".
pub fn hue_of(color: Color) -> Option<f32> {
    let [r, g, b] = [color.r, color.g, color.b].map(|v| f32::from(v) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    let saturation = if delta == 0.0 {
        0.0
    } else {
        delta / (1.0 - (2.0 * lightness - 1.0).abs())
    };
    if saturation < 0.2 || !(0.06..=0.97).contains(&lightness) {
        return None;
    }
    let sector = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    Some((60.0 * sector).rem_euclid(360.0))
}

/// The colour that picks a paint's hue: `stroke` if it has one, otherwise `fill`.
pub fn hue_color(paint: &Paint) -> Option<PaintColor> {
    let hued = |c: Option<PaintColor>| c.filter(|c| hue_of(c.rgb).is_some());
    hued(paint.stroke).or_else(|| hued(paint.fill))
}

/// Merges layers in order, a later value replacing an earlier one property by
/// property (colour spec §3.5). `None` when nothing readable was set.
pub fn merge<'a>(layers: impl IntoIterator<Item = &'a Props>) -> Option<Paint> {
    let mut merged = Props::default();
    for layer in layers {
        merged.fill = layer.fill.or(merged.fill);
        merged.stroke = layer.stroke.or(merged.stroke);
        merged.heavy = layer.heavy.or(merged.heavy);
    }
    let heavy = merged.heavy.unwrap_or(false);
    if merged.fill.is_none() && merged.stroke.is_none() && !heavy {
        return None;
    }
    let hued = |c: Option<(PaintColor, usize)>| c.filter(|(c, _)| hue_of(c.rgb).is_some());
    Some(Paint {
        fill: merged.fill.map(|(c, _)| c),
        stroke: merged.stroke.map(|(c, _)| c),
        heavy,
        origin: hued(merged.stroke).or(hued(merged.fill)).map(|(_, at)| at),
    })
}
```

Note: `Color::parse` already reads 3 and 6 digits without `#`. If clippy flags `f32 == 0.0` (`float_cmp` is pedantic, not default), leave it.

- [ ] **Step 5: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --lib mermaid::paint -- --test-threads=4`
Expected: PASS (8 tests). If `hue_detection...` misses by more than 0.2, print the value and compare with the python reference in the scratchpad (`r2lib.hsl`); do not loosen the bound without a reason.

- [ ] **Step 6: Gates and commit**

```bash
git add src/mermaid/ast.rs src/mermaid/paint.rs src/mermaid/mod.rs
git commit -m "feat: read Mermaid colour property lists into a Paint

Adds the AST type a node's colour lines merge into, and the theme-free
reader behind it: fill, stroke and stroke-width, hex and CSS names, the
hue test of colour spec 4.2 and the property-by-property merge of 3.5.
A CSS name remembers its slot, because 4.4 snaps it by name rather than
by angle. Nothing calls it yet.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Slot resolution

**Files:**
- Modify: `src/mermaid/paint.rs` (add after `merge`; tests into its `mod tests`)

**Interfaces:**
- Consumes: `hue_of`, `hue_color`, `SLOT_COUNT`, `Paint`, `PaintColor` (Task 2).
- Produces: `pub const NOMINAL: [f32; SLOT_COUNT]`; `pub struct Resolution`; `Resolution::of<'p>(paints: impl IntoIterator<Item = &'p Paint>) -> Resolution`; `Resolution::slot(&self, paint: &Paint) -> Option<usize>`. Private, tested directly: `fn nearest(angle: f32, allowed: impl Fn(usize) -> bool) -> Option<usize>`, `fn assign(angles: &[f32]) -> Vec<usize>`.

- [ ] **Step 1: Write the failing tests** (append to `mod tests` in `paint.rs`):

```rust
    /// A fully saturated colour at `degrees`, for building units at known angles.
    fn at(degrees: f32) -> Color {
        let h = degrees / 60.0;
        let x = 1.0 - (h.rem_euclid(2.0) - 1.0).abs();
        let (r, g, b) = match h as u32 {
            0 => (1.0, x, 0.0),
            1 => (x, 1.0, 0.0),
            2 => (0.0, 1.0, x),
            3 => (0.0, x, 1.0),
            4 => (x, 0.0, 1.0),
            _ => (1.0, 0.0, x),
        };
        let byte = |v: f32| (v * 255.0).round() as u8;
        Color::rgb(byte(r), byte(g), byte(b))
    }

    /// A paint whose stroke is `rgb`, written at byte `origin`.
    fn stroked(rgb: Color, origin: usize) -> Paint {
        merge([&Props {
            stroke: Some((PaintColor { rgb, named: None }, origin)),
            ..Props::default()
        }])
        .expect("painted")
    }

    #[test]
    fn nearest_breaks_ties_towards_the_larger_angle() {
        assert_eq!(nearest(7.5, |_| true), Some(1), "15 beats 0");
        assert_eq!(nearest(352.5, |_| true), Some(0), "0 counts as 360 against 337.5");
        assert_eq!(nearest(359.0, |_| true), Some(0), "the shorter way round");
        assert_eq!(nearest(198.0, |_| true), Some(9));
    }

    #[test]
    fn the_spec_example_resolves_as_its_table_says() {
        // classDef access/comm/part, then `style mbox` with part's stroke (colour spec §4.4).
        let access = stroked(Color::hex(0x2a8bb5), 10);
        let comm = stroked(Color::hex(0xd4831f), 20);
        let part = stroked(Color::hex(0xb8650a), 30);
        let mbox = stroked(Color::hex(0xb8650a), 40);
        let resolution = Resolution::of([&mbox, &part, &comm, &access]);
        assert_eq!(resolution.slot(&access), Some(9), "200 cyan-blue");
        assert_eq!(resolution.slot(&comm), Some(2), "30 orange");
        assert_eq!(resolution.slot(&part), Some(4), "45 yellow: 15, 30, 37.5 skipped");
        assert_eq!(resolution.slot(&mbox), Some(4), "same colour, same unit");
    }

    #[test]
    fn a_collision_skips_the_two_neighbours_then_falls_back() {
        // Rule 2: orange held, a second orange skips 15, 30 and 37.5.
        assert_eq!(assign(&[30.0, 31.0]), vec![2, 4]);
        // Rule 3: only the neighbours of 30 are free, so the nearest of them is taken.
        let mut angles: Vec<f32> = (0..SLOT_COUNT)
            .filter(|&slot| slot != 1 && slot != 3)
            .map(|slot| NOMINAL[slot])
            .collect();
        angles.push(31.0);
        assert_eq!(assign(&angles).last(), Some(&3), "37.5 is nearer than 15");
    }

    #[test]
    fn a_seventeenth_colour_shares_its_nearest_slot() {
        let mut angles: Vec<f32> = NOMINAL.to_vec();
        angles.push(2.0);
        let slots = assign(&angles);
        assert_eq!(&slots[..SLOT_COUNT], (0..SLOT_COUNT).collect::<Vec<_>>().as_slice());
        assert_eq!(slots[SLOT_COUNT], 0, "rule 4: share S");
    }

    #[test]
    fn units_go_in_source_order_not_in_argument_order() {
        let first = stroked(at(30.0), 5);
        let second = stroked(at(31.0), 9);
        let resolution = Resolution::of([&second, &first]);
        assert_eq!(resolution.slot(&first), Some(2));
        assert_eq!(resolution.slot(&second), Some(4));
    }

    #[test]
    fn a_css_name_snaps_by_name_and_an_equal_hex_joins_it() {
        let named = |name: &str, origin| {
            merge([&Props {
                stroke: Some((css_color(name).expect("css"), origin)),
                ..Props::default()
            }])
            .expect("painted")
        };
        let green = named("green", 1);
        let lime = named("lime", 2);
        let blue = named("blue", 3);
        let hex_blue = stroked(Color::hex(0x0000ff), 4);
        let resolution = Resolution::of([&green, &lime, &blue, &hex_blue]);
        assert_eq!(resolution.slot(&green), Some(6), "by name, though 120 is nearer 97.5");
        assert_eq!(resolution.slot(&lime), Some(8), "collides with green, skips 5-7");
        assert_eq!(resolution.slot(&blue), Some(10), "by name, though 240 is a slot");
        assert_eq!(resolution.slot(&hex_blue), Some(10), "same RGB, same unit");
    }

    #[test]
    fn a_colour_without_hue_takes_no_slot() {
        let grey = stroked(Color::hex(0x808080), 1);
        let red = stroked(Color::hex(0xff0000), 2);
        let resolution = Resolution::of([&grey, &red]);
        assert_eq!(resolution.slot(&grey), None);
        assert_eq!(resolution.slot(&red), Some(0));
    }

    #[test]
    fn all_sixteen_slots_can_be_held() {
        let paints: Vec<Paint> = NOMINAL
            .iter()
            .enumerate()
            .map(|(index, &angle)| stroked(at(angle), index))
            .collect();
        let resolution = Resolution::of(&paints);
        let slots: Vec<Option<usize>> = paints.iter().map(|p| resolution.slot(p)).collect();
        assert_eq!(slots, (0..SLOT_COUNT).map(Some).collect::<Vec<_>>());
    }
```

`at()` rounds to bytes, so a unit's real angle can be a degree off its nominal one; every nominal angle is at least 7.5 from its neighbours, so that is safe. If clippy objects to `h as u32` or `as u8` (`cast_*` lints are pedantic, not default), add `#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]` on `at` only if it actually warns.

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --lib mermaid::paint -- --test-threads=4`
Expected: FAIL to compile (`nearest`, `assign`, `Resolution`, `NOMINAL` missing).

- [ ] **Step 3: Implement** after `merge` in `paint.rs`:

```rust
/// The nominal angle of each slot (colour spec §4.1), in slot index order.
///
/// Fixed rather than measured from the active theme, so a diagram resolves to the same
/// slots in every theme (ruling 4). A midpoint slot draws as the blend of its
/// neighbours, whose real hue can be some way off this angle; the angle only picks.
pub const NOMINAL: [f32; SLOT_COUNT] = [
    0.0, 15.0, 30.0, 37.5, 45.0, 97.5, 150.0, 165.0, 180.0, 200.0, 220.0, 240.0, 260.0, 287.5,
    315.0, 337.5,
];

/// The shorter way round the circle between two angles.
fn distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// The nearest allowed slot to `angle`; a tie goes to the larger nominal angle, with 0
/// counted as 360 so that red wins a tie against magenta-red (colour spec §4.4).
fn nearest(angle: f32, allowed: impl Fn(usize) -> bool) -> Option<usize> {
    let tie = |slot: usize| if slot == 0 { 360.0 } else { NOMINAL[slot] };
    (0..SLOT_COUNT).filter(|&slot| allowed(slot)).min_by(|&a, &b| {
        distance(angle, NOMINAL[a])
            .total_cmp(&distance(angle, NOMINAL[b]))
            .then(tie(b).total_cmp(&tie(a)))
    })
}

/// Assigns a slot to each unit angle, in the order given (colour spec §4.4).
///
/// A colour that lost its slot skips that slot's two neighbours too: the neighbours
/// draw as blends with the held hue, and two classes one blend apart read as one
/// (ruling 11). Units are distinct colours, so rule 1's "held by the same colour"
/// never arises here; it is what deduplicating by RGB already did.
fn assign(angles: &[f32]) -> Vec<usize> {
    let mut held = [false; SLOT_COUNT];
    angles
        .iter()
        .map(|&angle| {
            let best = nearest(angle, |_| true).unwrap_or(0);
            let left = (best + SLOT_COUNT - 1) % SLOT_COUNT;
            let right = (best + 1) % SLOT_COUNT;
            let slot = if held[best] {
                nearest(angle, |s| !held[s] && s != best && s != left && s != right)
                    .or_else(|| nearest(angle, |s| !held[s]))
                    .unwrap_or(best)
            } else {
                best
            };
            held[slot] = true;
            slot
        })
        .collect()
}

/// Which slot each hue-picking colour of one diagram takes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolution {
    /// One entry per unit: its RGB value and its slot.
    units: Vec<(Color, usize)>,
}

impl Resolution {
    /// Resolves every paint of one diagram at once, nodes and frames together, so a
    /// colour gets the same slot wherever it is used.
    ///
    /// A unit is one RGB value. It is ordered by the first place its colour picked a
    /// hue, and takes the nominal angle of its name when that first place wrote a CSS
    /// name (§4.4). A colour that picks no paint's hue takes no slot (ruling 12).
    pub fn of<'p>(paints: impl IntoIterator<Item = &'p Paint>) -> Self {
        let mut first: Vec<(usize, PaintColor)> = Vec::new();
        for paint in paints {
            let (Some(color), Some(at)) = (hue_color(paint), paint.origin) else {
                continue;
            };
            match first.iter_mut().find(|(_, unit)| unit.rgb == color.rgb) {
                Some(unit) if at < unit.0 => *unit = (at, color),
                Some(_) => {}
                None => first.push((at, color)),
            }
        }
        first.sort_by_key(|&(at, color)| (at, color.rgb.r, color.rgb.g, color.rgb.b));
        let angles: Vec<f32> = first
            .iter()
            .map(|(_, color)| match color.named {
                Some(slot) => NOMINAL[slot],
                None => hue_of(color.rgb).unwrap_or_default(),
            })
            .collect();
        Self {
            units: first
                .iter()
                .zip(assign(&angles))
                .map(|(&(_, color), slot)| (color.rgb, slot))
                .collect(),
        }
    }

    /// The slot `paint` draws in, or `None` when it has no hued colour.
    pub fn slot(&self, paint: &Paint) -> Option<usize> {
        let color = hue_color(paint)?;
        self.units
            .iter()
            .find(|(rgb, _)| *rgb == color.rgb)
            .map(|&(_, slot)| slot)
    }
}
```

Note: a CSS name of slot S (for example `named: Some(0)`) whose unit sits at `NOMINAL[0] = 0.0` ties nothing; `nearest(0.0)` returns 0.

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --lib mermaid::paint -- --test-threads=4`
Expected: PASS (16 tests).

- [ ] **Step 5: Gates and commit**

```bash
git add src/mermaid/paint.rs
git commit -m "feat: resolve Mermaid colours to sixteen theme slots

Colour spec 4: each distinct hue-picking colour takes the slot nearest
its angle, in source order; a colour whose slot is taken skips the two
neighbouring slots, since those draw as blends of the held hue. CSS
names snap by name and an equal hex joins them. Nothing calls it yet.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Theme slot inks with contrast repair

**Files:**
- Create: `src/theme/slots.rs`
- Modify: `src/theme/mod.rs` (`mod slots; pub use slots::SlotInk;` near line 16; field on `Theme` after `diagram`, line 457)
- Modify: `src/theme/builtin.rs:126-307` (bind `DiagramStyles` to a `let`, derive slots, add the field)
- Modify: `tests/theme_contrast.rs` (two new tests at the end)

**Interfaces:**
- Consumes: `Color::contrast` (Task 1).
- Produces: `pub struct SlotInk { pub ink: Option<Color>, pub full_tint: Color, pub half_tint: Color }` (`Debug, Clone, Copy, PartialEq, Eq, Default`); `Theme::diagram_slots: [SlotInk; 16]`, index order of `paint::NOMINAL`. `ink: None` is the §5.3 step 2 fallback: draw with the theme defaults, and both tints are `bg`. (The spec says "three colours"; an `Option` is how the fallback of step 2 is represented without a fourth field.)

- [ ] **Step 1: Write the failing tests.** Unit tests at the bottom of the new `src/theme/slots.rs`, which at first holds only the module doc and this module:

```rust
#[cfg(test)]
mod tests {
    use crate::theme::{Color, Theme};

    #[test]
    fn the_dark_theme_needs_no_repair() {
        let theme = Theme::default_dark();
        let p = &theme.palette;
        let named = [p.red, p.orange, p.yellow, p.green, p.cyan, p.blue, p.purple, p.magenta];
        for (slot, ink) in theme.diagram_slots.iter().enumerate() {
            let hue = if slot % 2 == 0 {
                named[slot / 2]
            } else {
                named[slot / 2].blend(named[(slot / 2 + 1) % 8], 0.5)
            };
            assert_eq!(ink.ink, Some(hue), "slot {slot}");
            assert_eq!(ink.full_tint, p.bg.blend(hue, 0.15), "slot {slot}");
            assert_eq!(ink.half_tint, p.bg.blend(hue, 0.075), "slot {slot}");
        }
        assert_eq!(theme.diagram_slots[9].full_tint, Color::hex(0x1f2d3a));
        assert_eq!(theme.diagram_slots[9].half_tint, Color::hex(0x18212a));
    }

    /// Measured with the f32 blend of `Color::blend` (scratchpad `plan_repair.py`):
    /// six light inks move 0.05 or 0.10 towards the text, hue shifts under 2 degrees.
    #[test]
    fn the_light_theme_repairs_six_inks_and_keeps_its_tints() {
        let theme = Theme::default_light();
        let p = &theme.palette;
        for (slot, hex) in [
            (1, 0xb34a18),
            (2, 0xa55806),
            (3, 0x936006),
            (4, 0x816706),
            (7, 0x187762),
            (8, 0x107778),
        ] {
            assert_eq!(theme.diagram_slots[slot].ink, Some(Color::hex(hex)), "slot {slot}");
        }
        assert_eq!(theme.diagram_slots[0].ink, Some(p.red), "red needs no step");
        // Tints come from the palette ink, not the repaired one.
        assert_eq!(theme.diagram_slots[2].full_tint, Color::hex(0xf2e4d4));
        assert_eq!(theme.diagram_slots[2].half_tint, Color::hex(0xf7f0e6));
    }
}
```

In `tests/theme_contrast.rs` append:

```rust
use mdmost::theme::{Palette, SlotInk};

/// Colour spec §7: every slot ink and tint against every ground it meets.
#[test]
fn diagram_slots_clear_their_floors() {
    for theme in themes() {
        let name = &theme.name;
        let d = theme.diagram;
        let slots = theme.diagram_slots;
        for (x, slot) in slots.iter().enumerate() {
            let ink = slot.ink.unwrap_or_else(|| panic!("{name}: slot {x} fell back"));
            at_least(name, &format!("slot {x} ink on the page"), ink, theme.palette.bg, TEXT_FLOOR);
            at_least(name, &format!("slot {x} ink on its full tint"), ink, slot.full_tint, GRAPHIC_FLOOR);
            for (what, style) in [("text", d.node_text), ("stereotype", d.stereotype), ("edge label", d.edge_label)] {
                at_least(name, &format!("{what} on slot {x} full tint"), fg(what, style), slot.full_tint, TEXT_FLOOR);
            }
            for (what, style, floor) in [
                ("text", d.node_text, TEXT_FLOOR),
                ("edge label", d.edge_label, TEXT_FLOOR),
                ("group title", d.group_title, TEXT_FLOOR),
                ("line", d.line, GRAPHIC_FLOOR),
                ("arrow", d.arrow, GRAPHIC_FLOOR),
                ("node border", d.node_border, GRAPHIC_FLOOR),
                ("group border", d.group_border, GRAPHIC_FLOOR),
            ] {
                at_least(name, &format!("{what} on slot {x} half tint"), fg(what, style), slot.half_tint, floor);
            }
            for (y, other) in slots.iter().enumerate() {
                let other = other.ink.unwrap_or_else(|| panic!("{name}: slot {y} fell back"));
                at_least(name, &format!("slot {y} ink on slot {x} half tint"), other, slot.half_tint, TEXT_FLOOR);
            }
        }
    }
}

/// Palettes the repair has to survive (colour spec §8). Kept out of `themes()`, which
/// every other test here loops over and which such a palette would fail.
fn repair_palettes() -> Vec<(&'static str, Palette)> {
    let dark = Theme::default_dark().palette;
    let grey = Color::hex(0x777777);
    let murky = |hex| Color::hex(hex);
    vec![
        (
            "text misses the page",
            Palette { bg: grey, surface: grey, overlay: grey, fg: Color::hex(0x8a8a8a), ..dark.clone() },
        ),
        (
            "hues sit on the page",
            Palette {
                red: murky(0x2a1a1e),
                orange: murky(0x2a2018),
                yellow: murky(0x28261a),
                green: murky(0x1a2820),
                cyan: murky(0x1a2628),
                blue: murky(0x1a1e2c),
                purple: murky(0x221c2c),
                magenta: murky(0x2a1a26),
                ..dark
            },
        ),
    ]
}

/// The repair terminates and leaves every slot in one of the states §5.3 allows: an ink
/// that clears its own floors, or the step 2 fallback; a tint on which the fixed
/// diagram inks clear theirs, or a tint that step 3 lowered all the way to the page.
#[test]
fn the_slot_repair_terminates_on_hostile_palettes() {
    for (what, palette) in repair_palettes() {
        let theme = Theme::from_palette(what, true, palette);
        let bg = theme.palette.bg;
        let d = theme.diagram;
        let halves: Vec<Color> = theme.diagram_slots.iter().map(|s| s.half_tint).collect();
        for (x, slot) in theme.diagram_slots.iter().enumerate() {
            let SlotInk { ink, full_tint, half_tint } = *slot;
            match ink {
                None => assert_eq!((full_tint, half_tint), (bg, bg), "{what}: slot {x}"),
                Some(ink) => {
                    assert!(ink.contrast(bg) >= TEXT_FLOOR, "{what}: slot {x} on bg");
                    assert!(halves.iter().all(|h| ink.contrast(*h) >= TEXT_FLOOR), "{what}: slot {x} on halves");
                    assert!(ink.contrast(full_tint) >= GRAPHIC_FLOOR, "{what}: slot {x} on full");
                }
            }
            let text = fg("text", d.node_text);
            assert!(full_tint == bg || text.contrast(full_tint) >= TEXT_FLOOR, "{what}: slot {x} full");
            assert!(half_tint == bg || text.contrast(half_tint) >= TEXT_FLOOR, "{what}: slot {x} half");
        }
    }
    let text_misses = Theme::from_palette("t", true, repair_palettes()[0].1.clone());
    assert!(text_misses.diagram_slots.iter().all(|s| s.ink.is_none()), "every slot falls back");
    let murky = Theme::from_palette("m", true, repair_palettes()[1].1.clone());
    assert!(murky.diagram_slots.iter().all(|s| s.ink.is_some()), "inks move towards the text");
}
```

Run `cargo fmt` after pasting; the long lines above are written compact for the plan.

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test theme_contrast -- --test-threads=4`
Expected: FAIL to compile, `no field diagram_slots`, `SlotInk` not found.

- [ ] **Step 3: Implement `src/theme/slots.rs`** above the tests:

```rust
// SPDX-License-Identifier: MIT
//! The 16 hues a Mermaid diagram's colour classes are drawn in (colour spec §5).
//!
//! An author's stylesheet is made for a white page, so its colours are not drawn as
//! written: each class snaps to one of these slots (`mermaid::paint`), and the slot
//! decides the ink. The inks are repaired here, once per theme, so every theme a
//! config file defines inherits the contrast floors `tests/theme_contrast.rs` pins.

use super::{Color, DiagramStyles, Palette, Style};

/// How far a slot's full tint, the background of a filled node, leans to its ink.
const FULL_TINT: f32 = 0.15;
/// The half tint a filled subgraph or composite state washes its area with.
const HALF_TINT: f32 = 0.075;
/// One ink repair step towards the text colour, and how many are allowed.
const INK_STEP: f32 = 0.05;
const INK_STEPS: u8 = 20;
/// One tint repair step towards the page, and how many are allowed.
const TINT_STEP: f32 = 0.015;
const TINT_STEPS: u8 = 10;
/// WCAG floors: text and frame titles, and borders and rules.
const TEXT: f32 = 4.5;
const GRAPHIC: f32 = 3.0;

/// The colours one slot draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SlotInk {
    /// Borders, frame titles and inner rules of a node or frame in this slot. `None`
    /// when no repair step could make it readable (colour spec §5.3 step 2): such a
    /// slot draws in the theme's own diagram colours.
    pub ink: Option<Color>,
    /// The interior of a filled node.
    pub full_tint: Color,
    /// The area of a filled subgraph or composite state.
    pub half_tint: Color,
}

/// The palette hue of each slot: the 8 named hues and the 50/50 blend of each pair of
/// neighbours, in nominal angle order from red.
fn palette_inks(p: &Palette) -> [Color; 16] {
    let named = [p.red, p.orange, p.yellow, p.green, p.cyan, p.blue, p.purple, p.magenta];
    std::array::from_fn(|slot| {
        let hue = named[slot / 2];
        if slot % 2 == 0 {
            hue
        } else {
            hue.blend(named[(slot / 2 + 1) % named.len()], 0.5)
        }
    })
}

/// Derives the 16 slots for a theme (colour spec §5.2, §5.3).
///
/// The ink is repaired, not the tint: in the light theme orange ink measures 4.60:1 on
/// the page, so any tint under it fell short, and lowering the tints until it passed
/// left the light half tints invisible. Moving six light inks 0.05 to 0.10 towards the
/// text keeps every tint at full strength and shifts no hue by 2 degrees (measured
/// 2026-10-04: red-orange, green-cyan and cyan by 0.05, orange, orange-yellow and
/// yellow by 0.10; the dark theme moves none). Each loop is bounded, so a palette whose
/// own text misses the page ends in the fallback instead of looping.
pub(super) fn derive(p: &Palette, d: &DiagramStyles) -> [SlotInk; 16] {
    let hues = palette_inks(p);
    let full: [Color; 16] = std::array::from_fn(|slot| p.bg.blend(hues[slot], FULL_TINT));
    let half: [Color; 16] = std::array::from_fn(|slot| p.bg.blend(hues[slot], HALF_TINT));
    // Step 1, over all slots: the ink against the page, every half tint (it may title a
    // frame nested in any other) and its own full tint (inner rules).
    let inks: [Option<Color>; 16] = std::array::from_fn(|slot| {
        (0..=INK_STEPS)
            .map(|step| hues[slot].blend(p.fg, INK_STEP * f32::from(step)))
            .find(|&ink| {
                ink.contrast(p.bg) >= TEXT
                    && half.iter().all(|&ground| ink.contrast(ground) >= TEXT)
                    && ink.contrast(full[slot]) >= GRAPHIC
            })
    });
    let ink = |style: Style| style.fg.unwrap_or(p.fg);
    let on_full = [(ink(d.node_text), TEXT), (ink(d.stereotype), TEXT), (ink(d.edge_label), TEXT)];
    let on_half = [
        (ink(d.node_text), TEXT),
        (ink(d.edge_label), TEXT),
        (ink(d.group_title), TEXT),
        (ink(d.line), GRAPHIC),
        (ink(d.arrow), GRAPHIC),
        (ink(d.node_border), GRAPHIC),
        (ink(d.group_border), GRAPHIC),
    ];
    std::array::from_fn(|slot| match inks[slot] {
        // Step 2: the theme's own colours, and no tint at all.
        None => SlotInk { ink: None, full_tint: p.bg, half_tint: p.bg },
        // Step 3: the fixed theme inks on the tints.
        Some(repaired) => SlotInk {
            ink: Some(repaired),
            full_tint: tint(p.bg, hues[slot], FULL_TINT, &on_full),
            half_tint: tint(p.bg, hues[slot], HALF_TINT, &on_half),
        },
    })
}

/// The strongest tint of `hue` at or under `start` on which every `(ink, floor)` pair
/// clears its floor, lowered `TINT_STEP` at a time; the page itself when none does.
///
/// Lowering a tint towards the page keeps the ink checks of step 1 valid, because each
/// ink already clears both the page and the stronger tint.
fn tint(bg: Color, hue: Color, start: f32, pairs: &[(Color, f32)]) -> Color {
    (0..=TINT_STEPS)
        .map(|step| bg.blend(hue, (start - TINT_STEP * f32::from(step)).max(0.0)))
        .find(|&ground| pairs.iter().all(|&(ink, floor)| ink.contrast(ground) >= floor))
        .unwrap_or(bg)
}
```

`mod.rs`: `mod slots;` and `pub use slots::SlotInk;` beside `mod builtin;`/`pub use style::…`; field on `Theme`:

```rust
    /// The 16 hues Mermaid colour classes draw in, indexed in nominal angle order from
    /// red (colour spec §4.1, §5). Derived from the palette and the diagram styles by
    /// [`Theme::from_palette`], contrast-repaired; never configured on its own.
    pub diagram_slots: [SlotInk; 16],
```

`builtin.rs`: before `Theme {` add `let diagram = DiagramStyles { … };` (move the literal from its field verbatim) and `let diagram_slots = super::slots::derive(&p, &diagram);`; in the struct write `diagram,` and `diagram_slots,`. `palette: p` stays last, so the borrow ends before the move.

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --lib theme -- --test-threads=4` then `... cargo test -j4 --test theme_contrast -- --test-threads=4`
Expected: PASS. If a pinned light ink differs by one in a channel, check that the step factor is `INK_STEP * f32::from(step)` (not compounded blends); the expected values came from `plan_repair.py` in the scratchpad, which mirrors `Color::blend` in f32. If `the_slot_repair_terminates_on_hostile_palettes` fails on the "inks move towards the text" assertion, print the murky slots: a slot that fell back there means the derivation, not the test, needs looking at. Report rather than change a pinned value.

- [ ] **Step 5: Gates and commit**

```bash
git add src/theme/slots.rs src/theme/mod.rs src/theme/builtin.rs tests/theme_contrast.rs
git commit -m "feat: give every theme sixteen contrast-repaired diagram slots

Colour spec 5: each slot is a named palette hue or the blend of two
neighbours, with a full tint for filled nodes and a half tint for filled
frames. A weak ink moves towards the text colour until it reads on the
page and on every half tint; in the light theme that moves six inks by
0.05 to 0.10 and leaves the tints alone. Section 7's floors are pinned
for every theme, and the repair is tested on palettes that cannot pass.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The shared sheet, and flowchart paints

**Files:**
- Create: `src/mermaid/parse/sheet.rs`
- Modify: `src/mermaid/parse.rs:21-29` (`mod sheet;`), and its module doc lines 9-17 (styling lines are now read)
- Modify: `src/mermaid/ast.rs:399-406` (`FlowNode.paint`), `:474-486` (`Group.paint`)
- Modify: `src/mermaid/parse/flowchart.rs` (doc lines 9-10; `Builder` fields; `statement` line 89; `open_subgraph` lines 109-148; `node` lines 223-276; `finish` lines 279-307)
- Modify (add `paint: None`): `tests/mermaid_layout_monotone.rs:27`, `tests/mermaid_layout_flowchart.rs:17`, `:494`
- Test: `tests/mermaid_parse_families.rs` (module `flowcharts`), `tests/mermaid_parse_robustness.rs`

**Interfaces:**
- Consumes: `paint::parse_props`, `paint::merge`, `paint::Props` (Task 2).
- Produces: `pub(super) struct Sheet` with
  - `fn define(&mut self, rest: &str, src: &str)` for `classDef a,b props`
  - `fn assign_list(&mut self, rest: &str, key: impl Fn(&str) -> String)` for `class A,B c1,c2` and `cssClass "A,B" c`
  - `fn style(&mut self, rest: &str, src: &str, key: impl Fn(&str) -> String)` for `style A props`
  - `fn assign(&mut self, key: &str, class: &str)` for a `:::` suffix
  - `fn paint(&self, key: Option<&str>, own: &[String], node: bool) -> Option<Paint>`: layers `classDef default` (only when `node`), then `own` classes, then classes assigned to `key` in order, then `style` lines for `key` in order.
  - `fn plain_key(text: &str) -> String`: the key function for flowchart and state (unquote, trim).
- Produces: `FlowNode::paint: Option<Paint>`, `Group::paint: Option<Paint>`.

`key` closures normalise a written target to the key the family interns under, so `style "A"` and `cssClass "Square~T~"` find their node. A key nobody declares is simply never asked for (ruling 15); a class nobody defines adds no layer (§3.6). `own` exists only for anonymous subgraphs (`subgraph "Two words":::c`), which have no key to look up. A class defined twice is applied as all its definitions in source order.

- [ ] **Step 1: Write the failing tests** in `tests/mermaid_parse_families.rs`. Add near the top (after `node`):

```rust
/// A paint's colours and weight, without the source offsets that order slots.
fn colours(paint: Option<Paint>) -> Option<(Option<u32>, Option<u32>, bool)> {
    let rgb = |c: Option<PaintColor>| c.map(|c| u32::from_be_bytes([0, c.rgb.r, c.rgb.g, c.rgb.b]));
    paint.map(|p| (rgb(p.fill), rgb(p.stroke), p.heavy))
}
```

In `mod flowcharts`:

```rust
    /// The colour spec's driving example (§1).
    #[test]
    fn reads_classdef_class_and_style() {
        let chart = flowchart(
            "flowchart TD\n  airlock --> zimbra --> zmcfgapi --> mbox\n\
             classDef access fill:#e3f4fb,stroke:#2a8bb5,color:#000\n\
             classDef comm fill:#fdf0e1,stroke:#d4831f,color:#000\n\
             class airlock access\n  class zimbra comm\n\
             style mbox fill:#fff7ee,stroke:#b8650a,stroke-width:3px,color:#000\n",
        );
        assert_eq!(colours(node(&chart, "airlock").paint), Some((Some(0xe3f4fb), Some(0x2a8bb5), false)));
        assert_eq!(colours(node(&chart, "zimbra").paint), Some((Some(0xfdf0e1), Some(0xd4831f), false)));
        assert_eq!(node(&chart, "zmcfgapi").paint, None);
        assert_eq!(colours(node(&chart, "mbox").paint), Some((Some(0xfff7ee), Some(0xb8650a), true)));
        assert_eq!(chart.nodes.len(), 4, "colour lines create no node");
    }

    #[test]
    fn merges_default_then_classes_then_styles() {
        let chart = flowchart(
            "flowchart LR\n  A:::one --> B\n  class A two\n  style A stroke:#00ff00\n\
             classDef default fill:#111111,stroke-width:3px\n\
             classDef one fill:#ff0000,stroke:#ff0000\n  classDef two stroke:#0000ff\n",
        );
        assert_eq!(colours(node(&chart, "A").paint), Some((Some(0xff0000), Some(0x00ff00), true)));
        assert_eq!(colours(node(&chart, "B").paint), Some((Some(0x111111), None, true)), "default");
    }

    #[test]
    fn assigns_several_classes_and_several_nodes_at_once() {
        let chart = flowchart(
            "flowchart LR\n  A --> B --> C\n  class A,B one,two\n\
             classDef one fill:#ff0000\n  classDef two stroke:#0000ff\n",
        );
        for key in ["A", "B"] {
            assert_eq!(colours(node(&chart, key).paint), Some((Some(0xff0000), Some(0x0000ff), false)), "{key}");
        }
        assert_eq!(node(&chart, "C").paint, None);
    }

    #[test]
    fn paints_subgraphs_by_key_and_by_suffix_but_not_by_default() {
        let chart = flowchart(
            "flowchart TB\n  subgraph one\n    a\n  end\n  subgraph two:::warm [Two]\n    b\n  end\n\
             subgraph \"Three words\":::warm\n    c\n  end\n\
             style one fill:#e3f4fb\n  classDef warm stroke:#d4831f\n  classDef default stroke:#ff0000\n",
        );
        let groups = &chart.root.children;
        assert_eq!(colours(groups[0].paint), Some((Some(0xe3f4fb), None, false)));
        assert_eq!(colours(groups[1].paint), Some((None, Some(0xd4831f), false)));
        assert_eq!(colours(groups[2].paint), Some((None, Some(0xd4831f), false)), "anonymous");
        assert_eq!(colours(node(&chart, "a").paint), Some((None, Some(0xff0000), false)));
    }

    #[test]
    fn unknown_classes_undeclared_nodes_and_bad_values_are_dropped() {
        let chart = flowchart(
            "flowchart LR\n  A --> B\n  class A nosuch\n  style Z fill:#ff0000\n  class Y one\n\
             classDef one fill:#ff0000\n  style B fill:notacolour,stroke:#12\n",
        );
        assert_eq!(chart.nodes.len(), 2);
        assert_eq!(node(&chart, "A").paint, None);
        assert_eq!(node(&chart, "B").paint, None);
    }

    /// `;` ends a statement, but the splitter keeps `#333;` whole (colour spec §3.4).
    #[test]
    fn a_semicolon_ends_the_property_list() {
        let chart = flowchart(
            "flowchart LR\n  A --> B --> C\n  classDef c fill:#ff99ff,stroke:#333;\n  class A c\n\
             style B fill:#000;stroke:#123\n  style C fill:#e3f4fb;stroke:#2a8bb5\n",
        );
        assert_eq!(colours(node(&chart, "A").paint), Some((Some(0xff99ff), Some(0x333333), false)));
        assert_eq!(colours(node(&chart, "B").paint), Some((Some(0x000000), None, false)));
        assert_eq!(colours(node(&chart, "C").paint), Some((Some(0xe3f4fb), None, false)));
    }

    /// Colour spec Review Focus 2: no colour line can fail a diagram.
    #[test]
    fn colour_lines_with_garbage_never_fail_the_diagram() {
        for line in [
            "classDef", "classDef c", "classDef ,, fill:#f00", "class", "class A", "class  c",
            "style", "style A", "style A fill:", "style A stroke-width:1e999px",
            "style A fill:#ffffffffff", "style A fill:#ﬀ0000", "cssClass \"A\" c",
            "classDef é fill:#f00", "style A :", "classDef c fill:#f00,,,stroke:",
        ] {
            let chart = flowchart(&format!("flowchart LR\n  A --> B\n  {line}\n"));
            assert_eq!(chart.nodes.len(), 2, "{line}");
        }
    }
```

In `tests/mermaid_parse_robustness.rs`, extend `SAMPLES` (change its length to 9) with:

```rust
    "flowchart TD\n  classDef default fill:#fff\n  classDef a,b fill:#e3f4fb,stroke:#2a8bb5,stroke-width:3px\n  A:::a --> B\n  class A,B a,b\n  style B fill:#f00;stroke:#0f0\n  subgraph s:::a\n    C\n  end\n  style s stroke:red\n",
    "stateDiagram-v2\n  classDef hot fill:#f00\n  [*] --> A:::hot\n  class A hot\n  state B:::hot {\n    C\n  }\n  style B stroke:blue,stroke-width:4px\n",
```

The state, class and ER families get their own samples in Tasks 6 to 8; the state sample above is harmless now because state still skips the lines.

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_parse_families -- --test-threads=4`
Expected: FAIL to compile (`no field paint`).

- [ ] **Step 3: Implement.** AST fields, each with a doc comment:

```rust
    /// The merged colour lines aimed at this node (colour spec §3), or `None` to draw
    /// in the theme's own diagram colours.
    pub paint: Option<Paint>,
```

on `FlowNode` (after `shape`) and on `Group` (after `children`; `Group` derives `Default`, so its literals using `..Group::default()` need nothing). Add `paint: None` to the three test literals listed above and to the `FlowNode` literal in `flowchart.rs:253`.

`src/mermaid/parse/sheet.rs`:

```rust
// SPDX-License-Identifier: MIT
//! `classDef`, `class`, `cssClass`, `style` and `:::`, shared by the four families that
//! read them (colour spec §3.2).
//!
//! Everything is recorded while the diagram is read and merged only when it is done,
//! because a class may be defined after its use (§3.5). Nothing here fails: an unknown
//! class, an undeclared node or an unreadable value drops that statement or property
//! (§3.6), and a key no node has is simply never asked for, so `style X` cannot create
//! a node (ruling 15).

use crate::mermaid::ast::Paint;
use crate::mermaid::paint::{self, Props};

use super::lex;

/// The colour statements of one diagram, in source order.
#[derive(Debug, Default)]
pub(super) struct Sheet {
    /// `classDef` definitions: class name and properties.
    defs: Vec<(String, Props)>,
    /// Class assignments from `class`, `cssClass` and `:::`: node key and class name.
    assigned: Vec<(String, String)>,
    /// `style` lines: node key and properties.
    styles: Vec<(String, Props)>,
}

impl Sheet {
    /// `classDef a,b fill:#…`: defines every listed class.
    pub(super) fn define(&mut self, rest: &str, src: &str) {
        let (names, list) = lex::split_word(rest);
        let props = paint::parse_props(list, lex::offset_of(src, list).unwrap_or_default());
        for name in names.split(',').map(str::trim).filter(|name| is_class_name(name)) {
            self.defs.push((name.to_string(), props));
        }
    }

    /// `class A,B c1,c2` or `cssClass "A,B" c`: the last word names the classes, and
    /// everything before it the nodes, quoted or not.
    pub(super) fn assign_list(&mut self, rest: &str, key: impl Fn(&str) -> String) {
        let rest = rest.trim();
        let Some(at) = rest.rfind(char::is_whitespace) else {
            return;
        };
        let (list, classes) = (lex::unquote(&rest[..at]), rest[at..].trim());
        for target in list.split(',').map(|target| key(target.trim())) {
            for class in classes.split(',').map(str::trim) {
                self.assign(&target, class);
            }
        }
    }

    /// `style A fill:#…`.
    pub(super) fn style(&mut self, rest: &str, src: &str, key: impl Fn(&str) -> String) {
        let (target, list) = lex::split_word(rest);
        if target.is_empty() {
            return;
        }
        let props = paint::parse_props(list, lex::offset_of(src, list).unwrap_or_default());
        self.styles.push((key(target), props));
    }

    /// One class assigned to one node, as `:::name` writes it.
    pub(super) fn assign(&mut self, key: &str, class: &str) {
        if !key.is_empty() && is_class_name(class) {
            self.assigned.push((key.to_string(), class.to_string()));
        }
    }

    /// The merged paint of `key`: `classDef default` when `node` (ruling 14: never for
    /// a subgraph or composite state), then `own` classes, then the classes assigned
    /// to `key`, then its `style` lines.
    pub(super) fn paint(&self, key: Option<&str>, own: &[String], node: bool) -> Option<Paint> {
        let class = |name: &str| self.defs.iter().filter(move |(def, _)| def == name).map(|(_, props)| props);
        let assigned = self
            .assigned
            .iter()
            .filter(|(target, _)| Some(target.as_str()) == key)
            .map(|(_, name)| name.as_str());
        let styles = self
            .styles
            .iter()
            .filter(|(target, _)| Some(target.as_str()) == key)
            .map(|(_, props)| props);
        let default = node.then(|| class("default")).into_iter().flatten();
        let classes = own.iter().map(String::as_str).chain(assigned).flat_map(class);
        paint::merge(default.chain(classes).chain(styles))
    }
}

/// The key a flowchart or state diagram files a `style` or `class` target under.
pub(super) fn plain_key(text: &str) -> String {
    lex::unquote(text).to_string()
}

/// A class name is `[A-Za-z0-9_-]+` (colour spec §3.2).
fn is_class_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(lex::is_class_char)
}
```

(If the borrow checker rejects the `class` closure being used inside `flat_map` and `then`, make it a private method `fn class_layers<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'s Props> + 's`.)

`parse.rs`: `mod sheet;` and change the module doc bullet to say `classDef`, `class`, `cssClass`, `style` and `:::` are read by flowchart, state, class and ER diagrams (colour spec), and `click`, `linkStyle` and the rest still skipped.

`flowchart.rs`:
- Module doc: "Read for colour: `classDef`, `class`, `style` and the `:::name` suffix on a node or a subgraph (colour spec). Skipped silently: `click`, `cssClass`, `linkStyle`."
- `Builder`: add `sheet: super::sheet::Sheet,` and `/// Classes written as `:::` on each subgraph without a key, in the order subgraphs open, which is the order `finish` walks the tree in. subgraph_classes: Vec<Vec<String>>,`.
- `statement`: replace the cosmetic arm with

```rust
            "click" | "cssclass" | "linkstyle" => return Ok(()),
            "classdef" => {
                self.sheet.define(rest, self.src);
                return Ok(());
            }
            "class" => {
                self.sheet.assign_list(rest, sheet::plain_key);
                return Ok(());
            }
            "style" => {
                self.sheet.style(rest, self.src, sheet::plain_key);
                return Ok(());
            }
```
- `open_subgraph`: keep both `split_class_suffix` results: `let (rest, outer) = …;` and inside the shape arm `let (key, inner) = …;`; compute `let class = outer.or(inner)` (bind `inner` as `None` in the other arm). After the key check: if `key` is `Some`, `if let Some(class) = class { self.sheet.assign(&key, class) }` and push `Vec::new()` to `subgraph_classes`; else push `class.map(str::to_string).into_iter().collect()`. Replace the comment "which is not drawn yet" with why the suffix is split off.
- `node`: `let (text, class) = lex::split_class_suffix(text);` and after `key` is known and non-empty: `if let Some(class) = class { self.sheet.assign(key, class); }`. Update its comment likewise.
- `finish`, before building the result:

```rust
        for node in &mut self.nodes {
            node.paint = self.sheet.paint(Some(&node.key), &[], true);
        }
        let mut root = self.stack.pop().unwrap_or_default();
        let mut next = 0;
        paint_groups(&mut root, &self.sheet, &self.subgraph_classes, &mut next);
```
and use `root` in the returned `Flowchart`. Add the free function:

```rust
/// Paints every subgraph below `group`, in the order their `subgraph` lines opened.
///
/// That order is a pre-order walk of the tree, so the anonymous subgraphs' `:::`
/// classes line up by position without needing a key.
fn paint_groups(group: &mut Group, sheet: &Sheet, classes: &[Vec<String>], next: &mut usize) {
    for child in &mut group.children {
        let own = classes.get(*next).map_or(&[][..], Vec::as_slice);
        *next += 1;
        child.paint = sheet.paint(child.key.as_deref(), own, false);
        paint_groups(child, sheet, classes, next);
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_parse_families --test mermaid_parse_robustness -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Gates and commit** (full test suite: no snapshot may move)

```bash
git add src/mermaid/parse/sheet.rs src/mermaid/parse.rs src/mermaid/ast.rs src/mermaid/parse/flowchart.rs tests/mermaid_parse_families.rs tests/mermaid_parse_robustness.rs tests/mermaid_layout_monotone.rs tests/mermaid_layout_flowchart.rs
git commit -m "feat: attach Mermaid colour lines to flowchart nodes and subgraphs

A shared sheet records classDef, class, style and the ::: suffix while a
diagram is read and merges them per node once it is done: classDef
default, then the node's classes in order, then its style lines. A
subgraph is painted by key or by its own suffix but never by default.
Unknown classes, undeclared keys and unreadable values are dropped.
The paint is not drawn yet.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: State diagram paints

**Files:**
- Modify: `src/mermaid/ast.rs:1088-1096` (`StateNode.paint`)
- Modify: `src/mermaid/parse/state.rs` (doc lines 9-10; `Builder`; `statement` line 115; the `:::` sites at lines 151, 158, 189-190, 251-252, 308-309; `intern_state` literal line 353; `finish` lines 365-385)
- Modify (add `paint: None`): `src/mermaid/layout/state.rs:376`, `tests/mermaid_layout_families.rs:167`, `:209`, `tests/mermaid_layout_state.rs:16`, `:25`
- Test: `tests/mermaid_parse_families.rs` (module `states`), `tests/mermaid_parse_robustness.rs`

**Interfaces:**
- Consumes: `Sheet`, `sheet::plain_key` (Task 5).
- Produces: `StateNode::paint: Option<Paint>`: `Simple` states with `classDef default`; `Composite` without it (ruling 14); `Choice`, `Fork`, `Join` always `None` (§3.1).

- [ ] **Step 1: Write the failing tests** in `mod states` (the `colours` helper is at the top of the file):

```rust
    fn state_named<'a>(diagram: &'a StateDiagram, key: &str) -> &'a StateNode {
        diagram.states.iter().find(|s| s.key == key).unwrap_or_else(|| panic!("no state {key}"))
    }

    #[test]
    fn reads_colour_lines_on_states_and_composites() {
        let diagram = state(
            "stateDiagram-v2\n  [*] --> A:::hot\n  A --> B : go\n  B:::cold : waiting\n\
             state C:::hot {\n    D\n  }\n  state E <<choice>>\n  note left of F:::cold : n\n\
             class E hot\n  class B hot\n  style C stroke-width:3px\n\
             classDef hot fill:#ff0000\n  classDef cold stroke:#0000ff\n  classDef default stroke:#00ff00\n",
        );
        assert_eq!(colours(state_named(&diagram, "A").paint), Some((Some(0xff0000), Some(0x00ff00), false)));
        assert_eq!(colours(state_named(&diagram, "B").paint), Some((Some(0xff0000), Some(0x0000ff), false)));
        assert_eq!(colours(state_named(&diagram, "C").paint), Some((Some(0xff0000), None, true)), "no default");
        assert_eq!(colours(state_named(&diagram, "D").paint), Some((None, Some(0x00ff00), false)));
        assert_eq!(state_named(&diagram, "E").paint, None, "a choice takes no paint");
        assert_eq!(colours(state_named(&diagram, "F").paint), Some((None, Some(0x0000ff), false)), "note target");
    }

    #[test]
    fn style_on_an_undeclared_state_creates_none() {
        let diagram = state("stateDiagram-v2\n  A --> B\n  style Z fill:#ff0000\n  class Y c\n");
        assert_eq!(diagram.states.len(), 2);
    }

    #[test]
    fn a_declared_alias_takes_its_class() {
        let diagram = state("stateDiagram-v2\n  state \"Long name\" as C:::c\n  classDef c fill:#ff0000\n");
        assert_eq!(colours(state_named(&diagram, "C").paint), Some((Some(0xff0000), None, false)));
    }
```

Add to `SAMPLES` in `tests/mermaid_parse_robustness.rs` nothing new (Task 5 added the state sample); confirm it still runs.

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_parse_families -- --test-threads=4 states`
Expected: FAIL to compile (`no field paint`).

- [ ] **Step 3: Implement.** `StateNode.paint` with the doc comment of Task 5 plus "Only plain and composite states take one; markers, choice, fork, join and notes never do (colour spec §3.1)." Literals get `paint: None`.

`state.rs`:
- Doc: "Read for colour: `classDef`, `class`, `style` and the `:::name` suffix on a state, a `state` declaration and a note's target (colour spec). Skipped silently: `click`."
- `Builder`: `sheet: super::sheet::Sheet,`.
- `statement`: replace `"classdef" | "class" | "style" | "click" => return Ok(()),` with `"click" => return Ok(()),` plus arms calling `self.sheet.define(rest, self.src)`, `self.sheet.assign_list(rest, sheet::plain_key)`, `self.sheet.style(rest, self.src, sheet::plain_key)`, each returning `Ok(())`.
- Every `let (key, _class) = lex::split_class_suffix(…)` becomes `let (key, class) = …` and, once the state's key is known, `if let Some(class) = class { self.sheet.assign(<key>, class); }`: in the description form (key from `split_label_colon`), the state-alone form, `declare` (use the unquoted key), `note` (the target's key, after `unquote`), and `endpoint` (only after the `[*]` check, using the unquoted key). Rewrite each "not drawn yet" comment to say what the suffix is.
- `finish`, before building:

```rust
        for state in &mut self.states {
            state.paint = match state.kind {
                StateKind::Simple => self.sheet.paint(Some(&state.key), &[], true),
                // A composite is a frame: no `classDef default` (ruling 14).
                StateKind::Composite(_) => self.sheet.paint(Some(&state.key), &[], false),
                _ => None,
            };
        }
```

(`self.note` is checked before this; keep its early return first.)

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_parse_families --test mermaid_parse_robustness --test mermaid_layout_state --test mermaid_layout_families -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

```bash
git add src/mermaid/ast.rs src/mermaid/parse/state.rs src/mermaid/layout/state.rs tests/mermaid_parse_families.rs tests/mermaid_layout_families.rs tests/mermaid_layout_state.rs
git commit -m "feat: attach Mermaid colour lines to states and composite states

State diagrams read classDef, class, style and ::: through the shared
sheet. A plain state takes classDef default, a composite state does not,
and start and end markers, choice, fork and join take no paint. A class
on a note's target paints the target. The paint is not drawn yet.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Class diagram paints

**Files:**
- Modify: `src/mermaid/ast.rs:677-690` (`Class.paint`)
- Modify: `src/mermaid/parse/class.rs` (doc lines 9-10; `Builder`; `parse` lines 46-54; `statement` line 107; `intern_class` lines 230-264)
- Modify (add `paint: None`): `src/mermaid/layout/class.rs:196`, `tests/mermaid_layout_families.rs:57`, `tests/mermaid_layout_class.rs:16`
- Test: `tests/mermaid_parse_families.rs` (module `classes`), `tests/mermaid_parse_robustness.rs`

**Interfaces:**
- Consumes: `Sheet` (Task 5).
- Produces: `Class::paint: Option<Paint>`; private `Builder::class_key(&self, text: &str) -> String` (the key `intern_class` files a class under: unquoted, generic split off, first line of its label), used for `style`/`cssClass` targets.

In a class diagram `class` declares a class; classes are assigned with `cssClass` and `:::` (§3.2).

- [ ] **Step 1: Write the failing tests** in `mod classes`:

```rust
    fn class_named<'a>(diagram: &'a ClassDiagram, name: &str) -> &'a Class {
        diagram.classes.iter().find(|c| c.name.lines[0] == name).unwrap_or_else(|| panic!("no class {name}"))
    }

    #[test]
    fn reads_colour_lines_on_classes() {
        let diagram = class_diagram(
            "classDiagram\n  class Animal:::warm\n  Animal <|-- Dog:::cool\n  Square~Shape~ <|-- Cat\n\
             cssClass \"Cat, Square\" cool\n  style Animal stroke-width:3px\n\
             classDef warm fill:#d4831f\n  classDef cool stroke:#2a8bb5\n",
        );
        assert_eq!(colours(class_named(&diagram, "Animal").paint), Some((Some(0xd4831f), None, true)));
        assert_eq!(colours(class_named(&diagram, "Dog").paint), Some((None, Some(0x2a8bb5), false)));
        assert_eq!(colours(class_named(&diagram, "Cat").paint), Some((None, Some(0x2a8bb5), false)));
        assert_eq!(colours(class_named(&diagram, "Square").paint), Some((None, Some(0x2a8bb5), false)));
        assert_eq!(diagram.classes.len(), 4, "no class named after a colour line");
    }

    #[test]
    fn a_class_line_in_a_class_diagram_still_declares() {
        let diagram = class_diagram("classDiagram\n  class Bird\n  classDef default fill:#ff0000\n");
        assert_eq!(colours(class_named(&diagram, "Bird").paint), Some((Some(0xff0000), None, false)));
    }
```

Extend `SAMPLES` (length 10) with `"classDiagram\n  classDef k fill:#f00,stroke-width:3px\n  A:::k <|-- B\n  cssClass \"A,B\" k\n  style A stroke:#00f\n  class C:::k {\n    +int x\n  }\n"`.

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_parse_families -- --test-threads=4 classes`
Expected: FAIL to compile.

- [ ] **Step 3: Implement.** `Class.paint` field (same doc), literals `paint: None`.

`class.rs`:
- Doc: "Read for colour: `classDef`, `cssClass`, `style` and the `:::name` suffix after a class name (colour spec). Skipped silently: `click`, `callback`, `link`."
- `Builder`: `sheet: super::sheet::Sheet,`.
- `statement`: `"click" | "callback" | "link" => return Ok(()),` plus
  `"classdef" => { self.sheet.define(rest, self.src); return Ok(()); }`,
  `"cssclass" => { let key = |t: &str| self.class_key(t); … }`: because `key` borrows `self` while `self.sheet` is borrowed mutably, compute targets first: write `assign_list` and `style` so they accept a key function by value, and here call them on a `std::mem::take(&mut self.sheet)`, then put it back:

```rust
            "cssclass" | "style" => {
                let mut sheet = std::mem::take(&mut self.sheet);
                if word.eq_ignore_ascii_case("style") {
                    sheet.style(rest, self.src, |text| self.class_key(text));
                } else {
                    sheet.assign_list(rest, |text| self.class_key(text));
                }
                self.sheet = sheet;
                return Ok(());
            }
```
- Split `intern_class` into `class_key` and the interning:

```rust
    /// The key a class is filed under: its name without quotes or generic, as the first
    /// line of its label, so `Square~Shape~` and `"Square"` are the same class.
    fn class_key(&self, text: &str) -> String {
        let (name, _) = Self::split_generic(lex::unquote(lex::split_class_suffix(text).0));
        lex::label_at(self.src, name).lines.first().cloned().unwrap_or_default()
    }
```
  with `fn split_generic(text: &str) -> (&str, Option<String>)` holding the existing `split_once('~')` logic; `intern_class` uses both and, after interning, `if let Some(class) = class { self.sheet.assign(&key, class); }` where `class` comes from its own `split_class_suffix` call.
- `declare` strips the suffix itself before it calls `intern_class` (`let (name, _class) = lex::split_class_suffix(name);`, line 155), so `class Animal:::warm` would lose its class there: keep it (`let (name, class) = …`) and, after `let id = self.intern_class(name);`, assign it under `self.class_key(name)`.
- `parse`, after the loop and the open-block check:

```rust
    for class in &mut builder.classes {
        let key = class.name.lines.first().cloned().unwrap_or_default();
        class.paint = builder.sheet.paint(Some(&key), &[], true);
    }
```
  (Bind `let mut builder` already exists; take `classes` out after this loop.)

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_parse_families --test mermaid_parse_robustness --test mermaid_layout_class --test mermaid_layout_families -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

```bash
git add src/mermaid/ast.rs src/mermaid/parse/class.rs src/mermaid/layout/class.rs tests/mermaid_parse_families.rs tests/mermaid_parse_robustness.rs tests/mermaid_layout_families.rs tests/mermaid_layout_class.rs
git commit -m "feat: attach Mermaid colour lines to class diagram classes

Class diagrams read classDef, cssClass, style and ::: through the
shared sheet; class still declares. A style or cssClass target is
filed under the same key as the class it names, so quotes and a
generic parameter do not hide it. The paint is not drawn yet.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: ER diagram paints

**Files:**
- Modify: `src/mermaid/ast.rs:899-906` (`Entity.paint`)
- Modify: `src/mermaid/parse/er.rs` (doc lines 9-10; `Builder`; `parse` lines 43-48; `line` lines 66-72; `entity_ref` lines 155-166; `intern_entity` literal line 184)
- Modify (add `paint: None`): `src/mermaid/layout/er.rs:171`, `tests/mermaid_layout_er.rs:16`, `tests/mermaid_layout_families.rs:108`
- Test: `tests/mermaid_parse_families.rs` (module `entities`), `tests/mermaid_parse_robustness.rs`

**Interfaces:**
- Consumes: `Sheet` (Task 5).
- Produces: `Entity::paint: Option<Paint>`; private `Builder::entity_key(&self, text: &str) -> String` (unquoted name, alias and class suffix split off, first label line).

ER does not split lines at `;`, so `style A fill:#fff;stroke:#000` reaches the sheet whole and the property reader stops at the `;` (§3.4).

- [ ] **Step 1: Write the failing tests** in `mod entities`:

```rust
    fn entity_named<'a>(diagram: &'a ErDiagram, name: &str) -> &'a Entity {
        diagram.entities.iter().find(|e| e.name.lines[0] == name).unwrap_or_else(|| panic!("no entity {name}"))
    }

    #[test]
    fn reads_colour_lines_on_entities() {
        let diagram = er(
            "erDiagram\n  CUSTOMER:::warm ||--o{ ORDER : places\n  ORDER ||--|{ LINE:::cool : has\n\
             p[Person]:::cool\n  class ORDER warm\n  style LINE fill:#000000;stroke:#ff0000\n\
             classDef warm fill:#d4831f\n  classDef cool stroke:#2a8bb5\n",
        );
        assert_eq!(colours(entity_named(&diagram, "CUSTOMER").paint), Some((Some(0xd4831f), None, false)));
        assert_eq!(colours(entity_named(&diagram, "ORDER").paint), Some((Some(0xd4831f), None, false)));
        assert_eq!(colours(entity_named(&diagram, "LINE").paint), Some((Some(0x000000), Some(0x2a8bb5), false)));
        assert_eq!(colours(entity_named(&diagram, "p").paint), Some((None, Some(0x2a8bb5), false)));
        assert_eq!(diagram.entities.len(), 4);
    }
```

Extend `SAMPLES` (length 11) with `"erDiagram\n  classDef k fill:#f00\n  A:::k ||--o{ B : has\n  class A,B k\n  style B fill:#0f0;stroke:#00f\n  B:::k {\n    string n\n  }\n"`.

- [ ] **Step 2: Run to see it fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_parse_families -- --test-threads=4 entities`
Expected: FAIL to compile.

- [ ] **Step 3: Implement.** `Entity.paint` field, literals `paint: None`.

`er.rs`:
- Doc: "Read for colour: `classDef`, `class`, `style` and the `:::name` suffix on an entity (colour spec). Skipped silently: `click`, `direction`."
- `Builder`: `sheet: super::sheet::Sheet,`.
- `line`: replace the skip with

```rust
        let (word, rest) = lex::split_word(text);
        match word.to_ascii_lowercase().as_str() {
            "click" | "direction" => return Ok(()),
            "classdef" => {
                self.sheet.define(rest, self.src);
                return Ok(());
            }
            "class" | "style" => {
                let mut sheet = std::mem::take(&mut self.sheet);
                if word.eq_ignore_ascii_case("style") {
                    sheet.style(rest, self.src, |name| self.entity_key(name));
                } else {
                    sheet.assign_list(rest, |name| self.entity_key(name));
                }
                self.sheet = sheet;
                return Ok(());
            }
            _ => {}
        }
```
- `entity_key`: `let (text, _) = lex::split_class_suffix(text); let (name, _) = split_alias(text); lex::label_at(self.src, lex::unquote(name)).lines.first().cloned().unwrap_or_default()`.
- `entity_ref`: keep the suffix (`let (text, class) = …`), and after interning `if let Some(class) = class { let key = self.entity_key(text); self.sheet.assign(&key, class); }`.
- `parse`: before building, `for entity in &mut builder.entities { let key = entity.name.lines.first().cloned().unwrap_or_default(); entity.paint = builder.sheet.paint(Some(&key), &[], true); }`.

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_parse_families --test mermaid_parse_robustness --test mermaid_layout_er --test mermaid_layout_families -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Gates and commit**

```bash
git add src/mermaid/ast.rs src/mermaid/parse/er.rs src/mermaid/layout/er.rs tests/mermaid_parse_families.rs tests/mermaid_parse_robustness.rs tests/mermaid_layout_er.rs tests/mermaid_layout_families.rs
git commit -m "feat: attach Mermaid colour lines to ER entities

ER diagrams read classDef, class, style and ::: through the shared
sheet. A target written with an alias or a class suffix is filed under
the entity's own name. The paint is not drawn yet.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Heavy border sets and the glyph tables that read them

**Files:**
- Modify: `src/canvas/border.rs` (two consts after `DASHED`, line 121; `ALL` lines 123-135; tests lines 210-271)
- Modify: `src/mermaid/layout/graph/glyph.rs:255-262` (`stroke_of`)
- Modify: `src/mermaid/layout/graph.rs:309-326` (`ruled_offsets`)
- Test: `src/canvas/border.rs`, `src/mermaid/layout/graph/glyph.rs`, `src/mermaid/layout/graph/ink.rs`, `src/mermaid/layout/graph/tests.rs`

**Interfaces:**
- Produces: `BorderSet::ROUNDED_HEAVY` (`━ ┃`, arcs `╭╮╰╯`, tees `┯ ┷ ┠ ┨`, cross `┼`) and `BorderSet::DASHED_HEAVY` (`╍ ╏`, otherwise `ROUNDED_HEAVY`); `BorderSet::ALL: [Self; 7]`.

The tees of `ROUNDED_HEAVY` are the joins a light inner rule makes with a heavy border (`┠─┨` divider, `┯` subroutine bar, §6.2), which is the only use a heavy box has for them.

- [ ] **Step 1: Write the failing tests.** In `border.rs` tests:

```rust
    #[test]
    fn the_heavy_sets_keep_light_arcs() {
        let set = BorderSet::ROUNDED_HEAVY;
        assert_eq!((set.horizontal, set.vertical, set.top_left, set.bottom_right), ('━', '┃', '╭', '╯'));
        assert_eq!((set.tee_right, set.tee_left, set.tee_down, set.tee_up), ('┠', '┨', '┯', '┷'));
        let dashed = BorderSet::DASHED_HEAVY;
        assert_eq!((dashed.horizontal, dashed.vertical, dashed.top_left), ('╍', '╏', '╭'));
        assert!(BorderSet::ALL.contains(&set) && BorderSet::ALL.contains(&dashed));
        assert_eq!(BorderSet::rule_glyph('╍'), Some((dashed, None)));
        assert_eq!(BorderSet::rule_glyph('┠'), Some((set, Some(Rule::Middle))));
    }
```

and change `every_border_glyph_is_single_width` to loop `for set in BorderSet::ALL`.

In `glyph.rs` tests:

```rust
    #[test]
    fn a_heavy_dashed_frame_is_thick() {
        assert_eq!(stroke_of('╍'), Stroke::Thick);
        assert_eq!(stroke_of('╏'), Stroke::Thick);
        assert_eq!(heavy_of('╍'), Mask::LEFT | Mask::RIGHT);
    }
```

In `ink.rs` tests:

```rust
    #[test]
    fn a_thin_edge_crossing_a_heavy_frame_keeps_both_weights() {
        let theme = Theme::default_dark();
        let mut canvas = Canvas::new(3, 3, theme.base());
        canvas.write_str(1, 0, "╍╍╍", theme.base());
        let mut ink = Ink::new(3, 3);
        ink.run(0, 1, Dir::Down, 2, Stroke::Solid);
        ink.apply(&mut canvas, theme.base(), theme.base());
        assert_eq!(canvas.row_text(1), "╍┿╍");
        let mut canvas = Canvas::new(3, 3, theme.base());
        canvas.vline(0, 1, 3, "╏", theme.base());
        let mut ink = Ink::new(3, 3);
        ink.run(1, 0, Dir::Right, 2, Stroke::Solid);
        ink.apply(&mut canvas, theme.base(), theme.base());
        assert_eq!(canvas.row_text(1), "─╂─");
    }

    #[test]
    fn a_thin_edge_on_a_heavy_round_box_meets_it_mixed() {
        let theme = Theme::default_dark();
        let mut canvas = Canvas::new(3, 2, theme.base());
        canvas.write_str(0, 0, "╰━╯", theme.base());
        let mut ink = Ink::new(2, 3);
        ink.run(1, 1, Dir::Up, 1, Stroke::Solid);
        ink.apply(&mut canvas, theme.base(), theme.base());
        assert_eq!(canvas.row_text(0), "╰┯╯");
        let mut canvas = Canvas::new(2, 3, theme.base());
        canvas.vline(0, 0, 3, "┃", theme.base());
        let mut ink = Ink::new(3, 2);
        ink.run(1, 1, Dir::Left, 1, Stroke::Solid);
        ink.apply(&mut canvas, theme.base(), theme.base());
        assert_eq!(canvas.row_text(1), "┠─");
    }
```

In `graph/tests.rs` (Review Focus 3):

```rust
#[test]
fn ports_keep_off_heavy_rules() {
    let theme = Theme::default_dark();
    let mut canvas = Canvas::new(4, 0, theme.base());
    for row in ["┏┯┯┓", "┃  ┃", "┠──┨", "┗┷┷┛"] {
        canvas.push_text(row, Align::Left, theme.diagram.node_border);
    }
    assert_eq!(super::ruled_offsets(&canvas, false), vec![false, false, true, false]);
    assert_eq!(super::ruled_offsets(&canvas, true), vec![false, true, true, false]);
}
```

(Use the imports `graph/tests.rs` already has; add `use crate::text::Align;` if missing.)

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --lib -- --test-threads=4 canvas::border mermaid::layout::graph`
Expected: FAIL (`ROUNDED_HEAVY` missing).

- [ ] **Step 3: Implement.**

```rust
    /// Heavy lines with the light arcs of [`ROUNDED`](Self::ROUNDED): a painted
    /// Mermaid node with `stroke-width` 3px or more (colour spec §6.2). Box Drawing has
    /// no heavy arcs, so the corners stay light; the tees are where a light inner rule
    /// meets the heavy border.
    pub const ROUNDED_HEAVY: Self = Self {
        horizontal: '━',
        vertical: '┃',
        top_left: '╭',
        top_right: '╮',
        bottom_left: '╰',
        bottom_right: '╯',
        tee_down: '┯',
        tee_up: '┷',
        tee_right: '┠',
        tee_left: '┨',
        cross: '┼',
    };

    /// The heavy form of [`DASHED`](Self::DASHED): a painted Mermaid frame, which stays
    /// dashed and keeps its light arcs (colour spec rulings 9 and 18).
    pub const DASHED_HEAVY: Self = Self {
        horizontal: '╍',
        vertical: '╏',
        ..Self::ROUNDED_HEAVY
    };
```

`ALL` becomes `[Self; 7]` with `Self::ROUNDED_HEAVY, Self::DASHED_HEAVY` appended (after `DASHED`, so `━` still traces to `HEAVY` and the shared arcs to `ROUNDED`); extend its doc's order note with that sentence.

`stroke_of`: add `'╍' | '╏'` to the Thick arm, with a comment that a heavy frame is dashed only in look; a line crossing it meets a heavy arm.

`ruled_offsets`: extend the `matches!` with `| "┠" | "┨" | "┯" | "┷"`, and the doc comment: a heavy box draws its rules with these tees (colour spec §6.2).

- [ ] **Step 4: Run the tests**

Run: same as Step 2. Expected: PASS.

- [ ] **Step 5: Gates and commit**

```bash
git add src/canvas/border.rs src/mermaid/layout/graph/glyph.rs src/mermaid/layout/graph.rs src/mermaid/layout/graph/ink.rs src/mermaid/layout/graph/tests.rs
git commit -m "feat: add heavy border sets with light arcs for painted Mermaid boxes

A node or frame with stroke-width 3px or more will draw heavy lines,
but Box Drawing has no heavy arcs, so its round corners stay light.
The heavy dashed frame counts as thick where an edge crosses it, so the
crossing keeps both weights, and the router keeps edges off the tees a
heavy box draws for its inner rules, as it does for light ones. No
diagram draws these yet.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Flowchart nodes drawn in their resolved style

**Files:**
- Create: `src/mermaid/layout/painted.rs`
- Modify: `src/mermaid/layout.rs` (`mod painted;`)
- Modify: `src/mermaid/layout/flowchart.rs` (`draw_with` lines 39-47, `Art` lines 49-67)
- Modify: `src/mermaid/layout/flowchart/shape.rs` (all drawing functions; test at the end)
- Create: `tests/mermaid_colours.rs`

**Interfaces:**
- Consumes: `paint::Resolution`, `paint::hue_of` (Tasks 2, 3), `Theme::diagram_slots` (Task 4), `BorderSet::ROUNDED_HEAVY`/`HEAVY` (Task 9), `FlowNode::paint`, `Group::paint` (Task 5).
- Produces:

```rust
pub(crate) struct NodeStyle { pub border: Style, pub rule: Style, pub fill: Option<Color>, pub heavy: bool }
impl NodeStyle {
    pub(crate) fn plain(theme: &Theme) -> Self;
    pub(crate) fn fill_inside(&self, canvas: &mut Canvas, wall: usize);
}
pub(crate) struct Painter<'t> { .. }
impl<'t> Painter<'t> {
    pub(crate) fn new<'p>(paints: impl IntoIterator<Item = &'p Paint>, theme: &'t Theme) -> Self;
    pub(crate) fn node(&self, paint: Option<&Paint>) -> NodeStyle;
}
```
- `shape::draw(label, shape, budget, theme, style: &NodeStyle) -> Canvas`.

`rule` is the inner-rule ink: the border colour for a painted node (§6.2), `diagram.compartment` for an unpainted one, which is what record dividers draw in today. `fill` is the slot's full tint when the paint has a hued `fill` and a slot whose tint is not the page.

- [ ] **Step 1: Write the failing render tests** in the new `tests/mermaid_colours.rs`:

```rust
// SPDX-License-Identifier: MIT
//! Mermaid colour classes as drawn: border inks, interior tints, heavy outlines and
//! frame washes (colour spec §6). Uncoloured output is pinned by the snapshot corpus.

use mdmost::canvas::Canvas;
use mdmost::mermaid::render_mermaid;
use mdmost::theme::{Color, Theme};

fn draw(src: &str, theme: &Theme) -> Canvas {
    render_mermaid(src, 120, theme).expect("diagram draws")
}

/// The row and column of the first cell of `needle`.
#[track_caller]
fn locate(canvas: &Canvas, needle: &str) -> (usize, usize) {
    for row in 0..canvas.height() {
        let text = canvas.row_text(row);
        if let Some(at) = text.find(needle) {
            return (row, text[..at].chars().count());
        }
    }
    panic!("`{needle}` not drawn:\n{}", canvas.plain_text());
}

fn glyph(canvas: &Canvas, row: usize, col: usize) -> String {
    canvas.row(row).expect("row")[col].text().to_string()
}

fn fg(canvas: &Canvas, row: usize, col: usize) -> Option<Color> {
    canvas.row(row).expect("row")[col].style().fg
}

fn bg(canvas: &Canvas, row: usize, col: usize) -> Option<Color> {
    canvas.row(row).expect("row")[col].style().bg
}

/// The top-left corner of the box whose label starts at `(row, col)`.
///
/// The corner rather than the side: an edge may enter a box on the label's row and turn
/// that side cell into a junction drawn in the line ink.
#[track_caller]
fn corner(canvas: &Canvas, row: usize, col: usize) -> (usize, usize) {
    for top in (0..row).rev() {
        if let Some(c) = (0..col).rev().find(|&c| matches!(glyph(canvas, top, c).as_str(), "┌" | "┏" | "╭")) {
            return (top, c);
        }
    }
    panic!("no corner above {row},{col}:\n{}", canvas.plain_text());
}

const EXAMPLE: &str = "flowchart LR\n  Airlock --> Zimbra --> Cfgapi --> Mbox\n\
    classDef access fill:#e3f4fb,stroke:#2a8bb5,color:#000\n\
    classDef comm fill:#fdf0e1,stroke:#d4831f,color:#000\n\
    classDef part fill:#fbd9a8,stroke:#b8650a,color:#000\n\
    class Airlock access\n  class Zimbra comm\n  class Cfgapi part\n\
    style Mbox fill:#fff7ee,stroke:#b8650a,stroke-width:3px,color:#000\n";

#[test]
fn a_painted_node_draws_its_slot_ink_and_full_tint() {
    let theme = Theme::default_dark();
    let canvas = draw(EXAMPLE, &theme);
    let page = Some(theme.palette.bg);
    for (label, slot) in [("Airlock", 9), ("Zimbra", 2), ("Cfgapi", 4), ("Mbox", 4)] {
        let (row, col) = locate(&canvas, label);
        let (top, left) = corner(&canvas, row, col);
        let ink = theme.diagram_slots[slot];
        assert_eq!(fg(&canvas, top, left), ink.ink, "{label} border");
        assert_eq!(bg(&canvas, top, left), page, "{label} border keeps the page");
        assert_eq!(bg(&canvas, row, col), Some(ink.full_tint), "{label} text");
        assert_eq!(bg(&canvas, row, col - 1), Some(ink.full_tint), "{label} padding");
        assert_eq!(fg(&canvas, row, col), theme.diagram.node_text.fg, "{label} text ink");
    }
    let (row, col) = locate(&canvas, "Mbox");
    let (top, left) = corner(&canvas, row, col);
    assert_eq!(glyph(&canvas, top, left), "┏", "stroke-width 3px");
    assert_eq!(glyph(&canvas, top, left + 1), "━");
}

/// Review Focus 5.
#[test]
fn the_light_theme_draws_repaired_inks() {
    let theme = Theme::default_light();
    let canvas = draw(EXAMPLE, &theme);
    let (row, col) = locate(&canvas, "Zimbra");
    let (top, left) = corner(&canvas, row, col);
    assert_eq!(fg(&canvas, top, left), Some(Color::hex(0xa55806)));
}

#[test]
fn every_shape_has_its_heavy_form() {
    let theme = Theme::default_dark();
    let src = "flowchart TB\n  R[rect] --- O(round) --- S([stad]) --- C((circ))\n\
        H{rhomb} --- U[[sub]] --- Y[(cyl)]\n  classDef k stroke:#ff0000,stroke-width:3px\n\
        class R,O,S,C,H,U,Y k\n";
    let text = draw(src, &theme).plain_text();
    // Geometry: a subroutine row is `┃ │ sub │ ┃` under `┏━┯━━━━━┯━┓`; a circle row is
    // `((  circ  ))`.
    for piece in ["┏━", "╭━", "( stad", "((", "))", "╱━", "┃ rhomb", "┏━┯", "┃ │ sub", "┠─", "╰━"] {
        assert!(text.contains(piece), "missing `{piece}` in\n{text}");
    }
}

/// Spec §8: a light edge on a heavy border draws a mixed junction.
#[test]
fn a_light_edge_meets_a_heavy_border_mixed() {
    let theme = Theme::default_dark();
    // `---`, not `-->`: an arrowhead sits on the border cell above it and leaves `━`.
    let src = "flowchart TB\n  A --- B\n  style A stroke:#ff0000,stroke-width:3px\n  style B stroke:#ff0000,stroke-width:3px\n";
    let text = draw(src, &theme).plain_text();
    assert!(text.contains('┯') && text.contains('┷'), "{text}");
}

/// Review Focus 1: paint changes inks and weights, never where anything is.
#[test]
fn paint_never_changes_layout_or_spans() {
    let theme = Theme::default_dark();
    let plain = "flowchart LR\n  Airlock --> Zimbra --> Cfgapi --> Mbox\n";
    let light = |text: String| {
        text.chars()
            .map(|ch| match ch {
                '━' => '─',
                '┃' => '│',
                '┏' => '┌',
                '┓' => '┐',
                '┗' => '└',
                '┛' => '┘',
                '┯' => '┬',
                '┷' => '┴',
                '┠' => '├',
                '┨' => '┤',
                other => other,
            })
            .collect::<String>()
    };
    for width in [60u16, 80, 120, 200] {
        let (Ok(a), Ok(b)) = (render_mermaid(plain, width, &theme), render_mermaid(EXAMPLE, width, &theme)) else {
            continue;
        };
        assert_eq!(light(a.plain_text()), light(b.plain_text()), "width {width}");
        assert_eq!(a.spans(), b.spans(), "width {width}");
    }
}

/// Lines that reach no node draw nothing different (rulings 12 and 15).
#[test]
fn colour_lines_that_reach_no_node_change_nothing() {
    let theme = Theme::default_dark();
    let plain = draw("flowchart LR\n  A --> B\n", &theme);
    let unreached = draw(
        "flowchart LR\n  A --> B\n  classDef unused fill:#ff0000\n  style Z fill:#ff0000\n  class A nosuch\n",
        &theme,
    );
    assert_eq!(plain.rows(), unreached.rows());
}

/// Review Focus 4.
#[test]
fn more_colours_than_slots_still_draw() {
    let theme = Theme::default_dark();
    let mut src = String::from("flowchart LR\n");
    for index in 0..20u32 {
        src.push_str(&format!("  N{index}\n  style N{index} stroke:#{:06x}\n", 0x100000 * (index % 15 + 1) + 0x40));
    }
    let canvas = render_mermaid(&src, 400, &theme).expect("draws");
    assert!(canvas.plain_text().contains("N19"));
}
```

In `shape.rs` tests, extend `an_empty_label_still_draws_a_box` to pass `&NodeStyle::plain(&theme)` and add:

```rust
    #[test]
    fn the_fill_reaches_every_inside_cell_and_no_border_cell() {
        let theme = Theme::default_dark();
        let tint = Color::hex(0x123456);
        let style = NodeStyle { fill: Some(tint), ..NodeStyle::plain(&theme) };
        for (shape, wall) in [(NodeShape::Rect, 1), (NodeShape::Circle, 2), (NodeShape::Subroutine, 1), (NodeShape::Cylinder, 1), (NodeShape::Rhombus, 1)] {
            let canvas = draw(&Label::line("ab"), shape, 20, &theme, &style);
            let (rows, cols) = (canvas.height(), usize::from(canvas.width()));
            for row in 0..rows {
                for col in 0..cols {
                    let inside = row > 0 && row + 1 < rows && col >= wall && col + wall < cols;
                    let bg = canvas.row(row).expect("row")[col].style().bg;
                    assert_eq!(bg == Some(tint), inside, "{shape:?} at {row},{col}");
                }
            }
        }
    }
```

(imports: `use crate::mermaid::layout::painted::NodeStyle; use crate::theme::Color;`.)

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_colours -- --test-threads=4`
Expected: FAIL (borders still `node_border`, no tint).

- [ ] **Step 3: Implement `src/mermaid/layout/painted.rs`:**

```rust
// SPDX-License-Identifier: MIT
//! From an author's paint to the styles a node is drawn in (colour spec §6.1).
//!
//! The parse side keeps what was written; the theme keeps 16 repaired slot inks; this
//! is the one place the two meet, once per diagram, so every family resolves a colour
//! to the same slot and draws it the same way.

use crate::canvas::Canvas;
use crate::mermaid::ast::Paint;
use crate::mermaid::paint::{self, Resolution};
use crate::theme::{Color, SlotInk, Style, Theme};

/// How one node box is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NodeStyle {
    /// The outline.
    pub border: Style,
    /// Inner rules: subroutine bars, cylinder lids, class and ER dividers.
    pub rule: Style,
    /// The background of every cell between the border cells, or `None` for the page.
    pub fill: Option<Color>,
    /// Draw the outline heavy (`stroke-width` 3px or more).
    pub heavy: bool,
}

impl NodeStyle {
    /// The theme default, which is how every node was drawn before colour lines.
    pub(crate) fn plain(theme: &Theme) -> Self {
        Self {
            border: theme.diagram.node_border,
            rule: theme.diagram.compartment,
            fill: None,
            heavy: false,
        }
    }

    /// Sets the fill as background of every cell inside a finished box: all rows but
    /// the first and last, all columns but `wall` on each side.
    ///
    /// Border cells keep the page (colour spec §6.2): a tinted outline would read as a
    /// thicker box rather than a coloured one.
    pub(crate) fn fill_inside(&self, canvas: &mut Canvas, wall: usize) {
        let Some(fill) = self.fill else {
            return;
        };
        let cols = usize::from(canvas.width()).saturating_sub(2 * wall);
        for row in 1..canvas.height().saturating_sub(1) {
            canvas.patch_style(row, wall, cols, Style::new().bg(fill));
        }
    }
}

/// Resolves one diagram's paints against a theme.
pub(crate) struct Painter<'t> {
    theme: &'t Theme,
    resolution: Resolution,
}

impl<'t> Painter<'t> {
    /// Resolves every paint of the diagram, nodes and frames alike, in one go.
    pub(crate) fn new<'p>(paints: impl IntoIterator<Item = &'p Paint>, theme: &'t Theme) -> Self {
        Self { theme, resolution: Resolution::of(paints) }
    }

    /// The slot `paint` draws in, when it has a hue.
    fn slot(&self, paint: &Paint) -> Option<SlotInk> {
        self.resolution.slot(paint).map(|slot| self.theme.diagram_slots[slot])
    }

    /// The tint a paint's `fill` asks for: only a hued fill tints (ruling 13), and a
    /// slot that fell back to the page tints nothing.
    fn tint(&self, paint: &Paint, tint: impl Fn(SlotInk) -> Color) -> Option<Color> {
        let hued = paint.fill.is_some_and(|fill| paint::hue_of(fill.rgb).is_some());
        self.slot(paint)
            .filter(|_| hued)
            .map(tint)
            .filter(|&color| color != self.theme.palette.bg)
    }

    /// The style of a node with this paint (colour spec §6.1).
    pub(crate) fn node(&self, paint: Option<&Paint>) -> NodeStyle {
        let Some(paint) = paint else {
            return NodeStyle::plain(self.theme);
        };
        let base = self.theme.diagram.node_border;
        // The border takes the slot even when the slot came from `fill`.
        let border = match self.slot(paint).and_then(|slot| slot.ink) {
            Some(ink) => base.fg(ink),
            None => base,
        };
        NodeStyle {
            border,
            rule: border,
            fill: self.tint(paint, |slot| slot.full_tint),
            heavy: paint.heavy,
        }
    }
}
```

`layout.rs`: `mod painted;` (private; `pub(crate)` items are reachable from sibling modules).

`flowchart.rs`:

```rust
pub fn draw_with(chart: &Flowchart, width: u16, theme: &Theme, fit: Fit) -> Result<Canvas, MermaidError> {
    let mut paints: Vec<&Paint> = chart.nodes.iter().filter_map(|node| node.paint.as_ref()).collect();
    group_paints(&chart.root, &mut paints);
    let painter = Painter::new(paints, theme);
    let styles = chart.nodes.iter().map(|node| painter.node(node.paint.as_ref())).collect();
    let spec = build(chart);
    graph::draw(&spec, &Art { chart, styles }, width, theme, fit)
}

/// Every subgraph paint, so frames take part in the slot assignment from the start and
/// a node's slot never depends on whether frames are drawn.
fn group_paints<'c>(group: &'c Group, out: &mut Vec<&'c Paint>) {
    for child in &group.children {
        out.extend(child.paint.as_ref());
        group_paints(child, out);
    }
}
```

`Art` gets `styles: Vec<NodeStyle>`; `render` passes `self.styles.get(node.0).copied().unwrap_or_else(|| NodeStyle::plain(theme))`.

`shape.rs`: `draw(label, shape, budget, theme, style: &NodeStyle)`. Every `styles.node_border` becomes `style.border`; pass `style` into `walls`, `rhombus`, `subroutine`, `cylinder`. Border sets:

```rust
    let rounded = if style.heavy { BorderSet::ROUNDED_HEAVY } else { BorderSet::ROUNDED };
    let square = if style.heavy { BorderSet::HEAVY } else { BorderSet::PLAIN };
    let (mut out, wall) = match shape {
        NodeShape::Rect => (body.framed(square, style.border, None, theme.base()), 1),
        NodeShape::Round => (body.framed(rounded, style.border, None, theme.base()), 1),
        NodeShape::Stadium => (walls(&body, theme, style, "(", ")", rounded), 1),
        NodeShape::Circle => {
            let wide = body.indent(1, 1, theme.base());
            (walls(&wide, theme, style, "((", "))", rounded), 2)
        }
        NodeShape::Rhombus => (rhombus(&body, theme, style), 1),
        NodeShape::Subroutine => (subroutine(&body, theme, style, square), 1),
        NodeShape::Cylinder => (cylinder(&body, theme, style, rounded), 1),
    };
    style.fill_inside(&mut out, wall);
    out
```

- `rhombus`: cap `─` becomes `if style.heavy { "━" } else { "─" }`, sides `│` become `┃` when heavy; diagonals stay `╱ ╲`.
- `subroutine`: frame with `square`; the inner bars stay light `│`; their ends on the top and bottom edge are `┬ ┴` when light and `┯ ┷` when heavy (`BorderSet::ROUNDED_HEAVY.tee_down`/`.tee_up`, since `HEAVY`'s own tees are `┳ ┻`, which would draw the bar heavy).
- `cylinder`: frame with `rounded`; each lid rule is a tee, a run of light `─` and a tee: `├ ┤` light, `┠ ┨` heavy (`rounded.tee_right`/`.tee_left` for the heavy set, `'├'`/`'┤'` otherwise).
- Bars, lid rules and their tees are all drawn in `style.border`: for an unpainted node that is `node_border`, exactly as today, and for a painted node it is the slot ink, which is the "inner rules in the border colour" of §6.2. `style.rule` is used only by the record dividers of Task 11, whose unpainted ink is `diagram.compartment`.

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_colours --lib -- --test-threads=4 mermaid` then the full suite.
Expected: PASS; snapshot tests unchanged. If `every_shape_has_its_heavy_form` cannot find a piece because the layout wraps a label, widen the test to 200 rather than shortening the assertion list.

- [ ] **Step 5: Gates and commit**

```bash
git add src/mermaid/layout/painted.rs src/mermaid/layout.rs src/mermaid/layout/flowchart.rs src/mermaid/layout/flowchart/shape.rs tests/mermaid_colours.rs
git commit -m "feat: draw Mermaid flowchart nodes in their colour class

A painted flowchart node draws its border in the theme slot its class
resolved to, fills its inside with the slot's tint when the class sets
a fill, and draws a heavy outline for stroke-width 3px or more; arcs,
parentheses and diagonals stay light because Box Drawing has no heavy
form of them. A node without colour lines draws as before.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: State boxes and class/ER records drawn in their resolved style

**Files:**
- Modify: `src/mermaid/layout/state/shape.rs:28-79` (`state`, `note`, `box_of`; tests)
- Modify: `src/mermaid/layout/state.rs` (`draw_with` lines 62-83, `Art` lines 326-367)
- Modify: `src/mermaid/layout/record.rs:91-158` (`draw`; tests)
- Modify: `src/mermaid/layout/class.rs` (`draw_with`, `Art`, `class_box`), `src/mermaid/layout/er.rs` (`draw_with`, `Art`, `entity_box`)
- Test: `tests/mermaid_colours.rs`

**Interfaces:**
- Consumes: `Painter`, `NodeStyle` (Task 10); `StateNode::paint`, `Class::paint`, `Entity::paint` (Tasks 6-8).
- Produces: `shape::state(label, budget, theme, style: &NodeStyle)`; `record::draw(compartments, budget, theme, style: &NodeStyle)`. `shape::note` keeps `NodeStyle::plain` (notes take no paint, §3.1). In `state.rs`, a `Painter` built over every `StateNode` paint (plain and composite, so Task 12's frames share the resolution).

- [ ] **Step 1: Write the failing tests** in `tests/mermaid_colours.rs`:

```rust
#[test]
fn a_painted_state_is_a_heavy_round_box_in_its_slot() {
    let theme = Theme::default_dark();
    let canvas = draw("stateDiagram-v2\n  [*] --> Idle:::hot\n  Idle --> Done\n  classDef hot fill:#ff0000,stroke-width:3px\n", &theme);
    let (row, col) = locate(&canvas, "Idle");
    let (top, left) = corner(&canvas, row, col);
    assert_eq!(glyph(&canvas, top, left), "╭", "arcs stay light");
    assert_eq!(glyph(&canvas, top + 1, left), "┃");
    assert_eq!(fg(&canvas, top, left), theme.diagram_slots[0].ink);
    assert_eq!(bg(&canvas, row, col), Some(theme.diagram_slots[0].full_tint));
    let (row, col) = locate(&canvas, "Done");
    let (top, left) = corner(&canvas, row, col);
    assert_eq!(fg(&canvas, top, left), theme.diagram.node_border.fg, "unpainted");
}

#[test]
fn a_painted_class_has_heavy_dividers_in_its_border_ink() {
    let theme = Theme::default_dark();
    let canvas = draw("classDiagram\n  class Animal:::k {\n    +int age\n  }\n  classDef k fill:#2a8bb5,stroke-width:3px\n", &theme);
    let text = canvas.plain_text();
    assert!(text.contains("┏━") && text.contains("┠─") && text.contains("─┨"), "{text}");
    let (row, col) = locate(&canvas, "┠");
    let ink = theme.diagram_slots[9];
    assert_eq!(fg(&canvas, row, col), ink.ink);
    assert_eq!(fg(&canvas, row, col + 1), ink.ink, "the rule takes the border ink");
    assert_eq!(bg(&canvas, row, col + 1), Some(ink.full_tint), "the rule is inside");
    assert_eq!(bg(&canvas, row, col), Some(theme.palette.bg), "the tee is border");
}

#[test]
fn a_painted_entity_draws_its_slot() {
    let theme = Theme::default_dark();
    let canvas = draw("erDiagram\n  CUSTOMER:::k ||--o{ ORDER : places\n  CUSTOMER {\n    string name\n  }\n  classDef k stroke:#d4831f\n", &theme);
    let (row, col) = locate(&canvas, "CUSTOMER");
    let (top, left) = corner(&canvas, row, col);
    assert_eq!(fg(&canvas, top, left), theme.diagram_slots[2].ink);
    assert_eq!(bg(&canvas, row, col), Some(theme.palette.bg), "stroke alone does not fill");
}
```

Update the unit tests in `record.rs` and `state/shape.rs` to pass `&NodeStyle::plain(&theme)`.

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_colours -- --test-threads=4`
Expected: the three new tests FAIL.

- [ ] **Step 3: Implement.**

`state/shape.rs`: `state(label, budget, theme, style: &NodeStyle)` passes `style` to `box_of`; `note` passes `&NodeStyle::plain(theme)`. In `box_of`, frame with `if style.heavy { BorderSet::ROUNDED_HEAVY } else { BorderSet::ROUNDED }` in `style.border`, then `style.fill_inside(&mut out, 1)`.

`state.rs` `draw_with`:

```rust
    let painter = Painter::new(diagram.states.iter().filter_map(|state| state.paint.as_ref()), theme);
    let styles = diagram.states.iter().map(|state| painter.node(state.paint.as_ref())).collect();
```
`Art` gets `styles: Vec<NodeStyle>` indexed by `StateId`; the `_ =>` arm of `render` calls `shape::state(&label_text(state), budget, theme, &style)` with the style of the slot's `StateId` (look it up through `Slot::State(id)`). Keep `painter` alive for Task 12 (it builds the plan's frames); for now it is only used here.

`record.rs` `draw(compartments, budget, theme, style: &NodeStyle)`:

```rust
    let border = if style.heavy { BorderSet::HEAVY } else { BorderSet::PLAIN };
    let (left, right) = if style.heavy { ("┠", "┨") } else { ("├", "┤") };
    let mut out = body.framed(border, style.border, None, theme.base());
    for rule in rules {
        let row = rule + 1;
        out.write_str(row, 0, left, style.border);
        out.hline(row, 1, inner, "─", style.rule);
        out.write_str(row, inner + 1, right, style.border);
    }
    style.fill_inside(&mut out, 1);
    out
```
(`style.rule` is `diagram.compartment` for an unpainted node, which is today's divider ink.)

`class.rs` and `er.rs`: build a `Painter` over the classes' / entities' paints in `draw_with`, keep `styles: Vec<NodeStyle>` on `Art`, and pass the node's style to `class_box`/`entity_box`, which pass it to `record::draw`.

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_colours --lib -- --test-threads=4 mermaid`, then the full suite.
Expected: PASS, no snapshot change.

- [ ] **Step 5: Gates and commit**

```bash
git add src/mermaid/layout/state/shape.rs src/mermaid/layout/state.rs src/mermaid/layout/record.rs src/mermaid/layout/class.rs src/mermaid/layout/er.rs tests/mermaid_colours.rs
git commit -m "feat: draw Mermaid states, classes and entities in their colour class

State boxes, class boxes and ER entities take the border ink, interior
tint and heavy outline of their colour class, as flowchart nodes do.
A heavy class or entity box divides its compartments with light rules
in its border ink (┠─┨). Notes, start and end markers, choice, fork
and join stay in the theme colours.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Frames: ink, heavy dashes, half-tint wash

**Files:**
- Modify: `src/mermaid/layout/graph/spec.rs` (`FrameStyle` after `GroupSpec`, field on `GroupSpec` line 141; `NodeArt::keeps_page` after `ports`, line 297)
- Modify: `src/mermaid/layout/graph.rs` (`draw` lines 207-216; `Drawn` 456-461; `Item` 482-490; `group` 493-532; `frame` 571-627; `level` 711-730; new `tint_frames`)
- Modify: `src/mermaid/layout/flowchart.rs` (`build`, `group` lines 114-132)
- Modify: `src/mermaid/layout/state.rs` (`Plan::of`, `Plan::scope` lines 120-215, `Art::keeps_page`)
- Modify: `src/mermaid/layout/painted.rs` (`Painter::frame`)
- Test: `tests/mermaid_colours.rs`, `src/mermaid/layout/graph/tests.rs`

**Interfaces:**
- Consumes: `Painter` (Task 10), `BorderSet::DASHED_HEAVY` (Task 9), `Group::paint`, composite `StateNode::paint`.
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameStyle { pub ink: Option<Color>, pub heavy: bool, pub tint: Option<Color> }
pub struct GroupSpec { /* … */ pub style: FrameStyle }
pub trait NodeArt { /* … */ fn keeps_page(&self, node: NodeIdx) -> bool { false } }
impl Painter<'_> { pub(crate) fn frame(&self, paint: Option<&Paint>) -> FrameStyle }
```

`FrameStyle` holds colours, not styles, so `GroupSpec` stays `Default` and theme-free: `ink: None` means `group_border`/`group_title`. Frame rectangles are carried up the recursion beside the node hints and shifted the same way; since a frame is recorded after everything inside it, the list is innermost first, which is the paint order §6.3 asks for (no depth field needed). A cell counts as claimed once any frame's pass has visited it, so an inner frame keeps its own wash even when that wash equals the page; notes (`keeps_page`) are skipped.

- [ ] **Step 1: Write the failing tests** in `tests/mermaid_colours.rs`:

```rust
#[test]
fn a_heavy_frame_is_dashed_in_its_ink_and_an_edge_crosses_it_mixed() {
    let theme = Theme::default_dark();
    let canvas = draw(
        "flowchart TB\n  outside --> inside\n  subgraph box [Box]\n    inside\n  end\n\
         style box stroke:#ff0000,stroke-width:3px\n",
        &theme,
    );
    let text = canvas.plain_text();
    assert!(text.contains("╭ Box ╍") || text.contains("╭╍"), "{text}");
    assert!(text.contains('╏') && text.contains('┿'), "{text}");
    let (row, col) = locate(&canvas, "Box");
    let ink = theme.diagram_slots[0].ink;
    assert_eq!(fg(&canvas, row, col), ink, "title");
    assert!(canvas.row(row).expect("row")[col].style().attrs.contains(mdmost::theme::Attributes::BOLD));
    assert_eq!(fg(&canvas, row, col - 2), ink, "border");
}

#[test]
fn a_filled_frame_washes_its_area_innermost_first() {
    let theme = Theme::default_dark();
    let canvas = draw(
        "flowchart TB\n  subgraph outer\n    alpha\n    subgraph inner\n      beta\n    end\n    gamma:::own\n  end\n\
         style outer fill:#ff0000\n  style inner fill:#0000ff\n  classDef own fill:#00ff00\n",
        &theme,
    );
    let red = theme.diagram_slots[0];
    let blue = theme.diagram_slots[11];
    let (row, col) = locate(&canvas, "alpha");
    assert_eq!(bg(&canvas, row, col), Some(red.half_tint), "an unfilled node in the outer frame");
    let (row, col) = locate(&canvas, "beta");
    assert_eq!(bg(&canvas, row, col), Some(blue.half_tint), "the inner frame keeps its own wash");
    let (row, col) = locate(&canvas, "╭ inner");
    assert_eq!(bg(&canvas, row, col), Some(red.half_tint), "the inner border sits in the outer wash");
    let (row, col) = locate(&canvas, "gamma");
    // #00ff00 is 120 degrees, nearer 97.5 than 150: yellow-green, slot 5.
    let green = theme.diagram_slots[5];
    assert_eq!(bg(&canvas, row, col), Some(green.full_tint), "a filled node keeps its full tint");
    let (row, col) = locate(&canvas, "╭ outer");
    assert_eq!(bg(&canvas, row, col), Some(theme.palette.bg), "the frame's own border keeps the page");
}

#[test]
fn notes_in_a_filled_composite_keep_the_page() {
    let theme = Theme::default_light();
    let canvas = draw(
        "stateDiagram-v2\n  state Busy {\n    Work --> Rest\n    note right of Work : later\n  }\n  style Busy fill:#ff0000\n",
        &theme,
    );
    let red = theme.diagram_slots[0];
    let (row, col) = locate(&canvas, "Work");
    assert_eq!(bg(&canvas, row, col), Some(red.half_tint));
    let (row, col) = locate(&canvas, "later");
    assert_eq!(bg(&canvas, row, col), Some(theme.palette.bg));
}
```

In `graph/tests.rs` (engine-level, independent of any family):

```rust
#[test]
fn a_tinted_group_washes_only_page_cells_inside_it() {
    let theme = Theme::default_dark();
    let tint = crate::theme::Color::hex(0x203040);
    let mut spec = spec(Direction::TopToBottom, 2, &[(0, 1)]);
    spec.root.nodes = vec![NodeIdx(0)];
    spec.root.children = vec![GroupSpec {
        title: Some(DrawnLabel::whole(&Label::line("g"))),
        nodes: vec![NodeIdx(1)],
        style: super::FrameStyle { ink: None, heavy: false, tint: Some(tint) },
        ..GroupSpec::default()
    }];
    let canvas = draw(&spec, &art, 60, &theme, Fit::COMPACT).expect("fits");
    let tinted = canvas.rows().iter().flatten().filter(|cell| cell.style().bg == Some(tint)).count();
    assert!(tinted > 0, "{}", canvas.plain_text());
    assert_eq!(canvas.row(0).expect("row")[0].style().bg, Some(theme.palette.bg), "outside stays");
}
```

(`spec` and `art` are the helpers `graph/tests.rs` already uses; check their names at the top of that file and adapt.)

- [ ] **Step 2: Run to see them fail**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_colours -- --test-threads=4`
Expected: FAIL (`no field style`, frames unpainted).

- [ ] **Step 3: Implement.**

`spec.rs`:

```rust
/// How a container frame is drawn (colour spec §6.3). The default is the theme's own
/// frame, so a caller that knows nothing about colour gets today's drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameStyle {
    /// Border and title ink; `None` for `group_border` and `group_title`.
    pub ink: Option<Color>,
    /// Draw the frame with heavy dashes (`╍ ╏`).
    pub heavy: bool,
    /// The wash for every page-coloured cell inside the frame, or `None`.
    pub tint: Option<Color>,
}
```
`GroupSpec` gets `/// How the frame is drawn; ignored for a group without a title. pub style: FrameStyle,`. `NodeArt` gets:

```rust
    /// Whether `node` keeps the page background inside a washed frame.
    ///
    /// A state note does: its text is drawn in the note ink, which misses the text
    /// floor on the light theme's half tints (colour spec §6.3). Defaults to `false`.
    fn keeps_page(&self, node: NodeIdx) -> bool {
        let _ = node;
        false
    }
```
Re-export `FrameStyle` from `graph.rs` (`pub use spec::{…, FrameStyle, …}`).

`graph.rs`:
- New type beside `Spot`:

```rust
/// A washed frame's rectangle inside a canvas, and its wash.
#[derive(Debug, Clone, Copy)]
struct Washed {
    at: Spot,
    tint: Color,
}
```
- `Drawn` and `Item` gain `frames: Vec<Washed>`. In `group`, node items get `frames: Vec::new()`, child items `frames: drawn.frames`. In `level`, beside the hints loop: `for washed in &item.frames { frames.push(Washed { at: washed.at.shifted(row, col), ..*washed }); }`, returned in `Drawn`. The empty-items early return gets `frames: Vec::new()`.
- `group` passes `&group.style` to `frame`.
- `frame(drawn, title, crossed, style: &FrameStyle)`:

```rust
        let border = style.ink.map_or(styles.group_border, |ink| styles.group_border.fg(ink));
        let title_style = style.ink.map_or(styles.group_title, |ink| styles.group_title.fg(ink));
        let set = if style.heavy { BorderSet::DASHED_HEAVY } else { BorderSet::DASHED };
```
  replacing `styles.group_border`/`styles.group_title` in the framing and the spaced title; after computing `hints`, shift `drawn.frames` by `(2, 2 + place.shift)` and, when `style.tint` is `Some`, push this frame's own `Washed { at: Spot { row: 0, col: 0, rows: canvas.height(), cols: usize::from(canvas.width()) }, tint }` last.
- `draw`: change the `attempt` closure to

```rust
    let attempt = |gap: usize, budget: u16| {
        let ctx = Ctx { spec, art, theme, budget, gap };
        let drawn = ctx.group(&spec.root, spec.direction);
        ctx.washed(drawn)
    };
```
  and add to `impl Ctx`:

```rust
    /// The finished drawing with every washed frame applied, innermost first.
    ///
    /// Done once on the whole canvas rather than per frame, because the level above a
    /// frame draws the edges and labels that enter it after the frame is finished, on
    /// the page background (colour spec §6.3).
    fn washed(&self, mut drawn: Drawn) -> Canvas {
        if drawn.frames.is_empty() {
            return drawn.canvas;
        }
        let keep: Vec<Spot> = drawn
            .hints
            .iter()
            .filter(|&&(node, _)| self.art.keeps_page(node))
            .map(|&(_, spot)| spot)
            .collect();
        tint_frames(&mut drawn.canvas, &drawn.frames, &keep, self.theme.palette.bg);
        drawn.canvas
    }
```
  and the free function:

```rust
/// Washes the inside of each frame, in order, over cells still on the page.
///
/// `frames` is innermost first. A cell once visited is claimed, so an outer wash never
/// reaches into an inner frame, even one whose wash equals the page; cells of a node
/// that `keep`s the page are skipped; a node with its own fill is no longer on the page
/// and keeps it.
fn tint_frames(canvas: &mut Canvas, frames: &[Washed], keep: &[Spot], page: Color) {
    let cols = usize::from(canvas.width());
    let mut claimed = vec![false; canvas.height() * cols];
    let kept = |row: usize, col: usize| {
        keep.iter().any(|s| (s.row..s.row + s.rows).contains(&row) && (s.col..s.col + s.cols).contains(&col))
    };
    for washed in frames {
        let Spot { row, col, rows, cols: width } = washed.at;
        for r in row + 1..(row + rows).saturating_sub(1) {
            for c in col + 1..(col + width).saturating_sub(1) {
                let Some(flag) = claimed.get_mut(r * cols + c) else { continue };
                if std::mem::replace(flag, true) || kept(r, c) {
                    continue;
                }
                if canvas.row(r).and_then(|cells| cells.get(c)).is_some_and(|cell| cell.style().bg == Some(page)) {
                    canvas.patch_style(r, c, 1, Style::new().bg(washed.tint));
                }
            }
        }
    }
}
```
  (imports: `crate::theme::{Color, Style}`; guard `c < cols` before indexing.)

`painted.rs`:

```rust
    /// The style of a subgraph or composite state frame with this paint (§6.3).
    pub(crate) fn frame(&self, paint: Option<&Paint>) -> FrameStyle {
        let Some(paint) = paint else {
            return FrameStyle::default();
        };
        FrameStyle {
            ink: self.slot(paint).and_then(|slot| slot.ink),
            heavy: paint.heavy,
            tint: self.tint(paint, |slot| slot.half_tint),
        }
    }
```

`flowchart.rs`: `build(chart, &painter)` and `group(group, painter)` set `style: painter.frame(group.paint.as_ref())`.

`state.rs`: `Plan::of(diagram, &painter)`, threaded into `scope`; where a composite becomes a frame (`child.title = Some(…)`) also set `child.style = painter.frame(state.paint.as_ref());`. `Art::keeps_page`: `matches!(self.plan.slots.get(node.0), Some(Slot::Note(_)))`.

- [ ] **Step 4: Run the tests**

Run: `systemd-run --user --scope -q -p MemoryMax=8G -- cargo test -j4 --test mermaid_colours --lib -- --test-threads=4 mermaid`, then the full suite.
Expected: PASS, no snapshot change. If `a_heavy_frame…` finds no `┿`, print the canvas: the edge may enter through the title gap; then assert on the column the edge passes the frame's bottom or side instead, but keep a mixed-junction assertion (spec §8).

- [ ] **Step 5: Gates and commit**

```bash
git add src/mermaid/layout/graph/spec.rs src/mermaid/layout/graph.rs src/mermaid/layout/flowchart.rs src/mermaid/layout/state.rs src/mermaid/layout/painted.rs src/mermaid/layout/graph/tests.rs tests/mermaid_colours.rs
git commit -m "feat: draw Mermaid subgraph and composite state colours

A painted frame draws its border and title in its slot ink, heavy
frames as ╍ and ╏ with light arcs, and a fill washes every cell inside
the frame that is still on the page with the slot's half tint, inner
frames first. The layout now carries each washed frame's rectangle to
the finished canvas, because the edges into a frame are drawn by the
level above it. Notes keep the page background.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Documentation and changelog

**Files:**
- Modify: `docs/manual.md:547-556` (Mermaid section)
- Modify: `docs/superpowers/specs/2026-08-08-mdmost-design.md:312-360` (§6.1, §6.3, §6.4, §6.7)
- Modify: `docs/maintainer-notes.md` (new section after "The diagram engine seam is `NodeArt`")
- Modify: `CHANGES.md` (Unreleased)

Load the `repo-infra:writing-style` skill first (man-page voice, no self-praise, no em dashes; rationale goes to maintainer notes, not the manual).

- [ ] **Step 1: Manual.** In the flowchart bullet replace "Out of scope: `click`, `style`/`classDef`, `linkStyle`." with "Out of scope: `click`, `linkStyle`." and add after the family list:

```markdown
Flowchart, state, class and ER diagrams read `classDef`, `class` (`cssClass` in
class diagrams), `style` and the `:::name` suffix. Of the properties, `fill`,
`stroke` (`#rgb`, `#rrggbb`, `#rrggbbaa` or a CSS colour name) and `stroke-width`
are read; others, such as `color`, are ignored. Colours are drawn in the hues of the
active theme, not as written: each colour takes the nearest of 16 theme hues, and two
different colours never share one while a free hue is left. A `stroke` colours the
border, a `fill` also tints the inside, and a `stroke-width` of 3px or more draws the
outline heavy. A `fill` on a subgraph or composite state tints its whole area more
lightly. Node text keeps the theme's text colour.
```

- [ ] **Step 2: Design spec.** §6.1: replace "Out of scope: `click`, `style`/`classDef` colors, `linkStyle`." with "Colours from `classDef`, `class`, `style` and `:::`: see `docs/superpowers/specs/2026-10-03-mermaid-colours-design.md`. Out of scope: `click`, `linkStyle`." Add the same pointer sentence as a bullet to §6.3, §6.4 and §6.7.

- [ ] **Step 3: Maintainer notes.** New section:

```markdown
## Mermaid colours are snapped, not drawn as written

A Mermaid stylesheet is written for a white page: `fill:#e3f4fb,color:#000` is a pale
box with black text, and drawn as written on a dark terminal it is a bright block with
unreadable text. What the colours carry is the grouping, so each colour snaps to one of
16 theme slots at fixed angles (`mermaid::paint`), and the theme decides what the slot
looks like (`theme::slots`), repaired so every theme clears the floors in
`tests/theme_contrast.rs`. Fixed angles rather than the theme's measured hues keep a
diagram on the same slots in every theme.

When a colour finds its slot taken it also skips the two neighbouring slots: a midpoint
slot draws as the blend of its neighbours, so `#d4831f` and `#b8650a`, two oranges an
author meant as two classes, would otherwise land one blend apart and read as one.

The repair moves inks, not tints. Lowering the light theme's half tints until orange ink
passed on them left them invisible; moving six light inks 0.05 to 0.10 towards the text
keeps the tints and shifts no hue by 2 degrees. Design: `docs/superpowers/specs/2026-10-03-mermaid-colours-design.md`.
```

- [ ] **Step 4: CHANGES.md.** Under `### New` add (users read it; what they see, three sentences at most):

```markdown
- Mermaid flowchart, state, class and ER diagrams now draw the colours set with `classDef`, `class`, `cssClass`, `style` and `:::`. Each colour is drawn in the nearest hue of the active theme, so text stays readable on dark and light terminals: `stroke` colours the border, `fill` also tints the inside, and `stroke-width` of 3px or more draws a heavy outline. A `fill` on a subgraph or composite state tints its whole area.
```

and in the six `### Fixed` entries that end in "The class is not drawn yet." (or "The class is not drawn yet, so the node looks as it would without it."), delete that trailing sentence: it was true when written and is not any more. Ask the controller before changing any other wording of those entries.

- [ ] **Step 5: Verify and commit.** Run `cargo test -j4 --test glyph_inventory -- --test-threads=4` (the manual's block list) and the three gates. Then:

```bash
git add docs/manual.md docs/superpowers/specs/2026-08-08-mdmost-design.md docs/maintainer-notes.md CHANGES.md
git commit -m "docs: describe Mermaid colour classes

The manual lists the colour statements and properties read and says
colours are drawn in theme hues; the design spec points at the colour
design instead of listing the lines as out of scope; the maintainer
notes record why colours snap and why collisions skip neighbours.
CHANGES gains the entry and loses the 'not drawn yet' remarks.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Self-review

Spec section to task:

| Spec | Task |
|---|---|
| §1 driving example | 5 (parse), 3 (resolution table), 10 (render) |
| §2 rulings 1-4, 10-12, 20 | 3; 5 | ruling 5 (no `linkStyle`/`click`): 5-8 keep skipping them |
| ruling 6, 17 | 12 | ruling 7 (`color` ignored) | 2, 10 (`text ink` assertion) |
| ruling 8 (theme-free parse) | 2, 5 | ruling 9, 18 | 9, 12 | ruling 13 | 10 (`Painter::tint`) |
| ruling 14 | 5, 6 | ruling 15 | 5-8 | ruling 16 | 4 | ruling 19 | done before this plan (c7d24c4..1520caf) |
| §3.1 `Paint` | 2 (plus `PaintColor`, see below) | §3.2, §3.3 | 5-8 |
| §3.4 property lists | 2 | §3.5 merge | 2, 5 | §3.6 errors | 5 |
| §4.1-§4.4 | 2 (hue), 3 | §5.1-§5.3 | 1, 4 |
| §6.1 | 10, 11 | §6.2 table | 9, 10, 11; frame row 12 | §6.3 | 12 |
| §7 | 4 | §8 tests | 2-12 as listed | §9 docs | 13 |

Deviations from the spec text, each forced by the code or by another section of the spec:

1. `PaintColor { rgb, named }` instead of a bare `Color` in `Paint` (§3.1): §4.4 snaps a unit "first written as a CSS name" by name, which a bare RGB value cannot express.
2. `Paint::origin` is the offset of the property, not of the line: same order between lines, and a deterministic order between `fill` and `stroke` of one line.
3. `SlotInk::ink` is an `Option`: `None` is §5.3 step 2's fallback.
4. §8's theme test "every slot either passes §7 or has fallen back" cannot hold for the fixed theme inks on a palette whose own text misses the page: step 3 then ends with the tint at the page and the pair still failing. Task 4 asserts what §5.3 guarantees instead (slot inks pass or fall back; fixed pairs pass or the tint reached the page).
5. Frame nesting order comes from recording frames after their content (post-order), not from a depth field (§6.3).
6. An inner frame or node whose tint equals the page is not recorded at all, and visited cells are claimed, so a fallen-back inner frame cannot be overwashed (pushback round 2, item 8).
7. `ruled_offsets` gains `┠ ┨ ┯ ┷` (§6.2 is silent): otherwise edges attach to heavy dividers.

## Execution handoff

Skipped by request: the controlling session asks the owner.
