//! The shared layered graph layout engine.
//!
//! Flowchart, class, ER and state diagrams are all "boxes joined by edges", so they all
//! come through here (design spec §6.1, §6.3, §6.4, §6.7). A caller describes the
//! topology as a [`GraphSpec`] and supplies a [`NodeArt`] that knows how to draw one
//! node; the engine does cycle breaking, layer assignment, crossing reduction,
//! coordinate assignment on the character grid, orthogonal edge routing with junction
//! glyphs, container frames and the fit-to-width degradation ladder.
//!
//! ```no_run
//! use mdless::mermaid::ast::Direction;
//! use mdless::mermaid::layout::graph::{self, EdgeSpec, GraphSpec, NodeIdx};
//! use mdless::theme::Theme;
//! use mdless::canvas::Canvas;
//!
//! let theme = Theme::default_dark();
//! let mut spec = GraphSpec::new(Direction::TopToBottom);
//! spec.node_count = 2;
//! spec.root.nodes = vec![NodeIdx(0), NodeIdx(1)];
//! spec.edges.push(EdgeSpec::arrow(NodeIdx(0), NodeIdx(1)));
//! let art = |node: NodeIdx, _budget: u16, theme: &Theme| {
//!     Canvas::from_text(5, "  A  ", theme.base())
//!         .framed(Default::default(), theme.diagram.node_border, None, theme.base())
//! };
//! let canvas = graph::draw(&spec, &art, 40, &theme).expect("fits");
//! ```
//!
//! # Determinism
//!
//! Every stage is index-based and every tie is broken by index, so the same
//! `(GraphSpec, NodeArt, width, theme)` always produces exactly the same canvas
//! (design spec §13).

mod frame;
mod glyph;
mod ink;
mod order;
mod place;
mod rank;
mod route;
mod spec;

#[cfg(test)]
mod tests;

pub use glyph::{Dir, Stroke};
pub use spec::{EdgeSpec, GraphSpec, GroupSpec, NodeArt, NodeIdx, PortPolicy, Terminator};

use crate::canvas::{BorderSet, Canvas};
use crate::error::MermaidError;
use crate::mermaid::ast::Direction;
use crate::text::{Line, Span};
use crate::theme::Theme;

use frame::{Frame, Pen};
use rank::RawEdge;
use route::{Input, LevelEdge, Routing};

/// Spacing tried in turn until the drawing fits the width budget.
///
/// Each step is `(cross gap, share of the width a node label may use)`; later steps
/// trade beauty for fit, and the last one is as tight as the engine will go before
/// reporting [`MermaidError::TooNarrow`].
const LADDER: &[(usize, u16)] = &[(3, 1), (3, 2), (2, 2), (2, 3), (1, 3), (1, 4)];

/// Lays out `spec` into a canvas exactly `width` columns wide.
///
/// The drawing is centred in the canvas. Nodes are drawn by `art`, which is called
/// once per node per attempt at fitting the width budget.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when even the tightest spacing and the smallest
/// label budget do not fit into `width`.
pub fn draw(
    spec: &GraphSpec,
    art: &dyn NodeArt,
    width: u16,
    theme: &Theme,
) -> Result<Canvas, MermaidError> {
    validate(spec)?;
    for &(gap, share) in LADDER {
        let budget = (width / share).max(6);
        let ctx = Ctx {
            spec,
            art,
            theme,
            budget,
            gap,
        };
        let drawn = ctx.group(&spec.root, spec.direction);
        if drawn.canvas.width() <= width {
            return Ok(centred(drawn.canvas, width, theme));
        }
    }
    Err(MermaidError::TooNarrow { width })
}

/// Rejects a specification the engine cannot draw.
fn validate(spec: &GraphSpec) -> Result<(), MermaidError> {
    let mut seen = vec![false; spec.node_count];
    let mut stack = vec![&spec.root];
    while let Some(group) = stack.pop() {
        for node in &group.nodes {
            match seen.get_mut(node.0) {
                Some(flag) if !*flag => *flag = true,
                _ => {
                    return Err(MermaidError::Unsupported {
                        line: 0,
                        message: "node placed in more than one subgraph".to_string(),
                    });
                }
            }
        }
        stack.extend(&group.children);
    }
    if seen.iter().any(|placed| !placed) {
        return Err(MermaidError::Unsupported {
            line: 0,
            message: "node missing from the container tree".to_string(),
        });
    }
    if spec
        .edges
        .iter()
        .any(|edge| edge.from.0 >= spec.node_count || edge.to.0 >= spec.node_count)
    {
        return Err(MermaidError::Unsupported {
            line: 0,
            message: "edge refers to an unknown node".to_string(),
        });
    }
    Ok(())
}

