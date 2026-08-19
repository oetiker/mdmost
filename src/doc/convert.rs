// SPDX-License-Identifier: MIT
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
pub(super) fn options<'a>(math: crate::doc::MathSyntax) -> Options<'a> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    // Both, or neither. `math_dollars` covers `$…$` and `$$…$$`; `math_code` covers the
    // `` $`…`$ `` form. Turning one on without the other would accept a syntax GitHub
    // writers do not distinguish between. A ```` ```math ```` fence is neither of them:
    // comrak leaves it a `CodeBlock` for the whole parse and only its HTML renderer
    // treats it as math, so `convert` recognises it below instead.
    options.extension.math_dollars = math.dollars;
    options.extension.math_code = math.dollars;
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
    /// A line ending is one byte here, always: [`super::normalise_line_endings`] runs
    /// before the parser does, so there is no `\r` left to drop.
    fn line(&self, index: usize) -> Option<(usize, &'s str)> {
        let start = *self.starts.get(index)?;
        let end = self
            .starts
            .get(index + 1)
            .map_or(self.len, |next| next.saturating_sub(1));
        Some((start, self.source.get(start..end)?))
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
    let lines = super::literal_lines(literal);
    let mut out = Vec::with_capacity(lines.len());
    let mut index = offsets.line_index(span.start);
    let last = offsets.line_index(span.end.saturating_sub(1));
    for line in lines {
        // comrak copies a code block's bytes into `literal` verbatim (confirmed against
        // comrak's `parser/mod.rs`: the code-block path appends the raw slice and
        // strips only the info string), so `line` is a byte-for-byte copy of the part
        // of the source line that follows the container prefix — which is exactly what
        // the suffix match below needs, and only because both sides went through
        // `super::normalise_line_endings` first. This used to strip a trailing `\r`
        // from `line`, because `offsets.line` stripped one from the source side and the
        // two could never match otherwise; there is no longer a `\r` on either side.
        if line.is_empty() {
            // A blank line pushes an empty span without advancing `index`, which widens
            // the search window for the next line by one slot. That is safe: an empty
            // source line can never satisfy `text.ends_with(line)` for a non-empty
            // `line`, so it never produces a wrong match — only a wider, still-correct
            // search. Left alone deliberately; do not "optimise" it into a bug.
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
    math: crate::doc::MathSyntax,
    slugger: &mut Slugger,
    headings: &mut Vec<Heading>,
) -> Node {
    let offsets = LineOffsets::new(source);
    let mut doc = convert(root, &offsets, math, slugger, headings);
    number_footnotes(&mut doc);
    hoist_display_math(&mut doc);
    // The pass tolerates either order (`align` re-derives everything from the source
    // regardless of how finely `split_transcriptions` has already cut the run) — moving
    // this after it and rerunning every test here confirmed byte-identical trees. It
    // stays first anyway: that was the original design intent, and there is no cost to
    // keeping it.
    if math.backslash {
        super::backslash::split_backslash_math(&mut doc, source);
    }
    split_transcriptions(&mut doc, source);
    doc
}

/// Lifts a paragraph whose only child is display math into a block of its own.
///
/// comrak parses `$$…$$` with its *inline* code, so a display formula arrives as the
/// single child of a Paragraph. It is a block — spec §6 lays it out in two dimensions and
/// spec §7 centres it — so the block renderer has to be able to see it, and the document
/// layer is where that belongs: stage 2 needs the same shape.
///
/// Only a paragraph with exactly one child moves. `text $$x$$ text` is prose with a
/// formula in it and stays inline, where Task 10's arm shows it as its own source.
fn hoist_display_math(node: &mut Node) {
    for child in &mut node.children {
        hoist_display_math(child);
    }
    for child in &mut node.children {
        let hoist = matches!(child.kind, NodeKind::Paragraph)
            && matches!(
                child.children.as_slice(),
                [only] if matches!(only.kind, NodeKind::Math { display: true, .. })
            );
        if hoist {
            let inner = child.children.remove(0);
            *child = inner;
        }
    }
}

/// The longest `&…;` treated as an entity: HTML5's longest name is 31 characters.
const MAX_ENTITY: usize = 34;

/// What a divergence between the source and the text it drew turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transcription {
    /// A backslash escape: two source bytes, the second of which is drawn verbatim.
    Escape,
    /// A character reference of this many source bytes, `&` and `;` included.
    Entity(usize),
}

/// Splits every text node whose text is not a byte-for-byte copy of its source.
///
/// comrak reports one `Text` node per unbroken run of literal text, and the run keeps
/// its escapes and entities in its `sourcepos` while its text has them resolved. The
/// two lengths then disagree — and a run whose lengths disagree can carry no
/// provenance at all, because there is no way to say which byte drew which cell. So a
/// single `\*` or `&amp;` anywhere in a paragraph used to cost *every* word in it its
/// search spans, its selection highlight and its place in the clipboard.
///
/// Splitting makes that degradation one character wide instead of one paragraph wide.
/// The prose either side of a transcription is an exact copy of its source again, and
/// the transcription becomes a node of its own naming exactly the bytes that drew it.
/// A run that does not re-synchronise is left precisely as comrak reported it, which
/// is the behaviour every escaped run had before: no origin beats a guessed one.
///
/// Run once over the finished tree rather than inside [`convert`] because one node
/// becomes several, which only the parent can hold.
fn split_transcriptions(node: &mut Node, source: &str) {
    let mut out: Vec<Node> = Vec::with_capacity(node.children.len());
    for mut child in std::mem::take(&mut node.children) {
        split_transcriptions(&mut child, source);
        match segments(&child, source) {
            Some(segments) => out.extend(
                segments
                    .into_iter()
                    .map(|(text, span)| Node::new(NodeKind::Text(text), span)),
            ),
            None => out.push(child),
        }
    }
    node.children = out;
}

/// The aligned runs of a text node, or `None` if it is already faithful or cannot be
/// aligned.
fn segments(node: &Node, source: &str) -> Option<Vec<(String, SourceSpan)>> {
    let NodeKind::Text(text) = &node.kind else {
        return None;
    };
    if node.source.len() == text.len() {
        return None;
    }
    let bytes = source.get(node.source.start..node.source.end)?;
    align(bytes, text, node.source.start)
}

/// Walks `src` and the `text` it produced together, cutting at every transcription.
///
/// Where the two agree the walk advances both, which is the run of prose that copies
/// its source. Where they diverge it is at an escape or a character reference, and the
/// walk consumes the source form against the one character it drew before carrying on.
/// `start` is where `src` sits in the document, so the spans come out absolute.
///
/// Returns `None` the moment the two stop re-synchronising: an entity that expands to
/// more than one character (`&fjlig;` is `fj`), a tab comrak widened, anything unknown.
/// The caller then keeps the node whole, which is exactly today's behaviour.
pub(super) fn align(src: &str, text: &str, start: usize) -> Option<Vec<(String, SourceSpan)>> {
    let mut out: Vec<(String, SourceSpan)> = Vec::new();
    let (mut s, mut t) = (0usize, 0usize);
    let (mut run_s, mut run_t) = (0usize, 0usize);
    loop {
        let source_char = src[s..].chars().next();
        let text_char = text[t..].chars().next();
        if let (Some(a), Some(b)) = (source_char, text_char)
            && a == b
        {
            s += a.len_utf8();
            t += b.len_utf8();
            continue;
        }
        if source_char.is_none() && text_char.is_none() {
            break;
        }
        let (at, kind) = rewind(src, text, run_s, run_t, s)?;
        let t_at = run_t + (at - run_s);
        if at > run_s {
            out.push((
                text[run_t..t_at].to_string(),
                SourceSpan::new(start + run_s, start + at),
            ));
        }
        let drawn = text[t_at..].chars().next()?;
        let span = match kind {
            // The backslash is undrawn markup, like the `**` around a bold word: the
            // character it protects *is* a copy of the byte after it, so the segment
            // names that byte alone and stays faithful. `extend_over_markup` picks the
            // backslash up from the selection side, as it does every other marker.
            Transcription::Escape => {
                SourceSpan::new(start + at + 1, start + at + 1 + drawn.len_utf8())
            }
            // Nothing in `&amp;` copies the `&` it draws, so the character takes the
            // whole reference: those bytes are exactly what produced that one cell.
            Transcription::Entity(len) => SourceSpan::new(start + at, start + at + len),
        };
        out.push((drawn.to_string(), span));
        s = match kind {
            Transcription::Escape => at + 1 + drawn.len_utf8(),
            Transcription::Entity(len) => at + len,
        };
        t = t_at + drawn.len_utf8();
        run_s = s;
        run_t = t;
    }
    if s > run_s {
        out.push((
            text[run_t..t].to_string(),
            SourceSpan::new(start + run_s, start + s),
        ));
    }
    debug_assert_eq!(
        out.iter()
            .map(|(text, _)| text.as_str())
            .collect::<String>(),
        text,
        "the segments must reproduce the text they were cut from"
    );
    (!out.is_empty()).then_some(out)
}

/// Where the divergence noticed at `s` actually began, and what it is.
///
/// Usually `s` itself. But a transcription whose *first* character happens to be what
/// it draws is walked straight past: the first byte of `\\` compares equal to the one
/// backslash it draws, and so does the `&` opening the second of two `&amp;`s. Both
/// are only noticed a character later, with the transcription already behind the
/// cursor — so the search runs backwards, and the nearest candidate inside the run of
/// prose just walked wins.
fn rewind(
    src: &str,
    text: &str,
    run_s: usize,
    run_t: usize,
    s: usize,
) -> Option<(usize, Transcription)> {
    let bytes = src.as_bytes();
    for at in (run_s..=s).rev().filter(|at| *at < src.len()) {
        // The run walked between `run_s` and `s` copies its source, so the two cursors
        // moved in step and the text position is the same distance along.
        let t_at = run_t + (at - run_s);
        let Some(drawn) = text[t_at..].chars().next() else {
            continue;
        };
        match bytes[at] {
            b'\\' => {
                // comrak only honours a backslash in front of ASCII punctuation; in
                // front of anything else it is a literal backslash, which would have
                // compared equal and never reached here.
                if let Some(escaped) = src[at + 1..].chars().next()
                    && escaped.is_ascii_punctuation()
                    && escaped == drawn
                {
                    return Some((at, Transcription::Escape));
                }
            }
            b'&' => {
                if let Some(len) = entity_len(&src[at..]) {
                    return Some((at, Transcription::Entity(len)));
                }
            }
            _ => {}
        }
    }
    None
}

/// The byte length of the character reference `src` opens with, `&` and `;` included.
///
/// Shape only — whether the name is one comrak knows is not decidable here, and does
/// not need to be: a name it does not know never diverges from the text it drew, so
/// the walk never asks. `&notreal;` is passed through verbatim and stays a copy.
fn entity_len(src: &str) -> Option<usize> {
    let body = src.strip_prefix('&')?;
    let name = body.split(';').next()?;
    if name.is_empty()
        || name.len() > MAX_ENTITY - 2
        || name.len() == body.len()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'#')
    {
        return None;
    }
    Some(name.len() + 2)
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
    math: crate::doc::MathSyntax,
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
        // A ```` ```math ```` fence. `math_code` does not cover it — in comrak 0.54 that
        // extension is the `` $`…`$ `` form alone — and nothing in the parser sets
        // `display_math` for a fence, so it arrives here as an ordinary code block and is
        // recognised by its info string.
        //
        // The guard is not optional. Spec §3 requires that `math = false` parses a
        // document exactly as it did before math existed, and without it a ```` ```math ````
        // fence would stop being a code block whatever the reader configured.
        NodeValue::CodeBlock(code)
            if math.dollars
                && code
                    .info
                    .split_whitespace()
                    .next()
                    .is_some_and(|word| word.eq_ignore_ascii_case("math")) =>
        {
            NodeKind::Math {
                literal: code.literal.clone(),
                display: true,
            }
        }
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
        NodeValue::Math(comrak::nodes::NodeMath {
            literal,
            display_math,
            ..
        }) => NodeKind::Math {
            literal: literal.clone(),
            display: *display_math,
        },
        NodeValue::Raw(text) => NodeKind::Text(text.clone()),
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
            .map(|child| convert(child, offsets, math, slugger, headings))
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
