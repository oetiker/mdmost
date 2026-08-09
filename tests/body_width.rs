//! A cap on the width of the prose body, with a dispensation for what cannot reflow.
//!
//! Design spec §3.2. Prose past a hundred-odd columns is hard to read — the eye loses
//! the start of the next line — so the body is capped and centred on a wide terminal.
//! Tables and diagrams are exempt outright; everything else escalates to the full width
//! the moment the cap would cut it short. These tests pin the rule, both halves of it,
//! and pin that the horizontal-scroll machinery still sees what it expects.

use mdmost::canvas::Canvas;
use mdmost::doc::Doc;
use mdmost::render::{RenderOptions, render_document};
use mdmost::theme::Theme;
use mdmost::tui::wide::{render_scrollable, scroll_reach};

/// No Nerd Font glyphs, no line numbers: the layout is the subject here.
const PLAIN: RenderOptions = RenderOptions::new(false, false);

/// A terminal far wider than any sensible measure.
const WIDE: u16 = 200;

/// The cap every test here uses.
const CAP: u16 = 100;

/// The document rendered the way the pager renders it.
fn paged(markdown: &str, width: u16, cap: Option<u16>) -> Canvas {
    render_scrollable(
        &Doc::parse(markdown),
        width,
        cap,
        &Theme::default_dark(),
        &PLAIN,
    )
}

/// The column the first non-blank cell of `row` sits in, if the row draws anything.
fn indent_of(canvas: &Canvas, row: usize) -> Option<usize> {
    canvas
        .row(row)?
        .iter()
        .position(|cell| !cell.is_blank() && !cell.is_continuation())
}

/// The column one past the last thing `row` draws.
fn extent_of(canvas: &Canvas, row: usize) -> usize {
    canvas.row(row).map_or(0, |cells| {
        cells
            .iter()
            .rposition(|cell| !cell.is_blank() && !cell.is_continuation())
            .map_or(0, |index| index + 1)
    })
}

/// The indent of the first row whose text contains `needle`.
fn indent_of_row_with(canvas: &Canvas, needle: &str) -> usize {
    let row = (0..canvas.height())
        .find(|row| canvas.row_text(*row).contains(needle))
        .unwrap_or_else(|| panic!("no row contains {needle:?}:\n{}", canvas.plain_text()));
    indent_of(canvas, row).unwrap_or(0)
}

/// A paragraph long enough to fill any width it is given.
const PROSE: &str = "The measure of a line of text is how far the eye has to travel \
before it comes back, and a line that runs the whole width of a very wide terminal \
makes that return journey hard enough that readers lose their place between one line \
and the next, which is why every serious reader of long-form text caps it somewhere.";

#[test]
fn prose_is_centred_when_the_terminal_is_wider_than_the_cap() {
    let canvas = paged(PROSE, WIDE, Some(CAP));
    let margin = 1; // render::DOCUMENT_MARGIN
    let body = WIDE - 2 * margin;
    let pad = (body - CAP) / 2;
    let expected = usize::from(margin + pad);
    let indent = indent_of(&canvas, 0).expect("the paragraph draws something");
    assert_eq!(
        indent,
        expected,
        "prose should be centred at column {expected}, not {indent}:\n{}",
        canvas.row_text(0)
    );
    // Centred means indented on *both* sides: nothing may reach the right margin.
    for row in 0..canvas.height() {
        assert!(
            extent_of(&canvas, row) <= expected + usize::from(CAP),
            "row {row} runs past the capped body:\n{}",
            canvas.row_text(row)
        );
    }
}

#[test]
fn a_cap_wider_than_the_terminal_changes_nothing() {
    let capped = paged(PROSE, 80, Some(CAP));
    let uncapped = paged(PROSE, 80, None);
    assert_eq!(
        capped.plain_text(),
        uncapped.plain_text(),
        "a cap of {CAP} must not touch an 80-column terminal"
    );
    // And an uncapped render is still exactly what the piped renderer produces.
    let piped = render_document(&Doc::parse(PROSE), 80, &Theme::default_dark(), &PLAIN);
    assert_eq!(uncapped.plain_text(), piped.plain_text());
}

/// A table whose natural width is far past any body cap.
const WIDE_TABLE: &str = "| Setting | What it does | Where it comes from | What happens without it |\n|---|---|---|---|\n| `body_width` | Caps the prose body | The config file or `--body-width` | Prose runs the whole terminal |\n| `toc_width` | Sizes the contents pane | The config file only | The pane is thirty columns wide |\n";

