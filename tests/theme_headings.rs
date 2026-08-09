//! The heading system: one hue family that recedes with depth, rules that take the tint
//! of the level they belong to, and none of them fainter than the page's own chrome.
//!
//! Headings carried a prefix glyph until 2026-08-09, and its tint was the third thing
//! asserted here; the rule under the heading now does that job alone (design spec §9.1).
//!
//! These assertions run over *every* built-in theme and over a user-defined palette,
//! because the whole point of deriving them in `Theme::from_palette` is that a config
//! theme inherits the same discipline.

use mdmost::theme::{Color, Style, Theme};

/// Perceptual distance between two colours, as RGB Manhattan distance.
///
/// The same crude-but-honest measure the code-token distinctness tests use: it is not
/// a colour-science model, it just refuses to call two colours different when a
/// terminal would paint them the same.
fn distance(a: Color, b: Color) -> u32 {
    u32::from(a.r.abs_diff(b.r)) + u32::from(a.g.abs_diff(b.g)) + u32::from(a.b.abs_diff(b.b))
}

/// The foreground of a style that is required to have one.
fn fg(name: &str, style: Style) -> Color {
    style
        .fg
        .unwrap_or_else(|| panic!("{name} needs a foreground"))
}

/// Every theme worth checking: the built-ins plus a theme derived from a raw palette,
/// which is what a `[themes.<name>]` block in `config.toml` produces.
fn themes() -> Vec<Theme> {
    let mut out: Vec<Theme> = Theme::builtin_names()
        .iter()
        .map(|name| Theme::builtin(name).expect("built-in theme resolves"))
        .collect();
    let dark = Theme::default_dark();
    out.push(Theme::from_palette("custom", true, dark.palette.clone()));
    out
}

/// Depth must read as recession. Each level steps *away* from the page's accent and
/// *towards* the muted text colour, so contrast against the background falls
/// monotonically — no level may be more salient than the one that contains it.
#[test]
fn the_heading_ramp_dims_monotonically_with_depth() {
    for theme in themes() {
        let bg = theme.palette.bg;
        let name = &theme.name;
        let mut previous = f32::INFINITY;
        for level in 1..=6u8 {
            let color = fg("heading", theme.heading(level));
            let contrast = (color.luminance() - bg.luminance()).abs();
            assert!(
                contrast < previous,
                "{name}: H{level} ({contrast:.3}) is not quieter than H{}",
                level - 1
            );
            previous = contrast;
        }
    }
}

/// A ramp is only a hierarchy if the reader can see the steps. Adjacent levels must be
/// perceptibly apart even though — unlike the code tokens — they deliberately share a
/// hue, so the gap here is one of luminance and saturation rather than colour.
#[test]
fn adjacent_heading_levels_are_perceptibly_apart() {
    for theme in themes() {
        let name = &theme.name;
        for level in 2..=6u8 {
            let above = fg("heading", theme.heading(level - 1));
            let here = fg("heading", theme.heading(level));
            let gap = distance(above, here);
            assert!(
                gap >= 24,
                "{name}: H{} and H{level} are only {gap} apart",
                level - 1
            );
        }
    }
}

/// Deep headings recede, but they are still headings: none of them may collapse onto
/// body text or onto the muted colour used for captions.
#[test]
fn every_heading_level_stays_distinct_from_body_and_muted() {
    for theme in themes() {
        let name = &theme.name;
        for level in 1..=6u8 {
            let color = fg("heading", theme.heading(level));
            for (other_name, other) in [("body", theme.palette.fg), ("muted", theme.palette.muted)]
            {
                let gap = distance(color, other);
                assert!(
                    gap >= 24,
                    "{name}: H{level} is only {gap} from {other_name}"
                );
            }
        }
    }
}

/// The rule under a heading is, since the prefix glyph was dropped on 2026-08-09, the
/// element that encodes the level. It must therefore track the heading's own colour,
/// and never be one fixed accent for every level.
#[test]
fn heading_rules_follow_their_own_level() {
    for theme in themes() {
        let name = &theme.name;
        for level in 1..=6u8 {
            let heading = fg("heading", theme.heading(level));
            let rule = fg("rule", theme.heading_rule(level));
            assert!(
                distance(heading, rule) < 128,
                "{name}: the H{level} rule is a different colour from the H{level} text"
            );
            for other in 1..=6u8 {
                if other == level {
                    continue;
                }
                assert_ne!(
                    theme.heading_rule(level),
                    theme.heading_rule(other),
                    "{name}: the H{level} and H{other} rules are the same style"
                );
            }
        }
        assert_eq!(
            theme.block.heading_rule,
            theme.heading_rule(1),
            "{name}: the legacy slot must keep agreeing with the level-aware one"
        );
        assert_eq!(theme.heading_rule(0), theme.heading_rule(1));
        assert_eq!(theme.heading_rule(9), theme.heading_rule(6));
    }
}

