//! Character entities in Mermaid label text.
//!
//! Mermaid renders through HTML, so a diagram in the wild writes `<` as `&lt;` — that
//! is the only way to get the character past mermaid.js at all. Comrak hands us the
//! fence body undecoded (correct for code, wrong for a diagram), so the Mermaid parser
//! decodes label text itself. Mermaid also documents its own `#…;` spelling of the same
//! escapes, which these tests cover alongside the HTML one.

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

/// The lines of node `A`'s label in a one-node flowchart labelled `label`.
#[track_caller]
fn node_lines(label: &str) -> Vec<String> {
    let src = format!("flowchart LR\n    A[\"{label}\"]\n");
    match ok(&src) {
        Diagram::Flowchart(chart) => chart.nodes[0].label.lines.clone(),
        other => panic!("expected a flowchart, got {other:?}"),
    }
}

/// The single line of node `A`'s label, failing when the label wrapped.
#[track_caller]
fn node_text(label: &str) -> String {
    let lines = node_lines(label);
    assert_eq!(lines.len(), 1, "expected one line, got {lines:?}");
    lines.into_iter().next().unwrap_or_default()
}

mod html_entities {
    use super::*;

    #[test]
    fn decodes_the_named_entities_mermaid_documents() {
        assert_eq!(node_text("Vec&lt;Cell&gt;"), "Vec<Cell>");
        assert_eq!(node_text("a &amp; b"), "a & b");
        assert_eq!(node_text("say &quot;hi&quot;"), "say \"hi\"");
        assert_eq!(node_text("it&apos;s"), "it's");
        assert_eq!(node_text("a&nbsp;b"), "a\u{a0}b");
    }

    #[test]
    fn decodes_decimal_and_hex_numeric_forms() {
        assert_eq!(node_text("issue &#35;7"), "issue #7");
        assert_eq!(node_text("&#x3C;T&#x3e;"), "<T>");
        assert_eq!(node_text("&#X41;&#x2764;"), "A\u{2764}");
    }

    #[test]
    fn decodes_exactly_once() {
        // An author who wrote `&amp;lt;` asked for the six characters `&lt;`, not `<`.
        assert_eq!(node_text("&amp;lt;"), "&lt;");
        // The scan is one left-to-right pass over the *input*, so the `#35;` that
        // follows a decoded `&amp;` is still an escape in its own right. Mermaid does
        // the same: it rewrites `#…;` before the browser ever sees the `&amp;`.
        assert_eq!(node_text("&amp;#35;"), "&#");
    }

    #[test]
    fn leaves_unknown_entities_alone() {
        assert_eq!(node_text("&nosuch;"), "&nosuch;");
        assert_eq!(node_text("Tom &amp Jerry"), "Tom &amp Jerry");
        assert_eq!(node_text("&#xZZ;"), "&#xZZ;");
        assert_eq!(node_text("&#1114112;"), "&#1114112;");
        assert_eq!(node_text("&;"), "&;");
        // A long non-ASCII body: the scan for the `;` must stop on a char boundary.
        assert_eq!(node_text("&\u{2764}\u{2764}\u{2764}\u{2764};"), "&❤❤❤❤;");
    }
}

mod mermaid_entities {
    use super::*;

    #[test]
    fn decodes_the_hash_sigil_forms() {
        assert_eq!(node_text("a double quote:#quot;"), "a double quote:\"");
        assert_eq!(node_text("a dec char:#9829;"), "a dec char:\u{2665}");
        assert_eq!(node_text("#lt;T#gt;"), "<T>");
    }

    #[test]
    fn leaves_a_bare_hash_alone() {
        assert_eq!(node_text("issue #7 and #nosuch;"), "issue #7 and #nosuch;");
    }
}

mod line_breaks {
    use super::*;

    #[test]
    fn an_escaped_br_is_literal_text_on_one_line() {
        // Splitting happens before decoding, so `&lt;br&gt;` is text, not a break.
        assert_eq!(node_text("one&lt;br&gt;two"), "one<br>two");
    }

    #[test]
    fn a_real_br_still_breaks_the_line() {
        assert_eq!(node_lines("one<br/>two"), vec!["one", "two"]);
        assert_eq!(node_lines("one<br>&amp;<br />two"), vec!["one", "&", "two"]);
    }
}

