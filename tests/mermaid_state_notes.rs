// SPDX-License-Identifier: MIT
//! State diagram notes sit beside their state, on the side the source names.

use mdmost::canvas::Canvas;
use mdmost::mermaid::render_mermaid;
use mdmost::theme::Theme;

fn draw(src: &str) -> Canvas {
    render_mermaid(src, 100, &Theme::default_dark()).expect("diagram draws")
}

/// The row and column of the first cell of `needle`.
#[track_caller]
fn locate(canvas: &Canvas, needle: &str) -> (usize, usize) {
    for row in 0..canvas.height() {
        let text = canvas.row_text(row);
        if let Some(at) = text.find(needle) {
            return (row, text[..at].chars().count());
        }
    }
    panic!("`{needle}` not drawn:\n{}", canvas.plain_text());
}

#[test]
fn a_note_sits_on_the_named_side_of_its_state() {
    for (direction, side) in [
        ("TB", "right"),
        ("TB", "left"),
        ("BT", "right"),
        ("BT", "left"),
    ] {
        let canvas = draw(&format!(
            "stateDiagram-v2\n  direction {direction}\n  [*] --> Idle\n  Idle --> Busy\n\
             note {side} of Idle : the note\n"
        ));
        let text = canvas.plain_text();
        let (state_row, state_col) = locate(&canvas, "Idle");
        let (note_row, note_col) = locate(&canvas, "the note");
        assert_eq!(note_row, state_row, "{direction} {side}: same row\n{text}");
        if side == "right" {
            assert!(note_col > state_col, "{direction} right of\n{text}");
        } else {
            assert!(note_col < state_col, "{direction} left of\n{text}");
        }
        let row = canvas.row_text(state_row);
        assert!(row.contains('┄'), "{direction} {side}: dotted tie\n{text}");
    }
}

#[test]
fn a_note_in_a_sideways_diagram_sits_on_the_named_side() {
    for (direction, side) in [
        ("LR", "right"),
        ("LR", "left"),
        ("RL", "right"),
        ("RL", "left"),
    ] {
        let canvas = draw(&format!(
            "stateDiagram-v2\n  direction {direction}\n  [*] --> Idle\n  Idle --> Busy\n\
             note {side} of Idle : the note\n"
        ));
        let text = canvas.plain_text();
        let (_, state_col) = locate(&canvas, "Idle");
        let (_, note_col) = locate(&canvas, "the note");
        if side == "right" {
            assert!(note_col > state_col, "{direction} right of\n{text}");
        } else {
            assert!(note_col < state_col, "{direction} left of\n{text}");
        }
    }
}

/// How many edges meet the left side of the box in column `col`: a tee in the border,
/// or an arrowhead just before it.
fn entries(canvas: &Canvas, col: usize) -> usize {
    (0..canvas.height())
        .filter(|&row| {
            let text: Vec<char> = canvas.row_text(row).chars().collect();
            text.get(col) == Some(&'┤')
                || (text.get(col) == Some(&'│') && text.get(col - 1) == Some(&'▶'))
        })
        .count()
}

#[test]
fn a_sideways_note_tie_keeps_its_own_entry_beside_an_arrow() {
    let canvas = draw(
        "stateDiagram-v2\n  direction LR\n  [*] --> Idle\n  Idle --> Busy\n  note left of Idle : left note\n",
    );
    let text = canvas.plain_text();
    let (_, col) = locate(&canvas, "Idle");
    let border = col - 2;
    assert_eq!(entries(&canvas, border), 2, "two entries\n{text}");
    assert!(text.contains('┄'), "the tie stays dotted\n{text}");
}

#[test]
fn a_flowchart_box_grows_to_keep_different_ends_apart() {
    let canvas =
        draw("flowchart LR\n  S((s)) --> Idle\n  N[left note] -.- Idle\n  Idle --> Busy\n");
    let text = canvas.plain_text();
    let (_, col) = locate(&canvas, "Idle");
    let border = col - 2;
    assert_eq!(entries(&canvas, border), 2, "two entries\n{text}");
    assert!(text.contains('┄'), "the dotted edge stays dotted\n{text}");
}
