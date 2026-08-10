//! Unit tests for the renderers.
//!
//! Every helper here asserts the two invariants that hold for *all* rendered output:
//! the canvas is exactly the requested width, and it satisfies the canvas contract.
//! Individual tests then check what the output actually says.

use super::*;
use crate::canvas::Canvas;
use crate::doc::Doc;
use crate::text::display_width;
use crate::theme::{Attributes, Theme};
use crate::tui::icons::is_private_use;

/// Options the readable assertions below are written against.
///
/// Plain glyphs, because a Nerd Font code point in an `assert_eq!` is unreadable;
/// [`icons_change_the_glyphs_but_never_the_layout`] covers the other setting.
const PLAIN: RenderOptions = RenderOptions::new(false, false);

/// The same, with the lone-`#` title banner asked for.
///
/// The banner is opt-in, so the tests whose subject *is* the banner have to say so;
/// every other test wants the default, where a lone `#` heading is drawn as a heading.
const BANNER: RenderOptions = PLAIN.with_title_banner(true);

/// Options with the copy button asked for; it is off by default like the banner.
const BUTTONS: RenderOptions = PLAIN.with_copy_button(true);

/// Renders `markdown` at `width` with the plain glyph set, checking the invariants.
fn render(markdown: &str, width: u16) -> Canvas {
    render_with(markdown, width, &PLAIN)
}

/// Renders `markdown` at `width` with explicit options, checking the invariants.
fn render_with(markdown: &str, width: u16, options: &RenderOptions) -> Canvas {
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    let canvas = render_flat(&doc, width, &theme, options);
    assert_eq!(canvas.width(), width, "canvas must be exactly {width} wide");
    canvas
        .check_invariants()
        .unwrap_or_else(|problem| panic!("canvas contract violated at width {width}: {problem}"));
    for row in 0..canvas.height() {
        assert_eq!(
            display_width(&canvas.row_text(row)),
            usize::from(width),
            "row {row} is not exactly {width} display columns"
        );
    }
    canvas
}

/// The document body rendered at a budget of exactly `width` columns.
///
/// [`render_document`] insets the body by [`DOCUMENT_MARGIN`] on each side, so a body
/// budget of `width` means a canvas of `width + 2 * DOCUMENT_MARGIN`. Stripping the
/// margins here keeps every layout assertion in this file about the *body*, and keeps
/// the margin itself asserted in exactly one place
/// ([`every_row_keeps_a_margin_on_both_sides`]).
fn rows(markdown: &str, width: u16) -> Vec<String> {
    let canvas = render(markdown, width + 2 * DOCUMENT_MARGIN);
    body_rows(&canvas)
}

/// Renders `markdown` at a body budget of exactly `width`, margins included on top.
fn render_body(markdown: &str, width: u16, options: &RenderOptions) -> Canvas {
    render_with(markdown, width + 2 * DOCUMENT_MARGIN, options)
}

/// The column the body starts at in a canvas wide enough to carry margins.
const BODY_COL: usize = DOCUMENT_MARGIN as usize;

/// The body rows of an already-rendered canvas, margins and trailing padding removed.
fn body_rows(canvas: &Canvas) -> Vec<String> {
    let margin = usize::from(margins(canvas.width()));
    (0..canvas.height())
        .map(|row| {
            let text = canvas.row_text(row);
            crate::text::split_at_width(&text, margin)
                .1
                .trim_end()
                .to_string()
        })
        .collect()
}

/// The rows that contain any text at all.
fn lines(markdown: &str, width: u16) -> Vec<String> {
    rows(markdown, width)
        .into_iter()
        .filter(|row| !row.is_empty())
        .collect()
}

/// The same, with explicit options — for the tests whose subject is an option.
fn lines_with(markdown: &str, width: u16, options: &RenderOptions) -> Vec<String> {
    body_rows(&render_body(markdown, width, options))
        .into_iter()
        .filter(|row| !row.is_empty())
        .collect()
}

/// The style of one cell.
fn style_at(canvas: &Canvas, row: usize, col: usize) -> crate::theme::Style {
    canvas
        .row(row)
        .and_then(|cells| cells.get(col))
        .map(crate::canvas::Cell::style)
        .unwrap_or_default()
}

/// The display column at which `needle` starts in `text`.
fn column_of(text: &str, needle: &str) -> usize {
    let byte = text.find(needle).expect("needle present");
    display_width(&text[..byte])
}

/// The first row whose text contains `needle`.
fn find_row(canvas: &Canvas, needle: &str) -> usize {
    (0..canvas.height())
        .find(|row| canvas.row_text(*row).contains(needle))
        .unwrap_or_else(|| panic!("no row contains {needle:?}\n{}", canvas.plain_text()))
}

// ---------------------------------------------------------------- inline spans

#[test]
fn paragraph_wraps_at_the_width_budget() {
    assert_eq!(
        lines("one two three four five", 10),
        ["one two", "three four", "five"]
    );
}

/// A literal tab in prose is one column of whitespace, never a tab character.
///
/// `display_width` prices a tab at one column, so every measurement in the program —
/// wrapping, table negotiation, `check_invariants` — agreed the row was exact while the
/// terminal jumped to the next tab stop and drew it some six columns wider. That is a
/// breach of the one guarantee the whole canvas contract exists to make, and it is
/// invisible to a test that only measures the canvas.
#[test]
fn a_tab_in_prose_never_reaches_the_canvas() {
    let out = lines("A tab\tbetween words.\n", 40);
    assert_eq!(out, ["A tab between words."]);
}

/// The class, not the instance. `ESC` is the one that matters most: a document carrying
/// it could otherwise write an escape sequence straight through to the reader's screen.
#[test]
fn no_control_character_reaches_the_canvas() {
    let markdown = "bell \u{7} esc \u{1b}[31m nul \u{0} vt \u{b} del \u{7f} c1 \u{9b}\n";
    // `render` already runs `check_invariants`, which rejects a control character in a
    // cell; this asserts the same thing of the text the pager would print.
    let canvas = render(markdown, 60);
    assert!(
        !canvas.plain_text().chars().any(char::is_control),
        "{:?}",
        canvas.plain_text()
    );
}

/// A tab in a table cell is negotiated as one column and must draw as one.
#[test]
fn a_tab_in_a_table_cell_never_reaches_the_canvas() {
    let out = lines("| a\tb |\n|---|\n| c\td |\n", 30);
    assert!(out.iter().any(|row| row.contains("a b")), "{out:?}");
    assert!(!out.concat().contains('\t'), "{out:?}");
}

/// Code blocks keep their own, older tab handling: there a tab is expanded against the
/// real column *before* anything measures the line, so it aligns as the author meant.
#[test]
fn a_tab_in_a_code_block_is_still_expanded_to_its_tab_stop() {
    let out = lines("```\nab\tc\n```\n", 30);
    assert!(out[1].contains("ab  c"), "{out:?}");
}

#[test]
fn soft_breaks_become_spaces_and_hard_breaks_do_not() {
    assert_eq!(lines("one\ntwo", 20), ["one two"]);
    assert_eq!(lines("one  \ntwo", 20), ["one", "two"]);
}

#[test]
fn inline_markup_carries_the_theme_styles() {
    let theme = Theme::default_dark();
    let canvas = render("*em* **st** ~~del~~ `code`", 40);
    let row = canvas.row_text(0);
    assert_eq!(row.trim(), "em st del code");
    let column = |needle: &str| row.find(needle).expect("substring present");
    assert_eq!(style_at(&canvas, 0, column("em")), theme.text.emphasis);
    assert_eq!(style_at(&canvas, 0, column("st")), theme.text.strong);
    assert_eq!(
        style_at(&canvas, 0, column("del")),
        theme.text.strikethrough
    );
    assert_eq!(style_at(&canvas, 0, column("code")), theme.text.code);
}

#[test]
fn nested_emphasis_combines_attributes() {
    let canvas = render("***both***", 20);
    let attrs = style_at(&canvas, 0, BODY_COL).attrs;
    assert!(attrs.contains(Attributes::BOLD) && attrs.contains(Attributes::ITALIC));
}

#[test]
fn a_link_shows_its_target_but_an_autolink_does_not() {
    assert_eq!(lines("[text](http://a.b)", 40), ["text (http://a.b)"]);
    assert_eq!(lines("<http://a.b>", 40), ["http://a.b"]);
}

#[test]
fn footnotes_render_a_marker_and_a_definition() {
    let out = lines("a[^n]\n\n[^n]: body\n", 40);
    assert_eq!(out, ["a[1]", "[1] body"]);
}

#[test]
fn html_never_reaches_the_canvas() {
    let block = lines("<div>secret</div>\n", 40);
    assert_eq!(block, ["⟨html⟩"]);
    let inline = lines("before <b>middle</b> after\n", 40);
    assert_eq!(inline, ["before ⟨html⟩ middle after"]);
    assert!(!inline.concat().contains('<'));
}

// -------------------------------------------------------------------- headings

/// Headings start at the margin — no prefix glyph, removed at the owner's request on
/// 2026-08-09 — and every level but the sixth is underlined.
#[test]
fn headings_are_anchored_and_underlined_by_level() {
    for (level, ruled) in [(1u8, true), (2, true), (3, true), (5, true), (6, false)] {
        let markdown = format!("{} Title\n", "#".repeat(usize::from(level)));
        let canvas = render_with(&markdown, 20, &PLAIN);
        let first = canvas.row_text(0);
        assert_eq!(first.trim_end(), " Title", "level {level}: {first:?}");
        assert_eq!(canvas.anchors().len(), 1);
        assert_eq!(canvas.anchors()[0].row, 0);
        assert_eq!(canvas.anchors()[0].level, level);
        assert_eq!(canvas.anchors()[0].id, "title");
        assert_eq!(canvas.height(), if ruled { 2 } else { 1 });
    }
}

/// The rule under a heading is the *only* thing that says which level it is, now that
/// the prefix glyph has gone, so no two levels may draw the same one.
#[test]
fn heading_levels_use_distinct_rules() {
    let rules: Vec<Option<char>> = (1..=6)
        .map(|level| {
            let markdown = format!("{} T\n", "#".repeat(level));
            let drawn = body_rows(&render_with(&markdown, 12, &PLAIN));
            drawn.get(1).and_then(|rule| rule.chars().next())
        })
        .collect();
    assert_eq!(rules[5], None, "level 6 draws no rule at all");
    let unique: std::collections::HashSet<Option<char>> = rules.iter().copied().collect();
    assert_eq!(unique.len(), 6, "every level needs its own rule");
    assert_eq!(rules[0], Some('━'), "the heaviest rule is level one's");
}

#[test]
fn heading_text_wraps_at_the_margin() {
    assert_eq!(
        lines("# a long heading that wraps\n", 12),
        ["a long", "heading that", "wraps", "━━━━━━━━━━━━"]
    );
}

#[test]
fn anchors_are_recorded_for_every_heading_in_order() {
    // Without the banner: `a_lone_top_level_heading_is_drawn_as_a_banner` asserts that
    // a banner keeps its anchor, and this test is about the ordering of all of them.
    let canvas = render_with("# One\n\ntext\n\n## Two\n\n## Two\n", 30, &PLAIN);
    let ids: Vec<&str> = canvas
        .anchors()
        .iter()
        .map(|anchor| anchor.id.as_str())
        .collect();
    assert_eq!(ids, ["one", "two", "two-1"]);
    let rows: Vec<usize> = canvas.anchors().iter().map(|anchor| anchor.row).collect();
    assert!(rows.windows(2).all(|pair| pair[0] < pair[1]));
    for anchor in canvas.anchors() {
        assert!(canvas.row_text(anchor.row).contains(if anchor.id == "one" {
            "One"
        } else {
            "Two"
        }));
    }
}

