// SPDX-License-Identifier: MIT
//! Colour lines: property lists, CSS names, merging and slot resolution.
//!
//! Theme-free on purpose (colour spec ruling 8): this module turns what the author
//! wrote into [`Paint`] values and decides which of the theme's 16 slots each colour
//! takes. What a slot looks like is the theme's business (`theme::SlotInk`).

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
}
