//! Fit is monotone in width, and `TooNarrow.needed` is a true floor.
//!
//! The engine degrades through a ladder whose label budget is `width / share`, so a
//! *wider* terminal used to hand every node a wider budget and overshoot: the seven-node
//! pipeline drew at 63, failed at 64 and drew again at 65. That is indefensible to a
//! reader — widening the window made the diagram disappear — and it blocks any search
//! that bisects on width.
//!
//! These tests state the rule as a measurement rather than as the list of widths that
//! happened to be broken: sweep the width, and once a chart draws it must draw at every
//! greater width. The pinned 63/64/65 case was scaffolding and is deliberately gone; a
//! list of widths goes stale the moment the layout changes, the sweep does not.

use mdless::error::MermaidError;
use mdless::mermaid::ast::{
    ArrowHead, Direction, EdgeStroke, FlowEdge, FlowNode, Flowchart, Group, Label, NodeId,
    NodeShape,
};
use mdless::mermaid::layout::flowchart;
use mdless::theme::Theme;

/// The widest width any test sweeps to — comfortably past every chart's natural width.
const WIDEST: u16 = 200;

fn node(key: &str, label: &str, shape: NodeShape) -> FlowNode {
    FlowNode {
        key: key.to_string(),
        label: Label::line(label),
        shape,
    }
}

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

/// A labelled edge between two node indices.
fn labelled(from: usize, to: usize, label: &str) -> FlowEdge {
    FlowEdge {
        label: Some(Label::line(label)),
        ..edge(from, to)
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

/// Two boxes and an arrow.
fn small() -> Flowchart {
    let nodes = vec![
        node("A", "Read", NodeShape::Rect),
        node("B", "Write", NodeShape::Rect),
    ];
    chart(Direction::TopToBottom, nodes, vec![edge(0, 1)])
}

/// A four-node decision, top to bottom.
fn medium() -> Flowchart {
    let nodes = vec![
        node("A", "Open file", NodeShape::Stadium),
        node("B", "Readable?", NodeShape::Rhombus),
        node("C", "Render", NodeShape::Rect),
        node("D", "Report error", NodeShape::Rect),
    ];
    let edges = vec![
        edge(0, 1),
        labelled(1, 2, "yes"),
        labelled(1, 3, "no"),
        edge(2, 3),
    ];
    chart(Direction::TopToBottom, nodes, edges)
}

/// The seven-node `LR` pipeline whose fit was non-monotone at 63/64/65.
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
        labelled(2, 3, "yes"),
        labelled(2, 4, "no"),
        edge(3, 5),
        edge(5, 6),
        edge(4, 6),
    ];
    chart(Direction::LeftToRight, nodes, edges)
}

/// Two nodes inside a titled subgraph and one outside it, so the container frame — which
/// pads the drawing independently of the label budget — is in the measurement.
fn with_subgraph() -> Flowchart {
    let nodes = vec![
        node("A", "Collect", NodeShape::Rect),
        node("B", "Transform", NodeShape::Rect),
        node("C", "Publish", NodeShape::Rect),
    ];
    let root = Group {
        nodes: vec![NodeId(2)],
        children: vec![Group {
            key: Some("pipe".to_string()),
            title: Some(Label::line("Pipeline stage")),
            nodes: vec![NodeId(0), NodeId(1)],
            ..Group::default()
        }],
        ..Group::default()
    };
    Flowchart {
        direction: Direction::TopToBottom,
        nodes,
        edges: vec![edge(0, 1), edge(1, 2)],
        root,
    }
}

/// Labels far longer than any budget, where wrapping does all the work.
fn long_labels() -> Flowchart {
    let nodes = vec![
        node(
            "A",
            "A node whose label is far too long for the width it is given",
            NodeShape::Rect,
        ),
        node(
            "B",
            "Another label that also refuses to be short about it",
            NodeShape::Rect,
        ),
        node("C", "Short", NodeShape::Rect),
    ];
    chart(
        Direction::LeftToRight,
        nodes,
        vec![edge(0, 1), edge(1, 2), edge(0, 2)],
    )
}

/// Every chart the sweeps run over, with a name for the failure message.
fn charts() -> Vec<(&'static str, Flowchart)> {
    vec![
        ("small", small()),
        ("medium", medium()),
        ("seven-node pipeline", seven_node_pipeline()),
        ("subgraph", with_subgraph()),
        ("long labels", long_labels()),
    ]
}

/// The narrowest width in `1..=WIDEST` at which `chart` draws, if any.
fn first_fit(chart: &Flowchart, theme: &Theme) -> Option<u16> {
    (1..=WIDEST).find(|&width| flowchart::draw(chart, width, theme).is_ok())
}

#[test]
fn once_a_chart_draws_it_draws_at_every_greater_width() {
    let theme = Theme::default_dark();
    for (name, chart) in charts() {
        let first = first_fit(&chart, &theme)
            .unwrap_or_else(|| panic!("{name} draws at no width up to {WIDEST}"));
        for width in first..=WIDEST {
            let drawn = flowchart::draw(&chart, width, &theme);
            assert!(
                drawn.is_ok(),
                "{name} draws at {first} but not at the greater width {width}: {:?}",
                drawn.err()
            );
        }
    }
}

#[test]
fn a_drawing_still_fills_exactly_the_width_it_was_given() {
    // Monotonicity would be trivial to fake by relaxing the fit check, so pin the
    // contract it must not buy itself with: the canvas is exactly `width` and legal.
    let theme = Theme::default_dark();
    for (name, chart) in charts() {
        for width in 1..=WIDEST {
            if let Ok(canvas) = flowchart::draw(&chart, width, &theme) {
                assert_eq!(canvas.width(), width, "{name} at {width} is off-budget");
                canvas.check_invariants().unwrap_or_else(|why| {
                    panic!("{name} at {width} breaks the canvas contract: {why}")
                });
            }
        }
    }
}

#[test]
fn the_reported_floor_is_a_width_below_which_nothing_draws() {
    let theme = Theme::default_dark();
    for (name, chart) in charts() {
        let first = first_fit(&chart, &theme)
            .unwrap_or_else(|| panic!("{name} draws at no width up to {WIDEST}"));
        for width in 1..=WIDEST {
            let Err(error) = flowchart::draw(&chart, width, &theme) else {
                continue;
            };
            let MermaidError::TooNarrow { needed, .. } = error else {
                panic!("{name} at {width} failed for a reason other than width: {error:?}");
            };
            let needed = needed.unwrap_or_else(|| panic!("{name} at {width} names no floor"));
            // A floor is a claim about every width below it, not just about this one, so
            // check it against the sweep's answer: the narrowest width that draws at all.
            // Equality is the whole claim — `needed > first` would be a floor that lies
            // about a width which does draw, `needed < first` one that promises too soon.
            assert!(
                needed > width,
                "{name} at {width} claims a floor of {needed}"
            );
            assert_eq!(
                needed, first,
                "{name} at {width} claims a floor of {needed}, but it first draws at {first}"
            );
        }
    }
}
