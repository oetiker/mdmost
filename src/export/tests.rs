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
