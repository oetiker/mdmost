// SPDX-License-Identifier: MIT
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
/// compile. `mdmost` picks the pure-Rust engine on purpose (no C toolchain, see
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

/// A key no other test uses, so this test's entry is never confused with another
/// test's. It does not by itself protect against another test's theme switch —
/// see [`HIGHLIGHT_GLOBALS_TEST_LOCK`], which every test that touches the cache
/// holds for that reason.
const TASK1_SRC: &str = "let task1_unique_probe = 1;\n";

#[test]
fn a_second_highlight_of_the_same_block_is_not_recomputed() {
    let _guard = HIGHLIGHT_GLOBALS_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let theme = Theme::default_dark();
    assert_eq!(computed_count(Some("rust"), TASK1_SRC, &theme), None);

    let first = highlight(Some("rust"), TASK1_SRC, &theme);
    assert_eq!(computed_count(Some("rust"), TASK1_SRC, &theme), Some(1));

    let second = highlight(Some("rust"), TASK1_SRC, &theme);
    assert_eq!(computed_count(Some("rust"), TASK1_SRC, &theme), Some(1));
    assert_eq!(first, second);
}

/// The cache is keyed on the theme's code styles, so a second theme is a miss
/// rather than a wrong-coloured hit.
#[test]
fn a_different_theme_recomputes_rather_than_reusing_the_colours() {
    let _guard = HIGHLIGHT_GLOBALS_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    const SRC: &str = "let task1_theme_probe = 2;\n";
    let dark = Theme::default_dark();
    let light = Theme::default_light();

    let in_dark = highlight(Some("rust"), SRC, &dark);
    let in_light = highlight(Some("rust"), SRC, &light);

    assert_eq!(in_dark.len(), in_light.len());
    assert_ne!(in_dark, in_light, "the two themes colour code differently");
}

/// The two lines from `docs/upstream/2026-09-21-javascript-syntax-hang.md`.
///
/// This test used to be `the_javascript_hang_degrades_to_plain_text` and asserted that
/// the pager survived by giving up. Patch 1 in `vendor/syntect` fixes the loop, so the
/// block is now highlighted like any other. Kept rather than deleted: it is the only
/// test naming the input that caused the vendoring, and if a re-sync loses the patch
/// this is what says so.
#[test]
fn the_javascript_continuation_then_block_comment_is_highlighted() {
    let theme = Theme::default();
    let src = "  | { type: \"a\" }\n  /** x */\n";
    let lines = highlight(Some("js"), src, &theme);
    assert_eq!(lines.len(), 2);
    assert_ne!(
        lines,
        plain(src, &theme.code),
        "the block should be highlighted, not degraded"
    );
}

