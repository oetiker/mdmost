//! Plain-text input: the `$PAGER` path for streams that are not Markdown.
//!
//! `export PAGER=mdless` points `git log`, `--help` output and man pages at this
//! program, and none of them are Markdown. Parsing them as Markdown actively damages
//! them: line breaks are reflowed away, `Author: a@b.c` becomes a mailto link, and an
//! indented commit body becomes a framed code block (usability review P17).
//!
//! So a stream with no Markdown in it is not parsed as Markdown. It becomes a document
//! of paragraphs — one per run of non-blank lines — whose lines are joined by hard
//! breaks. Nothing is reinterpreted: no emphasis, no autolinks, no code fences. Long
//! lines still wrap to the terminal, which is the one thing a pager must do.

use super::{Node, NodeKind, SourceSpan};

/// Whether `source` contains anything that reads as Markdown markup.
///
/// This is deliberately generous towards Markdown: a document with a single heading,
/// fence, list, quote, table or link is treated as Markdown, and only a stream with
/// none of those at all takes the plain-text path. Being wrong in this direction costs
/// a plain-text stream nothing extra, while being wrong the other way would render a
/// real document as flat text.
pub(super) fn looks_like_markdown(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        let heading =
            trimmed.starts_with('#') && trimmed.trim_start_matches('#').starts_with([' ', '\t']);
        heading
            || trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || trimmed.starts_with("> ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || is_ordered_item(trimmed)
            || is_table_delimiter(trimmed)
            || line.contains("](")
            || line.contains("**")
            || line.contains("__")
    })
}

/// Whether a line opens an ordered list item, as in `1. text`.
fn is_ordered_item(line: &str) -> bool {
    let digits = line.trim_start_matches(|c: char| c.is_ascii_digit());
    digits.len() < line.len() && (digits.starts_with(". ") || digits.starts_with(") "))
}

/// Whether a line is a GFM table delimiter row, as in `|---|:--:|`.
fn is_table_delimiter(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|')
        && trimmed.len() > 2
        && trimmed
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
        && trimmed.contains('-')
}

/// Builds a document that reproduces `source` line for line.
pub(super) fn document(source: &str) -> Node {
    let mut root = Node::new(NodeKind::Document, SourceSpan::new(0, source.len()));
    let mut paragraph: Option<Node> = None;
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let text = line.strip_suffix('\n').unwrap_or(line);
        let text = text.strip_suffix('\r').unwrap_or(text);
        if text.trim().is_empty() {
            root.children.extend(paragraph.take());
        } else {
            let para = paragraph.get_or_insert_with(|| {
                Node::new(NodeKind::Paragraph, SourceSpan::new(offset, offset))
            });
            if !para.children.is_empty() {
                // A hard break, so the line structure the writer chose survives.
                para.children.push(Node::new(
                    NodeKind::LineBreak,
                    SourceSpan::new(offset, offset),
                ));
            }
            let span = SourceSpan::new(offset, offset + text.len());
            para.children
                .push(Node::new(NodeKind::Text(text.to_string()), span));
            para.source = SourceSpan::new(para.source.start, offset + text.len());
        }
        offset += line.len();
    }
    root.children.extend(paragraph);
    root
}
