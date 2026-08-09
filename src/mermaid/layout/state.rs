//! `stateDiagram-v2` layout (design spec §6.7).
//!
//! States are [`NodeArt`] boxes and composite states are [`GroupSpec`] containers, so
//! nesting costs this module nothing — the engine already frames a group and routes
//! edges across it.
//!
//! Two translations are worth knowing about, because the AST and the engine disagree
//! about what a node is:
//!
//! * **`[*]` is per scope, not per diagram.** Each scope that starts or ends with `[*]`
//!   gets its own marker node — a filled dot for the start, a ringed dot for the end —
//!   so a composite state's own start marker sits inside its frame where it belongs.
//! * **A composite state is a container, and an edge cannot end on a container.** A
//!   transition into a composite is aimed at what entering it actually means: the
//!   composite's start marker, or its first state when it has none. A transition out of
//!   one leaves from its end marker, or its last state. A composite whose body turns
//!   out to be empty is demoted to an ordinary state box so the transition still has
//!   somewhere to land.
//!
//! Notes (`note left of X`) are drawn as a box in the note ink, tied to their state by
//! a dotted line, on the side the author asked for where the layout has room for it.
//! The side is semantic content the author wrote, not a layout decision taken from the
//! AST, so consuming it here — at render time, with the width known — is exactly what
//! design spec §3 permits.

mod shape;

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::{
    Direction, NotePlacement, StateDiagram, StateEndpoint, StateId, StateKind, StateNode,
    StateNote, StateScope, Transition,
};
use crate::text::wrap_plain;
use crate::theme::Theme;

use super::graph::{
    self, EdgeSpec, Fit, GraphSpec, GroupSpec, NodeArt, NodeIdx, PortPolicy, Stroke, Terminator,
};

/// Widest a transition label is allowed to get before it is wrapped.
const LABEL_WIDTH: usize = 18;
/// Widest a note is allowed to get before it is wrapped.
const NOTE_WIDTH: usize = 24;

/// Draws a state diagram into a canvas exactly `width` columns wide.
///
/// The engine may degrade the drawing as far as [`Fit::COMPACT`] allows; use
/// [`draw_with`] to say otherwise.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when the diagram cannot be made to fit.
pub fn draw(diagram: &StateDiagram, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    draw_with(diagram, width, theme, Fit::COMPACT)
}

/// Draws a state diagram under the given fit policy.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when the diagram cannot be made to fit within
/// what `fit` allows.
pub fn draw_with(
    diagram: &StateDiagram,
    width: u16,
    theme: &Theme,
    fit: Fit,
) -> Result<Canvas, MermaidError> {
    let plan = Plan::of(diagram);
    let spec = GraphSpec {
        direction: diagram.direction.unwrap_or(Direction::TopToBottom),
        node_count: plan.slots.len(),
        edges: plan.edges.clone(),
        root: plan.root.clone(),
    };
    graph::draw(
        &spec,
        &Art {
            plan: &plan,
            diagram,
        },
        width,
        theme,
        fit,
    )
}

/// What one engine node stands for.
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    /// An ordinary, choice, fork or join state.
    State(StateId),
    /// A scope's `[*]` start marker.
    Start,
    /// A scope's `[*]` end marker.
    End,
    /// A note, holding its already-wrapped text.
    Note(Vec<String>),
}

/// The translated diagram: what each node is, how they are grouped and joined.
#[derive(Debug, Default)]
struct Plan {
    /// One entry per engine node, indexed by [`NodeIdx`].
    slots: Vec<Slot>,
    /// Every edge, in translation order.
    edges: Vec<EdgeSpec>,
    /// The top-level container.
    root: GroupSpec,
}

/// The nodes a scope offers to transitions in the scope above it.
#[derive(Debug, Clone, Copy, Default)]
struct Ends {
    /// Where a transition entering this scope should land.
    entry: Option<NodeIdx>,
    /// Where a transition leaving this scope should start.
    exit: Option<NodeIdx>,
}

impl Plan {
    /// Translates a whole diagram.
    fn of(diagram: &StateDiagram) -> Self {
        let mut plan = Self::default();
        let mut ends = vec![Ends::default(); diagram.states.len()];
        let (root, _) = plan.scope(diagram, &diagram.root, &mut ends);
        plan.root = root;
        plan
    }

    /// Allocates a node for `slot` and returns its index.
    fn push(&mut self, slot: Slot) -> NodeIdx {
        self.slots.push(slot);
        NodeIdx(self.slots.len() - 1)
    }

