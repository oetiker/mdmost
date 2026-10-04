// SPDX-License-Identifier: MIT
//! Colour lines: property lists, CSS names, merging and slot resolution.
//!
//! Theme-free on purpose (colour spec ruling 8): this module turns what the author
//! wrote into [`Paint`] values ([`parse_props`], [`merge`]) and decides which of the
//! theme's 16 slots each colour takes ([`Resolution`]). What a slot looks like is the
//! theme's business (`theme::SlotInk`).

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
    number
        .parse::<f32>()
        .ok()
        .filter(|w| w.is_finite() && *w >= 0.0)
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
/// counted as 360 against angles above 180, so that red wins a tie against
/// magenta-red (colour spec §4.4).
fn nearest(angle: f32, allowed: impl Fn(usize) -> bool) -> Option<usize> {
    let tie = |slot: usize| {
        if slot == 0 && angle > 180.0 {
            360.0
        } else {
            NOMINAL[slot]
        }
    };
    (0..SLOT_COUNT)
        .filter(|&slot| allowed(slot))
        .min_by(|&a, &b| {
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
            .map(|(_, color)| {
                // `named` is public, so a hand-built AST may carry a slot past 15:
                // that colour is placed by its hue like an unnamed one.
                color
                    .named
                    .and_then(|slot| NOMINAL.get(slot).copied())
                    .unwrap_or_else(|| hue_of(color.rgb).unwrap_or_default())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    fn hex(value: u32) -> PaintColor {
        PaintColor {
            rgb: Color::hex(value),
            named: None,
        }
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
        assert_eq!(
            parse_props("fill:#f9f", 0).fill.map(|f| f.0),
            Some(hex(0xff99ff))
        );
        assert_eq!(
            parse_props("fill:#11223344", 0).fill.map(|f| f.0),
            Some(hex(0x112233))
        );
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
                Some(PaintColor {
                    rgb: Color::hex(rgb),
                    named: slot
                }),
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
        assert_eq!(
            nearest(348.75, |_| true),
            Some(0),
            "360 beats 337.5, both 11.25 away"
        );
        assert_eq!(
            nearest(345.0, |_| true),
            Some(15),
            "no tie: 337.5 is nearer"
        );
        assert_eq!(nearest(359.0, |_| true), Some(0), "the shorter way round");
        assert_eq!(nearest(198.0, |_| true), Some(9));
    }

    #[test]
    fn a_named_slot_past_the_table_falls_back_to_its_hue() {
        let paint = merge([&Props {
            stroke: Some((
                PaintColor {
                    rgb: Color::hex(0x2a8bb5),
                    named: Some(16),
                },
                10,
            )),
            ..Props::default()
        }])
        .expect("painted");
        assert_eq!(
            Resolution::of([&paint]).slot(&paint),
            Some(9),
            "200 cyan-blue"
        );
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
        assert_eq!(
            resolution.slot(&part),
            Some(4),
            "45 yellow: 15, 30, 37.5 skipped"
        );
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
        assert_eq!(
            &slots[..SLOT_COUNT],
            (0..SLOT_COUNT).collect::<Vec<_>>().as_slice()
        );
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
        assert_eq!(
            resolution.slot(&green),
            Some(6),
            "by name, though 120 is nearer 97.5"
        );
        assert_eq!(
            resolution.slot(&lime),
            Some(8),
            "collides with green, skips 5-7"
        );
        assert_eq!(
            resolution.slot(&blue),
            Some(10),
            "by name, though 240 is a slot"
        );
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
}
