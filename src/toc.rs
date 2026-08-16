// SPDX-License-Identifier: MIT
//! The table of contents: heading tree, current position, fuzzy filter.
//!
//! A [`Toc`] is built from [`crate::doc::Doc::headings`] and holds a flat list in
//! document order plus parent/child links, which is what a pane needs: flat for
//! scrolling and hit-testing, hierarchical for indentation and for collapsing.
//!
//! Rows come from a render pass. [`Toc::attach_anchors`] takes the
//! [`Anchor`](crate::canvas::Anchor)s a [`Canvas`](crate::canvas::Canvas) recorded and
//! fills in where each heading landed. Because rows are attached rather than stored at
//! build time, the same `Toc` survives a resize: re-render, re-attach, done.
//!
//! ```
//! use mdmost::doc::Doc;
//! use mdmost::numbering::Numbering;
//! use mdmost::toc::Toc;
//!
//! let doc = Doc::parse("# One\n\n## Two\n\n# Three\n");
//! let toc = Toc::from_doc(&doc, &Numbering::for_doc(&doc));
//! assert_eq!(toc.len(), 3);
//! assert_eq!(toc.entries()[1].depth, 1);
//! ```

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::canvas::Anchor;
use crate::doc::Doc;
use crate::numbering::Numbering;

/// One heading in the table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// The heading's anchor id, unique within the document.
    pub id: String,
    /// The heading text, with inline markup already flattened.
    pub text: String,
    /// The Markdown heading level, `1..=6`.
    pub level: u8,
    /// Indentation depth, counted over the levels actually used by this document.
    ///
    /// A document whose headings jump from `#` to `###` still indents by one step, so
    /// the pane never shows a ragged gap the author did not intend.
    pub depth: usize,
    /// The section number of this heading, if the document is numbered (spec §9.3).
    ///
    /// Kept beside the text rather than folded into it: the number is ours, not the
    /// author's, so the fuzzy filter, the status bar's breadcrumb and every other
    /// reader of `text` must go on seeing the document as it was written. The pane
    /// draws it in [`Theme::heading_number`](crate::theme::Theme::heading_number), the
    /// same quiet slot the body uses.
    pub number: Option<String>,
    /// The index of the parent entry, if any.
    pub parent: Option<usize>,
    /// The row this heading was rendered at, once anchors have been attached.
    pub row: Option<usize>,
    /// The byte offset of the heading in the document source.
    pub source_start: usize,
}

/// The heading tree of a document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Toc {
    entries: Vec<TocEntry>,
}

impl Toc {
    /// Builds the table of contents from a parsed document and its section numbering.
    ///
    /// The numbering is passed in rather than derived here so that the pane and the
    /// page can never show different numbers for the same heading: there is one
    /// [`Numbering`] per document, built by the same
    /// [`Numbering::for_doc`](crate::numbering::Numbering::for_doc) the renderer uses.
    /// Pass [`Numbering::none`](crate::numbering::Numbering::none) for an unnumbered
    /// table of contents.
    pub fn from_doc(doc: &Doc, numbers: &Numbering) -> Self {
        let mut entries: Vec<TocEntry> = Vec::with_capacity(doc.headings().len());
        // Stack of (level, index) of the ancestors of the entry being added.
        let mut ancestors: Vec<(u8, usize)> = Vec::new();
        for heading in doc.headings() {
            while ancestors
                .last()
                .is_some_and(|(level, _)| *level >= heading.level)
            {
                ancestors.pop();
            }
            let parent = ancestors.last().map(|(_, index)| *index);
            let depth = ancestors.len();
            entries.push(TocEntry {
                id: heading.id.clone(),
                text: heading.text.clone(),
                level: heading.level,
                depth,
                number: numbers.label(&heading.id).map(ToString::to_string),
                parent,
                row: None,
                source_start: heading.source.start,
            });
            ancestors.push((heading.level, entries.len() - 1));
        }
        Self { entries }
    }

    /// Records where each heading was rendered.
    ///
    /// Headings without a matching anchor keep `row == None` and are skipped by
    /// [`Toc::current`] and [`Toc::row_of`]; that is the honest outcome for a heading
    /// the renderer did not emit.
    ///
    /// Indexed by id first: the obvious nested scan is quadratic in the number of
    /// headings, which is what made a heading-dense document take seconds to open
    /// (usability review B5) rather than merely being large.
    pub fn attach_anchors(&mut self, anchors: &[Anchor]) {
        let rows: BTreeMap<&str, usize> = anchors
            .iter()
            .map(|anchor| (anchor.id.as_str(), anchor.row))
            .collect();
        for entry in &mut self.entries {
            entry.row = rows.get(entry.id.as_str()).copied();
        }
    }

    /// Forgets every attached row, as a resize does before re-rendering.
    pub fn clear_anchors(&mut self) {
        for entry in &mut self.entries {
            entry.row = None;
        }
    }

