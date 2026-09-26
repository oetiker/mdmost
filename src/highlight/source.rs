// SPDX-License-Identifier: MIT
//! Where the renderer gets a code block's lines from.
//!
//! The renderer does not call the highlighter directly. It asks a [`CodeSource`], so that
//! `--render-once` can colour every block before output while the pager draws first and
//! colours later (see `Highlighter`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroUsize;

use super::{Outcome, highlight_full, token_limit_for};
use crate::text::Line;
use crate::theme::{CodeStyles, Theme};

/// What a code source hands the renderer for one block.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlock {
    /// One line per source line, tabs expanded. Plain and highlighted lines of the same
    /// block always have the same text, so they occupy the same cells.
    pub lines: Vec<Line>,
    /// What became of the block so far.
    pub outcome: Outcome,
    /// The source's handle for this block, if it colours blocks after layout; the
    /// renderer records it on every drawn row (`canvas::CodeRow`).
    pub key: Option<u64>,
}

/// Gives the renderer a code block's lines.
pub trait CodeSource: std::fmt::Debug {
    /// The lines to draw for this block and what became of it, in one call.
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock;
}

/// Highlights on every call and remembers nothing.
///
/// The source a fresh `render::Ctx` starts with. Entry points that lay a block out at
/// several widths hand the context a [`BlockingSource`] instead.
#[derive(Debug, Clone, Copy, Default)]
pub struct Uncached;

/// The one [`Uncached`] every fresh context points at.
pub(crate) static UNCACHED: Uncached = Uncached;

impl CodeSource for Uncached {
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock {
        let (lines, outcome) = highlight_full(lang, src, &theme.code, token_limit_for);
        CodeBlock {
            lines,
            outcome,
            key: None,
        }
    }
}

/// Highlights a block on first request and remembers it for the life of the instance.
///
/// `render::document::render_widened` lays a clipped block out at several widths; the
/// highlighter takes no width, so every one of those layouts asks for the same lines. One
/// instance per render keeps that to one parse, and dropping the instance with the render
/// is what keeps the memo from outliving the document it describes.
pub struct BlockingSource {
    limit_for: fn(&str) -> NonZeroUsize,
    memo: RefCell<Memo>,
}

#[derive(Default)]
struct Memo {
    styles: Option<CodeStyles>,
    entries: HashMap<(Option<String>, String), Entry>,
}

struct Entry {
    lines: Vec<Line>,
    outcome: Outcome,
    #[cfg(test)]
    computed: u64,
}

impl std::fmt::Debug for BlockingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockingSource").finish_non_exhaustive()
    }
}

impl Default for BlockingSource {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockingSource {
    /// A source with the full per-line budget, `token_limit_for`.
    pub fn new() -> Self {
        Self::with_line_limit(token_limit_for)
    }

    /// A source with the per-line budget replaced, for tests that need a real
    /// `Outcome::Failed` on a real syntax.
    pub(crate) fn with_line_limit(limit_for: fn(&str) -> NonZeroUsize) -> Self {
        Self {
            limit_for,
            memo: RefCell::new(Memo::default()),
        }
    }

    /// How many times this instance has highlighted the block, or `None` if it has not.
    #[cfg(test)]
    pub(crate) fn computed_count(&self, lang: Option<&str>, src: &str) -> Option<u64> {
        let memo = self.memo.borrow();
        memo.entries
            .get(&(lang.map(str::to_owned), src.to_owned()))
            .map(|entry| entry.computed)
    }
}

impl CodeSource for BlockingSource {
    fn block(&self, lang: Option<&str>, src: &str, theme: &Theme) -> CodeBlock {
        let mut memo = self.memo.borrow_mut();
        if memo.styles != Some(theme.code) {
            memo.entries.clear();
            memo.styles = Some(theme.code);
        }
        let key = (lang.map(str::to_owned), src.to_owned());
        let entry = memo.entries.entry(key).or_insert_with(|| {
            let (lines, outcome) = highlight_full(lang, src, &theme.code, self.limit_for);
            Entry {
                lines,
                outcome,
                #[cfg(test)]
                computed: 1,
            }
        });
        CodeBlock {
            lines: entry.lines.clone(),
            outcome: entry.outcome,
            key: None,
        }
    }
}
