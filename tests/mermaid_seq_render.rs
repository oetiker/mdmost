// SPDX-License-Identifier: MIT
//! Rendering tests for `sequenceDiagram` (design spec §6.2).
//!
//! Snapshots are named `<fixture>@<width>` and are reviewed with `cargo insta review`.

use mdmost::canvas::Canvas;
use mdmost::mermaid::ast::BlockKind;
use mdmost::mermaid::ast::{
    Branch, Label, Message, MessageHead, MessageLine, Note, NotePlacement, Participant,
    ParticipantId, ParticipantKind, SequenceBlock, SequenceDiagram, SequenceItem,
};
use mdmost::mermaid::sequence;
use mdmost::theme::Theme;
use proptest::prelude::*;

/// The widths every fixture is rendered at.
const WIDTHS: [u16; 3] = [40, 80, 120];

/// A boxed participant.
fn participant(key: &str) -> Participant {
    Participant {
        key: key.to_string(),
        label: Label::line(key),
        kind: ParticipantKind::Participant,
    }
}

/// A stick-figure actor.
fn actor(key: &str) -> Participant {
    Participant {
        kind: ParticipantKind::Actor,
        ..participant(key)
    }
}

/// A message with a solid shaft and a filled arrowhead.
fn message(from: usize, to: usize, text: &str) -> SequenceItem {
    SequenceItem::Message(Message {
        from: ParticipantId(from),
        to: ParticipantId(to),
        line: MessageLine::Solid,
        head: MessageHead::Arrow,
        label: Label::line(text),
        activates: false,
        deactivates: false,
    })
}

/// A diagram from participants and body items.
fn diagram(
    title: Option<&str>,
    participants: Vec<Participant>,
    items: Vec<SequenceItem>,
) -> SequenceDiagram {
    SequenceDiagram {
        title: title.map(str::to_string),
        participants,
        items,
    }
}

/// Renders at every snapshot width, checking the canvas contract each time.
fn snapshot(name: &str, diagram: &SequenceDiagram) {
    let theme = Theme::default_dark();
    for width in WIDTHS {
        let canvas = sequence::draw(diagram, width, &theme).expect("diagram fits");
        assert_contract(&canvas, width);
        insta::assert_snapshot!(format!("{name}@{width}"), canvas.plain_text());
    }
}

/// Asserts the canvas contract: exactly `width` columns on every row.
fn assert_contract(canvas: &Canvas, width: u16) {
    assert_eq!(canvas.width(), width);
    assert_eq!(canvas.check_invariants(), Ok(()));
    for row in 0..canvas.height() {
        assert_eq!(
            mdmost::text::display_width(&canvas.row_text(row)),
            usize::from(width),
            "row {row} is not {width} columns wide"
        );
    }
}

#[test]
fn two_participants_exchange_messages() {
    snapshot(
        "hello",
        &diagram(
            Some("Greeting"),
            vec![participant("Alice"), participant("Bob")],
            vec![
                message(0, 1, "hello"),
                SequenceItem::Message(Message {
                    from: ParticipantId(1),
                    to: ParticipantId(0),
                    line: MessageLine::Dotted,
                    head: MessageHead::Arrow,
                    label: Label::line("hi there"),
                    activates: false,
                    deactivates: false,
                }),
            ],
        ),
    );
}

#[test]
fn every_arrow_form_is_distinguishable() {
    let mut items = Vec::new();
    for (line, head, text) in [
        (MessageLine::Solid, MessageHead::None, "solid open"),
        (MessageLine::Dotted, MessageHead::None, "dotted open"),
        (MessageLine::Solid, MessageHead::Arrow, "solid arrow"),
        (MessageLine::Dotted, MessageHead::Arrow, "dotted arrow"),
        (MessageLine::Solid, MessageHead::Cross, "solid cross"),
        (MessageLine::Dotted, MessageHead::Cross, "dotted cross"),
    ] {
        items.push(SequenceItem::Message(Message {
            from: ParticipantId(0),
            to: ParticipantId(1),
            line,
            head,
            label: Label::line(text),
            activates: false,
            deactivates: false,
        }));
    }
    snapshot(
        "arrow_forms",
        &diagram(None, vec![participant("A"), participant("B")], items),
    );
}

