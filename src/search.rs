//! Search over the document source, mapped onto canvas positions.
//!
//! Searching happens in the **source text**, never in the rendered canvas: that is what
//! makes a match survive a resize, and what lets a query find text the renderer wrapped
//! across two lines. A search is therefore a two-step affair:
//!
//! 1. [`Search::new`] finds byte ranges in [`Doc::source`](crate::doc::Doc::source).
//! 2. [`Search::locate`] projects those ranges onto a rendered canvas through its
//!    [`SearchSpan`]s, producing one [`Segment`] per rendered piece.
//!
//! A hit that the renderer never put on screen — text inside a skipped HTML block, say
//! — has no segments and is dropped by `locate`, so the match count the user sees is
//! the number of matches they can actually reach.
//!
//! ```
//! use mdmost::search::{Search, SearchMode};
//!
//! let search = Search::new("Hello World", "world", SearchMode::Literal)?;
//! assert_eq!(search.source_hits().len(), 1);
//! assert!(!search.is_case_sensitive()); // smart case: the query is all lower case
//! # Ok::<(), mdmost::search::SearchError>(())
//! ```

#[cfg(test)]
mod tests;

use crate::canvas::SearchSpan;
use crate::text::display_width;

/// How the query text is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// The query is literal text.
    #[default]
    Literal,
    /// The query is a regular expression.
    Regex,
}

/// A search could not be started.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SearchError {
    /// The query was not a valid regular expression.
    #[error("invalid pattern: {0}")]
    BadPattern(String),
}

/// One rendered piece of a match.
///
/// A match wrapped across a line break produces several segments; each is a contiguous
/// run of cells on one canvas row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Segment {
    /// The canvas row.
    pub row: usize,
    /// The first column.
    pub col: u16,
    /// The number of columns covered.
    pub cols: u16,
}

/// One match, in the source and — once located — on the canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The first byte of the match in the document source.
    pub source_start: usize,
    /// One past the last byte of the match in the document source.
    pub source_end: usize,
    /// Where the match was drawn, in canvas order. Empty until [`Search::locate`].
    pub segments: Vec<Segment>,
}

impl Hit {
    /// The first row the match appears on, if it was drawn at all.
    pub fn row(&self) -> Option<usize> {
        self.segments.first().map(|segment| segment.row)
    }
}

/// A completed search over a document's source text.
#[derive(Debug, Clone)]
pub struct Search {
    query: String,
    mode: SearchMode,
    case_sensitive: bool,
    source_hits: Vec<Hit>,
    hits: Vec<Hit>,
}

impl Search {
    /// Runs `query` over `source`.
    ///
    /// Case handling is *smart*: a query containing an upper-case character is matched
    /// case-sensitively, an all-lower-case query is not. An empty query matches nothing
    /// rather than everything, which is the useful behaviour while the user is still
    /// typing at the search prompt.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::BadPattern`] when [`SearchMode::Regex`] is requested and
    /// the query does not compile.
    pub fn new(source: &str, query: &str, mode: SearchMode) -> Result<Self, SearchError> {
        let case_sensitive = query.chars().any(char::is_uppercase);
        let source_hits = if query.is_empty() {
            Vec::new()
        } else {
            match mode {
                SearchMode::Literal => literal_hits(source, query, case_sensitive),
                SearchMode::Regex => regex_hits(source, query, case_sensitive)?,
            }
        };
        Ok(Self {
            query: query.to_string(),
            mode,
            case_sensitive,
            source_hits,
            hits: Vec::new(),
        })
    }

    /// A search with no query and no matches.
    pub fn empty() -> Self {
        Self {
            query: String::new(),
            mode: SearchMode::Literal,
            case_sensitive: false,
            source_hits: Vec::new(),
            hits: Vec::new(),
        }
    }

    /// The query as the user typed it.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// How the query is interpreted.
    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    /// Whether smart case decided to match case-sensitively.
    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// Every match found in the source, including any the renderer never drew.
    pub fn source_hits(&self) -> &[Hit] {
        &self.source_hits
    }

    /// Every match that was drawn on the canvas, in canvas order.
    ///
    /// Empty until [`Search::locate`] has been called for the current render.
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    /// The number of reachable matches.
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    /// Whether the search found nothing reachable.
    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    /// Projects the source matches onto a rendered canvas.
    ///
    /// Call this again after every re-render; it is idempotent and cheap relative to
    /// rendering itself.
    pub fn locate(&mut self, source: &str, spans: &[SearchSpan]) {
        let mut hits = Vec::with_capacity(self.source_hits.len());
        for hit in &self.source_hits {
            let mut segments = segments_for(source, spans, hit.source_start, hit.source_end);
            if segments.is_empty() {
                continue;
            }
            segments.sort_unstable();
            segments.dedup();
            hits.push(Hit {
                source_start: hit.source_start,
                source_end: hit.source_end,
                segments,
            });
        }
        hits.sort_by_key(|hit| hit.segments.first().copied());
        self.hits = hits;
    }

    /// Drops the canvas projection, as a resize does before re-rendering.
    pub fn clear_location(&mut self) {
        self.hits.clear();
    }

    /// The index of the first match at or after `row`, wrapping to the top if asked.
    pub fn first_at_or_after(&self, row: usize, wrap: bool) -> Option<usize> {
        let found = self
            .hits
            .iter()
            .position(|hit| hit.row().is_some_and(|hit_row| hit_row >= row));
        match (found, wrap) {
            (Some(index), _) => Some(index),
            (None, true) => (!self.hits.is_empty()).then_some(0),
            (None, false) => None,
        }
    }

