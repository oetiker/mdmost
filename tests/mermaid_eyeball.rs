//! Temporary visual harness. Run with `cargo test --test mermaid_eyeball -- --nocapture`.

use mdless::mermaid::ast::*;
use mdless::mermaid::{gantt, pie, sequence};
use mdless::theme::Theme;

fn day(y: i64, m: u32, d: u32) -> i64 {
    mdless::mermaid::gantt::time::days_from_civil(y, m, d) * 86_400
}

#[test]
fn eyeball() {
    let theme = Theme::default_dark();

    let chart = PieChart {
        title: Some("Where the votes went".into()),
        show_data: true,
        slices: vec![
            PieSlice {
                label: "Cats".into(),
                value: 245.0,
            },
            PieSlice {
                label: "Dogs".into(),
                value: 170.0,
            },
            PieSlice {
                label: "Birds and other feathered friends".into(),
                value: 106.0,
            },
            PieSlice {
                label: "Other".into(),
                value: 74.0,
            },
        ],
    };
    for width in [40u16, 80, 120] {
        println!("=== pie @{width}");
        match pie::draw(&chart, width, &theme) {
            Ok(canvas) => {
                println!("{}", canvas.plain_text());
                assert_eq!(canvas.check_invariants(), Ok(()));
            }
            Err(err) => println!("error: {err}"),
        }
    }

    let gantt_chart = GanttChart {
        title: Some("Release plan".into()),
        axis_format: None,
        sections: vec![
            GanttSection {
                title: Some("Design".into()),
                tasks: vec![
                    GanttTask {
                        name: "Spec".into(),
                        id: Some("spec".into()),
                        progress: TaskProgress::Done,
                        critical: false,
                        milestone: false,
                        start: day(2024, 1, 1),
                        end: day(2024, 1, 9),
                    },
                    GanttTask {
                        name: "Review".into(),
                        id: None,
                        progress: TaskProgress::Active,
                        critical: false,
                        milestone: false,
                        start: day(2024, 1, 9),
                        end: day(2024, 1, 15),
                    },
                ],
            },
            GanttSection {
                title: Some("Build".into()),
                tasks: vec![
                    GanttTask {
                        name: "Core engine".into(),
                        id: None,
                        progress: TaskProgress::Planned,
                        critical: true,
                        milestone: false,
                        start: day(2024, 1, 15),
                        end: day(2024, 2, 12),
                    },
                    GanttTask {
                        name: "Ship".into(),
                        id: None,
                        progress: TaskProgress::Planned,
                        critical: false,
                        milestone: true,
                        start: day(2024, 2, 12),
                        end: day(2024, 2, 12),
                    },
                ],
            },
        ],
    };
    for width in [40u16, 80, 120] {
        println!("=== gantt @{width}");
        match gantt::draw(&gantt_chart, width, &theme) {
            Ok(canvas) => {
                println!("{}", canvas.plain_text());
                assert_eq!(canvas.check_invariants(), Ok(()));
            }
            Err(err) => println!("error: {err}"),
        }
    }

    let seq = SequenceDiagram {
        title: Some("Checkout".into()),
        participants: vec![
            Participant {
                key: "U".into(),
                label: Label::line("User"),
                kind: ParticipantKind::Actor,
            },
            Participant {
                key: "S".into(),
                label: Label::line("Shop"),
                kind: ParticipantKind::Participant,
            },
            Participant {
                key: "P".into(),
                label: Label::line("Payments"),
                kind: ParticipantKind::Participant,
            },
        ],
        items: vec![
            SequenceItem::Message(Message {
                from: ParticipantId(0),
                to: ParticipantId(1),
                line: MessageLine::Solid,
                head: MessageHead::Arrow,
                label: Label::line("place order"),
                activates: true,
                deactivates: false,
            }),
            SequenceItem::Note(Note {
                placement: NotePlacement::RightOf,
                participants: vec![ParticipantId(1)],
                text: Label::line("validate cart"),
            }),
            SequenceItem::Block(SequenceBlock {
                kind: BlockKind::Alt,
                branches: vec![
                    Branch {
                        label: Some(Label::line("in stock")),
                        items: vec![
                            SequenceItem::Message(Message {
                                from: ParticipantId(1),
                                to: ParticipantId(2),
                                line: MessageLine::Solid,
                                head: MessageHead::Arrow,
                                label: Label::line("charge"),
                                activates: true,
                                deactivates: false,
                            }),
                            SequenceItem::Message(Message {
                                from: ParticipantId(2),
                                to: ParticipantId(2),
                                line: MessageLine::Solid,
                                head: MessageHead::Arrow,
                                label: Label::line("risk check"),
                                activates: false,
                                deactivates: false,
                            }),
                            SequenceItem::Message(Message {
                                from: ParticipantId(2),
                                to: ParticipantId(1),
                                line: MessageLine::Dotted,
                                head: MessageHead::Arrow,
                                label: Label::line("receipt"),
                                activates: false,
                                deactivates: true,
                            }),
                        ],
                    },
                    Branch {
                        label: Some(Label::line("sold out")),
                        items: vec![SequenceItem::Message(Message {
                            from: ParticipantId(1),
                            to: ParticipantId(0),
                            line: MessageLine::Dotted,
                            head: MessageHead::Cross,
                            label: Label::line("sorry"),
                            activates: false,
                            deactivates: false,
                        })],
                    },
                ],
            }),
            SequenceItem::Note(Note {
                placement: NotePlacement::Over,
                participants: vec![ParticipantId(0), ParticipantId(2)],
                text: Label::line("done"),
            }),
            SequenceItem::Message(Message {
                from: ParticipantId(1),
                to: ParticipantId(0),
                line: MessageLine::Solid,
                head: MessageHead::Arrow,
                label: Label::line("confirmation"),
                activates: false,
                deactivates: true,
            }),
        ],
    };
    for width in [40u16, 80, 120] {
        println!("=== seq @{width}");
        match sequence::draw(&seq, width, &theme) {
            Ok(canvas) => {
                println!("{}", canvas.plain_text());
                assert_eq!(canvas.check_invariants(), Ok(()));
            }
            Err(err) => println!("error: {err}"),
        }
    }
}
