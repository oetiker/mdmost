//! `erDiagram` parsing (design spec §6.4).
//!
//! Supported: entity declarations with `{ type name PK "comment" }` attribute blocks,
//! `ENTITY["alias"]` aliases, every crow's-foot cardinality pair (`||--o{`, `}o--||`,
//! `||--||`, `}|..|{`, …) with both the identifying (`--`) and non-identifying (`..`)
//! line styles, and an optional `: label`.
//!
//! Skipped silently: `style`, `classDef`, `class`, `click`, `direction`.

use crate::error::MermaidError;
use crate::mermaid::ast::{
    Entity, EntityId, ErAttribute, ErCardinality, ErDiagram, ErKey, ErRelationship, Label,
    LineStyle,
};

use super::intern;
use super::lex::{self, Nesting, SrcLine};

/// Parses a whole `erDiagram`.
pub fn parse(lines: &[SrcLine<'_>]) -> Result<ErDiagram, MermaidError> {
    let Some((header, body)) = lines.split_first() else {
        return Err(lex::syntax(1, "empty diagram"));
    };
    let mut builder = Builder::default();
    let (_, rest) = lex::split_word(header.text);
    if !rest.is_empty() {
        builder.line(rest, header.number)?;
    }
    for line in body {
        builder.line(line.text, line.number)?;
    }
    if let Some(open) = builder.open {
        return Err(lex::syntax(open, "attribute block without a closing `}`"));
    }
    Ok(ErDiagram {
        entities: builder.entities,
        relationships: builder.relationships,
    })
}

/// Accumulates entities and relationships.
#[derive(Debug, Default)]
struct Builder {
    entities: Vec<Entity>,
    relationships: Vec<ErRelationship>,
    /// The entity whose attribute block is open, and the line it opened on.
    current: Option<EntityId>,
    open: Option<usize>,
}

impl Builder {
    /// Handles one source line.
    fn line(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        if self.open.is_some() {
            return self.attribute_line(text, line);
        }
        let (word, _) = lex::split_word(text);
        if matches!(
            word.to_ascii_lowercase().as_str(),
            "style" | "classdef" | "class" | "click" | "direction"
        ) {
            return Ok(());
        }
        if let Some(relationship) = self.relationship(text, line)? {
            self.relationships.push(relationship);
            return Ok(());
        }
        self.entity_declaration(text, line)
    }

    /// Handles a line inside an open `{ … }` attribute block.
    fn attribute_line(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        for statement in lex::split_top_level(text, ',', Nesting::Ignore) {
            if statement == "}" {
                self.open = None;
                self.current = None;
                continue;
            }
            let Some(id) = self.current else {
                return Err(lex::syntax(line, "attribute outside an entity"));
            };
            let attribute = parse_attribute(statement, line)?;
            if let Some(entity) = self.entities.get_mut(id.0) {
                entity.attributes.push(attribute);
            }
        }
        Ok(())
    }

    /// Handles `CUSTOMER`, `CUSTOMER["Customer"]` and `CUSTOMER {`.
    fn entity_declaration(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let (head, body) = match text.split_once('{') {
            Some((head, body)) => (head.trim(), Some(body)),
            None => (text.trim(), None),
        };
        if split_alias(head).0.is_empty() {
            return Err(lex::syntax(
                line,
                format!("cannot read an entity from `{text}`"),
            ));
        }
        let id = self.entity_ref(head);
        if let Some(body) = body {
            self.current = Some(id);
            self.open = Some(line);
            self.attribute_line(body, line)?;
        }
        Ok(())
    }

    /// Parses a relationship statement, returning `None` when there is no operator.
    fn relationship(
        &mut self,
        text: &str,
        line: usize,
    ) -> Result<Option<ErRelationship>, MermaidError> {
        let (body, label) = match lex::split_once_top_level(text, ':', Nesting::Ignore) {
            Some((body, label)) => (body, Some(label)),
            None => (text, None),
        };
        let Some(operator) = find_operator(body, line)? else {
            return Ok(None);
        };
        let left = body[..operator.start].trim();
        let right = body[operator.end..].trim();
        if left.is_empty() || right.is_empty() {
            return Err(lex::syntax(line, "relationship without both entities"));
        }
        let left = self.entity_ref(left);
        let right = self.entity_ref(right);
        let label = label
            .map(lex::unquote)
            .filter(|label| !label.is_empty())
            .map(Label::parse);
        Ok(Some(ErRelationship {
            left,
            right,
            left_cardinality: operator.left,
            right_cardinality: operator.right,
            line: operator.line,
            label,
        }))
    }

    /// Resolves an entity reference, recording the alias when one is written.
    fn entity_ref(&mut self, text: &str) -> EntityId {
        let (name, alias) = split_alias(text);
        let id = self.intern_entity(lex::unquote(name));
        if let Some(alias) = alias
            && let Some(entity) = self.entities.get_mut(id.0)
        {
            entity.alias = Some(alias.to_string());
        }
        id
    }

    /// Interns an entity name.
    fn intern_entity(&mut self, name: &str) -> EntityId {
        EntityId(intern(
            &mut self.entities,
            name,
            |entity| entity.name.as_str(),
            || Entity {
                name: name.to_string(),
                alias: None,
                attributes: Vec::new(),
            },
        ))
    }
}

/// Splits `CUSTOMER["Customer account"]` into its name and its alias.
fn split_alias(text: &str) -> (&str, Option<&str>) {
    match text.split_once('[') {
        Some((name, alias)) => (
            name.trim(),
            Some(lex::unquote(alias.trim().trim_end_matches(']'))),
        ),
        None => (text.trim(), None),
    }
}

/// A crow's-foot operator such as `||--o{`.
#[derive(Debug)]
struct Operator {
    left: ErCardinality,
    right: ErCardinality,
    line: LineStyle,
    start: usize,
    end: usize,
}

/// Finds the crow's-foot operator in `text`, if there is one.
fn find_operator(text: &str, line: usize) -> Result<Option<Operator>, MermaidError> {
    let bytes = text.as_bytes();
    let mut scanner = lex::Scanner::default();
    for (at, ch) in text.char_indices() {
        if !scanner.step(ch, Nesting::Ignore) {
            continue;
        }
        let rest = &text[at..];
        let style = if rest.starts_with("--") {
            LineStyle::Solid
        } else if rest.starts_with("..") {
            LineStyle::Dashed
        } else {
            continue;
        };
        let left_text = at
            .checked_sub(2)
            .and_then(|from| text.get(from..at))
            .ok_or_else(|| lex::syntax(line, "relationship without a left cardinality"))?;
        let left = left_cardinality(left_text)
            .ok_or_else(|| lex::syntax(line, format!("unknown cardinality `{left_text}`")))?;
        let mut end = at;
        while end < bytes.len() && (bytes[end] == b'-' || bytes[end] == b'.') {
            end += 1;
        }
        let right_text = text
            .get(end..end + 2)
            .ok_or_else(|| lex::syntax(line, "relationship without a right cardinality"))?;
        let right = right_cardinality(right_text)
            .ok_or_else(|| lex::syntax(line, format!("unknown cardinality `{right_text}`")))?;
        return Ok(Some(Operator {
            left,
            right,
            line: style,
            start: at - 2,
            end: end + 2,
        }));
    }
    Ok(None)
}

/// Maps the two characters left of the line to a cardinality.
fn left_cardinality(text: &str) -> Option<ErCardinality> {
    match text {
        "||" => Some(ErCardinality::ExactlyOne),
        "|o" => Some(ErCardinality::ZeroOrOne),
        "}o" => Some(ErCardinality::ZeroOrMore),
        "}|" => Some(ErCardinality::OneOrMore),
        _ => None,
    }
}

/// Maps the two characters right of the line to a cardinality.
fn right_cardinality(text: &str) -> Option<ErCardinality> {
    match text {
        "||" => Some(ErCardinality::ExactlyOne),
        "o|" => Some(ErCardinality::ZeroOrOne),
        "o{" => Some(ErCardinality::ZeroOrMore),
        "|{" => Some(ErCardinality::OneOrMore),
        _ => None,
    }
}

/// Parses one attribute line such as `string name PK "the name"`.
fn parse_attribute(text: &str, line: usize) -> Result<ErAttribute, MermaidError> {
    let (body, comment) = match text.find('"') {
        Some(open) => {
            let rest = &text[open + 1..];
            let close = rest
                .find('"')
                .ok_or_else(|| lex::syntax(line, "unterminated attribute comment"))?;
            (text[..open].trim(), Some(rest[..close].to_string()))
        }
        None => (text.trim(), None),
    };
    let mut words = body.split_whitespace();
    let (Some(ty), Some(name)) = (words.next(), words.next()) else {
        return Err(lex::syntax(
            line,
            format!("attribute `{text}` needs a type and a name"),
        ));
    };
    let mut keys = Vec::new();
    for word in words {
        let key = match word.to_ascii_uppercase().as_str() {
            "PK" => ErKey::Primary,
            "FK" => ErKey::Foreign,
            "UK" => ErKey::Unique,
            other => {
                return Err(lex::syntax(
                    line,
                    format!("unknown attribute key `{other}`, expected PK, FK or UK"),
                ));
            }
        };
        keys.push(key);
    }
    Ok(ErAttribute {
        ty: ty.to_string(),
        name: name.to_string(),
        keys,
        comment,
    })
}