    /// The index of the last match at or before `row`, wrapping to the bottom if asked.
    pub fn last_at_or_before(&self, row: usize, wrap: bool) -> Option<usize> {
        let found = self
            .hits
            .iter()
            .rposition(|hit| hit.row().is_some_and(|hit_row| hit_row <= row));
        match (found, wrap) {
            (Some(index), _) => Some(index),
            (None, true) => self.hits.len().checked_sub(1),
            (None, false) => None,
        }
    }

    /// Steps `count` matches forward (or backward) from `current`, wrapping around.
    ///
    /// Returns `None` only when there is nothing to step through.
    pub fn step(&self, current: Option<usize>, forward: bool) -> Option<usize> {
        let len = self.hits.len();
        if len == 0 {
            return None;
        }
        Some(match (current, forward) {
            (None, true) => 0,
            (None, false) => len - 1,
            (Some(index), true) => (index + 1) % len,
            (Some(index), false) => (index + len - 1) % len,
        })
    }

    /// Every segment drawn on `row`, paired with the index of the match it belongs to.
    pub fn segments_on_row(&self, row: usize) -> impl Iterator<Item = (usize, Segment)> + '_ {
        self.hits.iter().enumerate().flat_map(move |(index, hit)| {
            hit.segments
                .iter()
                .filter(move |segment| segment.row == row)
                .map(move |segment| (index, *segment))
        })
    }
}

impl Default for Search {
    fn default() -> Self {
        Self::empty()
    }
}

/// Finds every non-overlapping literal match.
///
/// The haystack is never lower-cased wholesale: case folding can change a string's byte
/// length (`İ`, `ẞ`), which would make the recorded offsets point into the wrong place
/// in the original source. Instead each candidate start position is compared by folding
/// one character at a time.
fn literal_hits(source: &str, query: &str, case_sensitive: bool) -> Vec<Hit> {
    let mut hits = Vec::new();
    if case_sensitive {
        let mut from = 0usize;
        while let Some(offset) = source[from..].find(query) {
            let start = from + offset;
            let end = start + query.len();
            hits.push(Hit {
                source_start: start,
                source_end: end,
                segments: Vec::new(),
            });
            from = advance(source, start, end);
        }
        return hits;
    }

    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let mut cursor = 0usize;
    while cursor < source.len() {
        match match_folded_at(source, cursor, &needle) {
            Some(end) => {
                hits.push(Hit {
                    source_start: cursor,
                    source_end: end,
                    segments: Vec::new(),
                });
                cursor = advance(source, cursor, end);
            }
            None => cursor = next_boundary(source, cursor),
        }
    }
    hits
}

/// Attempts a case-folded match of `needle` at byte offset `start`.
///
/// Returns the end offset on success. A needle that would end in the middle of a
/// character's case expansion is rejected, so offsets always land on boundaries.
fn match_folded_at(source: &str, start: usize, needle: &[char]) -> Option<usize> {
    let mut wanted = needle.iter();
    let mut offset = start;
    for ch in source[start..].chars() {
        for folded in ch.to_lowercase() {
            if wanted.next()? != &folded {
                return None;
            }
        }
        offset += ch.len_utf8();
        if wanted.len() == 0 {
            return Some(offset);
        }
    }
    None
}

/// Finds every non-overlapping regular-expression match.
fn regex_hits(source: &str, query: &str, case_sensitive: bool) -> Result<Vec<Hit>, SearchError> {
    let pattern = regex::RegexBuilder::new(query)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|error| SearchError::BadPattern(first_line(&error.to_string())))?;
    Ok(pattern
        .find_iter(source)
        .filter(|found| !found.is_empty())
        .map(|found| Hit {
            source_start: found.start(),
            source_end: found.end(),
            segments: Vec::new(),
        })
        .collect())
}

/// The next search position after a match, guaranteeing forward progress.
fn advance(source: &str, start: usize, end: usize) -> usize {
    if end > start {
        end
    } else {
        next_boundary(source, start)
    }
}

/// The next character boundary strictly after `offset`.
fn next_boundary(source: &str, offset: usize) -> usize {
    source[offset..]
        .chars()
        .next()
        .map_or(source.len(), |ch| offset + ch.len_utf8())
}

/// The first line of a possibly multi-line diagnostic.
fn first_line(message: &str) -> String {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(message)
        .trim()
        .to_string()
}

/// Projects one source range onto the canvas through the recorded spans.
///
/// *Every* span overlapping the range contributes a segment, not just the one holding
/// the start: a match that the renderer wrapped across a line break belongs to two
/// spans and must highlight in both.
fn segments_for(source: &str, spans: &[SearchSpan], start: usize, end: usize) -> Vec<Segment> {
    spans
        .iter()
        .filter(|span| span.source_start < end && span.source_end > start)
        .filter_map(|span| {
            let overlap_start = span.source_start.max(start);
            let overlap_end = span.source_end.min(end);
            let prefix = slice(source, span.source_start, overlap_start);
            let body = slice(source, overlap_start, overlap_end);
            let offset = u16::try_from(display_width(prefix)).unwrap_or(u16::MAX);
            let available = span.cols.checked_sub(offset)?;
            let cols = u16::try_from(display_width(body))
                .unwrap_or(u16::MAX)
                .min(available);
            (cols > 0).then_some(Segment {
                row: span.row,
                col: span.col.saturating_add(offset),
                cols,
            })
        })
        .collect()
}

/// Slices `source`, tolerating offsets that are out of range or off a boundary.
fn slice(source: &str, start: usize, end: usize) -> &str {
    if start >= end {
        return "";
    }
    source.get(start..end).unwrap_or("")
}
