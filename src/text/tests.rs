//! Unit tests for the text primitives.
//!
//! The Unicode corpus used throughout: CJK (double width), emoji (double width),
//! ZWJ sequences, regional-indicator flags, combining marks (zero width), and
//! bidirectional / Indic scripts.

use super::*;
use crate::text::span::{spans_min_width, spans_width};
use crate::theme::{Color, Style};

/// A woman-technologist ZWJ sequence: 👩 + ZWJ + 💻.
const ZWJ: &str = "\u{1f469}\u{200d}\u{1f4bb}";
/// The Swiss flag: two regional indicator symbols.
const FLAG: &str = "\u{1f1e8}\u{1f1ed}";
/// `e` followed by a combining acute accent.
const COMBINING: &str = "e\u{0301}";

#[test]
fn display_width_counts_columns_not_bytes() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(display_width("日本語"), 6);
    assert_eq!(display_width(COMBINING), 1);
    assert_eq!(display_width("naïve"), 5);
}

#[test]
fn grapheme_width_is_clamped_to_two() {
    assert_eq!(grapheme_width("a"), 1);
    assert_eq!(grapheme_width("日"), 2);
    assert_eq!(grapheme_width(ZWJ), 2);
    assert_eq!(grapheme_width(FLAG), 2);
    assert_eq!(grapheme_width(COMBINING), 1);
    assert_eq!(grapheme_width("\u{0301}"), 0);
    assert_eq!(grapheme_width("\u{200d}"), 0);
}

#[test]
fn graphemes_keep_clusters_whole() {
    assert_eq!(graphemes(ZWJ).count(), 1);
    assert_eq!(graphemes(FLAG).count(), 1);
    assert_eq!(graphemes(COMBINING).count(), 1);
    assert_eq!(graphemes("a👩‍💻b").count(), 3);
}

#[test]
fn truncate_never_splits_a_cluster() {
    assert_eq!(truncate_to_width("abcdef", 3), "abc");
    // The second CJK character would straddle column 3, so it is dropped whole.
    assert_eq!(truncate_to_width("日本語", 3), "日");
    assert_eq!(truncate_to_width("日本語", 4), "日本");
    assert_eq!(truncate_to_width(ZWJ, 1), "");
    assert_eq!(truncate_to_width(ZWJ, 2), ZWJ);
    assert_eq!(truncate_to_width(COMBINING, 1), COMBINING);
    assert_eq!(truncate_to_width("", 5), "");
}

#[test]
fn split_at_width_partitions_losslessly() {
    let (head, tail) = split_at_width("日本語", 3);
    assert_eq!(head, "日");
    assert_eq!(tail, "本語");
    assert_eq!(format!("{head}{tail}"), "日本語");
}

#[test]
fn pad_to_width_is_exact_for_every_alignment() {
    for (align, expected) in [
        (Align::Left, "ab   "),
        (Align::Right, "   ab"),
        (Align::Center, " ab  "),
    ] {
        let padded = pad_to_width("ab", 5, align);
        assert_eq!(padded, expected);
        assert_eq!(display_width(&padded), 5);
    }
}

#[test]
fn pad_to_width_handles_wide_characters_exactly() {
    // "日" is two columns; padding to five must add three spaces, not three columns
    // of guesswork.
    let padded = pad_to_width("日", 5, Align::Center);
    assert_eq!(display_width(&padded), 5);
    // An odd leftover after a double-width glyph still yields an exact width.
    let padded = pad_to_width("日本", 5, Align::Center);
    assert_eq!(display_width(&padded), 5);
}

#[test]
fn pad_to_width_truncates_overlong_text() {
    let padded = pad_to_width("abcdef", 3, Align::Left);
    assert_eq!(padded, "abc");
    assert_eq!(display_width(&padded), 3);
}

#[test]
fn repeat_to_width_is_exact() {
    assert_eq!(repeat_to_width("─", 4), "────");
    let wide = repeat_to_width("日", 5);
    assert_eq!(display_width(&wide), 5);
    assert_eq!(wide, "日日 ");
    assert_eq!(repeat_to_width("\u{0301}", 3), "   ");
}

#[test]
fn min_unbreakable_width_uses_the_longest_word() {
    assert_eq!(min_unbreakable_width("a bb ccc"), 3);
    assert_eq!(min_unbreakable_width("日本 語"), 4);
    assert_eq!(min_unbreakable_width("   "), 0);
}

#[test]
fn wrap_breaks_at_spaces() {
    let lines = wrap_plain("the quick brown fox", 10);
    assert_eq!(lines, vec!["the quick", "brown fox"]);
}

#[test]
fn wrap_respects_the_width_budget() {
    for width in 1..30usize {
        for line in wrap_plain("the quick brown fox jumps over 日本語 👩‍💻 text", width) {
            assert!(
                display_width(&line) <= width,
                "line {line:?} exceeds width {width}"
            );
        }
    }
}

#[test]
fn wrap_hard_splits_overlong_words_on_cluster_boundaries() {
    let lines = wrap_plain("aaaaaaaa", 3);
    assert_eq!(lines, vec!["aaa", "aaa", "aa"]);

    let text = format!("{ZWJ}{ZWJ}{ZWJ}");
    let lines = wrap_plain(&text, 4);
    assert_eq!(lines, vec![format!("{ZWJ}{ZWJ}"), ZWJ.to_string()]);
    for line in &lines {
        // Each cluster stays whole: no lone joiners or half emoji.
        assert!(!line.starts_with('\u{200d}'));
    }
}

