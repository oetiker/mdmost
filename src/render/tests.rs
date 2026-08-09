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

/// Options the readable assertions below are written against.
///
/// Plain glyphs, because a Nerd Font code point in an `assert_eq!` is unreadable;
/// [`icons_change_the_glyphs_but_never_the_layout`] covers the other setting.
const PLAIN: RenderOptions = RenderOptions::new(false, false);

/// Renders `markdown` at `width` with the plain glyph set, checking the invariants.
fn render(markdown: &str, width: u16) -> Canvas {
    render_with(markdown, width, &PLAIN)
}

/// Renders `markdown` at `width` with explicit options, checking the invariants.
fn render_with(markdown: &str, width: u16, options: &RenderOptions) -> Canvas {
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    let canvas = render_document(&doc, width, &theme, options);
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

#[test]
fn headings_have_a_prefix_an_anchor_and_a_rule_for_levels_one_and_two() {
    for (level, ruled) in [(1u8, true), (2, true), (3, false), (6, false)] {
        let markdown = format!("{} Title\n", "#".repeat(usize::from(level)));
        let canvas = render(&markdown, 20);
        let first = canvas.row_text(0);
        assert!(
            first.trim_end().ends_with(" Title"),
            "level {level}: {first:?}"
        );
        assert_eq!(canvas.anchors().len(), 1);
        assert_eq!(canvas.anchors()[0].row, 0);
        assert_eq!(canvas.anchors()[0].level, level);
        assert_eq!(canvas.anchors()[0].id, "title");
        assert_eq!(canvas.height(), if ruled { 2 } else { 1 });
    }
}

#[test]
fn heading_levels_use_distinct_prefixes() {
    let prefixes: Vec<char> = (1..=6)
        .map(|level| {
            let markdown = format!("{} T\n", "#".repeat(level));
            rows(&markdown, 10)[0]
                .chars()
                .next()
                .expect("a prefix glyph")
        })
        .collect();
    let unique: std::collections::HashSet<char> = prefixes.iter().copied().collect();
    assert_eq!(unique.len(), 6, "every level needs its own glyph");
}

#[test]
fn heading_text_wraps_under_a_hanging_indent() {
    assert_eq!(
        lines("# a long heading that wraps\n", 12),
        ["◆ a long", "  heading", "  that wraps", "━━━━━━━━━━━━"]
    );
}

#[test]
fn anchors_are_recorded_for_every_heading_in_order() {
    let canvas = render("# One\n\ntext\n\n## Two\n\n## Two\n", 30);
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

// ----------------------------------------------------------------------- lists

#[test]
fn nested_lists_indent_and_change_their_bullet() {
    assert_eq!(
        lines("- one\n  - two\n    - three\n", 30),
        ["• one", "  ◦ two", "    ⁃ three"]
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
        ["• alpha beta", "  gamma", "  delta"]
    );
}

#[test]
fn task_items_get_a_checkbox() {
    assert_eq!(lines("- [x] done\n- [ ] todo\n", 20), ["☑ done", "☐ todo"]);
}

#[test]
fn tight_lists_are_dense_and_loose_lists_are_spaced() {
    assert_eq!(rows("- one\n- two\n", 20).len(), 2);
    assert_eq!(rows("- one\n\n- two\n", 20), ["• one", "", "• two"]);
}

#[test]
fn a_list_inside_a_quote_inside_a_list_still_composes() {
    assert_eq!(
        lines("- outer\n  > quoted\n  > - inner\n", 30),
        ["• outer", "  ▌ quoted", "  ▌", "  ▌ ◦ inner"]
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
        last.starts_with("╰ not a diagram type — mdless draws ") && last.ends_with('╯'),
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

#[test]
fn an_image_splits_the_paragraph_it_sits_in() {
    let out = lines("before ![a](p.png) after\n", 24);
    assert_eq!(out.first().map(String::as_str), Some("before"));
    assert_eq!(out.last().map(String::as_str), Some("after"));
    assert!(out.iter().any(|row| row.contains("p.png")));
}

#[test]
fn a_nested_image_degrades_to_its_alt_text() {
    assert_eq!(lines("[![a](p.png)](t.md)\n", 30), ["a (t.md)"]);
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
    assert!(text.contains("• one"), "{text}");
    assert!(text.contains("• two"), "{text}");
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
    // marker (`tui::wide::ClipTest`). A fence whose *content* is box art — this
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
    // This is the contract `tui::wide::render_scrollable` relies on for horizontal
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
    // The glyphs themselves really do differ.
    assert_ne!(
        render_with("# Title\n", 20, &PLAIN).row_text(0),
        render_with("# Title\n", 20, &nerd).row_text(0)
    );
}

#[test]
fn every_icon_the_renderer_draws_has_a_plain_substitute() {
    let markdown = "# h1\n## h2\n### h3\n#### h4\n##### h5\n###### h6\n\n                    - a\n  - b\n    - c\n      - d\n\n- [x] y\n- [ ] n\n\n                    ```rust\ncode\n```\n";
    let plain = render_with(markdown, 40, &PLAIN);
    let fancy = render_with(markdown, 40, &RenderOptions::new(true, false));
    assert_eq!(plain.height(), fancy.height());
    // No private-use code point may survive with icons off.
    assert!(
        !plain
            .plain_text()
            .chars()
            .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch)),
        "the plain set must contain no Nerd Font code points"
    );
    assert!(
        fancy
            .plain_text()
            .chars()
            .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch)),
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
    assert!(plain.plain_text().contains('•'));
    assert!(
        !fancy.plain_text().contains('•'),
        "the cell must use the Nerd bullet too:\n{}",
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
    let canvas = render_document(&doc, 40, &Theme::default_dark(), &PLAIN);
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
    let canvas = render_document(
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
        let canvas = render_document(&doc, 30, &Theme::default_dark(), &PLAIN);
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
        let first = render_document(&doc, width, &theme, &PLAIN);
        let second = render_document(&doc, width, &theme, &PLAIN);
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
    let markdown = "# Title\n\nbody text\n";
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    let whole = render_document(&doc, 30 + 2 * DOCUMENT_MARGIN, &theme, &PLAIN);
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
        let canvas = render_document(&doc, width, &theme, &PLAIN);
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

#[test]
fn a_table_with_no_body_rows_keeps_its_header_rule() {
    let out = lines("| A | B |\n|---|---|\n", 40);
    assert_eq!(out, ["╭───┬───╮", "│ A │ B │", "├───┼───┤", "╰───┴───╯"]);
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
    let canvas = render_document(&doc, 30, &Theme::default_dark(), &PLAIN);
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
