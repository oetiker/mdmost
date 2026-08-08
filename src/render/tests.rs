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
    let full = render_table_full(table, 12, &theme, &PLAIN);
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
    let canvas = render_with(markdown, 20, &numbered);
    let out: Vec<String> = (0..canvas.height())
        .map(|row| canvas.row_text(row).trim_end().to_string())
        .collect();
    assert_eq!(
        out,
        [
            "╭──────────────────╮",
            "│1 │ one           │",
            "│2 │ two           │",
            "╰──────────────────╯"
        ]
    );
    let theme = Theme::default_dark();
    // Column 0 is the frame; the gutter occupies 1..=3 inside it.
    assert_eq!(style_at(&canvas, 1, 1), theme.code.line_number);
    assert_eq!(style_at(&canvas, 1, 3), theme.code.frame);
}

#[test]
fn the_gutter_is_as_wide_as_the_largest_line_number() {
    let body: String = (1..=12).map(|n| format!("line{n}\n")).collect();
    let markdown = format!("```\n{body}```\n");
    let canvas = render_with(&markdown, 20, &RenderOptions::new(false, true));
    assert!(
        canvas.row_text(1).starts_with("│ 1 │"),
        "{:?}",
        canvas.row_text(1)
    );
    assert!(
        canvas.row_text(12).starts_with("│12 │"),
        "{:?}",
        canvas.row_text(12)
    );
}

#[test]
fn the_gutter_is_outside_the_clipped_region() {
    let markdown = "```\nabcdefghijklmnopqrstuvwxyz\n```\n";
    let numbered = render_with(markdown, 12, &RenderOptions::new(false, true));
    let bare = render_with(markdown, 12, &PLAIN);
    let row = numbered.row_text(1);
    assert!(
        row.starts_with("│1 │"),
        "the gutter survives clipping: {row:?}"
    );
    assert!(row.contains('›'), "the code is still clipped: {row:?}");
    // The gutter costs code columns; it never widens the block or hides the marker.
    assert_eq!(numbered.width(), bare.width());
    assert!(bare.row_text(1).contains('›'));
    let code_columns = |text: &str| text.chars().filter(char::is_ascii_alphabetic).count();
    assert!(
        code_columns(&row) < code_columns(&bare.row_text(1)),
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
    let narrow = render_with(markdown, 6, &RenderOptions::new(false, true));
    assert_eq!(narrow.row_text(1), "│abc›│");
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
    assert_eq!((span.row, span.col, span.cols), (0, 0, 17));
}

#[test]
fn a_wrap_splits_the_mapping_at_the_line_break() {
    let source = "hello brave world\n";
    let doc = Doc::parse(source);
    let canvas = render_document(&doc, 11, &Theme::default_dark(), &PLAIN);
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
    let whole = render_document(&doc, 30, &theme, &PLAIN);
    let parts = render_blocks(&doc.root().children, 30, &theme, &PLAIN);
    assert_eq!(whole.plain_text(), parts.plain_text());
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