#[test]
fn wrap_drops_clusters_that_can_never_fit() {
    // A double-width cluster in a one-column budget cannot be shown at all; dropping
    // it keeps the promise that no returned line exceeds the width.
    let lines = wrap_plain("a日b", 1);
    assert_eq!(lines, vec!["a", "b"]);
}

#[test]
fn wrap_keeps_combining_marks_with_their_base() {
    let text = format!("caf{COMBINING} bar");
    let lines = wrap_plain(&text, 4);
    assert_eq!(lines, vec![format!("caf{COMBINING}"), "bar".to_string()]);
    assert_eq!(display_width(&lines[0]), 4);
}

#[test]
fn wrap_handles_wide_scripts() {
    let lines = wrap_plain("日本語 テスト", 6);
    assert_eq!(lines, vec!["日本語", "テスト"]);
    for line in &lines {
        assert_eq!(display_width(line), 6);
    }
}

#[test]
fn wrap_mixed_scripts_stays_within_budget() {
    let text = "Hello 世界 مرحبا नमस्ते 👋 done";
    for width in 4..20usize {
        for line in wrap_plain(text, width) {
            assert!(display_width(&line) <= width);
        }
    }
}

#[test]
fn wrap_honours_explicit_newlines_including_empty_lines() {
    let lines = wrap_plain("a\n\nb", 10);
    assert_eq!(lines, vec!["a", "", "b"]);
}

#[test]
fn wrap_of_zero_width_or_empty_input_is_empty() {
    assert!(wrap_plain("anything", 0).is_empty());
    assert!(wrap_plain("", 10).is_empty());
}

#[test]
fn wrap_preserves_styles_and_merges_equal_runs() {
    let red = Style::new().fg(Color::hex(0xff0000));
    let spans = vec![
        Span::new("hello ", red),
        Span::new("bright ", red),
        Span::new("world", Style::NONE),
    ];
    let lines = wrap_spans(&spans, 13);
    assert_eq!(lines.len(), 2);
    // "hello bright" shares one style and must arrive as a single run.
    assert_eq!(lines[0].spans.len(), 1);
    assert_eq!(lines[0].text(), "hello bright");
    assert_eq!(lines[0].spans[0].style, red);
    assert_eq!(lines[1].text(), "world");
    assert_eq!(lines[1].spans[0].style, Style::NONE);
}

#[test]
fn wrap_splits_words_that_cross_a_style_boundary() {
    let bold = Style::new().bold();
    let spans = vec![Span::new("bo", bold), Span::new("ldword", Style::NONE)];
    // The word is eight columns and does not fit into six.
    let lines = wrap_spans(&spans, 6);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text(), "boldwo");
    assert_eq!(lines[1].text(), "rd");
    assert_eq!(lines[0].spans[0].style, bold);
}

#[test]
fn wrap_keeps_leading_indentation_of_a_segment() {
    let lines = wrap_plain("  indented text", 12);
    assert_eq!(lines, vec!["  indented", "text"]);
    // Whitespace at a wrap point is dropped, so wrapped lines never start with it.
    let lines = wrap_plain("aaa bbb ccc", 4);
    assert!(lines.iter().all(|l| !l.starts_with(' ')));
}

#[test]
fn span_and_line_widths_are_display_widths() {
    let line = Line::new(vec![Span::raw("日"), Span::raw("ab")]);
    assert_eq!(line.width(), 4);
    assert_eq!(line.text(), "日ab");
    assert!(!line.is_empty());
    assert!(Line::empty().is_empty());
}

#[test]
fn line_push_merges_matching_styles_and_ignores_empties() {
    let mut line = Line::empty();
    line.push(Span::raw("a"));
    line.push(Span::raw(""));
    line.push(Span::raw("b"));
    line.push(Span::new("c", Style::new().bold()));
    assert_eq!(line.spans.len(), 2);
    assert_eq!(line.spans[0].text, "ab");
}

#[test]
fn line_patch_style_overlays_every_run() {
    let mut line = Line::new(vec![Span::new("a", Style::new().bold()), Span::raw("b")]);
    let overlay = Style::new().bg(Color::hex(0x333333));
    line.patch_style(overlay);
    assert!(line.spans.iter().all(|s| s.style.bg == overlay.bg));
    assert!(
        line.spans[0]
            .style
            .attrs
            .contains(crate::theme::Attributes::BOLD)
    );
}

#[test]
fn span_helpers_measure_across_style_boundaries() {
    let spans = vec![Span::raw("bo"), Span::new("ld word", Style::new().bold())];
    assert_eq!(spans_width(&spans), 9);
    // "bold" spans a style boundary and counts as one four-column word.
    assert_eq!(spans_min_width(&spans), 4);
}

#[test]
fn align_default_is_left() {
    assert_eq!(Align::default(), Align::Left);
}

#[test]
fn line_truncated_clips_on_cluster_boundaries() {
    let line = Line::new(vec![Span::raw("ab"), Span::raw("日本")]);
    assert_eq!(line.truncated(10).text(), "ab日本");
    assert_eq!(line.truncated(3).text(), "ab");
    // The wide cluster would straddle column four, so it is dropped whole.
    assert_eq!(line.truncated(4).text(), "ab日");
    assert_eq!(line.truncated(0).text(), "");
    assert!(line.truncated(5).width() <= 5);
}
