//! Snapshot tests of state-diagram layout (design spec §6.7).
//!
//! Every case is a hand-built [`StateDiagram`], so a change in `mermaid::parse` can
//! never silently rewrite what these snapshots check.

use mdless::mermaid::ast::{
    Direction, Label, NotePlacement, StateDiagram, StateEndpoint, StateId, StateKind, StateNode,
    StateNote, StateScope, Transition,
};
use mdless::mermaid::layout::state;
use mdless::theme::Theme;

/// A state with a key and a kind.
fn state(key: &str, kind: StateKind) -> StateNode {
    StateNode {
        key: key.to_string(),
        label: None,
        kind,
    }
}

/// A described state, shown by its description rather than its key.
fn described(key: &str, text: &str) -> StateNode {
    StateNode {
        key: key.to_string(),
        label: Some(Label::line(text)),
        kind: StateKind::Simple,
    }
}

/// A transition between two endpoints.
fn go(from: StateEndpoint, to: StateEndpoint) -> Transition {
    Transition {
        from,
        to,
        label: None,
    }
}

/// A labelled transition.
fn go_labelled(from: StateEndpoint, to: StateEndpoint, label: &str) -> Transition {
    Transition {
        from,
        to,
        label: Some(Label::line(label)),
    }
}

/// Shorthand for a named endpoint.
fn at(id: usize) -> StateEndpoint {
    StateEndpoint::State(StateId(id))
}

/// Renders a diagram to plain text for snapshotting.
fn render(diagram: &StateDiagram, width: u16) -> String {
    let theme = Theme::default_dark();
    let canvas = state::draw(diagram, width, &theme).expect("diagram fits");
    assert_eq!(canvas.width(), width, "canvas is exactly the width budget");
    canvas.check_invariants().expect("canvas contract holds");
    canvas.plain_text()
}

/// A diagram with the given states and a root scope.
fn diagram(states: Vec<StateNode>, root: StateScope) -> StateDiagram {
    StateDiagram {
        direction: None,
        states,
        root,
    }
}

#[test]
fn a_start_marker_a_state_and_an_end_marker() {
    let chart = diagram(
        vec![state("Running", StateKind::Simple)],
        StateScope {
            states: vec![StateId(0)],
            transitions: vec![
                go(StateEndpoint::Initial, at(0)),
                go(at(0), StateEndpoint::Final),
            ],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 50));
}