    /// Translates one scope into a container, returning it and its entry/exit nodes.
    ///
    /// Children are translated before this scope's own transitions, so every endpoint
    /// a transition can name is already known by the time the edges are built.
    fn scope(
        &mut self,
        diagram: &StateDiagram,
        scope: &StateScope,
        ends: &mut Vec<Ends>,
    ) -> (GroupSpec, Ends) {
        let mut group = GroupSpec {
            direction: scope.direction,
            ..GroupSpec::default()
        };

        let start = scope
            .transitions
            .iter()
            .any(|t| t.from == StateEndpoint::Initial)
            .then(|| {
                let node = self.push(Slot::Start);
                group.nodes.push(node);
                node
            });
        let end = scope
            .transitions
            .iter()
            .any(|t| t.to == StateEndpoint::Final)
            .then(|| {
                let node = self.push(Slot::End);
                group.nodes.push(node);
                node
            });

        let mut node_of = vec![None; diagram.states.len()];
        for &id in &scope.states {
            let Some(state) = diagram.states.get(id.0) else {
                continue;
            };
            match &state.kind {
                StateKind::Composite(inner) => {
                    let (mut child, inner_ends) = self.scope(diagram, inner, ends);
                    if child.nodes.is_empty() && child.children.is_empty() {
                        // An empty composite has nowhere for a transition to land, so
                        // it becomes an ordinary state box instead of a frame.
                        let node = self.push(Slot::State(id));
                        group.nodes.push(node);
                        node_of[id.0] = Some(node);
                        ends[id.0] = Ends {
                            entry: Some(node),
                            exit: Some(node),
                        };
                    } else {
                        child.title = Some(label_lines(state, LABEL_WIDTH));
                        group.children.push(child);
                        ends[id.0] = inner_ends;
                    }
                }
                _ => {
                    let node = self.push(Slot::State(id));
                    group.nodes.push(node);
                    node_of[id.0] = Some(node);
                    ends[id.0] = Ends {
                        entry: Some(node),
                        exit: Some(node),
                    };
                }
            }
        }

        for note in &scope.notes {
            self.note(note, &mut group, ends);
        }

        for transition in &scope.transitions {
            if let Some(edge) = self.transition(transition, start, end, ends) {
                self.edges.push(edge);
            }
        }

        let scope_ends = Ends {
            entry: start.or_else(|| group.nodes.first().copied()),
            exit: end.or_else(|| group.nodes.last().copied()),
        };
        (group, scope_ends)
    }

    /// Adds a note box and the dotted line tying it to its state.
    ///
    /// `note left of X` / `note right of X` is the author's own words, not a layout
    /// decision, so the side is honoured rather than discarded: the note is declared
    /// immediately before or after its state, which is the order the engine seeds its
    /// crossing reduction from, so the note comes out on the requested side of the
    /// state whenever the layout has room for it there.
    ///
    /// It is a *preference*, not a guarantee, and it is horizontal only. The engine
    /// ranks nodes by their edges, so a note tied to its state necessarily sits one
    /// rank further along the flow rather than exactly beside it; pinning two nodes to
    /// the same rank is not something the engine can currently express.
    fn note(&mut self, note: &StateNote, group: &mut GroupSpec, ends: &[Ends]) {
        let Some(target) = ends.get(note.target.0).and_then(|end| end.entry) else {
            return;
        };
        let lines: Vec<String> = note
            .text
            .lines
            .iter()
            .flat_map(|line| wrap_plain(line, NOTE_WIDTH))
            .collect();
        let node = self.push(Slot::Note(lines));
        match group.nodes.iter().position(|&at| at == target) {
            Some(at) if note.placement == NotePlacement::LeftOf => group.nodes.insert(at, node),
            Some(at) => group.nodes.insert(at + 1, node),
            None => group.nodes.push(node),
        }
        self.edges.push(EdgeSpec {
            from: target,
            to: node,
            stroke: Stroke::Dotted,
            tail: Terminator::None,
            head: Terminator::None,
            label: Vec::new(),
            tail_label: None,
            head_label: None,
        });
    }

    /// Translates one transition, resolving both of its endpoints.
    ///
    /// Returns `None` when an endpoint cannot be resolved — an `[*]` in a scope that
    /// has no marker, or a state the AST does not hold.
    fn transition(
        &self,
        transition: &Transition,
        start: Option<NodeIdx>,
        end: Option<NodeIdx>,
        ends: &[Ends],
    ) -> Option<EdgeSpec> {
        let from = match transition.from {
            StateEndpoint::Initial => start,
            StateEndpoint::Final => end,
            StateEndpoint::State(id) => ends.get(id.0).and_then(|e| e.exit),
        }?;
        let to = match transition.to {
            StateEndpoint::Initial => start,
            StateEndpoint::Final => end,
            StateEndpoint::State(id) => ends.get(id.0).and_then(|e| e.entry),
        }?;
        Some(EdgeSpec {
            from,
            to,
            stroke: Stroke::Solid,
            tail: Terminator::None,
            head: Terminator::Arrow,
            label: transition
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
        })
    }
}

