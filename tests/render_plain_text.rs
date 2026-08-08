//! `$PAGER` behaviour: a stream that is not Markdown must survive being paged.
//!
//! `export PAGER=mdless` points `git log`, `--help` output and man pages at this
//! program. Usability review P17 found all three mangled — line breaks reflowed away,
//! e-mail addresses turned into `(mailto:…)` links, indented bodies framed as code.

use mdless::doc::Doc;
use mdless::render::{RenderOptions, render_document};
use mdless::theme::Theme;

/// The output of a `git log`-shaped stream, one string per non-blank row.
fn lines(source: &str, width: u16) -> Vec<String> {
    let doc = Doc::parse_auto(source);
    let canvas = render_document(
        &doc,
        width,
        &Theme::default_dark(),
        &RenderOptions::new(false, false),
    );
    (0..canvas.height())
        .map(|row| canvas.row_text(row).trim().to_string())
        .filter(|row| !row.is_empty())
        .collect()
}

/// A verbatim `git log` extract: the exact shape P17 reported.
const GIT_LOG: &str = "\
commit 2041ef3b9c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f
Author: Tobias Oetiker <tobi@oetiker.ch>
Date:   Sat Aug 8 21:34:28 2026 +0200

    feat: sequence, pie and gantt renderers wired

    A body paragraph indented by four spaces, which Markdown would
    otherwise read as an indented code block.
";

#[test]
fn a_git_log_keeps_its_line_breaks() {
    let out = lines(GIT_LOG, 80);
    assert_eq!(out[0], "commit 2041ef3b9c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f");
    assert!(
        out[1].starts_with("Author: Tobias Oetiker"),
        "the header lines must not be reflowed into one paragraph: {out:?}"
    );
    assert!(out[2].starts_with("Date:"), "{out:?}");
}

#[test]
fn a_git_log_does_not_grow_mailto_links_or_code_frames() {
    let joined = lines(GIT_LOG, 80).join("\n");
    assert!(!joined.contains("mailto:"), "{joined}");
    assert!(
        !joined.contains('╭') && !joined.contains('│'),
        "an indented body is not a code block here: {joined}"
    );
    assert!(joined.contains("<tobi@oetiker.ch>"), "{joined}");
}

#[test]
fn a_plain_stream_still_wraps_to_the_terminal() {
    let source = format!("{}\n", "word ".repeat(40));
    let out = lines(&source, 40);
    assert!(out.len() > 1, "a long line must wrap: {out:?}");
    for row in &out {
        assert!(row.len() <= 40, "{row:?}");
    }
}

#[test]
fn anything_carrying_markdown_still_takes_the_markdown_path() {
    for source in [
        "# Heading\n\ntext\n",
        "- item\n- item\n",
        "> quoted\n",
        "```\ncode\n```\n",
        "| a | b |\n|---|---|\n| 1 | 2 |\n",
        "see the [docs](http://example.com)\n",
        "1. first\n2. second\n",
    ] {
        let doc = Doc::parse_auto(source);
        assert_eq!(
            doc,
            Doc::parse(source),
            "{source:?} must be parsed as Markdown"
        );
    }
}

#[test]
fn a_document_whose_only_markup_is_a_footnote_is_still_markdown() {
    // Regression: the first detector scanned for a hand-written list of signals, so a
    // document whose only markup was `[^a]` was paged as flat text and its footnotes
    // came out as raw source. Detection now asks the parser, so every construct it
    // recognises counts — including the ones nobody thought to list.
    let source = "Ref one[^a] and ref two[^long].\n\n[^a]: First.\n\n[^long]: Second.\n";
    assert_eq!(Doc::parse_auto(source), Doc::parse(source));
    let out = lines(source, 44);
    assert_eq!(out[0], "Ref one[1] and ref two[2].");
    assert!(out.iter().any(|row| row == "[1] First."), "{out:?}");
    assert!(out.iter().any(|row| row == "[2] Second."), "{out:?}");
}

#[test]
fn markup_nobody_would_type_by_accident_still_counts() {
    for source in [
        "a[^n]\n\n[^n]: note\n",
        "an *emphasis* alone\n",
        "an `inline code span` alone\n",
        "an ![image](p.png) alone\n",
        "a [real link](http://example.com) alone\n",
        "- [ ] a task\n",
    ] {
        assert_eq!(
            Doc::parse_auto(source),
            Doc::parse(source),
            "{source:?} must be parsed as Markdown"
        );
    }
}

#[test]
fn markup_plain_text_produces_by_accident_does_not_count() {
    for source in [
        // An indented commit body, not a code block.
        "Header line\n\n    an indented body paragraph\n",
        // An e-mail address in a header, not a link.
        "Author: Tobias Oetiker <tobi@oetiker.ch>\n",
        // A separator in --help output, not a thematic break.
        "Usage\n\n---\n\nOptions\n",
        // Two trailing spaces are lint, not a hard break.
        "one line  \ntwo lines\n",
    ] {
        assert_eq!(
            Doc::parse_auto(source),
            Doc::parse_plain(source),
            "{source:?} must be paged as plain text"
        );
    }
}

#[test]
fn a_stream_with_no_markup_takes_the_plain_path() {
    let doc = Doc::parse_auto(GIT_LOG);
    assert_eq!(doc, Doc::parse_plain(GIT_LOG));
    assert!(doc.headings().is_empty());
}

#[test]
fn plain_parsing_keeps_the_source_and_is_deterministic() {
    assert_eq!(Doc::parse_plain(GIT_LOG).source(), GIT_LOG);
    assert_eq!(Doc::parse_plain(GIT_LOG), Doc::parse_plain(GIT_LOG));
    // Degenerate inputs must not panic or invent content.
    assert!(Doc::parse_plain("").root().children.is_empty());
    assert!(Doc::parse_plain("\n\n\n").root().children.is_empty());
}

#[test]
fn plain_text_renders_at_every_width_without_breaking_the_canvas_contract() {
    let doc = Doc::parse_auto(GIT_LOG);
    let theme = Theme::default_dark();
    let options = RenderOptions::new(false, false);
    for width in 1..=120u16 {
        let canvas = render_document(&doc, width, &theme, &options);
        assert_eq!(canvas.width(), width);
        canvas
            .check_invariants()
            .unwrap_or_else(|problem| panic!("width {width}: {problem}"));
    }
}
