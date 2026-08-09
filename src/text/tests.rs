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

/// A wide base character followed by a spacing combining mark (`Mc`): one grapheme
/// cluster that genuinely draws three columns, which is more than a cell can hold.
const WIDE_PLUS_SPACING_MARK: &str = "\u{17000}\u{1A57}";

/// A ZWJ sequence followed by a spacing combining mark (`Mc`): one grapheme cluster
/// that draws three columns, so it must be split — but the ZWJ sequence inside it
/// draws two columns *only while it stays joined*, so the split may not fall between
/// the joined halves.
const ZWJ_PLUS_SPACING_MARK: &str = "\u{1f600}\u{200d}\u{1f600}\u{1A57}";

/// U+17D8 KHMER SIGN BEYYAL: a *single scalar* that draws three columns. It is the only
/// one in Unicode under `unicode-width` 0.2, but the interesting thing about it is not
/// its identity — it is that no split of it exists at all, because there is nothing
/// inside it to split. A cluster like that cannot be put in cells; it is replaced.
const INDIVISIBLE_TOO_WIDE: &str = "\u{17D8}";

/// The same sign carrying a spacing mark (`Mc`): four columns, and still nothing that a
/// width-preserving split could cut, because the only interior boundary leaves a
/// three-column head.
const INDIVISIBLE_TOO_WIDE_MARKED: &str = "\u{17D8}\u{093B}";

/// The marker an unplaceable cluster is replaced by.
const MARKER: &str = "\u{FFFD}";

#[test]
fn display_width_counts_columns_not_bytes() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(display_width("日本語"), 6);
    assert_eq!(display_width(COMBINING), 1);
    assert_eq!(display_width("naïve"), 5);
}

#[test]
fn grapheme_width_measures_a_one_cell_piece() {
    assert_eq!(grapheme_width("a"), 1);
    assert_eq!(grapheme_width("日"), 2);
    // A ZWJ sequence and a flag are each one cluster that really is two columns, so
    // the clamp never fires for them.
    assert_eq!(grapheme_width(ZWJ), 2);
    assert_eq!(grapheme_width(FLAG), 2);
    assert_eq!(grapheme_width(COMBINING), 1);
    assert_eq!(grapheme_width("\u{0301}"), 0);
    assert_eq!(grapheme_width("\u{200d}"), 0);
}

#[test]
fn grapheme_width_clamps_only_as_a_backstop() {
    // A wide base plus a spacing mark genuinely draws three columns; the clamp would
    // report two. This is why cell fillers must split with `cell_clusters` first.
    assert_eq!(display_width(WIDE_PLUS_SPACING_MARK), 3);
    assert_eq!(grapheme_width(WIDE_PLUS_SPACING_MARK), 2);
}

#[test]
fn cell_clusters_split_a_cluster_too_wide_for_one_cell() {
    let pieces: Vec<&str> = cell_clusters(WIDE_PLUS_SPACING_MARK).collect();
    assert_eq!(pieces, vec!["\u{17000}", "\u{1A57}"]);
    assert_eq!(pieces.iter().map(|p| display_width(p)).sum::<usize>(), 3);
}

#[test]
fn cell_clusters_never_split_inside_a_joined_sequence() {
    // Splitting between the two halves of the ZWJ sequence yields pieces measuring
    // 2 + 2 + 1 = 5 for a cluster that draws 3: each piece is honest on its own, but
    // writing them into adjacent cells re-joins them and the row comes up two columns
    // short. The split has to fall after the joined run, not inside it.
    assert_eq!(display_width(ZWJ_PLUS_SPACING_MARK), 3);
    let pieces: Vec<&str> = cell_clusters(ZWJ_PLUS_SPACING_MARK).collect();
    assert_eq!(pieces, vec!["\u{1f600}\u{200d}\u{1f600}", "\u{1A57}"]);
    assert_eq!(
        pieces.iter().map(|p| display_width(p)).sum::<usize>(),
        display_width(ZWJ_PLUS_SPACING_MARK),
        "the pieces must account for exactly the columns the cluster draws"
    );
}

