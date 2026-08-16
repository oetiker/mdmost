// SPDX-License-Identifier: MIT
//! Integration tests for the command-line interface.
//!
//! These drive the real binary, headlessly, exactly as the QA and snapshot workflows
//! do. Because the tests run with pipes on both ends, standard output is never a
//! terminal, which is precisely the path design spec §11 requires to produce plain
//! text rather than escape sequences.

use std::io::Write;
use std::process::{Command, Output, Stdio};

/// The document every test renders unless it says otherwise.
const SAMPLE: &str = "\
# Title

Some prose that is long enough to be worth wrapping at a narrow width indeed.

## Section

- one
- two
- [ ] three

```rust
fn main() {}
```
";

/// The binary, with anything in the environment that could change its output removed.
///
/// `MDMOST_ICONS` outranks the config file, so a developer who exports it in their shell
/// profile would otherwise be running a different program from everyone else — and the
/// failure would look like a real regression rather than a local setting.
fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mdmost"));
    command.env_remove("MDMOST_ICONS");
    command
}

/// Runs the binary with the given arguments and no standard input.
fn run(args: &[&str]) -> Output {
    command()
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("the binary should be runnable")
}

/// Runs the binary with `input` on standard input.
fn run_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = command()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should be runnable");
    child
        .stdin
        .as_mut()
        .expect("stdin was piped")
        .write_all(input.as_bytes())
        .expect("writing to the child should succeed");
    child
        .wait_with_output()
        .expect("the child should terminate")
}

/// Writes `SAMPLE` to a temporary file and returns its path.
fn sample_file(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("mdmost-{name}-{}.md", std::process::id()));
    std::fs::write(&path, SAMPLE).expect("the temporary file should be writable");
    path
}

#[test]
fn render_once_works_headlessly_and_honours_the_width() {
    let path = sample_file("width");
    for width in [40u16, 80, 120] {
        let output = run(&[
            "--render-once",
            "--width",
            &width.to_string(),
            &path.display().to_string(),
        ]);
        assert!(output.status.success(), "exit status for width {width}");
        let text = String::from_utf8(output.stdout).expect("output should be UTF-8");
        // The FIGlet title banner is opt-in, so the default run draws the lone `#` as an
        // ordinary heading — the word itself, not art.
        assert!(text.contains("Section"), "the heading should appear");
        assert!(
            text.lines()
                .next()
                .is_some_and(|line| line.contains("Title")),
            "the title should be drawn as text at width {width}: {text}"
        );
        for line in text.lines() {
            assert!(
                mdmost::text::display_width(line) <= usize::from(width),
                "line wider than {width}: {line:?}"
            );
        }
    }
    let _ = std::fs::remove_file(path);
}

/// A document whose prose is far wider than any cap, and whose table is wider than the
/// terminal it will be rendered at.
const WIDE: &str = "\
# Wide

Prose that runs on and on and on, long enough that a cap of forty columns has something \
to bite on and long enough that no cap at all leaves it running the full width of a very \
wide terminal indeed, which is the whole complaint.

| Component | Responsibility | Input | Output | Notes |
| --- | --- | --- | --- | --- |
| `render` | tree to canvas at a width | `Doc`, width, theme | `Canvas` | a pure function, no I/O at all |
";

/// Writes `source` to a scratch file and returns its path.
fn file_with(name: &str, source: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("mdmost-{name}-{}.md", std::process::id()));
    std::fs::write(&path, source).expect("the temporary file should be writable");
    path
}

/// `--body-width` is not a pager-only setting: a dump is the same layout as a frame.
///
/// It was, once. `--render-once` called the uncapped renderer directly, so the flag was
/// accepted and then discarded, and `mdmost doc.md | cat` disagreed with the pager about
/// where the prose sat.
#[test]
fn render_once_honours_the_body_width_cap() {
    let path = file_with("bodycap", WIDE);
    let output = run(&[
        "--render-once",
        "--width",
        "120",
        "--body-width",
        "40",
        "--no-icons",
        &path.display().to_string(),
    ]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("output should be UTF-8");
    let prose: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("Prose that runs") || line.contains("complaint"))
        .collect();
    assert!(!prose.is_empty(), "the prose should be drawn: {text}");
    for line in &prose {
        assert!(
            mdmost::text::display_width(line.trim()) <= 40,
            "prose past the cap: {line:?}"
        );
    }
    let _ = std::fs::remove_file(path);
}