// --------------------------------------------------------------- title banner

/// The document's own title, and only that, is set in the `FIGlet` font.
#[test]
fn a_lone_top_level_heading_is_drawn_as_a_banner() {
    let canvas = render_with("# Title\n\nbody\n", 40, &BANNER);
    let drawn = body_rows(&canvas);
    assert_eq!(
        drawn[..5],
        [
            " _____ _ _   _",
            "|_   _(_) |_| |___",
            "  | | | |  _| / -_)",
            "  |_| |_|\\__|_\\___|",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
        ],
        "{}",
        canvas.plain_text()
    );
    // Still a heading in every way the rest of the program cares about: the table of
    // contents jumps to it, and searching for it finds it.
    assert_eq!(canvas.anchors().len(), 1);
    assert_eq!(canvas.anchors()[0].id, "title");
    assert_eq!(canvas.anchors()[0].level, 1);
    assert_eq!(canvas.anchors()[0].row, 0);
    let mut search = crate::search::Search::new(
        "# Title\n\nbody\n",
        "Title",
        crate::search::SearchMode::Literal,
    )
    .expect("a literal query");
    search.locate("# Title\n\nbody\n", canvas.spans());
    let hit = &search.hits()[0];
    assert_eq!(hit.row(), Some(0), "the hit must be found on the banner");
    assert!(
        hit.segments.len() >= 4,
        "every row of the art belongs to the match: {:?}",
        hit.segments
    );
}

/// One `#` per chapter is a book, not a title page.
#[test]
fn a_document_with_several_top_level_headings_gets_no_banner() {
    let drawn = lines("# One\n\ntext\n\n# Two\n\nmore\n", 40);
    assert_eq!(drawn[0], "One");
    assert!(!drawn.concat().contains('_'), "{drawn:?}");
}

/// A `#` that arrives after the prose is a section heading, not the document's title.
#[test]
fn a_late_top_level_heading_gets_no_banner() {
    let drawn = lines("intro paragraph\n\n# Title\n\nbody\n", 40);
    assert_eq!(drawn[0], "intro paragraph");
    assert_eq!(drawn[1], "Title");
}

/// Too wide to draw is answered with the ordinary heading, never with truncated art.
///
/// Since a long title is wrapped between words, "too wide" now means a single word that
/// will not fit — there is nothing left to break.
#[test]
fn a_title_banner_gives_way_to_a_plain_heading_when_it_will_not_fit() {
    for width in [20u16, 30, 40] {
        let drawn = lines("# Unbreakableantidisestablishmentarianism\n\nbody\n", width);
        assert!(
            drawn[0].starts_with("Unbreakable"),
            "at width {width} the heading should be plain text: {drawn:?}"
        );
    }
    // And a title the font cannot draw at all: no banner at any width.
    let drawn = lines("# Übersicht\n\nbody\n", 100);
    assert_eq!(drawn[0], "Übersicht");
}

/// The banner is a render option like any other, so it can be turned off.
#[test]
fn the_title_banner_can_be_switched_off() {
    let drawn = body_rows(&render_with("# Title\n\nbody\n", 40, &PLAIN));
    assert_eq!(drawn[0], "Title");
}

// ------------------------------------------------------------ section numbers

/// A document with no title: the `#`s are the top level and number themselves.
#[test]
fn a_deeply_nested_document_numbers_its_sections() {
    let markdown = "# One\n\n## First\n\n### Deep\n\n## Second\n\n# Two\n";
    let drawn = lines(markdown, 40);
    for wanted in ["1 One", "1.1 First", "1.1.1 Deep", "1.2 Second", "2 Two"] {
        assert!(
            drawn.contains(&wanted.to_string()),
            "{wanted:?} in {drawn:?}"
        );
    }
}

/// A lone `#` is the title: it is unnumbered and the `##`s are the top level.
///
/// Written with the banner declined so the title is a row of text to assert on; the
/// banner'd shape is [`a_banner_title_is_not_numbered`], and the numbers below it are
/// the same either way.
#[test]
fn a_titled_document_numbers_from_the_second_level() {
    let markdown = "# Title\n\n## First\n\n### Deep\n\n#### Deeper\n\n## Second\n";
    let drawn = body_rows(&render_with(markdown, 40, &PLAIN))
        .into_iter()
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>();
    assert!(drawn.contains(&"Title".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"1 First".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"1.1 Deep".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"1.1.1 Deeper".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"2 Second".to_string()), "{drawn:?}");
}

/// The `FIGlet` title is unnumbered, and the sections under it number as they always do.
#[test]
fn a_banner_title_is_not_numbered() {
    let markdown = "# Title\n\n## First\n\n### Deep\n\n#### Deeper\n";
    let drawn = lines_with(markdown, 40, &BANNER);
    // The banner is art, drawn from the title's own text and nothing else: no row of
    // it carries a number, and the numbering below it is what it would be anyway.
    let art = &drawn[..4];
    assert!(art.iter().all(|row| !row.contains('1')), "{art:?}");
    assert!(drawn.contains(&"1 First".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"1.1 Deep".to_string()), "{drawn:?}");
}

/// A flat document needs no orientation aid and gets none.
#[test]
fn a_flat_document_is_not_numbered() {
    // Two `#`s so this is not a titled document: two levels in use, and no numbers.
    let markdown = "# One\n\n## A\n\n## B\n\n# Two\n\n## C\n";
    let drawn = lines(markdown, 40);
    assert!(drawn.contains(&"One".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"A".to_string()), "{drawn:?}");
    assert!(!drawn.iter().any(|row| row.starts_with("1")), "{drawn:?}");
    // And a titled document with only two levels under the title is flat as well: the
    // title is not a section, so it cannot make the document deep.
    let titled = body_rows(&render_with("# T\n\n## A\n\n### B\n", 40, &PLAIN));
    assert!(titled.contains(&"A".to_string()), "{titled:?}");
}

/// A skipped level leaves a `0` where the ancestor the author did not write would be.
#[test]
fn a_skipped_level_numbers_the_missing_ancestor_zero() {
    let markdown = "# One\n\n### Deep\n\n## Back\n\n#### Deeper\n\n# Two\n";
    let drawn = lines(markdown, 40);
    assert!(drawn.contains(&"1 One".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"1.0.1 Deep".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"1.1 Back".to_string()), "{drawn:?}");
    assert!(drawn.contains(&"1.1.0.1 Deeper".to_string()), "{drawn:?}");
}

/// The number is drawn in its own style, not the heading's.
#[test]
fn the_section_number_is_quieter_than_its_heading() {
    let theme = Theme::default_dark();
    // No `#` at all, so the `##`s are the top level and row 0 is `1 A`.
    let canvas = render_body("## A\n\n### B\n\n#### C\n", 40, &PLAIN);
    let number = style_at(&canvas, 0, BODY_COL);
    let text = style_at(&canvas, 0, BODY_COL + 2);
    assert_eq!(
        number, theme.heading_number,
        "the number wears its own slot"
    );
    assert_eq!(text, theme.heading(2), "the heading text is unchanged");
    assert_ne!(number, text);
}

/// Numbering is a setting, and off means no numbers anywhere.
#[test]
fn section_numbers_can_be_switched_off() {
    let markdown = "## A\n\n### B\n\n#### C\n";
    let plain = PLAIN.with_section_numbers(false);
    let drawn = body_rows(&render_with(markdown, 40, &plain));
    assert!(drawn.contains(&"A".to_string()), "{drawn:?}");
    // The same document with the setting on is the control.
    let on = body_rows(&render_with(markdown, 40, &PLAIN));
    assert!(on.contains(&"1 A".to_string()), "{on:?}");
}

/// A numbered heading wraps under its own text, not under its number.
#[test]
fn a_numbered_heading_wraps_under_its_text() {
    let markdown = "## A heading long enough to wrap twice over\n\n### B\n\n#### C\n";
    let drawn = lines(markdown, 24);
    assert_eq!(drawn[0], "1 A heading long enough");
    assert_eq!(drawn[1], "  to wrap twice over");
}

/// A number that would leave no room for words is dropped, and the words stay.
#[test]
fn a_heading_too_narrow_for_its_number_keeps_its_words() {
    // Ten deep, so the number is `1.1.1.1.1.1.1.1.1 ` — eighteen columns.
    let mut markdown = String::new();
    for level in 1..=6 {
        markdown.push_str(&format!("{} Section\n\n", "#".repeat(level)));
    }
    markdown.push_str("# Second\n");
    // Twenty columns still pays for the twelve-column number and leaves eight for the
    // word, which is the floor.
    let roomy = lines(&markdown, 20);
    assert!(
        roomy.contains(&"1.1.1.1.1.1 Section".to_string()),
        "{roomy:?}"
    );
    // Sixteen does not, so the deepest heading drops its number and keeps its word.
    // The shallower ones, whose numbers still fit, keep theirs.
    let tight = lines(&markdown, 16);
    assert!(tight.contains(&"Section".to_string()), "{tight:?}");
    assert!(tight.contains(&"1.1.1 Section".to_string()), "{tight:?}");
    // And nothing is ever cut: every heading still says its whole word.
    for row in tight.iter().filter(|row| row.ends_with("Section")) {
        assert!(row.ends_with(" Section") || row == "Section", "{row:?}");
    }
}

// ----------------------------------------------------------------------- lists

#[test]
fn nested_lists_indent_and_change_their_bullet() {
    assert_eq!(
        lines("- one\n  - two\n    - three\n", 30),
        ["* one", "  > two", "    + three"]
    );
}

#[test]
fn ordered_lists_respect_the_start_ordinal_and_align_the_numbers() {
    assert_eq!(
        lines("9. nine\n10. ten\n", 30),
        [" 9. nine", "10. ten"],
        "ordinals are right-aligned so the text lines up"
    );
}

#[test]
fn list_item_content_wraps_under_a_hanging_indent() {
    assert_eq!(
        lines("- alpha beta gamma delta\n", 12),
        ["* alpha beta", "  gamma", "  delta"]
    );
}

/// The box is followed by *two* spaces, not one.
///
/// Reported by the owner on 2026-08-09: "the checkbox is a large double char affair
/// which matches nicely between checked and unchecked, but it hugs the text following
/// it… so I guess two spaces would be in order". Both boxes, both glyph sets.
#[test]
fn task_items_get_a_checkbox() {
    assert_eq!(
        lines("- [x] done\n- [ ] todo\n", 20),
        ["[x]  done", "[ ]  todo"]
    );
}

/// A plain item in an *unordered* task list starts its text in the same column.
///
/// The widened gap belongs to the list, not to the item: every item pads to the same
/// marker field, so a bullet among checkboxes gets the same two columns of air and
/// the text edge stays straight.
#[test]
fn a_plain_item_in_an_unordered_task_list_keeps_the_text_column() {
    assert_eq!(
        lines("- [x] done\n- plain\n", 20),
        ["[x]  done", "*    plain"]
    );
}

/// Continuation lines in an unordered task list hang under the text, past both spaces.
#[test]
fn an_unordered_task_item_wraps_under_its_text() {
    assert_eq!(
        lines("- [ ] alpha beta gamma\n", 16),
        ["[ ]  alpha beta", "     gamma"]
    );
}

