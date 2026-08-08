//! The contract [`highlight`](mdless::highlight::highlight) owes its callers: line
//! semantics, text preservation, no wrapping, no panics, and a bounded cost.

use std::time::Instant;

use mdless::highlight::highlight;
use mdless::text::display_width;
use mdless::theme::Theme;

/// Concatenating the returned lines must reproduce the input, minus line endings and
/// with tabs expanded — the highlighter colours text, it never rewrites it.
fn assert_text_is_preserved(lang: Option<&str>, src: &str) {
    let theme = Theme::default_dark();
    let got: Vec<String> = highlight(lang, src, &theme)
        .iter()
        .map(|line| line.text())
        .collect();
    let want: Vec<String> = src
        .split_inclusive('\n')
        .map(|line| {
            line.trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string()
        })
        .collect();
    assert_eq!(got, want, "lang={lang:?}");
}

#[test]
fn empty_input_yields_no_lines() {
    let theme = Theme::default_dark();
    assert!(highlight(Some("rust"), "", &theme).is_empty());
    assert!(highlight(None, "", &theme).is_empty());
}

#[test]
fn line_count_follows_the_source_exactly() {
    let theme = Theme::default_dark();
    for (src, want) in [
        ("a", 1),
        ("a\n", 1),
        ("a\n\n", 2),
        ("a\nb", 2),
        ("\n\n\n", 3),
        ("a\r\nb\r\n", 2),
    ] {
        assert_eq!(highlight(Some("rust"), src, &theme).len(), want, "{src:?}");
        assert_eq!(highlight(None, src, &theme).len(), want, "{src:?}");
    }
}

#[test]
fn blank_lines_come_back_empty_rather_than_as_a_newline_span() {
    let theme = Theme::default_dark();
    let lines = highlight(Some("rust"), "let a = 1;\n\nlet b = 2;\n", &theme);
    assert_eq!(lines.len(), 3);
    assert!(lines[1].is_empty());
    assert_eq!(lines[1].width(), 0);
    assert!(
        lines
            .iter()
            .flat_map(|l| &l.spans)
            .all(|s| !s.text.contains('\n')),
        "no span may carry a line ending"
    );
}

#[test]
fn text_is_preserved_for_highlighted_and_plain_paths() {
    let cases: &[(Option<&str>, &str)] = &[
        (Some("rust"), "fn main() {\n    let x = 1;\n}\n"),
        (Some("python"), "def f():\n    return '\u{4f60}\u{597d}'\n"),
        (None, "no language tag here\nsecond line\n"),
        (Some("nonexistent-lang"), "still\njust text\n"),
        (Some("json"), "{\"a\": [1, 2, 3]}\n"),
    ];
    for &(lang, src) in cases {
        assert_text_is_preserved(lang, src);
    }
}

#[test]
fn unknown_and_absent_tags_produce_one_plain_span_per_line() {
    let theme = Theme::default_dark();
    for lang in [None, Some(""), Some("totally-made-up")] {
        let lines = highlight(lang, "alpha beta\ngamma\n", &theme);
        assert_eq!(lines.len(), 2, "{lang:?}");
        for line in &lines {
            assert_eq!(line.spans.len(), 1, "{lang:?}");
            assert_eq!(line.spans[0].style, theme.code.text, "{lang:?}");
        }
    }
}

#[test]
fn long_lines_are_returned_intact_and_never_wrapped() {
    let theme = Theme::default_dark();
    let long = format!("let s = \"{}\";\n", "x".repeat(4000));
    let lines = highlight(Some("rust"), &long, &theme);
    assert_eq!(lines.len(), 1, "a long line must not be split");
    assert_eq!(
        lines[0].width(),
        long.trim_end_matches('\n').chars().count()
    );
}

#[test]
fn tabs_become_spaces_on_tab_stops_in_both_paths() {
    let theme = Theme::default_dark();
    for lang in [Some("rust"), Some("no-such-language"), None] {
        let lines = highlight(lang, "\tlet x = 1;\n\t\ty\n", &theme);
        assert_eq!(lines[0].text(), "    let x = 1;", "{lang:?}");
        assert_eq!(lines[1].text(), "        y", "{lang:?}");
        assert!(
            lines
                .iter()
                .flat_map(|l| &l.spans)
                .all(|s| !s.text.contains('\t')),
            "{lang:?}: a tab reached the canvas"
        );
    }
}

