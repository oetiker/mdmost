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
