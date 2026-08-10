//! Unit tests for the canvas contract.
//!
//! Every test asserts [`Canvas::check_invariants`] after the operation under test:
//! the width guarantee is the whole point of this layer.

use super::*;
use crate::text::{Align, Line, Span, display_width};
use crate::theme::{Attributes, Color, Style, Theme};

const ZWJ: &str = "\u{1f469}\u{200d}\u{1f4bb}";
const FLAG: &str = "\u{1f1e8}\u{1f1ed}";
const COMBINING: &str = "e\u{0301}";

fn base() -> Style {
    Theme::default_dark().base()
}

#[test]
fn check_invariants_catches_cells_that_re_join_when_assembled() {
    // The exact row the cluster splitter used to produce for "😀\u{200d}😀ᩗ" at width 8:
    // the ZWJ sequence cut in half. Every cell is honest on its own — 2 + 2 + 1 plus
    // three blanks is 8 columns — but concatenating them re-forms one cluster that
    // draws 3 instead of 5, so the row renders two columns short. Only the assembled
    // check sees it, which is why this went unnoticed while every per-cell assertion
    // passed.
    let style = base();
    let mut canvas = Canvas::empty(8);
    canvas.rows.push(vec![
        Cell::blank(style),
        Cell::new("\u{1f600}\u{200d}", style),
        Cell::continuation(style),
        Cell::new("\u{1f600}", style),
        Cell::continuation(style),
        Cell::new("\u{1A57}", style),
        Cell::blank(style),
        Cell::blank(style),
    ]);

    let columns: usize = canvas.rows[0]
        .iter()
        .map(|c| usize::from(c.width()))
        .sum::<usize>();
    assert_eq!(columns, 8, "the cells must claim a full row");
    assert_eq!(
        display_width(&canvas.row_text(0)),
        6,
        "but the assembled row must really draw short"
    );

    let problem = canvas
        .check_invariants()
        .expect_err("a row that draws short must be rejected");
    assert!(
        problem.contains("re-joining"),
        "unexpected complaint: {problem}"
    );
}

fn ok(canvas: &Canvas) {
    if let Err(problem) = canvas.check_invariants() {
        panic!(
            "canvas invariant violated: {problem}\n{}",
            canvas.plain_text()
        );
    }
}

#[test]
fn new_canvas_is_blank_and_exact() {
    let canvas = Canvas::new(7, 3, base());
    ok(&canvas);
    assert_eq!(canvas.width(), 7);
    assert_eq!(canvas.height(), 3);
    assert_eq!(canvas.row_text(0), "       ");
    assert!(canvas.rows().iter().all(|r| r.len() == 7));
}

#[test]
fn empty_canvas_has_no_rows() {
    let canvas = Canvas::empty(5);
    ok(&canvas);
    assert!(canvas.is_empty());
    assert_eq!(canvas.row(0), None);
    assert_eq!(canvas.row_text(0), "");
    assert_eq!(canvas.plain_text(), "");
}

#[test]
fn write_str_fills_cells_and_pads() {
    let mut canvas = Canvas::new(6, 1, base());
    let written = canvas.write_str(0, 1, "abc", base());
    ok(&canvas);
    assert_eq!(written, 3);
    assert_eq!(canvas.row_text(0), " abc  ");
}

#[test]
fn double_width_uses_a_lead_and_a_continuation_cell() {
    let mut canvas = Canvas::new(6, 1, base());
    canvas.write_str(0, 0, "日本", base());
    ok(&canvas);
    let row = canvas.row(0).expect("row exists");
    assert_eq!(row.len(), 6, "four columns of text plus two blanks");
    assert_eq!(row[0].text(), "日");
    assert_eq!(row[0].width(), 2);
    assert!(row[1].is_continuation());
    assert_eq!(row[2].text(), "本");
    assert!(row[3].is_continuation());
    assert_eq!(canvas.row_text(0), "日本  ");
}