/// A single, deliberately dense minified JavaScript line: several thousand characters,
/// packed with string literals, regex literals, template literals, numbers, nested
/// arrow functions and operators. Synthetic (written for this test, not pasted from a
/// third-party library) so it carries no licence or attribution duty; measured at 6,505
/// tokens by the bisection in `docs/maintainer-notes.md` — a stand-in for dense,
/// real-world minified output, not a claim about the densest line any minifier could
/// produce (a real `jquery.min.js` build is far longer and was measured too; see the
/// same notes).
pub(crate) const MINIFIED_JS_LINE: &str = "(()=>{let v0=(a=>(b=>(c=>(a+b*c-0)**2)(2))(3))(4);const s0='str_0_\\n_end';const t0=`tpl${v0+0}end`;const r0=/^[a-zA-Z0-9_0]{2,5}$/gi;v0+=r0.test(s0)?0:-0;v0??=0*0.5e1;v0=v0<<1>>>2|0&255^0x00;let v1=(a=>(b=>(c=>(a+b*c-1)**2)(2))(3))(4);const s1='str_1_\\n_end';const t1=`tpl${v1+1}end`;const r1=/^[a-zA-Z0-9_1]{2,5}$/gi;v1+=r1.test(s1)?1:-1;v1??=1*0.5e1;v1=v1<<1>>>2|1&255^0x01;let v2=(a=>(b=>(c=>(a+b*c-2)**2)(2))(3))(4);const s2='str_2_\\n_end';const t2=`tpl${v2+2}end`;const r2=/^[a-zA-Z0-9_2]{2,5}$/gi;v2+=r2.test(s2)?2:-2;v2??=2*0.5e1;v2=v2<<1>>>2|2&255^0x02;let v3=(a=>(b=>(c=>(a+b*c-3)**2)(2))(3))(4);const s3='str_3_\\n_end';const t3=`tpl${v3+3}end`;const r3=/^[a-zA-Z0-9_3]{2,5}$/gi;v3+=r3.test(s3)?3:-3;v3??=3*0.5e1;v3=v3<<1>>>2|3&255^0x03;let v4=(a=>(b=>(c=>(a+b*c-4)**2)(2))(3))(4);const s4='str_4_\\n_end';const t4=`tpl${v4+4}end`;const r4=/^[a-zA-Z0-9_4]{2,5}$/gi;v4+=r4.test(s4)?4:-4;v4??=4*0.5e1;v4=v4<<1>>>2|4&255^0x04;let v5=(a=>(b=>(c=>(a+b*c-5)**2)(2))(3))(4);const s5='str_5_\\n_end';const t5=`tpl${v5+5}end`;const r5=/^[a-zA-Z0-9_5]{2,5}$/gi;v5+=r5.test(s5)?5:-5;v5??=5*0.5e1;v5=v5<<1>>>2|5&255^0x05;let v6=(a=>(b=>(c=>(a+b*c-6)**2)(2))(3))(4);const s6='str_6_\\n_end';const t6=`tpl${v6+6}end`;const r6=/^[a-zA-Z0-9_6]{2,5}$/gi;v6+=r6.test(s6)?6:-6;v6??=6*0.5e1;v6=v6<<1>>>2|6&255^0x06;let v7=(a=>(b=>(c=>(a+b*c-7)**2)(2))(3))(4);const s7='str_7_\\n_end';const t7=`tpl${v7+7}end`;const r7=/^[a-zA-Z0-9_7]{2,5}$/gi;v7+=r7.test(s7)?7:-7;v7??=7*0.5e1;v7=v7<<1>>>2|7&255^0x07;let v8=(a=>(b=>(c=>(a+b*c-8)**2)(2))(3))(4);const s8='str_8_\\n_end';const t8=`tpl${v8+8}end`;const r8=/^[a-zA-Z0-9_8]{2,5}$/gi;v8+=r8.test(s8)?8:-8;v8??=8*0.5e1;v8=v8<<1>>>2|8&255^0x08;let v9=(a=>(b=>(c=>(a+b*c-9)**2)(2))(3))(4);const s9='str_9_\\n_end';const t9=`tpl${v9+9}end`;const r9=/^[a-zA-Z0-9_9]{2,5}$/gi;v9+=r9.test(s9)?9:-9;v9??=9*0.5e1;v9=v9<<1>>>2|9&255^0x09;let v10=(a=>(b=>(c=>(a+b*c-10)**2)(2))(3))(4);const s10='str_10_\\n_end';const t10=`tpl${v10+10}end`;const r10=/^[a-zA-Z0-9_10]{2,5}$/gi;v10+=r10.test(s10)?10:-10;v10??=10*0.5e1;v10=v10<<1>>>2|10&255^0x0A;let v11=(a=>(b=>(c=>(a+b*c-11)**2)(2))(3))(4);const s11='str_11_\\n_end';const t11=`tpl${v11+11}end`;const r11=/^[a-zA-Z0-9_11]{2,5}$/gi;v11+=r11.test(s11)?11:-11;v11??=11*0.5e1;v11=v11<<1>>>2|11&255^0x0B;let v12=(a=>(b=>(c=>(a+b*c-12)**2)(2))(3))(4);const s12='str_12_\\n_end';const t12=`tpl${v12+12}end`;const r12=/^[a-zA-Z0-9_12]{2,5}$/gi;v12+=r12.test(s12)?12:-12;v12??=12*0.5e1;v12=v12<<1>>>2|12&255^0x0C;let v13=(a=>(b=>(c=>(a+b*c-13)**2)(2))(3))(4);const s13='str_13_\\n_end';const t13=`tpl${v13+13}end`;const r13=/^[a-zA-Z0-9_13]{2,5}$/gi;v13+=r13.test(s13)?13:-13;v13??=13*0.5e1;v13=v13<<1>>>2|13&255^0x0D;let v14=(a=>(b=>(c=>(a+b*c-14)**2)(2))(3))(4);const s14='str_14_\\n_end';const t14=`tpl${v14+14}end`;const r14=/^[a-zA-Z0-9_14]{2,5}$/gi;v14+=r14.test(s14)?14:-14;v14??=14*0.5e1;v14=v14<<1>>>2|14&255^0x0E;let v15=(a=>(b=>(c=>(a+b*c-15)**2)(2))(3))(4);const s15='str_15_\\n_end';const t15=`tpl${v15+15}end`;const r15=/^[a-zA-Z0-9_15]{2,5}$/gi;v15+=r15.test(s15)?15:-15;v15??=15*0.5e1;v15=v15<<1>>>2|15&255^0x0F;let v16=(a=>(b=>(c=>(a+b*c-16)**2)(2))(3))(4);const s16='str_16_\\n_end';const t16=`tpl${v16+16}end`;const r16=/^[a-zA-Z0-9_16]{2,5}$/gi;v16+=r16.test(s16)?16:-16;v16??=16*0.5e1;v16=v16<<1>>>2|16&255^0x10;let v17=(a=>(b=>(c=>(a+b*c-17)**2)(2))(3))(4);const s17='str_17_\\n_end';const t17=`tpl${v17+17}end`;const r17=/^[a-zA-Z0-9_17]{2,5}$/gi;v17+=r17.test(s17)?17:-17;v17??=17*0.5e1;v17=v17<<1>>>2|17&255^0x11;let v18=(a=>(b=>(c=>(a+b*c-18)**2)(2))(3))(4);const s18='str_18_\\n_end';const t18=`tpl${v18+18}end`;const r18=/^[a-zA-Z0-9_18]{2,5}$/gi;v18+=r18.test(s18)?18:-18;v18??=18*0.5e1;v18=v18<<1>>>2|18&255^0x12;let v19=(a=>(b=>(c=>(a+b*c-19)**2)(2))(3))(4);const s19='str_19_\\n_end';const t19=`tpl${v19+19}end`;const r19=/^[a-zA-Z0-9_19]{2,5}$/gi;v19+=r19.test(s19)?19:-19;v19??=19*0.5e1;v19=v19<<1>>>2|19&255^0x13;let v20=(a=>(b=>(c=>(a+b*c-20)**2)(2))(3))(4);const s20='str_20_\\n_end';const t20=`tpl${v20+20}end`;const r20=/^[a-zA-Z0-9_20]{2,5}$/gi;v20+=r20.test(s20)?20:-20;v20??=20*0.5e1;v20=v20<<1>>>2|20&255^0x14;let v21=(a=>(b=>(c=>(a+b*c-21)**2)(2))(3))(4);const s21='str_21_\\n_end';const t21=`tpl${v21+21}end`;const r21=/^[a-zA-Z0-9_21]{2,5}$/gi;v21+=r21.test(s21)?21:-21;v21??=21*0.5e1;v21=v21<<1>>>2|21&255^0x15;let v22=(a=>(b=>(c=>(a+b*c-22)**2)(2))(3))(4);const s22='str_22_\\n_end';const t22=`tpl${v22+22}end`;const r22=/^[a-zA-Z0-9_22]{2,5}$/gi;v22+=r22.test(s22)?22:-22;v22??=22*0.5e1;v22=v22<<1>>>2|22&255^0x16;let v23=(a=>(b=>(c=>(a+b*c-23)**2)(2))(3))(4);const s23='str_23_\\n_end';const t23=`tpl${v23+23}end`;const r23=/^[a-zA-Z0-9_23]{2,5}$/gi;v23+=r23.test(s23)?23:-23;v23??=23*0.5e1;v23=v23<<1>>>2|23&255^0x17;let v24=(a=>(b=>(c=>(a+b*c-24)**2)(2))(3))(4);const s24='str_24_\\n_end';const t24=`tpl${v24+24}end`;const r24=/^[a-zA-Z0-9_24]{2,5}$/gi;v24+=r24.test(s24)?24:-24;v24??=24*0.5e1;v24=v24<<1>>>2|24&255^0x18;let v25=(a=>(b=>(c=>(a+b*c-25)**2)(2))(3))(4);const s25='str_25_\\n_end';const t25=`tpl${v25+25}end`;const r25=/^[a-zA-Z0-9_25]{2,5}$/gi;v25+=r25.test(s25)?25:-25;v25??=25*0.5e1;v25=v25<<1>>>2|25&255^0x19;let v26=(a=>(b=>(c=>(a+b*c-26)**2)(2))(3))(4);const s26='str_26_\\n_end';const t26=`tpl${v26+26}end`;const r26=/^[a-zA-Z0-9_26]{2,5}$/gi;v26+=r26.test(s26)?26:-26;v26??=26*0.5e1;v26=v26<<1>>>2|26&255^0x1A;let v27=(a=>(b=>(c=>(a+b*c-27)**2)(2))(3))(4);const s27='str_27_\\n_end';const t27=`tpl${v27+27}end`;const r27=/^[a-zA-Z0-9_27]{2,5}$/gi;v27+=r27.test(s27)?27:-27;v27??=27*0.5e1;v27=v27<<1>>>2|27&255^0x1B;let v28=(a=>(b=>(c=>(a+b*c-28)**2)(2))(3))(4);const s28='str_28_\\n_end';const t28=`tpl${v28+28}end`;const r28=/^[a-zA-Z0-9_28]{2,5}$/gi;v28+=r28.test(s28)?28:-28;v28??=28*0.5e1;v28=v28<<1>>>2|28&255^0x1C;let v29=(a=>(b=>(c=>(a+b*c-29)**2)(2))(3))(4);const s29='str_29_\\n_end';const t29=`tpl${v29+29}end`;const r29=/^[a-zA-Z0-9_29]{2,5}$/gi;v29+=r29.test(s29)?29:-29;v29??=29*0.5e1;v29=v29<<1>>>2|29&255^0x1D;})();";

