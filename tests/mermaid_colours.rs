// SPDX-License-Identifier: MIT
//! Mermaid colour classes as drawn: border inks, interior tints, heavy outlines and
//! frame washes (colour spec §6). Uncoloured output is pinned by the snapshot corpus.

use mdmost::canvas::Canvas;
use mdmost::mermaid::render_mermaid;
use mdmost::theme::{Color, Theme};

fn draw(src: &str, theme: &Theme) -> Canvas {
    render_mermaid(src, 120, theme).expect("diagram draws")
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

fn glyph(canvas: &Canvas, row: usize, col: usize) -> String {
    canvas.row(row).expect("row")[col].text().to_string()
}

fn fg(canvas: &Canvas, row: usize, col: usize) -> Option<Color> {
    canvas.row(row).expect("row")[col].style().fg
}

fn bg(canvas: &Canvas, row: usize, col: usize) -> Option<Color> {
    canvas.row(row).expect("row")[col].style().bg
}

/// The top-left corner of the box whose label starts at `(row, col)`.
///
/// The corner rather than the side: an edge may enter a box on the label's row and turn
/// that side cell into a junction drawn in the line ink.
#[track_caller]
fn corner(canvas: &Canvas, row: usize, col: usize) -> (usize, usize) {
    for top in (0..row).rev() {
        if let Some(c) = (0..col)
            .rev()
            .find(|&c| matches!(glyph(canvas, top, c).as_str(), "┌" | "┏" | "╭"))
        {
            return (top, c);
        }
    }
    panic!("no corner above {row},{col}:\n{}", canvas.plain_text());
}

const EXAMPLE: &str = "flowchart LR\n  Airlock --> Zimbra --> Cfgapi --> Mbox\n\
    classDef access fill:#e3f4fb,stroke:#2a8bb5,color:#000\n\
    classDef comm fill:#fdf0e1,stroke:#d4831f,color:#000\n\
    classDef part fill:#fbd9a8,stroke:#b8650a,color:#000\n\
    class Airlock access\n  class Zimbra comm\n  class Cfgapi part\n\
    style Mbox fill:#fff7ee,stroke:#b8650a,stroke-width:3px,color:#000\n";

#[test]
fn a_painted_node_draws_its_slot_ink_and_full_tint() {
    let theme = Theme::default_dark();
    let canvas = draw(EXAMPLE, &theme);
    let page = Some(theme.palette.bg);
    for (label, slot) in [("Airlock", 9), ("Zimbra", 2), ("Cfgapi", 4), ("Mbox", 4)] {
        let (row, col) = locate(&canvas, label);
        let (top, left) = corner(&canvas, row, col);
        let ink = theme.diagram_slots[slot];
        assert_eq!(fg(&canvas, top, left), ink.ink, "{label} border");
        assert_eq!(
            bg(&canvas, top, left),
            page,
            "{label} border keeps the page"
        );
        assert_eq!(bg(&canvas, row, col), Some(ink.full_tint), "{label} text");
        assert_eq!(
            bg(&canvas, row, col - 1),
            Some(ink.full_tint),
            "{label} padding"
        );
        assert_eq!(
            fg(&canvas, row, col),
            theme.diagram.node_text.fg,
            "{label} text ink"
        );
    }
    let (row, col) = locate(&canvas, "Mbox");
    let (top, left) = corner(&canvas, row, col);
    assert_eq!(glyph(&canvas, top, left), "┏", "stroke-width 3px");
    assert_eq!(glyph(&canvas, top, left + 1), "━");
}

/// Review Focus 5.
#[test]
fn the_light_theme_draws_repaired_inks() {
    let theme = Theme::default_light();
    let canvas = draw(EXAMPLE, &theme);
    let (row, col) = locate(&canvas, "Zimbra");
    let (top, left) = corner(&canvas, row, col);
    assert_eq!(fg(&canvas, top, left), Some(Color::hex(0xa55806)));
}

#[test]
fn every_shape_has_its_heavy_form() {
    let theme = Theme::default_dark();
    let src = "flowchart TB\n  R[rect] --- O(round) --- S([stad]) --- C((circ))\n\
        H{rhomb} --- U[[sub]] --- Y[(cyl)]\n  classDef k stroke:#ff0000,stroke-width:3px\n\
        class R,O,S,C,H,U,Y k\n";
    let text = draw(src, &theme).plain_text();
    // Geometry: a subroutine row is `┃ │ sub │ ┃` under `┏━┯━━━━━┯━┓`; a circle row is
    // `((  circ  ))`.
    for piece in [
        "┏━",
        "╭━",
        "( stad",
        "((",
        "))",
        "╱━",
        "┃ rhomb",
        "┏━┯",
        "┃ │ sub",
        "┠─",
        "╰━",
    ] {
        assert!(text.contains(piece), "missing `{piece}` in\n{text}");
    }
}

/// Spec §8: a light edge on a heavy border draws a mixed junction.
#[test]
fn a_light_edge_meets_a_heavy_border_mixed() {
    let theme = Theme::default_dark();
    // `---`, not `-->`: an arrowhead sits on the border cell above it and leaves `━`.
    let src = "flowchart TB\n  A --- B\n  style A stroke:#ff0000,stroke-width:3px\n  style B stroke:#ff0000,stroke-width:3px\n";
    let text = draw(src, &theme).plain_text();
    assert!(text.contains('┯') && text.contains('┷'), "{text}");
}

/// Review Focus 1: paint changes inks and weights, never where anything is.
#[test]
fn paint_never_changes_layout_or_spans() {
    let theme = Theme::default_dark();
    let plain = "flowchart LR\n  Airlock --> Zimbra --> Cfgapi --> Mbox\n";
    let light = |text: String| {
        text.chars()
            .map(|ch| match ch {
                '━' => '─',
                '┃' => '│',
                '┏' => '┌',
                '┓' => '┐',
                '┗' => '└',
                '┛' => '┘',
                '┯' => '┬',
                '┷' => '┴',
                '┠' => '├',
                '┨' => '┤',
                other => other,
            })
            .collect::<String>()
    };
    let mut compared = 0;
    for width in [60u16, 80, 120, 200] {
        let (Ok(a), Ok(b)) = (
            render_mermaid(plain, width, &theme),
            render_mermaid(EXAMPLE, width, &theme),
        ) else {
            continue;
        };
        assert_eq!(
            light(a.plain_text()),
            light(b.plain_text()),
            "width {width}"
        );
        assert_eq!(a.spans(), b.spans(), "width {width}");
        compared += 1;
    }
    // A width that draws neither is skipped; several must remain, or the loop proves
    // nothing about how the layout degrades.
    assert!(compared >= 3, "only {compared} widths drew");
}

/// Lines that reach no node draw nothing different (rulings 12 and 15).
#[test]
fn colour_lines_that_reach_no_node_change_nothing() {
    let theme = Theme::default_dark();
    let plain = draw("flowchart LR\n  A --> B\n", &theme);
    let unreached = draw(
        "flowchart LR\n  A --> B\n  classDef unused fill:#ff0000\n  style Z fill:#ff0000\n  class A nosuch\n",
        &theme,
    );
    assert_eq!(plain.rows(), unreached.rows());
}

/// Review Focus 4: twenty colours, more than the sixteen slots, all still draw and all
/// take a slot ink, the last ones by sharing.
///
/// A border drawn in the theme default cannot be told apart from one in the slot whose
/// ink is that same palette blue, so the test also asks that the twenty borders between
/// them show every slot ink: with all sixteen slots held, no node fell back.
#[test]
fn more_colours_than_slots_still_draw() {
    let theme = Theme::default_dark();
    let mut src = String::from("flowchart LR\n");
    for index in 0..20u32 {
        let red = 0x10 + 12 * index;
        src.push_str(&format!(
            "  N{index:02}\n  style N{index:02} stroke:#{red:02x}0040\n"
        ));
    }
    let canvas = render_mermaid(&src, 400, &theme).expect("draws");
    let inks: Vec<Option<Color>> = theme.diagram_slots.iter().map(|slot| slot.ink).collect();
    let mut drawn = Vec::new();
    for index in 0..20u32 {
        let label = format!("N{index:02}");
        let (row, col) = locate(&canvas, &label);
        let (top, left) = corner(&canvas, row, col);
        let ink = fg(&canvas, top, left);
        assert!(
            ink.is_some() && inks.contains(&ink),
            "{label} border {ink:?}"
        );
        drawn.push(ink);
    }
    for (slot, ink) in inks.iter().enumerate() {
        assert!(drawn.contains(ink), "slot {slot} ink {ink:?} unused");
    }
}
