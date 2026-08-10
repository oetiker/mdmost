//! Search tests.

use super::*;

/// A span mapping a source range onto one canvas run.
fn span(source_start: usize, source_end: usize, row: usize, col: u16, cols: u16) -> SearchSpan {
    SearchSpan {
        source_start,
        source_end,
        row,
        col,
        cols,
    }
}

/// Runs a literal search and returns the source ranges it found.
fn ranges(source: &str, query: &str) -> Vec<(usize, usize)> {
    Search::new(source, query, SearchMode::Literal)
        .expect("a literal search cannot fail")
        .source_hits()
        .iter()
        .map(|hit| (hit.source_start, hit.source_end))
        .collect()
}

#[test]
fn an_empty_query_matches_nothing() {
    assert!(ranges("anything at all", "").is_empty());
}

#[test]
fn a_lower_case_query_ignores_case() {
    let search = Search::new("Hello HELLO hello", "hello", SearchMode::Literal).expect("valid");
    assert!(!search.is_case_sensitive());
    assert_eq!(search.source_hits().len(), 3);
}

#[test]
fn a_query_with_an_upper_case_letter_is_case_sensitive() {
    let search = Search::new("Hello HELLO hello", "Hello", SearchMode::Literal).expect("valid");
    assert!(search.is_case_sensitive());
    assert_eq!(search.source_hits().len(), 1);
    assert_eq!(search.source_hits()[0].source_start, 0);
}

#[test]
fn matches_do_not_overlap() {
    assert_eq!(ranges("aaaa", "aa"), vec![(0, 2), (2, 4)]);
}

#[test]
fn offsets_stay_valid_when_case_folding_changes_byte_length() {
    // `ẞ` is three bytes and lower-cases to the two-byte `ß`. Lower-casing the whole
    // haystack first — the obvious implementation — would shift every offset after it
    // by one byte and land the highlight in the wrong place.
    let source = "STRAẞE and straße";
    let hits = ranges(source, "straße");
    assert_eq!(hits.len(), 2, "both spellings must match");
    for (start, end) in &hits {
        assert!(
            source.is_char_boundary(*start),
            "start {start} off boundary"
        );
        assert!(source.is_char_boundary(*end), "end {end} off boundary");
    }
    assert_eq!(&source[hits[0].0..hits[0].1], "STRAẞE");
    assert_eq!(&source[hits[1].0..hits[1].1], "straße");

    let (start, end) = ranges(source, "and")[0];
    assert_eq!(&source[start..end], "and");
}

#[test]
fn folding_that_changes_character_count_never_reports_a_split_match() {
    // `İ` lower-cases to two characters (`i` plus a combining dot). A match must not
    // be reported as ending halfway through that expansion.
    let source = "İstanbul";
    for (start, end) in ranges(source, "i") {
        assert!(source.is_char_boundary(start));
        assert!(source.is_char_boundary(end));
    }
}

#[test]
fn multi_byte_matches_report_byte_ranges() {
    let source = "Grüße aus München";
    let hits = ranges(source, "münchen");
    assert_eq!(hits.len(), 1);
    let (start, end) = hits[0];
    assert_eq!(&source[start..end], "München");
}

#[test]
fn regex_mode_compiles_and_matches() {
    let search = Search::new("a1 b22 c333", r"[a-z]\d{2,}", SearchMode::Regex).expect("valid");
    assert_eq!(search.source_hits().len(), 2);
}

#[test]
fn an_invalid_pattern_is_an_error_not_a_panic() {
    let error = Search::new("text", "([unclosed", SearchMode::Regex).unwrap_err();
    assert!(matches!(error, SearchError::BadPattern(_)));
}

#[test]
fn regex_mode_honours_smart_case() {
    let lower = Search::new("Cat cat", "cat", SearchMode::Regex).expect("valid");
    assert_eq!(lower.source_hits().len(), 2);
    let upper = Search::new("Cat cat", "Cat", SearchMode::Regex).expect("valid");
    assert_eq!(upper.source_hits().len(), 1);
}

#[test]
fn located_matches_map_onto_canvas_positions() {
    let source = "hello world";
    let mut search = Search::new(source, "world", SearchMode::Literal).expect("valid");
    search.locate(source, &[span(0, 11, 3, 2, 11)]);
    assert_eq!(search.len(), 1);
    assert_eq!(
        search.hits()[0].segments,
        vec![Segment {
            row: 3,
            col: 2 + 6,
            cols: 5
        }]
    );
}

#[test]
fn a_match_split_across_a_line_wrap_highlights_in_both_rows() {
    let source = "alpha beta";
    let search = Search::new(source, "alphabeta", SearchMode::Literal).expect("valid");
    // No such literal exists, so use a query that spans the two rendered spans.
    assert!(search.source_hits().is_empty());

    let mut search = Search::new(source, "a b", SearchMode::Literal).expect("valid");
    // "a b" occupies bytes 4..7, which straddles the two spans below.
    search.locate(source, &[span(0, 5, 0, 0, 5), span(5, 10, 1, 0, 4)]);
    assert_eq!(search.len(), 1);
    let rows: Vec<usize> = search.hits()[0]
        .segments
        .iter()
        .map(|segment| segment.row)
        .collect();
    assert_eq!(rows, vec![0, 1], "both rows must be highlighted");
}

