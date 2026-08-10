//! Edge-label alignment, measured on rendered output rather than on the layout.
//!
//! `mermaid_layout_flowchart.rs` pins the same invariant by handing the layout engine a
//! pre-split [`Label`](mdmost::mermaid::ast::Label). That skips the parser and the block
//! renderer, so it cannot see a defect introduced anywhere else in the pipeline. These
//! cases go through [`render_document`], which is the entry point `--render-once` and
//! the pager both call, and read the columns back off the canvas the reader sees.
//!
//! Columns are counted in *characters*, never bytes: a box-drawing glyph is three bytes
//! and one column, so a byte offset runs two ahead of the column for every glyph to its
//! left. A row of box art before a label therefore reports a large false indent, which
//! is exactly the shape of a bug that is not there.

use mdmost::config::DEFAULT_BODY_WIDTH;
use mdmost::doc::Doc;
use mdmost::render::{RenderOptions, render_document};
use mdmost::theme::Theme;

/// The words a label is built from: distinct, and none a substring of another.
const WORDS: [&str; 8] = [
    "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
];

/// Renders a Mermaid source block the way a document containing it would be rendered.
fn render(mermaid: &str, width: u16) -> String {
    let doc = Doc::parse(&format!("```mermaid\n{mermaid}\n```\n"));
    let theme = Theme::default_dark();
    let options = RenderOptions::new(false, false);
    let canvas = render_document(&doc, width, Some(DEFAULT_BODY_WIDTH), &theme, &options);
    canvas.check_invariants().expect("canvas contract holds");
    canvas.plain_text()
}

/// The start column of the label on every row that carries any of [`WORDS`].
///
/// A row's label starts at the leftmost label word on it, whatever the wrap did with
/// the rest, so this reads a block's left edge without knowing where the lines break.
fn label_rows(text: &str) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();
    for (row, line) in text.lines().enumerate() {
        let start = WORDS
            .iter()
            .filter_map(|word| line.find(word))
            .map(|at| line[..at].chars().count())
            .min();
        if let Some(start) = start {
            rows.push((row, start));
        }
    }
    rows
}

/// Asserts every row of the label block starts in the same column.
///
/// The comparison is between the rows, never against a fixed number, so a relayout that
/// moves the whole diagram cannot make this pass by accident.
#[track_caller]
fn assert_flush(mermaid: &str, width: u16, want_rows: usize) {
    let text = render(mermaid, width);
    let rows = label_rows(&text);
    assert!(
        rows.len() >= want_rows,
        "expected the label to occupy at least {want_rows} rows at width {width}, \
         found {}:\n{text}",
        rows.len()
    );
    let (first_row, first_col) = rows[0];
    for &(row, col) in &rows[1..] {
        assert_eq!(
            col, first_col,
            "row {row} of the label starts at column {col} but row {first_row} starts \
             at column {first_col}, at width {width}:\n{text}"
        );
    }
}

#[test]
fn an_explicitly_broken_edge_label_is_flush_left_on_every_row() {
    let src = "flowchart LR\n    A -->|\"alpha<br>bravo<br>charlie\"| B\n";
    for width in [60, 80, 90, 100, 120] {
        assert_flush(src, width, 3);
    }
}

#[test]
fn a_wrapped_edge_label_is_flush_left_on_every_row() {
    // Wrapping is a different path from an explicit `<br>`: the layout chooses the
    // breaks, so the rows are not known in advance and only their left edge is checked.
    let src = "flowchart LR\n    A -->|\"alpha bravo charlie delta echo foxtrot golf hotel\"| B\n";
    for width in [60, 80, 90, 100, 120] {
        assert_flush(src, width, 2);
    }
}

#[test]
fn a_broken_edge_label_is_flush_left_going_top_to_bottom() {
    let src = "flowchart TD\n    A -->|\"alpha<br>bravo<br>charlie\"| B\n";
    for width in [60, 80, 90, 100, 120] {
        assert_flush(src, width, 3);
    }
}
