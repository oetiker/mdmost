//! `stateDiagram-v2` parsing (design spec §6.7).
//!
//! Supported: `[*] --> S` start and end markers (per scope), `S --> T : label`,
//! `state "description" as s`, `s : description`, `state X { … }` composite states to
//! any depth, the `<<choice>>`, `<<fork>>` and `<<join>>` stereotypes, `direction`, and
//! `note left of X` / `note right of X` in both the inline and the `end note` form.
//!
//! Skipped silently: `classDef`, `class`, `style`, `click`.
//!
//! Rejected with a reason: the `--` concurrency divider, because silently dropping it
//! would draw parallel regions as one.

use crate::error::MermaidError;
use crate::mermaid::ast::{
    Direction, Label, NotePlacement, StateDiagram, StateEndpoint, StateId, StateKind, StateNode,
    StateNote, StateScope, Transition,
};

use super::lex::{self, Nesting, SrcLine};
use super::{direction, intern};

/// Parses a whole `stateDiagram` / `stateDiagram-v2`.
///
/// `src` is the full mermaid source `lines` was lexed from; it is kept only so that
/// label text — always a subslice of it — can report where it came from.
pub fn parse<'a>(lines: &[SrcLine<'a>], src: &'a str) -> Result<StateDiagram, MermaidError> {
    let Some((header, body)) = lines.split_first() else {
        return Err(lex::syntax(1, "empty diagram"));
    };
    let mut builder = Builder {
        stack: vec![(None, StateScope::default())],
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
    builder.finish(lines.last().map_or(1, |line| line.number))
}

/// A note whose body runs to a later `end note`.
#[derive(Debug)]
struct PendingNote {
    placement: NotePlacement,
    target: StateId,
    lines: Vec<String>,
    /// A hull over every line accumulated so far: from the first line's start to the
    /// last line's end, so it also covers the newlines, per-line indentation and any
    /// blank or `%%` line `preprocess` dropped in between — not only the note's own
    /// text. `None` until the first line arrives. Grown on each push rather than
    /// recovered from `lines` afterwards, because by then the text has been copied
    /// into owned `String`s and lost its position.
    source: Option<std::ops::Range<usize>>,
    line: usize,
}

/// Accumulates the state arena and the scope stack.
#[derive(Debug, Default)]
struct Builder<'a> {
    direction: Option<Direction>,
    states: Vec<StateNode>,
    /// Open scopes; the last entry is the innermost. Entry 0 is the diagram root and
    /// has no owning state.
    stack: Vec<(Option<StateId>, StateScope)>,
    note: Option<PendingNote>,
    /// The full mermaid source, passed to `lex::label_at` (and, for the multi-line
    /// note form, `lex::offset_of` directly) to compute a label's byte offset — every
    /// label text this parser touches is a subslice of it.
    src: &'a str,
}

