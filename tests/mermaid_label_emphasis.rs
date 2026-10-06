// SPDX-License-Identifier: MIT
//! Markdown strings and HTML emphasis in Mermaid labels.

use mdmost::canvas::Canvas;
use mdmost::mermaid::render_mermaid;
use mdmost::theme::{Attributes, Theme};

fn draw(src: &str) -> Canvas {
    render_mermaid(src, 100, &Theme::default_dark()).expect("diagram draws")
}

/// The attributes of every cell that draws the first occurrence of `word`.
fn attrs_of(canvas: &Canvas, word: &str) -> Vec<Attributes> {
    let wanted: Vec<char> = word.chars().collect();
    for row in canvas.rows() {
        let chars: Vec<String> = row.iter().map(|cell| cell.text().to_string()).collect();
        for start in 0..chars.len() {
            let hit = wanted
                .iter()
                .enumerate()
                .all(|(i, ch)| chars.get(start + i).map(String::as_str) == Some(&ch.to_string()));
            if hit {
                return row[start..start + wanted.len()]
                    .iter()
                    .map(|cell| cell.style().attrs)
                    .collect();
            }
        }
    }
    panic!("`{word}` not drawn:\n{}", canvas.plain_text());
}

fn all(attrs: &[Attributes], which: Attributes) -> bool {
    attrs.iter().all(|attrs| attrs.contains(which))
}

fn none(attrs: &[Attributes], which: Attributes) -> bool {
    attrs.iter().all(|attrs| !attrs.contains(which))
}

#[test]
fn a_markdown_string_draws_bold_and_italic_without_its_markup() {
    let canvas = draw("flowchart LR\n    a[\"`**heavy** and *slanted*`\"] --> b\n");
    let text = canvas.plain_text();
    assert!(text.contains("heavy and slanted"), "{text}");
    assert!(!text.contains('`') && !text.contains('*'), "{text}");
    assert!(all(&attrs_of(&canvas, "heavy"), Attributes::BOLD));
    assert!(all(&attrs_of(&canvas, "slanted"), Attributes::ITALIC));
    assert!(none(&attrs_of(&canvas, "and"), Attributes::BOLD));
}

#[test]
fn a_markdown_string_may_span_lines() {
    let canvas = draw("flowchart LR\n    a[\"`first line\n    second _line_`\"] --> b\n");
    let text = canvas.plain_text();
    assert!(text.contains("first line"), "{text}");
    assert!(text.contains("second line"), "{text}");
    let line = attrs_of(&canvas, "second line");
    assert!(none(&line[..6], Attributes::ITALIC));
    assert!(all(&line[7..], Attributes::ITALIC));
}

#[test]
fn html_emphasis_tags_are_drawn_not_shown() {
    let canvas =
        draw("flowchart LR\n    a[\"<b>strong</b> <i>tilted</i> <em>stressed</em>\"] --> b\n");
    let text = canvas.plain_text();
    assert!(!text.contains('<'), "{text}");
    assert!(all(&attrs_of(&canvas, "strong"), Attributes::BOLD));
    assert!(all(&attrs_of(&canvas, "tilted"), Attributes::ITALIC));
    assert!(all(&attrs_of(&canvas, "stressed"), Attributes::ITALIC));
}

#[test]
fn edge_labels_and_subgraph_titles_carry_emphasis_too() {
    let canvas = draw(
        "flowchart LR\n    subgraph s[\"`**Boxed** in`\"]\n        c\n    end\n    a -- \"<i>sloped</i> words\" --> c\n",
    );
    assert!(all(&attrs_of(&canvas, "Boxed"), Attributes::BOLD));
    assert!(all(&attrs_of(&canvas, "sloped"), Attributes::ITALIC));
}

#[test]
fn a_styled_word_still_maps_to_its_own_source_bytes() {
    let src = "flowchart LR\n    a[\"`**heavy** text`\"]\n";
    let canvas = draw(src);
    let heavy = src.find("heavy").unwrap();
    assert!(
        canvas
            .spans()
            .iter()
            .any(|span| (span.source_start, span.source_end) == (heavy, heavy + 5)),
        "no span names the source bytes of `heavy`: {:?}",
        canvas.spans()
    );
}

#[test]
fn a_plain_label_with_stars_is_left_alone() {
    let canvas = draw("flowchart LR\n    a[\"2 * 3 and *not markdown*\"]\n");
    assert!(canvas.plain_text().contains("2 * 3 and *not markdown*"));
}
