// SPDX-License-Identifier: MIT
//! Parser acceptance tests, one module per Mermaid family (design spec §6.1–§6.7).
//!
//! The sources are taken from Mermaid's own documentation examples, so a passing test
//! means real-world diagrams parse, not just hand-tailored ones.

use mdmost::mermaid::ast::*;
use mdmost::mermaid::parse::parse;

/// Parses `src`, failing the test with the parser's reason when it does not.
#[track_caller]
fn ok(src: &str) -> Diagram {
    match parse(src) {
        Ok(diagram) => diagram,
        Err(error) => panic!("expected a diagram, got: {error}"),
    }
}

/// The flowchart in `src`.
#[track_caller]
fn flowchart(src: &str) -> Flowchart {
    match ok(src) {
        Diagram::Flowchart(chart) => chart,
        other => panic!("expected a flowchart, got {other:?}"),
    }
}

/// Looks a node up by key.
#[track_caller]
fn node<'a>(chart: &'a Flowchart, key: &str) -> &'a FlowNode {
    match chart.nodes.iter().find(|node| node.key == key) {
        Some(node) => node,
        None => panic!("no node `{key}` in {:?}", chart.nodes),
    }
}

/// A paint's colours and weight, without the source offsets that order slots.
fn colours(paint: Option<Paint>) -> Option<(Option<u32>, Option<u32>, bool)> {
    let rgb = |c: Option<PaintColor>| c.map(|c| u32::from_be_bytes([0, c.rgb.r, c.rgb.g, c.rgb.b]));
    paint.map(|p| (rgb(p.fill), rgb(p.stroke), p.heavy))
}

mod flowcharts {
    use super::*;

    /// The colour spec's driving example (§1).
    #[test]
    fn reads_classdef_class_and_style() {
        let chart = flowchart(
            "flowchart TD\n  airlock --> zimbra --> zmcfgapi --> mbox\n\
             classDef access fill:#e3f4fb,stroke:#2a8bb5,color:#000\n\
             classDef comm fill:#fdf0e1,stroke:#d4831f,color:#000\n\
             class airlock access\n  class zimbra comm\n\
             style mbox fill:#fff7ee,stroke:#b8650a,stroke-width:3px,color:#000\n",
        );
        assert_eq!(
            colours(node(&chart, "airlock").paint),
            Some((Some(0xe3f4fb), Some(0x2a8bb5), false))
        );
        assert_eq!(
            colours(node(&chart, "zimbra").paint),
            Some((Some(0xfdf0e1), Some(0xd4831f), false))
        );
        assert_eq!(node(&chart, "zmcfgapi").paint, None);
        assert_eq!(
            colours(node(&chart, "mbox").paint),
            Some((Some(0xfff7ee), Some(0xb8650a), true))
        );
        assert_eq!(chart.nodes.len(), 4, "colour lines create no node");
    }

    #[test]
    fn merges_default_then_classes_then_styles() {
        let chart = flowchart(
            "flowchart LR\n  A:::one --> B\n  class A two\n  style A stroke:#00ff00\n\
             classDef default fill:#111111,stroke-width:3px\n\
             classDef one fill:#ff0000,stroke:#ff0000\n  classDef two stroke:#0000ff\n",
        );
        assert_eq!(
            colours(node(&chart, "A").paint),
            Some((Some(0xff0000), Some(0x00ff00), true))
        );
        assert_eq!(
            colours(node(&chart, "B").paint),
            Some((Some(0x111111), None, true)),
            "default"
        );
    }

    #[test]
    fn assigns_several_classes_and_several_nodes_at_once() {
        let chart = flowchart(
            "flowchart LR\n  A --> B --> C\n  class A,B one,two\n\
             classDef one fill:#ff0000\n  classDef two stroke:#0000ff\n",
        );
        for key in ["A", "B"] {
            assert_eq!(
                colours(node(&chart, key).paint),
                Some((Some(0xff0000), Some(0x0000ff), false)),
                "{key}"
            );
        }
        assert_eq!(node(&chart, "C").paint, None);
    }

    #[test]
    fn paints_subgraphs_by_key_and_by_suffix_but_not_by_default() {
        let chart = flowchart(
            "flowchart TB\n  subgraph one\n    a\n  end\n  subgraph two:::warm [Two]\n    b\n  end\n\
             subgraph \"Three words\":::warm\n    c\n  end\n\
             style one fill:#e3f4fb\n  classDef warm stroke:#d4831f\n  classDef default stroke:#ff0000\n",
        );
        let groups = &chart.root.children;
        assert_eq!(
            colours(groups[0].paint),
            Some((Some(0xe3f4fb), None, false))
        );
        assert_eq!(
            colours(groups[1].paint),
            Some((None, Some(0xd4831f), false))
        );
        assert_eq!(
            colours(groups[2].paint),
            Some((None, Some(0xd4831f), false)),
            "anonymous"
        );
        assert_eq!(
            colours(node(&chart, "a").paint),
            Some((None, Some(0xff0000), false))
        );
    }

    #[test]
    fn unknown_classes_undeclared_nodes_and_bad_values_are_dropped() {
        let chart = flowchart(
            "flowchart LR\n  A --> B\n  class A nosuch\n  style Z fill:#ff0000\n  class Y one\n\
             classDef one fill:#ff0000\n  style B fill:notacolour,stroke:#12\n",
        );
        assert_eq!(chart.nodes.len(), 2);
        assert_eq!(node(&chart, "A").paint, None);
        assert_eq!(node(&chart, "B").paint, None);
    }

    /// `;` ends a statement, but the splitter keeps `#333;` whole (colour spec §3.4).
    #[test]
    fn a_semicolon_ends_the_property_list() {
        let chart = flowchart(
            "flowchart LR\n  A --> B --> C\n  classDef c fill:#ff99ff,stroke:#333;\n  class A c\n\
             style B fill:#000;stroke:#123\n  style C fill:#e3f4fb;stroke:#2a8bb5\n",
        );
        assert_eq!(
            colours(node(&chart, "A").paint),
            Some((Some(0xff99ff), Some(0x333333), false))
        );
        assert_eq!(
            colours(node(&chart, "B").paint),
            Some((Some(0x000000), None, false))
        );
        assert_eq!(
            colours(node(&chart, "C").paint),
            Some((Some(0xe3f4fb), None, false))
        );
    }

