//! Table-of-contents tests.

use super::*;
use crate::canvas::Anchor;

/// Builds a table of contents from Markdown.
fn toc(source: &str) -> Toc {
    Toc::from_doc(&Doc::parse(source))
}

/// A convenient anchor literal.
fn anchor(id: &str, level: u8, row: usize) -> Anchor {
    Anchor {
        id: id.to_string(),
        level,
        row,
    }
}

#[test]
fn a_document_without_headings_has_an_empty_toc() {
    let toc = toc("just a paragraph\n");
    assert!(toc.is_empty());
    assert_eq!(toc.current(0), None);
    assert!(toc.filter("").is_empty());
}

#[test]
fn nesting_follows_heading_levels() {
    let toc = toc("# A\n\n## B\n\n### C\n\n## D\n\n# E\n");
    let depths: Vec<usize> = toc.entries().iter().map(|entry| entry.depth).collect();
    assert_eq!(depths, vec![0, 1, 2, 1, 0]);
    let parents: Vec<Option<&str>> = toc
        .entries()
        .iter()
        .map(|entry| entry.parent.map(|index| toc.entries()[index].text.as_str()))
        .collect();
    assert_eq!(parents, vec![None, Some("A"), Some("B"), Some("A"), None]);
}

#[test]
fn a_skipped_level_still_indents_by_one_step() {
    let toc = toc("# A\n\n### C\n");
    assert_eq!(toc.entries()[1].depth, 1);
    assert_eq!(toc.entries()[1].level, 3);
}

#[test]
fn a_document_starting_deep_is_not_mis_nested() {
    let toc = toc("### A\n\n## B\n\n# C\n");
    let depths: Vec<usize> = toc.entries().iter().map(|entry| entry.depth).collect();
    assert_eq!(depths, vec![0, 0, 0]);
}

#[test]
fn current_section_tracks_the_viewport() {
    let mut toc = toc("# A\n\n## B\n\n# C\n");
    toc.attach_anchors(&[anchor("a", 1, 4), anchor("b", 2, 10), anchor("c", 1, 20)]);
    assert_eq!(toc.current(0), None, "a preamble belongs to no section");
    assert_eq!(toc.current(4), Some(0));
    assert_eq!(toc.current(9), Some(0));
    assert_eq!(toc.current(10), Some(1));
    assert_eq!(toc.current(999), Some(2));
}

#[test]
fn headings_the_renderer_never_emitted_are_skipped() {
    let mut toc = toc("# A\n\n## B\n");
    toc.attach_anchors(&[anchor("a", 1, 0)]);
    assert_eq!(toc.row_of(1), None);
    assert_eq!(toc.current(50), Some(0));
    assert_eq!(toc.next_after(0), None);
}

#[test]
fn anchors_can_be_reattached_after_a_resize() {
    let mut toc = toc("# A\n\n# B\n");
    toc.attach_anchors(&[anchor("a", 1, 0), anchor("b", 1, 9)]);
    assert_eq!(toc.row_of(1), Some(9));
    toc.attach_anchors(&[anchor("a", 1, 0), anchor("b", 1, 22)]);
    assert_eq!(toc.row_of(1), Some(22));
    toc.clear_anchors();
    assert_eq!(toc.row_of(1), None);
}

#[test]
fn heading_stepping_is_strict_in_both_directions() {
    let mut toc = toc("# A\n\n# B\n\n# C\n");
    toc.attach_anchors(&[anchor("a", 1, 0), anchor("b", 1, 10), anchor("c", 1, 20)]);
    assert_eq!(toc.next_after(0), Some(1));
    assert_eq!(toc.next_after(10), Some(2));
    assert_eq!(toc.next_after(20), None);
    assert_eq!(toc.prev_before(20), Some(1));
    assert_eq!(toc.prev_before(0), None);
}

#[test]
fn the_breadcrumb_runs_from_the_root_down() {
    let toc = toc("# A\n\n## B\n\n### C\n");
    assert_eq!(toc.breadcrumb(2), vec![0, 1, 2]);
    assert_eq!(toc.breadcrumb(0), vec![0]);
}

#[test]
fn an_empty_filter_keeps_everything_in_document_order() {
    let toc = toc("# Zebra\n\n# Apple\n");
    let hits: Vec<usize> = toc.filter("   ").iter().map(|hit| hit.index).collect();
    assert_eq!(hits, vec![0, 1]);
}

#[test]
fn the_filter_matches_subsequences_case_insensitively() {
    let toc = toc("# Installation\n\n# Configuration\n\n# Bananas\n");
    let hits: Vec<usize> = toc.filter("cnfg").iter().map(|hit| hit.index).collect();
    assert_eq!(hits, vec![1]);

    let hits: Vec<usize> = toc.filter("gzq").iter().map(|hit| hit.index).collect();
    assert_eq!(
        hits,
        Vec::<usize>::new(),
        "out-of-order letters must not match"
    );

    // An upper-case query matches lower-case text: the filter is never case-sensitive,
    // unlike search, because a heading's capitalisation is not something one recalls.
    let hits: Vec<usize> = toc.filter("BNN").iter().map(|hit| hit.index).collect();
    assert_eq!(hits, vec![2]);
}

#[test]
fn the_filter_prefers_the_tighter_match() {
    let toc = toc("# Table of contents\n\n# Toc\n");
    let hits = toc.filter("toc");
    assert_eq!(hits[0].index, 1, "the exact prefix must win");
    assert_eq!(hits.len(), 2);
}

#[test]
fn filter_positions_index_the_original_text() {
    let toc = toc("# Größe\n");
    let hits = toc.filter("gr");
    assert_eq!(hits.len(), 1);
    let text: Vec<char> = toc.entries()[0].text.chars().collect();
    for position in &hits[0].positions {
        assert!(
            *position < text.len(),
            "position {position} is outside {:?}",
            toc.entries()[0].text
        );
    }
    assert_eq!(hits[0].positions, vec![0, 1]);
}

#[test]
fn filter_positions_survive_case_folding_that_grows() {
    // `İ` lower-cases to two chars; the reported positions must still index the
    // original string, not the folded one.
    let toc = toc("# İstanbul guide\n");
    let hits = toc.filter("guide");
    assert_eq!(hits.len(), 1);
    let text: Vec<char> = toc.entries()[0].text.chars().collect();
    let matched: String = hits[0].positions.iter().map(|index| text[*index]).collect();
    assert_eq!(matched, "guide");
}

#[test]
fn a_filter_that_matches_nothing_returns_nothing() {
    let toc = toc("# Alpha\n\n# Beta\n");
    assert!(toc.filter("zzzz").is_empty());
}

#[test]
fn ids_can_be_looked_up() {
    let toc = toc("# Hello World\n");
    assert_eq!(toc.index_of("hello-world"), Some(0));
    assert_eq!(toc.index_of("nope"), None);
}
