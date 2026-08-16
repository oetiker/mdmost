// SPDX-License-Identifier: MIT
//! `flowchart` / `graph` parsing (design spec §6.1).
//!
//! Supported: all five directions, the seven documented node shapes (any other
//! bracket form degrades to a rectangle with its label intact), solid/dotted/thick
//! links with optional back arrows, both label forms (`-->|text|` and `-- text -->`),
//! `&` node groups, chained edges, and nested `subgraph` … `end`.
//!
//! Skipped silently: `click`, `style`, `classDef`, `class`, `cssClass`, `linkStyle`.
//!
//! Rejected with a reason: `--x` / `--o` link terminators, `@{ … }` shape metadata,
//! and edges whose endpoint is a subgraph.

use crate::error::MermaidError;
use crate::mermaid::ast::{
    ArrowHead, EdgeStroke, FlowEdge, FlowNode, Flowchart, Group, NodeId, NodeShape,
};

use super::lex::{self, Nesting, SrcLine};
use super::{direction, intern};

/// Parses a whole `flowchart` / `graph` diagram.
///
/// `src` is the full mermaid source `lines` was lexed from; it is kept only so that
/// label text — always a subslice of it — can report where it came from.
pub fn parse<'a>(lines: &[SrcLine<'a>], src: &'a str) -> Result<Flowchart, MermaidError> {
    let mut builder = Builder {
        src,
        ..Builder::default()
    };
    let Some((header, body)) = lines.split_first() else {
        return Err(lex::syntax(1, "empty diagram"));
    };

    // `graph TD; A-->B` puts statements on the header line.
    let (_, after_keyword) = lex::split_word(header.text);
    let mut statements = lex::split_piped_statements(after_keyword);
    if let Some(first) = statements.first() {
        let (word, rest) = lex::split_word(first);
        if let Some(dir) = direction(word) {
            builder.direction = dir;
            if rest.is_empty() {
                statements.remove(0);
            } else {
                statements[0] = rest;
            }
        }
    }
    for statement in statements {
        builder.statement(statement, header.number)?;
    }

    for line in body {
        for statement in lex::split_piped_statements(line.text) {
            builder.statement(statement, line.number)?;
        }
    }
    builder.finish(lines.last().map_or(1, |line| line.number))
}

/// Accumulates nodes, edges and the subgraph tree while statements are read.
#[derive(Debug, Default)]
struct Builder<'a> {
    direction: crate::mermaid::ast::Direction,
    nodes: Vec<FlowNode>,
    edges: Vec<FlowEdge>,
    /// The open container stack; index 0 is the implicit root group.
    stack: Vec<Group>,
    /// Keys of every subgraph seen, used to reject edges that point at one.
    subgraph_keys: Vec<String>,
    /// The full mermaid source, passed to `lex::label_at` to compute a label's byte
    /// offset — every label text this parser touches is a subslice of it.
    src: &'a str,
}