#[test]
fn zwj_and_flag_sequences_occupy_one_cell() {
    let mut canvas = Canvas::new(6, 1, base());
    canvas.write_str(0, 0, ZWJ, base());
    canvas.write_str(0, 2, FLAG, base());
    ok(&canvas);
    let row = canvas.row(0).expect("row exists");
    assert_eq!(row[0].text(), ZWJ);
    assert_eq!(row[0].width(), 2);
    assert_eq!(row[2].text(), FLAG);
    assert_eq!(row[2].width(), 2);
    assert_eq!(canvas.row_text(0), format!("{ZWJ}{FLAG}  "));
}

#[test]
fn combining_marks_merge_into_the_preceding_cell() {
    let mut canvas = Canvas::new(4, 1, base());
    canvas.write_str(0, 0, COMBINING, base());
    ok(&canvas);
    let row = canvas.row(0).expect("row exists");
    assert_eq!(row[0].text(), COMBINING);
    assert_eq!(row[0].width(), 1);
    assert_eq!(display_width(&canvas.row_text(0)), 4);
}

#[test]
fn a_leading_combining_mark_is_dropped_rather_than_stealing_a_column() {
    let mut canvas = Canvas::new(3, 1, base());
    canvas.write_str(0, 0, "\u{0301}ab", base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "ab ");
}

#[test]
fn writing_clips_at_the_right_edge() {
    let mut canvas = Canvas::new(4, 1, base());
    let written = canvas.write_str(0, 0, "abcdef", base());
    ok(&canvas);
    assert_eq!(written, 4);
    assert_eq!(canvas.row_text(0), "abcd");
}

#[test]
fn a_wide_cluster_straddling_the_edge_is_dropped_whole() {
    let mut canvas = Canvas::new(2, 1, base());
    let written = canvas.write_str(0, 0, "a日", base());
    ok(&canvas);
    assert_eq!(
        written, 1,
        "the wide cluster does not fit into the last column"
    );
    assert_eq!(canvas.row_text(0), "a ");

    // With one more column it fits exactly.
    let mut canvas = Canvas::new(3, 1, base());
    assert_eq!(canvas.write_str(0, 0, "a日", base()), 3);
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "a日");
}

#[test]
fn writing_out_of_range_is_a_no_op() {
    let mut canvas = Canvas::new(3, 1, base());
    assert_eq!(canvas.write_str(9, 0, "x", base()), 0);
    assert_eq!(canvas.write_str(0, 5, "x", base()), 0);
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "   ");
}

#[test]
fn overwriting_the_left_half_of_a_wide_cell_blanks_the_orphan() {
    let mut canvas = Canvas::new(4, 1, base());
    canvas.write_str(0, 0, "日本", base());
    canvas.write_str(0, 0, "x", base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "x 本");
}

#[test]
fn overwriting_the_right_half_of_a_wide_cell_blanks_the_orphan() {
    let mut canvas = Canvas::new(4, 1, base());
    canvas.write_str(0, 0, "日本", base());
    canvas.write_str(0, 1, "x", base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), " x本");
}

#[test]
fn a_wide_write_over_two_wide_cells_repairs_both_sides() {
    let mut canvas = Canvas::new(6, 1, base());
    canvas.write_str(0, 0, "日本語", base());
    canvas.write_str(0, 1, "＃", base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), " ＃ 語");
}

#[test]
fn write_line_overlays_span_styles_on_the_base() {
    let mut canvas = Canvas::new(8, 1, base());
    let accent = Style::new().fg(Color::hex(0xff0000)).bold();
    let line = Line::new(vec![Span::raw("ab"), Span::new("cd", accent)]);
    canvas.write_line(0, 0, &line, base());
    ok(&canvas);
    let row = canvas.row(0).expect("row exists");
    assert_eq!(row[0].style().fg, base().fg);
    assert_eq!(row[2].style().fg, accent.fg);
    assert_eq!(row[2].style().bg, base().bg, "base supplies the background");
    assert!(row[2].style().attrs.contains(Attributes::BOLD));
}

#[test]
fn write_field_pads_and_aligns_within_the_field() {
    let mut canvas = Canvas::new(10, 1, base());
    canvas.write_field(0, 2, 6, "hi", Align::Center, base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "    hi    ");
}

