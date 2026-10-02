// SPDX-License-Identifier: MIT
//! An edge label keeps a cell of air between its text and every other edge's line.
//!
//! A label sits beside its own line. When the line of another edge entering the same
//! rank runs through the label's columns, the label either overwrites that line or
//! the line overwrites the label; when it runs in the column just past the label, the
//! text and the line read as one (`│headers│`). Both are checked on the rendered
//! canvas, under both fit policies, without knowing where a label wraps: every label
//! word must be drawn whole, and no run of text may have line art directly on both
//! sides, since only one of those can be the label's own line.

use mdmost::mermaid::{Fit, render_mermaid_with};
use mdmost::theme::Theme;

/// True for any box-drawing glyph, which is what every edge line is drawn with.
fn is_line(ch: char) -> bool {
    ('\u{2500}'..='\u{257F}').contains(&ch)
}

/// True when `ch` has an arm reaching down to the cell below.
fn reaches_down(ch: char) -> bool {
    "│┃┊┋╎╏║╭┌┏╔╮┐┓╗├┣╠┤┫╣┬┳╦┼╋╬".contains(ch)
}

/// True when `ch` has an arm reaching up to the cell above, or is a downward arrow.
fn reaches_up(ch: char) -> bool {
    "│┃┊┋╎╏║╰└┗╚╯┘┛╝├┣╠┤┫╣┴┻╩┼╋╬▼".contains(ch)
}

/// The `(row, column)` cells where a line running down the page stops dead because
/// something other than line art was drawn over the cell below.
fn broken_lines(text: &str) -> Vec<(usize, usize)> {
    let rows: Vec<Vec<char>> = text.lines().map(|line| line.chars().collect()).collect();
    let mut out = Vec::new();
    for (row, pair) in rows.windows(2).enumerate() {
        for (col, &ch) in pair[0].iter().enumerate() {
            if reaches_down(ch) && !pair[1].get(col).copied().is_some_and(reaches_up) {
                out.push((row, col));
            }
        }
    }
    out
}

/// The text runs of `line` that have line art directly before and after them.
///
/// Box text is padded with a blank on either side and so never shows up here; an edge
/// label is drawn against its own line, so it does when a second line touches it.
fn pinched(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut start = None;
    for (at, &ch) in chars.iter().enumerate() {
        if is_line(ch) {
            if let Some(from) = start.take() {
                let run: String = chars[from..at].iter().collect();
                if !run.starts_with(' ') && !run.ends_with(' ') {
                    out.push(run);
                }
            }
            start = Some(at + 1);
        }
    }
    out.into_iter().filter(|run| !run.is_empty()).collect()
}

/// Asserts that every word of `words` is drawn and no label is pinched between two
/// lines, at every width in `widths` and under both fit policies.
#[track_caller]
fn assert_labels_have_air(src: &str, widths: &[u16], words: &[&str]) {
    let theme = Theme::default_dark();
    for &width in widths {
        for fit in [Fit::COMPACT, Fit::ROOMY] {
            let canvas = render_mermaid_with(src, width, &theme, fit).expect("diagram renders");
            canvas.check_invariants().expect("canvas contract holds");
            let text = canvas.plain_text();
            for word in words {
                assert!(
                    text.contains(word),
                    "label word {word:?} is not drawn whole at width {width}:\n{text}"
                );
            }
            let broken = broken_lines(&text);
            assert!(
                broken.is_empty(),
                "a line stops under a label at {broken:?}, width {width}:\n{text}"
            );
            for line in text.lines() {
                let runs = pinched(line);
                assert!(
                    runs.is_empty(),
                    "label text {runs:?} touches another edge's line at width {width}:\n{text}"
                );
            }
        }
    }
}

#[test]
fn a_label_does_not_overwrite_the_line_of_an_edge_into_the_same_node() {
    let src = r#"flowchart TB
    api["REST API (zmcfg)<br/>create, update, invalidate ID"]
    web["Web UI<br/>Servicecenter successor<br/>mailbox settings,<br/>SecureMail Global, delegates ?"]
    zm["Zimbra management<br/>accounts, aliases,<br/>domain lookup, mailbox settings"]
    ld["LDAP management<br/>user entries, mail password,<br/>SecureMail flags, delegates"]
    api -->|"mailbox create<br/>and update"| zm
    api -->|"write user entry<br/>(last step)"| ld
    web -->|"mailbox settings"| zm
    web -->|"SecureMail flags,<br/>delegates ?"| ld
"#;
    assert_labels_have_air(
        src,
        &[80, 120, 200],
        &[
            "mailbox",
            "create",
            "update",
            "write",
            "user",
            "entry",
            "(last",
            "step)",
            "settings",
            "SecureMail",
            "delegates",
        ],
    );
}

#[test]
fn a_label_does_not_touch_the_line_of_a_neighbouring_edge() {
    let src = r#"flowchart TB
    smtp["smtp-proxy"]
    nginx["nginx mail proxy"]
    authapi["mail auth API"]
    smtp -->|"token, sender,<br/>recipients, headers"| authapi
    nginx -->|"user, password"| authapi
"#;
    assert_labels_have_air(
        src,
        &[80, 120, 200],
        &[
            "token,",
            "sender,",
            "recipients,",
            "headers",
            "user,",
            "password",
        ],
    );
}
