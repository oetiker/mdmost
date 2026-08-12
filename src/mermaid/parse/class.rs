//! `classDiagram` parsing (design spec §6.3).
//!
//! Supported: `class X { … }` member blocks and the equivalent `X : +int age` line
//! form, visibility markers `+ - # ~`, `$`/`*` classifiers, `<<interface>>` style
//! annotations (inside or outside the block), generics written `List~T~`, `direction`,
//! and all six relation operators with quoted cardinalities and a `: label`.
//!
//! Skipped silently: `click`, `style`, `classDef`, `cssClass`, `callback`, `link`.
//!
//! Rejected with a reason: `note` statements and namespaces, which would otherwise be
//! dropped from the drawing without the reader noticing.

use crate::error::MermaidError;
use crate::mermaid::ast::{
    Class, ClassAnnotation, ClassArrow, ClassDiagram, ClassId, ClassRelation, Classifier, Field,
    LineStyle, Member, Method, Param, Visibility,
};

use super::lex::{self, Nesting, SrcLine};
use super::{direction, intern};

/// Parses a whole `classDiagram`.
///
/// `src` is the full mermaid source `lines` was lexed from; it is kept only so that
/// label text — always a subslice of it — can report where it came from.
pub fn parse<'a>(lines: &[SrcLine<'a>], src: &'a str) -> Result<ClassDiagram, MermaidError> {
    let Some((header, body)) = lines.split_first() else {
        return Err(lex::syntax(1, "empty diagram"));
    };
    let mut builder = Builder {
        src,
        ..Builder::default()
    };
    let (_, rest) = lex::split_word(header.text);
    if !rest.is_empty() {
        builder.line(rest, header.number)?;
    }
    for line in body {
        builder.line(line.text, line.number)?;
    }
    if let Some(open) = builder.open {
        return Err(lex::syntax(open, "`class` block without a closing `}`"));
    }
    Ok(ClassDiagram {
        direction: builder.direction,
        classes: builder.classes,
        relations: builder.relations,
    })
}

/// Accumulates classes and relations.
#[derive(Debug, Default)]
struct Builder<'a> {
    direction: Option<crate::mermaid::ast::Direction>,
    classes: Vec<Class>,
    relations: Vec<ClassRelation>,
    /// The class whose `{ … }` block is currently open, and the line it opened on.
    current: Option<ClassId>,
    open: Option<usize>,
    /// The full mermaid source, passed to `lex::label_at` to compute a label's byte
    /// offset — every label text this parser touches is a subslice of it.
    src: &'a str,
}

