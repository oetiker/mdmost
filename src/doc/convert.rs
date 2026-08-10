//! Conversion from the comrak arena AST to the owned [`Node`](super::Node) tree.
//!
//! Keeping this here means [`super`] holds only the owned data model.

use std::collections::HashMap;

use comrak::Options;
use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};

use super::slug::Slugger;
use super::{Heading, ListInfo, Node, NodeKind, SourceSpan, TableInfo};
use crate::text::Align;

/// The comrak options `mdmost` parses with.
pub(super) fn options<'a>() -> Options<'a> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    options
}

/// Converts a byte position table so `sourcepos` line/column pairs become byte offsets.
struct LineOffsets<'s> {
    source: &'s str,
    /// Byte offset of the first byte of each line.
    starts: Vec<usize>,
    len: usize,
}

impl<'s> LineOffsets<'s> {
    fn new(source: &'s str) -> Self {
        let mut starts = vec![0usize];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self {
            source,
            starts,
            len: source.len(),
        }
    }

    /// Byte offset of a 1-based line and 1-based byte column.
    fn offset(&self, line: usize, column: usize) -> usize {
        if line == 0 {
            return 0;
        }
        let start = self.starts.get(line - 1).copied().unwrap_or(self.len);
        (start + column.saturating_sub(1)).min(self.len)
    }

    /// The byte range of a comrak `Sourcepos`.
    fn span(&self, pos: comrak::nodes::Sourcepos) -> SourceSpan {
        if pos.start.line == 0 {
            return SourceSpan::default();
        }
        let start = self.offset(pos.start.line, pos.start.column);
        // comrak reports an inclusive end column, so one more byte belongs to the node.
        let end = self.offset(pos.end.line, pos.end.column + 1);
        SourceSpan::new(start, end)
    }

    /// The 0-based index of the line containing `offset`.
    fn line_index(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        }
    }

    /// The content of 0-based line `index`, without its line ending, and where it starts.
    ///
    /// A trailing `\r` is dropped so that a CRLF document matches like any other.
    fn line(&self, index: usize) -> Option<(usize, &'s str)> {
        let start = *self.starts.get(index)?;
        let end = self
            .starts
            .get(index + 1)
            .map_or(self.len, |next| next.saturating_sub(1));
        let text = self.source.get(start..end)?;
        Some((start, text.strip_suffix('\r').unwrap_or(text)))
    }
}

/// Where each line of a code block's `literal` came from in the source.
///
/// Matched as a **suffix** of a source line, which is comrak's prefix-stripping run
/// backwards: four spaces, `> ` and a list indent all fall out of the same rule without
/// being special-cased, and the match is checked against the real text rather than
/// assumed. A line that cannot be located gets an empty span and the walk carries on —
/// no provenance is today's behaviour, whereas a *wrong* offset would put a search hit
/// on the wrong cells and copy the wrong bytes.
fn code_lines(offsets: &LineOffsets<'_>, span: SourceSpan, literal: &str) -> Vec<SourceSpan> {
    let mut out = Vec::new();
    let mut index = offsets.line_index(span.start);
    let last = offsets.line_index(span.end.saturating_sub(1));
    for line in literal.strip_suffix('\n').unwrap_or(literal).split('\n') {
        if line.is_empty() {
            out.push(SourceSpan::default());
            continue;
        }
        let found = (index..=last).find_map(|at| {
            let (start, text) = offsets.line(at)?;
            text.ends_with(line).then(|| {
                (
                    at,
                    SourceSpan::new(start + text.len() - line.len(), start + text.len()),
                )
            })
        });
        match found {
            Some((at, found)) => {
                index = at + 1;
                out.push(found);
            }
            None => out.push(SourceSpan::default()),
        }
    }
    out
}

/// Recursively converts a comrak node into an owned [`Node`].
/// Converts a parsed comrak document into the owned tree.
///
/// This is the module's only entry point; the recursion below is an implementation
/// detail.
pub(super) fn document<'a>(
    root: &'a AstNode<'a>,
    source: &str,
    slugger: &mut Slugger,
    headings: &mut Vec<Heading>,
) -> Node {
    let offsets = LineOffsets::new(source);
    let mut doc = convert(root, &offsets, slugger, headings);
    number_footnotes(&mut doc);
    doc
}

/// Whether an inline HTML tag is a `<br>` in any of its accepted spellings.
fn is_line_break(html: &str) -> bool {
    let inner = html
        .trim()
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or("");
    let name = inner.trim_end_matches('/').trim();
    name.eq_ignore_ascii_case("br")
}

/// Gives every footnote definition the number its references are shown with.
///
/// comrak numbers references (`ix`) but not definitions, so the mapping is rebuilt
/// here from the references actually present. A definition nothing refers to keeps
/// `None` and is labelled by name.
fn number_footnotes(doc: &mut Node) {
    let mut numbers = HashMap::new();
    collect_footnote_numbers(doc, &mut numbers);
    apply_footnote_numbers(doc, &numbers);
}

/// Records the number of every footnote reference in the tree, by name.
fn collect_footnote_numbers(node: &Node, out: &mut HashMap<String, u32>) {
    if let NodeKind::FootnoteReference { name, number } = &node.kind {
        out.entry(name.clone()).or_insert(*number);
    }
    for child in &node.children {
        collect_footnote_numbers(child, out);
    }
}