/// The task box is the same three ASCII columns whichever glyph set is in force.
///
/// The boxes went through a Nerd Font pictograph pair and out the other side; what
/// pins the current answer is that the *marker field is identical in both sets*, so
/// `--no-icons` cannot move a task list sideways. The plain item in the same list pads
/// to the same field, so the text edge stays straight whatever an item's state.
#[test]
fn the_task_box_lays_out_identically_in_both_glyph_sets() {
    let document = "- [x] done\n- [ ] todo\n- plain\n";
    let nerd = body_rows(&render_body(document, 20, &RenderOptions::new(true, false)));
    let plain = body_rows(&render_body(document, 20, &PLAIN));
    assert_eq!(nerd, plain, "icons must not move a task list");
    let text: Vec<&String> = plain.iter().filter(|row| !row.is_empty()).collect();
    assert_eq!(text, ["[x]  done", "[ ]  todo", "*    plain"]);
}

/// An ordered task list has *two* things to say and has to say both.
///
/// The number is the item's identity — it is how the item is referred to elsewhere —
/// and the box is its state. The box used to be drawn instead of the number, which
/// renumbered the author's list to nothing and left a two-column gap where the ordinal
/// had been.
#[test]
fn an_ordered_task_list_keeps_its_numbers() {
    assert_eq!(
        lines("1. [x] done\n2. [ ] todo\n", 20),
        ["1. [x]  done", "2. [ ]  todo"]
    );
}

/// Ten items wide the ordinals right-align, and the boxes stay in one column with them.
#[test]
fn an_ordered_task_list_aligns_its_boxes_under_each_other() {
    assert_eq!(
        lines("9. [ ] item\n10. [ ] item\n", 20),
        [" 9. [ ]  item", "10. [ ]  item"]
    );
}

/// A plain item in a list that has task items keeps its ordinal in the same column.
#[test]
fn a_plain_item_in_a_task_list_keeps_the_marker_column() {
    assert_eq!(
        lines("1. [x] done\n2. plain\n", 20),
        ["1. [x]  done", "2.      plain"]
    );
}

/// Continuation lines hang under the text, clear of both ordinal and box.
#[test]
fn an_ordered_task_item_wraps_under_its_text() {
    assert_eq!(
        lines("1. [ ] alpha beta gamma\n", 16),
        ["1. [ ]  alpha", "        beta", "        gamma"]
    );
}

#[test]
fn tight_lists_are_dense_and_loose_lists_are_spaced() {
    assert_eq!(rows("- one\n- two\n", 20).len(), 2);
    assert_eq!(rows("- one\n\n- two\n", 20), ["* one", "", "* two"]);
}

/// The document the wrapping-spaces-the-list rule is asserted against.
///
/// Three items, one of which is long enough to wrap at a narrow budget and short
/// enough to fit at a wide one.
const THREE_BULLETS: &str = "- one\n- alpha beta gamma delta epsilon\n- three\n";

#[test]
fn a_list_stays_tight_while_every_item_fits_on_one_line() {
    assert_eq!(
        rows(THREE_BULLETS, 40),
        ["* one", "* alpha beta gamma delta epsilon", "* three"]
    );
}

#[test]
fn a_list_is_spaced_throughout_as_soon_as_one_item_wraps() {
    // The rule is per-list, not per-item: the gap appears between *every* pair, not
    // only around the item that wrapped, or the spacing would be ragged.
    assert_eq!(
        rows(THREE_BULLETS, 20),
        [
            "* one",
            "",
            "* alpha beta gamma",
            "  delta epsilon",
            "",
            "* three"
        ]
    );
}

#[test]
fn a_list_already_loose_by_commonmark_does_not_gain_a_second_blank_row() {
    let loose = "- one\n\n- alpha beta gamma delta epsilon\n\n- three\n";
    assert_eq!(
        rows(loose, 20),
        [
            "* one",
            "",
            "* alpha beta gamma",
            "  delta epsilon",
            "",
            "* three"
        ]
    );
}

#[test]
fn ordered_and_task_lists_follow_the_same_rule() {
    assert_eq!(
        rows("1. one\n2. alpha beta gamma delta\n", 16),
        ["1. one", "", "2. alpha beta", "   gamma delta"]
    );
    assert_eq!(
        rows("- [ ] one\n- [x] alpha beta gamma delta\n", 16),
        ["[ ]  one", "", "[x]  alpha beta", "     gamma delta"]
    );
}

#[test]
fn each_list_level_decides_its_own_spacing() {
    // The outer item carries a sublist, so it is more than one line tall and the outer
    // level spaces out; the inner items each fit on one line, so the sublist stays
    // tight. Spacing appears exactly where the crowding is.
    assert_eq!(
        rows("- outer one\n  - in a\n  - in b\n- outer two\n", 30),
        ["* outer one", "  > in a", "  > in b", "", "* outer two"]
    );
}

/// A spaced list is set off from the item that introduces it, not only from its own
/// siblings.
///
/// Regression: the blank rows were placed *between* items only, so a chain of nested
/// lists drew every parent welded to its child while every sibling below breathed —
/// the deepest descent read as one solid block and everything after it was spaced.
#[test]
fn a_spaced_sublist_is_set_off_from_the_item_that_introduces_it() {
    // The sublist wraps at this width, so it is spaced; the seam between "outer" and
    // the sublist is a seam of that spaced list and gets the same blank row.
    assert_eq!(
        rows("- outer\n  - alpha beta gamma delta\n  - short\n", 16),
        [
            "* outer",
            "",
            "  > alpha beta",
            "    gamma delta",
            "",
            "  > short",
        ]
    );
    // A chain of nested lists breathes at every level it descends through, rather
    // than packing the descent solid and spacing only what comes after it.
    assert_eq!(
        rows("- one\n  - two\n    - alpha beta gamma\n", 16),
        [
            "* one",
            "",
            "  > two",
            "",
            "    + alpha beta",
            "      gamma",
        ]
    );
}

#[test]
fn the_spacing_decision_is_re_taken_at_every_width() {
    // Width-dependent by construction: the same document is tight where it is roomy
    // and spaced where the items would look cramped. Rendering is a pure function of
    // (AST, width, theme, options), so a resize re-renders and re-decides.
    assert_eq!(rows(THREE_BULLETS, 40).len(), 3);
    assert_eq!(rows(THREE_BULLETS, 20).len(), 6);
    assert_eq!(rows(THREE_BULLETS, 40).len(), 3);
}

#[test]
fn a_list_inside_a_quote_inside_a_list_still_composes() {
    assert_eq!(
        lines("- outer\n  > quoted\n  > - inner\n", 30),
        ["* outer", "  ▌ quoted", "  ▌", "  ▌ > inner"]
    );
}

// ---------------------------------------------------------------------- quotes

#[test]
fn nested_quotes_draw_one_bar_per_level() {
    assert_eq!(lines("> one\n>\n> > two\n", 30), ["▌ one", "▌", "▌ ▌ two"]);
}

#[test]
fn quote_bars_change_hue_with_depth() {
    let canvas = render("> one\n>\n> > two\n", 30);
    let row = find_row(&canvas, "two");
    assert_ne!(
        style_at(&canvas, row, 0).fg,
        style_at(&canvas, row, 2).fg,
        "nested bars must be distinguishable"
    );
}

#[test]
fn a_thematic_break_is_inset_and_marked_so_it_is_not_a_heading_rule() {
    // Inset on both sides with a centred lozenge: a heading rule is full-bleed and
    // plain, so the two can no longer be confused.
    assert_eq!(lines("---\n", 20), ["   ──────◈───────"]);
    let rule = &lines("## Two\n", 20)[1];
    assert_eq!(rule, "────────────────────");
    // Too narrow to inset: still marked, never a bare full-bleed run.
    assert_eq!(lines("---\n", 3), ["─◈─"]);
    assert_eq!(lines("---\n", 2), ["──"]);
}

// ------------------------------------------------------------------ code blocks

#[test]
fn a_fenced_block_is_framed_and_titled_with_its_language() {
    let out = lines("```rust\nfn a() {}\n```\n", 20);
    assert_eq!(
        out,
        [
            "╭ rust ────────────╮",
            "│ fn a() {}        │",
            "╰──────────────────╯"
        ]
    );
}

#[test]
fn an_untagged_block_is_still_framed() {
    let out = lines("```\nplain\n```\n", 12);
    assert_eq!(out, ["╭──────────╮", "│ plain    │", "╰──────────╯"]);
}

#[test]
fn an_indented_block_is_framed_without_a_title() {
    let out = lines("    indented\n", 14);
    assert_eq!(out[0], "╭────────────╮");
    assert!(out[1].contains("indented"));
}

#[test]
fn long_code_lines_are_clipped_not_wrapped() {
    let out = lines("```\nabcdefghijklmnop\n```\n", 10);
    assert_eq!(out, ["╭────────╮", "│ abcde› │", "╰────────╯"]);
}

#[test]
fn a_mermaid_fence_degrades_to_a_captioned_code_block() {
    // Deliberately not a diagram any Mermaid implementation could accept, so this
    // stays a test of the degradation path once the real renderer is wired in.
    let out = lines("```mermaid\nnot a diagram at all\n```\n", 60);
    assert!(out[0].starts_with("╭ mermaid"));
    assert!(out.iter().any(|row| row.contains("not a diagram at all")));
    // The reason lives in the frame's bottom edge, mirroring the language label on the
    // top edge, rather than as a stray log line under the box.
    let last = out.last().unwrap_or_else(|| panic!("no rows in {out:?}"));
    assert!(
        last.starts_with("╰ not a diagram type — mdmost draws ") && last.ends_with('╯'),
        "{last:?}"
    );
    // The old caption said "unsupported" twice and quoted the reader's own typo back
    // at them as if it were a diagram type.
    assert!(!out.concat().contains("unsupported"), "{out:?}");
    // …and it must not quote the reader's first word back at them as a diagram type.
    assert!(!last.contains("`not`"), "{last:?}");
}

#[test]
fn an_unimplemented_mermaid_family_says_so_by_name() {
    let out = lines("```mermaid\nflowchart TD\n  A --> B\n```\n", 60);
    let last = out.last().unwrap_or_else(|| panic!("no rows in {out:?}"));
    // Either the family draws (in which case there is no fallback frame at all) or the
    // caption names it and promises nothing it cannot deliver.
    if last.starts_with('╰') && last.len() > 3 {
        assert!(
            last.contains("flowchart") && last.contains("not drawn yet"),
            "{last:?}"
        );
    }
}

// ----------------------------------------------------------------------- images

#[test]
fn an_image_becomes_a_framed_placeholder_with_alt_text_and_target() {
    let out = lines("![the alt](pic.png)\n", 20);
    assert_eq!(
        out,
        [
            "╭ image ───────────╮",
            "│ the alt          │",
            "│ pic.png          │",
            "╰──────────────────╯"
        ]
    );
}

/// An image inside a sentence stays inside it.
///
/// It used to become a framed box, which is a block, so the one paragraph the author
/// wrote came out as three: the words before, a full-width box, the words after. The
/// box is for an image that is a paragraph of its own; here the reader gets the alt
/// text in the brackets that mean "something the terminal cannot show was here".
#[test]
fn an_inline_image_stays_in_its_paragraph() {
    assert_eq!(
        lines("before ![a](p.png) after\n", 24),
        ["before ⟨a⟩ after"]
    );
}

/// The box survives exactly where it belongs: an image with nothing else around it.
#[test]
fn an_image_alone_in_its_paragraph_still_gets_its_box() {
    let out = lines("before\n\n![a](p.png)\n\nafter\n", 20);
    assert_eq!(out.first().map(String::as_str), Some("before"));
    assert_eq!(out.last().map(String::as_str), Some("after"));
    assert!(out.iter().any(|row| row.contains("p.png")), "{out:?}");
}

