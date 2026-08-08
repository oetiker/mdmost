//! Per-language colouring: does a token that *is* a keyword get the theme's keyword
//! style, in both built-in themes?

use mdless::highlight::{highlight, syntax_name};
use mdless::text::Line;
use mdless::theme::{Style, Theme};

/// The style of the first span whose text equals `token`.
///
/// Spans are the highlighter's own token boundaries, so an exact match keeps the test
/// honest: it fails if the highlighter merges the token into a neighbouring run.
fn style_of(lines: &[Line], token: &str) -> Style {
    lines
        .iter()
        .flat_map(|line| &line.spans)
        .find(|span| span.text == token)
        .unwrap_or_else(|| {
            panic!(
                "no span exactly matching {token:?}; spans were {:?}",
                lines
                    .iter()
                    .flat_map(|l| &l.spans)
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
            )
        })
        .style
}

/// Runs `case` against both built-in themes, so a mapping can never be tuned for the
/// dark theme alone.
fn for_each_builtin_theme(case: impl Fn(&Theme)) {
    for name in Theme::builtin_names() {
        let theme = Theme::builtin(name).expect("built-in theme resolves");
        case(&theme);
    }
}

#[test]
fn rust_tokens_take_their_semantic_slot() {
    for_each_builtin_theme(|theme| {
        let src = "\
// leading note
pub struct Point { x: u32 }
fn main() {
    let n = 42;
    let s = \"hi\";
}
";
        let lines = highlight(Some("rust"), src, theme);
        let c = &theme.code;
        assert_eq!(style_of(&lines, "// leading note"), c.comment);
        assert_eq!(style_of(&lines, "struct"), c.keyword);
        assert_eq!(style_of(&lines, "Point"), c.type_name);
        assert_eq!(style_of(&lines, "main"), c.function);
        assert_eq!(style_of(&lines, "42"), c.number);
        assert_eq!(style_of(&lines, "\"hi\""), c.string);
        assert_eq!(style_of(&lines, "="), c.operator);
    });
}

#[test]
fn rust_string_escapes_break_out_of_the_string_colour() {
    for_each_builtin_theme(|theme| {
        let lines = highlight(Some("rust"), "let s = \"a\\nb\";\n", theme);
        assert_eq!(style_of(&lines, "\\n"), theme.code.constant);
        assert_ne!(theme.code.constant, theme.code.string);
    });
}

#[test]
fn python_tokens_take_their_semantic_slot() {
    for_each_builtin_theme(|theme| {
        let src = "@dec\nclass K:\n    def f(a=1):\n        return 'x'  # note\n";
        let lines = highlight(Some("python"), src, theme);
        let c = &theme.code;
        assert_eq!(style_of(&lines, "class"), c.keyword);
        assert_eq!(style_of(&lines, "K"), c.type_name);
        assert_eq!(style_of(&lines, "f"), c.function);
        assert_eq!(style_of(&lines, "1"), c.number);
        assert_eq!(style_of(&lines, "@dec"), c.attribute);
        assert_eq!(style_of(&lines, "# note"), c.comment);
    });
}

#[test]
fn c_tokens_take_their_semantic_slot() {
    for_each_builtin_theme(|theme| {
        let src = "/* head */\nint main(void) { return 0; }\n";
        let lines = highlight(Some("c"), src, theme);
        let c = &theme.code;
        assert_eq!(style_of(&lines, "int"), c.keyword);
        assert_eq!(style_of(&lines, "main"), c.function);
        assert_eq!(style_of(&lines, "return"), c.keyword);
        assert_eq!(style_of(&lines, "0"), c.number);
        assert_eq!(style_of(&lines, "/* head */"), c.comment);
    });
}

#[test]
fn javascript_tokens_take_their_semantic_slot() {
    for_each_builtin_theme(|theme| {
        let lines = highlight(Some("js"), "const n = 1; // tail\n", theme);
        let c = &theme.code;
        assert_eq!(style_of(&lines, "const"), c.keyword);
        assert_eq!(style_of(&lines, "1"), c.number);
        assert_eq!(style_of(&lines, "// tail"), c.comment);
    });
}

#[test]
fn shell_commands_read_as_functions() {
    for_each_builtin_theme(|theme| {
        let lines = highlight(Some("bash"), "echo hi | grep x\n", theme);
        assert_eq!(style_of(&lines, "echo"), theme.code.function);
        assert_eq!(style_of(&lines, "grep"), theme.code.function);
        assert_eq!(style_of(&lines, "|"), theme.code.operator);
    });
}

#[test]
fn yaml_and_json_keys_are_distinct_from_their_values() {
    for_each_builtin_theme(|theme| {
        let yaml = highlight(Some("yml"), "key: value\n", theme);
        assert_eq!(style_of(&yaml, "key"), theme.code.keyword);
        assert_eq!(style_of(&yaml, "value"), theme.code.string);

        let json = highlight(Some("json"), "{\"key\": \"value\", \"n\": 12}\n", theme);
        assert_eq!(style_of(&json, "key"), theme.code.attribute);
        assert_eq!(style_of(&json, "\"value\""), theme.code.string);
        assert_eq!(style_of(&json, "12"), theme.code.number);
    });
}