/// A legitimate long line keeps its colour.
///
/// The per-line token budget (`MAX_TOKENS_PER_LINE_BASE` plus `MAX_TOKENS_PER_BYTE` for
/// every byte, see `docs/maintainer-notes.md`) is a backstop against a syntax that will
/// not terminate, not a size policy. If someone tightens it far enough to degrade real
/// code, this fails instead of a reader's document quietly losing colour.
#[test]
fn a_long_minified_line_is_still_highlighted() {
    let theme = Theme::default();
    let src = format!("{}\n", MINIFIED_JS_LINE);
    let lines = highlight(Some("js"), &src, &theme);
    assert_ne!(lines, plain(&src, &theme.code));
}

/// A long line needs a budget that grows with it, not a flat cap.
///
/// Sixteen copies of [`MINIFIED_JS_LINE`] (each a self-contained statement, so
/// concatenating them stays valid JavaScript) come to 95,057 bytes and measure at
/// 104,065 tokens — more than `MAX_TOKENS_PER_LINE_BASE` alone could ever cover, so this
/// only stays highlighted because the per-byte term grows the budget with the line.
/// Still well under `MAX_HIGHLIGHT_BYTES`, so the block reaches the parser rather than
/// being pre-emptively rendered as plain for its size.
#[test]
fn a_much_longer_minified_line_still_scales_with_the_budget() {
    let theme = Theme::default();
    let src = format!("{}\n", MINIFIED_JS_LINE.repeat(16));
    let lines = highlight(Some("js"), &src, &theme);
    assert_ne!(lines, plain(&src, &theme.code));
}