#[test]
fn fill_and_lines_stay_exact_with_wide_glyphs() {
    let mut canvas = Canvas::new(5, 3, base());
    canvas.hline(0, 0, 5, "─", base());
    canvas.fill(1, 0, 5, "日", base());
    canvas.vline(0, 4, 3, "│", base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "────│");
    // "日" is two columns, so five columns hold two of them plus one space — which the
    // vertical line then overwrites.
    assert_eq!(canvas.row_text(1), "日日│");
    assert_eq!(canvas.row_text(2), "    │");
}

#[test]
fn push_helpers_align_within_the_canvas() {
    let mut canvas = Canvas::empty(9);
    canvas.push_text("hi", Align::Right, base());
    canvas.push_line(&Line::styled("mid", base()), Align::Center, base());
    canvas.push_blank_rows(2, base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "       hi");
    assert_eq!(canvas.row_text(1), "   mid   ");
    assert_eq!(canvas.height(), 4);
}

#[test]
fn push_text_ellipsized_truncates_with_a_marker() {
    let mut canvas = Canvas::empty(6);
    canvas.push_text_ellipsized("abcdefghij", Align::Left, base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "abcde…");
}

#[test]
fn from_lines_renders_each_line_on_its_own_row() {
    let lines = vec![Line::styled("one", base()), Line::styled("two", base())];
    let canvas = Canvas::from_lines(5, &lines, base());
    ok(&canvas);
    assert_eq!(canvas.plain_text(), "one  \ntwo  ");
}

#[test]
fn from_text_makes_a_single_row() {
    let canvas = Canvas::from_text(4, "hi", base());
    ok(&canvas);
    assert_eq!(canvas.height(), 1);
    assert_eq!(canvas.row_text(0), "hi  ");
}

#[test]
fn pad_to_width_grows_and_refuses_to_shrink() {
    let mut canvas = Canvas::from_text(3, "abc", base());
    canvas.pad_to_width(6, base()).expect("growing is allowed");
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "abc   ");
    let err = canvas
        .pad_to_width(2, base())
        .expect_err("shrinking must fail");
    assert!(matches!(err, crate::error::CanvasError::Narrowing { .. }));
}

#[test]
fn truncate_width_repairs_a_straddling_wide_cell() {
    let mut canvas = Canvas::from_text(6, "日本語", base());
    canvas.truncate_width(3, base());
    ok(&canvas);
    assert_eq!(canvas.width(), 3);
    assert_eq!(canvas.row_text(0), "日 ");
}

#[test]
fn resize_width_goes_both_ways() {
    let mut canvas = Canvas::from_text(4, "abcd", base());
    canvas.resize_width(2, base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "ab");
    canvas.resize_width(5, base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "ab   ");
}

#[test]
fn blit_places_a_canvas_at_an_offset() {
    let mut dst = Canvas::new(8, 3, base());
    let src = Canvas::from_text(3, "xyz", base());
    dst.blit(1, 2, &src, base());
    ok(&dst);
    assert_eq!(dst.row_text(0), "        ");
    assert_eq!(dst.row_text(1), "  xyz   ");
}

#[test]
fn blit_grows_the_destination_downwards() {
    let mut dst = Canvas::new(4, 1, base());
    let src = Canvas::new(4, 3, base());
    dst.blit(2, 0, &src, base());
    ok(&dst);
    assert_eq!(dst.height(), 5);
}

#[test]
fn blit_clips_wide_cells_at_the_destination_edge() {
    let mut dst = Canvas::new(3, 1, base());
    let src = Canvas::from_text(4, "日本", base());
    dst.blit(0, 0, &src, base());
    ok(&dst);
    assert_eq!(dst.row_text(0), "日 ");
}

#[test]
fn blit_at_an_odd_offset_into_wide_content_repairs_the_row() {
    let mut dst = Canvas::from_text(6, "日本語", base());
    let src = Canvas::from_text(2, "ab", base());
    dst.blit(0, 1, &src, base());
    ok(&dst);
    assert_eq!(dst.row_text(0), " ab 語");
}