impl Builder<'_> {
    /// Handles one `;`-separated statement.
    fn statement(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        if self.stack.is_empty() {
            self.stack.push(Group::default());
        }
        let (word, rest) = lex::split_word(text);
        let lowered = word.to_ascii_lowercase();
        match lowered.as_str() {
            // Cosmetic statements carry nothing the renderer can use.
            "click" | "style" | "classdef" | "class" | "cssclass" | "linkstyle" => return Ok(()),
            "direction" => {
                let dir = direction(lex::split_word(rest).0)
                    .ok_or_else(|| lex::syntax(line, format!("unknown direction `{rest}`")))?;
                match self.stack.len() {
                    1 => self.direction = dir,
                    _ => {
                        if let Some(group) = self.stack.last_mut() {
                            group.direction = Some(dir);
                        }
                    }
                }
                return Ok(());
            }
            "subgraph" => return self.open_subgraph(rest, line),
            "end" if rest.is_empty() => return self.close_subgraph(line),
            _ => {}
        }
        self.edge_chain(text, line)
    }

    /// Opens a `subgraph id [Title]` container.
    fn open_subgraph(&mut self, rest: &str, line: usize) -> Result<(), MermaidError> {
        let src = self.src;
        let (key, title) = match shape_at(rest, 0) {
            // `subgraph one [Title]` / `subgraph one["Title"]`
            Some(shape) if shape.start > 0 => {
                let text = lex::unquote(shape.text);
                (
                    Some(rest[..shape.start].trim().to_string()),
                    Some(lex::label_at(src, text)),
                )
            }
            _ => {
                let name = lex::unquote(rest);
                if name.is_empty() {
                    (None, None)
                } else if name.chars().all(lex::is_ident_char) {
                    // Mermaid uses the bare word as both id and title.
                    (Some(name.to_string()), Some(lex::label_at(src, name)))
                } else {
                    (None, Some(lex::label_at(src, name)))
                }
            }
        };
        if let Some(key) = &key {
            if key.is_empty() {
                return Err(lex::syntax(line, "subgraph without a name"));
            }
            self.subgraph_keys.push(key.clone());
        }
        self.stack.push(Group {
            key,
            title,
            ..Group::default()
        });
        Ok(())
    }

    /// Closes the innermost `subgraph`.
    fn close_subgraph(&mut self, line: usize) -> Result<(), MermaidError> {
        if self.stack.len() < 2 {
            return Err(lex::syntax(line, "`end` without a matching `subgraph`"));
        }
        let group = self.stack.pop().unwrap_or_default();
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(group);
        }
        Ok(())
    }

    /// Parses a statement of the form `A[x] -->|l| B & C --- D`.
    fn edge_chain(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let mut groups: Vec<Vec<NodeId>> = Vec::new();
        let mut links: Vec<Link> = Vec::new();
        let mut at = 0;
        while at < text.len() {
            match find_link(text, at, line)? {
                Some(link) => {
                    groups.push(self.node_group(text[at..link.start].trim(), line)?);
                    at = link.end;
                    links.push(link);
                }
                None => {
                    groups.push(self.node_group(text[at..].trim(), line)?);
                    break;
                }
            }
        }
        if links.len() + 1 != groups.len() {
            return Err(lex::syntax(line, "link without a target node"));
        }
        for (index, link) in links.into_iter().enumerate() {
            for &from in &groups[index] {
                for &to in &groups[index + 1] {
                    self.edges.push(FlowEdge {
                        from,
                        to,
                        stroke: link.stroke,
                        tail: link.tail,
                        head: link.head,
                        label: link.label.map(|text| lex::label_at(self.src, text)),
                    });
                }
            }
        }
        Ok(())
    }

    /// Parses `A[x] & B(y)` into node ids, declaring nodes as needed.
    fn node_group(&mut self, text: &str, line: usize) -> Result<Vec<NodeId>, MermaidError> {
        let parts = lex::split_top_level(text, '&', Nesting::Honour);
        if parts.is_empty() {
            return Err(lex::syntax(line, "missing node before or after a link"));
        }
        parts
            .into_iter()
            .map(|part| self.node(part, line))
            .collect()
    }

    /// Declares or updates a single node, returning its id.
    fn node(&mut self, text: &str, line: usize) -> Result<NodeId, MermaidError> {
        let src = self.src;
        let (key, rest) = lex::take_ident(text);
        if key.is_empty() {
            return Err(lex::syntax(
                line,
                format!("cannot read a node from `{text}`"),
            ));
        }
        if rest.starts_with('@') {
            return Err(lex::unsupported(line, "`@{ … }` node metadata"));
        }
        let shape = match shape_at(rest, 0) {
            Some(shape) if shape.start == 0 => {
                if !rest[shape.end..].trim().is_empty() {
                    return Err(lex::syntax(
                        line,
                        format!("trailing text after node `{key}`"),
                    ));
                }
                Some(shape)
            }
            _ if rest.is_empty() => None,
            _ => {
                return Err(lex::syntax(
                    line,
                    format!("cannot read a node shape from `{rest}`"),
                ));
            }
        };

        let key_owned = key.to_string();
        let fresh = !self.nodes.iter().any(|node| node.key == key_owned);
        let index = intern(
            &mut self.nodes,
            key,
            |node| node.key.as_str(),
            || FlowNode {
                key: key_owned.clone(),
                label: lex::label_at(src, key),
                shape: NodeShape::Rect,
            },
        );
        if fresh && let Some(group) = self.stack.last_mut() {
            group.nodes.push(NodeId(index));
        }
        // A later declaration upgrades the shape and label of an existing node.
        if let Some(shape) = shape
            && let Some(node) = self.nodes.get_mut(index)
        {
            node.shape = shape.shape;
            let text = lex::unquote(shape.text);
            node.label = lex::label_at(src, text);
        }
        Ok(NodeId(index))
    }

    /// Validates the finished diagram and returns it.
    fn finish(mut self, last_line: usize) -> Result<Flowchart, MermaidError> {
        if self.stack.len() > 1 {
            return Err(lex::syntax(
                last_line,
                "`subgraph` without a matching `end`",
            ));
        }
        if let Some(node) = self
            .nodes
            .iter()
            .find(|node| self.subgraph_keys.contains(&node.key))
        {
            return Err(lex::unsupported(
                last_line,
                format!("`{}` is a subgraph and cannot be used as a node", node.key),
            ));
        }
        if self.nodes.is_empty() {
            // A header with no body is not a diagram. Reporting it means the block
            // renderer shows the source with a caption, which tells the reader more
            // than an empty box would (design spec §6).
            return Err(lex::syntax(last_line, "flowchart has no nodes".to_string()));
        }
        Ok(Flowchart {
            direction: self.direction,
            nodes: self.nodes,
            edges: self.edges,
            root: self.stack.pop().unwrap_or_default(),
        })
    }
}

