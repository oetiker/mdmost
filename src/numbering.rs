// SPDX-License-Identifier: MIT
//! Section numbers for a deeply nested document (design spec §9.3).
//!
//! A document that nests three or more section levels is hard to keep a position in:
//! the heading rules say *how deep* you are but never *where*, and by the third `###`
//! of the fourth `##` the reader has lost the thread. So such a document gets a
//! `1.2.3` in front of every heading, in the body and in the table of contents alike.
//!
//! **These numbers are ours, not the author's.** They are drawn in a dedicated quiet
//! slot ([`Theme::heading_number`](crate::theme::Theme::heading_number)) precisely so
//! that nobody mistakes them for text the author wrote, and they are never part of the
//! heading text: search, the TOC filter and the anchor ids all see the document
//! exactly as it was written.
//!
//! # The rule
//!
//! * A document **titled** by a lone `#` ([`Doc::lone_title`]) leaves that title
//!   unnumbered; the numbering starts one level below it.
//! * The **top level** is the shallowest heading level that gets numbered — the
//!   shallowest level in the document, or the shallowest below the title. A document
//!   whose headings start at `###` therefore numbers them `1`, `2`, `3` rather than
//!   `0.0.1`.
//! * A heading of level `L` owns component number `L - top` of the counter, counting
//!   from zero. Entering a component resets everything below it.
//! * A **skipped level** — `#` straight to `###` — leaves a component with no heading
//!   of its own, and it is printed as `0`: `1.0.1`. This is the rule pandoc's
//!   `--number-sections` has always used, and it is a rule rather than a list of
//!   cases: the number of components is a function of the level alone, so two headings
//!   at different levels can never be given sibling numbers, and the numbers can never
//!   disagree with the hierarchy the heading rules draw.
//! * Numbering applies only when the numbered headings use **three or more distinct
//!   levels** ([`MIN_NUMBERED_LEVELS`]). Two levels is `1` and `1.1`, a shape the
//!   reader holds in their head; the third is where orientation starts to cost
//!   something. The title is excluded from that count because it is not a section, and
//!   because otherwise adding a `#` title to a document would conjure numbering it did
//!   not have before.
//!
//! Every heading is therefore either the title (no number) or has a level at or below
//! the top level (a number) — the rule is total by construction, and
//! [`Numbering::label`] answering `None` means either "not numbered" or "not this
//! document".
//!
//! # Where it is computed
//!
//! Once, per render, from the whole document — never at parse time (design spec §3).
//! A block renderer can see a heading's level but never whether it is the only `#` in
//! the document, exactly as with the title banner.

use std::collections::{BTreeMap, BTreeSet};

use crate::doc::Doc;

/// How many distinct numbered levels a document must use before it is numbered at all.
///
/// Three. A flat document needs no orientation aid, and giving it one would be pure
/// noise on the page.
pub const MIN_NUMBERED_LEVELS: usize = 3;

/// The section number of every numbered heading in a document, by anchor id.
///
/// Empty for a document that does not qualify, which is the ordinary case — so
/// [`Numbering::none`] and a shallow document are the same object, and no caller needs
/// a special case for "numbering is off".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Numbering {
    labels: BTreeMap<String, String>,
}

impl Numbering {
    /// No numbering at all.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The numbering of `doc`, or nothing when `enabled` is false.
    ///
    /// The gate lives here so that the option is consulted in one place rather than at
    /// each of the four call sites that need numbers.
    #[must_use]
    pub fn enabled(doc: &Doc, enabled: bool) -> Self {
        if enabled {
            Self::for_doc(doc)
        } else {
            Self::none()
        }
    }