#[test]
fn blit_translates_anchors_and_spans() {
    let mut src = Canvas::new(4, 2, base());
    src.add_anchor(Anchor {
        id: "intro".into(),
        level: 1,
        row: 1,
    });
    src.add_span(SearchSpan {
        source_start: 5,
        source_end: 9,
        row: 1,
        col: 1,
        cols: 3,
    });
    let mut dst = Canvas::new(8, 1, base());
    dst.blit(3, 2, &src, base());
    ok(&dst);
    assert_eq!(dst.anchors()[0].row, 4);
    assert_eq!(dst.spans()[0].row, 4);
    assert_eq!(dst.spans()[0].col, 3);
    assert_eq!(dst.spans()[0].source_start, 5);
}

#[test]
fn a_pin_travels_with_whole_rows_and_no_further() {
    // The rule that makes the pin channel per block: a pin is a claim about a row's
    // *first* columns, so it survives the operations that move whole rows and dies in
    // the one that drops a canvas into the middle of somebody else's row. Without that
    // asymmetry a numbered fence inside a table cell would pin the columns of the cells
    // to its left — the very corruption the channel replaced.
    let mut src = Canvas::new(4, 2, base());
    src.add_pin(1, 3);
    assert_eq!(src.pinned_prefix(), vec![0, 3], "one entry per row");

    let indented = src.indent(2, 1, base());
    ok(&indented);
    assert_eq!(
        indented.pinned_prefix(),
        vec![0, 5],
        "an indent moves the whole row, so the chrome starts two columns later"
    );

    let mut stacked = Canvas::new(4, 1, base());
    stacked.append(&src, base());
    ok(&stacked);
    assert_eq!(
        stacked.pinned_prefix(),
        vec![0, 0, 3],
        "stacking moves a pin down and leaves its columns alone"
    );

    let mut dst = Canvas::new(9, 2, base());
    dst.blit(0, 5, &src, base());
    ok(&dst);
    assert_eq!(
        dst.pinned_prefix(),
        vec![0, 0],
        "a canvas placed at column five owns none of the row's first five columns"
    );

    assert_eq!(
        stacked.slice_rows(2, 1).pinned_prefix(),
        vec![3],
        "a viewport slice keeps the pins of the rows it kept"
    );
}

#[test]
fn append_widens_the_narrower_canvas() {
    let mut top = Canvas::from_text(3, "ab", base());
    let bottom = Canvas::from_text(6, "wide!!", base());
    top.append(&bottom, base());
    ok(&top);
    assert_eq!(top.width(), 6);
    assert_eq!(top.plain_text(), "ab    \nwide!!");
}

#[test]
fn vconcat_stacks_in_order_and_honours_a_minimum_width() {
    let parts = vec![
        Canvas::from_text(2, "a", base()),
        Canvas::from_text(4, "bcd", base()),
    ];
    let stacked = Canvas::vconcat(&parts, 6, base());
    ok(&stacked);
    assert_eq!(stacked.width(), 6);
    assert_eq!(stacked.plain_text(), "a     \nbcd   ");
}

#[test]
fn hconcat_places_canvases_side_by_side_with_a_gap() {
    let parts = vec![
        Canvas::from_text(2, "ab", base()),
        Canvas::from_lines(
            3,
            &[Line::styled("cd", base()), Line::styled("ef", base())],
            base(),
        ),
    ];
    let joined = Canvas::hconcat(&parts, 1, base());
    ok(&joined);
    assert_eq!(joined.width(), 6);
    assert_eq!(
        joined.height(),
        2,
        "shorter parts are top-aligned and padded"
    );
    assert_eq!(joined.plain_text(), "ab cd \n   ef ");
}

#[test]
fn hconcat_of_nothing_is_empty() {
    let joined = Canvas::hconcat(&[], 2, base());
    ok(&joined);
    assert_eq!(joined.width(), 0);
    assert_eq!(joined.height(), 0);
}

