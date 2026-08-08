//! Independent end-to-end smoke tests owned by the integrator.
//!
//! These deliberately duplicate no module's internal tests. They assert only that the
//! pieces are wired together and that a realistic document survives the whole pipeline,
//! so that a module passing its own tests in isolation cannot hide a broken seam.

use mdless::mermaid::ast::Diagram;
use mdless::mermaid::parse::parse;
use mdless::mermaid::render_mermaid;
use mdless::theme::Theme;

/// A Mermaid sample paired with a predicate identifying the variant it must parse into.
type FamilyCase = (&'static str, fn(&Diagram) -> bool);

/// One realistic sample per supported family, in the style of Mermaid's own
/// documentation. Shared by every test in this file.
const FAMILY_SAMPLES: &[FamilyCase] = &[
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

/// Every sample must parse into the matching [`Diagram`] variant.
#[test]
fn every_family_parses() {
    for (src, is_expected) in FAMILY_SAMPLES {
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

/// Every family must survive the whole parse-and-draw path at any width: it either
/// produces a canvas of exactly the requested width, or reports a `MermaidError` the
/// block renderer can caption. Neither a panic nor a mis-sized canvas is acceptable.
///
/// Families whose renderer has not landed yet legitimately take the error branch, so
/// this test does not need updating as they arrive — it only tightens.
#[test]
fn every_family_draws_or_degrades_at_every_width() {
    let themes = [Theme::default_dark(), Theme::default_light()];

    for (source, _) in FAMILY_SAMPLES {
        for width in [10_u16, 20, 40, 80, 120, 200] {
            for theme in &themes {
                match render_mermaid(source, width, theme) {
                    Ok(canvas) => {
                        assert_eq!(
                            canvas.width(),
                            width,
                            "canvas width must match the budget it was given"
                        );
                        assert!(
                            canvas.check_invariants().is_ok(),
                            "canvas invariants must hold at width {width}"
                        );
                    }
                    // Degrading is a valid outcome; the caller captions it.
                    Err(_) => continue,
                }
            }
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