    /// Colour spec Review Focus 2: no colour line can fail a diagram.
    #[test]
    fn colour_lines_with_garbage_never_fail_the_diagram() {
        for line in [
            "classDef",
            "classDef c",
            "classDef ,, fill:#f00",
            "class",
            "class A",
            "class  c",
            "style",
            "style A",
            "style A fill:",
            "style A stroke-width:1e999px",
            "style A fill:#ffffffffff",
            "style A fill:#ﬀ0000",
            "cssClass \"A\" c",
            "classDef é fill:#f00",
            "style A :",
            "classDef c fill:#f00,,,stroke:",
        ] {
            let chart = flowchart(&format!("flowchart LR\n  A --> B\n  {line}\n"));
            assert_eq!(chart.nodes.len(), 2, "{line}");
        }
    }

    #[test]
    fn parses_the_documentation_flowchart() {
        let chart = flowchart(
            r#"flowchart TD
    A[Christmas] -->|Get money| B(Go shopping)
    B --> C{Let me think}
    C -->|One| D[Laptop]
    C -->|Two| E[iPhone]
    C -->|Three| F[Car]
"#,
        );
        assert_eq!(chart.direction, Direction::TopToBottom);
        assert_eq!(chart.nodes.len(), 6);
        assert_eq!(node(&chart, "A").shape, NodeShape::Rect);
        assert_eq!(node(&chart, "A").label, Label::line("Christmas"));
        assert_eq!(node(&chart, "B").shape, NodeShape::Round);
        assert_eq!(node(&chart, "C").shape, NodeShape::Rhombus);
        assert_eq!(chart.edges.len(), 5);
        assert_eq!(chart.edges[0].from, NodeId(0));
        assert_eq!(chart.edges[0].to, NodeId(1));
        assert_eq!(chart.edges[0].head, ArrowHead::Arrow);
        assert_eq!(chart.edges[0].stroke, EdgeStroke::Solid);
        assert_eq!(chart.edges[0].label, Some(Label::line("Get money")));
        assert_eq!(chart.edges[1].label, None);
        assert_eq!(chart.root.nodes.len(), 6);
        assert!(chart.root.children.is_empty());
    }

    #[test]
    fn a_flowchart_node_label_knows_its_source_range() {
        let src = "flowchart LR\n  A[Parse] --> B[Layout]\n";
        let chart = flowchart(src);
        let a = node(&chart, "A");
        assert_eq!(a.label.lines, ["Parse"]);
        assert_eq!(&src[a.label.source.clone()], "Parse");
        let b = node(&chart, "B");
        assert_eq!(&src[b.label.source.clone()], "Layout");
    }

    #[test]
    fn parses_every_direction() {
        for (source, expected) in [
            ("TD", Direction::TopToBottom),
            ("TB", Direction::TopToBottom),
            ("BT", Direction::BottomToTop),
            ("LR", Direction::LeftToRight),
            ("RL", Direction::RightToLeft),
        ] {
            let chart = flowchart(&format!("graph {source}\n  A-->B\n"));
            assert_eq!(chart.direction, expected, "direction {source}");
        }
    }

    #[test]
    fn parses_every_supported_shape() {
        let chart = flowchart(
            "flowchart LR
    a[rect]
    b(round)
    c([stadium])
    d{rhombus}
    e((circle))
    f[[subroutine]]
    g[(cylinder)]
    h{{hexagon}}
    i[/parallelogram/]
",
        );
        assert_eq!(node(&chart, "a").shape, NodeShape::Rect);
        assert_eq!(node(&chart, "b").shape, NodeShape::Round);
        assert_eq!(node(&chart, "c").shape, NodeShape::Stadium);
        assert_eq!(node(&chart, "d").shape, NodeShape::Rhombus);
        assert_eq!(node(&chart, "e").shape, NodeShape::Circle);
        assert_eq!(node(&chart, "f").shape, NodeShape::Subroutine);
        assert_eq!(node(&chart, "g").shape, NodeShape::Cylinder);
        // Unsupported shapes degrade to a rectangle but keep their label.
        assert_eq!(node(&chart, "h").shape, NodeShape::Rect);
        assert_eq!(node(&chart, "h").label, Label::line("hexagon"));
        assert_eq!(node(&chart, "i").shape, NodeShape::Rect);
        assert_eq!(node(&chart, "i").label, Label::line("parallelogram"));
    }

    #[test]
    fn parses_every_link_form() {
        let chart = flowchart(
            "flowchart LR
    A --> B
    A --- C
    A -.-> D
    A ==> E
    A -- text --> F
    A -. dotted .-> G
    A == thick ==> H
    A <--> I
    A -- plain --- J
",
        );
        let strokes: Vec<_> = chart.edges.iter().map(|edge| edge.stroke).collect();
        assert_eq!(
            strokes,
            vec![
                EdgeStroke::Solid,
                EdgeStroke::Solid,
                EdgeStroke::Dotted,
                EdgeStroke::Thick,
                EdgeStroke::Solid,
                EdgeStroke::Dotted,
                EdgeStroke::Thick,
                EdgeStroke::Solid,
                EdgeStroke::Solid,
            ]
        );
        assert_eq!(chart.edges[0].head, ArrowHead::Arrow);
        assert_eq!(chart.edges[1].head, ArrowHead::None);
        assert_eq!(chart.edges[4].label, Some(Label::line("text")));
        assert_eq!(chart.edges[5].label, Some(Label::line("dotted")));
        assert_eq!(chart.edges[6].label, Some(Label::line("thick")));
        assert_eq!(chart.edges[7].tail, ArrowHead::Arrow);
        assert_eq!(chart.edges[7].head, ArrowHead::Arrow);
        assert_eq!(chart.edges[8].label, Some(Label::line("plain")));
        assert_eq!(chart.edges[8].head, ArrowHead::None);
    }

