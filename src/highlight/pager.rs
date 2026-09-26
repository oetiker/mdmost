// SPDX-License-Identifier: MIT
//! The pager's code source: draw first, colour later.
//!
//! [`Highlighter`] answers the renderer at once — plain lines for a block it has not
//! parsed, coloured lines for the part it has — and parses more in [`Highlighter::advance`],
//! which the pager calls between key presses. Design:
//! `docs/superpowers/specs/2026-09-25-viewport-highlighter-design.md`.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::ops::Range;

use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use super::{
    CodeBlock, CodeSource, HighlightError, MAX_HIGHLIGHT_BYTES, MAX_HIGHLIGHT_LINES, Outcome,
    Parse, plain, resolve_syntax, token_limit_for,
};
use crate::text::Line;
use crate::theme::{CodeStyles, Theme};

/// The per-line token cap in the pager.
///
/// One line cannot be split across slices: `syntect` cannot resume inside a line. This
/// bounds how long one line can hold up a key press. Measured in `docs/maintainer-notes.md`.
pub const PAGER_LINE_TOKENS: NonZeroUsize = match NonZeroUsize::new(5_500) {
    Some(n) => n,
    None => NonZeroUsize::MIN,
};

/// How many part-parsed blocks keep their parser state.
const PARKING_LOT: usize = 4;

/// The pager's per-line limit: the full budget, but never above [`PAGER_LINE_TOKENS`].
pub(crate) fn pager_limit_for(line: &str) -> NonZeroUsize {
    token_limit_for(line).min(PAGER_LINE_TOKENS)
}

type Syntax = (&'static SyntaxSet, &'static SyntaxReference);

struct Block {
    #[allow(dead_code)] // kept for parity with the design's field list; not read yet
    lang: Option<String>,
    src: String,
    /// Byte ranges of the source lines, line endings included.
    raw: Vec<Range<usize>>,
    syntax: Option<Syntax>,
    /// Finished lines: coloured while `Pending`/`Highlighted`, plain once `Failed`/`Plain`.
    lines: Vec<Line>,
    outcome: Outcome,
    /// The render generation that last asked for this block.
    seen: u64,
}

struct Inner {
    styles: Option<CodeStyles>,
    limit_for: fn(&str) -> NonZeroUsize,
    keys: HashMap<(Option<String>, String), u64>,
    blocks: HashMap<u64, Block>,
    next_key: u64,
    /// Parser state of part-parsed blocks, least recently advanced first, with the index
    /// of the next line each will parse.
    parked: VecDeque<(u64, Parse<'static>, usize)>,
    generation: u64,
    failed: bool,
}

/// The pager's code source. See the module documentation.
pub struct Highlighter {
    inner: RefCell<Inner>,
}

impl std::fmt::Debug for Highlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlighter").finish_non_exhaustive()
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    /// A highlighter capped at [`pager_limit_for`] — the full per-line budget, but never
    /// above [`PAGER_LINE_TOKENS`].
    pub fn new() -> Self {
        Self::with_line_limit(pager_limit_for)
    }

    /// A highlighter with a replaced per-line budget, for tests that need a block to
    /// hit [`Outcome::Failed`] deterministically.
    pub(crate) fn with_line_limit(limit_for: fn(&str) -> NonZeroUsize) -> Self {
        Self {
            inner: RefCell::new(Inner {
                styles: None,
                limit_for,
                keys: HashMap::new(),
                blocks: HashMap::new(),
                next_key: 0,
                parked: VecDeque::new(),
                generation: 0,
                failed: false,
            }),
        }
    }

    /// What became of `key`'s block, or `None` if it is not known.
    pub fn outcome(&self, key: u64) -> Option<Outcome> {
        self.inner
            .borrow()
            .blocks
            .get(&key)
            .map(|block| block.outcome)
    }

    /// The line at `index` of `key`'s block, cloned, or `None` if either is unknown.
    pub fn line(&self, key: u64, index: usize) -> Option<Line> {
        self.inner
            .borrow()
            .blocks
            .get(&key)
            .and_then(|block| block.lines.get(index))
            .cloned()
    }

