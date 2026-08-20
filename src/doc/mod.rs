// SPDX-License-Identifier: MIT
//! The owned document AST.
//!
//! The document is parsed exactly once, into an **owned** tree that carries no
//! lifetimes from the parser arena. No layout decision is taken here: the tree records
//! what the author wrote, and nothing about how wide anything is.
//!
//! GitHub Flavored Markdown is enabled (tables, strikethrough, task lists, autolinks,
//! footnotes). Raw HTML is *not* supported: HTML blocks and inline HTML become
//! [`NodeKind::SkippedHtml`], which renderers must skip entirely — neither rendering
//! it nor passing it through (design spec §2 and §12).
//!
//! ```
//! use mdmost::doc::{Doc, NodeKind};
//!
//! let doc = Doc::parse("# Title\n\nSome *text*.\n");
//! assert_eq!(doc.headings().len(), 1);
//! assert_eq!(doc.headings()[0].id, "title");
//! assert!(matches!(doc.root().children[0].kind, NodeKind::Heading { level: 1, .. }));
//! ```

mod backslash;
mod convert;
mod plain;
pub(crate) mod slug;

#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use comrak::{Arena, parse_document};

use crate::text::Align;
use slug::Slugger;

/// A byte range in the document source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceSpan {
    /// First byte of the construct.
    pub start: usize,
    /// One past the last byte of the construct.
    pub end: usize,
}

impl SourceSpan {
    /// Creates a span, normalising an inverted range to an empty one.
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end: end.max(start),
        }
    }

    /// The length of the span in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the span covers no bytes.
    pub fn is_empty(&self) -> bool {
        self.end == self.start
    }

    /// Whether `offset` lies inside the span.
    pub fn contains(&self, offset: usize) -> bool {
        (self.start..self.end).contains(&offset)
    }
}

/// Splits a code block's `literal` into its lines, exactly as `NodeKind::CodeBlock`'s
/// `lines` field is built against in `convert::code_lines`.
///
/// This is the one definition of "a line of `literal`" in the crate. Anything that
/// indexes a per-line mapping by row — `convert::code_lines` itself, and
/// `render::code`, which needs the raw (unexpanded) text of the line a `SourceSpan`
/// names — must split the same way, or the two walks drift apart and index
/// `n` in one no longer names the same line as index `n` in the other.
///
/// An empty literal has no lines, not one: `"".split('\n')` would otherwise yield a
/// single phantom empty item, and a block with no lines must stay distinguishable from
/// a block holding one blank line (literal `"\n"`).
pub(crate) fn literal_lines(literal: &str) -> Vec<&str> {
    if literal.is_empty() {
        return Vec::new();
    }
    literal
        .strip_suffix('\n')
        .unwrap_or(literal)
        .split('\n')
        .collect()
}

/// Rewrites every line ending to a lone `\n`, before anything indexes the source.
///
/// **This is the crate's single boundary rule for line endings** (owner ruling: "crlf
/// should NOT cause \r on the clipboard ... copy paste should always get clean \n
/// newline"). It runs inside [`Doc::parse`] and [`Doc::parse_plain`], which between them
/// are the only ways to build a [`Doc`] — the parser is handed the normalised text and
/// [`Doc::source`] keeps that same text, so every byte offset in the application, from a
/// [`SourceSpan`] to a search hit to the clipboard, indexes a document that has no `\r`
/// in it. Nothing downstream needs a rule of its own, and nothing downstream may assume
/// it can recover the bytes as they sat on disk.
///
/// A **lone `\r`** is normalised too, on evidence rather than symmetry: `CommonMark`
/// counts it as a line ending and comrak breaks the line there, so this changes no
/// document's shape — but comrak's sourcepos for what follows a lone `\r` is wrong
/// (probed: `Text("Beta")` in `"Alpha\rBeta\n"` comes back as the empty span `11..11` in
/// an 11-byte document), so leaving it alone would leave a document whose provenance is
/// already broken. `"\n\r"` stays two line endings, as `CommonMark` says it is.
///
/// Borrows when there is nothing to do, which is every document anybody wrote on this
/// machine.
fn normalise_line_endings(source: &str) -> Cow<'_, str> {
    if !source.contains('\r') {
        return Cow::Borrowed(source);
    }
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find('\r') {
        out.push_str(&rest[..at]);
        out.push('\n');
        // Only the `\n` of a `\r\n` pair is swallowed: a `\r` followed by anything else
        // ended its own line, and the byte after it opens the next one.
        rest = rest[at + 1..].strip_prefix('\n').unwrap_or(&rest[at + 1..]);
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// How a list is numbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListInfo {
    /// `true` for `1.` style lists, `false` for bullets.
    pub ordered: bool,
    /// The first ordinal of an ordered list.
    pub start: usize,
    /// Whether the list is tight (no blank lines between items).
    pub tight: bool,
    /// The bullet character the author used, for bullet lists.
    pub bullet: char,
}

