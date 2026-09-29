// SPDX-License-Identifier: MIT
//! A YAML front matter block at the top of a file is not Markdown.
//!
//! It is drawn as an italic `Frontmatter` label, the YAML as a `yaml` code block, and a
//! rule: the three blocks a writer would use to show it. Here: what the renderer and
//! the parser make of it.

use mdmost::canvas::{Canvas, HotspotKind};
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

/// The same document with its front matter written out as the three blocks it is
/// drawn as.
const EQUIVALENT: &str =
    "*Frontmatter*\n\n```yaml\ntitle: Hello\ntags: [a, b]\n```\n\n---\n\n# Heading\n\nText.\n";

/// The document rendered at 60 columns with `options`.
fn canvas_with(source: &str, options: &RenderOptions) -> Canvas {
    render_document(
        &Doc::parse(source),
        60,
        None,
        &Theme::default_dark(),
        options,
    )
}

/// Every row of `canvas` as `(text, styles)`.
fn cells(canvas: &Canvas) -> Vec<(String, Vec<mdmost::theme::Style>)> {
    canvas
        .rows()
        .iter()
        .map(|row| {
            (
                row.iter().map(|cell| cell.text()).collect(),
                row.iter().map(|cell| cell.style()).collect(),
            )
        })
        .collect()
}

#[test]
fn front_matter_draws_as_a_label_a_yaml_block_and_a_rule() {
    // Cell for cell, text and style, what the document would draw if the front matter
    // were written as `*Frontmatter*`, a `yaml` fence and a `---` rule.
    let options = RenderOptions::new(false, false);
    assert_eq!(
        cells(&canvas_with(SOURCE, &options)),
        cells(&canvas_with(EQUIVALENT, &options))
    );
}

#[test]
fn the_label_is_italic_and_the_yaml_is_framed() {
    let rows = rows(SOURCE);
    assert_eq!(rows[0], "Frontmatter", "{rows:?}");
    assert!(rows[1].contains("yaml"), "the frame's title: {rows:?}");
    assert!(rows[2].contains("title: Hello"), "{rows:?}");
    assert!(rows[3].contains("tags: [a, b]"), "{rows:?}");

    let canvas = canvas_with(SOURCE, &RenderOptions::new(false, false));
    let label = canvas
        .rows()
        .iter()
        .find(|row| row.iter().any(|cell| cell.text() == "F"))
        .expect("the label row");
    let emphasis = Theme::default_dark().text.emphasis.attrs;
    for cell in label.iter().filter(|cell| !cell.text().trim().is_empty()) {
        assert!(
            cell.style().attrs.contains(emphasis),
            "{:?} is drawn in the emphasis style",
            cell.text()
        );
    }
}

#[test]
fn the_delimiters_are_never_drawn_and_one_rule_follows_the_yaml() {
    let rule = rows("a\n\n---\n")[1].clone();
    let rows = rows(SOURCE);
    let heading = rows
        .iter()
        .position(|row| row == "Heading")
        .expect("the heading");
    let rules = rows[..heading].iter().filter(|row| **row == rule).count();
    assert_eq!(rules, 1, "{rows:?}");
    assert!(
        !rows.iter().any(|row| row == "---"),
        "no delimiter line: {rows:?}"
    );
    let closing = rows[..heading]
        .iter()
        .rposition(|row| row.starts_with('\u{2570}'))
        .expect("the frame's bottom edge");
    assert_eq!(
        rows[closing + 1],
        rule,
        "the rule follows the YAML: {rows:?}"
    );
}

#[test]
fn the_yaml_block_copies_the_yaml_with_its_button() {
    let options = RenderOptions::new(false, false).with_copy_button(true);
    let canvas = canvas_with(SOURCE, &options);
    let copies: Vec<_> = canvas
        .hotspots()
        .iter()
        .filter_map(|spot| match &spot.kind {
            HotspotKind::Copy { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(copies, ["title: Hello\ntags: [a, b]\n"]);
}

#[test]
fn the_yaml_rows_point_at_their_source_lines() {
    let canvas = canvas_with(SOURCE, &RenderOptions::new(false, false));
    let copied: Vec<&str> = canvas
        .spans()
        .iter()
        .map(|span| &SOURCE[span.source_start..span.source_end])
        .filter(|text| text.contains(':'))
        .collect();
    assert_eq!(copied, ["title: Hello", "tags: [a, b]"]);
}

#[test]
fn a_document_without_front_matter_draws_as_before() {
    // The blocks after the front matter are drawn exactly as they are without it.
    let options = RenderOptions::new(false, false);
    let with = cells(&canvas_with(SOURCE, &options));
    let without = cells(&canvas_with("# Heading\n\nText.\n", &options));
    assert_eq!(with[with.len() - without.len()..], without[..]);
}

#[test]
fn front_matter_is_parsed_into_its_own_node() {
    let doc = Doc::parse(SOURCE);
    let first = &doc.root().children[0];
    let NodeKind::FrontMatter { yaml, lines } = &first.kind else {
        panic!("not front matter: {:?}", first.kind);
    };
    assert_eq!(yaml, "title: Hello\ntags: [a, b]\n");
    let copied: Vec<&str> = lines
        .iter()
        .map(|span| &SOURCE[span.start..span.end])
        .collect();
    assert_eq!(copied, ["title: Hello", "tags: [a, b]"]);
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
