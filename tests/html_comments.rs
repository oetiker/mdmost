// SPDX-License-Identifier: MIT
//! HTML comments render as nothing at all: no `⟨html⟩` marker, no blank row.
//!
//! Any other raw HTML still leaves the marker, so a reader knows something was skipped.

use mdmost::doc::Doc;

use mdmost::render::{RenderOptions, render_document};
use mdmost::theme::Theme;

/// What skipped HTML is drawn as.
const HTML_MARKER: &str = "⟨html⟩";

/// Every row of the rendered document, right-trimmed.
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
        .map(|row| canvas.row_text(row).trim_end().to_string())
        .collect()
}

#[test]
fn a_block_comment_takes_no_room() {
    assert_eq!(
        rows("Para one.\n\n<!-- hidden -->\n\nPara two.\n"),
        rows("Para one.\n\nPara two.\n")
    );
}

#[test]
fn a_multi_line_comment_takes_no_room() {
    assert_eq!(
        rows("Para one.\n\n<!--\nTODO: rewrite\nthis part\n-->\n\nPara two.\n"),
        rows("Para one.\n\nPara two.\n")
    );
}

#[test]
fn an_inline_comment_leaves_no_trace_in_its_sentence() {
    assert_eq!(
        rows("Before <!-- note --> after.\n"),
        rows("Before after.\n")
    );
}

#[test]
fn a_comment_in_a_table_cell_leaves_no_trace() {
    let joined = rows("| a |\n|---|\n| x <!-- c --> |\n").join("\n");
    assert!(!joined.contains(HTML_MARKER), "{joined}");
    assert!(joined.contains('x'), "{joined}");
}

#[test]
fn html_that_is_not_only_a_comment_keeps_its_marker() {
    let joined = rows("<!-- note --><div>x</div>\n").join("\n");
    assert!(joined.contains(HTML_MARKER), "{joined}");
}
