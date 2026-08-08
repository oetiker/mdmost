//! Graceful-failure tests for the Mermaid parser (design spec §6 and §12).
//!
//! Malformed or out-of-subset input must return a [`MermaidError`] with a readable
//! reason and must never panic — the block renderer turns the reason into an
//! "unsupported mermaid syntax: …" caption.

use mdless::error::MermaidError;
use mdless::mermaid::parse::parse;
use proptest::prelude::*;

/// The reason string of the error `src` produces, failing the test when it parses.
#[track_caller]
fn reason(src: &str) -> String {
    match parse(src) {
        Ok(diagram) => panic!("expected an error, got {diagram:?}"),
        Err(error) => error.reason(),
    }
}

#[test]
fn rejects_unknown_families_by_name() {
    let error = parse("journey\n  title My day\n").expect_err("an error");
    assert!(matches!(error, MermaidError::UnsupportedFamily(ref name) if name == "journey"));
    assert_eq!(error.reason(), "unsupported diagram type `journey`");
}

#[test]
fn rejects_empty_and_comment_only_input() {
    assert!(reason("").contains("empty diagram"));
    assert!(reason("%% just a comment\n").contains("empty diagram"));
    assert!(reason("%%{init: {'theme':'dark'}}%%\n").contains("empty diagram"));
}

#[test]
fn reports_the_line_of_the_offending_statement() {
    let error = parse("flowchart TD\n  A --> B\n  subgraph one\n  C --> D\n").expect_err("error");
    let MermaidError::Syntax { line, ref message } = error else {
        panic!("expected a syntax error, got {error:?}");
    };
    assert_eq!(line, 4);
    assert!(message.contains("subgraph"), "message was {message}");
}

#[test]
fn reports_out_of_subset_constructs_as_unsupported() {
    for (src, needle) in [
        ("flowchart LR\n  A --x B\n", "link terminator"),
        ("sequenceDiagram\n  A-)B: async\n", "async arrows"),
        ("classDiagram\n  note \"hi\"\n", "`note` statements"),
        ("stateDiagram-v2\n  state A {\n    --\n  }\n", "concurrency"),
        (
            "gantt\n  section S\n  T :t1, 2014-01-01, 3y\n",
            "duration unit",
        ),
    ] {
        let error = parse(src).expect_err("an error");
        assert!(
            matches!(error, MermaidError::Unsupported { .. }),
            "{src} gave {error:?}"
        );
        assert!(
            error.reason().contains(needle),
            "{src} gave `{}`, expected it to mention `{needle}`",
            error.reason()
        );
    }
}

#[test]
fn rejects_malformed_statements_with_a_readable_reason() {
    for (src, needle) in [
        ("flowchart TD\n  A -->\n", "link without a target"),
        ("flowchart TD\n  end\n", "without a matching `subgraph`"),
        ("sequenceDiagram\n  Alice Bob\n", "statement"),
        ("sequenceDiagram\n  loop forever\n  A->>B: hi\n", "no `end`"),
        ("sequenceDiagram\n  Note over: hi\n", "participant"),
        ("erDiagram\n  A ||--?? B : x\n", "cardinality"),
        ("erDiagram\n  A {\n    onlyone\n  }\n", "type and a name"),
        ("pie title X\n", "without any slices"),
        ("pie\n  \"Dogs\" : lots\n", "not a number"),
        ("gantt\n  section S\n  T :t1, 3d\n", "no start date"),
        ("gantt\n  section S\n  T :t1, 2014-01-01\n", "no end date"),
        (
            "gantt\n  section S\n  T :after nope, 3d\n",
            "unknown task id",
        ),
        ("stateDiagram-v2\n  state A {\n  [*] --> B\n", "closing `}`"),
        (
            "stateDiagram-v2\n  note left of A\n  body\n",
            "no `end note`",
        ),
        ("classDiagram\n  class\n", "without a name"),
    ] {
        let text = reason(src);
        assert!(
            text.contains(needle),
            "{src:?} gave `{text}`, expected it to mention `{needle}`"
        );
    }
}

#[test]
fn rejects_edges_that_point_at_a_subgraph() {
    let text = reason("flowchart TB\n  subgraph one\n    a --> b\n  end\n  one --> c\n");
    assert!(text.contains("subgraph"), "message was {text}");
}

/// Every family header, used to give the fuzz-ish tests a plausible starting point.
const HEADERS: [&str; 7] = [
    "flowchart TD",
    "sequenceDiagram",
    "classDiagram",
    "erDiagram",
    "pie title X",
    "gantt",
    "stateDiagram-v2",
];

/// Diagram sources whose every prefix must parse or fail, but never panic.
const SAMPLES: [&str; 7] = [
    "flowchart TD\n  A[a] -->|l| B(b)\n  subgraph s\n    C{c} -.-> D((d))\n  end\n",
    "sequenceDiagram\n  A->>+B: hi\n  loop x\n    B-->>-A: bye\n  end\n  Note over A,B: n\n",
    "classDiagram\n  A <|-- B : l\n  class A {\n    +int x\n    +f(int a) B\n  }\n",
    "erDiagram\n  A ||--o{ B : has\n  A {\n    string n PK \"c\"\n  }\n",
    "pie showData\n  title T\n  \"a\" : 1\n  \"b\" : 2.5\n",
    "gantt\n  dateFormat YYYY-MM-DD\n  section S\n  t :a1, 2014-01-01, 3d\n  u :after a1, 2w\n",
    "stateDiagram-v2\n  [*] --> A\n  state A {\n    [*] --> B\n  }\n  note left of A : n\n",
];

#[test]
fn never_panics_on_a_truncated_sample() {
    for sample in SAMPLES {
        for end in 0..=sample.len() {
            if !sample.is_char_boundary(end) {
                continue;
            }
            let _ = parse(&sample[..end]);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Random text after a family header must never panic.
    #[test]
    fn never_panics_on_random_body(
        header in prop::sample::select(HEADERS.as_slice()),
        body in prop::collection::vec(
            "[a-zA-Z0-9 \\[\\]{}()<>|:;\"'.,%&*~#$/\\\\=+-]{0,40}",
            0..8,
        ),
    ) {
        let src = format!("{header}\n{}\n", body.join("\n"));
        let _ = parse(&src);
    }

    /// Entirely random input must never panic either.
    #[test]
    fn never_panics_on_arbitrary_input(src in ".{0,200}") {
        let _ = parse(&src);
    }

    /// Random mutations of a real sample must never panic.
    #[test]
    fn never_panics_on_a_mutated_sample(
        index in 0usize..SAMPLES.len(),
        cut in 0usize..400,
        insert in "[\\[\\]{}()<>|:;\"%&-]{0,4}",
    ) {
        let sample = SAMPLES[index];
        let cut = cut.min(sample.len());
        let cut = (0..=cut).rev().find(|at| sample.is_char_boundary(*at)).unwrap_or(0);
        let mutated = format!("{}{insert}{}", &sample[..cut], &sample[cut..]);
        let _ = parse(&mutated);
    }
}
