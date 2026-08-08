//! Engine-level tests: the invariants every layout must satisfy.

use crate::canvas::{BorderSet, Canvas};
use crate::mermaid::ast::Direction;
use crate::theme::Theme;

use super::{EdgeSpec, GraphSpec, GroupSpec, NodeIdx, Stroke, Terminator, draw};

/// A one-letter box, so a label can never be wrapped away by the width ladder.
fn art(node: NodeIdx, _budget: u16, theme: &Theme) -> Canvas {
    let letter = char::from(b'A' + (node.0 % 26) as u8);
    Canvas::from_text(3, &format!(" {letter} "), theme.diagram.node_text).framed(
        BorderSet::PLAIN,
        theme.diagram.node_border,
        None,
        theme.base(),
    )
}

/// A graph of `count` nodes wired by `edges`, all in the root container.
fn spec(direction: Direction, count: usize, edges: &[(usize, usize)]) -> GraphSpec {
    GraphSpec {
        direction,
        node_count: count,
        edges: edges
            .iter()
            .map(|&(from, to)| EdgeSpec::arrow(NodeIdx(from), NodeIdx(to)))
            .collect(),
        root: GroupSpec {
            nodes: (0..count).map(NodeIdx).collect(),
            ..GroupSpec::default()
        },
    }
}

/// Every direction, so a test can sweep them all.
const DIRECTIONS: [Direction; 4] = [
    Direction::TopToBottom,
    Direction::BottomToTop,
    Direction::LeftToRight,
    Direction::RightToLeft,
];

#[test]
fn every_direction_keeps_the_canvas_contract() {
    let theme = Theme::default_dark();
    for direction in DIRECTIONS {
        let spec = spec(direction, 5, &[(0, 1), (0, 2), (1, 3), (2, 3), (3, 4)]);
        let canvas = draw(&spec, &art, 80, &theme).expect("fits in 80 columns");
        assert_eq!(canvas.width(), 80);
        canvas.check_invariants().expect("canvas contract holds");
    }
}

#[test]
fn every_node_survives_the_layout() {
    let theme = Theme::default_dark();
    for direction in DIRECTIONS {
        let spec = spec(
            direction,
            6,
            &[(0, 1), (1, 2), (2, 3), (0, 4), (4, 5), (5, 3)],
        );
        let canvas = draw(&spec, &art, 100, &theme).expect("fits");
        let text = canvas.plain_text();
        for node in 0..6 {
            let letter = char::from(b'A' + node as u8);
            assert_eq!(
                text.matches(letter).count(),
                1,
                "node {letter} drawn exactly once in {direction:?}\n{text}"
            );
        }
    }
}

#[test]
fn layout_is_deterministic() {
    let theme = Theme::default_dark();
    let spec = spec(
        Direction::TopToBottom,
        7,
        &[(0, 3), (1, 3), (2, 4), (3, 5), (4, 5), (5, 6), (6, 0)],
    );
    let first = draw(&spec, &art, 90, &theme).expect("fits");
    for _ in 0..3 {
        assert_eq!(draw(&spec, &art, 90, &theme).expect("fits"), first);
    }
}

#[test]
fn a_cycle_does_not_hang_and_keeps_its_arrow() {
    let theme = Theme::default_dark();
    let spec = spec(Direction::TopToBottom, 3, &[(0, 1), (1, 2), (2, 0)]);
    let canvas = draw(&spec, &art, 60, &theme).expect("fits");
    let text = canvas.plain_text();
    assert!(text.contains('▼') || text.contains('▲'), "{text}");
}

#[test]
fn a_single_node_needs_no_edges() {
    let theme = Theme::default_dark();
    let canvas = draw(&spec(Direction::TopToBottom, 1, &[]), &art, 20, &theme).expect("fits");
    assert_eq!(canvas.height(), 3);
    assert_eq!(canvas.width(), 20);
}

#[test]
fn disconnected_nodes_sit_side_by_side() {
    let theme = Theme::default_dark();
    let canvas = draw(&spec(Direction::TopToBottom, 3, &[]), &art, 40, &theme).expect("fits");
    assert_eq!(canvas.height(), 3);
    assert_eq!(canvas.plain_text().matches('A').count(), 1);
}

#[test]
fn a_hundred_nodes_stay_within_the_budget() {
    let theme = Theme::default_dark();
    let edges: Vec<(usize, usize)> = (1..100).map(|node| (node / 3, node)).collect();
    let spec = spec(Direction::LeftToRight, 100, &edges);
    let canvas = draw(&spec, &art, 200, &theme).expect("fits in 200 columns");
    assert_eq!(canvas.width(), 200);
    canvas.check_invariants().expect("canvas contract holds");
}

#[test]
fn an_impossible_width_is_reported_rather_than_overflowing() {
    let theme = Theme::default_dark();
    let spec = spec(Direction::TopToBottom, 8, &[]);
    let error = draw(&spec, &art, 6, &theme).expect_err("cannot fit eight boxes in six columns");
    assert!(matches!(
        error,
        crate::error::MermaidError::TooNarrow { width: 6 }
    ));
}