#[test]
fn cell_clusters_split_width_preservingly_across_the_corpus() {
    // The property the splitter exists to hold: pieces round-trip, each fits a cell,
    // and their widths sum to what the whole draws. A split that breaks the last one
    // is invisible per-piece and only shows up as a short row on screen.
    for text in [
        WIDE_PLUS_SPACING_MARK,
        ZWJ_PLUS_SPACING_MARK,
        ZWJ,
        FLAG,
        COMBINING,
        "\u{1f469}\u{200d}\u{1f4bb}\u{1A57}",
        "\u{1f600}\u{200d}\u{1f600}\u{200d}\u{1f600}\u{1A57}",
        "a\u{1f600}\u{200d}\u{1f600}\u{1A57}日",
    ] {
        let pieces: Vec<&str> = cell_clusters(text).collect();
        assert_eq!(pieces.concat(), text, "round trip failed for {text:?}");
        for piece in &pieces {
            assert!(
                display_width(piece) <= 2,
                "{piece:?} of {text:?} draws {} columns",
                display_width(piece)
            );
        }
        assert_eq!(
            pieces.iter().map(|p| display_width(p)).sum::<usize>(),
            display_width(text),
            "pieces of {text:?} do not account for its columns: {pieces:?}"
        );
    }
}

#[test]
fn cell_clusters_replace_a_cluster_that_cannot_be_split_at_all() {
    // U+17D8 is one scalar drawing three columns. There is no boundary inside it, so
    // no split can preserve its width; handing it to a cell would make the cell claim
    // two columns and draw three. It is replaced by a marker padded to its own width,
    // which keeps the column arithmetic every caller already did with `display_width`
    // exactly right.
    assert_eq!(display_width(INDIVISIBLE_TOO_WIDE), 3);
    let pieces: Vec<&str> = cell_clusters(INDIVISIBLE_TOO_WIDE).collect();
    assert_eq!(pieces, vec![MARKER, " ", " "]);
    assert_eq!(
        pieces.iter().map(|p| display_width(p)).sum::<usize>(),
        display_width(INDIVISIBLE_TOO_WIDE)
    );
}

#[test]
fn cell_clusters_replace_the_whole_cluster_marks_and_all() {
    // The marks belong to the base that was replaced; leaving one behind would strand a
    // combining mark against a blank.
    assert_eq!(display_width(INDIVISIBLE_TOO_WIDE_MARKED), 4);
    let pieces: Vec<&str> = cell_clusters(INDIVISIBLE_TOO_WIDE_MARKED).collect();
    assert_eq!(pieces, vec![MARKER, " ", " ", " "]);
    assert_eq!(
        pieces.iter().map(|p| display_width(p)).sum::<usize>(),
        display_width(INDIVISIBLE_TOO_WIDE_MARKED)
    );
}

#[test]
fn cell_clusters_replace_only_what_they_cannot_split() {
    // Neighbouring text is untouched, and the replacement is exactly as wide as the
    // cluster it stands in for, so everything after it stays in its column.
    let pieces: Vec<&str> = cell_clusters("a\u{17D8}日").collect();
    assert_eq!(pieces, vec!["a", MARKER, " ", " ", "日"]);
    assert_eq!(
        pieces.iter().map(|p| display_width(p)).sum::<usize>(),
        display_width("a\u{17D8}日")
    );
}

#[test]
fn every_cell_piece_of_every_scalar_fits_in_a_cell() {
    // The class, not the instance: whatever Unicode adds next, no piece may be wider
    // than a cell and the pieces must account for exactly the columns the text draws.
    // This is the property `Cell::new`'s assertion defends, proved without relying on
    // a randomly seeded test having tried the right character.
    let mut buffer = [0u8; 4];
    for scalar in 0u32..=0x10FFFF {
        let Some(ch) = char::from_u32(scalar) else {
            continue;
        };
        let text: &str = ch.encode_utf8(&mut buffer);
        let pieces: Vec<&str> = cell_clusters(text).collect();
        for piece in &pieces {
            assert!(
                display_width(piece) <= 2,
                "U+{scalar:04X}: piece {piece:?} draws {} columns",
                display_width(piece)
            );
        }
        assert_eq!(
            pieces.iter().map(|p| display_width(p)).sum::<usize>(),
            display_width(text),
            "U+{scalar:04X}: pieces {pieces:?} do not account for its columns"
        );
    }
}

