//! Property tests for the shared graph layout engine (design spec §13.3).
//!
//! The engine is exercised directly rather than through the flowchart parser, so the
//! invariants hold for every family that will build on it.

use mdless::canvas::{BorderSet, Canvas};
use mdless::error::MermaidError;
use mdless::mermaid::Fit;
use mdless::mermaid::ast::Direction;
use mdless::mermaid::layout::graph::{
    self, EdgeSpec, GraphSpec, GroupSpec, NodeIdx, Stroke, Terminator,
};
use mdless::text::display_width;
use mdless::theme::Theme;
use proptest::prelude::*;

/// A one-letter box, small enough that the width ladder never has to wrap it.
fn art(node: NodeIdx, _budget: u16, theme: &Theme) -> Canvas {
    let letter = char::from(b'a' + (node.0 % 26) as u8);
    Canvas::from_text(3, &format!(" {letter} "), theme.diagram.node_text).framed(
        BorderSet::PLAIN,
        theme.diagram.node_border,
        None,
        theme.base(),
    )
}

/// Every direction, indexed by a small number so proptest can pick one.
fn direction(index: usize) -> Direction {
    match index % 4 {
        0 => Direction::TopToBottom,
        1 => Direction::BottomToTop,
        2 => Direction::LeftToRight,
        _ => Direction::RightToLeft,
    }
}

/// A stroke, indexed the same way.
fn stroke(index: usize) -> Stroke {
    match index % 3 {
        0 => Stroke::Solid,
        1 => Stroke::Dotted,
        _ => Stroke::Thick,
    }
}

/// Builds a graph from a raw description, grouping some nodes into a subgraph.
fn build(dir: usize, count: usize, pairs: &[(usize, usize, usize)], grouped: usize) -> GraphSpec {
    let count = count.max(1);
    let grouped = grouped.min(count);
    let edges = pairs
        .iter()
        .map(|&(from, to, style)| EdgeSpec {
            stroke: stroke(style),
            head: if style % 2 == 0 {
                Terminator::Arrow
            } else {
                Terminator::None
            },
            ..EdgeSpec::arrow(NodeIdx(from % count), NodeIdx(to % count))
        })
        .collect();
    let mut root = GroupSpec {
        nodes: (grouped..count).map(NodeIdx).collect(),
        ..GroupSpec::default()
    };
    if grouped > 0 {
        root.children.push(GroupSpec {
            title: Some(vec!["SUB".to_string()]),
            nodes: (0..grouped).map(NodeIdx).collect(),
            ..GroupSpec::default()
        });
    }
    GraphSpec {
        direction: direction(dir),
        node_count: count,
        edges,
        root,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Whatever the graph, the canvas contract holds and nothing panics.
    #[test]
    fn rendering_never_panics_and_keeps_the_canvas_contract(
        dir in 0usize..4,
        count in 1usize..9,
        pairs in prop::collection::vec((0usize..8, 0usize..8, 0usize..3), 0..14),
        grouped in 0usize..4,
        width in 20u16..120,
    ) {
        let theme = Theme::default_dark();
        let spec = build(dir, count, &pairs, grouped);
        // A graph too wide for its budget must be reported, never drawn over the edge.
        let canvas = match graph::draw(&spec, &art, width, &theme, Fit::COMPACT) {
            Ok(canvas) => canvas,
            Err(error) => {
                prop_assert!(matches!(error, MermaidError::TooNarrow { .. }), "{:?}", error);
                return Ok(());
            }
        };
        prop_assert_eq!(canvas.width(), width);
        prop_assert!(canvas.check_invariants().is_ok());
        for row in 0..canvas.height() {
            prop_assert_eq!(display_width(&canvas.row_text(row)), usize::from(width));
        }
    }

    /// Every node is drawn exactly once: no box is overlapped or clipped away.
    #[test]
    fn every_node_is_drawn_exactly_once(
        dir in 0usize..4,
        count in 1usize..9,
        pairs in prop::collection::vec((0usize..8, 0usize..8, 0usize..3), 0..14),
        grouped in 0usize..4,
    ) {
        let theme = Theme::default_dark();
        let spec = build(dir, count, &pairs, grouped);
        let canvas = graph::draw(&spec, &art, 200, &theme, Fit::COMPACT).expect("a small graph always fits");
        let text = canvas.plain_text();
        for node in 0..count {
            let letter = char::from(b'a' + node as u8);
            prop_assert_eq!(
                text.matches(letter).count(),
                1,
                "node {} drawn once\n{}",
                letter,
                text
            );
        }
    }

    /// The same input always produces the same output.
    #[test]
    fn layout_is_deterministic(
        dir in 0usize..4,
        count in 1usize..9,
        pairs in prop::collection::vec((0usize..8, 0usize..8, 0usize..3), 0..14),
        grouped in 0usize..4,
    ) {
        let theme = Theme::default_dark();
        let spec = build(dir, count, &pairs, grouped);
        let first = graph::draw(&spec, &art, 200, &theme, Fit::COMPACT).expect("fits");
        let second = graph::draw(&spec, &art, 200, &theme, Fit::COMPACT).expect("fits");
        prop_assert!(first == second);
    }

    /// No edge glyph is ever drawn over a node's label.
    #[test]
    fn edges_never_run_through_a_node(
        dir in 0usize..4,
        count in 2usize..9,
        pairs in prop::collection::vec((0usize..8, 0usize..8, 0usize..3), 1..14),
    ) {
        let theme = Theme::default_dark();
        let spec = build(dir, count, &pairs, 0);
        let canvas = graph::draw(&spec, &art, 200, &theme, Fit::COMPACT).expect("fits");
        let text = canvas.plain_text();
        // Every label keeps its blank on both sides; an edge cutting through a box
        // would have replaced one of them with a line glyph.
        for node in 0..count {
            let letter = char::from(b'a' + node as u8);
            let at = text.find(letter).expect("label present");
            let before = text[..at].chars().next_back().expect("a cell before");
            let after = text[at + letter.len_utf8()..]
                .chars()
                .next()
                .expect("a cell after");
            prop_assert_eq!((before, after), (' ', ' '), "{}", text);
        }
    }
}
