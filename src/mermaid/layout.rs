//! Diagram layout onto a [`Canvas`](crate::canvas::Canvas).
//!
//! The families that are "boxes joined by edges" — flowchart (§6.1), class (§6.3), ER
//! (§6.4) and state (§6.7) — all share the layered layout engine in [`graph`]. Each
//! family module is then only two things: a translation of its AST into a
//! [`GraphSpec`](graph::GraphSpec), and a [`NodeArt`](graph::NodeArt) that knows how to
//! draw one of its boxes.
//!
//! Sequence, pie and gantt diagrams are not graphs and live outside this module.

pub mod class;
pub mod er;
pub mod flowchart;
pub mod graph;
mod record;
pub mod state;