#[test]
fn indent_insets_the_content() {
    let canvas = Canvas::from_text(3, "abc", base()).indent(2, 1, base());
    ok(&canvas);
    assert_eq!(canvas.width(), 6);
    assert_eq!(canvas.row_text(0), "  abc ");
}

#[test]
fn slice_rows_keeps_the_width_and_retargets_metadata() {
    let mut canvas = Canvas::new(4, 6, base());
    canvas.add_anchor(Anchor {
        id: "a".into(),
        level: 2,
        row: 3,
    });
    canvas.add_anchor(Anchor {
        id: "b".into(),
        level: 2,
        row: 5,
    });
    let slice = canvas.slice_rows(2, 2);
    ok(&slice);
    assert_eq!(slice.width(), 4);
    assert_eq!(slice.height(), 2);
    assert_eq!(slice.anchors().len(), 1);
    assert_eq!(slice.anchors()[0].row, 1);
}

#[test]
fn slice_rows_past_the_end_is_clamped() {
    let canvas = Canvas::new(4, 3, base());
    let slice = canvas.slice_rows(2, 10);
    ok(&slice);
    assert_eq!(slice.height(), 1);
    assert!(canvas.slice_rows(9, 3).is_empty());
}

#[test]
fn framed_wraps_the_content_in_a_border() {
    let inner = Canvas::from_text(3, "abc", base());
    let framed = inner.framed(BorderSet::ROUNDED, base(), None, base());
    ok(&framed);
    assert_eq!(framed.width(), 5);
    assert_eq!(framed.height(), 3);
    assert_eq!(framed.plain_text(), "╭───╮\n│abc│\n╰───╯");
}

#[test]
fn framed_writes_a_title_into_the_top_edge() {
    let inner = Canvas::new(10, 1, base());
    let title = Line::styled("rust", base());
    let framed = inner.framed(BorderSet::ROUNDED, base(), Some(&title), base());
    ok(&framed);
    assert_eq!(framed.row_text(0), "╭ rust ────╮");
}

#[test]
fn framed_clips_an_overlong_title_and_keeps_the_corners() {
    let inner = Canvas::new(6, 1, base());
    let title = Line::styled("a very long language name", base());
    let framed = inner.framed(BorderSet::ROUNDED, base(), Some(&title), base());
    ok(&framed);
    let top = framed.row_text(0);
    assert!(top.starts_with('╭'), "got {top:?}");
    assert!(top.ends_with('╮'), "got {top:?}");
    assert_eq!(display_width(&top), 8);

    // The same holds when the clip lands inside a double-width cluster.
    let title = Line::styled("日本語の見出し", base());
    let framed = inner.framed(BorderSet::ROUNDED, base(), Some(&title), base());
    ok(&framed);
    let top = framed.row_text(0);
    assert!(top.starts_with('╭') && top.ends_with('╮'), "got {top:?}");
}

#[test]
fn framed_keeps_wide_content_exact() {
    let inner = Canvas::from_text(4, "日本", base());
    let framed = inner.framed(BorderSet::PLAIN, base(), None, base());
    ok(&framed);
    assert_eq!(framed.plain_text(), "┌────┐\n│日本│\n└────┘");
}

#[test]
fn framed_translates_anchors() {
    let mut inner = Canvas::new(4, 2, base());
    inner.add_anchor(Anchor {
        id: "x".into(),
        level: 3,
        row: 1,
    });
    let framed = inner.framed(BorderSet::ROUNDED, base(), None, base());
    ok(&framed);
    assert_eq!(framed.anchors()[0].row, 2);
}

#[test]
fn push_rule_fills_the_whole_row() {
    let mut canvas = Canvas::empty(5);
    canvas.push_rule("─", base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "─────");
}

