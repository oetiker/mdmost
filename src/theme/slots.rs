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
    let named = [
        p.red, p.orange, p.yellow, p.green, p.cyan, p.blue, p.purple, p.magenta,
    ];
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
    let on_full = [
        (ink(d.node_text), TEXT),
        (ink(d.stereotype), TEXT),
        (ink(d.edge_label), TEXT),
    ];
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
        None => SlotInk {
            ink: None,
            full_tint: p.bg,
            half_tint: p.bg,
        },
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
        .find(|&ground| {
            pairs
                .iter()
                .all(|&(ink, floor)| ink.contrast(ground) >= floor)
        })
        .unwrap_or(bg)
}

#[cfg(test)]
mod tests {
    use crate::theme::{Color, Theme};

    /// Pinned rather than re-derived from the palette, so a change to the derivation
    /// cannot move the test along with it.
    #[test]
    fn the_dark_theme_needs_no_repair() {
        let theme = Theme::default_dark();
        let p = &theme.palette;
        let inks = [
            0xff6b7f, 0xff896b, 0xffa657, 0xf9bb61, 0xf2d06b, 0xb4d486, 0x76d7a0, 0x6bd7bc,
            0x5fd7d7, 0x6dbde7, 0x7aa2f7, 0x9a9ff8, 0xb99bf8, 0xd58de4, 0xf07fd0, 0xf875a8,
        ];
        for (slot, (ink, hex)) in theme.diagram_slots.iter().zip(inks).enumerate() {
            let hue = Color::hex(hex);
            assert_eq!(ink.ink, Some(hue), "slot {slot}");
            assert_eq!(ink.full_tint, p.bg.blend(hue, 0.15), "slot {slot}");
            assert_eq!(ink.half_tint, p.bg.blend(hue, 0.075), "slot {slot}");
        }
        assert_eq!(
            theme.diagram_slots[0].ink,
            Some(p.red),
            "red is the palette red"
        );
        assert_eq!(theme.diagram_slots[9].full_tint, Color::hex(0x1f2d3a));
        assert_eq!(theme.diagram_slots[9].half_tint, Color::hex(0x18212a));
    }

    /// Measured with the f32 blend of `Color::blend`: six light inks move 0.05 or 0.10
    /// towards the text, hue shifts under 2 degrees.
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
            assert_eq!(
                theme.diagram_slots[slot].ink,
                Some(Color::hex(hex)),
                "slot {slot}"
            );
        }
        assert_eq!(theme.diagram_slots[0].ink, Some(p.red), "red needs no step");
        // Tints come from the palette ink, not the repaired one.
        assert_eq!(theme.diagram_slots[2].full_tint, Color::hex(0xf2e4d4));
        assert_eq!(theme.diagram_slots[2].half_tint, Color::hex(0xf7f0e6));
    }
}
