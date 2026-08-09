//! Unit tests for the internals: language resolution, tab stops, line semantics and
//! the size guards. Behaviour visible through the public API is exercised by the
//! integration suites in `tests/highlight_*.rs`.

use super::*;

/// The bundled set must be the widened one, not `syntect`'s own defaults.
///
/// `syntect`'s `load_defaults_newlines` is the Sublime bundle as of 2016 and knows
/// nothing of TypeScript, Kotlin, Swift, Zig, Nix, Terraform or GraphQL. Swapping
/// [`BUNDLED_SYNTAXES`] back to it would compile, pass every per-language test that
/// happens to name an old language, and quietly halve the coverage — so measure the
/// difference here rather than trusting the call site to stay right.
#[test]
fn the_bundled_set_is_wider_than_syntects_own_defaults() {
    let syntect_defaults = SyntaxSet::load_defaults_newlines().syntaxes().len();
    let bundled = BUNDLED_SYNTAXES.syntaxes().len();
    assert!(
        bundled > syntect_defaults + 100,
        "bundled set has {bundled} syntaxes against syntect's {syntect_defaults}; that is \
         not the widened set"
    );
}

/// Every alias must point at a token the bundled syntax set actually knows, otherwise
/// the alias silently does nothing and the tag falls back to plain text.
#[test]
fn every_alias_resolves_to_a_real_syntax() {
    for (alias, token) in ALIASES {
        assert!(
            resolve_syntax(Some(token)).is_some(),
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
        // Deliberately bypasses the alias table on the left-hand side: the question is
        // what the raw tag would resolve to *without* the alias.
        let direct = find_in_sets(alias).map(|(_, syntax)| syntax.name.as_str());
        let aliased = find_in_sets(token).map(|(_, syntax)| syntax.name.as_str());
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

/// The two languages the bundled set deliberately drops, recorded rather than assumed.
///
/// `two-face` ships two builds of the same curation, one per `syntect` regex engine, and
/// excludes the definitions whose regexes the pure-Rust `fancy-regex` engine cannot
/// compile. `mdless` picks the pure-Rust engine on purpose (no C toolchain, see
/// `Cargo.toml`), so PowerShell and ARM assembly are the price. They fall back to plain
/// text like any unknown tag — the point of this test is that the *README* says so, and
/// this fails the day that stops being true, which is the day the README needs editing.
#[test]
fn the_languages_the_fancy_regex_build_drops_are_still_the_ones_the_readme_names() {
    assert_eq!(syntax_name(Some("powershell")), None);
    assert_eq!(syntax_name(Some("ps1")), None);
    // x86_64 assembly survives; only the ARM definition is dropped.
    assert_eq!(syntax_name(Some("asm")), Some("x86_64 Assembly"));
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

/// A definition that fails to parse is skipped at runtime; that must never actually
/// happen, so assert every one of them made it into the set.
#[test]
fn every_extra_syntax_loads() {
    assert_eq!(
        EXTRA_SYNTAX_SET.syntaxes().len(),
        EXTRA_SYNTAXES.len(),
        "an extra syntax definition failed to parse"
    );
    for (name, _) in EXTRA_SYNTAXES {
        assert!(
            EXTRA_SYNTAX_SET.syntaxes().iter().any(|s| &s.name == name),
            "extra syntax {name} is missing or its `name` key does not match the table"
        );
    }
}

/// The extra definitions must be reachable by their own file extensions, which is what
/// makes them need no [`ALIASES`] entry.
#[test]
fn extra_syntaxes_resolve_without_an_alias() {
    assert_eq!(syntax_name(Some("toml")), Some("TOML"));
    assert_eq!(syntax_name(Some("dockerfile")), Some("Dockerfile"));
    assert_eq!(syntax_name(Some("containerfile")), Some("Dockerfile"));
    assert!(!ALIASES.iter().any(|(alias, _)| *alias == "toml"));
}

/// The two sets must stay separate: merging them would re-link every bundled syntax.
/// A `ParseState` is also only valid against the set its syntax came from, so this
/// asserts each tag resolves against the set that actually owns it.
#[test]
fn each_syntax_is_paired_with_its_own_set() {
    let (set, syntax) = resolve_syntax(Some("toml")).expect("toml resolves");
    assert!(std::ptr::eq(set, &*EXTRA_SYNTAX_SET));
    assert!(set.find_syntax_by_name(&syntax.name).is_some());

    let (set, syntax) = resolve_syntax(Some("rust")).expect("rust resolves");
    assert!(std::ptr::eq(set, &*BUNDLED_SYNTAXES));
    assert!(set.find_syntax_by_name(&syntax.name).is_some());
}
