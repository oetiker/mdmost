//! Property tests for the renderers (design spec §13.3).
//!
//! Three properties are asserted for every input at every width:
//!
//! 1. rendering never panics;
//! 2. the canvas is a rectangle at least `width` columns wide — every row exactly as
//!    wide as the canvas. It is not exactly `width` because a block that cannot reflow
//!    (a table with more columns than fit, a long code line) is laid out at the width it
//!    needs, and that surplus is what the horizontal scroll keys reach. The rectangle is
//!    the load-bearing half: the viewport blits slices and scrolls every row of a run by
//!    the same amount, so a short row would tear;
//! 3. rendering is deterministic — the same `(document, width, theme, options)`
//!    always produces the same canvas, which is what makes the render cache safe to
//!    drop.
//!
//! Every case runs under both [`RenderOptions`] settings, because the glyph set and
//! the code gutter both change what is drawn and neither may break the width rule.

use mdmost::doc::Doc;
use mdmost::render::{RenderOptions, render_document};
use mdmost::text::display_width;
use mdmost::theme::Theme;

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

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
        let canvas = render_document(&doc, width, None, &theme, &options);

        assert!(canvas.width() >= width);
        canvas.check_invariants().unwrap_or_else(|problem| {
            panic!("canvas contract violated at width {width} with {options:?}: {problem}")
        });
        let canvas_width = usize::from(canvas.width());
        for row in 0..canvas.height() {
            assert_eq!(
                display_width(&canvas.row_text(row)),
                canvas_width,
                "row {row} at width {width} with {options:?} is not exactly {canvas_width} columns"
            );
        }

        let again = render_document(&doc, width, None, &theme, &options);
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
    // Design spec §13.1 names five hard categories; the class must contain all of
    // them or a "passing" property test proves nothing about any of them. In order:
    // ASCII, Latin-1, CJK (double width), emoji, ZWJ (emoji joiner), a combining mark,
    // a zero-width space, Hebrew and Arabic (RTL), and a Tangut base plus a *spacing*
    // Tai Tham mark — the pair that draws three columns and broke the canvas contract.
    let text = "[ -~\u{00e9}\u{4e2d}\u{1f600}\u{200d}\u{0301}\u{200b}\u{05d0}\u{0627}\u{17000}\u{1a57}]{0,40}";
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
    // Persistence is pinned to an explicit path. Proptest cannot resolve a source
    // root for a test under `tests/`, so with the default the failing seed is silently
    // not written and the gate goes green or red on the luck of the seed.
    #![proptest_config(ProptestConfig {
        cases: 96,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/render_property.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    })]

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
        let dark = render_document(&doc, width, None, &Theme::default_dark(), &options);
        let light = render_document(&doc, width, None, &Theme::default_light(), &options);
        assert_eq!(
            dark.plain_text(),
            light.plain_text(),
            "layout must not depend on the palette"
        );
    }
}

/// Turning icons off must not reflow the document, at any width.
///
/// This briefly had an exception. The Nerd Font checkboxes were *drawn* across two
/// cells — twice the advance of an ASCII character — while `unicode-width` reported one
/// for their private-use code points, so a task list's marker field differed between
/// the sets and this sweep had to skip task items to stay green. Replacing the boxes
/// with `[ ]`/`[x]` removed the discrepancy, and the sweep is back on the whole corpus.
#[test]
fn turning_icons_off_never_changes_the_layout() {
    let markdown = include_str!("corpus/adversarial.md");
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    for width in 4..=120u16 {
        for line_numbers in [false, true] {
            let fancy = render_document(
                &doc,
                width,
                None,
                &theme,
                &RenderOptions::new(true, line_numbers),
            );
            let plain = render_document(
                &doc,
                width,
                None,
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