/// A link operator found in a statement.
#[derive(Debug)]
struct Link<'a> {
    stroke: EdgeStroke,
    tail: ArrowHead,
    head: ArrowHead,
    /// The `|text|` or `-- text -->` label, still a slice of the mermaid source so
    /// `lex::label_at` can recover its offset.
    label: Option<&'a str>,
    /// Byte offset of the first character of the operator.
    start: usize,
    /// Byte offset just past the operator (and its `|label|`, if any).
    end: usize,
}

/// The characters a link operator's line is drawn from.
fn is_line_char(ch: u8) -> bool {
    matches!(ch, b'-' | b'=' | b'.')
}

/// Reads the run of line characters starting at `at`, returning its end offset.
fn line_run(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut end = at;
    while end < bytes.len() && is_line_char(bytes[end]) {
        end += 1;
    }
    end
}

/// The stroke a run of line characters describes.
fn stroke_of(run: &str) -> EdgeStroke {
    if run.contains('=') {
        EdgeStroke::Thick
    } else if run.contains('.') {
        EdgeStroke::Dotted
    } else {
        EdgeStroke::Solid
    }
}

/// Finds the next link operator at or after `from`, at the top level of `text`.
fn find_link(text: &str, from: usize, line: usize) -> Result<Option<Link<'_>>, MermaidError> {
    let bytes = text.as_bytes();
    let mut scanner = lex::Scanner::default();
    for (at, ch) in text.char_indices() {
        let top = scanner.step(ch, Nesting::Honour);
        if !top || at < from {
            continue;
        }
        let tail = if ch == '<' && bytes.get(at + 1).copied().is_some_and(is_line_char) {
            ArrowHead::Arrow
        } else if ch.is_ascii() && is_line_char(ch as u8) {
            ArrowHead::None
        } else {
            continue;
        };
        let run_start = if tail == ArrowHead::Arrow { at + 1 } else { at };
        let run_end = line_run(text, run_start);
        let run = &text[run_start..run_end];
        if run.len() < 2 {
            // A lone `-` or `.` belongs to an identifier such as `my-node`.
            continue;
        }
        return finish_link(text, line, at, run, run_end, tail).map(Some);
    }
    Ok(None)
}

/// Completes a link once its opening run has been read.
fn finish_link<'a>(
    text: &'a str,
    line: usize,
    start: usize,
    run: &str,
    run_end: usize,
    tail: ArrowHead,
) -> Result<Link<'a>, MermaidError> {
    let bytes = text.as_bytes();
    let mut stroke = stroke_of(run);
    let mut label = None;
    let mut head = ArrowHead::None;
    let mut end = run_end;

    match bytes.get(run_end) {
        Some(b'>') => {
            head = ArrowHead::Arrow;
            end = run_end + 1;
        }
        Some(&ch @ (b'x' | b'o'))
            if bytes
                .get(run_end + 1)
                .is_none_or(|next| next.is_ascii_whitespace()) =>
        {
            return Err(lex::unsupported(
                line,
                format!("`{}{}` link terminator", run, ch as char),
            ));
        }
        _ => {
            // Possibly the `-- text -->` form, whose opening run is exactly two
            // characters and whose closing run is `-->`, `---` or the dotted/thick
            // equivalents.
            if run.len() == 2
                && let Some((mid, close_start, close_end, close_head)) = closing_run(text, run_end)
            {
                label = Some(mid);
                head = close_head;
                stroke = stroke_of(&text[close_start..close_end]);
                end = close_end + usize::from(close_head == ArrowHead::Arrow);
            }
        }
    }

    // The `-->|text|` form.
    let bar = end + (text.len() - end - text[end..].trim_start().len());
    if text[bar..].starts_with('|') {
        match text[bar + 1..].find('|') {
            Some(close) => {
                label = Some(&text[bar + 1..bar + 1 + close]);
                end = bar + 1 + close + 1;
            }
            None => return Err(lex::syntax(line, "unterminated `|` edge label")),
        }
    }

    Ok(Link {
        stroke,
        tail,
        head,
        label: label.map(lex::unquote),
        start,
        end,
    })
}