/// A table's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInfo {
    /// Per-column alignment; `None` means the author did not specify one.
    pub alignments: Vec<Option<Align>>,
    /// The number of columns, as declared by the delimiter row.
    pub columns: usize,
}

/// Which math delimiters the parser recognises.
///
/// The **one** place the document tree depends on configuration, and deliberately so
/// (design spec §3): `dollars: false` must mean a document parses exactly as it did
/// before math existed, so that no reader who never wanted this can have a `$` change
/// the shape of anything. `math_inline` is not here — it acts in the renderer and never
/// changes the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MathSyntax {
    /// `$…$`, `$$…$$`, `` $`…`$ `` and ```` ```math ```` fences.
    pub dollars: bool,
    /// `\(…\)` and `\[…\]`, which no other Markdown renderer accepts (§3.1).
    pub backslash: bool,
}

impl Default for MathSyntax {
    fn default() -> Self {
        Self {
            dollars: true,
            backslash: false,
        }
    }
}

/// What a [`Node`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeKind {
    /// The document root.
    Document,
    /// A heading. `id` is stable within the document and unique.
    Heading {
        /// Heading level, `1..=6`.
        level: u8,
        /// The anchor id used by the table of contents.
        id: String,
    },
    /// A paragraph.
    Paragraph,
    /// A block quote.
    BlockQuote,
    /// A list. Children are [`NodeKind::Item`] or [`NodeKind::TaskItem`].
    List(ListInfo),
    /// A list item.
    Item,
    /// A task list item.
    TaskItem {
        /// Whether the box is ticked.
        checked: bool,
    },
    /// A fenced or indented code block.
    CodeBlock {
        /// The raw info string, as written after the opening fence.
        info: String,
        /// The first word of the info string, lower-cased; `None` if there is none.
        language: Option<String>,
        /// The code itself, including its trailing newline.
        literal: String,
        /// Whether the block was fenced (as opposed to indented).
        fenced: bool,
        /// Where each line of `literal` came from in the document source.
        ///
        /// One entry per line of `literal`; an empty span for a line that is blank or
        /// that could not be located. comrak strips the container prefix from every
        /// line — four spaces, `> `, a list indent — so this cannot be recovered from
        /// [`Node::source`] by arithmetic, and it is built in [`super::convert`] where
        /// the source text is in hand.
        lines: Vec<SourceSpan>,
    },
    /// A thematic break (`---`).
    ThematicBreak,
    /// A GFM table. Children are [`NodeKind::TableRow`].
    Table(TableInfo),
    /// A table row. Children are [`NodeKind::TableCell`].
    TableRow {
        /// Whether this is the header row.
        header: bool,
    },
    /// A table cell. Its children are a full nested document.
    TableCell,
    /// A footnote definition.
    FootnoteDefinition {
        /// The footnote's name, as written between the brackets.
        name: String,
        /// The 1-based number the footnote is displayed as, matching the number its
        /// references carry. `None` when nothing in the document refers to it.
        number: Option<u32>,
    },
    /// A reference to a footnote.
    FootnoteReference {
        /// The footnote's name.
        name: String,
        /// The 1-based number the footnote is displayed as.
        number: u32,
    },
    /// Literal text.
    Text(String),
    /// A newline inside a paragraph, which the renderer may turn into a space.
    SoftBreak,
    /// An explicit line break (two trailing spaces or a backslash).
    LineBreak,
    /// An inline code span.
    Code {
        /// The code text.
        literal: String,
    },
    /// A formula, inline or display.
    ///
    /// The node's [`source`](Node::source) covers the delimiters as well as the
    /// formula, which is what lets a copy of it paste back as math.
    Math {
        /// The LaTeX, without its delimiters.
        literal: String,
        /// `true` for `$$…$$` and a ```` ```math ```` fence, `false` for `$…$`.
        display: bool,
    },
    /// Emphasis (`*x*`).
    Emph,
    /// Strong emphasis (`**x**`).
    Strong,
    /// Strikethrough (`~~x~~`).
    Strikethrough,
    /// A link. Children are the link text.
    Link {
        /// The target.
        url: String,
        /// The optional title.
        title: String,
    },
    /// An image. Children are the alt text; renderers draw a placeholder box.
    Image {
        /// The target.
        url: String,
        /// The optional title.
        title: String,
    },
    /// Raw HTML that `mdmost` deliberately does not support.
    ///
    /// **Renderers must skip these nodes entirely.** The literal is retained only so
    /// that diagnostics can mention what was dropped; it must never reach the canvas.
    SkippedHtml {
        /// `true` for an HTML block, `false` for inline HTML.
        block: bool,
        /// The raw HTML that was dropped.
        literal: String,
    },
}