/// The label lines of a state: its description, or its key when it has none.
///
/// The lines are the author's own; wrapping is left to whoever draws them, because a
/// node body and a group title get different budgets (design spec §3).
fn label_text(state: &StateNode) -> Vec<String> {
    match state.label.as_ref().filter(|label| !label.is_empty()) {
        Some(label) => label.lines.clone(),
        None => vec![state.key.clone()],
    }
}

/// [`label_text`] wrapped to `width`, for callers that need finished lines.
fn label_lines(state: &StateNode, width: usize) -> Vec<String> {
    label_text(state)
        .iter()
        .flat_map(|line| wrap_plain(line, width))
        .collect()
}

/// Draws state boxes for the engine.
struct Art<'a> {
    plan: &'a Plan,
    diagram: &'a StateDiagram,
}

impl Art<'_> {
    /// The state a node stands for, if it stands for one.
    fn state(&self, node: NodeIdx) -> Option<&StateNode> {
        match self.plan.slots.get(node.0)? {
            Slot::State(id) => self.diagram.states.get(id.0),
            _ => None,
        }
    }
}

impl NodeArt for Art<'_> {
    fn render(&self, node: NodeIdx, budget: u16, theme: &Theme) -> Canvas {
        match self.plan.slots.get(node.0) {
            Some(Slot::Start) => shape::start(theme),
            Some(Slot::End) => shape::end(theme),
            Some(Slot::Note(lines)) => shape::note(lines, budget, theme),
            Some(Slot::State(_)) => match self.state(node) {
                Some(state) => match state.kind {
                    StateKind::Choice => shape::choice(theme),
                    StateKind::Fork | StateKind::Join => shape::bar(theme),
                    // A composite only reaches here when it was demoted for being
                    // empty, so it draws like any other state.
                    _ => shape::state(&label_text(state), budget, theme),
                },
                None => Canvas::empty(0),
            },
            None => Canvas::empty(0),
        }
    }

    fn ports(&self, node: NodeIdx) -> PortPolicy {
        let centred = match self.plan.slots.get(node.0) {
            Some(Slot::Start | Slot::End) => true,
            Some(Slot::State(_)) => {
                matches!(self.state(node).map(|s| &s.kind), Some(StateKind::Choice))
            }
            _ => false,
        };
        shape::ports(centred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::ast::Label;

    fn state(key: &str, kind: StateKind) -> StateNode {
        StateNode {
            key: key.to_string(),
            label: None,
            kind,
        }
    }

    fn transition(from: StateEndpoint, to: StateEndpoint) -> Transition {
        Transition {
            from,
            to,
            label: None,
        }
    }

    #[test]
    fn a_scope_gets_its_own_start_and_end_markers() {
        let diagram = StateDiagram {
            direction: None,
            states: vec![state("A", StateKind::Simple)],
            root: StateScope {
                states: vec![StateId(0)],
                transitions: vec![
                    transition(StateEndpoint::Initial, StateEndpoint::State(StateId(0))),
                    transition(StateEndpoint::State(StateId(0)), StateEndpoint::Final),
                ],
                ..StateScope::default()
            },
        };
        let plan = Plan::of(&diagram);
        assert!(plan.slots.contains(&Slot::Start));
        assert!(plan.slots.contains(&Slot::End));
        assert_eq!(plan.edges.len(), 2);
    }

    #[test]
    fn a_scope_without_markers_allocates_none() {
        let diagram = StateDiagram {
            direction: None,
            states: vec![state("A", StateKind::Simple), state("B", StateKind::Simple)],
            root: StateScope {
                states: vec![StateId(0), StateId(1)],
                transitions: vec![transition(
                    StateEndpoint::State(StateId(0)),
                    StateEndpoint::State(StateId(1)),
                )],
                ..StateScope::default()
            },
        };
        let plan = Plan::of(&diagram);
        assert!(!plan.slots.contains(&Slot::Start));
        assert!(!plan.slots.contains(&Slot::End));
    }

    #[test]
    fn a_composite_becomes_a_container_with_its_own_marker() {
        let inner = StateScope {
            states: vec![StateId(1)],
            transitions: vec![transition(
                StateEndpoint::Initial,
                StateEndpoint::State(StateId(1)),
            )],
            ..StateScope::default()
        };
        let diagram = StateDiagram {
            direction: None,
            states: vec![
                state("Outer", StateKind::Composite(inner)),
                state("Inner", StateKind::Simple),
            ],
            root: StateScope {
                states: vec![StateId(0)],
                transitions: Vec::new(),
                ..StateScope::default()
            },
        };
        let plan = Plan::of(&diagram);
        assert_eq!(plan.root.children.len(), 1, "the composite is a container");
        assert_eq!(
            plan.root.children[0].title.as_deref(),
            Some(["Outer".to_string()].as_slice())
        );
        assert!(plan.root.nodes.is_empty(), "nothing sits beside the frame");
    }

    #[test]
    fn a_transition_into_a_composite_lands_on_its_start_marker() {
        let inner = StateScope {
            states: vec![StateId(2)],
            transitions: vec![transition(
                StateEndpoint::Initial,
                StateEndpoint::State(StateId(2)),
            )],
            ..StateScope::default()
        };
        let diagram = StateDiagram {
            direction: None,
            states: vec![
                state("Before", StateKind::Simple),
                state("Outer", StateKind::Composite(inner)),
                state("Inner", StateKind::Simple),
            ],
            root: StateScope {
                states: vec![StateId(0), StateId(1)],
                transitions: vec![transition(
                    StateEndpoint::State(StateId(0)),
                    StateEndpoint::State(StateId(1)),
                )],
                ..StateScope::default()
            },
        };
        let plan = Plan::of(&diagram);
        let edge = plan
            .edges
            .iter()
            .find(|edge| plan.slots[edge.from.0] == Slot::State(StateId(0)))
            .expect("the outer transition");
        assert_eq!(plan.slots[edge.to.0], Slot::Start, "enters the composite");
    }

    #[test]
    fn an_empty_composite_is_demoted_to_a_plain_state() {
        let diagram = StateDiagram {
            direction: None,
            states: vec![state("Hollow", StateKind::Composite(StateScope::default()))],
            root: StateScope {
                states: vec![StateId(0)],
                ..StateScope::default()
            },
        };
        let plan = Plan::of(&diagram);
        assert!(plan.root.children.is_empty());
        assert_eq!(plan.slots, vec![Slot::State(StateId(0))]);
    }

    #[test]
    fn a_note_is_declared_on_the_side_it_asked_for() {
        use crate::mermaid::ast::NotePlacement;
        let build = |placement| {
            let diagram = StateDiagram {
                direction: None,
                states: vec![state("A", StateKind::Simple)],
                root: StateScope {
                    states: vec![StateId(0)],
                    notes: vec![StateNote {
                        placement,
                        target: StateId(0),
                        text: Label::line("careful"),
                    }],
                    ..StateScope::default()
                },
            };
            let plan = Plan::of(&diagram);
            let state_at = plan
                .root
                .nodes
                .iter()
                .position(|&n| plan.slots[n.0] == Slot::State(StateId(0)))
                .expect("the state");
            let note_at = plan
                .root
                .nodes
                .iter()
                .position(|&n| matches!(plan.slots[n.0], Slot::Note(_)))
                .expect("the note");
            (state_at, note_at)
        };

        let (state_at, note_at) = build(NotePlacement::LeftOf);
        assert!(note_at < state_at, "a left note is declared first");

        let (state_at, note_at) = build(NotePlacement::RightOf);
        assert!(note_at > state_at, "a right note is declared after");
    }

    #[test]
    fn a_note_is_a_node_tied_to_its_state_by_a_dotted_line() {
        use crate::mermaid::ast::NotePlacement;
        let diagram = StateDiagram {
            direction: None,
            states: vec![state("A", StateKind::Simple)],
            root: StateScope {
                states: vec![StateId(0)],
                notes: vec![StateNote {
                    placement: NotePlacement::LeftOf,
                    target: StateId(0),
                    text: Label::line("careful"),
                }],
                ..StateScope::default()
            },
        };
        let plan = Plan::of(&diagram);
        assert_eq!(plan.slots.len(), 2);
        assert_eq!(plan.edges.len(), 1);
        assert_eq!(plan.edges[0].stroke, Stroke::Dotted);
        assert_eq!(plan.edges[0].head, Terminator::None);
    }

    #[test]
    fn a_state_shows_its_description_rather_than_its_key() {
        let mut node = state("s1", StateKind::Simple);
        node.label = Some(Label::line("Waiting for input"));
        assert_eq!(
            label_lines(&node, 40),
            vec!["Waiting for input".to_string()]
        );
        let plain = state("s2", StateKind::Simple);
        assert_eq!(label_lines(&plain, 40), vec!["s2".to_string()]);
    }

    #[test]
    fn translation_is_deterministic() {
        let diagram = StateDiagram {
            direction: None,
            states: vec![state("A", StateKind::Simple), state("B", StateKind::Simple)],
            root: StateScope {
                states: vec![StateId(0), StateId(1)],
                transitions: vec![
                    transition(StateEndpoint::Initial, StateEndpoint::State(StateId(0))),
                    transition(
                        StateEndpoint::State(StateId(0)),
                        StateEndpoint::State(StateId(1)),
                    ),
                ],
                ..StateScope::default()
            },
        };
        let first = Plan::of(&diagram);
        for _ in 0..5 {
            let again = Plan::of(&diagram);
            assert_eq!(first.slots, again.slots);
            assert_eq!(first.edges, again.edges);
        }
    }
}