#[test]
fn a_labelled_transition_chain() {
    let chart = diagram(
        vec![
            state("Idle", StateKind::Simple),
            state("Working", StateKind::Simple),
            state("Done", StateKind::Simple),
        ],
        StateScope {
            states: vec![StateId(0), StateId(1), StateId(2)],
            transitions: vec![
                go(StateEndpoint::Initial, at(0)),
                go_labelled(at(0), at(1), "start"),
                go_labelled(at(1), at(2), "finish"),
                go(at(2), StateEndpoint::Final),
            ],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn a_choice_state_branches() {
    let chart = diagram(
        vec![
            state("Check", StateKind::Simple),
            state("pick", StateKind::Choice),
            state("Yes", StateKind::Simple),
            state("No", StateKind::Simple),
        ],
        StateScope {
            states: vec![StateId(0), StateId(1), StateId(2), StateId(3)],
            transitions: vec![
                go(at(0), at(1)),
                go_labelled(at(1), at(2), "ok"),
                go_labelled(at(1), at(3), "bad"),
            ],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn a_fork_and_a_join() {
    let chart = diagram(
        vec![
            state("split", StateKind::Fork),
            state("Left", StateKind::Simple),
            state("Right", StateKind::Simple),
            state("merge", StateKind::Join),
        ],
        StateScope {
            states: vec![StateId(0), StateId(1), StateId(2), StateId(3)],
            transitions: vec![
                go(at(0), at(1)),
                go(at(0), at(2)),
                go(at(1), at(3)),
                go(at(2), at(3)),
            ],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn a_composite_state_with_its_own_start_marker() {
    let inner = StateScope {
        states: vec![StateId(2), StateId(3)],
        transitions: vec![go(StateEndpoint::Initial, at(2)), go(at(2), at(3))],
        ..StateScope::default()
    };
    let chart = diagram(
        vec![
            state("Off", StateKind::Simple),
            state("On", StateKind::Composite(inner)),
            state("Warming", StateKind::Simple),
            state("Hot", StateKind::Simple),
        ],
        StateScope {
            states: vec![StateId(0), StateId(1)],
            transitions: vec![
                go(StateEndpoint::Initial, at(0)),
                go_labelled(at(0), at(1), "on"),
            ],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 70));
}

#[test]
fn nested_composite_states() {
    let innermost = StateScope {
        states: vec![StateId(3)],
        transitions: vec![go(StateEndpoint::Initial, at(3))],
        ..StateScope::default()
    };
    // Outer holds Middle, Middle holds Leaf: two levels of nesting.
    let middle = StateScope {
        states: vec![StateId(2)],
        ..StateScope::default()
    };
    let chart = diagram(
        vec![
            state("Outer", StateKind::Composite(middle)),
            state("unused", StateKind::Simple),
            state("Middle", StateKind::Composite(innermost)),
            state("Leaf", StateKind::Simple),
        ],
        StateScope {
            states: vec![StateId(0)],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 70));
}

#[test]
fn a_note_beside_a_state() {
    let chart = diagram(
        vec![state("Armed", StateKind::Simple)],
        StateScope {
            states: vec![StateId(0)],
            transitions: vec![go(StateEndpoint::Initial, at(0))],
            notes: vec![StateNote {
                placement: NotePlacement::RightOf,
                target: StateId(0),
                text: Label::line("only from the console"),
            }],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 70));
}

#[test]
fn a_described_state_shows_its_description() {
    let chart = diagram(
        vec![described("s1", "Waiting for the user")],
        StateScope {
            states: vec![StateId(0)],
            transitions: vec![go(StateEndpoint::Initial, at(0))],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 50));
}

#[test]
fn a_self_transition() {
    let chart = diagram(
        vec![state("Polling", StateKind::Simple)],
        StateScope {
            states: vec![StateId(0)],
            transitions: vec![go_labelled(at(0), at(0), "tick")],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 50));
}

#[test]
fn a_cycle_between_two_states() {
    let chart = diagram(
        vec![
            state("Open", StateKind::Simple),
            state("Closed", StateKind::Simple),
        ],
        StateScope {
            states: vec![StateId(0), StateId(1)],
            transitions: vec![
                go_labelled(at(0), at(1), "close"),
                go_labelled(at(1), at(0), "open"),
            ],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 50));
}

#[test]
fn a_left_to_right_state_diagram() {
    let mut chart = diagram(
        vec![state("A", StateKind::Simple), state("B", StateKind::Simple)],
        StateScope {
            states: vec![StateId(0), StateId(1)],
            transitions: vec![
                go(StateEndpoint::Initial, at(0)),
                go_labelled(at(0), at(1), "next"),
                go(at(1), StateEndpoint::Final),
            ],
            ..StateScope::default()
        },
    );
    chart.direction = Some(Direction::LeftToRight);
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn a_single_state_and_nothing_else() {
    insta::assert_snapshot!(render(
        &diagram(
            vec![state("Alone", StateKind::Simple)],
            StateScope {
                states: vec![StateId(0)],
                ..StateScope::default()
            },
        ),
        30
    ));
}

#[test]
fn an_empty_diagram() {
    insta::assert_snapshot!(render(&diagram(Vec::new(), StateScope::default()), 30));
}

#[test]
fn cjk_state_labels() {
    let chart = diagram(
        vec![described("s1", "待機中"), described("s2", "実行中🏃")],
        StateScope {
            states: vec![StateId(0), StateId(1)],
            transitions: vec![go_labelled(at(0), at(1), "開始")],
            ..StateScope::default()
        },
    );
    insta::assert_snapshot!(render(&chart, 50));
}
