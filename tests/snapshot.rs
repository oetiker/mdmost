//! Golden snapshot tests over the adversarial corpus.
//!
//! This file establishes the pattern the renderer workstreams extend:
//!
//! * fixtures live in `tests/corpus/*.md`;
//! * every fixture is checked at widths 40, 80 and 120 (design spec §13.2);
//! * snapshots are named `<fixture>@<width>` so a failure names the case exactly;
//! * accepted snapshots live in `tests/snapshots/` and are reviewed with
//!   `cargo insta review` (or accepted wholesale with `INSTA_UPDATE=always cargo test`).
//!
//! `render_at` calls the real document renderer, so the accepted snapshots are golden
//! *layout*: a change to margins, table sizing, quote gutters or code padding shows up
//! here as a reviewable diff over the whole adversarial corpus (design spec §13.2) —
//! nested tables, Markdown inside cells, deep lists and mixed scripts included.

use std::path::Path;

use mdless::canvas::Canvas;
use mdless::doc::{Doc, NodeKind};
use mdless::render::{RenderOptions, render_document};
use mdless::theme::Theme;

/// The options the snapshots are taken under.
///
/// Plain glyphs, because a Nerd Font code point in a golden file is unreadable and
/// `render_property` already proves the two sets share a layout.
const OPTIONS: RenderOptions = RenderOptions::new(false, false);

/// The widths every fixture is rendered at.
const WIDTHS: [u16; 3] = [40, 80, 120];

/// The feature exerciser: one fixture per feature family, not one giant document.
///
/// `adversarial.md` proves the renderer survives hostile input. These fixtures prove it
/// renders the *documented* feature surface — README and design spec — correctly, and
/// they are split by family on the same reasoning as spec §13.2's refusal of a 1000-node
/// golden: a diff nobody can read gets rubber-stamped. A failure in `tables.md@80` names
/// the subsystem before anyone opens the file, and an unrelated change touches one
/// snapshot rather than a six-hundred-line one.
///
/// They are also meant to be paged through by a human — `mdless tests/corpus/lists.md` —
/// which is the other reason they are separate: one screenful per concern.
const EXERCISER: [&str; 8] = [
    "headings_text.md",
    "lists.md",
    "tables.md",
    "code.md",
    "diagrams.md",
    "unicode.md",
    "title-only.md",
    "minimal.md",
];

/// Every fixture under `tests/corpus`, exerciser and adversarial alike.
fn all_fixtures() -> Vec<&'static str> {
    let mut names = vec!["adversarial.md"];
    names.extend(EXERCISER);
    names.push("empty.md");
    names
}

/// Reads a fixture from `tests/corpus`.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// A structural dump of the parsed document: one line per node, indented by depth.
///
/// This snapshots the parse stage independently of any rendering, so a change in
/// layout cannot mask a change in parsing.
fn outline(doc: &Doc) -> String {
    fn walk(node: &mdless::doc::Node, depth: usize, out: &mut String) {
        let label = match &node.kind {
            NodeKind::Heading { level, id } => format!("Heading(h{level}, #{id})"),
            NodeKind::CodeBlock { language, .. } => {
                format!("CodeBlock({})", language.as_deref().unwrap_or("-"))
            }
            NodeKind::Table(info) => format!("Table({} columns)", info.columns),
            NodeKind::TableRow { header } => format!("TableRow(header={header})"),
            NodeKind::List(info) => format!("List(ordered={})", info.ordered),
            NodeKind::TaskItem { checked } => format!("TaskItem(checked={checked})"),
            NodeKind::Link { url, .. } => format!("Link({url})"),
            NodeKind::Image { url, .. } => format!("Image({url})"),
            NodeKind::SkippedHtml { block, .. } => format!("SkippedHtml(block={block})"),
            NodeKind::Text(text) => format!("Text({:?})", elide(text)),
            other => format!("{other:?}")
                .split(['(', ' '])
                .next()
                .unwrap_or("?")
                .to_string(),
        };
        out.push_str(&"  ".repeat(depth));
        out.push_str(&label);
        out.push('\n');
        for child in &node.children {
            walk(child, depth + 1, out);
        }
    }
    let mut out = String::new();
    walk(doc.root(), 0, &mut out);
    out
}

/// Shortens long literals so the snapshot stays readable.
fn elide(text: &str) -> String {
    let trimmed = mdless::text::truncate_to_width(text, 40);
    if trimmed.len() == text.len() {
        text.to_string()
    } else {
        format!("{trimmed}…")
    }
}

/// Renders a document at `width` with the real renderer, checking the contract.
fn render_at(doc: &Doc, width: u16, theme: &Theme) -> Canvas {
    let canvas = render_document(doc, width, theme, &OPTIONS);
    assert_eq!(canvas.width(), width);
    canvas
        .check_invariants()
        .unwrap_or_else(|problem| panic!("canvas invariant violated at width {width}: {problem}"));
    canvas
}

#[test]
fn corpus_outlines_are_stable() {
    for name in all_fixtures() {
        let doc = Doc::parse(&fixture(name));
        insta::assert_snapshot!(format!("{name}@outline"), outline(&doc));
    }
}

#[test]
fn corpus_renders_are_stable_at_every_width() {
    let theme = Theme::default_dark();
    for name in all_fixtures() {
        let doc = Doc::parse(&fixture(name));
        for width in WIDTHS {
            let canvas = render_at(&doc, width, &theme);
            insta::assert_snapshot!(format!("{name}@{width}"), canvas.plain_text());
        }
    }
}

/// The line-number gutter has no golden anywhere else, and it is the one render option
/// that changes layout rather than glyphs, so `code.md` is pinned with it on as well.
#[test]
fn the_line_number_gutter_is_stable() {
    const NUMBERED: RenderOptions = RenderOptions::new(false, true);
    let theme = Theme::default_dark();
    let doc = Doc::parse(&fixture("code.md"));
    for width in WIDTHS {
        let canvas = render_document(&doc, width, &theme, &NUMBERED);
        canvas.check_invariants().unwrap_or_else(|problem| {
            panic!("canvas invariant violated at width {width}: {problem}")
        });
        insta::assert_snapshot!(format!("code.md@numbered@{width}"), canvas.plain_text());
    }
}

/// The light theme has to lay out identically to the dark one — only colours differ.
///
/// This is the cheap guard on that: same fixtures, same widths, one assertion, no
/// second set of goldens to review.
#[test]
fn the_theme_never_changes_the_layout() {
    let dark = Theme::default_dark();
    let light = Theme::default_light();
    for name in all_fixtures() {
        let doc = Doc::parse(&fixture(name));
        for width in WIDTHS {
            let in_dark = render_at(&doc, width, &dark);
            let in_light = render_at(&doc, width, &light);
            pretty_assertions::assert_eq!(
                in_dark.plain_text(),
                in_light.plain_text(),
                "{name} at width {width} lays out differently in the light theme"
            );
        }
    }
}

#[test]
fn rendering_is_idempotent_and_width_exact() {
    let theme = Theme::default_dark();
    for name in all_fixtures() {
        let doc = Doc::parse(&fixture(name));
        for width in WIDTHS {
            let first = render_at(&doc, width, &theme);
            let second = render_at(&doc, width, &theme);
            pretty_assertions::assert_eq!(first.plain_text(), second.plain_text());
            for row in 0..first.height() {
                assert_eq!(
                    mdless::text::display_width(&first.row_text(row)),
                    usize::from(width),
                    "{name}: row {row} at width {width} is not exactly {width} columns"
                );
            }
        }
    }
}