/// Looks for the closing run of a `-- text -->` style link.
///
/// Returns the label text, the closing run's byte range, and its arrow head.
fn closing_run(text: &str, from: usize) -> Option<(&str, usize, usize, ArrowHead)> {
    let bytes = text.as_bytes();
    let mut scanner = lex::Scanner::default();
    for (at, ch) in text.char_indices() {
        let top = scanner.step(ch, Nesting::Honour);
        if !top || at < from || !is_line_char(ch as u8) {
            continue;
        }
        let end = line_run(text, at);
        let run = &text[at..end];
        if run.len() < 2 {
            continue;
        }
        let head = if bytes.get(end) == Some(&b'>') {
            ArrowHead::Arrow
        } else if run.len() >= 3 {
            ArrowHead::None
        } else {
            continue;
        };
        let mid = text[from..at].trim();
        if mid.is_empty() {
            return None;
        }
        return Some((mid, at, end, head));
    }
    None
}

/// A bracketed shape following a node identifier.
#[derive(Debug)]
struct Shape<'a> {
    shape: NodeShape,
    /// The inner label text, brackets and decorations removed. Still a slice of the
    /// mermaid source, so `lex::label_at` can recover its offset.
    text: &'a str,
    /// Byte offset of the opening bracket.
    start: usize,
    /// Byte offset just past the closing bracket.
    end: usize,
}

/// Reads the bracketed shape starting at or after `from`, if any.
///
/// Bracket forms outside design spec §6.1 keep their label and degrade to
/// [`NodeShape::Rect`].
fn shape_at(text: &str, from: usize) -> Option<Shape<'_>> {
    let bytes = text.as_bytes();
    let start = (from..text.len())
        .find(|&at| text.is_char_boundary(at) && matches!(bytes[at], b'[' | b'(' | b'{' | b'>'))?;
    let mut depth = 0i32;
    let mut quoted = false;
    let mut end = None;
    for (at, ch) in text[start..].char_indices() {
        let at = start + at;
        if quoted {
            if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => {
                depth -= 1;
                if depth <= 0 {
                    end = Some(at + ch.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let whole = &text[start..end];
    let (shape, inner) = classify_shape(whole);
    Some(Shape {
        shape,
        text: inner,
        start,
        end,
    })
}

/// Maps a bracketed token such as `([Go])` to a shape and its inner text.
fn classify_shape(whole: &str) -> (NodeShape, &str) {
    let inner_of = |open: usize, close: usize| &whole[open..whole.len() - close];
    let pairs: [(&str, &str, NodeShape); 5] = [
        ("([", "])", NodeShape::Stadium),
        ("[[", "]]", NodeShape::Subroutine),
        ("[(", ")]", NodeShape::Cylinder),
        ("((", "))", NodeShape::Circle),
        ("{{", "}}", NodeShape::Rect), // hexagon degrades to a rectangle
    ];
    for (open, close, shape) in pairs {
        if whole.len() > open.len() + close.len()
            && whole.starts_with(open)
            && whole.ends_with(close)
        {
            return (shape, inner_of(open.len(), close.len()).trim());
        }
    }
    let shape = match whole.as_bytes().first() {
        Some(b'(') => NodeShape::Round,
        Some(b'{') => NodeShape::Rhombus,
        // `[text]`, `[/text/]`, `[\text\]`, `>text]` all degrade to a rectangle.
        _ => NodeShape::Rect,
    };
    let inner = inner_of(1, 1).trim();
    let inner = inner
        .trim_start_matches(['/', '\\'])
        .trim_end_matches(['/', '\\'])
        .trim();
    (shape, inner)
}