/// A short line gets a small, cheap-to-exhaust budget: exactly
/// `MAX_TOKENS_PER_LINE_BASE + MAX_TOKENS_PER_BYTE * line.len()`, and nowhere near the
/// scale a long line needs. Without this, a flat limit large enough for a long line
/// (any value at or above the previous test's 104,065) would pass every other test in
/// this module while leaving a short, pathological line free to run up to that same
/// huge limit before the guard notices — exactly the risk proportional scaling exists
/// to avoid.
#[test]
fn a_short_lines_budget_is_the_exact_formula_and_stays_small() {
    let limit = token_limit_for("x\n").get();
    assert_eq!(limit, MAX_TOKENS_PER_LINE_BASE + MAX_TOKENS_PER_BYTE * 2);
    assert!(
        limit < 10_000,
        "a two-byte line must not be given a huge budget: got {limit}"
    );
}

/// The budget grows with the line: a line ten times longer gets a proportionally larger
/// budget, pinned to the exact formula so a future edit can't quietly flatten it back
/// out while still passing the tests above.
#[test]
fn the_budget_grows_with_the_line_by_the_exact_formula() {
    let short = "a".repeat(100);
    let long = "a".repeat(1_000);
    assert_eq!(
        token_limit_for(&short).get(),
        MAX_TOKENS_PER_LINE_BASE + MAX_TOKENS_PER_BYTE * 100
    );
    assert_eq!(
        token_limit_for(&long).get(),
        MAX_TOKENS_PER_LINE_BASE + MAX_TOKENS_PER_BYTE * 1_000
    );
    assert!(
        token_limit_for(&long).get() > token_limit_for(&short).get() * 2,
        "a line ten times longer must get a substantially larger budget, not a flat one"
    );
}

