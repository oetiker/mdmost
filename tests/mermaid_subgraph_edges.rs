// SPDX-License-Identifier: MIT
//! Edges that cross a subgraph border must land on the node inside, not on the frame.

use mdmost::mermaid::render_mermaid;
use mdmost::theme::Theme;

/// The drawing as a grid of characters, one row per line.
fn grid(src: &str, width: u16) -> Vec<Vec<char>> {
    let canvas = render_mermaid(src, width, &Theme::default_dark()).expect("diagram draws");
    canvas
        .plain_text()
        .lines()
        .map(|line| line.chars().collect())
        .collect()
}

/// Asserts every downward arrowhead sits on the solid top border of a node.
fn every_arrow_meets_a_node(src: &str, width: u16) {
    let rows = grid(src, width);
    let drawing: String = rows
        .iter()
        .map(|row| row.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    let mut arrows = 0;
    for (row, cells) in rows.iter().enumerate() {
        for (col, &ch) in cells.iter().enumerate() {
            if ch != '▼' {
                continue;
            }
            arrows += 1;
            let below = rows.get(row + 1).and_then(|next| next.get(col)).copied();
            assert!(
                matches!(below, Some('─' | '┬')),
                "arrow at row {row}, column {col} ends on {below:?}, not on a node:\n{drawing}"
            );
        }
    }
    assert!(arrows > 0, "no arrows drawn:\n{drawing}");
}

#[test]
fn an_edge_under_the_subgraph_title_reaches_its_node() {
    every_arrow_meets_a_node(
        r#"flowchart TB
    a["outside A"]
    b["outside B"]
    subgraph box["container"]
        x["inner X"]
        y["inner Y"]
    end
    z["outside Z"]
    a --> x
    b --> y
    y --> z
"#,
        120,
    );
}

#[test]
fn a_fan_of_edges_into_a_subgraph_reaches_each_node() {
    every_arrow_meets_a_node(
        r#"flowchart TB
    p["first caller"]
    q["second caller"]
    r["third caller"]
    s["fourth caller"]
    subgraph svc["service"]
        direction TB
        m["the one on the left"]
        n["the one on the right"]
    end
    p --> m
    q --> m
    r --> m
    s --> n
"#,
        160,
    );
}
