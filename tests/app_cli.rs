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
";

/// Runs the binary with the given arguments and no standard input.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mdless"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("the binary should be runnable")
}

/// Runs the binary with `input` on standard input.
fn run_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdless"))
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
    let path = std::env::temp_dir().join(format!("mdless-{name}-{}.md", std::process::id()));
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
        assert!(text.contains("Title"), "the heading should appear");
        for line in text.lines() {
            assert!(
                mdless::text::display_width(line) <= usize::from(width),
                "line wider than {width}: {line:?}"
            );
        }
    }
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
    assert!(text.contains("Title"));
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
    let config = std::env::temp_dir().join(format!("mdless-bad-{}.toml", std::process::id()));
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
    let with = run_with_stdin(&["--render-once", "--width", "60"], SAMPLE);
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
    let config = std::env::temp_dir().join(format!("mdless-ln-{}.toml", std::process::id()));

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