/// Centres `canvas` in a canvas exactly `width` columns wide.
fn centred(canvas: Canvas, width: u16, theme: &Theme) -> Canvas {
    let slack = width.saturating_sub(canvas.width());
    let left = slack / 2;
    let mut out = canvas.indent(left, slack - left, theme.base());
    out.resize_width(width, theme.base());
    out
}

/// One attempt at drawing the graph, at a fixed spacing and label budget.
struct Ctx<'a> {
    spec: &'a GraphSpec,
    art: &'a dyn NodeArt,
    theme: &'a Theme,
    budget: u16,
    gap: usize,
}

/// A drawn group: its canvas plus where each node it contains ended up.
struct Drawn {
    canvas: Canvas,
    /// Centre of every contained node, as `(row, col)` inside `canvas`.
    hints: Vec<(NodeIdx, (usize, usize))>,
}

/// One box taking part in a level's layout.
struct Item {
    canvas: Canvas,
    ports: PortPolicy,
    hints: Vec<(NodeIdx, (usize, usize))>,
    members: Vec<NodeIdx>,
}

impl Ctx<'_> {
    /// Lays out one container and everything below it.
    fn group(&self, group: &GroupSpec, inherited: Direction) -> Drawn {
        let direction = group.direction.unwrap_or(inherited);
        let mut items = Vec::new();
        for &node in &group.nodes {
            let canvas = self.art.render(node, self.budget, self.theme);
            let centre = (canvas.height() / 2, usize::from(canvas.width()) / 2);
            items.push(Item {
                canvas,
                ports: self.art.ports(node),
                hints: vec![(node, centre)],
                members: vec![node],
            });
        }
        for child in &group.children {
            let drawn = self.group(child, direction);
            let members = drawn.hints.iter().map(|&(node, _)| node).collect();
            items.push(Item {
                canvas: drawn.canvas,
                ports: PortPolicy::Spread,
                hints: drawn.hints,
                members,
            });
        }
        let inner = self.level(&items, direction);
        match &group.title {
            None => inner,
            Some(title) => self.frame(inner, title),
        }
    }

    /// Wraps a drawn container in its titled frame.
    fn frame(&self, drawn: Drawn, title: &[String]) -> Drawn {
        let styles = self.theme.diagram;
        let mut padded = Canvas::new(drawn.canvas.width(), 1, self.theme.base());
        padded.append(&drawn.canvas, self.theme.base());
        padded.push_blank_row(self.theme.base());
        let padded = padded.indent(1, 1, self.theme.base());
        let heading = title
            .first()
            .map(|text| Line::new(vec![Span::new(text.clone(), styles.group_title)]));
        let canvas = padded.framed(
            BorderSet::DASHED,
            styles.group_border,
            heading.as_ref(),
            self.theme.base(),
        );
        // The frame adds one row and column of border plus one of padding.
        let hints = drawn
            .hints
            .into_iter()
            .map(|(node, (row, col))| (node, (row + 2, col + 2)))
            .collect();
        Drawn { canvas, hints }
    }

    /// Lays out one level: the boxes of a container and the edges between them.
    fn level(&self, items: &[Item], direction: Direction) -> Drawn {
        let vertical = Frame::vertical(direction);
        if items.is_empty() {
            return Drawn {
                canvas: Canvas::empty(0),
                hints: Vec::new(),
            };
        }
        let owner = self.owners(items);
        let (raw, mut level_edges, loops) = self.edges(items, &owner, vertical);
        let mut layered = rank::build(items.len(), &raw);
        order::reduce(&mut layered);
        // An edge reversed to break a cycle is drawn against the flow, so its
        // terminators swap ends and its arrow still points where the source said.
        for (edge, reversed) in level_edges.iter_mut().zip(&layered.reversed) {
            if *reversed {
                std::mem::swap(&mut edge.tail, &mut edge.head);
                std::mem::swap(&mut edge.from_hint, &mut edge.to_hint);
            }
        }

        let count = layered.vnodes.len();
        let mut cross_size = vec![1usize; count];
        let mut flow_size = vec![0usize; count];
        let mut place_size = vec![1usize; count];
        let mut loop_pad = vec![0usize; count];
        let mut ports = vec![PortPolicy::Center; count];
        for (index, item) in items.iter().enumerate() {
            let (rows, cols) = (item.canvas.height(), usize::from(item.canvas.width()));
            let (cross, flow) = if vertical { (cols, rows) } else { (rows, cols) };
            cross_size[index] = cross;
            flow_size[index] = flow;
            let looped = loops.iter().any(|&(item, _)| item == index);
            // A self loop needs three cells beside the box and two rows below it.
            place_size[index] = cross + if looped { 3 } else { 0 };
            loop_pad[index] = if looped { 2 } else { 0 };
            ports[index] = item.ports;
        }
        let cross_gap = if vertical {
            self.gap
        } else {
            self.gap.saturating_sub(2).max(1)
        };
        let cross = place::assign(&layered, &place_size, cross_gap);
        let input = Input {
            layered: &layered,
            cross: &cross,
            cross_size: &cross_size,
            flow_size: &flow_size,
            ports: &ports,
            edges: &level_edges,
            loops: &loops,
            loop_pad: &loop_pad,
            min_gap: if vertical { 1 } else { 3 },
            vertical,
        };
        let routing = Routing::compute(&input);
        let total_cross = cross
            .iter()
            .zip(&place_size)
            .map(|(at, size)| at + size)
            .max()
            .unwrap_or(0)
            .max(routing.cross_extent);
        let frame = Frame::new(direction, routing.total_flow, total_cross);
        let styles = self.theme.diagram;
        let mut pen = Pen::new(frame, self.theme.base(), styles.edge_label);
        let mut hints = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let (row, col) = frame.origin(
                routing.flow[index],
                cross[index],
                flow_size[index],
                cross_size[index],
            );
            pen.canvas.blit(row, col, &item.canvas, self.theme.base());
            for &(node, (hint_row, hint_col)) in &item.hints {
                hints.push((node, (row + hint_row, col + hint_col)));
            }
        }
        routing.paint(&input, &mut pen);
        let mut canvas = pen.canvas;
        pen.ink.apply(&mut canvas, styles.line, styles.arrow);
        Drawn { canvas, hints }
    }

    /// Maps every node under this container to the item that holds it.
    fn owners(&self, items: &[Item]) -> Vec<Option<usize>> {
        let mut owner = vec![None; self.spec.node_count];
        for (index, item) in items.iter().enumerate() {
            for node in &item.members {
                owner[node.0] = Some(index);
            }
        }
        owner
    }

    /// Splits this container's edges into layered edges and self loops.
    fn edges(
        &self,
        items: &[Item],
        owner: &[Option<usize>],
        vertical: bool,
    ) -> (Vec<RawEdge>, Vec<LevelEdge>, Vec<(usize, usize)>) {
        let mut raw = Vec::new();
        let mut level = Vec::new();
        let mut pending = Vec::new();
        for edge in &self.spec.edges {
            let (Some(from), Some(to)) = (owner[edge.from.0], owner[edge.to.0]) else {
                continue;
            };
            let hint = |item: usize, node: NodeIdx| -> usize {
                items[item]
                    .hints
                    .iter()
                    .find(|&&(other, _)| other == node)
                    .map(|&(_, (row, col))| if vertical { col } else { row })
                    .unwrap_or(0)
            };
            let described = LevelEdge {
                stroke: edge.stroke,
                tail: edge.tail,
                head: edge.head,
                label: edge.label.clone(),
                tail_label: edge.tail_label.clone(),
                head_label: edge.head_label.clone(),
                from_hint: hint(from, edge.from),
                to_hint: hint(to, edge.to),
            };
            if from == to {
                // Both ends inside the same nested container: drawn one level down.
                if edge.from == edge.to {
                    pending.push((from, described));
                }
                continue;
            }
            raw.push(RawEdge { from, to });
            level.push(described);
        }
        // Self loops take no part in layering, so they are indexed after the rest.
        let mut loops = Vec::with_capacity(pending.len());
        for (item, described) in pending {
            loops.push((item, level.len()));
            level.push(described);
        }
        (raw, level, loops)
    }
}