#[test]
fn an_inline_image_with_no_alt_text_names_itself() {
    assert_eq!(lines("see ![](p.png) here\n", 24), ["see ⟨image⟩ here"]);
}

#[test]
fn a_nested_image_degrades_to_its_alt_text() {
    assert_eq!(lines("[![a](p.png)](t.md)\n", 30), ["⟨a⟩ (t.md)"]);
}

// ----------------------------------------------------------------------- tables

#[test]
fn a_table_fills_the_width_and_draws_rounded_borders() {
    let out = lines("| a | b |\n|---|---|\n| 1 | 2 |\n", 21);
    assert_eq!(
        out,
        [
            "╭───┬───╮",
            "│ a │ b │",
            "├───┼───┤",
            "│ 1 │ 2 │",
            "╰───┴───╯"
        ],
        "a table stops at its natural width instead of filling the terminal"
    );
}

#[test]
fn per_column_alignment_is_honoured() {
    // The headers are wider than the body cells, so each column has slack for the
    // alignment to show in.
    let markdown = "| left | centre | right |\n|:--|:-:|--:|\n| x | x | x |\n";
    let out = lines(markdown, 40);
    let body = &out[3];
    let cells: Vec<&str> = body.trim_matches('│').split('│').collect();
    assert!(cells[0].starts_with(" x"), "left: {:?}", cells[0]);
    assert!(
        cells[1].starts_with("  ") && cells[1].trim_end().len() < cells[1].len(),
        "centre: {:?}",
        cells[1]
    );
    assert!(cells[2].ends_with("x "), "right: {:?}", cells[2]);
}

#[test]
fn cells_recurse_into_the_block_renderer() {
    let markdown = "| md |\n|----|\n| *em* and `code` |\n";
    let canvas = render(markdown, 30);
    let row = find_row(&canvas, "em and code");
    let theme = Theme::default_dark();
    let col = column_of(&canvas.row_text(row), "em");
    assert_eq!(
        style_at(&canvas, row, col).attrs,
        theme.table.cell.patch(theme.text.emphasis).attrs
    );
}

/// Builds a table whose single body cell holds the blocks of `content`.
///
/// Pipe syntax cannot express a table inside a cell, but the AST can, and the
/// renderer recurses either way — so the recursion is tested on the tree directly.
fn table_with_cell(content: &str) -> crate::doc::Node {
    use crate::doc::NodeKind;
    let outer = Doc::parse("| h |\n|---|\n| x |\n");
    let inner = Doc::parse(content);
    let mut table = outer.root().children[0].clone();
    let row = table
        .children
        .iter_mut()
        .find(|node| matches!(node.kind, NodeKind::TableRow { header: false }))
        .expect("a body row");
    row.children[0].children = inner.root().children.clone();
    table
}

#[test]
fn a_list_inside_a_cell_keeps_its_markers() {
    let table = table_with_cell("- one\n- two\n");
    let canvas = render_block(&table, 30, &Theme::default_dark(), &PLAIN);
    canvas.check_invariants().expect("contract holds");
    let text = canvas.plain_text();
    assert!(text.contains("* one"), "{text}");
    assert!(text.contains("* two"), "{text}");
}

#[test]
fn a_nested_table_inside_a_cell_is_rendered_as_a_table() {
    let table = table_with_cell("| in |\n|----|\n| y |\n");
    let canvas = render_block(&table, 40, &Theme::default_dark(), &PLAIN);
    canvas.check_invariants().expect("contract holds");
    assert_eq!(canvas.width(), 40);
    let text = canvas.plain_text();
    // The inner table draws its own frame inside the outer one.
    assert!(text.contains("╭─"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.matches('╭').count() == 1 && line.starts_with('│')),
        "the inner frame sits inside an outer row:\n{text}"
    );
    assert!(text.contains("in"), "{text}");
}

#[test]
fn a_cell_too_narrow_for_a_nested_table_still_renders() {
    let table = table_with_cell("| in |\n|----|\n| y |\n");
    for width in 1..=12u16 {
        let canvas = render_block(&table, width, &Theme::default_dark(), &PLAIN);
        assert_eq!(canvas.width(), width);
        canvas.check_invariants().expect("contract holds");
    }
}

#[test]
fn a_clipped_table_closes_its_rules_and_marks_only_its_content() {
    // `docs/qa/visual-review-3.md` §11: the chevron was stamped on the rule rows too, so
    // a clipped table showed a `╭` with no `╮` and read as a rendering fault rather than
    // as something to scroll. A rule ends in its own corner or tee; the chevron belongs
    // on the content rows, which are what is actually cut off.
    let markdown = "| aaaaaaaaaa | bbbbbbbbbb |\n|---|---|\n| cccccccccc | dddddddddd |\n";
    let rows = body_rows(&render_body(markdown, 14, &PLAIN));
    let last = |row: usize| rows[row].chars().last();
    assert_eq!(last(0), Some('╮'), "top rule: {:?}", rows[0]);
    assert_eq!(last(2), Some('┤'), "header rule: {:?}", rows[2]);
    assert_eq!(last(4), Some('╯'), "bottom rule: {:?}", rows[4]);
    for row in [1, 3] {
        assert_eq!(last(row), Some('›'), "content row: {:?}", rows[row]);
    }
}

#[test]
fn a_clipped_code_block_of_box_art_still_carries_the_marker() {
    // The pager decides whether to re-render a block wider by hunting for the overflow
    // marker (`render::document::ClipTest`). A fence whose *content* is box art — this
    // project's own documentation is full of it — must therefore keep its chevrons: a
    // clip that closed those lines into corners instead would leave the block unmarked
    // and switch its horizontal scrolling off in silence.
    let art = "╭─────────────────────────────╮";
    let markdown = format!("```text\n{art}\n{art}\n```\n");
    let canvas = render_body(&markdown, 16, &PLAIN);
    let rows = body_rows(&canvas);
    for row in [1, 2] {
        assert!(
            rows[row].contains('›'),
            "the fence's own content is still marked, not closed: {:?}",
            rows[row]
        );
    }
}

#[test]
fn a_table_narrower_than_its_minimums_is_clipped_with_a_marker() {
    let markdown = "| aaaaaaaaaa | bbbbbbbbbb |\n|---|---|\n| cccccccccc | dddddddddd |\n";
    let canvas = render_body(markdown, 12, &PLAIN);
    let out = body_rows(&canvas);
    let row = out
        .iter()
        .find(|row| row.contains("aaa"))
        .unwrap_or_else(|| panic!("no row of {out:?} carries the header"));
    assert!(
        row.ends_with('›'),
        "clipped rows carry the overflow marker: {row:?}"
    );
}

#[test]
fn re_rendering_a_clipped_table_wider_reveals_the_columns_it_lost() {
    // This is the contract `render::render_document` relies on for horizontal
    // scrolling: it re-renders an over-wide *block* at a larger budget, and the table
    // renderer must reveal the columns it had clipped when given one. Widening is the
    // viewport's job precisely because it applies to every block, not just tables —
    // the table renderer offers no unclipped entry point of its own, deliberately.
    let markdown = "| aaaaaaaaaa | bbbbbbbbbb |\n|---|---|\n| cccccccccc | dddddddddd |\n";
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    let table = &doc.root().children[0];

    let narrow = render_block(table, 12, &theme, &PLAIN);
    assert_eq!(narrow.width(), 12);
    assert!(narrow.plain_text().contains(code::OVERFLOW_MARKER));
    assert!(!narrow.plain_text().contains("bbbbbbbbbb"));

    let wide = render_block(table, 40, &theme, &PLAIN);
    assert_eq!(wide.width(), 40);
    assert!(wide.plain_text().contains("bbbbbbbbbb"));
    assert!(!wide.plain_text().contains(code::OVERFLOW_MARKER));
    wide.check_invariants().expect("contract holds");
}

#[test]
fn row_height_is_the_tallest_cell_and_shorter_cells_are_top_aligned() {
    let markdown = "| a | b |\n|---|---|\n| one two three four | x |\n";
    let canvas = render(markdown, 24);
    let row = find_row(&canvas, "one");
    assert!(
        canvas.row_text(row).contains('x'),
        "short cell sits at the top"
    );
    assert!(
        !canvas.row_text(row + 1).contains('x'),
        "and is padded below"
    );
}

#[test]
fn degenerate_tables_do_not_panic() {
    for markdown in [
        "| |\n|-|\n| |\n",
        "| a |\n|---|\n",
        "| a | b |\n|---|---|\n| only |\n",
        "|a|b|c|\n|-|-|-|\n|1|2|3|4|\n",
    ] {
        for width in [1u16, 2, 3, 4, 7, 40] {
            let canvas = render(markdown, width);
            assert_eq!(canvas.width(), width);
        }
    }
}

// ------------------------------------------------------------- text edge cases

#[test]
fn wide_and_zero_width_clusters_survive_every_width() {
    let markdown = "日本語のテキスト 👩‍💻 🇨🇭 👍🏽 café e\u{0301}́ combining\n";
    for width in 1..=40u16 {
        let canvas = render(markdown, width);
        assert!(canvas.height() > 0 || width == 0);
    }
}

#[test]
fn a_double_width_cluster_is_dropped_rather_than_split() {
    let canvas = render("日本\n", 1);
    // One column cannot show a two-column cluster; the row stays exactly one column.
    assert_eq!(canvas.width(), 1);
}

#[test]
fn an_unbreakable_word_is_split_on_cluster_boundaries() {
    assert_eq!(lines("abcdefgh", 3), ["abc", "def", "gh"]);
}

#[test]
fn every_width_from_one_upwards_renders_without_panicking() {
    let markdown = include_str!("../../tests/corpus/adversarial.md");
    for width in 1..=24u16 {
        render(markdown, width);
    }
}

// ------------------------------------------------------------- render options

/// Turning icons on changes what the markers look like, never where the document sits.
///
/// This briefly had an exception — a task list was a column wider with icons on, when
/// the box was a Nerd Font pictograph drawn two cells wide while measuring one — and
/// the corpus had to be rendered here with its task items filtered out. `[ ]`/`[x]`
/// removed the discrepancy, so the test is back on the whole corpus, task items
/// included, and the rule is an absolute again.
#[test]
fn icons_change_the_glyphs_but_never_the_layout() {
    let markdown = include_str!("../../tests/corpus/adversarial.md");
    let nerd = RenderOptions::new(true, false);
    for width in [17u16, 40, 80] {
        let plain = render_with(markdown, width, &PLAIN);
        let fancy = render_with(markdown, width, &nerd);
        assert_eq!(
            plain.height(),
            fancy.height(),
            "turning icons on must not change how many rows are used at {width}"
        );
        for row in 0..plain.height() {
            assert_eq!(
                display_width(&fancy.row_text(row)),
                usize::from(width),
                "row {row} at width {width}"
            );
        }
        assert_eq!(
            plain.anchors(),
            fancy.anchors(),
            "anchors must land on the same rows at {width}"
        );
        assert_eq!(plain.spans(), fancy.spans(), "spans must agree at {width}");
    }
    // The glyphs themselves really do differ, or this test would pass vacuously.
    // Headings, bullets and task boxes no longer differ — the prefix went, and the
    // other two are ASCII in both sets — so the last thing that changes in the
    // document body is a code fence's language icon.
    assert_ne!(
        render_with("```rust\ncode\n```\n", 20, &PLAIN).plain_text(),
        render_with("```rust\ncode\n```\n", 20, &nerd).plain_text()
    );
}

