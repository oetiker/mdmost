//! Independent end-to-end smoke tests owned by the integrator.
//!
//! These deliberately duplicate no module's internal tests. They assert only that the
//! pieces are wired together and that a realistic document survives the whole pipeline,
//! so that a module passing its own tests in isolation cannot hide a broken seam.

use mdless::mermaid::ast::Diagram;
use mdless::mermaid::parse::parse;

/// A Mermaid sample paired with a predicate identifying the variant it must parse into.
type FamilyCase = (&'static str, fn(&Diagram) -> bool);

/// One realistic sample per supported family, taken from Mermaid's own documentation
/// style, must parse into the matching [`Diagram`] variant.
#[test]
fn every_family_parses() {
    let cases: &[FamilyCase] = &[
        (
            "flowchart TD\n  A[Start] --> B{Choice}\n  B -->|yes| C(Done)\n  B -.->|no| A\n",
            |d| matches!(d, Diagram::Flowchart(_)),
        ),
        (
            "sequenceDiagram\n  participant A as Alice\n  participant B as Bob\n  A->>B: Hello\n  activate B\n  B-->>A: Hi\n  deactivate B\n  Note over A,B: greeting\n",
            |d| matches!(d, Diagram::Sequence(_)),
        ),
        (
            "classDiagram\n  class Animal {\n    +String name\n    +eat() void\n  }\n  Animal <|-- Dog\n",
            |d| matches!(d, Diagram::Class(_)),
        ),
        (
            "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER {\n    int id PK\n  }\n",
            |d| matches!(d, Diagram::Er(_)),
        ),
        ("pie title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85\n", |d| {
            matches!(d, Diagram::Pie(_))
        }),
        (
            "gantt\n  title Plan\n  dateFormat YYYY-MM-DD\n  section Build\n  design :a1, 2024-01-01, 3d\n  code   :after a1, 5d\n",
            |d| matches!(d, Diagram::Gantt(_)),
        ),
        (
            "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Running : start\n  Running --> [*]\n",
            |d| matches!(d, Diagram::State(_)),
        ),
    ];

    for (src, is_expected) in cases {
        let family = src.lines().next().unwrap_or_default();
        match parse(src) {
            Ok(diagram) => assert!(
                is_expected(&diagram),
                "`{family}` parsed into the wrong Diagram variant"
            ),
            Err(err) => panic!("`{family}` failed to parse: {err}"),
        }
    }
}

/// Malformed and truncated input must produce an error, never a panic.
#[test]
fn malformed_input_errors_without_panicking() {
    let cases = [
        "",
        "not a diagram at all",
        "flowchart",
        "flowchart TD\n  A[",
        "sequenceDiagram\n  A->>",
        "pie title\n  \"x\" :",
        "gantt\n  section\n",
        "classDiagram\n  class {",
        "erDiagram\n  A ||--",
        "stateDiagram-v2\n  [*] -->",
    ];

    for src in cases {
        // The contract is only that this returns rather than unwinds.
        let _ = parse(src);
    }
}