    /// Parses `key` from where it stopped; returns the range of lines newly finished.
    ///
    /// Does nothing and returns an empty range if `key` is unknown or not
    /// [`Outcome::Pending`]. `should_stop` is polled once per parsed line, including the
    /// last, so a caller timing a budget sees every line this call actually parsed.
    pub fn advance(&mut self, key: u64, should_stop: &mut dyn FnMut() -> bool) -> Range<usize> {
        let inner = self.inner.get_mut();
        let Some(styles) = inner.styles else {
            return 0..0;
        };
        let limit_for = inner.limit_for;
        let Some(block) = inner.blocks.get(&key) else {
            return 0..0;
        };
        if block.outcome != Outcome::Pending {
            return 0..0;
        }
        let Some((set, syntax)) = block.syntax else {
            return 0..0;
        };
        let first = block.lines.len();

        // Resume the parked parser state, or start over from the first line: an evicted
        // block's parser state is gone, but its already-finished lines are not, so a
        // fresh parse replays them without re-pushing what is already there.
        let (mut parse, mut next) = match inner
            .parked
            .iter()
            .position(|(parked_key, _, _)| *parked_key == key)
        {
            Some(pos) => {
                let (_, parse, next) = inner
                    .parked
                    .remove(pos)
                    .expect("position() just returned this index");
                (parse, next)
            }
            None => (Parse::new(set, syntax), 0usize),
        };

        let Some(block) = inner.blocks.get_mut(&key) else {
            return 0..0;
        };

        if block.raw.is_empty() {
            block.outcome = Outcome::Highlighted;
            return first..first;
        }

        while let Some(range) = block.raw.get(next).cloned() {
            let result = match block.src.get(range) {
                Some(raw_line) => parse.line(raw_line, &styles, limit_for(raw_line)),
                None => Err(HighlightError::Other),
            };
            match result {
                Ok(line) => {
                    if next >= block.lines.len() {
                        block.lines.push(line);
                    }
                    next += 1;
                    if next == block.raw.len() {
                        block.outcome = Outcome::Highlighted;
                    }
                }
                Err(HighlightError::TokenLimitExceeded) => {
                    block.lines = plain(&block.src, &styles);
                    block.outcome = Outcome::Failed;
                    inner.failed = true;
                    return first..first;
                }
                Err(HighlightError::Other) => {
                    block.lines = plain(&block.src, &styles);
                    block.outcome = Outcome::Plain;
                    inner.failed = true;
                    return first..first;
                }
            }
            if should_stop() || block.outcome == Outcome::Highlighted {
                break;
            }
        }

        if block.outcome == Outcome::Pending {
            inner.parked.push_back((key, parse, next));
            while inner.parked.len() > PARKING_LOT {
                inner.parked.pop_front();
            }
        }

        first..block.lines.len()
    }

    /// Drops blocks the last render did not ask for; call once after each render.
    pub fn end_render(&mut self) {
        let inner = self.inner.get_mut();
        let generation = inner.generation;
        let stale: Vec<u64> = inner
            .blocks
            .iter()
            .filter(|(_, block)| block.seen != generation)
            .map(|(&key, _)| key)
            .collect();
        for key in stale {
            inner.blocks.remove(&key);
            inner.keys.retain(|_, &mut k| k != key);
            inner.parked.retain(|(parked_key, _, _)| *parked_key != key);
        }
        inner.generation += 1;
    }

    /// Whether a block failed or went plain mid-parse since the last call; clears the
    /// flag.
    pub fn take_failed(&mut self) -> bool {
        std::mem::take(&mut self.inner.get_mut().failed)
    }

    /// The parked keys, oldest first.
    #[cfg(test)]
    pub(crate) fn parked_keys(&self) -> Vec<u64> {
        self.inner
            .borrow()
            .parked
            .iter()
            .map(|(key, _, _)| *key)
            .collect()
    }
}

impl CodeSource for Highlighter {
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock {
        let mut inner = self.inner.borrow_mut();
        if inner.styles != Some(theme.code) {
            inner.keys.clear();
            inner.blocks.clear();
            inner.parked.clear();
            inner.styles = Some(theme.code);
        }
        let styles = theme.code;
        let generation = inner.generation;
        let map_key = (lang.map(str::to_owned), src.to_owned());
        let key = if let Some(&key) = inner.keys.get(&map_key) {
            key
        } else {
            let key = inner.next_key;
            inner.next_key += 1;
            inner.keys.insert(map_key, key);
            key
        };

        let block = inner.blocks.entry(key).or_insert_with(|| {
            let mut raw = Vec::new();
            let mut offset = 0usize;
            for line in LinesWithEndings::from(src) {
                let end = offset + line.len();
                raw.push(offset..end);
                offset = end;
            }
            // Check order matches `highlight_uncached`, so the outcome agrees with the
            // blocking path: the byte guard first, then resolution, then the line guard.
            let syntax = if src.len() > MAX_HIGHLIGHT_BYTES {
                None
            } else {
                resolve_syntax(lang).filter(|_| raw.len() <= MAX_HIGHLIGHT_LINES)
            };
            let (lines, outcome) = if syntax.is_some() {
                (Vec::new(), Outcome::Pending)
            } else {
                (plain(src, &styles), Outcome::Plain)
            };
            Block {
                lang: lang.map(str::to_owned),
                src: src.to_owned(),
                raw,
                syntax,
                lines,
                outcome,
                seen: generation,
            }
        });
        block.seen = generation;
        let outcome = block.outcome;
        let lines = if outcome == Outcome::Pending {
            let mut lines = block.lines.clone();
            let rest = plain(&block.src, &styles);
            lines.extend_from_slice(rest.get(block.lines.len()..).unwrap_or_default());
            lines
        } else {
            block.lines.clone()
        };

        CodeBlock {
            key: Some(key),
            outcome,
            lines,
        }
    }
}

#[cfg(test)]
#[path = "pager_tests.rs"]
mod tests;
