//! Conversion from the comrak arena AST to the owned [`Node`](super::Node) tree.
//!
//! Keeping this here means [`super`] holds only the owned data model.

use comrak::Options;
use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};

use super::slug::Slugger;
use super::{Heading, ListInfo, Node, NodeKind, SourceSpan, TableInfo};
use crate::text::Align;

/// The comrak options `mdless` parses with.
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
struct LineOffsets {
    /// Byte offset of the first byte of each line.
    starts: Vec<usize>,
    len: usize,
}

impl LineOffsets {
    fn new(source: &str) -> Self {
        let mut starts = vec![0usize];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self {
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
    convert(root, &offsets, slugger, headings)
}

fn convert<'a>(
    node: &'a AstNode<'a>,
    offsets: &LineOffsets,
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
        },
        NodeValue::FootnoteReference(reference) => NodeKind::FootnoteReference {
            name: reference.name.clone(),
            number: reference.ref_num,
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
