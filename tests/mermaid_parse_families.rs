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

mod flowcharts {
    use super::*;

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

    #[test]
    fn accepts_the_v1_spelling() {
        let diagram = state("stateDiagram\n    [*] --> Still\n");
        assert_eq!(diagram.states.len(), 1);
    }
}