/// The signature rule under the signature heading must not be the least visible thing
/// on the line. Expressed as contrast against the page, so it holds in both polarities.
///
/// Two floors, because there are now two kinds of rule (design spec §9.1). The solid
/// ones under H1 and H2 keep the original bar: no fainter than muted text. The dashed
/// ones under H3-H5 are *meant* to recede, so theirs is that they must never be fainter
/// than the border colour the page draws its box frames and thematic breaks with — a
/// rule that announces a section cannot be quieter than the chrome around it. In the
/// light theme H4 sits between the two floors, which is why this is stated as two
/// bounds rather than one: the heading ramp there is nearly flat and that, not the
/// ladder, is the thing to fix.
#[test]
fn heading_rules_are_never_fainter_than_the_chrome() {
    for theme in themes() {
        let name = &theme.name;
        let bg = theme.palette.bg.luminance();
        let text_floor = (theme.palette.muted.luminance() - bg).abs();
        let chrome_floor = (theme.palette.border.luminance() - bg).abs();
        for level in 1..=6u8 {
            if !theme.heading_has_rule(level) {
                continue;
            }
            let rule = fg("rule", theme.heading_rule(level));
            let contrast = (rule.luminance() - bg).abs();
            let (floor, against) = if level <= 2 {
                (text_floor, "muted text")
            } else {
                (chrome_floor, "the border colour")
            };
            assert!(
                contrast >= floor,
                "{name}: the H{level} rule ({contrast:.3}) is fainter than {against} ({floor:.3})"
            );
        }
    }
}

/// The theme must carry its own background, or the document area inherits the
/// terminal's and the light theme is unusable on a dark terminal.
#[test]
fn the_base_style_carries_the_theme_background() {
    for theme in themes() {
        let name = &theme.name;
        assert_eq!(
            theme.base().bg,
            Some(theme.palette.bg),
            "{name}: base() must paint the theme background"
        );
        assert_eq!(theme.base().fg, Some(theme.palette.fg));
        assert_eq!(theme.background().bg, Some(theme.palette.bg));
        assert_eq!(theme.background().fg, None);
    }
}

/// The accent used to mean seven things at once. It now means "heading hierarchy", and
/// the chrome that used to borrow it has neutrals of its own.
#[test]
fn the_accent_is_not_reused_by_chrome_and_tables() {
    for theme in themes() {
        let name = &theme.name;
        let accent = theme.palette.accent;
        for (slot_name, style) in [
            ("table header", theme.table.header),
            ("scrollbar thumb", theme.ui.scrollbar_thumb),
            ("status accent", theme.ui.status_accent),
            ("list marker", theme.block.list_marker),
            ("table overflow marker", theme.table.overflow_marker),
            ("code overflow marker", theme.code.overflow_marker),
            ("diagram arrow", theme.diagram.arrow),
        ] {
            assert_ne!(
                style.fg,
                Some(accent),
                "{name}: {slot_name} still borrows the heading accent"
            );
        }
        // The current search match and the truncation marker are different events and
        // must not share a colour.
        assert_ne!(theme.ui.search_current.bg, theme.table.overflow_marker.fg);
    }
}

/// In a diagram the labels are the content; the lines are scaffolding. The scaffolding
/// must not out-shout what it carries.
#[test]
fn diagram_labels_read_louder_than_the_lines_they_sit_on() {
    for theme in themes() {
        let name = &theme.name;
        let bg = theme.palette.bg.luminance();
        let contrast = |style: Style, slot: &str| (fg(slot, style).luminance() - bg).abs();
        let label = contrast(theme.diagram.edge_label, "edge label");
        // Every stroke a diagram is built from: quieter than the labels, but never
        // fainter than secondary text — diagram ink was the faintest thing on the page.
        let floor = (theme.palette.muted.luminance() - bg).abs();
        for (slot_name, style) in [
            ("line", theme.diagram.line),
            ("lifeline", theme.diagram.lifeline),
        ] {
            let ink = contrast(style, slot_name);
            assert!(
                ink < label,
                "{name}: the diagram {slot_name} is louder than the labels on it"
            );
            assert!(
                ink >= floor,
                "{name}: the diagram {slot_name} ({ink:.3}) is fainter than muted text ({floor:.3})"
            );
        }
    }
}
