//! Measured contrast floors for the colours a reader looks at all day.
//!
//! `theme_headings` asserts *relationships* — this level is quieter than that one —
//! using a crude luminance difference. That is the right tool for a hierarchy and the
//! wrong tool for a floor: a ramp can be perfectly ordered and entirely unreadable.
//! The assertions here are absolute, in WCAG 2 contrast ratios, because a floor is a
//! number and drift back below it is exactly what a visual review caught by hand.
//!
//! Like `theme_headings`, every assertion runs over both built-ins *and* over a theme
//! derived from a raw palette, so a `[themes.<name>]` block in `config.toml` inherits
//! the same discipline rather than quietly opting out of it.

use mdless::theme::{Color, Style, Theme};

/// WCAG's floor for text: 4.5:1 against its own background.
const TEXT_FLOOR: f32 = 4.5;

/// WCAG's floor for meaningful non-text graphics — borders, rules, frames: 3:1.
const GRAPHIC_FLOOR: f32 = 3.0;

/// One sRGB channel, linearised (WCAG 2, relative luminance).
fn channel(value: u8) -> f32 {
    let c = f32::from(value) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG 2 relative luminance.
///
/// Deliberately *not* [`Color::luminance`], which is the NTSC weighting the palette
/// derivation uses to decide which way to shade. That one answers "which of these is
/// lighter"; this one is the only definition a contrast ratio is defined against.
fn relative_luminance(color: Color) -> f32 {
    0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
}

/// The WCAG 2 contrast ratio between two colours, in `1.0..=21.0`.
fn contrast(a: Color, b: Color) -> f32 {
    let (x, y) = (relative_luminance(a), relative_luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The foreground of a style that is required to have one.
fn fg(name: &str, style: Style) -> Color {
    style
        .fg
        .unwrap_or_else(|| panic!("{name} needs a foreground"))
}

/// Every theme worth checking: the built-ins plus a theme derived from a raw palette.
fn themes() -> Vec<Theme> {
    let mut out: Vec<Theme> = Theme::builtin_names()
        .iter()
        .map(|name| Theme::builtin(name).expect("built-in theme resolves"))
        .collect();
    for source in [Theme::default_dark(), Theme::default_light()] {
        let is_dark = source.is_dark;
        out.push(Theme::from_palette(
            "custom",
            is_dark,
            source.palette.clone(),
        ));
    }
    out
}

/// Asserts a ratio and says what it measured either way, because a failure whose
/// message is only "assertion failed" costs the next reader the same experiment.
fn at_least(theme: &str, what: &str, ink: Color, ground: Color, floor: f32) {
    let ratio = contrast(ink, ground);
    assert!(
        ratio >= floor,
        "{theme}: {what} measures {ratio:.2}:1, below the {floor:.1}:1 floor \
         (#{:02x}{:02x}{:02x} on #{:02x}{:02x}{:02x})",
        ink.r,
        ink.g,
        ink.b,
        ground.r,
        ground.g,
        ground.b
    );
}

/// The single most-used foreground in the application.
///
/// One colour draws every table frame, every code-fence frame and the thematic break —
/// a visual review counted 2387 cells of it on its probe pages, more than any other
/// ink on screen. It measured 1.79:1 (dark) and 1.77:1 (light), which is not a muted
/// border, it is a border the reader has to take on trust.
///
/// Both grounds matter. A code fence sits on the page, but a table's *vertical* rules
/// sit on the striped row's surface, and the surface is the harder of the two.
#[test]
fn structural_borders_clear_the_non_text_contrast_floor() {
    for theme in themes() {
        let name = &theme.name;
        let border = theme.palette.border;
        at_least(
            name,
            "the border colour on the page",
            border,
            theme.palette.bg,
            GRAPHIC_FLOOR,
        );
        at_least(
            name,
            "the border colour on a surface",
            border,
            theme.palette.surface,
            GRAPHIC_FLOOR,
        );
        // And through the slots that actually draw it, so a future theme cannot pass
        // this test with a compliant palette and a non-compliant derivation.
        for (slot, style, ground) in [
            ("the table border", theme.table.border, theme.palette.bg),
            (
                "the table border on a striped row",
                theme.table.border,
                theme.palette.surface,
            ),
            ("the code fence frame", theme.code.frame, theme.palette.bg),
            ("the thematic break", theme.block.rule, theme.palette.bg),
        ] {
            at_least(name, slot, fg(slot, style), ground, GRAPHIC_FLOOR);
        }
    }
}

/// Code punctuation is text, and in Rust it is the text you squint at.
///
/// `; : :: ( ) { } < >` were the one token class pushed below every other: 3.00:1 on
/// the dark surface and 2.61:1 on the light one, against 4.6–7.4:1 for keywords,
/// strings, types and the rest. Quiet is a decision; illegible is a bug.
#[test]
fn code_punctuation_clears_the_text_contrast_floor() {
    for theme in themes() {
        let name = &theme.name;
        at_least(
            name,
            "code punctuation on the code surface",
            fg("punctuation", theme.code.punctuation),
            theme.palette.surface,
            TEXT_FLOOR,
        );
    }
}

/// Body text and the theme's own page must not drift apart either.
///
/// Cheap, and it is the assertion that would have caught a palette edit that fixed a
/// border by moving the page out from under everything else.
#[test]
fn body_text_clears_the_text_contrast_floor() {
    for theme in themes() {
        at_least(
            &theme.name,
            "body text on the page",
            fg("body", theme.text.body),
            theme.palette.bg,
            TEXT_FLOOR,
        );
    }
}

/// Structure must stay quieter than the content riding on it.
///
/// The floors above push in one direction only; without this, "fix the contrast" has
/// an obvious wrong answer — paint the borders in body-text ink — that no other test
/// here would reject. The muted look is deliberate.
#[test]
fn borders_stay_quieter_than_the_text_they_frame() {
    for theme in themes() {
        let name = &theme.name;
        let page = theme.palette.bg;
        let border = contrast(theme.palette.border, page);
        let body = contrast(fg("body", theme.text.body), page);
        let muted = contrast(theme.palette.muted, page);
        assert!(
            border < muted && muted < body,
            "{name}: structure must recede — border {border:.2}:1, \
             muted {muted:.2}:1, body {body:.2}:1"
        );
        let punctuation = contrast(
            fg("punctuation", theme.code.punctuation),
            theme.palette.surface,
        );
        let text = contrast(fg("code text", theme.code.text), theme.palette.surface);
        assert!(
            punctuation < text,
            "{name}: punctuation ({punctuation:.2}:1) must stay quieter than code text \
             ({text:.2}:1)"
        );
    }
}