#[test]
fn every_icon_the_renderer_draws_has_a_plain_substitute() {
    // The fence must be unindented, or it parses as an indented code block and draws
    // no language icon — which would leave the Nerd render with no private-use code
    // point at all and make the second assertion below fail for the wrong reason.
    // It used to be indented, and passed only because the task boxes were pictographs.
    let markdown = concat!(
        "# h1\n## h2\n### h3\n#### h4\n##### h5\n###### h6\n\n",
        "- a\n  - b\n    - c\n      - d\n\n",
        "- [x] y\n- [ ] n\n\n",
        "```rust\ncode\n```\n",
    );
    let plain = render_with(markdown, 40, &PLAIN);
    let fancy = render_with(markdown, 40, &RenderOptions::new(true, false));
    assert_eq!(plain.height(), fancy.height());
    // No private-use code point may survive with icons off. The predicate is the
    // shared one rather than a range written out here: Nerd Fonts use plane 15 as
    // well as the basic-plane area, and a hand-written `\u{e000}..=\u{f8ff}` silently
    // stopped seeing the task boxes the day they moved.
    assert!(
        !plain.plain_text().chars().any(is_private_use),
        "the plain set must contain no Nerd Font code points"
    );
    assert!(
        fancy.plain_text().chars().any(is_private_use),
        "the Nerd set must actually use them"
    );
}

#[test]
fn a_code_fence_shows_a_language_icon_only_when_icons_are_on() {
    let markdown = "```rust\ncode\n```\n";
    let plain = lines(markdown, 24);
    assert!(plain[0].starts_with("╭ rust"), "{:?}", plain[0]);
    let fancy = render_with(markdown, 24, &RenderOptions::new(true, false));
    let title = fancy.row_text(0);
    assert!(title.contains("rust"), "{title:?}");
    assert!(title.contains('\u{e7a8}'), "{title:?}");
}

#[test]
fn line_numbers_draw_a_themed_gutter() {
    let markdown = "```\none\ntwo\n```\n";
    let numbered = RenderOptions::new(false, true);
    let canvas = render_body(markdown, 20, &numbered);
    let out = body_rows(&canvas);
    assert_eq!(
        out,
        [
            "╭───┬──────────────╮",
            "│ 1 │ one          │",
            "│ 2 │ two          │",
            "╰───┴──────────────╯"
        ]
    );
    let theme = Theme::default_dark();
    // The body starts at BODY_COL with the frame, then one column of padding, then
    // the numbers and the rule that separates them from the code.
    assert_eq!(style_at(&canvas, 1, BODY_COL + 2), theme.code.line_number);
    assert_eq!(style_at(&canvas, 1, BODY_COL + 4), theme.code.frame);
}

#[test]
fn the_gutter_is_as_wide_as_the_largest_line_number() {
    let body: String = (1..=12).map(|n| format!("line{n}\n")).collect();
    let markdown = format!("```\n{body}```\n");
    let out = body_rows(&render_body(
        &markdown,
        20,
        &RenderOptions::new(false, true),
    ));
    assert!(out[1].starts_with("│  1 │"), "{:?}", out[1]);
    assert!(out[12].starts_with("│ 12 │"), "{:?}", out[12]);
    assert!(
        out[0].contains('┬'),
        "the gutter joins the frame: {:?}",
        out[0]
    );
    assert!(
        out[13].contains('┴'),
        "the gutter joins the frame: {:?}",
        out[13]
    );
}

/// A fence with a language *and* a gutter joins both edges.
///
/// The label and the `┬` want the same columns, and the label used to win outright, so
/// the gutter came out closed at the bottom and open at the top — chrome that looks
/// like it failed to draw. The label now starts after the junction.
#[test]
fn a_labelled_fence_still_joins_its_gutter_to_the_top_rule() {
    let markdown = "```rust\nfn main() {}\n```\n";
    let out = body_rows(&render_body(markdown, 30, &RenderOptions::new(false, true)));
    assert_eq!(out[0], "╭───┬ rust ──────────────────╮", "{out:?}");
    assert_eq!(out[2], "╰───┴────────────────────────╯", "{out:?}");
    assert!(out[1].starts_with("│ 1 │"), "{:?}", out[1]);
}

#[test]
fn the_gutter_is_outside_the_clipped_region() {
    let markdown = "```\nabcdefghijklmnopqrstuvwxyz\n```\n";
    let numbered = render_body(markdown, 12, &RenderOptions::new(false, true));
    let bare = render_body(markdown, 12, &PLAIN);
    let row = body_rows(&numbered)[1].clone();
    assert!(
        row.starts_with("│ 1 │"),
        "the gutter survives clipping: {row:?}"
    );
    assert!(row.contains('›'), "the code is still clipped: {row:?}");
    // The gutter costs code columns; it never widens the block or hides the marker.
    assert_eq!(numbered.width(), bare.width());
    let bare_row = body_rows(&bare)[1].clone();
    assert!(bare_row.contains('›'));
    let code_columns = |text: &str| text.chars().filter(char::is_ascii_alphabetic).count();
    assert!(
        code_columns(&row) < code_columns(&bare_row),
        "less code fits once the gutter takes its columns: {row:?}"
    );
}

#[test]
fn a_block_too_narrow_for_a_gutter_drops_it_rather_than_the_code() {
    let markdown = "```\nabcdef\n```\n";
    for width in 1..=8u16 {
        let canvas = render_with(markdown, width, &RenderOptions::new(false, true));
        assert_eq!(canvas.width(), width);
        canvas.check_invariants().expect("contract holds");
    }
    // At six columns the frame leaves four, which cannot carry a gutter and code
    // both, so the gutter goes and the code keeps every column it can.
    let narrow = render_body(markdown, 6, &RenderOptions::new(false, true));
    assert_eq!(body_rows(&narrow)[1], "│ a› │");
}

#[test]
fn options_reach_into_table_cells() {
    let table = table_with_cell("- one\n");
    let theme = Theme::default_dark();
    let plain = render_block(&table, 30, &theme, &PLAIN);
    let fancy = render_block(&table, 30, &theme, &RenderOptions::new(true, false));
    // The bullet is the same plain Unicode either way (`render::glyphs`); what the
    // options must still reach into the cell with is the *rest* of the glyph set, so
    // the check is that they agree here and differ where they should.
    assert!(plain.plain_text().contains('*'));
    assert!(
        fancy.plain_text().contains('*'),
        "the cell must use the same bullet:\n{}",
        fancy.plain_text()
    );
    assert_eq!(plain.height(), fancy.height());
    assert_eq!(plain.width(), fancy.width());
}

#[test]
fn a_code_block_inside_a_table_cell_honours_both_flags() {
    // The deepest recursion the options have to survive: document -> table -> row ->
    // cell -> block sequence -> code block, where both flags change what is drawn.
    let table = table_with_cell("```rust\nfn a() {}\nlet b = 2;\n```\n");
    let theme = Theme::default_dark();

    let plain = render_block(&table, 60, &theme, &RenderOptions::new(false, false));
    plain.check_invariants().expect("contract holds");
    let plain_text = plain.plain_text();
    assert!(plain_text.contains("╭ rust"), "plain title:\n{plain_text}");
    assert!(
        !plain_text
            .chars()
            .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch)),
        "no Nerd glyph may reach a cell with icons off:\n{plain_text}"
    );
    assert!(
        !plain_text.contains("1 │fn"),
        "no gutter with line numbers off:\n{plain_text}"
    );

    let fancy = render_block(&table, 60, &theme, &RenderOptions::new(true, true));
    fancy.check_invariants().expect("contract holds");
    let fancy_text = fancy.plain_text();
    assert!(
        fancy_text.contains('\u{e7a8}'),
        "the language icon must reach the cell:\n{fancy_text}"
    );
    assert!(
        fancy_text.contains("1 │ fn a() {}") && fancy_text.contains("2 │ let b = 2;"),
        "the gutter must reach the cell:\n{fancy_text}"
    );

    assert_eq!(plain.width(), fancy.width());
    for canvas in [&plain, &fancy] {
        for row in 0..canvas.height() {
            assert_eq!(display_width(&canvas.row_text(row)), 60);
        }
    }
}

#[test]
fn a_narrow_cell_still_clips_its_code_and_keeps_the_gutter() {
    let table = table_with_cell("```\nabcdefghijklmnopqrstuvwxyz\n```\n");
    let theme = Theme::default_dark();
    for width in 1..=30u16 {
        for options in [
            RenderOptions::new(false, false),
            RenderOptions::new(true, true),
        ] {
            let canvas = render_block(&table, width, &theme, &options);
            assert_eq!(canvas.width(), width);
            canvas.check_invariants().expect("contract holds");
        }
    }
    let numbered = render_block(&table, 24, &theme, &RenderOptions::new(false, true));
    let text = numbered.plain_text();
    assert!(
        text.contains("1 │"),
        "gutter survives inside a cell:\n{text}"
    );
    assert!(text.contains('›'), "the code is still clipped:\n{text}");
}

#[test]
fn the_default_options_are_icons_on_and_line_numbers_off() {
    let defaults = RenderOptions::default();
    assert!(defaults.icons);
    assert!(!defaults.line_numbers);
    assert_eq!(defaults, RenderOptions::new(true, false));
}

// ------------------------------------------------------------------- metadata

#[test]
fn search_spans_map_source_offsets_onto_the_canvas() {
    let source = "hello brave world\n";
    let doc = Doc::parse(source);
    let canvas = render_flat(&doc, 40, &Theme::default_dark(), &PLAIN);
    // Unwrapped, the whole run is one contiguous mapping.
    assert_eq!(canvas.spans().len(), 1);
    let span = canvas.spans()[0];
    assert_eq!(
        &source[span.source_start..span.source_end],
        "hello brave world"
    );
    assert_eq!((span.row, span.col, span.cols), (0, DOCUMENT_MARGIN, 17));
}

#[test]
fn a_wrap_splits_the_mapping_at_the_line_break() {
    let source = "hello brave world\n";
    let doc = Doc::parse(source);
    let canvas = render_flat(
        &doc,
        11 + 2 * DOCUMENT_MARGIN,
        &Theme::default_dark(),
        &PLAIN,
    );
    let texts: Vec<(&str, usize, u16)> = canvas
        .spans()
        .iter()
        .map(|span| {
            (
                &source[span.source_start..span.source_end],
                span.row,
                span.col,
            )
        })
        .collect();
    let margin = DOCUMENT_MARGIN;
    assert_eq!(texts, [("hello brave", 0, margin), ("world", 1, margin)]);
    for span in canvas.spans() {
        let row = body_rows(&canvas)[span.row].clone();
        assert!(row.starts_with(&source[span.source_start..span.source_end]));
    }
}

#[test]
fn search_spans_follow_content_into_lists_quotes_and_cells() {
    for markdown in [
        "- needle\n",
        "> needle\n",
        "| h |\n|---|\n| needle |\n",
        "**needle**\n",
    ] {
        let doc = Doc::parse(markdown);
        let canvas = render_flat(&doc, 30, &Theme::default_dark(), &PLAIN);
        let span = canvas
            .spans()
            .iter()
            .find(|span| markdown[span.source_start..span.source_end].contains("needle"))
            .unwrap_or_else(|| panic!("no span for {markdown:?}"));
        assert!(canvas.row_text(span.row).contains("needle"));
        assert!(usize::from(span.col) < usize::from(canvas.width()));
    }
}

#[test]
fn rendering_is_deterministic() {
    let markdown = include_str!("../../tests/corpus/adversarial.md");
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    for width in [13u16, 40, 80] {
        let first = render_flat(&doc, width, &theme, &PLAIN);
        let second = render_flat(&doc, width, &theme, &PLAIN);
        assert_eq!(first, second, "width {width} is not deterministic");
    }
}

