// SPDX-License-Identifier: MIT
//! The tab-separated grid: the payload every reader receives.

use crate::doc::{Node, NodeKind};

/// A table as tab-separated rows, with a trailing newline.
///
/// Each cell is flattened to a single line. A tab or a newline *inside* a cell becomes a
/// space, which is a deliberate choice over Excel's `"…"` quoting convention: quoting is
/// fragile and Sheets and Excel disagree about it, whereas flattening cannot produce a
/// grid that misaligns — and the pager is already showing that cell on one line.
pub fn table_tsv(node: &Node) -> String {
    let mut out = String::new();
    for row in node
        .children
        .iter()
        .filter(|c| matches!(c.kind, NodeKind::TableRow { .. }))
    {
        let cells: Vec<String> = row
            .children
            .iter()
            .filter(|c| matches!(c.kind, NodeKind::TableCell))
            .map(|cell| flatten(&cell.plain_text()))
            .collect();
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

/// One cell's text, with everything that would break the grid replaced by a space.
fn flatten(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c == '\t' || c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}