    /// The entries, in document order.
    pub fn entries(&self) -> &[TocEntry] {
        &self.entries
    }

    /// The number of headings.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the document has no headings at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The index of the entry with the given anchor id.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.id == id)
    }

    /// The row an entry was rendered at.
    pub fn row_of(&self, index: usize) -> Option<usize> {
        self.entries.get(index).and_then(|entry| entry.row)
    }

    /// The entry whose section contains `row`: the last heading at or above it.
    ///
    /// Returns `None` when `row` sits above the first heading, which is the correct
    /// answer for a document with a preamble.
    pub fn current(&self, row: usize) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.row.map(|entry_row| (index, entry_row)))
            .take_while(|(_, entry_row)| *entry_row <= row)
            .last()
            .map(|(index, _)| index)
    }

    /// The chain of ancestors of `index`, outermost first, including `index` itself.
    pub fn breadcrumb(&self, index: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut cursor = Some(index);
        while let Some(current) = cursor {
            chain.push(current);
            cursor = self.entries.get(current).and_then(|entry| entry.parent);
        }
        chain.reverse();
        chain
    }

    /// The first heading strictly below `row`.
    pub fn next_after(&self, row: usize) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.row.is_some_and(|entry_row| entry_row > row))
            .map(|(index, _)| index)
    }

    /// The last heading strictly above `row`.
    pub fn prev_before(&self, row: usize) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .rfind(|(_, entry)| entry.row.is_some_and(|entry_row| entry_row < row))
            .map(|(index, _)| index)
    }

    /// Filters the table of contents fuzzily.
    ///
    /// An empty query matches everything, in document order. A non-empty query keeps
    /// entries whose text contains the query's characters in order, best score first;
    /// ties are broken by document order so the result never jitters.
    pub fn filter(&self, query: &str) -> Vec<FilterHit> {
        if query.trim().is_empty() {
            return (0..self.entries.len())
                .map(|index| FilterHit {
                    index,
                    score: 0,
                    positions: Vec::new(),
                })
                .collect();
        }
        let mut hits: Vec<FilterHit> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                fuzzy_match(&entry.text, query).map(|(score, positions)| FilterHit {
                    index,
                    score,
                    positions,
                })
            })
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
        hits
    }
}

/// One entry surviving a fuzzy filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterHit {
    /// The index into [`Toc::entries`].
    pub index: usize,
    /// How well the entry matched; higher is better.
    pub score: i32,
    /// The character positions of the match, for highlighting.
    pub positions: Vec<usize>,
}

/// Scores `query` against `text` as a subsequence match.
///
/// Returns `None` when the query's characters do not appear in order. The score
/// rewards consecutive runs, matches at word starts and matches near the beginning,
/// which is what makes a short query land on the heading the user meant.
fn fuzzy_match(text: &str, query: &str) -> Option<(i32, Vec<usize>)> {
    let haystack: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    // Recomputing the char indices keeps `positions` aligned with `text` even when
    // case folding changes the character count (for example `İ`).
    let plain: Vec<char> = text.chars().collect();
    let folded_lengths: Vec<usize> = plain
        .iter()
        .map(|ch| ch.to_lowercase().count().max(1))
        .collect();
    let needle: Vec<char> = query
        .trim()
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if needle.is_empty() {
        return Some((0, Vec::new()));
    }

    let mut positions = Vec::with_capacity(needle.len());
    let mut score = 0;
    let mut cursor = 0usize;
    let mut previous: Option<usize> = None;
    for wanted in &needle {
        let found = haystack[cursor..]
            .iter()
            .position(|ch| ch == wanted)
            .map(|offset| cursor + offset)?;
        score += match previous {
            Some(previous) if previous + 1 == found => 8,
            _ => 0,
        };
        if found == 0 {
            score += 6;
        } else if !haystack[found - 1].is_alphanumeric() {
            score += 4;
        }
        positions.push(found);
        previous = Some(found);
        cursor = found + 1;
    }
    // Shorter matches spread over less text are better matches.
    let span = positions.last().copied().unwrap_or(0) - positions.first().copied().unwrap_or(0) + 1;
    score += 20 - i32::try_from(span.min(20)).unwrap_or(20);
    score -= i32::try_from(positions.first().copied().unwrap_or(0).min(20)).unwrap_or(0);

    Some((score, map_positions(&positions, &folded_lengths)))
}

/// Maps positions in the case-folded string back to character indices in the original.
fn map_positions(positions: &[usize], folded_lengths: &[usize]) -> Vec<usize> {
    // Prefix sums of the folded lengths give, for each original character, the folded
    // index it starts at. Folding is expanding-only, so this is monotonic.
    let mut starts = Vec::with_capacity(folded_lengths.len());
    let mut running = 0usize;
    for length in folded_lengths {
        starts.push(running);
        running += length;
    }
    positions
        .iter()
        .map(|folded| match starts.binary_search(folded) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        })
        .collect()
}