#[test]
fn cell_pieces_account_for_the_columns_of_composed_clusters() {
    // The families that share the failure mode: an over-wide base with marks on it, a
    // joined sequence with a mark, a flag with a mark, a variation-selected emoji, and
    // an unsplittable sign with and without marks.
    for text in [
        INDIVISIBLE_TOO_WIDE,
        INDIVISIBLE_TOO_WIDE_MARKED,
        "\u{17D8}\u{0301}",
        "\u{17D8}\u{17D8}",
        WIDE_PLUS_SPACING_MARK,
        ZWJ_PLUS_SPACING_MARK,
        "\u{1f1e8}\u{1f1ed}\u{1A57}",
        "\u{2764}\u{fe0f}",
        "\u{2764}\u{fe0f}\u{1A57}",
        "日\u{1A57}",
        "日\u{0301}",
        "\u{1f469}\u{200d}\u{1f4bb}\u{1A57}",
        "a\u{17D8}b\u{17000}\u{1A57}日",
    ] {
        let pieces: Vec<&str> = cell_clusters(text).collect();
        for piece in &pieces {
            assert!(
                display_width(piece) <= 2,
                "{text:?}: piece {piece:?} draws {} columns",
                display_width(piece)
            );
        }
        assert_eq!(
            pieces.iter().map(|p| display_width(p)).sum::<usize>(),
            display_width(text),
            "{text:?}: pieces {pieces:?} do not account for its columns"
        );
    }
}

#[test]
fn cell_clusters_leave_every_cluster_that_fits_alone() {
    for text in [ZWJ, FLAG, COMBINING, "日", "a", "\u{0301}"] {
        let pieces: Vec<&str> = cell_clusters(text).collect();
        assert_eq!(pieces, vec![text], "{text:?} must not be split");
    }
}

#[test]
fn cell_clusters_never_lose_or_reorder_text() {
    for text in [
        "",
        "plain",
        WIDE_PLUS_SPACING_MARK,
        ZWJ,
        FLAG,
        "mixed 日本 \u{17000}\u{1A57} tail",
    ] {
        let rejoined: String = cell_clusters(text).collect();
        assert_eq!(rejoined, text, "round trip failed for {text:?}");
    }
}

/// A `TAB` is priced at one column by every measurement in the program and drawn at
/// the next tab stop by every terminal, so it may never reach a cell. The machinery
/// that expands tabs against a real column lives in `highlight`, and runs before
/// anything measures a code line; inline text has no such column to expand against —
/// its tab was counted as one — so what a cell may hold is one column of whitespace.
#[test]
fn cell_clusters_substitute_a_space_for_a_tab() {
    let pieces: Vec<&str> = cell_clusters("A\tB").collect();
    assert_eq!(pieces, vec!["A", " ", "B"]);
}

/// The class, not the instance: `TAB` was the reported defect, but `ESC` is the
/// dangerous one — a document carrying it could paint the terminal from inside a
/// paragraph — and both are Unicode `Cc`.
#[test]
fn cell_clusters_never_yield_a_control_character() {
    let mut buffer = [0u8; 4];
    for scalar in 0u32..=0x10FFFF {
        let Some(ch) = char::from_u32(scalar) else {
            continue;
        };
        let text: &str = ch.encode_utf8(&mut buffer);
        for piece in cell_clusters(text) {
            assert!(
                !piece.chars().any(char::is_control),
                "U+{scalar:04X} reached a cell as {piece:?}"
            );
        }
    }
}

/// The substitution has to be width-preserving in both directions, or the row it is on
/// stops being exactly as wide as it was laid out to be.
#[test]
fn a_substituted_control_character_costs_exactly_the_column_it_was_measured_at() {
    for text in ["a\tb", "\u{7}", "\u{1b}[31m", "\0", "\u{9b}", "日\tx"] {
        let pieces: Vec<&str> = cell_clusters(text).collect();
        assert_eq!(
            pieces.iter().map(|p| display_width(p)).sum::<usize>(),
            display_width(text),
            "{text:?}: pieces {pieces:?} do not account for its columns"
        );
    }
}

