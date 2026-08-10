//! Unit tests for the clipboard exporters.

use super::*;
use crate::doc::{Doc, Node, NodeKind};

/// Runs `f` on the first table in `markdown`.
///
/// A closure rather than a returned `Node`, so the helper does not depend on `Node`
/// being `Clone` and the borrow of the parsed document stays alive for the assertion.
fn with_table<T>(markdown: &str, f: impl FnOnce(&Node) -> T) -> T {
    fn find(node: &Node) -> Option<&Node> {
        if matches!(node.kind, NodeKind::Table(_)) {
            return Some(node);
        }
        node.children.iter().find_map(find)
    }
    let doc = Doc::parse(markdown);
    f(find(doc.root()).expect("a table"))
}

/// The TSV of the first table in `markdown`.
fn tsv_of(markdown: &str) -> String {
    with_table(markdown, table_tsv)
}

#[test]
fn a_table_becomes_a_tab_separated_grid() {
    let grid = tsv_of("| a | b |\n| --- | --- |\n| 1 | 2 |\n");
    assert_eq!(grid, "a\tb\n1\t2\n");
}

#[test]
fn cell_markup_is_flattened_to_its_text() {
    let grid = tsv_of("| a |\n| --- |\n| **bold** `code` |\n");
    assert_eq!(grid, "a\nbold code\n");
}

#[test]
fn a_tab_inside_a_cell_cannot_break_the_grid() {
    // A literal tab in a cell would otherwise add a column to that row alone.
    let grid = tsv_of("| a | b |\n| --- | --- |\n| x\ty | z |\n");
    assert_eq!(
        grid.lines().nth(1).unwrap().split('\t').count(),
        2,
        "every row has the same number of columns: {grid:?}"
    );
}

#[test]
fn a_line_break_inside_a_cell_cannot_break_the_grid() {
    let grid = tsv_of("| a |\n| --- |\n| x<br>y |\n");
    assert_eq!(grid.lines().count(), 2, "two rows, not three: {grid:?}");
}

#[test]
fn an_empty_cell_keeps_its_column() {
    let grid = tsv_of("| a | b |\n| --- | --- |\n|  | 2 |\n");
    assert_eq!(grid, "a\tb\n\t2\n");
}

/// The HTML of the first table in `markdown`.
fn html_of(markdown: &str) -> String {
    with_table(markdown, table_html)
}

#[test]
fn a_table_becomes_an_html_table() {
    let html = html_of("| a | b |\n| --- | --- |\n| 1 | 2 |\n");
    assert!(html.starts_with("<table>"), "got {html}");
    assert!(html.contains("<th>a</th>"), "the header row is th: {html}");
    assert!(html.contains("<td>1</td>"), "got {html}");
}

#[test]
fn inline_markup_becomes_inline_html() {
    let html = html_of("| a |\n| --- |\n| **b** *i* ~~s~~ `c` |\n");
    assert!(html.contains("<strong>b</strong>"), "got {html}");
    assert!(html.contains("<em>i</em>"), "got {html}");
    assert!(html.contains("<del>s</del>"), "got {html}");
    assert!(html.contains("<code>c</code>"), "got {html}");
}

#[test]
fn declared_alignment_reaches_the_cells() {
    let html = html_of("| a |\n| ---: |\n| 1 |\n");
    assert!(html.contains(r#"align="right""#), "got {html}");
}

#[test]
fn text_is_escaped() {
    // A `<` the parser did not read as a tag stays in the text, and must not be able to
    // open one in the payload.
    let html = html_of("| a |\n| --- |\n| 1 < 2 & \"q\" |\n");
    assert!(html.contains("1 &lt; 2"), "got {html}");
    assert!(html.contains("&amp;"), "got {html}");
    assert!(html.contains("&quot;q&quot;"), "got {html}");
}

#[test]
fn raw_html_in_a_cell_reaches_the_clipboard_as_nothing() {
    // The parser drops raw HTML into `SkippedHtml`, so the reader never saw the tag.
    // Escaping it back in would put text in the clipboard that was never on the screen.
    let html = html_of("| a |\n| --- |\n| <script>x</script> |\n");
    assert!(!html.contains("<script"), "no live tag: {html}");
    assert!(!html.contains("script"), "not even escaped: {html}");
    assert!(html.contains("<td>x</td>"), "the text survives: {html}");
}

#[test]
fn an_http_link_keeps_its_href() {
    let html = html_of("| a |\n| --- |\n| [t](https://example.com/x?a=1&b=2) |\n");
    assert!(
        html.contains(r#"<a href="https://example.com/x?a=1&amp;b=2">t</a>"#),
        "got {html}"
    );
}

#[test]
fn a_javascript_link_loses_its_href_and_keeps_its_text() {
    let html = html_of("| a |\n| --- |\n| [click](javascript:alert(1)) |\n");
    assert!(!html.contains("javascript"), "got {html}");
    assert!(!html.contains("<a "), "no anchor at all: {html}");
    assert!(html.contains("click"), "the text survives: {html}");
}

#[test]
fn a_quote_inside_a_url_cannot_escape_the_attribute() {
    let html = html_of("| a |\n| --- |\n| [t](https://e.com/\"onx=1) |\n");
    assert!(!html.contains(r#""onx"#), "got {html}");
}

#[test]
fn a_line_break_in_a_cell_becomes_br() {
    let html = html_of("| a |\n| --- |\n| x<br>y |\n");
    assert!(html.contains("<br>"), "got {html}");
}