#[test]
fn style_helpers_set_and_overlay() {
    let mut canvas = Canvas::from_text(5, "abcde", base());
    let hit = Style::new().bg(Color::hex(0xffff00));
    canvas.patch_style(0, 1, 2, hit);
    ok(&canvas);
    let row = canvas.row(0).expect("row exists");
    assert_eq!(row[0].style().bg, base().bg);
    assert_eq!(row[1].style().bg, hit.bg);
    assert_eq!(row[2].style().bg, hit.bg);
    assert_eq!(row[3].style().bg, base().bg);

    canvas.set_style(0, 0, 2, hit);
    assert_eq!(canvas.row(0).expect("row exists")[0].style(), hit);

    canvas.patch_style_all(Style::new().bold());
    assert!(
        canvas
            .row(0)
            .expect("row exists")
            .iter()
            .all(|c| c.style().attrs.contains(Attributes::BOLD))
    );
}

#[test]
fn style_range_out_of_bounds_is_ignored() {
    let mut canvas = Canvas::from_text(3, "abc", base());
    canvas.patch_style(9, 0, 3, Style::new().bold());
    canvas.patch_style(0, 2, 99, Style::new().bold());
    ok(&canvas);
}

#[test]
fn check_invariants_detects_a_broken_row() {
    let mut canvas = Canvas::new(3, 1, base());
    // Reach past the safe API to prove the checker actually checks.
    canvas.rows[0].pop();
    assert!(canvas.check_invariants().is_err());
}

#[test]
fn cell_accessors_behave() {
    let cell = Cell::new("日", base());
    assert_eq!(cell.width(), 2);
    assert!(!cell.is_continuation());
    assert!(!cell.is_blank());
    assert!(Cell::blank(base()).is_blank());
    assert!(Cell::continuation(base()).is_continuation());
    assert_eq!(Cell::default().text(), " ");
}

#[test]
fn every_row_is_exactly_width_columns_for_adversarial_text() {
    let corpus = [
        "plain ascii",
        "日本語のテキスト",
        "emoji 👩‍💻 and flags 🇨🇭🇯🇵",
        "combining e\u{0301}a\u{0300} marks",
        "mixed 混合 text with 👍 emoji",
        "\u{0301}leading mark",
        "",
    ];
    for width in 1..=12u16 {
        for text in corpus {
            let mut canvas = Canvas::new(width, 1, base());
            canvas.write_str(0, 0, text, base());
            ok(&canvas);
            assert_eq!(display_width(&canvas.row_text(0)), usize::from(width));
        }
    }
}

/// A wide base character followed by a spacing combining mark (`Mc`): one grapheme
/// cluster that genuinely draws three columns.
///
/// This exact string is the minimal input that `render_property`'s
/// `arbitrary_text_renders_cleanly` shrank to when it caught the canvas claiming two
/// columns for it and drawing three.
const WIDE_PLUS_SPACING_MARK: &str = "\u{17000}\u{1A57}";

#[test]
fn a_cluster_too_wide_for_one_cell_is_spread_over_two() {
    let mut canvas = Canvas::new(6, 1, base());
    let written = canvas.write_str(0, 0, WIDE_PLUS_SPACING_MARK, base());

    assert_eq!(written, 3, "the cluster advances three columns");
    ok(&canvas);
    assert_eq!(canvas.row_text(0).trim_end(), WIDE_PLUS_SPACING_MARK);
    assert_eq!(display_width(&canvas.row_text(0)), 6);
}

/// U+17D8 KHMER SIGN BEYYAL: three columns in one scalar, so there is nothing inside it
/// to split. `render_property` shrank to this at width 5.
const INDIVISIBLE_TOO_WIDE: &str = "\u{17D8}";

#[test]
fn a_cluster_that_cannot_be_split_is_replaced_by_a_marker_of_its_width() {
    let mut canvas = Canvas::new(6, 1, base());
    let written = canvas.write_str(0, 0, INDIVISIBLE_TOO_WIDE, base());

    assert_eq!(
        written, 3,
        "the replacement occupies the columns the sign drew"
    );
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "\u{FFFD}     ");
}

#[test]
fn text_after_an_unplaceable_cluster_keeps_its_column() {
    // The point of padding the marker to the cluster's width: everything the layout
    // measured with `display_width` still lands where it was measured to land.
    let mut canvas = Canvas::new(8, 1, base());
    canvas.write_str(0, 0, "a\u{17D8}b", base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), "a\u{FFFD}  b   ");
}

