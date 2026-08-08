//! `flowchart` / `graph` layout (design spec §6.1).
//!
//! All the work is done by the shared engine in [`graph`](super::graph); this module
//! only translates the flowchart AST into a [`GraphSpec`] and says how a flowchart node
//! is drawn.

mod shape;

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::{ArrowHead, EdgeStroke, Flowchart, Group, NodeId};
use crate::text::wrap_plain;
use crate::theme::Theme;

use super::graph::{
    self, EdgeSpec, GraphSpec, GroupSpec, NodeArt, NodeIdx, PortPolicy, Stroke, Terminator,
};

/// Widest an edge label is allowed to get before it is wrapped.
const LABEL_WIDTH: usize = 18;

/// Draws a flowchart into a canvas exactly `width` columns wide.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when the diagram cannot be made to fit.
pub fn draw(chart: &Flowchart, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    let spec = build(chart);
    graph::draw(&spec, &Art { chart }, width, theme)
}

/// Draws flowchart nodes for the engine.
struct Art<'a> {
    chart: &'a Flowchart,
}

impl NodeArt for Art<'_> {
    fn render(&self, node: NodeIdx, budget: u16, theme: &Theme) -> Canvas {
        match self.chart.nodes.get(node.0) {
            Some(flow) => shape::draw(&flow.label, flow.shape, budget, theme),
            None => Canvas::empty(0),
        }
    }

    fn ports(&self, node: NodeIdx) -> PortPolicy {
        self.chart
            .nodes
            .get(node.0)
            .map_or(PortPolicy::Spread, |flow| shape::ports(flow.shape))
    }
}

/// Translates the flowchart AST into an engine specification.
fn build(chart: &Flowchart) -> GraphSpec {
    GraphSpec {
        direction: chart.direction,
        node_count: chart.nodes.len(),
        edges: chart.edges.iter().map(edge).collect(),
        root: group(&chart.root),
    }
}

/// Translates one edge, including its stroke, terminators and label.
fn edge(edge: &crate::mermaid::ast::FlowEdge) -> EdgeSpec {
    EdgeSpec {
        from: NodeIdx(edge.from.0),
        to: NodeIdx(edge.to.0),
        stroke: match edge.stroke {
            EdgeStroke::Solid => Stroke::Solid,
            EdgeStroke::Dotted => Stroke::Dotted,
            EdgeStroke::Thick => Stroke::Thick,
        },
        tail: terminator(edge.tail),
        head: terminator(edge.head),
        label: edge
            .label
            .as_ref()
            .filter(|label| !label.is_empty())
            .map(|label| {
                label
                    .lines
                    .iter()
                    .flat_map(|line| wrap_plain(line, LABEL_WIDTH))
                    .collect()
            })
            .unwrap_or_default(),
        tail_label: None,
        head_label: None,
    }
}

/// Translates an arrowhead.
fn terminator(head: ArrowHead) -> Terminator {
    match head {
        ArrowHead::None => Terminator::None,
        ArrowHead::Arrow => Terminator::Arrow,
    }
}

/// Translates a subgraph tree.
fn group(group: &Group) -> GroupSpec {
    GroupSpec {
        title: group
            .title
            .as_ref()
            .map(|title| title.lines.clone())
            .or_else(|| group.key.clone().map(|key| vec![key])),
        direction: group.direction,
        nodes: group.nodes.iter().map(|&NodeId(id)| NodeIdx(id)).collect(),
        children: group.children.iter().map(self::group).collect(),
    }
}