    /// The numbering of `doc`, by the rule in the module documentation.
    #[must_use]
    pub fn for_doc(doc: &Doc) -> Self {
        let title = doc.lone_title().map(|heading| heading.id.as_str());
        let numbered: Vec<(&str, u8)> = doc
            .headings()
            .iter()
            .filter(|heading| Some(heading.id.as_str()) != title)
            .map(|heading| (heading.id.as_str(), heading.level))
            .collect();
        let levels: BTreeSet<u8> = numbered.iter().map(|(_, level)| *level).collect();
        if levels.len() < MIN_NUMBERED_LEVELS {
            return Self::none();
        }
        // The shallowest level actually used, so a document written entirely in `###`
        // numbers from `1` rather than from `0.0.1`. Only *interior* gaps are gaps.
        let top = levels.iter().next().copied().unwrap_or(1);
        let mut counters: Vec<u32> = Vec::new();
        let mut labels = BTreeMap::new();
        for (id, level) in numbered {
            let depth = usize::from(level.saturating_sub(top));
            if depth < counters.len() {
                counters.truncate(depth + 1);
                if let Some(last) = counters.last_mut() {
                    *last += 1;
                }
            } else {
                // A skipped level leaves the components between as zeroes — the
                // ancestor the author did not write.
                counters.resize(depth, 0);
                counters.push(1);
            }
            labels.insert(id.to_string(), join(&counters));
        }
        Self { labels }
    }

    /// The number of the heading with this anchor id, if it has one.
    #[must_use]
    pub fn label(&self, id: &str) -> Option<&str> {
        self.labels.get(id).map(String::as_str)
    }

    /// Whether nothing at all is numbered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

/// Renders a counter stack as `1.2.3`.
fn join(counters: &[u32]) -> String {
    let mut out = String::new();
    for (index, value) in counters.iter().enumerate() {
        if index > 0 {
            out.push('.');
        }
        out.push_str(&value.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers of every heading, in document order.
    fn labels(markdown: &str) -> Vec<String> {
        let doc = Doc::parse(markdown);
        let numbering = Numbering::for_doc(&doc);
        doc.headings()
            .iter()
            .map(|heading| numbering.label(&heading.id).unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn a_document_without_a_title_numbers_its_level_ones() {
        assert_eq!(
            labels("# a\n## b\n### c\n## d\n# e\n"),
            ["1", "1.1", "1.1.1", "1.2", "2"]
        );
    }

    #[test]
    fn a_lone_title_is_unnumbered_and_numbering_starts_below_it() {
        assert_eq!(
            labels("# t\n## a\n### b\n#### c\n## d\n"),
            ["", "1", "1.1", "1.1.1", "2"]
        );
    }

    #[test]
    fn fewer_than_three_levels_is_not_numbered() {
        assert_eq!(labels("# a\n## b\n## c\n# d\n"), ["", "", "", ""]);
        // The title does not count towards the three, so this is two levels, not three.
        assert_eq!(labels("# t\n## a\n### b\n"), ["", "", ""]);
    }

    #[test]
    fn a_skipped_level_is_a_zero_component() {
        // Two `#`s, so neither is a title and the top level is 1.
        assert_eq!(
            labels("# a\n### b\n## c\n#### d\n# e\n"),
            ["1", "1.0.1", "1.1", "1.1.0.1", "2"]
        );
    }

    /// The same rule at the top of the ladder: a section deeper than anything above it.
    ///
    /// Under a title, a `###` before the document's first `##` has no parent to belong
    /// to, and says so with the same zero an interior gap uses. One rule, not two.
    #[test]
    fn a_section_deeper_than_anything_before_it_leads_with_a_zero() {
        assert_eq!(
            labels("# t\n### b\n## c\n#### d\n"),
            ["", "0.1", "1", "1.0.1"]
        );
    }

    #[test]
    fn numbering_starts_at_the_shallowest_level_the_document_uses() {
        assert_eq!(
            labels("### a\n#### b\n##### c\n### d\n"),
            ["1", "1.1", "1.1.1", "2"]
        );
    }

    #[test]
    fn a_heading_after_a_deeper_one_closes_it() {
        assert_eq!(
            labels("# a\n### b\n### c\n# d\n## e\n"),
            ["1", "1.0.1", "1.0.2", "2", "2.1"]
        );
    }

    #[test]
    fn switching_it_off_numbers_nothing() {
        // Four heading levels, because the title does not count towards the three.
        let doc = Doc::parse("# t\n## a\n### b\n#### c\n");
        assert!(Numbering::enabled(&doc, false).is_empty());
        assert!(!Numbering::enabled(&doc, true).is_empty());
    }

    #[test]
    fn a_document_without_headings_is_not_numbered() {
        assert!(Numbering::for_doc(&Doc::parse("just prose\n")).is_empty());
    }
}