#[test]
fn no_cell_ever_claims_a_width_its_text_does_not_draw() {
    // The invariant that makes this class of bug impossible to reintroduce.
    let samples = [
        WIDE_PLUS_SPACING_MARK,
        INDIVISIBLE_TOO_WIDE,
        "\u{17D8}\u{093B}",
        "a\u{17D8}日",
        ZWJ,
        FLAG,
        COMBINING,
        "\u{2764}\u{fe0f}",
        "日本語",
        "plain",
        "\u{17000}\u{1A57}\u{17000}\u{1A57}",
    ];
    for text in samples {
        for width in 1..12u16 {
            let mut canvas = Canvas::new(width, 1, base());
            canvas.write_str(0, 0, text, base());
            assert!(
                canvas.check_invariants().is_ok(),
                "{text:?} at width {width}: {:?}",
                canvas.check_invariants()
            );
            for cell in canvas.row(0).unwrap_or_default() {
                assert_eq!(
                    display_width(cell.text()),
                    usize::from(cell.width()),
                    "cell {:?} lies about its width",
                    cell.text()
                );
            }
        }
    }
}

#[test]
fn an_over_wide_cluster_is_dropped_rather_than_half_drawn() {
    // Only two columns left: the three-column cluster cannot be shown whole, and the
    // canvas must not draw part of it and mis-count the row.
    let mut canvas = Canvas::new(2, 1, base());
    canvas.write_str(0, 0, WIDE_PLUS_SPACING_MARK, base());
    ok(&canvas);
    assert_eq!(display_width(&canvas.row_text(0)), 2);
}

#[test]
fn clip_with_marker_stamps_only_rows_that_lost_something() {
    let mut canvas = Canvas::new(10, 0, base());
    canvas.push_text("aaaaaaaaaa", Align::Left, base());
    canvas.push_text("bb", Align::Left, base());

    canvas.clip_with_marker(5, "›", base());

    assert_eq!(canvas.width(), 5);
    assert_eq!(canvas.row_text(0), "aaaa›");
    assert_eq!(canvas.row_text(1), "bb   ", "a short row keeps no marker");
    ok(&canvas);
}

#[test]
fn clip_with_marker_never_underflows_at_a_zero_width() {
    // The bug this op exists to make structurally impossible: the code renderer's
    // hand-rolled version computed `width - 1` without guarding `width == 0`.
    let mut canvas = Canvas::new(6, 0, base());
    canvas.push_text("content", Align::Left, base());
    canvas.clip_with_marker(0, "›", base());
    assert_eq!(canvas.width(), 0);
    ok(&canvas);
}

#[test]
fn clip_with_marker_leaves_a_canvas_that_already_fits() {
    let mut canvas = Canvas::new(4, 0, base());
    canvas.push_text("ab", Align::Left, base());
    canvas.clip_with_marker(9, "›", base());
    assert_eq!(canvas.width(), 4);
    assert_eq!(canvas.row_text(0), "ab  ");
}

#[test]
fn clip_with_marker_handles_a_wide_marker_and_wide_content() {
    let mut canvas = Canvas::new(8, 0, base());
    canvas.push_text("日本語話", Align::Left, base());
    canvas.clip_with_marker(5, "…", base());
    assert_eq!(canvas.width(), 5);
    ok(&canvas);
    assert_eq!(display_width(&canvas.row_text(0)), 5);

    // A marker too wide for what is left clips without stamping rather than
    // overflowing the row.
    let mut narrow = Canvas::new(6, 0, base());
    narrow.push_text("abcdef", Align::Left, base());
    narrow.clip_with_marker(1, "日", base());
    ok(&narrow);
    assert_eq!(display_width(&narrow.row_text(0)), 1);
}