impl Builder<'_> {
    /// Handles one source line, which may hold several `;`-separated statements.
    fn line(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        if self.note.is_some() {
            if text.eq_ignore_ascii_case("end note") {
                // Taking the note here — rather than borrowing it first and taking it
                // afterwards — keeps the function total, with no `unreachable!`.
                if let Some(note) = self.note.take() {
                    self.scope().notes.push(StateNote {
                        placement: note.placement,
                        target: note.target,
                        text: Label {
                            lines: note.lines,
                            source: note.source.unwrap_or_default(),
                        },
                    });
                }
            } else if let Some(note) = self.note.as_mut() {
                // A line that fails `offset_of` (should never happen — `text` is
                // always a slice of `self.src`) contributes no position information
                // rather than a wrong one: `note.source` simply stops growing, so the
                // final range covers only the lines that did check out.
                if let Some(at) = lex::offset_of(self.src, text) {
                    let end = at + text.len();
                    note.source = Some(match note.source.take() {
                        Some(range) => range.start..end,
                        None => at..end,
                    });
                }
                note.lines.push(text.to_string());
            }
            return Ok(());
        }
        for statement in lex::split_top_level(text, ';', Nesting::Honour) {
            self.statement(statement, line)?;
        }
        Ok(())
    }

    /// Handles one statement.
    fn statement(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let (word, rest) = lex::split_word(text);
        match word.to_ascii_lowercase().as_str() {
            "classdef" | "class" | "style" | "click" => return Ok(()),
            "direction" => {
                let dir = direction(lex::split_word(rest).0)
                    .ok_or_else(|| lex::syntax(line, format!("unknown direction `{rest}`")))?;
                if self.stack.len() == 1 {
                    self.direction = Some(dir);
                }
                self.scope().direction = Some(dir);
                return Ok(());
            }
            "note" => return self.note(rest, line),
            "state" => return self.declare(rest, line),
            _ => {}
        }
        if text == "--" {
            return Err(lex::unsupported(line, "the `--` concurrency divider"));
        }
        if text == "}" {
            return self.close(line);
        }
        // A composite state written on one line: `state A { B --> C }`.
        if self.stack.len() > 1
            && let Some(inner) = text.strip_suffix('}')
        {
            let inner = inner.trim();
            if !inner.is_empty() {
                self.statement(inner, line)?;
            }
            return self.close(line);
        }
        if let Some(rest) = text.strip_suffix('{') {
            // `X {` without the `state` keyword is not legal Mermaid, but `state X {`
            // has already been handled; anything else here is a syntax error.
            let _ = rest;
            return Err(lex::syntax(line, "`{` without a `state` declaration"));
        }
        if lex::find_top_level(text, "-->", Nesting::Honour).is_some() {
            return self.transition(text, line);
        }
        // `s2 : This is a description`.
        if let Some((key, description)) = lex::split_once_top_level(text, ':', Nesting::Honour) {
            let id = self.intern_state(key);
            let description = lex::unquote(description);
            if let Some(state) = self.states.get_mut(id.0) {
                state.label = Some(lex::label_at(self.src, description));
            }
            return Ok(());
        }
        if text.chars().all(lex::is_ident_char) {
            self.intern_state(text);
            return Ok(());
        }
        Err(lex::syntax(
            line,
            format!("cannot read a statement from `{text}`"),
        ))
    }

    /// Handles `state "desc" as id`, `state id <<choice>>` and `state id { … }`.
    fn declare(&mut self, rest: &str, line: usize) -> Result<(), MermaidError> {
        let (head, body) = match rest.split_once('{') {
            Some((head, body)) => (head.trim(), Some(body)),
            None => (rest.trim(), None),
        };
        let (head, stereotype) = lex::split_stereotype(head, line)?;
        let (key, description) = match lex::split_as(head) {
            // `state "Long description" as s2`
            Some((description, key)) => (key, Some(description)),
            None => (head, None),
        };
        let key = lex::unquote(key);
        if key.is_empty() {
            return Err(lex::syntax(line, "`state` without a name"));
        }
        let id = self.intern_state(key);
        if let Some(description) = description
            && let Some(state) = self.states.get_mut(id.0)
        {
            let description = lex::unquote(description);
            state.label = Some(lex::label_at(self.src, description));
        }
        if let Some(stereotype) = stereotype {
            let kind = match stereotype.to_ascii_lowercase().as_str() {
                "choice" => StateKind::Choice,
                "fork" => StateKind::Fork,
                "join" => StateKind::Join,
                other => {
                    return Err(lex::unsupported(line, format!("`<<{other}>>` stereotype")));
                }
            };
            if let Some(state) = self.states.get_mut(id.0) {
                state.kind = kind;
            }
        }
        if let Some(body) = body {
            self.stack.push((Some(id), StateScope::default()));
            let body = body.trim();
            if !body.is_empty() {
                self.line(body, line)?;
            }
        }
        Ok(())
    }

    /// Closes the innermost composite state.
    fn close(&mut self, line: usize) -> Result<(), MermaidError> {
        if self.stack.len() < 2 {
            return Err(lex::syntax(line, "`}` without a matching `state … {`"));
        }
        let Some((owner, scope)) = self.stack.pop() else {
            return Err(lex::syntax(line, "`}` without a matching `state … {`"));
        };
        if let Some(owner) = owner
            && let Some(state) = self.states.get_mut(owner.0)
        {
            state.kind = StateKind::Composite(scope);
        }
        Ok(())
    }

    /// Handles a `note left of X` / `note right of X` statement.
    fn note(&mut self, rest: &str, line: usize) -> Result<(), MermaidError> {
        let placed = lex::split_note_placement(rest)
            .filter(|(placement, _)| *placement != NotePlacement::Over);
        let Some((placement, after)) = placed else {
            return Err(lex::unsupported(
                line,
                "notes other than `left of` and `right of`",
            ));
        };
        let (target, text) = match lex::split_once_top_level(after, ':', Nesting::Honour) {
            Some((target, text)) => (target, Some(text)),
            None => (after.trim(), None),
        };
        let target = self.intern_state(lex::unquote(target));
        match text {
            Some(text) => {
                let text = lex::unquote(text);
                let note = StateNote {
                    placement,
                    target,
                    text: lex::label_at(self.src, text),
                };
                self.scope().notes.push(note);
            }
            None => {
                self.note = Some(PendingNote {
                    placement,
                    target,
                    lines: Vec::new(),
                    source: None,
                    line,
                });
            }
        }
        Ok(())
    }

    /// Handles `a --> b : label`, including the `[*]` markers.
    fn transition(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let (body, label) = match lex::split_once_top_level(text, ':', Nesting::Honour) {
            Some((body, label)) => (body, Some(label)),
            None => (text, None),
        };
        let Some(at) = lex::find_top_level(body, "-->", Nesting::Honour) else {
            return Err(lex::syntax(line, "transition without `-->`"));
        };
        let from = self.endpoint(body[..at].trim(), true, line)?;
        let to = self.endpoint(body[at + 3..].trim(), false, line)?;
        let transition = Transition {
            from,
            to,
            label: label
                .map(lex::unquote)
                .filter(|label| !label.is_empty())
                .map(|label| lex::label_at(self.src, label)),
        };
        self.scope().transitions.push(transition);
        Ok(())
    }

    /// Resolves one end of a transition.
    fn endpoint(
        &mut self,
        text: &str,
        is_source: bool,
        line: usize,
    ) -> Result<StateEndpoint, MermaidError> {
        if text == "[*]" {
            return Ok(if is_source {
                StateEndpoint::Initial
            } else {
                StateEndpoint::Final
            });
        }
        let key = lex::unquote(text);
        if key.is_empty() {
            return Err(lex::syntax(line, "transition without both states"));
        }
        Ok(StateEndpoint::State(self.intern_state(key)))
    }

    /// The innermost open scope.
    fn scope(&mut self) -> &mut StateScope {
        if self.stack.is_empty() {
            self.stack.push((None, StateScope::default()));
        }
        // The stack holds at least one scope by the line above, so the subtraction
        // cannot underflow and the index is in bounds.
        let innermost = self.stack.len() - 1;
        &mut self.stack[innermost].1
    }

    /// Interns a state key, registering new states in the current scope.
    fn intern_state(&mut self, key: &str) -> StateId {
        let fresh = !self.states.iter().any(|state| state.key == key);
        let index = intern(
            &mut self.states,
            key,
            |state| state.key.as_str(),
            || StateNode {
                key: key.to_string(),
                label: None,
                kind: StateKind::Simple,
            },
        );
        if fresh {
            self.scope().states.push(StateId(index));
        }
        StateId(index)
    }

    /// Validates the finished diagram and returns it.
    fn finish(mut self, last_line: usize) -> Result<StateDiagram, MermaidError> {
        if let Some(note) = self.note {
            return Err(lex::syntax(
                last_line,
                format!("note opened on line {} has no `end note`", note.line),
            ));
        }
        if self.stack.len() > 1 {
            return Err(lex::syntax(
                last_line,
                "composite state without a closing `}`",
            ));
        }
        let root = self.stack.pop().map(|(_, scope)| scope).unwrap_or_default();
        Ok(StateDiagram {
            direction: self.direction,
            states: self.states,
            root,
        })
    }
}
