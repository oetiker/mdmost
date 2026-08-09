//! Per-language colouring: does a token that *is* a keyword get the theme's keyword
//! style, in both built-in themes?

use mdmost::highlight::{highlight, syntax_name};
use mdmost::text::Line;
use mdmost::theme::{Style, Theme};

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
        assert_eq!(style_of(&lines, ";"), c.punctuation);
    });
}

#[test]
fn rust_string_escapes_break_out_of_the_string_colour() {
    for_each_builtin_theme(|theme| {
        let lines = highlight(Some("rust"), "let s = \"a\\nb\";\n", theme);
        assert_eq!(style_of(&lines, "\\n"), theme.code.escape);
        assert_ne!(theme.code.escape, theme.code.string);
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
fn rust_macros_are_not_confused_with_functions() {
    for_each_builtin_theme(|theme| {
        let lines = highlight(Some("rust"), "fn go() { println!(\"x\"); }\n", theme);
        assert_eq!(style_of(&lines, "println!"), theme.code.macro_name);
        assert_eq!(style_of(&lines, "go"), theme.code.function);
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
fn toml_covers_sections_keys_values_dates_and_arrays() {
    for_each_builtin_theme(|theme| {
        let src = "\
# a note
[server.http]
name = \"api\"
literal = 'raw\\nnot-an-escape'
port = 8080
ratio = 1.5e3
mask = 0xdead_beef
bits = 0b1010
enabled = true
started = 1979-05-27T07:32:00Z
day = 1979-05-27
ports = [80, 443]
inline = { a = 1 }

[[handlers]]
\"quoted key\" = 1
";
        let lines = highlight(Some("toml"), src, theme);
        let c = &theme.code;
        assert_eq!(style_of(&lines, "# a note"), c.comment);
        // A table header names a container, so it takes the namespace slot.
        assert_eq!(style_of(&lines, "server"), c.namespace);
        assert_eq!(style_of(&lines, "http"), c.namespace);
        assert_eq!(style_of(&lines, "handlers"), c.namespace);
        assert_eq!(style_of(&lines, "["), c.punctuation);
        assert_eq!(style_of(&lines, "[["), c.punctuation);
        assert_eq!(style_of(&lines, "name"), c.keyword);
        assert_eq!(style_of(&lines, "quoted key"), c.keyword);
        assert_eq!(style_of(&lines, "="), c.operator);
        assert_eq!(style_of(&lines, "\"api\""), c.string);
        assert_eq!(style_of(&lines, "'raw\\nnot-an-escape'"), c.string);
        assert_eq!(style_of(&lines, "8080"), c.number);
        assert_eq!(style_of(&lines, "1.5e3"), c.number);
        assert_eq!(style_of(&lines, "0xdead_beef"), c.number);
        assert_eq!(style_of(&lines, "0b1010"), c.number);
        assert_eq!(style_of(&lines, "true"), c.constant);
        assert_eq!(style_of(&lines, "1979-05-27T07:32:00Z"), c.constant);
        assert_eq!(style_of(&lines, "1979-05-27"), c.constant);
        assert_eq!(style_of(&lines, "80"), c.number);
        assert_eq!(style_of(&lines, "443"), c.number);
        assert_eq!(style_of(&lines, ","), c.punctuation);
    });
}

#[test]
fn toml_multi_line_strings_and_escapes() {
    for_each_builtin_theme(|theme| {
        let lines = highlight(
            Some("toml"),
            "a = \"\"\"first\nsecond\"\"\"\nb = \"x\\ty\\u00e9\"\nc = \"bad\\q\"\n",
            theme,
        );
        assert_eq!(lines.len(), 4);
        // The block string keeps its colour across the line break; its second line is
        // one span because the closing delimiter shares the string style.
        assert_eq!(style_of(&lines, "second\"\"\""), theme.code.string);
        assert_eq!(style_of(&lines, "\\t"), theme.code.escape);
        assert_eq!(style_of(&lines, "\\u00e9"), theme.code.escape);
        assert_eq!(style_of(&lines, "\\q"), theme.code.invalid);
    });
}

#[test]
fn dockerfile_directives_are_keywords_not_commands() {
    for_each_builtin_theme(|theme| {
        let src = "# base\nFROM alpine:3 AS build\nRUN apk add --no-cache curl\nEXPOSE 8080\nENV P=\"$HOME\"\n";
        let lines = highlight(Some("dockerfile"), src, theme);
        let c = &theme.code;
        assert_eq!(style_of(&lines, "# base"), c.comment);
        assert_eq!(style_of(&lines, "FROM"), c.keyword);
        assert_eq!(style_of(&lines, "RUN"), c.keyword);
        assert_eq!(style_of(&lines, "EXPOSE"), c.keyword);
        assert_eq!(style_of(&lines, "AS"), c.operator);
        assert_eq!(style_of(&lines, "--no-cache"), c.variable);
        assert_eq!(style_of(&lines, "8080"), c.number);
        assert_eq!(style_of(&lines, "\"$HOME\""), c.string);
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

/// A sample of each language the bundled set is expected to cover, as a Markdown author
/// would tag it: `(fence tag, the syntax it must resolve to, a sample)`.
///
/// The sample is deliberately a few lines of ordinary code rather than a single keyword,
/// because the question these cases answer is not "is the definition present" but "does
/// it actually parse this and give the tokens different meanings".
const MODERN_LANGUAGES: &[(&str, &str, &str)] = &[
    (
        "toml",
        "TOML",
        "# note\n[server]\nport = 8080\nname = \"api\"\n",
    ),
    (
        "typescript",
        "TypeScript",
        "interface P { x: number }\nconst f = (p: P): string => `${p.x}`;\n",
    ),
    (
        "tsx",
        "TypeScriptReact",
        "const A = (): JSX.Element => <div className=\"a\">{1}</div>;\n",
    ),
    (
        "dockerfile",
        "Dockerfile",
        "# base\nFROM alpine:3 AS build\nRUN apk add curl\n",
    ),
    (
        "nix",
        "Nix",
        "{ pkgs, ... }:\nlet x = 1; in pkgs.mkShell { name = \"s\"; }\n",
    ),
    (
        "kotlin",
        "Kotlin",
        "fun main() {\n    val n: Int = 42 // note\n    println(\"n=$n\")\n}\n",
    ),
    (
        "swift",
        "Swift",
        "struct P { let x: Int }\nfunc f(_ p: P) -> String { return \"\\(p.x)\" }\n",
    ),
    (
        "zig",
        "Zig",
        "const std = @import(\"std\");\npub fn main() void {\n    var n: u32 = 1;\n}\n",
    ),
    (
        "terraform",
        "Terraform",
        "resource \"aws_s3_bucket\" \"b\" {\n  bucket = \"name\"\n  count  = 2\n}\n",
    ),
    (
        "elixir",
        "Elixir",
        "defmodule M do\n  # note\n  def f(x), do: {:ok, x + 1}\nend\n",
    ),
    (
        "dart",
        "Dart",
        "class P {\n  final int x;\n  P(this.x);\n}\nvoid main() => print('hi');\n",
    ),
    (
        "julia",
        "Julia",
        "function f(x::Int)\n    # note\n    return x + 1\nend\n",
    ),
    (
        "proto",
        "Protocol Buffer",
        "syntax = \"proto3\";\nmessage P {\n  string name = 1;\n}\n",
    ),
    (
        "graphql",
        "GraphQL",
        "type Query {\n  user(id: ID!): User\n}\n",
    ),
    (
        "vue",
        "Vue Component",
        "<template>\n  <div class=\"a\">{{ n }}</div>\n</template>\n",
    ),
    (
        "svelte",
        "Svelte",
        "<script>\n  let n = 1;\n</script>\n<p>{n}</p>\n",
    ),
    ("scss", "SCSS", "$c: red;\n.a {\n  color: $c; // note\n}\n"),
    (
        "cmake",
        "CMake",
        "cmake_minimum_required(VERSION 3.20)\nproject(demo C)\n",
    ),
    (
        "asm",
        "x86_64 Assembly",
        "section .text\nglobal _start\n_start:\n    mov rax, 60\n",
    ),
    ("f#", "F#", "let f (x: int) = x + 1 // note\n"),
    (
        "solidity",
        "Solidity",
        "contract C {\n    uint256 public n = 1;\n}\n",
    ),
    (
        "groovy",
        "Groovy",
        "plugins { id 'java' }\ndef n = 1 // note\n",
    ),
    ("jq", "JQ", ".items[] | select(.n > 1) | .name\n"),
    ("nim", "Nim", "proc f(x: int): int =\n  result = x + 1\n"),
    ("lua", "Lua", "local function f(x)\n  return x + 1\nend\n"),
    ("ini", "INI", "; note\n[section]\nkey = value\n"),
    (
        "nginx",
        "nginx",
        "server {\n    listen 80;\n    root /srv;\n}\n",
    ),
];

/// The number of distinct styles the highlighter used across a whole block.
fn distinct_styles(lines: &[Line]) -> usize {
    let mut seen: Vec<Style> = Vec::new();
    for span in lines.iter().flat_map(|line| &line.spans) {
        if !seen.contains(&span.style) {
            seen.push(span.style);
        }
    }
    seen.len()
}

/// Every tag in [`MODERN_LANGUAGES`] must reach the syntax a Markdown author means by it.
///
/// Resolving to *something* is not enough: before the bundled set was widened, `ts` and
/// `typescript` resolved to `JavaScript`, which parses most TypeScript but silently
/// mis-colours the half that is types.
#[test]
fn modern_language_tags_reach_the_syntax_they_name() {
    for (tag, want, _) in MODERN_LANGUAGES {
        assert_eq!(
            syntax_name(Some(tag)),
            Some(*want),
            "fence tag `{tag}` does not resolve to {want}"
        );
    }
}

/// …and must really be parsed, not merely matched by name.
///
/// Three styles is the bar: a definition that loads but understands nothing produces one
/// (all text), and one that only finds comments or only finds strings produces two.
#[test]
fn modern_languages_are_really_highlighted_in_both_themes() {
    for_each_builtin_theme(|theme| {
        for (tag, _, src) in MODERN_LANGUAGES {
            let lines = highlight(Some(tag), src, theme);
            let styles = distinct_styles(&lines);
            assert!(
                styles >= 3,
                "{}: `{tag}` produced only {styles} distinct style(s) — it is not really \
                 being highlighted",
                theme.name
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
            c.macro_name,
            c.punctuation,
            c.namespace,
            c.escape,
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
        ("csharp", "cs"),
        ("fsharp", "f#"),
        ("objc", "objective-c"),
        ("graphviz", "dot"),
        ("fortran", "f90"),
        ("scheme", "scm"),
        ("kt", "kotlin"),
        ("tf", "terraform"),
        ("hcl", "terraform"),
        ("ex", "elixir"),
        ("jl", "julia"),
        ("gql", "graphql"),
        ("protobuf", "proto"),
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
