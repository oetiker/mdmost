//! Plain-text input: the `$PAGER` path for streams that are not Markdown.
//!
//! `export PAGER=mdmost` points `git log`, `--help` output and man pages at this
//! program, and none of them are Markdown. Parsing them as Markdown actively damages
//! them: line breaks are reflowed away, `Author: a@b.c` becomes a mailto link, and an
//! indented commit body becomes a framed code block (usability review P17).
//!
//! So a stream with no Markdown in it is not parsed as Markdown. It becomes a document
//! of paragraphs — one per run of non-blank lines — whose lines are joined by hard
//! breaks. Nothing is reinterpreted: no emphasis, no autolinks, no code fences. Long
//! lines still wrap to the terminal, which is the one thing a pager must do.

use super::{Node, NodeKind, SourceSpan};

/// Whether `root` — a document already parsed as Markdown — contains real markup.
///
/// This asks the Markdown parser rather than scanning for `#` and `- ` with a growing
/// pile of string rules. A hand-written signal list is wrong the moment a construct is
/// added: it was, and a document whose only markup was a footnote (`[^a]`) was paged as
/// flat text. Anything the parser recognises therefore counts — except the handful of
/// constructs plain text produces *by accident*:
///
/// * an **indented** code block — four leading spaces is how `git log` sets a commit
///   body, not how a writer opens a code block (a fenced one still counts);
/// * an **autolink** — `Author: a@b.c` is an e-mail address in a header, not a link a
///   writer typed;
/// * a **hard line break** — two trailing spaces are lint in plain text, not intent;
/// * a **thematic break** — a row of dashes is a separator in `--help` output as often
///   as it is a Markdown rule;
/// * **raw HTML** — a stray `<foo>` in prose is not a document with HTML in it.
///
/// Being wrong towards Markdown is cheap; being wrong the other way renders a real
/// document as flat text, so the accidental list is kept deliberately short.
pub(super) fn has_markup(root: &Node) -> bool {
    match &root.kind {
        NodeKind::Heading { .. }
        | NodeKind::BlockQuote
        | NodeKind::List(_)
        | NodeKind::TaskItem { .. }
        | NodeKind::Table(_)
        | NodeKind::FootnoteDefinition { .. }
        | NodeKind::FootnoteReference { .. }
        | NodeKind::Code { .. }
        | NodeKind::Emph
        | NodeKind::Strong
        | NodeKind::Strikethrough
        | NodeKind::Image { .. } => return true,
        NodeKind::CodeBlock { fenced, .. } => return *fenced,
        NodeKind::Link { url, .. } if !is_autolink(root, url) => return true,
        _ => {}
    }
    root.children.iter().any(has_markup)
}

/// Whether a link is one the parser inferred from bare text rather than one written.
///
/// comrak gives a bare `a@b.c` the target `mailto:a@b.c`, so the scheme has to come off
/// before the two can be compared.
fn is_autolink(node: &Node, url: &str) -> bool {
    let text = node.plain_text();
    let text = text.trim();
    url == text || url.trim_start_matches("mailto:") == text
}

/// Builds a document that reproduces `source` line for line.
///
/// `source` has been through [`super::normalise_line_endings`], so splitting on `\n` and
/// taking one byte off the end is the whole of "a line" here — a `\r` this walk had to
/// strip for itself would also be a `\r` the offsets below counted as text.
pub(super) fn document(source: &str) -> Node {
    let mut root = Node::new(NodeKind::Document, SourceSpan::new(0, source.len()));
    let mut paragraph: Option<Node> = None;
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let text = line.strip_suffix('\n').unwrap_or(line);
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