#[test]
fn html_and_css_tokens_take_their_semantic_slot() {
    for_each_builtin_theme(|theme| {
        let html = highlight(Some("html"), "<div class=\"a\">hi</div>\n", theme);
        assert_eq!(style_of(&html, "div"), theme.code.keyword);
        assert_eq!(style_of(&html, "class"), theme.code.attribute);

        let css = highlight(Some("css"), "a { color: red; }\n", theme);
        assert_eq!(style_of(&css, "a"), theme.code.keyword);
        assert_eq!(style_of(&css, "color"), theme.code.type_name);
    });
}

#[test]
fn go_java_ruby_sql_are_highlighted_at_all() {
    for_each_builtin_theme(|theme| {
        for (lang, src, token) in [
            ("go", "func main() { var x int = 3 }\n", "func"),
            ("java", "public class A { }\n", "class"),
            ("rb", "def f\n  puts 'x'\nend\n", "def"),
            ("sql", "SELECT a FROM t;\n", "SELECT"),
            ("php", "<?php echo 1; ?>\n", "echo"),
        ] {
            let lines = highlight(Some(lang), src, theme);
            assert_ne!(
                style_of(&lines, token),
                theme.code.text,
                "{lang}: {token} was left unstyled"
            );
        }
    });
}

/// Every style the highlighter emits must come from the active theme's code slots.
///
/// This is the guard against a `syntect` theme creeping back in: a colour from
/// anywhere else would fail here for at least one of the two built-in themes.
#[test]
fn no_colour_ever_comes_from_outside_the_theme() {
    for_each_builtin_theme(|theme| {
        let c = theme.code;
        let allowed = [
            c.text,
            c.keyword,
            c.string,
            c.number,
            c.comment,
            c.function,
            c.type_name,
            c.variable,
            c.constant,
            c.operator,
            c.attribute,
            c.invalid,
        ];
        let src = "\
#[derive(Debug)]
pub struct S<'a> { field: &'a str }
impl S<'_> {
    /// doc
    pub fn go(&self) -> Result<(), Box<dyn std::error::Error>> {
        let v = vec![1_u8, 0xff, 2.5e3];
        println!(\"{v:?} \\t {}\", true);
        Ok(())
    }
}
";
        for span in highlight(Some("rust"), src, theme)
            .iter()
            .flat_map(|l| &l.spans)
        {
            assert!(
                allowed.contains(&span.style),
                "{}: span {:?} carries a style that is not a theme code slot: {:?}",
                theme.name,
                span.text,
                span.style
            );
        }
    });
}

#[test]
fn the_two_themes_really_do_produce_different_colours() {
    let src = "fn main() { let s = \"x\"; }\n";
    let dark = highlight(Some("rust"), src, &Theme::default_dark());
    let light = highlight(Some("rust"), src, &Theme::default_light());
    assert_eq!(dark.len(), light.len());
    assert_eq!(dark[0].text(), light[0].text());
    assert_ne!(style_of(&dark, "let"), style_of(&light, "let"));
}

#[test]
fn aliases_resolve_to_the_same_syntax_as_their_canonical_tag() {
    for (alias, canonical) in [
        ("rs", "rust"),
        ("py", "python"),
        ("py3", "python"),
        ("python3", "python"),
        ("js", "javascript"),
        ("mjs", "javascript"),
        ("jsx", "javascript"),
        ("ts", "javascript"),
        ("tsx", "javascript"),
        ("sh", "bash"),
        ("zsh", "bash"),
        ("shell", "bash"),
        ("console", "bash"),
        ("yml", "yaml"),
        ("markdown", "md"),
        ("golang", "go"),
        ("htm", "html"),
        ("cpp", "c++"),
        ("hpp", "c++"),
        ("jsonc", "json"),
        ("text", "txt"),
        ("plaintext", "txt"),
    ] {
        let resolved = syntax_name(Some(alias));
        assert!(resolved.is_some(), "alias {alias} resolves to nothing");
        assert_eq!(
            resolved,
            syntax_name(Some(canonical)),
            "alias {alias} disagrees with {canonical}"
        );
    }
}

#[test]
fn every_tag_the_spec_names_is_highlighted() {
    for tag in [
        "rs",
        "rust",
        "py",
        "python",
        "js",
        "ts",
        "jsx",
        "tsx",
        "sh",
        "bash",
        "zsh",
        "yml",
        "yaml",
        "md",
        "toml",
        "json",
        "c",
        "cpp",
        "h",
        "hpp",
        "go",
        "java",
        "rb",
        "php",
        "sql",
        "html",
        "css",
        "dockerfile",
        "makefile",
        "diff",
        "xml",
        "lua",
        "perl",
        "haskell",
        "scala",
        "clojure",
        "r",
        "tex",
    ] {
        assert!(
            syntax_name(Some(tag)).is_some(),
            "language tag {tag} would fall back to plain text"
        );
    }
}
