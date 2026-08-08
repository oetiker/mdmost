//! `sequenceDiagram` parsing (design spec §6.2).
//!
//! Supported: `participant`/`actor` with `as` aliases and implicit participants from
//! first use, the six arrow forms (`->`, `-->`, `->>`, `-->>`, `-x`, `--x`) including
//! self-messages, `activate`/`deactivate` and the `+`/`-` shorthand, all three `Note`
//! placements, and `loop`/`alt`/`else`/`opt`/`par`/`and`/`critical`/`option` frames.
//!
//! Skipped silently: `autonumber`, `link`/`links`, `box` … `end` and `rect` … `end`
//! (whose matching `end` is swallowed with them).
//!
//! Rejected with a reason: the async arrows `-)` / `--)`, `break`, `create`/`destroy`.

use crate::error::MermaidError;
use crate::mermaid::ast::{
    BlockKind, Branch, Label, Message, MessageHead, MessageLine, Note, NotePlacement, Participant,
    ParticipantId, ParticipantKind, SequenceBlock, SequenceDiagram, SequenceItem,
};

use super::intern;
use super::lex::{self, Nesting, SrcLine};

/// Parses a whole `sequenceDiagram`.
pub fn parse(lines: &[SrcLine<'_>]) -> Result<SequenceDiagram, MermaidError> {
    let Some((header, body)) = lines.split_first() else {
        return Err(lex::syntax(1, "empty diagram"));
    };
    let mut builder = Builder::default();
    let (_, rest) = lex::split_word(header.text);
    if !rest.is_empty() {
        builder.statement(rest, header.number)?;
    }
    for line in body {
        builder.statement(line.text, line.number)?;
    }
    builder.finish(lines.last().map_or(1, |line| line.number))
}

/// One entry of the open-frame stack.
#[derive(Debug)]
struct Frame {
    /// `None` for a skipped `box`/`rect` frame, whose body is kept but whose own
    /// borders are dropped.
    block: Option<SequenceBlock>,
    /// Items collected for the frame's current branch.
    items: Vec<SequenceItem>,
    /// The label of the frame's current branch.
    label: Option<Label>,
    /// The line the frame was opened on, for error messages.
    line: usize,
}

/// Accumulates participants and the statement tree.
#[derive(Debug, Default)]
struct Builder {
    title: Option<String>,
    participants: Vec<Participant>,
    items: Vec<SequenceItem>,
    stack: Vec<Frame>,
}

impl Builder {
    /// Handles one source line.
    fn statement(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let (word, rest) = lex::split_word(text);
        match word.to_ascii_lowercase().as_str() {
            "autonumber" | "link" | "links" => return Ok(()),
            "box" | "rect" => {
                self.stack.push(Frame {
                    block: None,
                    items: Vec::new(),
                    label: None,
                    line,
                });
                return Ok(());
            }
            "title" => {
                self.title = Some(lex::unquote(rest.trim_start_matches(':').trim()).to_string());
                return Ok(());
            }
            "participant" | "actor" => return self.declare(word, rest, line),
            "activate" | "deactivate" => {
                let id = self.participant(rest, line)?;
                let item = if word.eq_ignore_ascii_case("activate") {
                    SequenceItem::Activate(id)
                } else {
                    SequenceItem::Deactivate(id)
                };
                self.push(item);
                return Ok(());
            }
            "note" => return self.note(rest, line),
            "loop" | "alt" | "opt" | "par" | "critical" => {
                let kind = match word.to_ascii_lowercase().as_str() {
                    "loop" => BlockKind::Loop,
                    "alt" => BlockKind::Alt,
                    "opt" => BlockKind::Opt,
                    "par" => BlockKind::Par,
                    _ => BlockKind::Critical,
                };
                self.stack.push(Frame {
                    block: Some(SequenceBlock {
                        kind,
                        branches: Vec::new(),
                    }),
                    items: Vec::new(),
                    label: label_of(rest),
                    line,
                });
                return Ok(());
            }
            "else" | "and" | "option" => return self.branch(rest, line),
            "end" if rest.is_empty() => return self.close(line),
            "break" => return Err(lex::unsupported(line, "`break` blocks")),
            "create" | "destroy" => {
                return Err(lex::unsupported(line, "`create`/`destroy` participants"));
            }
            _ => {}
        }
        self.message(text, line)
    }

    /// Handles `participant A as Alice` / `actor A`.
    fn declare(&mut self, word: &str, rest: &str, line: usize) -> Result<(), MermaidError> {
        let kind = if word.eq_ignore_ascii_case("actor") {
            ParticipantKind::Actor
        } else {
            ParticipantKind::Participant
        };
        let (key, alias) = match split_alias(rest) {
            Some((key, alias)) => (key, Some(alias)),
            None => (rest.trim(), None),
        };
        let key = lex::unquote(key);
        if key.is_empty() {
            return Err(lex::syntax(line, "participant without a name"));
        }
        let index = self.intern_participant(key);
        if let Some(participant) = self.participants.get_mut(index) {
            participant.kind = kind;
            if let Some(alias) = alias {
                participant.label = Label::parse(lex::unquote(alias));
            }
        }
        Ok(())
    }

    /// Handles a `Note left of|right of|over …` statement.
    fn note(&mut self, rest: &str, line: usize) -> Result<(), MermaidError> {
        let lowered = rest.to_ascii_lowercase();
        let (placement, after) = if let Some(after) = lowered.strip_prefix("left of") {
            (NotePlacement::LeftOf, &rest[rest.len() - after.len()..])
        } else if let Some(after) = lowered.strip_prefix("right of") {
            (NotePlacement::RightOf, &rest[rest.len() - after.len()..])
        } else if let Some(after) = lowered.strip_prefix("over") {
            (NotePlacement::Over, &rest[rest.len() - after.len()..])
        } else {
            return Err(lex::syntax(
                line,
                "note without `left of`/`right of`/`over`",
            ));
        };
        let Some((targets, text)) = lex::split_once_top_level(after, ':', Nesting::Ignore) else {
            return Err(lex::syntax(line, "note without `: text`"));
        };
        let participants = lex::split_top_level(targets, ',', Nesting::Ignore)
            .into_iter()
            .map(|target| self.participant(target, line))
            .collect::<Result<Vec<_>, _>>()?;
        if participants.is_empty() {
            return Err(lex::syntax(line, "note without a participant"));
        }
        self.push(SequenceItem::Note(Note {
            placement,
            participants,
            text: Label::parse(lex::unquote(text)),
        }));
        Ok(())
    }

    /// Handles an `else`/`and`/`option` continuation.
    fn branch(&mut self, rest: &str, line: usize) -> Result<(), MermaidError> {
        let Some(frame) = self.stack.last_mut() else {
            return Err(lex::syntax(line, "branch keyword outside a block"));
        };
        let Some(block) = frame.block.as_mut() else {
            return Err(lex::syntax(line, "branch keyword inside a `box`/`rect`"));
        };
        block.branches.push(Branch {
            label: frame.label.take(),
            items: std::mem::take(&mut frame.items),
        });
        frame.label = label_of(rest);
        Ok(())
    }

    /// Closes the innermost frame.
    fn close(&mut self, line: usize) -> Result<(), MermaidError> {
        let Some(mut frame) = self.stack.pop() else {
            return Err(lex::syntax(line, "`end` without a matching block"));
        };
        match frame.block.take() {
            Some(mut block) => {
                block.branches.push(Branch {
                    label: frame.label,
                    items: frame.items,
                });
                self.push(SequenceItem::Block(block));
            }
            // A skipped `box`/`rect` contributes its body to the enclosing scope.
            None => {
                for item in frame.items {
                    self.push(item);
                }
            }
        }
        Ok(())
    }

    /// Parses a message such as `Alice->>+John: Hello`.
    fn message(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let Some((head_text, label)) = lex::split_once_top_level(text, ':', Nesting::Ignore) else {
            return Err(lex::syntax(
                line,
                format!("cannot read a statement from `{text}`"),
            ));
        };
        let arrow = find_arrow(head_text, line)?;
        let from = self.participant(&head_text[..arrow.start], line)?;
        let target = head_text[arrow.end..].trim();
        let (target, activates, deactivates) = match target.as_bytes().first() {
            Some(b'+') => (target[1..].trim(), true, false),
            Some(b'-') => (target[1..].trim(), false, true),
            _ => (target, false, false),
        };
        let to = self.participant(target, line)?;
        self.push(SequenceItem::Message(Message {
            from,
            to,
            line: arrow.line,
            head: arrow.head,
            label: Label::parse(lex::unquote(label)),
            activates,
            deactivates,
        }));
        Ok(())
    }

    /// Appends an item to the innermost open frame.
    fn push(&mut self, item: SequenceItem) {
        match self.stack.last_mut() {
            Some(frame) => frame.items.push(item),
            None => self.items.push(item),
        }
    }

    /// Resolves a participant reference, creating it implicitly when new.
    fn participant(&mut self, text: &str, line: usize) -> Result<ParticipantId, MermaidError> {
        let key = lex::unquote(text.trim());
        if key.is_empty() {
            return Err(lex::syntax(line, "message without a participant"));
        }
        Ok(ParticipantId(self.intern_participant(key)))
    }

    /// Interns a participant key, preserving declaration order.
    fn intern_participant(&mut self, key: &str) -> usize {
        intern(
            &mut self.participants,
            key,
            |participant| participant.key.as_str(),
            || Participant {
                key: key.to_string(),
                label: Label::parse(key),
                kind: ParticipantKind::Participant,
            },
        )
    }

    /// Validates the finished diagram and returns it.
    fn finish(self, last_line: usize) -> Result<SequenceDiagram, MermaidError> {
        if let Some(frame) = self.stack.first() {
            return Err(lex::syntax(
                last_line,
                format!("block opened on line {} has no `end`", frame.line),
            ));
        }
        Ok(SequenceDiagram {
            title: self.title,
            participants: self.participants,
            items: self.items,
        })
    }
}

/// Turns the text after a block keyword into an optional label.
fn label_of(rest: &str) -> Option<Label> {
    let rest = lex::unquote(rest.trim());
    (!rest.is_empty()).then(|| Label::parse(rest))
}

/// Splits `A as Alice` into `("A", "Alice")`.
fn split_alias(text: &str) -> Option<(&str, &str)> {
    let lowered = text.to_ascii_lowercase();
    let at = lowered.find(" as ")?;
    Some((text[..at].trim(), text[at + 4..].trim()))
}

/// A message arrow found in a statement.
#[derive(Debug)]
struct Arrow {
    line: MessageLine,
    head: MessageHead,
    start: usize,
    end: usize,
}

/// Every arrow spelling, longest first so that `-->>` wins over `-->`.
const ARROWS: [(&str, MessageLine, MessageHead); 6] = [
    ("-->>", MessageLine::Dotted, MessageHead::Arrow),
    ("--x", MessageLine::Dotted, MessageHead::Cross),
    ("-->", MessageLine::Dotted, MessageHead::None),
    ("->>", MessageLine::Solid, MessageHead::Arrow),
    ("-x", MessageLine::Solid, MessageHead::Cross),
    ("->", MessageLine::Solid, MessageHead::None),
];

/// Finds the message arrow in the part of a statement before the `:`.
fn find_arrow(text: &str, line: usize) -> Result<Arrow, MermaidError> {
    let mut scanner = lex::Scanner::default();
    for (at, ch) in text.char_indices() {
        if !scanner.step(ch, Nesting::Ignore) {
            continue;
        }
        let rest = &text[at..];
        if rest.starts_with("--)") || rest.starts_with("-)") {
            return Err(lex::unsupported(line, "async arrows `-)` and `--)`"));
        }
        for (token, message_line, head) in ARROWS {
            if rest.starts_with(token) {
                return Ok(Arrow {
                    line: message_line,
                    head,
                    start: at,
                    end: at + token.len(),
                });
            }
        }
    }
    Err(lex::syntax(
        line,
        format!("cannot read a message arrow from `{text}`"),
    ))
}
