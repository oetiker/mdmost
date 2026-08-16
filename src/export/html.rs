// SPDX-License-Identifier: MIT
//! The HTML flavour: the upgrade a local clipboard can carry.
//!
//! Offered beside the TSV, never instead of it — OSC 52 has no MIME flavours, so a
//! reader on a remote host receives the TSV alone and must still get cells.
//!
//! **Everything here is generated and escaped here.** A document is untrusted input, and
//! this payload is handed to another application to interpret: a cell containing
//! `<script>` arrives as escaped text, and only `http`, `https` and `mailto` links keep
//! an `href`. No markup from the document is ever passed through — that would be the
//! "no HTML" rule, and this is the opposite direction: an AST the pager already parsed,
//! serialised out.

use crate::doc::{Node, NodeKind, TableInfo};
use crate::text::Align;

/// A table as an HTML `<table>`.
pub fn table_html(node: &Node) -> String {
    let NodeKind::Table(info) = &node.kind else {
        return String::new();
    };
    let mut out = String::from("<table>");
    for row in &node.children {
        let NodeKind::TableRow { header } = row.kind else {
            continue;
        };
        out.push_str("<tr>");
        for (index, cell) in row
            .children
            .iter()
            .filter(|c| matches!(c.kind, NodeKind::TableCell))
            .enumerate()
        {
            let tag = if header { "th" } else { "td" };
            out.push('<');
            out.push_str(tag);
            out.push_str(align_attribute(info, index));
            out.push('>');
            for child in &cell.children {
                inline(child, &mut out);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

/// The `align` attribute for a column, or nothing when none was declared.
///
/// A column the author left undeclared gets no attribute at all rather than an explicit
/// `left`, so the pasting application keeps whatever default it would have used.
fn align_attribute(info: &TableInfo, column: usize) -> &'static str {
    match info.alignments.get(column).copied().flatten() {
        Some(Align::Right) => r#" align="right""#,
        Some(Align::Center) => r#" align="center""#,
        Some(Align::Left) => r#" align="left""#,
        None => "",
    }
}

/// Serialises one inline node, escaping everything it emits.
fn inline(node: &Node, out: &mut String) {
    match &node.kind {
        NodeKind::Text(text) => escape_into(text, out),
        NodeKind::Strong => wrap(out, "strong", node),
        NodeKind::Emph => wrap(out, "em", node),
        NodeKind::Strikethrough => wrap(out, "del", node),
        NodeKind::Code { literal } => {
            out.push_str("<code>");
            escape_into(literal, out);
            out.push_str("</code>");
        }
        NodeKind::Link { url, .. } => {
            if is_safe_url(url) {
                out.push_str(r#"<a href=""#);
                escape_into(url, out);
                out.push_str(r#"">"#);
                children(node, out);
                out.push_str("</a>");
            } else {
                // The scheme is not one another application should be handed. The link
                // text is still what the reader saw, so it stays.
                children(node, out);
            }
        }
        NodeKind::LineBreak => out.push_str("<br>"),
        NodeKind::SoftBreak => out.push(' '),
        _ => escape_into(&node.plain_text(), out),
    }
}

/// Serialises `node`'s children inside a `tag` element.
fn wrap(out: &mut String, tag: &str, node: &Node) {
    out.push('<');
    out.push_str(tag);
    out.push('>');
    children(node, out);
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

/// Serialises `node`'s children with no element around them.
fn children(node: &Node, out: &mut String) {
    for child in &node.children {
        inline(child, out);
    }
}

/// Whether a URL may be handed to another application as an `href`.
///
/// An allow-list, not a deny-list: the payload leaves this process and is interpreted
/// elsewhere, so the question is closed by naming what is permitted rather than by
/// trying to name every scheme that is not. A relative target is not on the list either
/// — it would resolve against whatever document the paste lands in, which is never the
/// one the reader was looking at.
fn is_safe_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Appends `text` with the four characters that would otherwise be markup escaped.
///
/// `"` is escaped along with the rest rather than only in attribute position, so that no
/// caller can pick the wrong helper: there is only one.
fn escape_into(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}