#[test]
fn an_empty_document_renders_an_empty_canvas_of_the_right_width() {
    let canvas = render("", 30);
    assert_eq!(canvas.width(), 30);
    assert_eq!(canvas.height(), 0);
}

#[test]
fn a_zero_width_budget_is_survivable() {
    let canvas = render("# Title\n\ntext\n", 0);
    assert_eq!(canvas.width(), 0);
}

#[test]
fn render_block_and_render_blocks_agree_with_the_document_renderer() {
    // Two level-1 headings, so the document renderer does not add the title banner
    // that only it knows about: that is the one place the two paths differ, by
    // design, and `a_lone_top_level_heading_is_drawn_as_a_banner` is where it is
    // asserted rather than worked around.
    let markdown = "# Title\n\nbody text\n\n# Another\n";
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    let whole = render_flat(&doc, 30 + 2 * DOCUMENT_MARGIN, &theme, &PLAIN);
    let parts = render_blocks(&doc.root().children, 30, &theme, &PLAIN);
    assert_eq!(
        body_rows(&whole),
        body_rows(&parts.indent(DOCUMENT_MARGIN, DOCUMENT_MARGIN, theme.base()))
    );
    let heading = render_block(&doc.root().children[0], 30, &theme, &PLAIN);
    assert!(heading.row_text(0).contains("Title"));
}

#[test]
fn render_table_ignores_a_node_that_is_not_a_table() {
    let doc = Doc::parse("text\n");
    let canvas = render_table(&doc.root().children[0], 20, &Theme::default_dark(), &PLAIN);
    assert!(canvas.is_empty());
    assert_eq!(canvas.width(), 20);
}

// --------------------------------------------------------------------- margins

/// Markdown exercising every block that draws to the edge of its budget.
const MARGIN_FIXTURE: &str = "\
# Heading one

A paragraph long enough to fill the width and wrap onto a second line of text.

---

| a | b |
| - | - |
| 1 | 2 |

```rust
fn wide() -> &'static str { \"a line long enough to be clipped by the frame\" }
```

> a quote

- a list item
";

#[test]
fn every_row_keeps_a_margin_on_both_sides() {
    let doc = Doc::parse(MARGIN_FIXTURE);
    let theme = Theme::default_dark();
    for width in [20u16, 40, 60, 80, 100, 120] {
        let canvas = render_flat(&doc, width, &theme, &PLAIN);
        let margin = usize::from(DOCUMENT_MARGIN);
        for row in 0..canvas.height() {
            let text = canvas.row_text(row);
            let (head, rest) = crate::text::split_at_width(&text, margin);
            let (body, tail) = crate::text::split_at_width(rest, usize::from(width) - 2 * margin);
            assert_eq!(head, " ", "width {width} row {row}: left margin {text:?}");
            assert_eq!(tail, " ", "width {width} row {row}: right margin {text:?}");
            let _ = body;
        }
    }
}

#[test]
fn the_margin_is_dropped_only_when_it_would_leave_no_body() {
    assert_eq!(margins(0), 0);
    assert_eq!(margins(1), 0);
    assert_eq!(margins(2), 0);
    assert_eq!(margins(3), DOCUMENT_MARGIN);
    assert_eq!(margins(120), DOCUMENT_MARGIN);
    // Degenerate widths still satisfy the canvas contract.
    for width in 0..=4u16 {
        let canvas = render(MARGIN_FIXTURE, width);
        assert_eq!(canvas.width(), width);
    }
}

// ------------------------------------------------------- table width negotiation

#[test]
fn a_table_stops_at_its_natural_width_however_wide_the_terminal() {
    for width in [40u16, 80, 120] {
        let out = lines("| Only |\n|------|\n| a |\n| b |\n", width);
        assert_eq!(
            out,
            [
                "╭──────╮",
                "│ Only │",
                "├──────┤",
                "│ a    │",
                "│ b    │",
                "╰──────╯"
            ],
            "at width {width} the table must not become a {width}-column void"
        );
    }
}

/// A header rule separates a header from a body, so a table with no body draws none.
///
/// With one, the box ends `├───┤` `╰───╯`: two rules with nothing between them, which
/// is how box art spells an empty row. Three reviewers read it as a table that failed
/// to draw.
#[test]
fn a_table_with_no_body_rows_closes_after_its_header() {
    let out = lines("| A | B |\n|---|---|\n", 40);
    assert_eq!(out, ["╭───┬───╮", "│ A │ B │", "╰───┴───╯"]);
}

#[test]
fn identical_columns_negotiate_identical_widths() {
    // Three columns wanting the same amount, with less room than they want between
    // them: the rounding must not leave a visible one-column stagger.
    let cell = "aaaa bbbb cccc dddd eeee";
    let markdown = format!("| h | h | h |\n|---|---|---|\n| {cell} | {cell} | {cell} |\n");
    let out = lines(&markdown, 40);
    let widths: Vec<usize> = out[0]
        .trim_matches(|c| c == '╭' || c == '╮')
        .split('┬')
        .map(display_width)
        .collect();
    assert_eq!(widths.len(), 3, "{:?}", out[0]);
    assert_eq!(
        widths[0], widths[1],
        "columns with identical content must come out identical: {:?}",
        out[0]
    );
    assert_eq!(widths[1], widths[2], "{:?}", out[0]);
}

#[test]
fn a_column_gets_its_natural_width_rather_than_wrapping_beside_a_padded_neighbour() {
    let markdown = "| feature | detail |\n|---|---|\n| nested table | see the doc |\n";
    let out = lines(markdown, 80);
    assert!(
        out.iter().any(|row| row.contains("│ nested table │")),
        "a 12-column cell must not wrap while its neighbour carries blanks: {out:?}"
    );
}

#[test]
fn a_cell_is_measured_as_a_sentence_not_as_its_longest_word() {
    // Bare inline siblings in a cell wrap together, so they must be measured together.
    let out = lines("| md |\n|----|\n| *em* and `code` |\n", 40);
    assert!(
        out.iter().any(|row| row.contains("em and code")),
        "the run must be measured as one line: {out:?}"
    );
}

// ------------------------------------------------------------------- footnotes

#[test]
fn footnote_references_are_numbered_in_document_order() {
    let markdown = "one[^a] two[^long] three[^a]\n\n[^a]: first\n[^long]: second\n";
    let out = lines(markdown, 40);
    assert_eq!(out[0], "one[1] two[2] three[1]");
    assert!(out.iter().any(|row| row == "[1] first"), "{out:?}");
    assert!(out.iter().any(|row| row == "[2] second"), "{out:?}");
}

#[test]
fn an_unreferenced_footnote_definition_keeps_its_name() {
    // comrak may drop it entirely; if it survives, it must still be identifiable.
    let out = lines("text\n\n[^ghost]: nobody points here\n", 40);
    assert!(
        !out.iter().any(|row| row.starts_with("[]")),
        "a definition with no number must not render an empty label: {out:?}"
    );
}

// ------------------------------------------------------------------ inline HTML

#[test]
fn a_br_tag_breaks_the_line_instead_of_leaving_a_marker() {
    for spelling in ["<br>", "<br/>", "<br />", "<BR>"] {
        let markdown = format!("| k | v |\n|---|---|\n| list | - one{spelling}- two |\n");
        let out = lines(&markdown, 60);
        assert!(
            out.iter().any(|row| row.contains("- one"))
                && out.iter().any(|row| row.contains("- two")),
            "{spelling} must break the cell: {out:?}"
        );
        assert!(
            !out.concat().contains(inline::HTML_MARKER),
            "{spelling}: {out:?}"
        );
    }
}

#[test]
fn other_html_leaves_a_spaced_marker_rather_than_a_word() {
    let out = lines("before <b>middle</b> after\n", 40);
    assert_eq!(out, ["before ⟨html⟩ middle after"]);
}

// ----------------------------------------------------------------------- links

#[test]
fn a_link_target_is_suppressed_inside_a_table_cell() {
    let url = "https://example.com/a/very/long/url/that/keeps/going/and/going";
    let markdown = format!("| k | v |\n|---|---|\n| link | [example]({url}) |\n");
    let out = lines(&markdown, 80);
    assert!(out.iter().any(|row| row.contains("example")), "{out:?}");
    assert!(
        !out.concat().contains("example.com"),
        "one URL must not claim a whole row: {out:?}"
    );
}

#[test]
fn a_long_link_target_is_elided_in_the_middle() {
    let url = "https://example.com/a/very/long/url/that/keeps/going/and/going";
    let out = lines(&format!("[text]({url})\n"), 120);
    let shown = out.concat();
    assert!(shown.contains('…'), "{shown:?}");
    assert!(shown.contains("https://"), "the host survives: {shown:?}");
    assert!(shown.contains("going)"), "the tail survives: {shown:?}");
    assert!(!shown.contains(url), "{shown:?}");
}

// ---------------------------------------------------------------------- images

#[test]
fn an_image_with_no_alt_text_does_not_say_image_twice() {
    let out = lines("![](empty-alt.png)\n", 40);
    assert_eq!(out[0].matches("image").count(), 1, "{out:?}");
    assert!(
        out.iter().any(|row| row.contains("empty-alt.png")),
        "{out:?}"
    );
}

// ---------------------------------------------------------------- block quotes

#[test]
fn nested_quotes_do_not_stack_a_gutter_per_level() {
    let out = lines("> > > > level four\n", 40);
    assert_eq!(out, ["▌▌▌▌ level four"]);
    // A level that carries text of its own still gets its separating space.
    assert_eq!(lines("> one\n", 40), ["▌ one"]);
}

// ------------------------------------------------------------ mermaid captions

#[test]
fn the_advertised_families_are_the_ones_that_actually_parse() {
    // The caption promises these render; a promise the parser does not keep is worse
    // than saying nothing, so the list is pinned to reality rather than maintained.
    for family in code::FAMILIES {
        let out = lines(&format!("```mermaid\n{family}\n```\n"), 80);
        let last = out.last().cloned().unwrap_or_default();
        assert!(
            !last.contains("not a diagram type"),
            "{family} is advertised but the parser rejects the family: {out:?}"
        );
    }
    // Wide enough that the caption is not elided, so every promise is visible.
    let out = lines("```mermaid\nnonsensekeyword\n```\n", 200);
    let last = out.last().cloned().unwrap_or_default();
    assert!(last.contains("not a diagram type"), "{out:?}");
    for family in code::FAMILIES {
        assert!(
            last.contains(family),
            "the caption must name {family}: {last:?}"
        );
    }
}

#[test]
fn a_too_narrow_caption_names_a_width_worth_widening_to() {
    // The old wording restated the width the reader already had, so it was a tautology
    // and widening the terminal was a guessing game. The number must be a width they do
    // not have, and widening to it must actually work — a floor that lies is worse than
    // no floor, because it is acted on.
    let markdown = "```mermaid\nflowchart LR\n  A[Start here] --> B{Is the label long?}\n  B --> C[Report the outcome]\n  C --> D[Finish up here]\n```\n";
    let width = 30;
    let out = lines(markdown, width);
    let last = out.last().cloned().unwrap_or_default();
    let needed: u16 = last
        .split_whitespace()
        .skip_while(|word| *word != "needs")
        .nth(1)
        .and_then(|word| word.parse().ok())
        .unwrap_or_else(|| panic!("caption names a floor: {last:?}"));
    assert!(
        needed > width - 2,
        "the floor is a width we do not have: {last:?}"
    );
    // `lines` budgets the body at exactly this many columns, which is what the caption
    // is counting. Every width from the named one up must draw, or the advice sends the
    // reader somewhere that fails.
    for body in needed..needed + 8 {
        let out = lines(markdown, body);
        let last = out.last().cloned().unwrap_or_default();
        assert!(
            !last.contains("needs"),
            "widening to {body} was supposed to be enough: {last:?}"
        );
    }
    // And one column short of it must still fail: the caption states a width flatly,
    // with no "at least" to hide behind, so it has to be the exact one.
    let short = lines(markdown, needed - 1);
    assert!(
        short.last().cloned().unwrap_or_default().contains("needs"),
        "the caption names {needed} columns, but one fewer already draws"
    );
}

