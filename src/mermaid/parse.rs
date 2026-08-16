// SPDX-License-Identifier: MIT
//! Mermaid source to a typed [`Diagram`].
//!
//! The public interface named in design spec §5 is
//! [`parse`]`(&str) -> Result<Diagram, MermaidError>`. The family is detected from the
//! first significant line and the rest is handed to one of the seven family parsers in
//! the `parse::` submodules, all of which share the lexical helpers in `parse::lex`.
//!
//! Three rules hold for every family (design spec §6):
//!
//! * Directives, `%%` comments and `%%{init}%%` blocks are parsed and ignored.
//! * Cosmetic statements that carry no structure the renderer could use — `click`,
//!   `style`, `classDef`, `class` (styling form), `cssClass`, `linkStyle`,
//!   `autonumber`, `box`, `rect`, `link`/`links`, `todayMarker`, `excludes` — are
//!   skipped silently. `box`/`rect` swallow their matching `end`.
//! * Anything else outside the implemented subset returns a [`MermaidError`] naming
//!   the line and the construct. Parsing never panics and never silently produces a
//!   diagram that disagrees with the source.

mod class;
mod date;
mod er;
mod flowchart;
mod gantt;
mod lex;
mod pie;
mod sequence;
mod state;

use crate::error::MermaidError;
use crate::mermaid::ast::{Diagram, Direction};

/// Parses Mermaid source into a typed [`Diagram`].
///
/// # Errors
///
/// Returns [`MermaidError::UnsupportedFamily`] when the diagram keyword is not one of
/// the seven supported families, and [`MermaidError::Unsupported`] or
/// [`MermaidError::Syntax`] when a statement inside a supported family is outside the
/// implemented subset or malformed. The block renderer turns any of these into the
/// captioned fallback described in design spec §6.
pub fn parse(src: &str) -> Result<Diagram, MermaidError> {
    let lines = lex::preprocess(src);
    let Some(first) = lines.first() else {
        return Err(lex::syntax(1, "empty diagram"));
    };
    let (keyword, _) = lex::split_word(first.text);
    // `graph TD;` and `pie title X` put content on the header line, so the family
    // parsers receive the header line too and consume it themselves.
    match keyword.to_ascii_lowercase().as_str() {
        "flowchart" | "flowchart-v2" | "graph" => {
            flowchart::parse(&lines, src).map(Diagram::Flowchart)
        }
        "sequencediagram" => sequence::parse(&lines, src).map(Diagram::Sequence),
        "classdiagram" | "classdiagram-v2" => class::parse(&lines, src).map(Diagram::Class),
        "erdiagram" => er::parse(&lines, src).map(Diagram::Er),
        "pie" | "pie-beta" => pie::parse(&lines, src).map(Diagram::Pie),
        "gantt" => gantt::parse(&lines, src).map(Diagram::Gantt),
        // Plain `stateDiagram` uses the v1 renderer in Mermaid but the same grammar
        // subset we support, so both spellings are accepted.
        "statediagram" | "statediagram-v2" => state::parse(&lines, src).map(Diagram::State),
        other => Err(MermaidError::UnsupportedFamily(other.to_string())),
    }
}

/// Parses a direction keyword shared by flowchart, class and state diagrams.
///
/// `TD` and `TB` are the same direction; see [`Direction`].
fn direction(word: &str) -> Option<Direction> {
    match word.to_ascii_uppercase().as_str() {
        "TD" | "TB" | "V" => Some(Direction::TopToBottom),
        "BT" | "^" => Some(Direction::BottomToTop),
        "LR" | ">" => Some(Direction::LeftToRight),
        "RL" | "<" => Some(Direction::RightToLeft),
        _ => None,
    }
}

/// Looks `key` up in an arena, appending a freshly built entry when it is new.
///
/// This is the "define after use" rule of Mermaid: `A --> B` creates both nodes, and a
/// later `B{Decision}` must upgrade that same entry rather than add a second one.
fn intern<T>(
    items: &mut Vec<T>,
    key: &str,
    key_of: impl Fn(&T) -> &str,
    make: impl FnOnce() -> T,
) -> usize {
    match items.iter().position(|item| key_of(item) == key) {
        Some(index) => index,
        None => {
            items.push(make());
            items.len() - 1
        }
    }
}