#[test]
fn actors_and_boxes_stand_side_by_side() {
    snapshot(
        "actors",
        &diagram(
            None,
            vec![actor("Customer"), participant("Till"), actor("Manager")],
            vec![message(0, 1, "pay"), message(1, 2, "escalate")],
        ),
    );
}

#[test]
fn self_messages_hook_back_to_their_own_lifeline() {
    snapshot(
        "self_messages",
        &diagram(
            None,
            vec![participant("A"), participant("B")],
            vec![
                message(0, 0, "think"),
                message(0, 1, "ask"),
                message(1, 1, "look it up"),
                message(1, 0, "answer"),
            ],
        ),
    );
}

#[test]
fn activations_nest_and_stray_deactivations_are_ignored() {
    snapshot(
        "activations",
        &diagram(
            None,
            vec![participant("Client"), participant("Server")],
            vec![
                SequenceItem::Message(Message {
                    from: ParticipantId(0),
                    to: ParticipantId(1),
                    line: MessageLine::Solid,
                    head: MessageHead::Arrow,
                    label: Label::line("request"),
                    activates: true,
                    deactivates: false,
                }),
                SequenceItem::Activate(ParticipantId(1)),
                message(1, 1, "work"),
                SequenceItem::Deactivate(ParticipantId(1)),
                SequenceItem::Message(Message {
                    from: ParticipantId(1),
                    to: ParticipantId(0),
                    line: MessageLine::Dotted,
                    head: MessageHead::Arrow,
                    label: Label::line("response"),
                    activates: false,
                    deactivates: true,
                }),
                // A close with nothing open must be ignored, not panic.
                SequenceItem::Deactivate(ParticipantId(0)),
            ],
        ),
    );
}

#[test]
fn notes_sit_left_right_and_over() {
    snapshot(
        "notes",
        &diagram(
            None,
            vec![participant("A"), participant("B"), participant("C")],
            vec![
                SequenceItem::Note(Note {
                    placement: NotePlacement::LeftOf,
                    participants: vec![ParticipantId(0)],
                    text: Label::line("pre"),
                }),
                message(0, 1, "go"),
                SequenceItem::Note(Note {
                    placement: NotePlacement::RightOf,
                    participants: vec![ParticipantId(1)],
                    text: Label::line("busy"),
                }),
                SequenceItem::Note(Note {
                    placement: NotePlacement::Over,
                    participants: vec![ParticipantId(2), ParticipantId(0)],
                    text: Label::parse("over all<br>three"),
                }),
                SequenceItem::Note(Note {
                    placement: NotePlacement::RightOf,
                    participants: vec![ParticipantId(2)],
                    text: Label::line("end"),
                }),
                message(1, 2, "done"),
            ],
        ),
    );
}

#[test]
fn blocks_nest_and_enclose_their_contents() {
    snapshot(
        "blocks",
        &diagram(
            Some("Every frame"),
            vec![participant("A"), participant("B")],
            vec![
                SequenceItem::Block(SequenceBlock {
                    kind: BlockKind::Loop,
                    branches: vec![Branch {
                        label: Some(Label::line("3 times")),
                        items: vec![
                            message(0, 1, "poll"),
                            SequenceItem::Block(SequenceBlock {
                                kind: BlockKind::Alt,
                                branches: vec![
                                    Branch {
                                        label: Some(Label::line("ready")),
                                        items: vec![message(1, 0, "data")],
                                    },
                                    Branch {
                                        label: Some(Label::line("not yet")),
                                        items: vec![message(1, 0, "wait")],
                                    },
                                ],
                            }),
                        ],
                    }],
                }),
                SequenceItem::Block(SequenceBlock {
                    kind: BlockKind::Opt,
                    branches: vec![Branch {
                        label: None,
                        items: vec![message(0, 1, "cleanup")],
                    }],
                }),
                SequenceItem::Block(SequenceBlock {
                    kind: BlockKind::Par,
                    branches: vec![
                        Branch {
                            label: Some(Label::line("one")),
                            items: vec![message(0, 1, "a")],
                        },
                        Branch {
                            label: Some(Label::line("two")),
                            items: vec![message(0, 1, "b")],
                        },
                    ],
                }),
                SequenceItem::Block(SequenceBlock {
                    kind: BlockKind::Critical,
                    branches: vec![
                        Branch {
                            label: Some(Label::line("connect")),
                            items: vec![message(0, 1, "open")],
                        },
                        Branch {
                            label: Some(Label::line("offline")),
                            items: vec![message(0, 1, "queue")],
                        },
                    ],
                }),
            ],
        ),
    );
}