#[test]
fn a_caption_never_reports_line_zero() {
    // Lines are 1-based to a reader; "line 0" sends them hunting somewhere that cannot
    // exist. An internal error with no line simply omits the location.
    for markdown in [
        "```mermaid\nnonsense\n```\n",
        "```mermaid\nsequenceDiagram\n  Alice ->> ??? oops\n```\n",
        "```mermaid\n\n```\n",
        "```mermaid\nflowchart TD\n  A --> \n```\n",
    ] {
        let out = lines(markdown, 100);
        assert!(!out.concat().contains("line 0"), "{markdown:?} -> {out:?}");
    }
}

#[test]
fn a_syntax_error_names_its_line_and_quotes_the_offending_text() {
    let out = lines(
        "```mermaid\nsequenceDiagram\n  Alice ->> ??? oops\n```\n",
        100,
    );
    let last = out.last().cloned().unwrap_or_default();
    assert!(last.contains("line 2"), "{last:?}");
    assert!(last.contains("Alice ->> ??? oops"), "{last:?}");
}

// -------------------------------------------------- wide clusters and autolinks

#[test]
fn a_cluster_wider_than_a_cell_is_charged_the_columns_it_draws() {
    // U+17000 plus a *spacing* Tai Tham mark is one grapheme drawing three columns.
    // Pricing it at the two a cell can hold walks the search-span cursor left of its
    // own text and drags every following span with it.
    let source = "\u{17000}\u{1a57} tail\n";
    let doc = Doc::parse(source);
    let canvas = render_flat(&doc, 30, &Theme::default_dark(), &PLAIN);
    canvas.check_invariants().expect("contract holds");
    for span in canvas.spans() {
        let text = &source[span.source_start..span.source_end];
        let row = canvas.row_text(span.row);
        let at = crate::text::split_at_width(&row, usize::from(span.col)).1;
        assert!(
            at.starts_with(text),
            "span for {text:?} points at {at:?} in {row:?}"
        );
    }
}

#[test]
fn a_bare_email_address_does_not_grow_a_mailto_tail() {
    let out = lines("Author: tobi@oetiker.ch\n", 60);
    assert_eq!(out, ["Author: tobi@oetiker.ch"]);
}

/// A table with enough body rows that the second one is striped.
const STRIPED: &str = "\
| a | b |
| - | - |
| 1 | 2 |
| 3 | 4 |
| 5 | 6 |
";

#[test]
fn a_striped_row_is_shaded_from_border_to_border() {
    // The stripe told the reader "these two rows go together" and the vertical rules
    // punched a hole in it at every column boundary, because `table.border` carries
    // the *page* background. A banded row rendered as two separate shaded boxes with
    // an unshaded gap between them — in the light theme, as two selected cells.
    let theme = Theme::default_dark();
    let stripe = theme
        .table
        .row_alt
        .bg
        .expect("the stripe is defined as a background");
    let page = theme.palette.bg;
    assert_ne!(stripe, page, "the stripe must differ from the page");

    let canvas = render(STRIPED, 40);
    let banded: Vec<usize> = (0..canvas.height())
        .filter(|&row| {
            canvas
                .row(row)
                .is_some_and(|cells| cells.iter().any(|cell| cell.style().bg == Some(stripe)))
        })
        .collect();
    assert_eq!(
        banded.len(),
        1,
        "exactly one body row of three is striped: {banded:?}"
    );

    let cells = canvas.row(banded[0]).expect("the striped row");
    let first = cells
        .iter()
        .position(|cell| cell.style().bg == Some(stripe))
        .expect("a striped cell");
    let last = cells
        .iter()
        .rposition(|cell| cell.style().bg == Some(stripe))
        .expect("a striped cell");
    let text: String = cells[first..=last].iter().map(|cell| cell.text()).collect();
    assert!(
        text.contains('\u{2502}'),
        "the span under test must actually contain a column separator: {text:?}"
    );
    for (offset, cell) in cells[first..=last].iter().enumerate() {
        assert_eq!(
            cell.style().bg,
            Some(stripe),
            "column {} ({:?}) breaks the stripe in {text:?}",
            first + offset,
            cell.text()
        );
    }
}

/// A three-column table whose prose column wraps at any width a terminal has.
const WRAPPING: &str = "\
| part | does | note |
| --- | --- | --- |
| renderer | turns the parsed document into a canvas of styled cells | pure function of width |
| pager | owns the viewport, scrolling and the key bindings | never lays anything out |
| theme | every colour and attribute the renderer may reach for | two built-ins |
";

/// The upper and lower half-block glyphs a table's row gap is shaded with.
const UPPER_HALF: char = '\u{2580}';
const LOWER_HALF: char = '\u{2584}';

/// The rows of `canvas` that are a table's row gap: nothing but column separators and
/// half-block shading.
///
/// The shading is what identifies them, which is sound because a gap always has exactly
/// one striped neighbour: the zebra alternates, so no two adjacent body rows are both
/// plain.
fn gap_rows(canvas: &Canvas) -> Vec<usize> {
    (0..canvas.height())
        .filter(|&row| {
            let text = canvas.row_text(row);
            text.contains([UPPER_HALF, LOWER_HALF])
                && text
                    .chars()
                    .all(|ch| matches!(ch, UPPER_HALF | LOWER_HALF | '\u{2502}' | ' '))
        })
        .collect()
}

#[test]
fn a_table_whose_rows_wrap_gets_air_between_them() {
    // Six content lines with nothing between them read as one block of prose: the row
    // boundaries are invisible, because the only cue left is where a cell happens to
    // stop. The gap is what puts them back.
    let canvas = render(WRAPPING, 70);
    let out = body_rows(&canvas);
    assert!(
        out.iter().any(|row| row.contains("canvas of styled")),
        "the fixture must actually wrap at this width: {out:#?}"
    );
    assert_eq!(
        gap_rows(&canvas).len(),
        2,
        "three body rows want two gaps: {out:#?}"
    );
}

#[test]
fn a_table_of_one_line_rows_stays_compact() {
    // Air between rows that need none is just a taller table.
    let canvas = render(STRIPED, 40);
    assert_eq!(
        gap_rows(&canvas),
        Vec::<usize>::new(),
        "{:#?}",
        body_rows(&canvas)
    );
}

#[test]
fn the_gap_shades_the_half_that_touches_the_striped_row() {
    // The half block is a foreground glyph on the page background, so it shades half of
    // the row's height. It has to be the half adjacent to the striped row, or the band
    // detaches from the rows it is grouping.
    let theme = Theme::default_dark();
    let stripe = theme
        .table
        .row_alt
        .bg
        .expect("the stripe is defined as a background");
    let canvas = render(WRAPPING, 70);
    let striped = |row: usize| {
        canvas
            .row(row)
            .is_some_and(|cells| cells.iter().any(|cell| cell.style().bg == Some(stripe)))
    };
    let gaps = gap_rows(&canvas);
    assert!(!gaps.is_empty(), "no gap to inspect");
    for gap in gaps {
        let text = canvas.row_text(gap);
        let want = if striped(gap - 1) {
            assert!(!striped(gap + 1), "both neighbours striped at row {gap}");
            UPPER_HALF
        } else {
            assert!(striped(gap + 1), "neither neighbour striped at row {gap}");
            LOWER_HALF
        };
        let other = if want == UPPER_HALF {
            LOWER_HALF
        } else {
            UPPER_HALF
        };
        assert!(
            text.contains(want),
            "row {gap} shades the wrong half: {text:?}"
        );
        assert!(
            !text.contains(other),
            "row {gap} shades both halves: {text:?}"
        );
        for cell in canvas.row(gap).expect("a gap row") {
            if cell.text() == want.to_string() {
                assert_eq!(
                    cell.style().fg,
                    Some(stripe),
                    "the shading must be the stripe colour, painted as foreground"
                );
            }
        }
    }
    for glyph in [UPPER_HALF, LOWER_HALF] {
        assert_eq!(
            display_width(&glyph.to_string()),
            1,
            "{glyph:?} must occupy exactly one column"
        );
    }
}

#[test]
fn the_gap_keeps_the_column_separators() {
    // A gap that dropped the vertical rules would punch a row-high hole in every one of
    // them, and the box would stop reading as a table.
    let canvas = render(WRAPPING, 70);
    let gaps = gap_rows(&canvas);
    assert!(!gaps.is_empty(), "no gap to inspect");
    let separators = |row: usize| -> Vec<usize> {
        canvas
            .row(row)
            .expect("a row")
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.text() == "\u{2502}")
            .map(|(index, _)| index)
            .collect()
    };
    for gap in gaps {
        assert_eq!(
            separators(gap),
            separators(gap - 1),
            "gap row {gap} does not carry the same rules as the row above it"
        );
    }
}

#[test]
fn the_gap_never_lands_against_a_rule() {
    // The rule under the header already separates it from the body, and the top and
    // bottom borders close the box; a gap next to any of them is padding, not structure.
    let canvas = render(WRAPPING, 70);
    let out = body_rows(&canvas);
    let gaps = gap_rows(&canvas);
    assert!(!gaps.is_empty(), "no gap to inspect");
    for gap in gaps {
        for neighbour in [gap - 1, gap + 1] {
            let text = out[neighbour].trim().to_string();
            assert!(
                !text.starts_with(['\u{256d}', '\u{251c}', '\u{2570}']),
                "gap row {gap} sits against the rule {text:?}"
            );
        }
    }
}

#[test]
fn a_cut_gap_row_is_not_marked_as_having_more_to_the_right() {
    // A gap carries no content, so nothing is lost when it is cut: a chevron there would
    // claim something continues where nothing does, and would stack into a column of
    // markers beside the ones the content rows honestly earn.
    let canvas = render(WRAPPING, 34);
    let out = body_rows(&canvas);
    assert!(
        out.iter().any(|row| row.contains(code::OVERFLOW_MARKER)),
        "the table must actually be clipped at this width: {out:#?}"
    );
    let gaps = gap_rows(&canvas);
    assert!(!gaps.is_empty(), "no gap to inspect");
    for gap in gaps {
        assert!(
            !out[gap].contains(code::OVERFLOW_MARKER),
            "cut gap row {gap} claims content continues: {:?}",
            out[gap]
        );
    }
}

#[test]
fn a_code_line_carries_a_span_back_to_the_source() {
    let markdown = "```rust\nlet a = 1;\n```\n";
    let canvas = render(markdown, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| &markdown[s.source_start..s.source_end] == "let a = 1;")
        .expect("the code line maps back to the source");
    assert_eq!(
        canvas.row_text(span.row)[..]
            .chars()
            .skip(usize::from(span.col))
            .take(usize::from(span.cols))
            .collect::<String>(),
        "let a = 1;"
    );
}