#[test]
fn tab_stops_are_measured_across_span_boundaries() {
    let theme = Theme::default_dark();
    // `let` and the tab that follows it land in different highlighted spans, so the
    // tab stop is only right if the column is tracked for the whole line.
    let lines = highlight(Some("rust"), "let\tx = 1;\n", &theme);
    assert_eq!(lines[0].text(), "let x = 1;");
}

#[test]
fn cjk_and_emoji_survive_in_strings_and_comments() {
    let theme = Theme::default_dark();
    let src = "// \u{6ce8}\u{91c8} \u{1f680}\nlet s = \"\u{3053}\u{3093}\u{306b}\u{3061}\u{306f} \u{1f44b}\u{1f3fd}\";\n";
    let lines = highlight(Some("rust"), src, &theme);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text(), "// \u{6ce8}\u{91c8} \u{1f680}");
    assert!(
        lines[1]
            .text()
            .contains("\u{3053}\u{3093}\u{306b}\u{3061}\u{306f} \u{1f44b}\u{1f3fd}")
    );

    // Widths come from `text::display_width`, so a double-width cluster counts twice
    // and the ZWJ-free skin-tone emoji is not split.
    assert_eq!(
        lines[0].width(),
        display_width("// \u{6ce8}\u{91c8} \u{1f680}")
    );
    assert!(
        lines
            .iter()
            .flat_map(|l| &l.spans)
            .all(|s| s.text.is_char_boundary(0)),
        "spans must start on a character boundary"
    );
}

#[test]
fn a_comment_containing_cjk_is_still_one_comment() {
    let theme = Theme::default_dark();
    let lines = highlight(
        Some("python"),
        "# \u{4f60}\u{597d}\u{4e16}\u{754c}\n",
        &theme,
    );
    assert_eq!(lines.len(), 1);
    for span in &lines[0].spans {
        assert_eq!(span.style, theme.code.comment);
    }
}

#[test]
fn malformed_and_hostile_input_does_not_panic() {
    let theme = Theme::default_dark();
    for src in [
        "\"unterminated string\n",
        "/* unterminated comment\n",
        "\u{feff}let x = 1;\n",
        "\u{0}\u{1}\u{7f}\n",
        "}}}}{{{{\n",
        "let s = \"\u{1f1e9}\u{1f1ea}\";\n",
    ] {
        for lang in [Some("rust"), Some("python"), Some("html"), None] {
            let lines = highlight(lang, src, &theme);
            assert!(!lines.is_empty(), "{lang:?} {src:?}");
        }
    }
}

/// Highlighting a 5000-line block must stay well inside "a keypress felt instant".
///
/// The bound is generous — the machine is shared and an unoptimised `cargo test` build
/// of `fancy-regex` is roughly ten times slower than a release build — but it is still
/// far below the point where the cost would be a rewrite rather than a regression.
#[test]
fn a_five_thousand_line_block_highlights_quickly() {
    let theme = Theme::default_dark();
    let unit = "pub fn compute(alpha: u32, beta: &str) -> Option<String> {\n\
                \x20   // a comment about things\n\
                \x20   let value = alpha * 2 + 1;\n\
                \x20   println!(\"value = {value} {}\", beta);\n\
                \x20   Some(format!(\"{value}\"))\n\
                }\n";
    let src = unit.repeat(834);
    assert!(src.lines().count() >= 5000);

    let started = Instant::now();
    let lines = highlight(Some("rust"), &src, &theme);
    let elapsed = started.elapsed();
    assert_eq!(lines.len(), 5004);

    let budget = if cfg!(debug_assertions) { 40.0 } else { 4.0 };
    assert!(
        elapsed.as_secs_f64() < budget,
        "highlighting 5000 lines took {elapsed:?}, budget was {budget}s"
    );
}

/// The plain path is the fallback for oversized blocks, so it must be cheap.
#[test]
fn the_plain_path_is_essentially_free() {
    let theme = Theme::default_dark();
    let src = "some untagged text line\n".repeat(20_000);
    let started = Instant::now();
    let lines = highlight(None, &src, &theme);
    let elapsed = started.elapsed();
    assert_eq!(lines.len(), 20_000);
    let budget = if cfg!(debug_assertions) { 5.0 } else { 1.0 };
    assert!(
        elapsed.as_secs_f64() < budget,
        "plain rendering took {elapsed:?}, budget was {budget}s"
    );
}