#[test]
fn a_node_in_two_groups_is_rejected() {
    let theme = Theme::default_dark();
    let mut spec = spec(Direction::TopToBottom, 2, &[]);
    spec.root.children.push(GroupSpec {
        title: Some(vec!["dup".to_string()]),
        nodes: vec![NodeIdx(0)],
        ..GroupSpec::default()
    });
    assert!(draw(&spec, &art, 40, &theme).is_err());
}

#[test]
fn nested_groups_are_framed_from_the_inside_out() {
    let theme = Theme::default_dark();
    let mut spec = spec(Direction::TopToBottom, 3, &[(0, 1), (1, 2)]);
    spec.root.nodes = vec![NodeIdx(0)];
    spec.root.children = vec![GroupSpec {
        title: Some(vec!["outer".to_string()]),
        nodes: vec![NodeIdx(1)],
        children: vec![GroupSpec {
            title: Some(vec!["inner".to_string()]),
            nodes: vec![NodeIdx(2)],
            ..GroupSpec::default()
        }],
        ..GroupSpec::default()
    }];
    let text = draw(&spec, &art, 60, &theme).expect("fits").plain_text();
    assert!(text.contains("outer"), "{text}");
    assert!(text.contains("inner"), "{text}");
    let outer = text.find("outer").expect("outer title");
    let inner = text.find("inner").expect("inner title");
    assert!(outer < inner, "outer frame opens first\n{text}");
}

#[test]
fn strokes_and_terminators_reach_the_canvas() {
    let theme = Theme::default_dark();
    let mut spec = spec(Direction::TopToBottom, 4, &[(0, 1), (1, 2), (2, 3)]);
    spec.edges[0].stroke = Stroke::Dotted;
    spec.edges[1].stroke = Stroke::Thick;
    spec.edges[2].head = Terminator::HollowTriangle;
    let text = draw(&spec, &art, 40, &theme).expect("fits").plain_text();
    assert!(text.contains('┊'), "dotted stroke\n{text}");
    assert!(text.contains('┃'), "thick stroke\n{text}");
    assert!(text.contains('▽'), "hollow triangle\n{text}");
}

#[test]
fn a_self_loop_draws_beside_its_node() {
    let theme = Theme::default_dark();
    let mut spec = spec(Direction::TopToBottom, 2, &[(0, 1)]);
    spec.edges.push(EdgeSpec::arrow(NodeIdx(0), NodeIdx(0)));
    let canvas = draw(&spec, &art, 40, &theme).expect("fits");
    canvas.check_invariants().expect("canvas contract holds");
    let text = canvas.plain_text();
    assert!(text.contains('▲'), "the loop returns into its node\n{text}");
}

#[test]
fn parallel_edges_get_their_own_ports_when_the_box_is_wide_enough() {
    let theme = Theme::default_dark();
    let wide = |node: NodeIdx, _budget: u16, theme: &Theme| {
        let letter = char::from(b'A' + (node.0 % 26) as u8);
        Canvas::from_text(11, &format!("     {letter}     "), theme.diagram.node_text).framed(
            BorderSet::PLAIN,
            theme.diagram.node_border,
            None,
            theme.base(),
        )
    };
    let spec = spec(Direction::TopToBottom, 2, &[(0, 1), (0, 1)]);
    let canvas = draw(&spec, &wide, 40, &theme).expect("fits");
    assert_eq!(
        canvas.plain_text().matches('▼').count(),
        2,
        "{}",
        canvas.plain_text()
    );
}

#[test]
fn parallel_edges_merge_into_one_port_on_a_narrow_box() {
    let theme = Theme::default_dark();
    let spec = spec(Direction::TopToBottom, 2, &[(0, 1), (0, 1)]);
    let canvas = draw(&spec, &art, 40, &theme).expect("fits");
    assert_eq!(
        canvas.plain_text().matches('▼').count(),
        1,
        "{}",
        canvas.plain_text()
    );
}

#[test]
fn end_notes_are_drawn_beside_their_terminators() {
    let theme = Theme::default_dark();
    let mut spec = spec(Direction::TopToBottom, 2, &[(0, 1)]);
    spec.edges[0].tail_label = Some("1".to_string());
    spec.edges[0].head_label = Some("0..*".to_string());
    spec.edges[0].head = Terminator::HollowDiamond;
    let text = draw(&spec, &art, 40, &theme).expect("fits").plain_text();
    assert!(text.contains('1'), "tail note\n{text}");
    assert!(text.contains("0..*"), "head note\n{text}");
    assert!(text.contains('◇'), "hollow diamond\n{text}");
}

#[test]
fn end_notes_survive_a_left_to_right_graph() {
    let theme = Theme::default_dark();
    let mut spec = spec(Direction::LeftToRight, 2, &[(0, 1)]);
    spec.edges[0].tail_label = Some("1".to_string());
    spec.edges[0].head_label = Some("0..*".to_string());
    let canvas = draw(&spec, &art, 60, &theme).expect("fits");
    canvas.check_invariants().expect("canvas contract holds");
    let text = canvas.plain_text();
    assert!(text.contains("0..*"), "{text}");
}