/// Content that cannot be reflowed is laid out at the width it needs, in a dump exactly
/// as in the pager: squeezing a table into the terminal wraps every cell, which is not a
/// smaller table but a mangled one.
#[test]
fn render_once_lays_wide_content_out_at_its_natural_width() {
    let path = file_with("widetable", WIDE);
    let output = run(&[
        "--render-once",
        "--width",
        "60",
        "--no-icons",
        &path.display().to_string(),
    ]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("output should be UTF-8");
    let table = text
        .lines()
        .find(|line| line.contains("Responsibility"))
        .expect("the table header should be drawn");
    assert!(
        mdmost::text::display_width(table) > 60,
        "the table should reach past the render width: {table:?}"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn render_once_is_deterministic() {
    let path = sample_file("determinism");
    let first = run(&[
        "--render-once",
        "--width",
        "80",
        &path.display().to_string(),
    ]);
    let second = run(&[
        "--render-once",
        "--width",
        "80",
        &path.display().to_string(),
    ]);
    assert_eq!(first.stdout, second.stdout);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_non_terminal_stdout_implies_render_once_and_emits_no_escapes() {
    // No `--render-once`: the implication alone must produce output and exit.
    let path = sample_file("implied");
    let output = run(&[&path.display().to_string()]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("output should be UTF-8");
    assert!(!text.is_empty());
    assert!(
        !text.contains('\u{1b}'),
        "piped output must be plain text, not escape soup"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn stdin_is_read_when_no_file_is_given() {
    let output = run_with_stdin(&["--render-once", "--width", "60"], SAMPLE);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("output should be UTF-8");
    assert!(text.contains("Section"));
}

#[test]
fn a_dash_also_means_standard_input() {
    let output = run_with_stdin(&["--render-once", "--width", "60", "-"], SAMPLE);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("output should be UTF-8");
    assert!(text.contains("Section"));
}

#[test]
fn an_unreadable_file_exits_with_one_and_says_why() {
    let output = run(&["/nonexistent/definitely/not/here.md"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "no partial output before the error"
    );
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("here.md"),
        "the message should name the file"
    );
}

#[test]
fn bad_arguments_exit_with_two() {
    let output = run(&["--no-such-flag"]);
    assert_eq!(output.status.code(), Some(2));
}

/// `--licenses` is how the third-party syntax notices reach whoever has the binary, so
/// it has to work with no document, no config and no terminal — the state an auditor is
/// in — and it has to come out as something `mdmost` itself can read back.
#[test]
fn licenses_prints_the_bundled_notices_and_exits_cleanly() {
    let output = run(&["--licenses"]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("Permission is hereby granted"),
        "no MIT notice"
    );
    assert!(text.contains("Copyright"), "no copyright notice");
    assert!(!text.contains("<details>"), "the listing must be HTML-free");

    // …and it really is Markdown mdmost can render.
    let rendered = run_with_stdin(&["--render-once", "--width", "80"], &text);
    assert_eq!(rendered.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&rendered.stdout).contains("Third-party syntax definitions"),
        "the licence listing did not survive being rendered"
    );
}

#[test]
fn a_zero_width_is_a_usage_error() {
    let output = run_with_stdin(&["--render-once", "--width", "0"], SAMPLE);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn an_unknown_theme_falls_back_instead_of_refusing_to_start() {
    let output = run_with_stdin(
        &["--render-once", "--width", "60", "--theme", "chartreuse"],
        SAMPLE,
    );
    assert!(
        output.status.success(),
        "an unknown theme must not be fatal"
    );
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("chartreuse"));
    assert!(!output.stdout.is_empty());
}

#[test]
fn a_broken_config_is_reported_and_defaults_are_used() {
    let config = std::env::temp_dir().join(format!("mdmost-bad-{}.toml", std::process::id()));
    std::fs::write(&config, "theme = \n").expect("the temporary file should be writable");
    let output = run_with_stdin(
        &[
            "--render-once",
            "--width",
            "60",
            "--config",
            &config.display().to_string(),
        ],
        SAMPLE,
    );
    assert!(output.status.success(), "a broken config must not be fatal");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("invalid config"), "got: {error}");
    assert!(!output.stdout.is_empty());
    let _ = std::fs::remove_file(config);
}

#[test]
fn no_icons_reaches_the_renderer_not_just_the_chrome() {
    // `--render-once` draws no chrome at all, so any difference here is proof the
    // flag reached the document renderer.
    //
    // `--icons` is passed explicitly rather than relied on as the default: glyphs are
    // detected now, and `--render-once` writes to a pipe, where detection deliberately
    // gives up and picks plain Unicode. Without the flag both sides would be plain and
    // this test would quietly compare nothing with nothing.
    //
    // `SAMPLE`'s fenced code block is what carries the difference. Bullets and task
    // boxes became ASCII in both glyph sets on 2026-08-09, so a code-fence language
    // icon is now the only thing the *document* renderer draws that this flag changes.
    let with = run_with_stdin(&["--render-once", "--width", "60", "--icons"], SAMPLE);
    let without = run_with_stdin(&["--render-once", "--width", "60", "--no-icons"], SAMPLE);
    assert!(with.status.success() && without.status.success());
    assert_ne!(
        with.stdout, without.stdout,
        "--no-icons must change the rendered document, not only the status bar"
    );

    // The fallback glyphs are the same display width, so the layout is untouched.
    let with = String::from_utf8(with.stdout).expect("output should be UTF-8");
    let without = String::from_utf8(without.stdout).expect("output should be UTF-8");
    assert_eq!(
        with.lines().count(),
        without.lines().count(),
        "the icon fallback must not reflow the document"
    );
}

#[test]
fn line_numbers_from_config_reach_the_renderer() {
    let source = "# Code\n\n```rust\nfn main() {}\nlet x = 1;\n```\n";
    let config = std::env::temp_dir().join(format!("mdmost-ln-{}.toml", std::process::id()));

    std::fs::write(&config, "line_numbers = false\n").expect("writable");
    let off = run_with_stdin(
        &[
            "--render-once",
            "--width",
            "60",
            "--config",
            &config.display().to_string(),
        ],
        source,
    );
    std::fs::write(&config, "line_numbers = true\n").expect("writable");
    let on = run_with_stdin(
        &[
            "--render-once",
            "--width",
            "60",
            "--config",
            &config.display().to_string(),
        ],
        source,
    );

    assert!(off.status.success() && on.status.success());
    assert_ne!(
        off.stdout, on.stdout,
        "`line_numbers` must reach the code renderer"
    );
    let _ = std::fs::remove_file(config);
}

#[test]
fn an_empty_document_renders_nothing_and_succeeds() {
    let output = run_with_stdin(&["--render-once", "--width", "40"], "");
    assert!(output.status.success());
}

#[test]
fn the_help_and_version_flags_succeed() {
    for flag in ["--help", "--version"] {
        let output = run(&[flag]);
        assert!(output.status.success(), "{flag} should exit 0");
        assert!(!output.stdout.is_empty(), "{flag} should print something");
    }
}