    #[test]
    fn expands_ampersand_groups_into_a_cross_product() {
        let chart = flowchart("flowchart LR\n    a & b --> c & d\n");
        assert_eq!(chart.edges.len(), 4);
        let pairs: Vec<_> = chart
            .edges
            .iter()
            .map(|edge| (edge.from, edge.to))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (NodeId(0), NodeId(2)),
                (NodeId(0), NodeId(3)),
                (NodeId(1), NodeId(2)),
                (NodeId(1), NodeId(3)),
            ]
        );
    }

    #[test]
    fn chains_edges_and_upgrades_nodes_declared_later() {
        let chart = flowchart("graph TD; A-->B-->C; B{Decision}\n");
        assert_eq!(chart.nodes.len(), 3);
        assert_eq!(chart.edges.len(), 2);
        assert_eq!(node(&chart, "B").shape, NodeShape::Rhombus);
        assert_eq!(node(&chart, "B").label, Label::line("Decision"));
    }

    #[test]
    fn parses_nested_subgraphs() {
        let chart = flowchart(
            "flowchart TB
    c1-->a2
    subgraph ide1 [one]
        a1-->a2
        subgraph inner
            direction LR
            i1 --> i2
        end
    end
    subgraph two
        b1-->b2
    end
",
        );
        assert_eq!(chart.root.children.len(), 2);
        let one = &chart.root.children[0];
        assert_eq!(one.key.as_deref(), Some("ide1"));
        assert_eq!(one.title, Some(Label::line("one")));
        assert_eq!(one.children.len(), 1);
        let inner = &one.children[0];
        assert_eq!(inner.key.as_deref(), Some("inner"));
        assert_eq!(inner.direction, Some(Direction::LeftToRight));
        assert_eq!(inner.nodes.len(), 2);
        // `c1` and `a2` are first mentioned outside any subgraph.
        assert_eq!(chart.root.nodes.len(), 2);
        assert_eq!(chart.root.children[1].key.as_deref(), Some("two"));
    }

    #[test]
    fn turns_br_markup_into_label_lines() {
        let chart = flowchart("flowchart LR\n  A[\"first<br/>second\"] --> B\n");
        assert_eq!(
            node(&chart, "A").label.lines,
            vec!["first".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn ignores_comments_directives_and_styling() {
        let chart = flowchart(
            "%%{init: {'theme': 'dark'} }%%
flowchart LR
    %% a comment
    A --> B
    style A fill:#f9f
    classDef big font-size:20px
    click A callback
    linkStyle 0 stroke:#333
",
        );
        assert_eq!(chart.nodes.len(), 2);
        assert_eq!(chart.edges.len(), 1);
    }

    /// `:::name` may follow a node reference wherever one stands (colour spec §3.3).
    /// Until colours are drawn the class is read and dropped, so the diagram must parse
    /// exactly as if the suffix were not there.
    #[test]
    fn reads_a_class_suffix_wherever_a_node_stands() {
        let chart = flowchart(
            "flowchart LR
    A:::foo --> B[Box]:::bar
    C:::c & D(Round):::d --> E:::e
    F:::f
    G[\"keeps:::this\"] -- label --> H:::h
",
        );
        let keys: Vec<_> = chart.nodes.iter().map(|node| node.key.as_str()).collect();
        assert_eq!(keys, vec!["A", "B", "C", "D", "E", "F", "G", "H"]);
        assert_eq!(node(&chart, "B").shape, NodeShape::Rect);
        assert_eq!(node(&chart, "B").label, Label::line("Box"));
        assert_eq!(node(&chart, "D").shape, NodeShape::Round);
        assert_eq!(node(&chart, "D").label, Label::line("Round"));
        assert_eq!(node(&chart, "G").label, Label::line("keeps:::this"));
        assert_eq!(chart.edges.len(), 4);
    }

    /// `subgraph one:::name` attaches a class to the subgraph; it is not part of the
    /// key or the title (colour spec §3.3).
    #[test]
    fn reads_a_class_suffix_on_a_subgraph() {
        let chart = flowchart(
            "flowchart LR
    subgraph one:::foo
        a
    end
    subgraph two:::foo [Second]
        b
    end
    subgraph three[Third]:::foo
        c
    end
    subgraph \"Fourth one\":::foo
        d
    end
",
        );
        let groups: Vec<_> = chart
            .root
            .children
            .iter()
            .map(|group| (group.key.as_deref(), group.title.as_ref().map(Label::text)))
            .collect();
        assert_eq!(
            groups,
            vec![
                (Some("one"), Some("one".to_string())),
                (Some("two"), Some("Second".to_string())),
                (Some("three"), Some("Third".to_string())),
                (None, Some("Fourth one".to_string())),
            ]
        );
    }

    /// `;` ends a statement, so `stroke:#2a8bb5` after a `style` line's `;` is a stray
    /// CSS fragment, not a node (colour spec §3.4).
    #[test]
    fn drops_css_fragments_cut_off_a_styling_statement() {
        let chart = flowchart(
            "flowchart LR
    A --> B
    style A fill:#e3f4fb;stroke:#2a8bb5;stroke-width:3px
    classDef big fill:#fff; stroke:#000
    linkStyle 0 stroke:#333;color:red
    class A big; C --> D
",
        );
        let keys: Vec<_> = chart.nodes.iter().map(|node| node.key.as_str()).collect();
        assert_eq!(keys, vec!["A", "B", "C", "D"]);
        assert_eq!(chart.edges.len(), 2);
    }
}

mod sequences {
    use super::*;

    /// The sequence diagram in `src`.
    #[track_caller]
    fn sequence(src: &str) -> SequenceDiagram {
        match ok(src) {
            Diagram::Sequence(diagram) => diagram,
            other => panic!("expected a sequence diagram, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_documentation_sequence_diagram() {
        let diagram = sequence(
            "sequenceDiagram
    autonumber
    participant Alice
    participant Bob
    Alice->>John: Hello John, how are you?
    loop HealthCheck
        John->>John: Fight against hypochondria
    end
    Note right of John: Rational thoughts <br/>prevail!
    John-->>Alice: Great!
    John->>Bob: How about you?
    Bob-->>John: Jolly good!
",
        );
        let keys: Vec<_> = diagram
            .participants
            .iter()
            .map(|p| p.key.as_str())
            .collect();
        assert_eq!(keys, vec!["Alice", "Bob", "John"]);
        assert_eq!(diagram.items.len(), 6);

        let SequenceItem::Message(first) = &diagram.items[0] else {
            panic!("expected a message, got {:?}", diagram.items[0]);
        };
        assert_eq!(first.from, ParticipantId(0));
        assert_eq!(first.to, ParticipantId(2));
        assert_eq!(first.line, MessageLine::Solid);
        assert_eq!(first.head, MessageHead::Arrow);
        assert_eq!(first.label, Label::line("Hello John, how are you?"));

        let SequenceItem::Block(block) = &diagram.items[1] else {
            panic!("expected a block, got {:?}", diagram.items[1]);
        };
        assert_eq!(block.kind, BlockKind::Loop);
        assert_eq!(block.branches.len(), 1);
        assert_eq!(block.branches[0].label, Some(Label::line("HealthCheck")));
        assert_eq!(block.branches[0].items.len(), 1);

        let SequenceItem::Note(note) = &diagram.items[2] else {
            panic!("expected a note, got {:?}", diagram.items[2]);
        };
        assert_eq!(note.placement, NotePlacement::RightOf);
        assert_eq!(note.participants, vec![ParticipantId(2)]);
        assert_eq!(
            note.text.lines,
            vec!["Rational thoughts".to_string(), "prevail!".to_string()]
        );

        let SequenceItem::Message(reply) = &diagram.items[3] else {
            panic!("expected a message, got {:?}", diagram.items[3]);
        };
        assert_eq!(reply.line, MessageLine::Dotted);
        assert_eq!(reply.head, MessageHead::Arrow);
    }

    #[test]
    fn parses_aliases_actors_and_every_arrow() {
        let diagram = sequence(
            "sequenceDiagram
    actor A as Alice
    participant J as John
    A->J: solid, no head
    A-->J: dotted, no head
    A->>J: solid arrow
    A-->>J: dotted arrow
    A-xJ: solid cross
    A--xJ: dotted cross
    J->>J: self message
",
        );
        assert_eq!(diagram.participants[0].kind, ParticipantKind::Actor);
        assert_eq!(diagram.participants[0].label, Label::line("Alice"));
        assert_eq!(diagram.participants[1].label, Label::line("John"));
        let arrows: Vec<_> = diagram
            .items
            .iter()
            .filter_map(|item| match item {
                SequenceItem::Message(message) => Some((message.line, message.head)),
                _ => None,
            })
            .collect();
        assert_eq!(
            arrows,
            vec![
                (MessageLine::Solid, MessageHead::None),
                (MessageLine::Dotted, MessageHead::None),
                (MessageLine::Solid, MessageHead::Arrow),
                (MessageLine::Dotted, MessageHead::Arrow),
                (MessageLine::Solid, MessageHead::Cross),
                (MessageLine::Dotted, MessageHead::Cross),
                (MessageLine::Solid, MessageHead::Arrow),
            ]
        );
        let SequenceItem::Message(self_message) = diagram.items.last().expect("a message") else {
            panic!("expected a message");
        };
        assert_eq!(self_message.from, self_message.to);
    }

    #[test]
    fn parses_activations_in_both_spellings() {
        let diagram = sequence(
            "sequenceDiagram
    Alice->>+John: Hello
    activate Bob
    John-->>-Alice: Bye
    deactivate Bob
",
        );
        let SequenceItem::Message(hello) = &diagram.items[0] else {
            panic!("expected a message");
        };
        assert!(hello.activates);
        assert!(!hello.deactivates);
        assert_eq!(diagram.items[1], SequenceItem::Activate(ParticipantId(2)));
        let SequenceItem::Message(bye) = &diagram.items[2] else {
            panic!("expected a message");
        };
        assert!(bye.deactivates);
        assert_eq!(diagram.items[3], SequenceItem::Deactivate(ParticipantId(2)));
    }

    #[test]
    fn parses_alt_par_and_critical_branches() {
        let diagram = sequence(
            "sequenceDiagram
    alt is sick
        Bob->>Alice: Not so good :(
    else is well
        Bob->>Alice: Feeling fresh like a daisy
    end
    par Alice to Bob
        Alice->>Bob: Hello
    and Alice to John
        Alice->>John: Hello
    end
    critical Establish connection
        Service-->Db: connect
    option Network timeout
        Service-->Service: Log error
    end
    opt Extra
        Alice->>Bob: Thanks
    end
",
        );
        let kinds: Vec<_> = diagram
            .items
            .iter()
            .filter_map(|item| match item {
                SequenceItem::Block(block) => Some((block.kind, block.branches.len())),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                (BlockKind::Alt, 2),
                (BlockKind::Par, 2),
                (BlockKind::Critical, 2),
                (BlockKind::Opt, 1),
            ]
        );
        let SequenceItem::Block(alt) = &diagram.items[0] else {
            panic!("expected a block");
        };
        assert_eq!(alt.branches[0].label, Some(Label::line("is sick")));
        assert_eq!(alt.branches[1].label, Some(Label::line("is well")));
    }

    #[test]
    fn parses_notes_over_two_participants() {
        let diagram = sequence(
            "sequenceDiagram
    participant Alice
    participant John
    Note over Alice,John: A typical interaction
    Note left of Alice: thinking
",
        );
        let SequenceItem::Note(over) = &diagram.items[0] else {
            panic!("expected a note");
        };
        assert_eq!(over.placement, NotePlacement::Over);
        assert_eq!(over.participants, vec![ParticipantId(0), ParticipantId(1)]);
        let SequenceItem::Note(left) = &diagram.items[1] else {
            panic!("expected a note");
        };
        assert_eq!(left.placement, NotePlacement::LeftOf);
    }

    #[test]
    fn keeps_the_body_of_skipped_box_and_rect_frames() {
        let diagram = sequence(
            "sequenceDiagram
    box Purple Alice & John
    participant A
    participant J
    end
    rect rgb(191, 223, 255)
    A->>J: Hello
    end
",
        );
        assert_eq!(diagram.participants.len(), 2);
        assert_eq!(diagram.items.len(), 1);
    }
}

mod classes {
    use super::*;

    /// The class diagram in `src`.
    #[track_caller]
    fn class_diagram(src: &str) -> ClassDiagram {
        match ok(src) {
            Diagram::Class(diagram) => diagram,
            other => panic!("expected a class diagram, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_documentation_class_diagram() {
        let diagram = class_diagram(
            "classDiagram
    Animal <|-- Duck
    Animal <|-- Fish
    Animal <|-- Zebra
    Animal : +int age
    Animal : +String gender
    Animal: +isMammal()
    Animal: +mate()
    class Duck{
        +String beakColor
        +swim()
        +quack()
    }
",
        );
        let names: Vec<_> = diagram.classes.iter().map(|c| c.name.text()).collect();
        assert_eq!(names, vec!["Animal", "Duck", "Fish", "Zebra"]);
        assert_eq!(diagram.relations.len(), 3);
        assert_eq!(diagram.relations[0].left, ClassId(0));
        assert_eq!(diagram.relations[0].right, ClassId(1));
        assert_eq!(diagram.relations[0].left_end, ClassArrow::Triangle);
        assert_eq!(diagram.relations[0].right_end, ClassArrow::None);
        assert_eq!(diagram.relations[0].line, LineStyle::Solid);
        assert_eq!(
            diagram.relations[0].kind(),
            Some(ClassRelationKind::Inheritance)
        );

        let animal = &diagram.classes[0];
        assert_eq!(animal.members.len(), 4);
        assert_eq!(
            animal.members[0],
            Member::Field(Field {
                visibility: Some(Visibility::Public),
                name: "age".to_string(),
                ty: Some("int".to_string()),
                classifier: None,
            })
        );
        assert_eq!(
            animal.members[2],
            Member::Method(Method {
                visibility: Some(Visibility::Public),
                name: "isMammal".to_string(),
                params: Vec::new(),
                returns: None,
                classifier: None,
            })
        );
        let duck = &diagram.classes[1];
        assert_eq!(duck.members.len(), 3);
        assert_eq!(
            duck.members[0],
            Member::Field(Field {
                visibility: Some(Visibility::Public),
                name: "beakColor".to_string(),
                ty: Some("String".to_string()),
                classifier: None,
            })
        );
    }

    #[test]
    fn parses_every_relation_operator_with_cardinalities() {
        let diagram = class_diagram(
            "classDiagram
    direction LR
    classA <|-- classB : inheritance
    classC *-- classD : composition
    classE o-- classF : aggregation
    classG <-- classH : association
    classI <.. classJ : dependency
    classK <|.. classL : realization
    Customer \"1\" --> \"*\" Ticket
",
        );
        assert_eq!(diagram.direction, Some(Direction::LeftToRight));
        let kinds: Vec<_> = diagram
            .relations
            .iter()
            .map(|relation| relation.kind())
            .collect();
        assert_eq!(
            kinds,
            vec![
                Some(ClassRelationKind::Inheritance),
                Some(ClassRelationKind::Composition),
                Some(ClassRelationKind::Aggregation),
                Some(ClassRelationKind::Association),
                Some(ClassRelationKind::Dependency),
                Some(ClassRelationKind::Realization),
                Some(ClassRelationKind::Association),
            ]
        );
        let cardinality = diagram.relations.last().expect("a relation");
        assert_eq!(cardinality.left_cardinality.as_deref(), Some("1"));
        assert_eq!(cardinality.right_cardinality.as_deref(), Some("*"));
        assert_eq!(diagram.classes[12].name.text(), "Customer");
        assert_eq!(diagram.classes[13].name.text(), "Ticket");
        assert_eq!(diagram.relations[0].label, Some(Label::line("inheritance")));
    }

    /// A `;` inside a `style` or `classDef` line must not declare a class named after
    /// the CSS property it cut off (colour spec §3.4).
    #[test]
    fn drops_css_fragments_cut_off_a_styling_statement() {
        let diagram = class_diagram(
            "classDiagram
    class Animal
    style Animal fill:#e3f4fb;stroke:#2a8bb5
    classDef foo fill:#fff;stroke:#000
    cssClass \"Animal\" foo; Animal : +int age
",
        );
        assert_eq!(diagram.classes.len(), 1);
        assert_eq!(diagram.classes[0].name.text(), "Animal");
        assert_eq!(diagram.classes[0].members.len(), 1);
    }

    /// `class A:::c` names class `A` and attaches `c` to it (colour spec §3.3).
    #[test]
    fn reads_a_class_suffix_on_a_class_declaration() {
        let diagram = class_diagram(
            "classDiagram
    class Animal
    class Animal:::foo
    class Duck:::bar {
        +swim()
    }
    class Shape~T~:::baz
    Animal <|-- Duck
",
        );
        let names: Vec<_> = diagram.classes.iter().map(|c| c.name.text()).collect();
        assert_eq!(names, vec!["Animal", "Duck", "Shape"]);
        assert_eq!(diagram.classes[1].members.len(), 1);
        assert_eq!(diagram.classes[2].generic.as_deref(), Some("T"));
        assert_eq!(diagram.relations.len(), 1);
    }

    /// `:::name` may follow a class name in a relation and in the `A : member` form,
    /// and binds tighter than the `:` that starts a label or a member (colour spec §3.3).
    #[test]
    fn reads_a_class_suffix_in_a_relation_and_a_member_line() {
        let diagram = class_diagram(
            "classDiagram
    Animal:::foo <|-- Dog
    Animal <|-- Cat:::bar
    Animal:::foo \"1\" --> \"*\" Fish:::baz : eats
    Animal:::foo : +int age
    <<interface>> Bird:::qux
",
        );
        let names: Vec<_> = diagram.classes.iter().map(|c| c.name.text()).collect();
        assert_eq!(names, vec!["Animal", "Dog", "Cat", "Fish", "Bird"]);
        let labels: Vec<_> = diagram
            .relations
            .iter()
            .map(|r| r.label.as_ref().map(Label::text))
            .collect();
        assert_eq!(labels, vec![None, None, Some("eats".to_string())]);
        assert_eq!(diagram.relations[2].left_cardinality.as_deref(), Some("1"));
        assert_eq!(diagram.relations[2].right_cardinality.as_deref(), Some("*"));
        assert_eq!(diagram.classes[0].members.len(), 1);
        assert_eq!(
            diagram.classes[4].annotation,
            Some(ClassAnnotation::Interface)
        );
    }

    #[test]
    fn parses_a_class_block_written_on_one_line() {
        let diagram = class_diagram("classDiagram\n    class A { +f() }\n    A <|-- B\n");
        assert_eq!(diagram.classes[0].name.text(), "A");
        assert_eq!(diagram.classes[0].members.len(), 1);
        assert_eq!(diagram.relations.len(), 1);
    }

    #[test]
    fn parses_annotations_generics_and_classifiers() {
        let diagram = class_diagram(
            "classDiagram
    class Shape {
        <<interface>>
        noOfVertices$
        draw()*
    }
    class Square~Shape~ {
        int id
        List~int~ position
        setPoints(List~int~ points)
        getPoints() List~int~
    }
    <<abstract>> Square
",
        );
        assert_eq!(
            diagram.classes[0].annotation,
            Some(ClassAnnotation::Interface)
        );
        assert_eq!(
            diagram.classes[0].members[0],
            Member::Field(Field {
                visibility: None,
                name: "noOfVertices".to_string(),
                ty: None,
                classifier: Some(Classifier::Static),
            })
        );
        let Member::Method(draw) = &diagram.classes[0].members[1] else {
            panic!("expected a method");
        };
        assert_eq!(draw.classifier, Some(Classifier::Abstract));

        let square = &diagram.classes[1];
        assert_eq!(square.name.text(), "Square");
        assert_eq!(square.generic.as_deref(), Some("Shape"));
        assert_eq!(square.annotation, Some(ClassAnnotation::Abstract));
        assert_eq!(
            square.members[1],
            Member::Field(Field {
                visibility: None,
                name: "position".to_string(),
                ty: Some("List<int>".to_string()),
                classifier: None,
            })
        );
        let Member::Method(set_points) = &square.members[2] else {
            panic!("expected a method");
        };
        assert_eq!(
            set_points.params,
            vec![Param {
                name: "points".to_string(),
                ty: Some("List<int>".to_string()),
            }]
        );
        let Member::Method(get_points) = &square.members[3] else {
            panic!("expected a method");
        };
        assert_eq!(get_points.returns.as_deref(), Some("List<int>"));
    }
}

mod entities {
    use super::*;

    /// The ER diagram in `src`.
    #[track_caller]
    fn er(src: &str) -> ErDiagram {
        match ok(src) {
            Diagram::Er(diagram) => diagram,
            other => panic!("expected an ER diagram, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_documentation_er_diagram() {
        let diagram = er("erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE-ITEM : contains
    CUSTOMER }|..|{ DELIVERY-ADDRESS : uses
");
        let names: Vec<_> = diagram.entities.iter().map(|e| e.name.text()).collect();
        assert_eq!(
            names,
            vec!["CUSTOMER", "ORDER", "LINE-ITEM", "DELIVERY-ADDRESS"]
        );
        assert_eq!(diagram.relationships.len(), 3);
        let places = &diagram.relationships[0];
        assert_eq!(places.left, EntityId(0));
        assert_eq!(places.right, EntityId(1));
        assert_eq!(places.left_cardinality, ErCardinality::ExactlyOne);
        assert_eq!(places.right_cardinality, ErCardinality::ZeroOrMore);
        assert_eq!(places.line, LineStyle::Solid);
        assert_eq!(places.label, Some(Label::line("places")));

        let contains = &diagram.relationships[1];
        assert_eq!(contains.right_cardinality, ErCardinality::OneOrMore);

        let uses = &diagram.relationships[2];
        assert_eq!(uses.left_cardinality, ErCardinality::OneOrMore);
        assert_eq!(uses.right_cardinality, ErCardinality::OneOrMore);
        assert_eq!(uses.line, LineStyle::Dashed);
    }

    #[test]
    fn parses_attribute_blocks() {
        let diagram = er("erDiagram
    CAR ||--o{ NAMED-DRIVER : allows
    CAR {
        string registrationNumber PK
        string make
        string model
        string[] parts
    }
    PERSON {
        string driversLicense PK \"The license #\"
        string firstName
        int age
    }
");
        let car = &diagram.entities[0];
        assert_eq!(car.attributes.len(), 4);
        assert_eq!(
            car.attributes[0],
            ErAttribute {
                ty: "string".to_string(),
                name: "registrationNumber".to_string(),
                keys: vec![ErKey::Primary],
                comment: None,
            }
        );
        assert_eq!(car.attributes[3].ty, "string[]");
        let person = diagram
            .entities
            .iter()
            .find(|entity| entity.name.text() == "PERSON")
            .expect("PERSON");
        assert_eq!(
            person.attributes[0].comment.as_deref(),
            Some("The license #")
        );
        assert_eq!(person.attributes[0].keys, vec![ErKey::Primary]);
    }

    #[test]
    fn parses_zero_or_one_cardinalities_and_aliases() {
        let diagram = er("erDiagram
    p[Person] |o--o| c[\"Car park\"] : \"may own\"
");
        assert_eq!(
            diagram.entities[0]
                .alias
                .as_ref()
                .map(Label::text)
                .as_deref(),
            Some("Person")
        );
        assert_eq!(
            diagram.entities[1]
                .alias
                .as_ref()
                .map(Label::text)
                .as_deref(),
            Some("Car park")
        );
        let relationship = &diagram.relationships[0];
        assert_eq!(relationship.left_cardinality, ErCardinality::ZeroOrOne);
        assert_eq!(relationship.right_cardinality, ErCardinality::ZeroOrOne);
        assert_eq!(relationship.label, Some(Label::line("may own")));
    }

    /// `:::name` may follow an entity name alone, before `{` and at either end of a
    /// relationship, and binds tighter than the `:` of the label (colour spec §3.3).
    #[test]
    fn reads_a_class_suffix_after_an_entity_name() {
        let diagram = er("erDiagram
    CUSTOMER:::foo ||--o{ ORDER : places
    ORDER ||--|{ LINE:::bar : contains
    PRODUCT:::baz
    CUSTOMER:::foo {
        string name
    }
    p[Person]:::qux
");
        let names: Vec<_> = diagram.entities.iter().map(|e| e.name.text()).collect();
        assert_eq!(names, vec!["CUSTOMER", "ORDER", "LINE", "PRODUCT", "p"]);
        assert_eq!(diagram.entities[0].attributes.len(), 1);
        assert_eq!(
            diagram.entities[4]
                .alias
                .as_ref()
                .map(Label::text)
                .as_deref(),
            Some("Person")
        );
        let labels: Vec<_> = diagram
            .relationships
            .iter()
            .map(|r| r.label.as_ref().map(Label::text))
            .collect();
        assert_eq!(
            labels,
            vec![Some("places".to_string()), Some("contains".to_string())]
        );
    }
}

mod pies {
    use super::*;

    /// The pie chart in `src`.
    #[track_caller]
    fn pie(src: &str) -> PieChart {
        match ok(src) {
            Diagram::Pie(chart) => chart,
            other => panic!("expected a pie chart, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_documentation_pie_chart() {
        let chart = pie("pie title Pets adopted by volunteers
    \"Dogs\" : 386
    \"Cats\" : 85
    \"Rats\" : 15
");
        assert_eq!(chart.title.as_deref(), Some("Pets adopted by volunteers"));
        assert!(!chart.show_data);
        assert_eq!(chart.slices.len(), 3);
        assert_eq!(chart.slices[0].label.text(), "Dogs");
        assert!((chart.slices[0].value - 386.0).abs() < f64::EPSILON);
        assert_eq!(chart.slices[2].label.text(), "Rats");
    }

    #[test]
    fn parses_show_data_and_fractional_values() {
        let chart = pie("pie showData
    title Key elements in Product X
    \"Calcium\" : 42.96
    \"Potassium\" : 50.05
");
        assert!(chart.show_data);
        assert_eq!(chart.title.as_deref(), Some("Key elements in Product X"));
        assert!((chart.slices[1].value - 50.05).abs() < 1e-9);
    }
}

mod gantts {
    use super::*;

    /// The gantt chart in `src`.
    #[track_caller]
    fn gantt(src: &str) -> GanttChart {
        match ok(src) {
            Diagram::Gantt(chart) => chart,
            other => panic!("expected a gantt chart, got {other:?}"),
        }
    }

    /// Seconds in a day.
    const DAY: i64 = 86_400;

    #[test]
    fn parses_and_resolves_the_documentation_gantt_chart() {
        let chart = gantt(
            "gantt
    title A Gantt Diagram
    dateFormat YYYY-MM-DD
    axisFormat %Y-%m-%d
    section Section
        A task           :a1, 2014-01-01, 30d
        Another task     :after a1, 20d
    section Another
        Task in Another  :2014-01-12, 12d
        another task     :24d
",
        );
        assert_eq!(chart.title.as_deref(), Some("A Gantt Diagram"));
        assert_eq!(chart.axis_format.as_deref(), Some("%Y-%m-%d"));
        assert_eq!(chart.sections.len(), 2);
        assert_eq!(chart.sections[0].title.as_deref(), Some("Section"));

        let first = &chart.sections[0].tasks[0];
        assert_eq!(first.name.text(), "A task");
        assert_eq!(first.id.as_deref(), Some("a1"));
        assert_eq!(first.end - first.start, 30 * DAY);

        // `after a1` starts where a1 ends.
        let second = &chart.sections[0].tasks[1];
        assert_eq!(second.start, first.end);
        assert_eq!(second.end - second.start, 20 * DAY);

        // A task with only a duration continues the previous task.
        let last = &chart.sections[1].tasks[1];
        assert_eq!(last.start, chart.sections[1].tasks[0].end);
        assert_eq!(last.end - last.start, 24 * DAY);

        let (start, end) = chart.span().expect("a span");
        assert_eq!(start, first.start);
        assert!(end >= last.end);
    }

    #[test]
    fn parses_status_tags_and_milestones() {
        let chart = gantt(
            "gantt
    dateFormat  YYYY-MM-DD
    title       Adding GANTT diagram functionality to mermaid
    section A section
    Completed task            :done,    des1, 2014-01-06,2014-01-08
    Active task               :active,  des2, 2014-01-09, 3d
    Future task               :         des3, after des2, 5d
    section Critical tasks
    Completed task in the critical line :crit, done, 2014-01-06,24h
    Create tests for parser             :crit, active, 3d
    Functionality added                 :milestone, 2014-01-25, 0d
",
        );
        let section = &chart.sections[0];
        assert_eq!(section.tasks[0].progress, TaskProgress::Done);
        assert_eq!(section.tasks[0].end - section.tasks[0].start, 2 * DAY);
        assert_eq!(section.tasks[1].progress, TaskProgress::Active);
        assert_eq!(section.tasks[2].progress, TaskProgress::Planned);
        assert_eq!(section.tasks[2].start, section.tasks[1].end);

        let critical = &chart.sections[1];
        assert!(critical.tasks[0].critical);
        assert_eq!(critical.tasks[0].progress, TaskProgress::Done);
        assert_eq!(critical.tasks[0].end - critical.tasks[0].start, DAY);
        let milestone = &critical.tasks[2];
        assert!(milestone.milestone);
        assert_eq!(milestone.start, milestone.end);
    }

    #[test]
    fn honours_a_custom_date_format() {
        let chart = gantt(
            "gantt
    dateFormat DD-MM-YYYY
    section S
    Task :t1, 06-01-2014, 1d
",
        );
        let same = gantt(
            "gantt
    section S
    Task :t1, 2014-01-06, 1d
",
        );
        assert_eq!(
            chart.sections[0].tasks[0].start,
            same.sections[0].tasks[0].start
        );
    }
}

mod states {
    use super::*;

    /// The state diagram in `src`.
    #[track_caller]
    fn state(src: &str) -> StateDiagram {
        match ok(src) {
            Diagram::State(diagram) => diagram,
            other => panic!("expected a state diagram, got {other:?}"),
        }
    }

    fn state_named<'a>(diagram: &'a StateDiagram, key: &str) -> &'a StateNode {
        diagram
            .states
            .iter()
            .find(|s| s.key == key)
            .unwrap_or_else(|| panic!("no state {key}"))
    }

    #[test]
    fn reads_colour_lines_on_states_and_composites() {
        let diagram = state(
            "stateDiagram-v2\n  [*] --> A:::hot\n  A --> B : go\n  B:::cold : waiting\n\
             state C:::hot {\n    D\n  }\n  state E <<choice>>\n  note left of F:::cold : n\n\
             class E hot\n  class B hot\n  style C stroke-width:3px\n\
             classDef hot fill:#ff0000\n  classDef cold stroke:#0000ff\n  classDef default stroke:#00ff00\n",
        );
        assert_eq!(
            colours(state_named(&diagram, "A").paint),
            Some((Some(0xff0000), Some(0x00ff00), false))
        );
        assert_eq!(
            colours(state_named(&diagram, "B").paint),
            Some((Some(0xff0000), Some(0x0000ff), false))
        );
        assert_eq!(
            colours(state_named(&diagram, "C").paint),
            Some((Some(0xff0000), None, true)),
            "no default"
        );
        assert_eq!(
            colours(state_named(&diagram, "D").paint),
            Some((None, Some(0x00ff00), false))
        );
        assert_eq!(
            state_named(&diagram, "E").paint,
            None,
            "a choice takes no paint"
        );
        assert_eq!(
            colours(state_named(&diagram, "F").paint),
            Some((None, Some(0x0000ff), false)),
            "note target"
        );
    }

    #[test]
    fn style_on_an_undeclared_state_creates_none() {
        let diagram = state("stateDiagram-v2\n  A --> B\n  style Z fill:#ff0000\n  class Y c\n");
        assert_eq!(diagram.states.len(), 2);
    }

    #[test]
    fn a_declared_alias_takes_its_class() {
        let diagram =
            state("stateDiagram-v2\n  state \"Long name\" as C:::c\n  classDef c fill:#ff0000\n");
        assert_eq!(
            colours(state_named(&diagram, "C").paint),
            Some((Some(0xff0000), None, false))
        );
    }

    #[test]
    fn parses_the_documentation_state_diagram() {
        let diagram = state(
            "stateDiagram-v2
    [*] --> Still
    Still --> [*]
    Still --> Moving
    Moving --> Still
    Moving --> Crash
    Crash --> [*]
",
        );
        let keys: Vec<_> = diagram.states.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, vec!["Still", "Moving", "Crash"]);
        assert_eq!(diagram.root.transitions.len(), 6);
        assert_eq!(diagram.root.transitions[0].from, StateEndpoint::Initial);
        assert_eq!(
            diagram.root.transitions[0].to,
            StateEndpoint::State(StateId(0))
        );
        assert_eq!(diagram.root.transitions[1].to, StateEndpoint::Final);
    }

    #[test]
    fn parses_descriptions_labels_and_composite_states() {
        let diagram = state(
            "stateDiagram-v2
    direction LR
    state \"This is a state description\" as s2
    s3 : Another description
    [*] --> First
    First --> Second : the transition
    state First {
        [*] --> fir
        fir --> [*]
        state fir {
            [*] --> deep
        }
    }
",
        );
        assert_eq!(diagram.direction, Some(Direction::LeftToRight));
        let s2 = &diagram.states[0];
        assert_eq!(s2.key, "s2");
        assert_eq!(s2.label, Some(Label::line("This is a state description")));
        assert_eq!(
            diagram.states[1].label,
            Some(Label::line("Another description"))
        );
        assert_eq!(
            diagram.root.transitions[1].label,
            Some(Label::line("the transition"))
        );

        let first = diagram
            .states
            .iter()
            .find(|state| state.key == "First")
            .expect("First");
        let StateKind::Composite(scope) = &first.kind else {
            panic!("expected a composite state, got {:?}", first.kind);
        };
        assert_eq!(scope.transitions.len(), 2);
        assert_eq!(scope.states.len(), 1);
        let fir = diagram
            .states
            .iter()
            .find(|state| state.key == "fir")
            .expect("fir");
        assert!(matches!(fir.kind, StateKind::Composite(_)));
    }

    #[test]
    fn parses_stereotypes_and_notes() {
        let diagram = state(
            "stateDiagram-v2
    state if_state <<choice>>
    state fork_state <<fork>>
    state join_state <<join>>
    [*] --> if_state
    note right of if_state : all lines are inside the note
    note left of join_state
        A multi-line
        note body
    end note
",
        );
        assert_eq!(diagram.states[0].kind, StateKind::Choice);
        assert_eq!(diagram.states[1].kind, StateKind::Fork);
        assert_eq!(diagram.states[2].kind, StateKind::Join);
        assert_eq!(diagram.root.notes.len(), 2);
        assert_eq!(diagram.root.notes[0].placement, NotePlacement::RightOf);
        assert_eq!(
            diagram.root.notes[0].text,
            Label::line("all lines are inside the note")
        );
        assert_eq!(
            diagram.root.notes[1].text.lines,
            vec!["A multi-line".to_string(), "note body".to_string()]
        );
    }

    #[test]
    fn parses_a_composite_state_written_on_one_line() {
        let diagram = state("stateDiagram-v2\n    state A { B --> C }\n    [*] --> A\n");
        let keys: Vec<_> = diagram.states.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, vec!["A", "B", "C"]);
        let StateKind::Composite(scope) = &diagram.states[0].kind else {
            panic!("expected a composite state");
        };
        assert_eq!(scope.transitions.len(), 1);
        assert_eq!(diagram.root.transitions.len(), 1);
    }

    /// A `;` inside a styling line must not draw a state named after the CSS property
    /// it cut off, with the value as its description (colour spec §3.4).
    #[test]
    fn drops_css_fragments_cut_off_a_styling_statement() {
        let diagram = state(
            "stateDiagram-v2
    A --> B
    style A fill:#e3f4fb;stroke:#2a8bb5
    classDef foo fill:#fff;stroke:#000
    class A foo; C : waiting
",
        );
        let keys: Vec<_> = diagram.states.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, vec!["A", "B", "C"]);
        assert_eq!(
            diagram.states[2].label.as_ref().map(Label::text).as_deref(),
            Some("waiting")
        );
    }

    /// In a note `:::name` belongs to the target state, not to the note text, and on a
    /// `state` declaration it is not part of the key (colour spec §3.3).
    #[test]
    fn reads_a_class_suffix_on_a_note_target_and_a_state_declaration() {
        let diagram = state(
            "stateDiagram-v2
    state A:::foo {
        [*] --> B
    }
    state \"Long name\" as C:::bar
    state D:::baz <<choice>>
    note left of A:::foo : hi
    note right of C:::bar
        two
    end note
",
        );
        let keys: Vec<_> = diagram.states.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, vec!["A", "B", "C", "D"]);
        assert!(matches!(diagram.states[0].kind, StateKind::Composite(_)));
        assert_eq!(
            diagram.states[2].label.as_ref().map(Label::text).as_deref(),
            Some("Long name")
        );
        assert_eq!(diagram.states[3].kind, StateKind::Choice);
        let notes: Vec<_> = diagram
            .root
            .notes
            .iter()
            .map(|n| (n.target, n.text.text()))
            .collect();
        assert_eq!(
            notes,
            vec![
                (StateId(0), "hi".to_string()),
                (StateId(2), "two".to_string())
            ]
        );
    }

    /// `:::name` binds tighter than `:`, so it is never read as the start of a label
    /// or a description (colour spec §3.3).
    #[test]
    fn reads_a_class_suffix_after_a_state_name() {
        let diagram = state(
            "stateDiagram-v2
    [*] --> A:::foo
    A:::foo --> B:::bar : go
    C:::baz
    D:::qux : waiting
    [*]:::foo --> D
    B --> [*]:::foo
",
        );
        let keys: Vec<_> = diagram.states.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, vec!["A", "B", "C", "D"]);
        let labels: Vec<_> = diagram
            .root
            .transitions
            .iter()
            .map(|t| t.label.as_ref().map(Label::text))
            .collect();
        assert_eq!(labels, vec![None, Some("go".to_string()), None, None]);
        let transitions = &diagram.root.transitions;
        assert_eq!(transitions[0].from, StateEndpoint::Initial);
        assert_eq!(transitions[0].to, StateEndpoint::State(StateId(0)));
        assert_eq!(transitions[1].to, StateEndpoint::State(StateId(1)));
        assert_eq!(transitions[2].from, StateEndpoint::Initial);
        assert_eq!(transitions[3].to, StateEndpoint::Final);
        assert_eq!(diagram.states[2].label, None);
        assert_eq!(
            diagram.states[3].label.as_ref().map(Label::text).as_deref(),
            Some("waiting")
        );
    }

    #[test]
    fn accepts_the_v1_spelling() {
        let diagram = state("stateDiagram\n    [*] --> Still\n");
        assert_eq!(diagram.states.len(), 1);
    }
}
