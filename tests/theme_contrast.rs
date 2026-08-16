// SPDX-License-Identifier: MIT
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

use mdmost::theme::{Color, Style, Theme};

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
/// Both grounds are pinned, though only the page is drawn on today: a table's vertical
/// rules used to sit on the striped row's surface and now keep the page background even
/// there (see `render::table::render_row`). The surface is the harder of the two, and a
/// border is the one ink a theme may reasonably move onto either, so the floor stays.
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

/// Inline code is a hue with nothing behind it, so the page is what it has to clear.
///
/// It used to carry `surface` as a background and was measured against that. Dropping
/// the background — it was indistinguishable from the zebra stripe, so a `` `span` ``
/// inside a table read as a torn-off piece of banding — moves the pairing that matters
/// onto the page, and onto the stripe wherever a table puts it there. Measured when this
/// was written: 7.59:1 (dark) and 6.06:1 (light) on the page, 7.02:1 and 5.41:1 on the
/// stripe, against 7.02:1 and 5.41:1 for the old surface pairing. Both grounds are
/// asserted, because both are grounds inline code actually appears on.
#[test]
fn inline_code_clears_the_text_contrast_floor() {
    for theme in themes() {
        let name = &theme.name;
        let ink = fg("inline code", theme.text.code);
        assert_eq!(
            theme.text.code.bg, None,
            "{name}: inline code must not carry a background of its own"
        );
        at_least(
            name,
            "inline code on the page",
            ink,
            theme.palette.bg,
            TEXT_FLOOR,
        );
        at_least(
            name,
            "inline code on a striped table row",
            ink,
            theme
                .table
                .row_alt
                .bg
                .unwrap_or_else(|| panic!("{name}: the stripe needs a background")),
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

/// A highlight the reader is looking straight at must stay readable under it.
///
/// The three washes that can cover document text — the two search colours and the
/// mouse selection — replace the background wholesale, so the ink on top of them is a
/// different pairing from anything else the palette is checked for. A selection is also
/// the one of the three the reader makes *while watching it*, which is exactly when an
/// illegible wash is most annoying.
#[test]
fn text_under_a_highlight_clears_the_text_contrast_floor() {
    for theme in themes() {
        let name = &theme.name;
        for (slot, style) in [
            ("the selection wash", theme.ui.selection),
            ("an ordinary search match", theme.ui.search_match),
            ("the current search match", theme.ui.search_current),
        ] {
            let ground = style
                .bg
                .unwrap_or_else(|| panic!("{name}: {slot} needs a background"));
            at_least(name, slot, fg(slot, style), ground, TEXT_FLOOR);
        }
    }
}

/// A selection and a search hit can be on screen together and mean different things.
#[test]
fn the_selection_does_not_borrow_a_search_colour() {
    for theme in themes() {
        let name = &theme.name;
        assert_ne!(
            theme.ui.selection.bg, theme.ui.search_match.bg,
            "{name}: the selection wash is the search-match wash"
        );
        assert_ne!(
            theme.ui.selection.bg, theme.ui.search_current.bg,
            "{name}: the selection wash is the current-match wash"
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

/// Section numbers are ours, and have to look it — without becoming unreadable.
///
/// The numbers `mdmost` puts in front of a deeply nested document's headings (design
/// spec §9.3) are not in the author's text, so the owner asked for them "in a light
/// colour, to make clear the numbering is ours". That is two requirements pulling
/// against each other and both are asserted here: the digits clear the 4.5:1 text
/// floor, because they are text the reader reads; and they stay quieter than *every*
/// heading level, including the sixth, because a number as loud as the words beside it
/// has stopped announcing itself as an annotation.
///
/// Measured on the built-ins when this was written: 5.04:1 against the dark page
/// (quietest heading 5.56:1) and 4.71:1 against the light one (quietest heading
/// 4.80:1). The light theme's margin is thin because its heading ramp is nearly flat —
/// a known finding, and the reason this is a test rather than a comment.
#[test]
fn section_numbers_are_readable_but_quieter_than_every_heading() {
    for theme in themes() {
        let name = &theme.name;
        let page = theme.palette.bg;
        let number = fg("the section number", theme.heading_number);
        at_least(
            name,
            "a section number on the page",
            number,
            page,
            TEXT_FLOOR,
        );
        let numbered = contrast(number, page);
        for level in 1..=6u8 {
            let heading = contrast(fg("a heading", theme.heading(level)), page);
            assert!(
                numbered < heading,
                "{name}: the section number ({numbered:.2}:1) must stay quieter than \
                 the level-{level} heading it prefixes ({heading:.2}:1)"
            );
            // And it is a slot of its own, not the heading colour worn thin.
            assert_ne!(
                theme.heading_number.fg,
                theme.heading(level).fg,
                "{name}: the section number borrows the level-{level} heading colour"
            );
        }
    }
}

/// The `[copy]` button under the pointer, in every theme.
///
/// The hovered style is *derived* — the resting button is the frame it is drawn in, and
/// hovering blends that colour towards the theme's own ink. Two things have to hold and
/// neither implies the other. It has to stay a legible piece of chrome, so the 3:1
/// non-text floor applies to it exactly as it does to the frame it came from. And it has
/// to move *away* from the page: lighter on a dark theme, darker on a light one, which
/// is what the owner asked for and what a shift in the wrong direction would fail.
///
/// Measured at `HOVER_SHIFT = 0.6`, against the page: dark 3.27:1 at rest and 8.16:1
/// hovered, light 3.45:1 and 7.51:1. Against a code surface: 3.03 → 7.55 and
/// 3.08 → 6.70. Blending the same distance towards the *background* instead — the shift
/// inverted — measures 1.52:1 and 1.53:1, well under the floor, which is what makes this
/// test a guard on the direction rather than a restatement of the palette's own numbers.
#[test]
fn the_hovered_copy_button_stays_legible_in_every_theme() {
    for theme in themes() {
        let name = &theme.name;
        for (slot, resting, ground) in [
            (
                "the hovered code button",
                theme.code.frame,
                theme.palette.bg,
            ),
            (
                "the hovered code button on a surface",
                theme.code.frame,
                theme.palette.surface,
            ),
            (
                "the hovered table button",
                theme.table.border,
                theme.palette.bg,
            ),
            (
                "the hovered table button on a striped row",
                theme.table.border,
                theme.palette.surface,
            ),
        ] {
            let hovered = theme.hovered(resting);
            at_least(name, slot, fg(slot, hovered), ground, GRAPHIC_FLOOR);
            // Louder than the resting button, not merely different from it: a shift
            // that dimmed the control the pointer is on would read as it going away.
            let (rest, over) = (
                contrast(fg(slot, resting), ground),
                contrast(fg(slot, hovered), ground),
            );
            assert!(
                over > rest,
                "{name}: {slot} measures {over:.2}:1 against its ground, no clearer \
                 than the {rest:.2}:1 it has at rest"
            );
            // And in the direction the theme's own polarity asks for.
            let (before, after) = (fg(slot, resting).luminance(), fg(slot, hovered).luminance());
            if theme.is_dark {
                assert!(
                    after > before,
                    "{name}: {slot} must go lighter on a dark theme \
                     ({before:.3} → {after:.3})"
                );
            } else {
                assert!(
                    after < before,
                    "{name}: {slot} must go darker on a light theme \
                     ({before:.3} → {after:.3})"
                );
            }
            // Perceptibly, which a floor and a direction both allow to be one step.
            let shift = (after - before).abs();
            assert!(
                shift >= 0.05,
                "{name}: {slot} shifts by {shift:.3} in luminance, which nobody sees"
            );
        }
    }
}

/// A link under the pointer, in every theme.
///
/// Same terms as [`the_hovered_copy_button_stays_legible_in_every_theme`]: the shade is
/// derived, not chosen for this slot, so both the 3:1 non-text floor and the direction
/// of travel — lighter on a dark theme, darker on a light one — have to hold here too,
/// even though a link's resting colour (`theme.text.link`) is a different slot from the
/// button's frame and was never checked against this floor before.
///
/// Measured at `HOVER_SHIFT = 0.6`, against the page: dark 7.32:1 at rest and 10.51:1
/// hovered, light 5.83:1 and 9.67:1. Against a table's striped row: dark 6.77:1 →
/// 9.72:1 and light 5.20:1 → 8.63:1. Blending towards the *background* instead — the
/// shift inverted — measures 2.19:1 (dark) and 1.84:1 (light) against the page, both
/// far under the floor, which is what makes this test a guard on the direction rather
/// than a restatement of the palette's own numbers.
#[test]
fn the_hovered_link_stays_legible_in_every_theme() {
    for theme in themes() {
        let name = &theme.name;
        for (slot, resting, ground) in [
            ("the hovered link", theme.text.link, theme.palette.bg),
            (
                "the hovered link on a striped row",
                theme.text.link,
                theme.palette.surface,
            ),
        ] {
            let hovered = theme.hovered(resting);
            at_least(name, slot, fg(slot, hovered), ground, GRAPHIC_FLOOR);
            // Louder than the resting link, not merely different from it: a shift that
            // dimmed the control the pointer is on would read as it going away.
            let (rest, over) = (
                contrast(fg(slot, resting), ground),
                contrast(fg(slot, hovered), ground),
            );
            assert!(
                over > rest,
                "{name}: {slot} measures {over:.2}:1 against its ground, no clearer \
                 than the {rest:.2}:1 it has at rest"
            );
            // And in the direction the theme's own polarity asks for.
            let (before, after) = (fg(slot, resting).luminance(), fg(slot, hovered).luminance());
            if theme.is_dark {
                assert!(
                    after > before,
                    "{name}: {slot} must go lighter on a dark theme \
                     ({before:.3} → {after:.3})"
                );
            } else {
                assert!(
                    after < before,
                    "{name}: {slot} must go darker on a light theme \
                     ({before:.3} → {after:.3})"
                );
            }
            // Perceptibly, which a floor and a direction both allow to be one step.
            let shift = (after - before).abs();
            assert!(
                shift >= 0.05,
                "{name}: {slot} shifts by {shift:.3} in luminance, which nobody sees"
            );
        }
    }
}