/// Copies the collected numbers onto the matching definitions.
fn apply_footnote_numbers(node: &mut Node, numbers: &HashMap<String, u32>) {
    if let NodeKind::FootnoteDefinition { name, number } = &mut node.kind {
        *number = numbers.get(name).copied();
    }
    for child in &mut node.children {
        apply_footnote_numbers(child, numbers);
    }
}

fn convert<'a>(
    node: &'a AstNode<'a>,
    offsets: &LineOffsets<'_>,
    slugger: &mut Slugger,
    headings: &mut Vec<Heading>,
) -> Node {
    let ast = node.data.borrow();
    let source = offsets.span(ast.sourcepos);
    let kind = match &ast.value {
        NodeValue::Document | NodeValue::FrontMatter(_) => NodeKind::Document,
        NodeValue::Heading(h) => NodeKind::Heading {
            level: h.level,
            // The id is patched in below, once the children are known.
            id: String::new(),
        },
        NodeValue::Paragraph => NodeKind::Paragraph,
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => NodeKind::BlockQuote,
        NodeValue::List(list) => NodeKind::List(ListInfo {
            ordered: list.list_type == ListType::Ordered,
            start: list.start,
            tight: list.tight,
            bullet: char::from(if list.bullet_char == 0 {
                b'-'
            } else {
                list.bullet_char
            }),
        }),
        NodeValue::Item(_) => NodeKind::Item,
        NodeValue::TaskItem(task) => NodeKind::TaskItem {
            checked: task.symbol.is_some(),
        },
        NodeValue::CodeBlock(code) => {
            let info = code.info.clone();
            let language = info
                .split_whitespace()
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase);
            NodeKind::CodeBlock {
                info,
                language,
                lines: code_lines(offsets, source, &code.literal),
                literal: code.literal.clone(),
                fenced: code.fenced,
            }
        }
        NodeValue::ThematicBreak => NodeKind::ThematicBreak,
        NodeValue::Table(table) => NodeKind::Table(TableInfo {
            alignments: table.alignments.iter().copied().map(alignment).collect(),
            columns: table.num_columns,
        }),
        NodeValue::TableRow(header) => NodeKind::TableRow { header: *header },
        NodeValue::TableCell => NodeKind::TableCell,
        NodeValue::FootnoteDefinition(def) => NodeKind::FootnoteDefinition {
            name: def.name.clone(),
            // Filled in by `number_footnotes` once every reference has been seen.
            number: None,
        },
        // `ix` is the footnote's position in the document; `ref_num` counts the
        // references to *one* footnote and is not what a reader is shown.
        NodeValue::FootnoteReference(reference) => NodeKind::FootnoteReference {
            name: reference.name.clone(),
            number: reference.ix,
        },
        NodeValue::Text(text) => NodeKind::Text(text.to_string()),
        NodeValue::SoftBreak => NodeKind::SoftBreak,
        NodeValue::LineBreak => NodeKind::LineBreak,
        NodeValue::Code(code) => NodeKind::Code {
            literal: code.literal.clone(),
        },
        NodeValue::Emph => NodeKind::Emph,
        NodeValue::Strong | NodeValue::Highlight | NodeValue::Insert => NodeKind::Strong,
        NodeValue::Strikethrough => NodeKind::Strikethrough,
        NodeValue::Link(link) => NodeKind::Link {
            url: link.url.clone(),
            title: link.title.clone(),
        },
        NodeValue::WikiLink(link) => NodeKind::Link {
            url: link.url.clone(),
            title: String::new(),
        },
        NodeValue::Image(image) => NodeKind::Image {
            url: image.url.clone(),
            title: image.title.clone(),
        },
        NodeValue::HtmlBlock(html) => NodeKind::SkippedHtml {
            block: true,
            literal: html.literal.clone(),
        },
        // `<br>` is the only way GFM offers to break a line inside a table cell, and
        // it is what every writer reaches for. Honouring it as a line break is not
        // "passing HTML through": nothing of the tag reaches the canvas.
        NodeValue::HtmlInline(html) if is_line_break(html) => NodeKind::LineBreak,
        NodeValue::HtmlInline(html) => NodeKind::SkippedHtml {
            block: false,
            literal: html.clone(),
        },
        NodeValue::Raw(text) | NodeValue::Math(comrak::nodes::NodeMath { literal: text, .. }) => {
            NodeKind::Text(text.clone())
        }
        // Everything else is a container we do not style specially; treat it as a
        // paragraph-like wrapper so its children still render.
        _ => NodeKind::Paragraph,
    };
    drop(ast);

    let mut owned = Node::new(kind, source);
    // Raw HTML is dropped wholesale: no children are converted.
    if !matches!(owned.kind, NodeKind::SkippedHtml { .. }) {
        owned.children = node
            .children()
            .map(|child| convert(child, offsets, slugger, headings))
            .collect();
    }

    if let NodeKind::Heading { level, id } = &mut owned.kind {
        let text = {
            let mut buf = String::new();
            for child in &owned.children {
                child.collect_text(&mut buf);
            }
            buf.trim().to_string()
        };
        *id = slugger.slug(&text);
        headings.push(Heading {
            id: id.clone(),
            level: *level,
            text,
            source,
        });
    }

    owned
}

/// Maps a comrak table alignment to ours.
fn alignment(value: TableAlignment) -> Option<Align> {
    match value {
        TableAlignment::None => None,
        TableAlignment::Left => Some(Align::Left),
        TableAlignment::Center => Some(Align::Center),
        TableAlignment::Right => Some(Align::Right),
    }
}
