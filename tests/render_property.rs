//! Property tests for the renderers (design spec §13.3).
//!
//! Three properties are asserted for every input at every width:
//!
//! 1. rendering never panics;
//! 2. every rendered line is exactly `width` display columns;
//! 3. rendering is deterministic — the same `(document, width, theme, options)`
//!    always produces the same canvas, which is what makes the render cache safe to
//!    drop.
//!
//! Every case runs under both [`RenderOptions`] settings, because the glyph set and
//! the code gutter both change what is drawn and neither may break the width rule.

use mdless::doc::Doc;
use mdless::render::{RenderOptions, render_document};
use mdless::text::display_width;
use mdless::theme::Theme;

use proptest::prelude::*;

/// Every combination of the render options.
const OPTION_SETS: [RenderOptions; 4] = [
    RenderOptions::new(true, false),
    RenderOptions::new(false, false),
    RenderOptions::new(true, true),
    RenderOptions::new(false, true),
];

/// Renders `markdown` at `width` under every option set, asserting the properties.
fn check(markdown: &str, width: u16) {
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    for options in OPTION_SETS {
        let canvas = render_document(&doc, width, &theme, &options);

        assert_eq!(canvas.width(), width);
        canvas.check_invariants().unwrap_or_else(|problem| {
            panic!("canvas contract violated at width {width} with {options:?}: {problem}")
        });
        for row in 0..canvas.height() {
            assert_eq!(
                display_width(&canvas.row_text(row)),
                usize::from(width),
                "row {row} at width {width} with {options:?} is not exactly {width} columns"
            );
        }

        let again = render_document(&doc, width, &theme, &options);
        assert_eq!(
            canvas, again,
            "rendering is not deterministic at {width} with {options:?}"
        );
    }
}

/// The building blocks a generated document is assembled from.
///
/// Random bytes alone rarely produce a table or a nested list; a fragment grammar
/// reaches the interesting layout paths far more often.
fn fragment() -> impl Strategy<Value = String> {
    let text = "[ -~\u{00e9}\u{4e2d}\u{1f600}]{0,40}";
    prop_oneof![
        text.prop_map(|t| t),
        text.prop_map(|t| format!("# {t}")),
        text.prop_map(|t| format!("###### {t}")),
        text.prop_map(|t| format!("- {t}\n  - {t}\n    1. {t}")),
        text.prop_map(|t| format!("- [x] {t}\n- [ ] {t}")),
        text.prop_map(|t| format!("> {t}\n>\n> > {t}")),
        text.prop_map(|t| format!("| a | b |\n|:--|--:|\n| {t} | {t} |")),
        text.prop_map(|t| format!("| {t} |\n|---|\n| `{t}` |")),
        text.prop_map(|t| format!("```rust\n{t}\n```")),
        text.prop_map(|t| format!("```mermaid\n{t}\n```")),
        text.prop_map(|t| format!("![{t}](p.png)")),
        text.prop_map(|t| format!("*{t}* **{t}** ~~{t}~~ `{t}` [{t}](u)")),
        text.prop_map(|t| format!("<div>{t}</div>")),
        text.prop_map(|t| format!("{t}[^a]\n\n[^a]: {t}")),
        Just("---".to_string()),
    ]
}

/// A whole generated document.
fn document() -> impl Strategy<Value = String> {
    proptest::collection::vec(fragment(), 0..6).prop_map(|parts| parts.join("\n\n"))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    /// Generated Markdown renders cleanly at any terminal width.
    #[test]
    fn generated_documents_render_at_any_width(markdown in document(), width in 1u16..=120) {
        check(&markdown, width);
    }

    /// Arbitrary bytes are still a valid `CommonMark` document (design spec §12).
    #[test]
    fn arbitrary_text_renders_cleanly(markdown in ".{0,400}", width in 1u16..=120) {
        check(&markdown, width);
    }
}

#[test]
fn the_adversarial_corpus_renders_at_every_width_from_one_to_a_hundred_and_twenty() {
    let markdown = include_str!("corpus/adversarial.md");
    for width in 1..=120u16 {
        check(markdown, width);
    }
}

#[test]
fn both_built_in_themes_produce_identically_shaped_output() {
    let markdown = include_str!("corpus/adversarial.md");
    let doc = Doc::parse(markdown);
    for width in [40u16, 80, 120] {
        let options = RenderOptions::default();
        let dark = render_document(&doc, width, &Theme::default_dark(), &options);
        let light = render_document(&doc, width, &Theme::default_light(), &options);
        assert_eq!(
            dark.plain_text(),
            light.plain_text(),
            "layout must not depend on the palette"
        );
    }
}

#[test]
fn turning_icons_off_never_changes_the_layout() {
    let markdown = include_str!("corpus/adversarial.md");
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    for width in 4..=120u16 {
        for line_numbers in [false, true] {
            let fancy =
                render_document(&doc, width, &theme, &RenderOptions::new(true, line_numbers));
            let plain = render_document(
                &doc,
                width,
                &theme,
                &RenderOptions::new(false, line_numbers),
            );
            assert_eq!(
                fancy.height(),
                plain.height(),
                "row count differs at width {width} (line_numbers={line_numbers})"
            );
            assert_eq!(
                fancy.anchors(),
                plain.anchors(),
                "anchors differ at {width}"
            );
            assert_eq!(
                fancy.spans(),
                plain.spans(),
                "search spans differ at {width}"
            );
        }
    }
}
