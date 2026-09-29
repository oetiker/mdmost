// SPDX-License-Identifier: MIT
//! A YAML front matter block at the top of a file is not Markdown.
//!
//! It is drawn as one `[Frontmatter]` control; activating it opens the YAML in a popup
//! (see the pager's own tests). Here: what the renderer and the parser make of it.

use mdmost::canvas::HotspotKind;
use mdmost::doc::{Doc, NodeKind};
use mdmost::render::{RenderOptions, render_document};
use mdmost::theme::Theme;

const SOURCE: &str = "---\ntitle: Hello\ntags: [a, b]\n---\n\n# Heading\n\nText.\n";

/// The non-blank rows of the rendered document, trimmed.
fn rows(source: &str) -> Vec<String> {
    let doc = Doc::parse(source);
    let canvas = render_document(
        &doc,
        60,
        None,
        &Theme::default_dark(),
        &RenderOptions::new(false, false),
    );
    (0..canvas.height())
        .map(|row| canvas.row_text(row).trim().to_string())
        .filter(|row| !row.is_empty())
        .collect()
}

#[test]
fn front_matter_draws_as_one_control_and_not_as_yaml() {
    let rows = rows(SOURCE);
    assert_eq!(rows[0], "[Frontmatter]", "{rows:?}");
    assert_eq!(rows[1], "Heading", "{rows:?}");
    let joined = rows.join("\n");
    assert!(!joined.contains("title"), "{joined}");
}

#[test]
fn the_control_is_a_hotspot() {
    let doc = Doc::parse(SOURCE);
    let canvas = render_document(
        &doc,
        60,
        None,
        &Theme::default_dark(),
        &RenderOptions::new(false, false),
    );
    let spot = canvas
        .hotspots()
        .iter()
        .find(|spot| spot.kind == HotspotKind::FrontMatter)
        .expect("a front matter hotspot");
    let text = canvas.row_text(spot.row);
    let start = text
        .char_indices()
        .nth(usize::from(spot.col))
        .map_or(text.len(), |(at, _)| at);
    assert!(
        text[start..].starts_with("[Frontmatter]"),
        "the hotspot sits on the label: {text:?}"
    );
    assert_eq!(spot.cols, 13);
}

#[test]
fn front_matter_is_parsed_into_its_own_node() {
    let doc = Doc::parse(SOURCE);
    let first = &doc.root().children[0];
    assert_eq!(
        first.kind,
        NodeKind::FrontMatter {
            yaml: "title: Hello\ntags: [a, b]\n".to_string()
        }
    );
}

#[test]
fn front_matter_is_not_a_heading() {
    let doc = Doc::parse(SOURCE);
    let titles: Vec<&str> = doc.headings().iter().map(|h| h.text.as_str()).collect();
    assert_eq!(titles, ["Heading"]);
}

#[test]
fn offsets_after_front_matter_still_point_at_their_source() {
    let doc = Doc::parse(SOURCE);
    let heading = &doc.headings()[0];
    assert_eq!(
        &SOURCE[heading.source.start..heading.source.end],
        "# Heading"
    );
}

#[test]
fn a_rule_later_in_the_document_is_still_a_rule() {
    let doc = Doc::parse("Intro.\n\n---\n\nkey: value\n---\n");
    let mut front = 0;
    doc.root().walk(&mut |node| {
        if matches!(node.kind, NodeKind::FrontMatter { .. }) {
            front += 1;
        }
    });
    assert_eq!(front, 0, "front matter only counts at the very start");
    assert!(
        doc.root()
            .children
            .iter()
            .any(|node| node.kind == NodeKind::ThematicBreak)
    );
}

#[test]
fn the_front_matter_span_ends_with_its_closing_line() {
    // comrak's block runs on over the blank lines after the closing `---`; the node's
    // span stops at the end of that line, newline included, so a copy of the block is
    // the block and nothing after it.
    let doc = Doc::parse(SOURCE);
    let span = doc.root().children[0].source;
    assert_eq!(
        &SOURCE[span.start..span.end],
        "---\ntitle: Hello\ntags: [a, b]\n---\n"
    );
}

#[test]
fn the_control_is_an_atom_holding_the_whole_block() {
    let doc = Doc::parse(SOURCE);
    let canvas = render_document(
        &doc,
        60,
        None,
        &Theme::default_dark(),
        &RenderOptions::new(false, false),
    );
    let spot = canvas
        .hotspots()
        .iter()
        .find(|spot| spot.kind == HotspotKind::FrontMatter)
        .expect("a front matter hotspot");
    let atom = canvas.atoms().first().expect("a front matter atom");
    assert_eq!((atom.row, atom.rows), (spot.row, 1));
    assert_eq!((atom.col, atom.cols), (spot.col, spot.cols));
    assert_eq!(
        &SOURCE[atom.source_start..atom.source_end],
        "---\ntitle: Hello\ntags: [a, b]\n---\n"
    );
    assert_eq!(atom.content, "title: Hello\ntags: [a, b]\n");
}