/// A node of the owned document tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// What this node is.
    pub kind: NodeKind,
    /// The node's children, in document order.
    pub children: Vec<Node>,
    /// The byte range of the node in the document source.
    pub source: SourceSpan,
}

impl Node {
    /// Creates a node.
    pub fn new(kind: NodeKind, source: SourceSpan) -> Self {
        Self {
            kind,
            children: Vec::new(),
            source,
        }
    }

    /// The concatenated literal text of this node and its descendants.
    ///
    /// Soft and hard breaks become a single space; skipped HTML contributes nothing.
    /// Used for heading ids, TOC labels and diagram captions.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        match &self.kind {
            NodeKind::Text(text) => out.push_str(text),
            NodeKind::Code { literal } | NodeKind::Math { literal, .. } => out.push_str(literal),
            NodeKind::SoftBreak | NodeKind::LineBreak => out.push(' '),
            NodeKind::SkippedHtml { .. } => {}
            _ => {}
        }
        for child in &self.children {
            child.collect_text(out);
        }
    }

    /// Visits this node and every descendant, depth first, in document order.
    pub fn walk(&self, visit: &mut impl FnMut(&Node)) {
        visit(self);
        for child in &self.children {
            child.walk(visit);
        }
    }
}

/// A heading, as collected for the table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// The stable anchor id.
    pub id: String,
    /// The heading level, `1..=6`.
    pub level: u8,
    /// The heading's plain text.
    pub text: String,
    /// The heading's byte range in the source.
    pub source: SourceSpan,
}

/// A parsed Markdown document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    source: String,
    root: Node,
    headings: Vec<Heading>,
    version: u64,
}

impl Doc {
    /// Parses Markdown into an owned document tree.
    ///
    /// Parsing never fails: `CommonMark` has no syntax errors (design spec §12).
    ///
    /// Line endings are normalised first, so a CRLF document becomes an ordinary one
    /// here and stays one everywhere after; see [`normalise_line_endings`].
    pub fn parse(source: &str) -> Self {
        Self::parse_with(source, MathSyntax::default())
    }

    /// Parses `source` as Markdown, recognising the math delimiters `math` names.
    pub fn parse_with(source: &str, math: MathSyntax) -> Self {
        let source = &*normalise_line_endings(source);
        let arena = Arena::new();
        let root = parse_document(&arena, source, &convert::options(math));
        let mut slugger = Slugger::new();
        let mut headings = Vec::new();
        let owned = convert::document(root, source, math, &mut slugger, &mut headings);

        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);

