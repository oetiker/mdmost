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

/// Renders `markdown` at `width`, checking the universal invariants.
fn render(markdown: &str, width: u16) -> Canvas {
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    let canvas = render_document(&doc, width, &theme);
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

/// The rendered rows with trailing padding removed, which is what tests read.
fn rows(markdown: &str, width: u16) -> Vec<String> {
    let canvas = render(markdown, width);
    (0..canvas.height())
        .map(|row| canvas.row_text(row).trim_end().to_string())
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
    assert_eq!(row.trim_end(), "em st del code");
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
    let attrs = style_at(&canvas, 0, 0).attrs;
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
    assert_eq!(out, ["a[1]", "[n] body"]);
}

#[test]
fn html_never_reaches_the_canvas() {
    let block = lines("<div>secret</div>\n", 40);
    assert_eq!(block, ["⟨html⟩"]);
    let inline = lines("before <b>middle</b> after\n", 40);
    assert_eq!(inline, ["before ⟨html⟩middle after"]);
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
            render(&markdown, 10)
                .row_text(0)
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
        ["• one", "  ◦ two", "    ‣ three"]
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
fn a_thematic_break_fills_the_width() {
    assert_eq!(lines("---\n", 8), ["────────"]);
}

// ------------------------------------------------------------------ code blocks

#[test]
fn a_fenced_block_is_framed_and_titled_with_its_language() {
    let out = lines("```rust\nfn a() {}\n```\n", 20);
    assert_eq!(
        out,
        [
            "╭ rust ────────────╮",
            "│fn a() {}         │",
            "╰──────────────────╯"
        ]
    );
}

#[test]
fn an_untagged_block_is_still_framed() {
    let out = lines("```\nplain\n```\n", 12);
    assert_eq!(out, ["╭──────────╮", "│plain     │", "╰──────────╯"]);
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
    assert_eq!(out, ["╭────────╮", "│abcdefg›│", "╰────────╯"]);
}

#[test]
fn a_mermaid_fence_degrades_to_a_captioned_code_block() {
    // Deliberately not a diagram any Mermaid implementation could accept, so this
    // stays a test of the degradation path once the real renderer is wired in.
    let out = lines("```mermaid\nnot a diagram at all\n```\n", 60);
    assert!(out[0].starts_with("╭ mermaid"));
    assert!(out.iter().any(|row| row.contains("not a diagram at all")));
    let start = out
        .iter()
        .position(|row| row.starts_with("unsupported mermaid syntax:"))
        .unwrap_or_else(|| panic!("no caption in {out:?}"));
    assert!(!out[start..].join(" ").is_empty());
}

// ----------------------------------------------------------------------- images

#[test]
fn an_image_becomes_a_framed_placeholder_with_alt_text_and_target() {
    let out = lines("![the alt](pic.png)\n", 20);
    assert_eq!(
        out,
        [
            "╭ image ───────────╮",
            "│the alt           │",
            "│pic.png           │",
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
            "╭─────────┬─────────╮",
            "│ a       │ b       │",
            "├─────────┼─────────┤",
            "│ 1       │ 2       │",
            "╰─────────┴─────────╯"
        ],
        "slack is spread evenly once every column has its natural width"
    );
}

#[test]
fn per_column_alignment_is_honoured() {
    let markdown = "| l | c | r |\n|:--|:-:|--:|\n| x | x | x |\n";
    let out = lines(markdown, 25);
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
    let canvas = render_block(&table, 30, &Theme::default_dark());
    canvas.check_invariants().expect("contract holds");
    let text = canvas.plain_text();
    assert!(text.contains("• one"), "{text}");
    assert!(text.contains("• two"), "{text}");
}

#[test]
fn a_nested_table_inside_a_cell_is_rendered_as_a_table() {
    let table = table_with_cell("| in |\n|----|\n| y |\n");
    let canvas = render_block(&table, 40, &Theme::default_dark());
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
        let canvas = render_block(&table, width, &Theme::default_dark());
        assert_eq!(canvas.width(), width);
        canvas.check_invariants().expect("contract holds");
    }
}

#[test]
fn a_table_narrower_than_its_minimums_is_clipped_with_a_marker() {
    let markdown = "| aaaaaaaaaa | bbbbbbbbbb |\n|---|---|\n| cccccccccc | dddddddddd |\n";
    let canvas = render(markdown, 12);
    let row = find_row(&canvas, "aaa");
    assert!(
        canvas.row_text(row).ends_with('›'),
        "clipped rows carry the overflow marker: {:?}",
        canvas.row_text(row)
    );
}

#[test]
fn the_full_table_canvas_keeps_the_columns_the_viewport_scrolls_through() {
    let markdown = "| aaaaaaaaaa | bbbbbbbbbb |\n|---|---|\n| cccccccccc | dddddddddd |\n";
    let doc = Doc::parse(markdown);
    let theme = Theme::default_dark();
    let table = &doc.root().children[0];
    let full = render_table_full(table, 12, &theme);
    assert!(
        full.width() > 12,
        "the unclipped canvas is wider than the budget"
    );
    assert!(full.plain_text().contains("bbbbbbbbbb"));
    full.check_invariants().expect("contract holds");
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

// ------------------------------------------------------------------- metadata

#[test]
fn search_spans_map_source_offsets_onto_the_canvas() {
    let source = "hello brave world\n";
    let doc = Doc::parse(source);
    let canvas = render_document(&doc, 40, &Theme::default_dark());
    // Unwrapped, the whole run is one contiguous mapping.
    assert_eq!(canvas.spans().len(), 1);
    let span = canvas.spans()[0];
    assert_eq!(
        &source[span.source_start..span.source_end],
        "hello brave world"
    );
    assert_eq!((span.row, span.col, span.cols), (0, 0, 17));
}

#[test]
fn a_wrap_splits_the_mapping_at_the_line_break() {
    let source = "hello brave world\n";
    let doc = Doc::parse(source);
    let canvas = render_document(&doc, 11, &Theme::default_dark());
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
    assert_eq!(texts, [("hello brave", 0, 0), ("world", 1, 0)]);
    for span in canvas.spans() {
        let row = canvas.row_text(span.row);
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
        let canvas = render_document(&doc, 30, &Theme::default_dark());
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
        let first = render_document(&doc, width, &theme);
        let second = render_document(&doc, width, &theme);
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
    let whole = render_document(&doc, 30, &theme);
    let parts = render_blocks(&doc.root().children, 30, &theme);
    assert_eq!(whole.plain_text(), parts.plain_text());
    let heading = render_block(&doc.root().children[0], 30, &theme);
    assert!(heading.row_text(0).contains("Title"));
}

#[test]
fn render_table_ignores_a_node_that_is_not_a_table() {
    let doc = Doc::parse("text\n");
    let canvas = render_table(&doc.root().children[0], 20, &Theme::default_dark());
    assert!(canvas.is_empty());
    assert_eq!(canvas.width(), 20);
}