#[test]
fn matches_the_renderer_never_drew_are_dropped() {
    let source = "<p>hidden</p> visible";
    let mut search = Search::new(source, "hidden", SearchMode::Literal).expect("valid");
    assert_eq!(search.source_hits().len(), 1);
    // The renderer emitted a span only for the visible text.
    search.locate(source, &[span(14, 21, 0, 0, 7)]);
    assert_eq!(search.len(), 0, "unreachable matches must not be counted");
    assert!(search.is_empty());
}

#[test]
fn locating_is_idempotent() {
    let source = "one two one";
    let spans = [span(0, 11, 0, 0, 11)];
    let mut search = Search::new(source, "one", SearchMode::Literal).expect("valid");
    search.locate(source, &spans);
    let first = search.hits().to_vec();
    search.locate(source, &spans);
    assert_eq!(search.hits(), first.as_slice());
}

#[test]
fn stepping_wraps_around_in_both_directions() {
    let source = "x x x";
    let mut search = Search::new(source, "x", SearchMode::Literal).expect("valid");
    search.locate(
        source,
        &[
            span(0, 1, 0, 0, 1),
            span(2, 3, 5, 0, 1),
            span(4, 5, 9, 0, 1),
        ],
    );
    assert_eq!(search.len(), 3);
    assert_eq!(search.step(None, true), Some(0));
    assert_eq!(search.step(Some(2), true), Some(0), "forward must wrap");
    assert_eq!(search.step(None, false), Some(2));
    assert_eq!(search.step(Some(0), false), Some(2), "backward must wrap");
}

#[test]
fn stepping_an_empty_search_yields_nothing() {
    let search = Search::empty();
    assert_eq!(search.step(None, true), None);
    assert_eq!(search.step(Some(0), false), None);
    assert!(search.is_empty());
}

#[test]
fn matches_can_be_found_relative_to_a_row() {
    let source = "x x x";
    let mut search = Search::new(source, "x", SearchMode::Literal).expect("valid");
    search.locate(
        source,
        &[
            span(0, 1, 0, 0, 1),
            span(2, 3, 5, 0, 1),
            span(4, 5, 9, 0, 1),
        ],
    );
    assert_eq!(search.first_at_or_after(1, false), Some(1));
    assert_eq!(search.first_at_or_after(10, false), None);
    assert_eq!(search.first_at_or_after(10, true), Some(0));
    assert_eq!(search.last_at_or_before(6, false), Some(1));
    assert_eq!(search.last_at_or_before(0, false), Some(0));
}

#[test]
fn segments_can_be_looked_up_by_row() {
    let source = "one two";
    let mut search = Search::new(source, "o", SearchMode::Literal).expect("valid");
    search.locate(source, &[span(0, 7, 2, 0, 7)]);
    let on_row: Vec<(usize, Segment)> = search.segments_on_row(2).collect();
    assert_eq!(on_row.len(), 2);
    assert!(search.segments_on_row(3).next().is_none());
}

#[test]
fn clearing_the_location_keeps_the_source_matches() {
    let source = "abc";
    let mut search = Search::new(source, "b", SearchMode::Literal).expect("valid");
    search.locate(source, &[span(0, 3, 0, 0, 3)]);
    assert_eq!(search.len(), 1);
    search.clear_location();
    assert_eq!(search.len(), 0);
    assert_eq!(search.source_hits().len(), 1);
}

/// Renders `markdown` and runs a located literal search over it, the way the pager
/// does: render, then `Search::locate` against the canvas's own spans.
fn hits_for(markdown: &str, query: &str) -> Vec<Hit> {
    let doc = crate::doc::Doc::parse(markdown);
    let theme = crate::theme::Theme::default_dark();
    let options = crate::render::RenderOptions::default();
    let canvas = crate::render::render_flat(&doc, 40, &theme, &options);
    let mut search = Search::new(doc.source(), query, SearchMode::Literal).expect("valid pattern");
    search.locate(doc.source(), canvas.spans());
    search.hits().to_vec()
}

#[test]
fn search_matches_inside_a_fenced_code_block() {
    let markdown = "text\n\n```rust\nlet needle = 1;\n```\n";
    let hits = hits_for(markdown, "needle");
    assert_eq!(hits.len(), 1, "the fence is searchable");
    assert!(
        !hits[0].segments.is_empty(),
        "and the hit has cells to draw"
    );
}

#[test]
fn search_matches_inside_a_quoted_fence() {
    let markdown = "> ```\n> let needle = 1;\n> ```\n";
    let hits = hits_for(markdown, "needle");
    assert_eq!(hits.len(), 1);
}