        Self {
            source: source.to_string(),
            root: owned,
            headings,
            version: hasher.finish(),
        }
    }

    /// Parses `source` as Markdown, or as plain text when it contains no Markdown.
    ///
    /// This is the `$PAGER` entry point: `git log | mdmost` must not have its line
    /// breaks reflowed away, its e-mail addresses turned into links or its indented
    /// bodies framed as code (usability review P17). A stream carrying any Markdown
    /// markup at all still takes the full Markdown path.
    pub fn parse_auto(source: &str) -> Self {
        Self::parse_auto_with(source, MathSyntax::default())
    }

    /// [`Doc::parse_auto`], with an explicit math syntax.
    pub fn parse_auto_with(source: &str, math: MathSyntax) -> Self {
        let markdown = Self::parse_with(source, math);
        if plain::has_markup(markdown.root()) {
            markdown
        } else {
            Self::parse_plain(source)
        }
    }

    /// Parses `source` as plain text: paragraphs of hard-broken lines, nothing else.
    ///
    /// Normalises line endings on the same terms as [`Doc::parse`]: this builds its own
    /// tree with its own offsets, so a `\r` left here would be a `\r` on the clipboard
    /// of a `git log | mdmost` pipe.
    pub fn parse_plain(source: &str) -> Self {
        let source = &*normalise_line_endings(source);
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self {
            root: plain::document(source),
            source: source.to_string(),
            headings: Vec::new(),
            version: hasher.finish(),
        }
    }

    /// The document source, exactly as it was parsed.
    ///
    /// **Exactly as it was parsed, which is not necessarily as it was written**: line
    /// endings have been normalised (see [`normalise_line_endings`]). This is the string
    /// every offset the crate hands out indexes, so it is the one to slice — never the
    /// bytes the caller read off disk, which may be a different length.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The root node, always [`NodeKind::Document`].
    pub fn root(&self) -> &Node {
        &self.root
    }

    /// Every heading, in document order.
    pub fn headings(&self) -> &[Heading] {
        &self.headings
    }

    /// Looks a heading up by its anchor id.
    pub fn heading(&self, id: &str) -> Option<&Heading> {
        self.headings.iter().find(|h| h.id == id)
    }

    /// The heading that is this document's *title*, if it has one.
    ///
    /// A title is **exactly one level-1 heading, and it is the document's first
    /// block**. Both halves matter: a reference manual with a `#` per chapter has no
    /// title, and a `#` that arrives after the prose is a section heading rather than
    /// the document's name.
    ///
    /// This is one predicate with two readers, deliberately. The `FIGlet` banner
    /// (design spec §9.2) and section numbering (§9.3) both turn on "is this document
    /// titled", and two implementations of that question would eventually disagree —
    /// at which point a document would show a banner *and* number its title, or number
    /// from the wrong level under a banner. The banner adds conditions of its own on
    /// top of this one (it must be switched on, and the art must be drawable at the
    /// current width), but those are about whether the art can be *drawn*, never about
    /// whether the heading is the title. A title whose banner is declined for a CJK
    /// character is still the title, and still goes unnumbered.
    ///
    /// ```
    /// use mdmost::doc::Doc;
    ///
    /// assert_eq!(Doc::parse("# T\n\n## A\n").lone_title().map(|h| h.level), Some(1));
    /// assert!(Doc::parse("# A\n\n# B\n").lone_title().is_none());
    /// assert!(Doc::parse("intro\n\n# A\n").lone_title().is_none());
    /// ```
    pub fn lone_title(&self) -> Option<&Heading> {
        let mut level_ones = self.headings.iter().filter(|heading| heading.level == 1);
        let title = level_ones.next()?;
        if level_ones.next().is_some() {
            return None;
        }
        let first = self.root.children.first()?;
        let NodeKind::Heading { level: 1, id } = &first.kind else {
            return None;
        };
        (id == &title.id).then_some(title)
    }

    /// A hash of the source, suitable as part of a render cache key together with the
    /// width and the theme (design spec §3).
    pub fn version(&self) -> u64 {
        self.version
    }
}
