//! Snapshot tests of flowchart layout (design spec §6.1).
//!
//! The parser is deliberately not involved: every case is a hand-built
//! [`Flowchart`], so a change in `mermaid::parse` can never silently rewrite what these
//! snapshots are checking.

use mdmost::mermaid::ast::{
    ArrowHead, Direction, EdgeStroke, FlowEdge, FlowNode, Flowchart, Group, Label, NodeId,
    NodeShape,
};
use mdmost::mermaid::layout::flowchart;
use mdmost::theme::Theme;

/// A node with the given key, label and shape.
fn node(key: &str, label: &str, shape: NodeShape) -> FlowNode {
    FlowNode {
        key: key.to_string(),
        label: Label::line(label),
        shape,
    }
}

/// A plain arrow between two node indices.
fn edge(from: usize, to: usize) -> FlowEdge {
    FlowEdge {
        from: NodeId(from),
        to: NodeId(to),
        stroke: EdgeStroke::Solid,
        tail: ArrowHead::None,
        head: ArrowHead::Arrow,
        label: None,
    }
}

/// A flowchart whose nodes all sit in the root container.
fn chart(direction: Direction, nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> Flowchart {
    let root = Group {
        nodes: (0..nodes.len()).map(NodeId).collect(),
        ..Group::default()
    };
    Flowchart {
        direction,
        nodes,
        edges,
        root,
    }
}

/// Renders a chart to plain text for snapshotting.
fn render(chart: &Flowchart, width: u16) -> String {
    let theme = Theme::default_dark();
    let canvas = flowchart::draw(chart, width, &theme).expect("diagram fits");
    assert_eq!(canvas.width(), width, "canvas is exactly the width budget");
    canvas.check_invariants().expect("canvas contract holds");
    canvas.plain_text()
}

/// The column `needle` starts at, as a count of characters rather than of bytes.
///
/// Box-drawing glyphs are three bytes and one column wide, so a byte offset runs ahead
/// of the column by two for every one to its left. Everything these tests look for is
/// preceded only by spaces and box art, all of it one column per character.
#[track_caller]
fn column_of(text: &str, needle: &str) -> usize {
    for line in text.lines() {
        if let Some(at) = line.find(needle) {
            return line[..at].chars().count();
        }
    }
    panic!("`{needle}` is not in:\n{text}")
}

/// The classic decision flow, used to check all four directions.
fn decision(direction: Direction) -> Flowchart {
    let nodes = vec![
        node("A", "Start", NodeShape::Stadium),
        node("B", "Is it OK?", NodeShape::Rhombus),
        node("C", "Handle it", NodeShape::Rect),
        node("D", "Report", NodeShape::Rect),
        node("E", "Done", NodeShape::Stadium),
    ];
    let edges = vec![
        edge(0, 1),
        FlowEdge {
            label: Some(Label::line("yes")),
            ..edge(1, 2)
        },
        FlowEdge {
            label: Some(Label::line("no")),
            ..edge(1, 3)
        },
        edge(2, 4),
        edge(3, 4),
    ];
    chart(direction, nodes, edges)
}

#[test]
fn every_node_shape() {
    let shapes = [
        (NodeShape::Rect, "Rect"),
        (NodeShape::Round, "Round"),
        (NodeShape::Stadium, "Stadium"),
        (NodeShape::Rhombus, "Rhombus"),
        (NodeShape::Circle, "Circle"),
        (NodeShape::Subroutine, "Subroutine"),
        (NodeShape::Cylinder, "Cylinder"),
    ];
    let nodes = shapes
        .iter()
        .map(|&(shape, name)| node(name, name, shape))
        .collect();
    let edges = (0..shapes.len() - 1).map(|at| edge(at, at + 1)).collect();
    insta::assert_snapshot!(render(&chart(Direction::TopToBottom, nodes, edges), 60));
}

#[test]
fn direction_top_to_bottom() {
    insta::assert_snapshot!(render(&decision(Direction::TopToBottom), 70));
}

#[test]
fn direction_bottom_to_top() {
    insta::assert_snapshot!(render(&decision(Direction::BottomToTop), 70));
}

#[test]
fn direction_left_to_right() {
    insta::assert_snapshot!(render(&decision(Direction::LeftToRight), 90));
}

#[test]
fn direction_right_to_left() {
    insta::assert_snapshot!(render(&decision(Direction::RightToLeft), 90));
}

#[test]
fn every_edge_style() {
    let nodes = vec![
        node("A", "Solid", NodeShape::Rect),
        node("B", "Dotted", NodeShape::Rect),
        node("C", "Thick", NodeShape::Rect),
        node("D", "Plain", NodeShape::Rect),
    ];
    let edges = vec![
        edge(0, 1),
        FlowEdge {
            stroke: EdgeStroke::Dotted,
            label: Some(Label::line("maybe")),
            ..edge(1, 2)
        },
        FlowEdge {
            stroke: EdgeStroke::Thick,
            ..edge(2, 3)
        },
        FlowEdge {
            head: ArrowHead::None,
            ..edge(0, 3)
        },
    ];
    insta::assert_snapshot!(render(&chart(Direction::TopToBottom, nodes, edges), 60));
}

#[test]
fn nested_subgraphs() {
    let nodes = vec![
        node("A", "Client", NodeShape::Round),
        node("B", "API", NodeShape::Rect),
        node("C", "Worker", NodeShape::Rect),
        node("D", "DB", NodeShape::Cylinder),
        node("E", "Cache", NodeShape::Cylinder),
    ];
    let edges = vec![edge(0, 1), edge(1, 2), edge(2, 3), edge(1, 4)];
    let storage = Group {
        key: Some("store".to_string()),
        title: Some(Label::line("Storage")),
        nodes: vec![NodeId(2), NodeId(3), NodeId(4)],
        ..Group::default()
    };
    let backend = Group {
        key: Some("backend".to_string()),
        title: Some(Label::line("Backend")),
        nodes: vec![NodeId(1)],
        children: vec![storage],
        ..Group::default()
    };
    let chart = Flowchart {
        direction: Direction::TopToBottom,
        nodes,
        edges,
        root: Group {
            nodes: vec![NodeId(0)],
            children: vec![backend],
            ..Group::default()
        },
    };
    insta::assert_snapshot!(render(&chart, 70));
}

#[test]
fn a_subgraph_with_its_own_direction() {
    let nodes = vec![
        node("A", "In", NodeShape::Round),
        node("B", "One", NodeShape::Rect),
        node("C", "Two", NodeShape::Rect),
    ];
    let edges = vec![edge(0, 1), edge(1, 2)];
    let inner = Group {
        key: Some("row".to_string()),
        title: Some(Label::line("Pipeline")),
        direction: Some(Direction::LeftToRight),
        nodes: vec![NodeId(1), NodeId(2)],
        ..Group::default()
    };
    let chart = Flowchart {
        direction: Direction::TopToBottom,
        nodes,
        edges,
        root: Group {
            nodes: vec![NodeId(0)],
            children: vec![inner],
            ..Group::default()
        },
    };
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn a_cycle_and_a_self_loop() {
    let nodes = vec![
        node("A", "Idle", NodeShape::Round),
        node("B", "Working", NodeShape::Rect),
        node("C", "Retry", NodeShape::Rect),
    ];
    let edges = vec![
        edge(0, 1),
        FlowEdge {
            label: Some(Label::line("fail")),
            ..edge(1, 2)
        },
        edge(2, 0),
        FlowEdge {
            label: Some(Label::line("poll")),
            ..edge(1, 1)
        },
    ];
    insta::assert_snapshot!(render(&chart(Direction::TopToBottom, nodes, edges), 60));
}

#[test]
fn a_single_node_and_no_edges() {
    let nodes = vec![node("A", "Alone", NodeShape::Round)];
    insta::assert_snapshot!(render(
        &chart(Direction::TopToBottom, nodes, Vec::new()),
        30
    ));
}

/// The chart from `docs/qa/visual-review-3.md` §1, which used to give up below 92
/// columns and dump its own source instead.
///
/// It is a small, entirely ordinary `flowchart LR`, and 80 columns is the default
/// terminal width — minus the two the fence chrome takes, the engine sees 78.
fn seven_node_pipeline() -> Flowchart {
    let nodes = vec![
        node("Start", "Start", NodeShape::Stadium),
        node("Parse", "Parse Markdown", NodeShape::Rect),
        node("Check", "Valid?", NodeShape::Rhombus),
        node("Layout", "Layout to canvas", NodeShape::Rect),
        node("Error", "Report error", NodeShape::Rect),
        node("Draw", "Draw", NodeShape::Cylinder),
        node("Stop", "Stop", NodeShape::Stadium),
    ];
    let edges = vec![
        edge(0, 1),
        edge(1, 2),
        FlowEdge {
            label: Some(Label::line("yes")),
            ..edge(2, 3)
        },
        FlowEdge {
            label: Some(Label::line("no")),
            ..edge(2, 4)
        },
        edge(3, 5),
        edge(5, 6),
        edge(4, 6),
    ];
    chart(Direction::LeftToRight, nodes, edges)
}

#[test]
fn an_ordinary_pipeline_draws_in_an_eighty_column_terminal() {
    insta::assert_snapshot!(render(&seven_node_pipeline(), 78));
}

#[test]
fn the_same_pipeline_top_to_bottom_is_unchanged() {
    let chart = Flowchart {
        direction: Direction::TopToBottom,
        ..seven_node_pipeline()
    };
    insta::assert_snapshot!(render(&chart, 78));
}

#[test]
fn a_very_long_label_is_wrapped() {
    let nodes = vec![
        node(
            "A",
            "A node whose label is far too long for the width it is given",
            NodeShape::Rect,
        ),
        node("B", "Short", NodeShape::Rect),
    ];
    insta::assert_snapshot!(render(
        &chart(Direction::TopToBottom, nodes, vec![edge(0, 1)]),
        34
    ));
}

#[test]
fn an_explicit_line_break_in_a_label() {
    let nodes = vec![node("A", "first<br>second", NodeShape::Rect)];
    let nodes = vec![FlowNode {
        label: Label::parse("first<br>second"),
        ..nodes.into_iter().next().expect("one node")
    }];
    insta::assert_snapshot!(render(
        &chart(Direction::TopToBottom, nodes, Vec::new()),
        30
    ));
}

#[test]
fn a_wide_fan_out_and_back_in() {
    let mut nodes = vec![node("A", "Root", NodeShape::Round)];
    let mut edges = Vec::new();
    for index in 1..=5 {
        nodes.push(node(
            &format!("N{index}"),
            &format!("Job {index}"),
            NodeShape::Rect,
        ));
        edges.push(edge(0, index));
    }
    nodes.push(node("Z", "Join", NodeShape::Round));
    for index in 1..=5 {
        edges.push(edge(index, 6));
    }
    insta::assert_snapshot!(render(&chart(Direction::TopToBottom, nodes, edges), 80));
}

#[test]
fn a_long_edge_skips_a_rank() {
    let nodes = vec![
        node("A", "One", NodeShape::Rect),
        node("B", "Two", NodeShape::Rect),
        node("C", "Three", NodeShape::Rect),
        node("D", "Four", NodeShape::Rect),
    ];
    let edges = vec![edge(0, 1), edge(1, 2), edge(2, 3), edge(0, 3)];
    insta::assert_snapshot!(render(&chart(Direction::TopToBottom, nodes, edges), 60));
}

#[test]
fn a_labelled_edge_draws_its_label_once_whatever_its_rank_span() {
    // A long edge is cut into one segment per rank it crosses. The label belongs to the
    // edge, not to a segment, so drawing it per segment paints one copy per rank — the
    // count rises with the span, which is why every span from one upwards is checked
    // here: a fix that only handled two ranks would pass a two-rank test and still be
    // wrong.
    for span in 1..=4usize {
        // A chain `n0 → n1 → … → nSpan`, plus a `T` that jumps straight to the end.
        // `T` ranks alongside `n0`, so its edge spans exactly `span` ranks.
        let mut nodes: Vec<FlowNode> = (0..=span)
            .map(|index| node(&format!("N{index}"), &format!("n{index}"), NodeShape::Rect))
            .collect();
        nodes.push(node("T", "tee", NodeShape::Rect));
        let mut edges: Vec<FlowEdge> = (0..span).map(|index| edge(index, index + 1)).collect();
        edges.push(FlowEdge {
            label: Some(Label::line("skip")),
            ..edge(span + 1, span)
        });
        for direction in [Direction::TopToBottom, Direction::LeftToRight] {
            let text = render(&chart(direction, nodes.clone(), edges.clone()), 70);
            let count = text.matches("skip").count();
            assert_eq!(
                count, 1,
                "a label on an edge spanning {span} rank(s) was drawn {count} times \
                 going {direction:?}:\n{text}"
            );
        }
    }
}

#[test]
fn every_line_of_a_multi_line_edge_label_shares_one_left_origin() {
    // A label is one block: whatever decides where it goes, all of its lines have to
    // start in the same column. Asserted as equality between the lines rather than
    // against fixed columns, so that a relayout moving the whole diagram cannot make
    // the test lie. Three lines, because a two-line label would pass a fix that only
    // brought the last line into place.
    let nodes = vec![
        node("A", "Source", NodeShape::Rect),
        node("B", "Target", NodeShape::Rect),
    ];
    let edges = vec![FlowEdge {
        label: Some(Label {
            lines: vec!["alpha".into(), "bravo".into(), "charlie".into()],
            source: Default::default(),
        }),
        ..edge(0, 1)
    }];
    for direction in [Direction::TopToBottom, Direction::LeftToRight] {
        for width in [60, 80, 100, 120] {
            let text = render(&chart(direction, nodes.clone(), edges.clone()), width);
            let first = column_of(&text, "alpha");
            for line in ["bravo", "charlie"] {
                assert_eq!(
                    column_of(&text, line),
                    first,
                    "`{line}` is not under `alpha` at width {width} going \
                     {direction:?}:\n{text}"
                );
            }
        }
    }
}

#[test]
fn wide_and_emoji_labels_keep_the_grid() {
    let nodes = vec![
        node("A", "开始处理", NodeShape::Round),
        node("B", "🚀 deploy", NodeShape::Rect),
        node("C", "日本語のラベル", NodeShape::Stadium),
    ];
    let edges = vec![
        FlowEdge {
            label: Some(Label::line("はい")),
            ..edge(0, 1)
        },
        edge(1, 2),
    ];
    insta::assert_snapshot!(render(&chart(Direction::TopToBottom, nodes, edges), 60));
}

#[test]
fn two_edges_between_the_same_pair() {
    let nodes = vec![
        node("A", "Producer", NodeShape::Rect),
        node("B", "Consumer", NodeShape::Rect),
    ];
    let edges = vec![
        FlowEdge {
            label: Some(Label::line("data")),
            ..edge(0, 1)
        },
        FlowEdge {
            stroke: EdgeStroke::Dotted,
            label: Some(Label::line("ack")),
            ..edge(1, 0)
        },
    ];
    insta::assert_snapshot!(render(&chart(Direction::TopToBottom, nodes, edges), 50));
}

#[test]
fn a_hundred_nodes_still_fit_the_budget() {
    let nodes: Vec<FlowNode> = (0..100)
        .map(|index| node(&format!("N{index}"), &format!("n{index}"), NodeShape::Rect))
        .collect();
    let edges: Vec<FlowEdge> = (1..100).map(|index| edge(index / 3, index)).collect();
    let chart = chart(Direction::LeftToRight, nodes, edges);
    let theme = Theme::default_dark();
    let canvas = flowchart::draw(&chart, 200, &theme).expect("a hundred nodes fit in 200 columns");
    assert_eq!(canvas.width(), 200);
    canvas.check_invariants().expect("canvas contract holds");
    // Every node is drawn exactly once, so nothing was overlapped or clipped.
    let text = canvas.plain_text();
    for index in 0..100 {
        assert_eq!(
            text.matches(&format!("n{index} ")).count()
                + text.matches(&format!("n{index}\n")).count(),
            1,
            "node n{index} drawn exactly once"
        );
    }
}