impl Builder<'_> {
    /// Handles one source line, which may hold several `;`-separated statements.
    fn line(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        if self.open.is_some() {
            return self.block_line(text, line);
        }
        for statement in lex::split_top_level(text, ';', Nesting::Honour) {
            self.statement(statement, line)?;
        }
        Ok(())
    }

    /// Handles a line inside an open `class X { … }` block.
    fn block_line(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        for statement in lex::split_top_level(text, ';', Nesting::Honour) {
            // A class written on one line: `class A { +f() }`.
            let (statement, closes) = match statement.strip_suffix('}') {
                Some(prefix) => (prefix.trim(), true),
                None => (statement, false),
            };
            if !statement.is_empty() {
                let Some(id) = self.current else {
                    return Err(lex::syntax(line, "member outside a class block"));
                };
                self.member(id, statement, line)?;
            }
            if closes {
                self.open = None;
                self.current = None;
            }
        }
        Ok(())
    }

    /// Handles one top-level statement.
    fn statement(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let (word, rest) = lex::split_word(text);
        match word.to_ascii_lowercase().as_str() {
            "click" | "style" | "classdef" | "cssclass" | "callback" | "link" => return Ok(()),
            "note" => return Err(lex::unsupported(line, "`note` statements")),
            "namespace" => return Err(lex::unsupported(line, "`namespace` blocks")),
            "direction" => {
                self.direction = Some(
                    direction(lex::split_word(rest).0)
                        .ok_or_else(|| lex::syntax(line, format!("unknown direction `{rest}`")))?,
                );
                return Ok(());
            }
            "class" => return self.declare(rest, line),
            _ => {}
        }
        // A bare `<<interface>> Shape` annotation line.
        if let Some(rest) = text.strip_prefix("<<")
            && let Some((annotation, name)) = rest.split_once(">>")
        {
            let id = self.intern_class(name.trim());
            if let Some(class) = self.classes.get_mut(id.0) {
                class.annotation = Some(annotation_of(annotation.trim()));
            }
            return Ok(());
        }
        if let Some(relation) = self.relation(text, line)? {
            self.relations.push(relation);
            return Ok(());
        }
        // The `Animal : +int age` member form.
        if let Some((name, member)) = lex::split_once_top_level(text, ':', Nesting::Honour) {
            let id = self.intern_class(name);
            return self.member(id, member, line);
        }
        Err(lex::syntax(
            line,
            format!("cannot read a statement from `{text}`"),
        ))
    }

    /// Handles `class Foo`, optionally opening a `{ … }` member block.
    fn declare(&mut self, rest: &str, line: usize) -> Result<(), MermaidError> {
        let (head, body) = match rest.split_once('{') {
            Some((head, body)) => (head.trim(), Some(body)),
            None => (rest.trim(), None),
        };
        let (name, annotation) = lex::split_stereotype(head, line)?;
        if name.is_empty() {
            return Err(lex::syntax(line, "`class` without a name"));
        }
        let id = self.intern_class(name);
        if let Some(annotation) = annotation
            && let Some(class) = self.classes.get_mut(id.0)
        {
            class.annotation = Some(annotation_of(annotation));
        }
        if let Some(body) = body {
            self.current = Some(id);
            self.open = Some(line);
            self.block_line(body, line)?;
        }
        Ok(())
    }

    /// Parses one member of a class.
    fn member(&mut self, id: ClassId, text: &str, line: usize) -> Result<(), MermaidError> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        if let Some(rest) = text.strip_prefix("<<")
            && let Some((annotation, _)) = rest.split_once(">>")
        {
            if let Some(class) = self.classes.get_mut(id.0) {
                class.annotation = Some(annotation_of(annotation.trim()));
            }
            return Ok(());
        }
        let member = parse_member(text, line)?;
        if let Some(class) = self.classes.get_mut(id.0) {
            class.members.push(member);
        }
        Ok(())
    }

    /// Parses a relation statement, returning `None` when `text` holds no operator.
    fn relation(&mut self, text: &str, line: usize) -> Result<Option<ClassRelation>, MermaidError> {
        let (body, label) = match lex::split_once_top_level(text, ':', Nesting::Honour) {
            Some((body, label)) => (body, Some(label)),
            None => (text, None),
        };
        let Some(operator) = find_operator(body) else {
            return Ok(None);
        };
        let (left, left_cardinality) = split_cardinality(&body[..operator.start], Side::Left);
        let (right, right_cardinality) = split_cardinality(&body[operator.end..], Side::Right);
        if left.is_empty() || right.is_empty() {
            return Err(lex::syntax(line, "relation without both classes"));
        }
        let left = self.intern_class(left);
        let right = self.intern_class(right);
        Ok(Some(ClassRelation {
            left,
            right,
            left_end: operator.left_end,
            right_end: operator.right_end,
            line: operator.line,
            left_cardinality: left_cardinality.map(str::to_string),
            right_cardinality: right_cardinality.map(str::to_string),
            label: label.map(|label| lex::label_at(self.src, lex::unquote(label))),
        }))
    }

    /// Interns a class, splitting a `Square~Shape~` generic parameter off the name.
    ///
    /// Mermaid identifies such a class as `Square`, so the generic must not become
    /// part of the key: `class Square~Shape~` and a later `<<abstract>> Square` are
    /// the same class.
    fn intern_class(&mut self, text: &str) -> ClassId {
        let text = lex::unquote(text.trim());
        let (name, generic) = match text.split_once('~') {
            Some((name, rest)) => (
                name.trim(),
                Some(normalise_generics(rest.trim_end().trim_end_matches('~'))),
            ),
            None => (text, None),
        };
        // Keyed on the label's visible first line, not on the raw source text: see
        // `er::Builder::intern_entity` for why the two can differ and why this is the
        // one an author means.
        let label = lex::label_at(self.src, name);
        let key = label.lines.first().cloned().unwrap_or_default();
        let id = ClassId(intern(
            &mut self.classes,
            &key,
            |class| class.name.lines.first().map_or("", String::as_str),
            || Class {
                name: label.clone(),
                generic: None,
                annotation: None,
                members: Vec::new(),
            },
        ));
        if let Some(generic) = generic
            && let Some(class) = self.classes.get_mut(id.0)
        {
            class.generic = Some(generic);
        }
        id
    }
}

