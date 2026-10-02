// SPDX-License-Identifier: MIT
//! Two edges never share a stretch of line.
//!
//! When one edge ends in the column where another starts, and each runs sideways in
//! the gap, the two can draw over each other's vertical run, and the reader can no
//! longer tell which line goes where. In a top-to-bottom chart of plain boxes, with
//! every edge on a port of its own, a side junction (`├`, `┤`) has exactly one cause:
//! a line joining another line. So none may appear.

use mdmost::mermaid::{Fit, render_mermaid_with};
use mdmost::theme::Theme;

/// Asserts that no side junction is drawn at any width in `widths`, under both fit
/// policies.
#[track_caller]
fn assert_no_shared_line(src: &str, widths: &[u16]) {
    let theme = Theme::default_dark();
    for &width in widths {
        for fit in [Fit::COMPACT, Fit::ROOMY] {
            let canvas = render_mermaid_with(src, width, &theme, fit).expect("diagram renders");
            canvas.check_invariants().expect("canvas contract holds");
            let text = canvas.plain_text();
            assert!(
                !text.contains(['├', '┤']),
                "two edges share a line at width {width}:\n{text}"
            );
        }
    }
}

/// REST API to LDAP management and Web UI to Zimbra management cross. With both fans
/// spread evenly, each edge ended in the column where the other one started, and the
/// pair was drawn as one loop.
#[test]
fn crossing_edges_that_swap_columns_stay_apart() {
    let src = r#"flowchart TB
    idm["IDM Hub"]
    airlock["Airlock<br/>replaces the Nevis proxy"]
    subgraph mbox["hin-mbox-mgr"]
        zmcfgapi["REST API (zmcfg)<br/>create, update, invalidate ID"]
        webui["Web UI<br/>Servicecenter successor<br/>mailbox settings,<br/>SecureMail Global, delegates ?"]
        zmmgmt["Zimbra management<br/>accounts, aliases,<br/>domain lookup, mailbox settings"]
        ldapmgmt["LDAP management<br/>user entries, mail password,<br/>SecureMail flags, delegates"]
    end
    idm --> zmcfgapi
    airlock --> webui
    zmcfgapi -->|"mailbox create<br/>and update"| zmmgmt
    zmcfgapi -->|"write user entry<br/>(last step)"| ldapmgmt
    webui -->|"mailbox settings"| zmmgmt
    webui -->|"SecureMail flags,<br/>delegates ?"| ldapmgmt
"#;
    assert_no_shared_line(src, &[60, 70, 90, 100, 120]);
}

/// The same crossing without a subgraph around it.
#[test]
fn crossing_edges_between_plain_boxes_stay_apart() {
    let src = r#"flowchart TB
    rest["REST API (zmcfg)<br/>create, update, invalidate ID"]
    web["Web UI<br/>Servicecenter successor<br/>mailbox settings,<br/>SecureMail Global, delegates ?"]
    zm["Zimbra management<br/>accounts, aliases,<br/>domain lookup, mailbox settings"]
    ldm["LDAP management<br/>user entries, mail password,<br/>SecureMail flags, delegates"]
    rest --> zm
    rest --> ldm
    web --> zm
    web --> ldm
"#;
    assert_no_shared_line(src, &[50, 60, 70, 80, 90, 100, 120]);
}
