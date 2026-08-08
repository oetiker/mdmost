//! Unit tests for the internals: language resolution, tab stops, line semantics and
//! the size guards. Behaviour visible through the public API is exercised by the
//! integration suites in `tests/highlight_*.rs`.

use super::*;

/// Every alias must point at a token the default syntax set actually knows, otherwise
/// the alias silently does nothing and the tag falls back to plain text.
#[test]
fn every_alias_resolves_to_a_real_syntax() {
    for (alias, token) in ALIASES {
        assert!(
            SYNTAX_SET.find_syntax_by_token(token).is_some(),
            "alias {alias} points at unknown syntect token {token}"
        );
    }
}

/// The table is scanned linearly and read by humans; keep it sorted and unique.
#[test]
fn alias_table_is_sorted_and_free_of_duplicates() {
    for pair in ALIASES.windows(2) {
        assert!(
            pair[0].0 < pair[1].0,
            "alias table out of order at {} / {}",
            pair[0].0,
            pair[1].0
        );
    }
}

/// An alias must never shadow a tag `syntect` already resolves differently.
#[test]
fn aliases_only_cover_tags_syntect_misses_or_misresolves() {
    for (alias, token) in ALIASES {
        let direct = SYNTAX_SET.find_syntax_by_token(alias).map(|s| &s.name);
        let aliased = SYNTAX_SET.find_syntax_by_token(token).map(|s| &s.name);
        assert_ne!(
            direct, aliased,
            "alias {alias} is redundant; remove it from the table"
        );
    }
}

#[test]
fn info_string_is_reduced_to_its_first_token() {
    let rust = syntax_name(Some("rust"));
    assert!(rust.is_some());
    for tag in ["rust,no_run", "rust ignore", "  RUST  ", "rust{.line}"] {
        assert_eq!(syntax_name(Some(tag)), rust, "tag {tag}");
    }
}

#[test]
fn absent_or_unknown_tags_resolve_to_nothing() {
    assert_eq!(syntax_name(None), None);
    assert_eq!(syntax_name(Some("")), None);
    assert_eq!(syntax_name(Some("   ")), None);
    assert_eq!(syntax_name(Some("brainfuck-9000")), None);
}

#[test]
fn eol_stripping_handles_lf_and_crlf() {
    assert_eq!(strip_eol("a\n"), "a");
    assert_eq!(strip_eol("a\r\n"), "a");
    assert_eq!(strip_eol("a"), "a");
    assert_eq!(strip_eol("\n"), "");
    assert_eq!(strip_eol("a\rb\n"), "a\rb");
}

#[test]
fn tabs_expand_to_the_next_tab_stop() {
    let mut column = 0;
    assert_eq!(expand_tabs("\tx", &mut column), "    x");
    assert_eq!(column, 5);

    let mut column = 0;
    assert_eq!(expand_tabs("ab\tc", &mut column), "ab  c");
    assert_eq!(column, 5);

    // Continuing an already-started line keeps the stops global to the line.
    let mut column = 3;
    assert_eq!(expand_tabs("\tx", &mut column), " x");
    assert_eq!(column, 5);
}

#[test]
fn tab_stops_account_for_double_width_clusters() {
    let mut column = 0;
    // "日" is two columns wide, so the tab only needs two more to reach column 4.
    assert_eq!(expand_tabs("日\tx", &mut column), "日  x");
    assert_eq!(column, 5);
}

#[test]
fn oversized_blocks_degrade_to_plain_text() {
    let theme = Theme::default_dark();

    let wide = format!("let x = \"{}\";\n", "y".repeat(MAX_HIGHLIGHT_BYTES));
    let lines = highlight(Some("rust"), &wide, &theme);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].spans.len(), 1, "oversized block must not be split");
    assert_eq!(lines[0].spans[0].style, theme.code.text);

    let tall = "let x = 1;\n".repeat(MAX_HIGHLIGHT_LINES + 1);
    assert!(tall.len() < MAX_HIGHLIGHT_BYTES, "must trip the line guard");
    let lines = highlight(Some("rust"), &tall, &theme);
    assert_eq!(lines.len(), MAX_HIGHLIGHT_LINES + 1);
    assert!(lines.iter().all(|l| l.spans.len() == 1));
}

/// Tabs are expanded *after* parsing, so a tab-sensitive syntax still sees the tab.
#[test]
fn makefile_recipe_lines_still_parse_with_a_leading_tab() {
    let theme = Theme::default_dark();
    let lines = highlight(Some("makefile"), "all:\n\techo hi\n", &theme);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].text(), "    echo hi");
    // The recipe body is recognised as shell, not left as undifferentiated text.
    assert!(
        lines[1].spans.iter().any(|s| s.style != theme.code.text),
        "recipe line lost its highlighting: {:?}",
        lines[1]
    );
}