/// A control character carrying a combining mark is one grapheme cluster; the mark is
/// not a control and must survive to be merged into the substituted cell.
#[test]
fn cell_clusters_take_a_control_character_off_its_cluster() {
    assert_eq!(
        cell_clusters("\r\n").collect::<Vec<_>>(),
        vec![" ", " "],
        "each control in a CRLF cluster owes its own column"
    );
    assert_eq!(
        cell_clusters("\u{7}\u{0301}").collect::<Vec<_>>(),
        vec![MARKER, "\u{0301}"]
    );
}

#[test]
fn every_cell_piece_fits_in_a_cell() {
    let text = "a日\u{17000}\u{1A57}\u{1F469}\u{200D}\u{1F4BB}e\u{0301}";
    for piece in cell_clusters(text) {
        assert!(
            display_width(piece) <= 2,
            "{piece:?} draws {} columns",
            display_width(piece)
        );
    }
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

#[test]
fn distribute_evenly_hands_out_exactly_the_slack() {
    let mut slots = [0usize; 3];
    distribute_evenly(&mut slots, 4);
    assert_eq!(slots, [2, 1, 1], "the leftovers go to the leftmost slots");
    assert_eq!(slots.iter().sum::<usize>(), 4);
}

#[test]
fn distribute_evenly_adds_to_what_is_already_there() {
    let mut slots = [5usize, 1, 9];
    distribute_evenly(&mut slots, 6);
    assert_eq!(slots, [7, 3, 11]);
}

#[test]
fn distribute_evenly_is_a_no_op_when_there_is_nothing_to_do() {
    let mut slots = [3usize, 4];
    distribute_evenly(&mut slots, 0);
    assert_eq!(slots, [3, 4]);

    // An empty slice swallows the slack rather than dividing by zero.
    let mut none: [usize; 0] = [];
    distribute_evenly(&mut none, 7);
    assert!(none.is_empty());
}

#[test]
fn distribute_evenly_totals_correctly_for_every_shape() {
    for count in 1..8usize {
        for extra in 0..20usize {
            let mut slots = vec![0usize; count];
            distribute_evenly(&mut slots, extra);
            assert_eq!(slots.iter().sum::<usize>(), extra, "{count} slots, {extra}");
            let (max, min) = (
                slots.iter().max().copied().unwrap_or(0),
                slots.iter().min().copied().unwrap_or(0),
            );
            assert!(max - min <= 1, "slack must be spread evenly: {slots:?}");
        }
    }
}

#[test]
fn ellipsize_leaves_text_that_fits_alone() {
    assert_eq!(ellipsize("hello", 10), "hello");
    assert_eq!(ellipsize("hello", 5), "hello");
    assert_eq!(ellipsize("", 0), "");
}

#[test]
fn ellipsize_marks_the_cut() {
    assert_eq!(ellipsize("hello", 4), "hel…");
    assert_eq!(ellipsize("hello", 1), "…");
    assert_eq!(ellipsize("hello", 0), "");
}

#[test]
fn ellipsize_never_splits_a_cluster() {
    // A double-width cluster that would straddle the limit is dropped, not halved,
    // so the result can be a column narrower than the budget but never wider.
    assert_eq!(ellipsize("日本語", 4), "日…");
    assert_eq!(ellipsize(&format!("{ZWJ}{ZWJ}"), 3), format!("{ZWJ}…"));
    assert_eq!(ellipsize(&format!("caf{COMBINING}x"), 4), "caf…");
}

#[test]
fn ellipsize_never_exceeds_its_budget() {
    let samples = [
        "plain text that is quite long",
        "日本語のテキスト",
        WIDE_PLUS_SPACING_MARK,
        ZWJ,
        FLAG,
        COMBINING,
        "mixed 日本 \u{17000}\u{1A57} tail",
    ];
    for text in samples {
        for width in 0..14usize {
            let cut = ellipsize(text, width);
            assert!(
                display_width(&cut) <= width,
                "{text:?} at {width}: {cut:?} draws {}",
                display_width(&cut)
            );
        }
    }
}

#[test]
fn truncate_to_width_costs_a_wide_cluster_honestly() {
    // Three columns of content, two columns of budget: the cluster cannot fit at all.
    assert_eq!(truncate_to_width(WIDE_PLUS_SPACING_MARK, 2), "");
    assert_eq!(
        truncate_to_width(WIDE_PLUS_SPACING_MARK, 3),
        WIDE_PLUS_SPACING_MARK
    );
}