#[test]
fn a_single_participant_still_draws() {
    snapshot(
        "one_participant",
        &diagram(
            None,
            vec![participant("Solo")],
            vec![
                message(0, 0, "muse"),
                SequenceItem::Note(Note {
                    placement: NotePlacement::Over,
                    participants: vec![ParticipantId(0)],
                    text: Label::line("alone"),
                }),
            ],
        ),
    );
}

#[test]
fn no_participants_degrades_to_a_placeholder() {
    snapshot("empty", &diagram(Some("Nothing here"), vec![], vec![]));
}

#[test]
fn long_labels_degrade_by_truncation_not_overflow() {
    snapshot(
        "long_labels",
        &diagram(
            None,
            vec![
                Participant {
                    label: Label::line("A participant with a very long name indeed"),
                    ..participant("A")
                },
                participant("B"),
            ],
            vec![message(
                0,
                1,
                "a message label that is much too long for a narrow terminal",
            )],
        ),
    );
}

#[test]
fn wide_and_zero_width_clusters_survive() {
    snapshot(
        "cjk_emoji",
        &diagram(
            Some("多言語"),
            vec![
                Participant {
                    label: Label::line("利用者 🚀"),
                    ..actor("U")
                },
                Participant {
                    label: Label::line("サーバ"),
                    ..participant("S")
                },
            ],
            vec![message(0, 1, "こんにちは"), message(1, 0, "café\u{301} ✓")],
        ),
    );
}

#[test]
fn a_diagram_too_wide_for_the_budget_reports_it() {
    let wide = diagram(
        None,
        (0..12).map(|i| participant(&format!("P{i}"))).collect(),
        vec![],
    );
    assert!(sequence::draw(&wide, 20, &Theme::default_dark()).is_err());
}

#[test]
fn rendering_is_deterministic() {
    let theme = Theme::default_dark();
    let subject = diagram(
        None,
        vec![participant("A"), participant("B"), participant("C")],
        vec![message(0, 2, "x"), message(2, 1, "y"), message(1, 1, "z")],
    );
    let first = sequence::draw(&subject, 80, &theme).expect("diagram fits");
    for _ in 0..5 {
        assert_eq!(
            sequence::draw(&subject, 80, &theme).expect("diagram fits"),
            first
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Whatever the messages and the width, drawing never panics and never breaks the
    /// canvas contract.
    #[test]
    fn never_panics_and_always_fills_the_width(
        count in 1usize..6,
        pairs in prop::collection::vec((0usize..8, 0usize..8, "[a-z ]{0,20}"), 0..14),
        width in 4u16..200,
    ) {
        let participants: Vec<Participant> = (0..count)
            .map(|index| if index % 3 == 0 { actor(&format!("p{index}")) } else { participant(&format!("p{index}")) })
            .collect();
        let items: Vec<SequenceItem> = pairs
            .iter()
            .map(|(from, to, text)| message(from % count, to % count, text))
            .collect();
        let subject = diagram(None, participants, items);
        if let Ok(canvas) = sequence::draw(&subject, width, &Theme::default_dark()) {
            prop_assert_eq!(canvas.width(), width);
            prop_assert_eq!(canvas.check_invariants(), Ok(()));
        }
    }
}