#[test]
fn rect_stamps_a_hollow_box_in_place() {
    let mut canvas = Canvas::new(8, 4, base());
    canvas.rect(0, 1, 3, 6, BorderSet::PLAIN, base());
    ok(&canvas);
    assert_eq!(canvas.row_text(0), " ┌────┐ ");
    assert_eq!(canvas.row_text(1), " │    │ ");
    assert_eq!(canvas.row_text(2), " └────┘ ");
    assert_eq!(canvas.row_text(3), "        ", "nothing outside the box");
}

#[test]
fn rect_degrades_instead_of_panicking_on_degenerate_sizes() {
    let mut canvas = Canvas::new(6, 3, base());
    canvas.rect(0, 0, 0, 4, BorderSet::PLAIN, base());
    assert_eq!(canvas.row_text(0), "      ", "zero height draws nothing");

    canvas.rect(0, 0, 3, 0, BorderSet::PLAIN, base());
    assert_eq!(canvas.row_text(0), "      ", "zero width draws nothing");

    // One row is a horizontal rule; one column is a vertical one.
    canvas.rect(0, 1, 1, 4, BorderSet::PLAIN, base());
    assert_eq!(canvas.row_text(0), " ──── ");

    let mut tall = Canvas::new(3, 3, base());
    tall.rect(0, 1, 3, 1, BorderSet::PLAIN, base());
    assert_eq!(tall.row_text(0), " │ ");
    assert_eq!(tall.row_text(2), " │ ");
    ok(&tall);
}

#[test]
fn rect_clips_rather_than_growing_the_canvas() {
    // Unlike `framed`, which returns a bigger canvas, `rect` draws onto what is there.
    let mut canvas = Canvas::new(5, 2, base());
    canvas.rect(0, 0, 4, 5, BorderSet::PLAIN, base());
    assert_eq!(canvas.height(), 2, "the canvas does not grow");
    assert_eq!(canvas.row_text(0), "┌───┐");
    assert_eq!(canvas.row_text(1), "│   │", "the bottom edge fell outside");
    ok(&canvas);
}

#[test]
fn grid_border_row_puts_a_junction_over_every_column_break() {
    // Each column is drawn with one cell of padding either side.
    let row = Canvas::grid_border_row(&[2, 3], '├', '┼', '┤', BorderSet::ROUNDED);
    assert_eq!(row, "├────┼─────┤");

    let single = Canvas::grid_border_row(&[1], '╭', '┬', '╮', BorderSet::ROUNDED);
    assert_eq!(single, "╭───╮");

    let none = Canvas::grid_border_row(&[], '╰', '┴', '╯', BorderSet::ROUNDED);
    assert_eq!(none, "╰╯");
}

#[test]
fn grid_border_row_matches_the_widths_it_was_given() {
    let widths = [4usize, 2, 7];
    let row = Canvas::grid_border_row(&widths, '├', '┼', '┤', BorderSet::ROUNDED);
    let expected: usize = widths.iter().map(|w| w + 2).sum::<usize>() + widths.len() + 1;
    assert_eq!(display_width(&row), expected);
}

/// A hotspot at a known place, for the propagation tests.
fn marked(width: u16) -> Canvas {
    let mut canvas = Canvas::new(width, 2, Style::default());
    canvas.add_hotspot(Hotspot {
        row: 0,
        col: 3,
        cols: 6,
        text: "payload".to_string(),
        html: None,
    });
    canvas
}

#[test]
fn append_moves_a_hotspot_down() {
    let mut top = Canvas::new(20, 3, Style::default());
    top.append(&marked(20), Style::default());
    let spot = &top.hotspots()[0];
    assert_eq!((spot.row, spot.col), (3, 3));
    assert_eq!(spot.text, "payload");
}

#[test]
fn indent_moves_a_hotspot_right() {
    let indented = marked(20).indent(2, 1, Style::default());
    let spot = &indented.hotspots()[0];
    assert_eq!((spot.row, spot.col), (0, 5));
}

#[test]
fn blit_drops_a_hotspot() {
    let mut host = Canvas::new(40, 4, Style::default());
    host.blit(1, 5, &marked(20), Style::default());
    assert!(
        host.hotspots().is_empty(),
        "a canvas placed into a row it shares cannot claim a control there"
    );
}
