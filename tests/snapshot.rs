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
//! Once `render::block` exists, `render_at` below is the only function that has to
//! change: replace the placeholder rendering with the real document renderer and the
//! snapshots become the real golden files.

use std::path::Path;

use mdless::canvas::{BorderSet, Canvas};
use mdless::doc::{Doc, NodeKind};
use mdless::text::{Align, Line, Span, wrap_spans};
use mdless::theme::Theme;

/// The widths every fixture is rendered at.
const WIDTHS: [u16; 3] = [40, 80, 120];

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

/// Renders a document at `width`.
///
/// **Placeholder.** The block renderer does not exist yet, so this shows the document
/// outline in a framed box built from the foundation layer. It nevertheless exercises
/// the real contract: the output is a [`Canvas`] whose every row is exactly `width`
/// columns wide.
fn render_at(doc: &Doc, width: u16, theme: &Theme) -> Canvas {
    let inner_width = usize::from(width.saturating_sub(2));
    let mut body = Canvas::empty(width.saturating_sub(2));
    for heading in doc.headings() {
        let bullet = "#".repeat(usize::from(heading.level));
        let spans = vec![
            Span::new(format!("{bullet} "), theme.block.heading_prefix),
            Span::new(heading.text.clone(), theme.heading(heading.level)),
        ];
        for line in wrap_spans(&spans, inner_width) {
            body.push_line(&line, Align::Left, theme.base());
        }
    }
    let title = Line::styled("outline", theme.code.language);
    let framed = body.framed(
        BorderSet::ROUNDED,
        theme.block.image_border,
        Some(&title),
        theme.base(),
    );
    assert_eq!(framed.width(), width);
    framed
        .check_invariants()
        .unwrap_or_else(|problem| panic!("canvas invariant violated at width {width}: {problem}"));
    framed
}

#[test]
fn corpus_outlines_are_stable() {
    for name in ["adversarial.md"] {
        let doc = Doc::parse(&fixture(name));
        insta::assert_snapshot!(format!("{name}@outline"), outline(&doc));
    }
}

#[test]
fn corpus_renders_are_stable_at_every_width() {
    let theme = Theme::default_dark();
    for name in ["adversarial.md"] {
        let doc = Doc::parse(&fixture(name));
        for width in WIDTHS {
            let canvas = render_at(&doc, width, &theme);
            insta::assert_snapshot!(format!("{name}@{width}"), canvas.plain_text());
        }
    }
}

#[test]
fn rendering_is_idempotent_and_width_exact() {
    let theme = Theme::default_dark();
    let doc = Doc::parse(&fixture("adversarial.md"));
    for width in WIDTHS {
        let first = render_at(&doc, width, &theme);
        let second = render_at(&doc, width, &theme);
        pretty_assertions::assert_eq!(first.plain_text(), second.plain_text());
        for row in 0..first.height() {
            assert_eq!(
                mdless::text::display_width(&first.row_text(row)),
                usize::from(width),
                "row {row} at width {width} is not exactly {width} columns"
            );
        }
    }
}