#[test]
fn a_wide_table_takes_the_full_terminal_width() {
    let markdown = format!("{PROSE}\n\n{WIDE_TABLE}");
    let canvas = paged(&markdown, WIDE, Some(CAP));
    let uncapped = paged(&markdown, WIDE, None);
    let row_with = |canvas: &Canvas| {
        (0..canvas.height())
            .find(|row| canvas.row_text(*row).contains("body_width"))
            .expect("the table is drawn")
    };
    // The width the table itself occupies, wherever it has been placed.
    let drawn = |canvas: &Canvas| {
        let row = row_with(canvas);
        extent_of(canvas, row) - indent_of(canvas, row).unwrap_or(0)
    };
    let width = drawn(&canvas);
    assert!(
        width > usize::from(CAP),
        "the table was squeezed into the prose measure: {width} columns\n{}",
        canvas.row_text(row_with(&canvas))
    );
    assert_eq!(
        width,
        drawn(&uncapped),
        "a table must be laid out exactly as it would be with no cap at all"
    );
    // The prose beside it is still capped and still centred.
    assert_eq!(
        indent_of(&canvas, 0),
        Some(usize::from(1 + (198 - CAP) / 2))
    );
}

#[test]
fn a_table_wider_than_the_cap_shares_the_prose_centre_line() {
    let markdown = format!("{PROSE}\n\n{WIDE_TABLE}");
    let canvas = paged(&markdown, WIDE, Some(CAP));
    let row = (0..canvas.height())
        .find(|row| canvas.row_text(*row).contains("body_width"))
        .expect("the table is drawn");
    let left = indent_of(&canvas, row).expect("the table draws something");
    let right = usize::from(WIDE) - extent_of(&canvas, row);
    assert!(
        left.abs_diff(right) <= 1,
        "the table is not centred: {left} columns left, {right} right\n{}",
        canvas.row_text(row)
    );
    // And it is wider than the prose above it, not narrower.
    assert!(left < indent_of(&canvas, 0).expect("prose"));
}

#[test]
fn a_narrow_table_sits_with_the_prose_rather_than_at_the_far_left() {
    let markdown = format!("{PROSE}\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
    let canvas = paged(&markdown, WIDE, Some(CAP));
    let prose = indent_of(&canvas, 0).expect("the paragraph draws something");
    let table = indent_of_row_with(&canvas, "│ a");
    assert_eq!(
        table,
        prose,
        "a table that fits the measure should be aligned with it:\n{}",
        canvas.plain_text()
    );
}

/// A fenced block with one line far longer than any body cap.
fn long_code(len: usize) -> String {
    format!("{PROSE}\n\n```text\n{}\n```\n", "x".repeat(len))
}

#[test]
fn a_code_line_the_cap_would_cut_takes_the_full_terminal_width() {
    let canvas = paged(&long_code(150), WIDE, Some(CAP));
    let row = (0..canvas.height())
        .find(|row| canvas.row_text(*row).contains("xxxxx"))
        .expect("the code is drawn");
    let extent = extent_of(&canvas, row);
    assert!(
        extent > usize::from(CAP) + 10,
        "a 150-column line was cut to the prose measure: extent {extent}"
    );
}

#[test]
fn a_short_code_block_stays_with_the_prose() {
    let canvas = paged(&long_code(20), WIDE, Some(CAP));
    let prose = indent_of(&canvas, 0).expect("the paragraph draws something");
    let code = indent_of_row_with(&canvas, "xxxxx");
    assert!(
        code >= prose,
        "a short code block should not break out to the left margin: {code} < {prose}\n{}",
        canvas.plain_text()
    );
    for row in 0..canvas.height() {
        assert!(
            extent_of(&canvas, row) <= prose + usize::from(CAP),
            "row {row} broke out of the measure for no reason:\n{}",
            canvas.row_text(row)
        );
    }
}

#[test]
fn content_past_the_terminal_still_scrolls_while_the_prose_stays_put() {
    // A code line wider than the whole terminal: it is laid out wide and reached with
    // the horizontal scroll keys, exactly as it was before the cap existed.
    let canvas = paged(&long_code(400), WIDE, Some(CAP));
    assert!(
        canvas.width() > WIDE,
        "the surplus that horizontal scrolling reaches was not kept: width {}",
        canvas.width()
    );
    let reach = scroll_reach(&canvas, WIDE);
    assert_eq!(reach.len(), canvas.height());
    let prose_reach = reach[0];
    assert!(
        prose_reach <= WIDE,
        "the prose was dragged into the scrollable run: reach {prose_reach}"
    );
    assert!(
        reach.iter().any(|row| *row > WIDE),
        "nothing reaches past the viewport, so nothing scrolls"
    );
}

#[test]
fn a_wide_block_inside_a_quote_still_reaches_the_full_terminal() {
    let markdown = format!(
        "> {}\n>\n> | a | b |\n> |---|---|\n> | {} | {} |\n",
        PROSE,
        "long cell content ".repeat(4),
        "more of it ".repeat(4)
    );
    let canvas = paged(&markdown, WIDE, Some(CAP));
    let row = (0..canvas.height())
        .find(|row| canvas.row_text(*row).contains("long cell content"))
        .expect("the quoted table is drawn");
    assert!(
        extent_of(&canvas, row) > usize::from(CAP),
        "a quoted table was confined to the prose measure:\n{}",
        canvas.row_text(row)
    );
}