/// Rewrites Mermaid's `~T~` generic markers as angle brackets.
fn normalise_generics(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut open = false;
    for ch in text.chars() {
        if ch == '~' {
            out.push(if open { '>' } else { '<' });
            open = !open;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Maps a stereotype word to a [`ClassAnnotation`].
fn annotation_of(text: &str) -> ClassAnnotation {
    match text.to_ascii_lowercase().as_str() {
        "interface" => ClassAnnotation::Interface,
        "abstract" => ClassAnnotation::Abstract,
        "enumeration" | "enum" => ClassAnnotation::Enumeration,
        "service" => ClassAnnotation::Service,
        _ => ClassAnnotation::Other(text.to_string()),
    }
}

/// Which side of a relation operator a fragment came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

/// Splits `Animal "1"` into the class name and its quoted cardinality.
fn split_cardinality(text: &str, side: Side) -> (&str, Option<&str>) {
    let text = text.trim();
    let Some(open) = text.find('"') else {
        return (text, None);
    };
    let rest = &text[open + 1..];
    let Some(close) = rest.find('"') else {
        return (text, None);
    };
    let cardinality = rest[..close].trim();
    let name = match side {
        // `Animal "1" <|-- "0..*" Duck`: the quote sits next to the operator.
        Side::Left => text[..open].trim(),
        Side::Right => rest[close + 1..].trim(),
    };
    (name, Some(cardinality))
}

/// A relation operator such as `<|--` or `..>`.
#[derive(Debug)]
struct Operator {
    left_end: ClassArrow,
    right_end: ClassArrow,
    line: LineStyle,
    start: usize,
    end: usize,
}

/// Finds the relation operator in `text`, if there is one.
fn find_operator(text: &str) -> Option<Operator> {
    let bytes = text.as_bytes();
    let mut scanner = lex::Scanner::default();
    for (at, ch) in text.char_indices() {
        if !scanner.step(ch, Nesting::Honour) {
            continue;
        }
        let rest = &text[at..];
        let line = if rest.starts_with("--") {
            LineStyle::Solid
        } else if rest.starts_with("..") {
            LineStyle::Dashed
        } else {
            continue;
        };
        let mut start = at;
        let mut left_end = ClassArrow::None;
        if at.checked_sub(2).and_then(|from| text.get(from..at)) == Some("<|") {
            left_end = ClassArrow::Triangle;
            start = at - 2;
        } else if at >= 1 {
            match bytes[at - 1] {
                b'*' => (left_end, start) = (ClassArrow::FilledDiamond, at - 1),
                b'o' => (left_end, start) = (ClassArrow::HollowDiamond, at - 1),
                b'<' => (left_end, start) = (ClassArrow::Arrow, at - 1),
                _ => {}
            }
        }
        // A run of `-` or `.` may be longer than two characters.
        let mut end = at;
        while end < bytes.len() && (bytes[end] == b'-' || bytes[end] == b'.') {
            end += 1;
        }
        let mut right_end = ClassArrow::None;
        if text[end..].starts_with("|>") {
            right_end = ClassArrow::Triangle;
            end += 2;
        } else {
            match bytes.get(end) {
                Some(b'*') => (right_end, end) = (ClassArrow::FilledDiamond, end + 1),
                Some(b'o') => (right_end, end) = (ClassArrow::HollowDiamond, end + 1),
                Some(b'>') => (right_end, end) = (ClassArrow::Arrow, end + 1),
                _ => {}
            }
        }
        return Some(Operator {
            left_end,
            right_end,
            line,
            start,
            end,
        });
    }
    None
}

/// Parses a member such as `+int age`, `+age: int` or `+isMammal() bool`.
fn parse_member(text: &str, line: usize) -> Result<Member, MermaidError> {
    let text = normalise_generics(text.trim());
    let mut rest = text.as_str();
    let visibility = match rest.as_bytes().first() {
        Some(b'+') => Some(Visibility::Public),
        Some(b'-') => Some(Visibility::Private),
        Some(b'#') => Some(Visibility::Protected),
        Some(b'~') => Some(Visibility::PackageInternal),
        _ => None,
    };
    if visibility.is_some() {
        rest = rest[1..].trim();
    }
    let (rest, classifier) = split_classifier(rest);

    match rest.find('(') {
        Some(open) => {
            let close = rest
                .rfind(')')
                .ok_or_else(|| lex::syntax(line, format!("unbalanced `(` in member `{text}`")))?;
            if close < open {
                return Err(lex::syntax(
                    line,
                    format!("unbalanced `)` in member `{text}`"),
                ));
            }
            let params = lex::split_top_level(&rest[open + 1..close], ',', Nesting::Honour)
                .into_iter()
                .map(parse_param)
                .collect();
            let returns = rest[close + 1..].trim().trim_start_matches(':').trim();
            Ok(Member::Method(Method {
                visibility,
                name: rest[..open].trim().to_string(),
                params,
                returns: (!returns.is_empty()).then(|| returns.to_string()),
                classifier,
            }))
        }
        None => {
            let (name, ty) = split_typed(rest);
            if name.is_empty() {
                return Err(lex::syntax(line, format!("empty member in `{text}`")));
            }
            Ok(Member::Field(Field {
                visibility,
                name: name.to_string(),
                ty: ty.map(str::to_string),
                classifier,
            }))
        }
    }
}

/// Splits a trailing `$` (static) or `*` (abstract) classifier off a member.
fn split_classifier(text: &str) -> (&str, Option<Classifier>) {
    let text = text.trim();
    let classifier = match text.as_bytes().last() {
        Some(b'$') => Classifier::Static,
        Some(b'*') => Classifier::Abstract,
        _ => return (text, None),
    };
    (text[..text.len() - 1].trim_end(), Some(classifier))
}

/// Splits `int age` or `age: int` into the name and its type.
fn split_typed(text: &str) -> (&str, Option<&str>) {
    let text = text.trim();
    if let Some((name, ty)) = text.split_once(':') {
        return (name.trim(), Some(ty.trim()).filter(|ty| !ty.is_empty()));
    }
    match text.rsplit_once(char::is_whitespace) {
        Some((ty, name)) => (name.trim(), Some(ty.trim())),
        None => (text, None),
    }
}

/// Parses one parameter of a method signature.
fn parse_param(text: &str) -> Param {
    let (name, ty) = split_typed(text);
    Param {
        name: name.to_string(),
        ty: ty.map(str::to_string),
    }
}