#[test]
fn an_untagged_block_is_plain_not_failed() {
    let theme = Theme::default();
    let src = "just text\n";
    highlight(None, src, &theme);
    assert_eq!(outcome(None, src, &theme), Outcome::Plain);
}

#[test]
fn an_unknown_tag_is_plain_not_failed() {
    let theme = Theme::default();
    let src = "just text\n";
    highlight(Some("no-such-language"), src, &theme);
    assert_eq!(
        outcome(Some("no-such-language"), src, &theme),
        Outcome::Plain
    );
}

#[test]
fn a_highlighted_block_says_so() {
    let theme = Theme::default();
    let src = "fn main() {}\n";
    highlight(Some("rust"), src, &theme);
    assert_eq!(outcome(Some("rust"), src, &theme), Outcome::Highlighted);
}

/// For `Failed`, drive a real token-limit error rather than a stub, so the test
/// exercises the path a reader would hit: the minified line from Task 4 under a
/// deliberately tiny limit, via the `#[cfg(test)]` seam rather than a mutable constant.
///
/// The source carries its own marker comment on a second line: the memo is keyed on
/// `(lang, src, theme)` alone, not on which limit computed the entry, so every test that
/// primes a `Failed` outcome needs a key no other test — locked or not — also touches.
/// The first line alone already exceeds a limit of two tokens, so the second line never
/// reaches the parser.
#[test]
fn a_block_that_exceeds_its_token_budget_is_failed() {
    let _guard = HIGHLIGHT_GLOBALS_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let theme = Theme::default();
    let src = format!("{MINIFIED_JS_LINE}\n// task5-highlight-outcome-probe\n");
    let lines = highlight_with_limit(Some("js"), &src, &theme, NonZeroUsize::new(2).unwrap());
    assert_eq!(lines, plain(&src, &theme.code));
    assert_eq!(outcome(Some("js"), &src, &theme), Outcome::Failed);
}