mod string_labels {
    use super::*;

    #[test]
    fn a_pie_title_and_slice_label_decode() {
        let chart = match ok("pie title Dogs &amp; Cats\n    \"a &lt;b\" : 10\n") {
            Diagram::Pie(chart) => chart,
            other => panic!("expected a pie chart, got {other:?}"),
        };
        assert_eq!(chart.title.as_deref(), Some("Dogs & Cats"));
        assert_eq!(chart.slices[0].label, "a <b");
    }

    #[test]
    fn a_gantt_title_section_and_task_decode() {
        let chart = match ok(
            "gantt\n    dateFormat YYYY-MM-DD\n    title A &amp; B\n    section S &lt;1&gt;\n    Design &amp; build :2024-01-01, 3d\n",
        ) {
            Diagram::Gantt(chart) => chart,
            other => panic!("expected a gantt chart, got {other:?}"),
        };
        assert_eq!(chart.title.as_deref(), Some("A & B"));
        assert_eq!(chart.sections[0].title.as_deref(), Some("S <1>"));
        assert_eq!(chart.sections[0].tasks[0].name, "Design & build");
    }

    #[test]
    fn a_sequence_title_decodes() {
        let diagram = match ok("sequenceDiagram\n    title A &amp; B\n    A->>B: x &lt; y\n") {
            Diagram::Sequence(diagram) => diagram,
            other => panic!("expected a sequence diagram, got {other:?}"),
        };
        assert_eq!(diagram.title.as_deref(), Some("A & B"));
        match &diagram.items[0] {
            SequenceItem::Message(message) => assert_eq!(message.label.text(), "x < y"),
            other => panic!("expected a message, got {other:?}"),
        }
    }
}

mod identifiers {
    use super::*;

    #[test]
    fn a_node_key_is_not_decoded() {
        // Ids must keep matching between definition and reference, so they stay raw.
        let chart = match ok("flowchart LR\n    A[\"x &lt; y\"] --> B\n") {
            Diagram::Flowchart(chart) => chart,
            other => panic!("expected a flowchart, got {other:?}"),
        };
        assert_eq!(chart.nodes[0].key, "A");
        assert_eq!(chart.nodes[0].label, Label::line("x < y"));
    }
}

mod edge_labels {
    use super::*;

    /// The label of the only edge in `src`.
    #[track_caller]
    fn edge_label(src: &str) -> String {
        match ok(src) {
            Diagram::Flowchart(chart) => match chart.edges.as_slice() {
                [edge] => edge.label.as_ref().map(Label::text).unwrap_or_default(),
                other => panic!("expected one edge, got {other:?}"),
            },
            other => panic!("expected a flowchart, got {other:?}"),
        }
    }

    #[test]
    fn a_pipe_label_survives_the_semicolon_of_an_entity() {
        // `;` separates statements, but not inside a `|…|` label — which is where an
        // entity most often appears, because that is where generics get written.
        assert_eq!(
            edge_label("flowchart LR\n    A -->|Vec&lt;T&gt;| B\n"),
            "Vec<T>"
        );
        assert_eq!(
            edge_label("flowchart LR\n    A -->|a &amp; b| B\n"),
            "a & b"
        );
    }

    #[test]
    fn a_pipe_label_may_contain_a_plain_semicolon() {
        assert_eq!(
            edge_label("flowchart LR\n    A -->|do this; then that| B\n"),
            "do this; then that"
        );
    }

    #[test]
    fn a_semicolon_outside_a_pipe_label_still_separates_statements() {
        let chart = match ok("flowchart LR\n    A -->|x| B; B --> C\n") {
            Diagram::Flowchart(chart) => chart,
            other => panic!("expected a flowchart, got {other:?}"),
        };
        assert_eq!(chart.edges.len(), 2);
        assert_eq!(chart.nodes.len(), 3);
    }

    #[test]
    fn a_semicolon_in_a_bracketed_node_label_is_not_a_pipe() {
        let chart = match ok("flowchart LR\n    A[\"a|b\"] --> B; B --> C\n") {
            Diagram::Flowchart(chart) => chart,
            other => panic!("expected a flowchart, got {other:?}"),
        };
        assert_eq!(chart.nodes[0].label, Label::line("a|b"));
        assert_eq!(chart.edges.len(), 2);
    }
}