#[test]
fn a_clipped_code_line_spans_only_the_drawn_columns() {
    // The frame, its padding and the overflow marker leave far less than the line needs.
    let markdown = "```\nabcdefghijklmnopqrstuvwxyz\n```\n";
    let canvas = render(markdown, 14);
    let span = canvas
        .spans()
        .iter()
        .find(|s| markdown[s.source_start..s.source_end].starts_with('a'))
        .expect("a span for the clipped line");
    let text = &markdown[s_range(span)];
    assert!(
        text.len() < "abcdefghijklmnopqrstuvwxyz".len(),
        "the span must stop where the drawing stopped, got {text:?}"
    );
    assert!(
        !text.contains('z'),
        "the clipped tail is not on screen and must not be spanned"
    );
}

/// The byte range of a span, as a `Range`, for slicing the source in assertions.
fn s_range(span: &crate::canvas::SearchSpan) -> std::ops::Range<usize> {
    span.source_start..span.source_end
}

#[test]
fn the_line_number_gutter_carries_no_span() {
    let options = RenderOptions::new(false, true);
    let markdown = "```\nlet a = 1;\n```\n";
    let canvas = render_with(markdown, 40, &options);
    let mut saw_a_span = false;
    for span in canvas.spans() {
        let text = &markdown[s_range(span)];
        assert!(
            !text.trim().is_empty() && !text.chars().all(|c| c.is_ascii_digit()),
            "a gutter number is not in the document: {text:?}"
        );
        // The text check alone is vacuous: a span starting at column 0, covering both
        // the gutter and the code, would still slice `"let a = 1;"` out of the source —
        // the gutter's own cells carry no bytes of their own to leak into that check.
        // Design §3 is a claim about *columns*, so assert one directly: with a single
        // line this block's gutter is `digit_count(1) + 3 == 4` columns wide
        // (`code::gutter_width`), and a span starting inside it would prove the rule
        // broken even though the text happened to read back correctly.
        assert!(
            span.col >= 4,
            "span at column {} starts inside the gutter, not the code",
            span.col
        );
        saw_a_span = true;
    }
    assert!(saw_a_span, "the code line must have produced a span at all");
}

#[test]
fn a_tab_in_the_code_maps_to_its_source_byte_not_its_expanded_columns() {
    // `bridge::highlight` expands the tab to spaces before this line ever reaches the
    // canvas (`highlight::expand_tabs`); the source line is 13 bytes, its *drawn* text
    // is 16 columns wide. Measuring `source_end` against the drawn text instead of the
    // raw source line lands three bytes past the end of this line — into the newline
    // and the closing fence.
    let markdown = "```\n\tfn main() {}\n```\n";
    let canvas = render(markdown, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| markdown[s.source_start..s.source_end].contains("fn main"))
        .expect("a span for the tabbed line");
    assert_eq!(
        &markdown[s_range(span)],
        "\tfn main() {}",
        "the span must cover exactly this source line, tab included, and nothing past it"
    );
}

#[test]
fn a_row_that_exactly_fits_is_not_shortened_because_another_row_overflows() {
    // The block's *widest* line drives `natural`, which used to also decide whether
    // *every* row reserved a column for the overflow marker. A shorter row that lands
    // exactly on the code budget is not clipped at all — `Canvas::clip_with_edges`
    // marks a row only when it actually has a non-blank cell past the cut — so basing
    // its span on the block-wide decision reported it one column and one byte short of
    // what was actually drawn.
    let short = "A".repeat(16);
    let long = "B".repeat(26);
    let markdown = format!("```\n{short}\n{long}\n```\n");
    // `render_body` fixes the body budget at exactly `width`, so the frame's chrome
    // (`code::chrome_width`, two border columns and one padding column each side) can
    // be reasoned about directly: at a body width of 20 the code area is exactly 16
    // columns, precisely `short`'s length.
    let canvas = render_body(&markdown, 20, &PLAIN);
    let span = canvas
        .spans()
        .iter()
        .find(|s| markdown[s.source_start..s.source_end].starts_with('A'))
        .expect("a span for the exactly-fitting row");
    assert_eq!(
        &markdown[s_range(span)],
        short,
        "the full row must be spanned, not one column short"
    );
    assert_eq!(usize::from(span.cols), 16);
}

#[test]
fn a_crlf_authored_fence_maps_to_the_source_without_the_carriage_return() {
    // comrak keeps a CRLF document's `\r` inside a code block's `literal`
    // (`literal == "let needle = 1;\r\n"`), but `LineOffsets::line` strips it from the
    // *source* line `doc::convert::code_lines` searches — so before that function
    // stripped it too, `text.ends_with(line)` could never hold and every line of every
    // code block in a CRLF document got no provenance at all. `doc::literal_lines`'s
    // split and `bridge::highlight`'s both split only on `\n`, so the `\r` travels
    // with the same line on both sides and the row alignment between `origins` and the
    // drawn lines was never in question — only the byte match was broken.
    let markdown = "```rust\r\nlet needle = 1;\r\n```\r\n";
    let canvas = render(markdown, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| markdown[s.source_start..s.source_end].contains("needle"))
        .expect("a CRLF-authored code line must still map to its source");
    let text = &markdown[s_range(span)];
    assert_eq!(text, "let needle = 1;");
    assert!(
        !text.contains('\r'),
        "a copy must never carry the line's own carriage return: {text:?}"
    );
}

#[test]
fn a_crlf_fence_with_a_blank_line_still_maps_the_lines_around_it() {
    // The blank-line branch of `code_lines` returns early, without advancing the
    // search index, before ever reaching the `ends_with` match this fix repairs — so
    // it needs its own coverage: a CRLF literal's blank line is `"\r"`, not `""`, and
    // has to be recognised as empty *after* the `\r` is stripped, or every line after
    // it searches from the wrong index.
    let markdown = "```rust\r\nlet needle = 1;\r\n\r\nlet other = 2;\r\n```\r\n";
    let canvas = render(markdown, 40);
    let needle = canvas
        .spans()
        .iter()
        .find(|s| markdown[s.source_start..s.source_end].contains("needle"))
        .expect("the line before the blank line must map");
    assert_eq!(&markdown[s_range(needle)], "let needle = 1;");
    let other = canvas
        .spans()
        .iter()
        .find(|s| markdown[s.source_start..s.source_end].contains("other"))
        .expect("the line after the blank line must map");
    assert_eq!(&markdown[s_range(other)], "let other = 2;");
}

#[test]
fn a_clipped_tabbed_line_maps_to_the_tab_aware_source_position() {
    // A full, unclipped tabbed line proves nothing about the *measurement*: the
    // correct byte end for a full line is `origin.end`, and the clamp
    // (`.min(origin.end)`) lands there regardless of whether the walk that fed it was
    // tab-aware or not, as long as the (buggy) walk overshoots — which a tab-expanded
    // walk always does. Clipping the line is what isolates the fix: the correct byte
    // end is then strictly *less* than `origin.end`, so a walk measured against the
    // wrong (expanded) text lands on a wrong-but-in-bounds offset that the clamp
    // cannot catch.
    let raw = format!("\t{}", "x".repeat(50));
    let markdown = format!("```\n{raw}\n```\n");
    let canvas = render(&markdown, 20);
    let span = canvas
        .spans()
        .iter()
        .find(|s| markdown[s.source_start..s.source_end].starts_with('\t'))
        .expect("a span for the clipped tabbed line");
    // 20 columns, minus the frame/padding chrome and the overflow marker, draws 13
    // columns of code: the tab expands to 4 (TAB_WIDTH, starting at column 0) and 9
    // more columns are plain `x`s — 10 raw bytes (the tab plus 9 `x`s), well short of
    // `origin.end` (55), so nothing here is rescued by the clamp.
    assert_eq!(&markdown[s_range(span)], "\txxxxxxxxx");
}

#[test]
fn a_code_frame_offers_a_copy_button() {
    let canvas = render_with("```rust\nlet a = 1;\n```\n", 40, &BUTTONS);
    let top = canvas.row_text(0);
    assert!(top.contains("[copy]"), "got {top:?}");
    let spot = canvas.hotspots().first().expect("a hotspot");
    assert_eq!(spot.row, 0);
    assert_eq!(spot.cols, 6);
    assert_eq!(spot.text, "let a = 1;\n");
    assert_eq!(spot.html, None, "code has one flavour");
}

#[test]
fn the_copy_button_is_off_by_default() {
    let canvas = render("```rust\nlet a = 1;\n```\n", 40);
    assert!(!canvas.row_text(0).contains("[copy]"));
    assert!(canvas.hotspots().is_empty());
}

#[test]
fn a_narrow_code_frame_drops_the_button_entirely() {
    let canvas = render_with("```rust\nlet a = 1;\n```\n", 16, &BUTTONS);
    assert!(!canvas.row_text(0).contains("[copy]"));
    assert!(
        canvas.hotspots().is_empty(),
        "a label without a hotspot would be a control that does nothing"
    );
}

#[test]
fn the_button_never_overwrites_the_language_label() {
    let canvas = render_with("```rust\nlet a = 1;\n```\n", 40, &BUTTONS);
    let top = canvas.row_text(0);
    assert!(top.contains("rust"), "the language label survives: {top:?}");
}

#[test]
fn the_button_yields_to_the_gutter_junction() {
    // A wide, three-digit gutter (500 numbered lines) pushes `join_gutter`'s `┬` far
    // enough right — and, because the label and the junction want the same columns,
    // pushes the relocated `rust` label with it (`╭─────┬ rust ──...─╮`) — that at a
    // canvas width of 26 (frame width 24, after the one-column document margin on each
    // side) `top_edge_occupied` reports the top edge occupied past the point
    // `button::place` needs two spare columns beyond, so the button is dropped rather
    // than drawn over the junction or the label. One column narrower and it stays
    // dropped; two columns wider (width 28) it fits cleanly, so 26 is the exact seam
    // where only the junction protection — not mere width — is what drops it.
    let options = RenderOptions::new(false, true).with_copy_button(true);
    let markdown = format!("```rust\n{}```\n", "x\n".repeat(500));
    let canvas = render_with(&markdown, 26, &options);
    let top = canvas.row_text(0);
    assert!(!top.contains("[copy]"), "the junction survives: {top:?}");
    assert!(
        canvas.hotspots().is_empty(),
        "a dropped button leaves no hotspot: {:?}",
        canvas.hotspots()
    );
}

#[test]
fn a_failed_mermaid_block_offers_its_source() {
    // The fence degraded to a highlighted code block showing Mermaid source, and that
    // source is exactly what a reader who just saw the failure caption wants.
    let canvas = render_with("```mermaid\nnot a diagram at all\n```\n", 40, &BUTTONS);
    let spot = canvas
        .hotspots()
        .first()
        .expect("a hotspot on the fallback");
    assert_eq!(spot.text, "not a diagram at all\n");
}

#[test]
fn a_code_block_in_a_table_cell_shows_no_button() {
    // GFM table cells are single-line and inline-only — a fence cannot open and close
    // inside one, so there is no markdown source that puts a `NodeKind::CodeBlock`
    // under a `NodeKind::TableCell`. `table_with_cell` (used by
    // `a_list_inside_a_cell_keeps_its_markers` and
    // `a_nested_table_inside_a_cell_is_rendered_as_a_table` above) splices an
    // independently parsed block into a cell's children, which is the only way to reach
    // this shape and is already this file's precedent for it.
    let table = table_with_cell("```rust\nlet value = 1234567890;\n```\n");
    let canvas = render_block(&table, 80, &Theme::default_dark(), &BUTTONS);
    canvas.check_invariants().expect("contract holds");
    let text = canvas.plain_text();
    assert!(
        !text.contains("[copy]"),
        "a code block inside a table cell must draw no button: {text}"
    );
    assert!(
        canvas.hotspots().is_empty(),
        "a code block inside a table cell must record no hotspot: {:?}",
        canvas.hotspots()
    );
}
