//! Character entities in Mermaid label text.
//!
//! Mermaid renders through HTML, so a diagram in the wild writes `<` as `&lt;` — that
//! is the only way to get the character past mermaid.js at all. Comrak hands us the
//! fence body undecoded (correct for code, wrong for a diagram), so the Mermaid parser
//! decodes label text itself. Mermaid also documents its own `#…;` spelling of the same
//! escapes, which these tests cover alongside the HTML one.

use mdmost::mermaid::ast::*;
use mdmost::mermaid::parse::parse;
use mdmost::mermaid::render_mermaid;
use mdmost::theme::Theme;

/// Parses `src`, failing the test with the parser's reason when it does not.
#[track_caller]
fn ok(src: &str) -> Diagram {
    match parse(src) {
        Ok(diagram) => diagram,
        Err(error) => panic!("expected a diagram, got: {error}"),
    }
}

/// The diagram `src` draws at width 60, as plain text.
///
/// Decoding is a change to what is *drawn*, so the assertions that matter are made on
/// the canvas rather than on the AST: a parse-only test passes while the screen is still
/// wrong.
#[track_caller]
fn drawn(src: &str) -> String {
    let theme = Theme::default_dark();
    match render_mermaid(src, 60, &theme) {
        Ok(canvas) => canvas.plain_text(),
        Err(error) => panic!("expected a drawn diagram, got: {error}"),
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
        assert_eq!(chart.slices[0].label.text(), "a <b");
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
        assert_eq!(chart.sections[0].tasks[0].name.text(), "Design & build");
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

/// A character reference ends in `;`, which is also Mermaid's statement separator.
///
/// A splitter that does not know that cuts a statement in half at the entity's own
/// terminator. In a state diagram the tail then reads as a further statement and draws a
/// node the author never wrote — silently wrong, which is worse than the hard error the
/// same input used to raise in a class diagram.
///
/// These assert the *drawn* diagram, not only the AST: the symptom an author sees is a
/// phantom box, and a parse-only test can pass while the box is still drawn.
mod statement_separator {
    use super::drawn;

    /// How many boxes the drawn diagram has, counted by their top-left corner.
    ///
    /// A state is drawn with rounded corners and a class or flowchart node with square
    /// ones, so both spellings count.
    fn boxes(drawn: &str) -> usize {
        drawn.matches(['╭', '┌']).count()
    }

    #[test]
    fn a_state_transition_label_survives_the_semicolon_of_an_entity() {
        let art = drawn("stateDiagram-v2\n    s1 --> s2 : press &amp; hold\n");
        assert_eq!(
            boxes(&art),
            2,
            "two states were written, so two are drawn:\n{art}"
        );
        assert!(
            art.contains("press & hold"),
            "the label decodes whole:\n{art}"
        );
    }

    #[test]
    fn a_state_transition_label_survives_a_numeric_entity() {
        // `&amp;` is the one input a wrong implementation can look right on, because the
        // `&` that opens it is also the character it draws. A numeric reference cannot
        // be confused with what it decodes to.
        let art = drawn("stateDiagram-v2\n    s1 --> s2 : press &#65; then &#x42;\n");
        assert_eq!(boxes(&art), 2, "two states, no phantom:\n{art}");
        assert!(art.contains("press A then B"), "both decode:\n{art}");
    }

    #[test]
    fn a_state_transition_label_survives_mermaids_own_hash_spelling() {
        // Mermaid documents `#…;` as a second spelling of the same escapes, and the
        // decoder consumes it, so the splitter has to step over it too — a splitter that
        // only knew `&` would cut here and the two would disagree about what a reference
        // is. `#35;` draws the `#` it names.
        let art = drawn("stateDiagram-v2\n    s1 --> s2 : issue #35;7 filed\n");
        assert_eq!(boxes(&art), 2, "two states, no phantom:\n{art}");
        assert!(art.contains("issue #7 filed"), "the label decodes:\n{art}");
    }

    #[test]
    fn a_separator_after_a_reference_still_separates() {
        // The step is over the reference and no further. A step that ran to the end of
        // the line instead would swallow this genuine separator and draw two states
        // where three were written — the same class of error as the bug, in reverse.
        let art = drawn("stateDiagram-v2\n    s1 --> s2 : a &amp; b; s2 --> s3\n");
        assert_eq!(boxes(&art), 3, "three states from two statements:\n{art}");
        assert!(art.contains("a & b"), "and the label still decodes:\n{art}");
    }

    #[test]
    fn a_class_relation_label_survives_the_semicolon_of_an_entity() {
        let art = drawn("classDiagram\n    Cat <|-- Lion : eats &amp; sleeps\n");
        assert_eq!(boxes(&art), 2, "two classes are drawn:\n{art}");
        assert!(art.contains("eats & sleeps"), "the label decodes:\n{art}");
    }

    #[test]
    fn a_class_member_survives_the_semicolon_of_an_entity() {
        // The `class X { … }` block body is split by the same call, and used to be cut
        // into the two members `T&gt` and `+get(): Vec&lt`. What this pins is that the
        // member is ONE member and its text is not cut; that it also *decodes* is
        // `class_members`' business.
        let art = drawn("classDiagram\n    class Box {\n        +get() Vec&lt;T&gt;\n    }\n");
        assert!(
            art.contains("+get(): Vec<T>"),
            "one whole, uncut member:\n{art}"
        );
        assert_eq!(
            art.matches('├').count(),
            1,
            "one divider, so one member compartment:\n{art}"
        );
    }

    #[test]
    fn a_flowchart_pipe_label_stays_correct() {
        // Flowchart was never affected — its `|…|` rule already shielded the label.
        // This pins that the shared splitter did not take that away.
        let art = drawn("flowchart LR\n    A[Start] -->|press &amp; hold| B[Stop]\n");
        assert_eq!(boxes(&art), 2, "two nodes:\n{art}");
        assert!(art.contains("press & hold"), "the label decodes:\n{art}");
    }

    #[test]
    fn a_semicolon_that_is_a_real_separator_still_separates() {
        // Both directions matter: the statement must stop being cut, and a genuine
        // separator must keep separating.
        let art = drawn("stateDiagram-v2\n    s1 --> s2; s2 --> s3\n");
        assert_eq!(boxes(&art), 3, "three states from two statements:\n{art}");
        let art = drawn("classDiagram\n    Cat <|-- Lion; Lion <|-- Cub\n");
        assert_eq!(boxes(&art), 3, "three classes from two statements:\n{art}");
    }

    #[test]
    fn an_ampersand_that_opens_nothing_changes_nothing() {
        // No terminator, so no reference: the `&` is text and the later `;` is still a
        // separator, exactly as before.
        let art = drawn("stateDiagram-v2\n    s1 --> s2 : a & b; s2 --> s3\n");
        assert_eq!(boxes(&art), 3, "three states:\n{art}");
        assert!(
            art.contains("a & b"),
            "the ampersand is drawn as written:\n{art}"
        );
    }

    #[test]
    fn a_reference_the_decoder_does_not_know_still_separates() {
        // The boundary, stated rather than stumbled into: the splitter steps over
        // exactly what the decoder will consume, so `&nosuch;` — which decodes to
        // itself and keeps its `;` as drawn text — separates as any other `;` does.
        // Anything wider would be the splitter and the decoder disagreeing about what
        // a reference is.
        let art = drawn("stateDiagram-v2\n    s1 --> s2 : a &nosuch; s2 --> s3\n");
        assert_eq!(boxes(&art), 3, "three states:\n{art}");
    }

    #[test]
    fn a_flowchart_multi_node_ampersand_is_untouched() {
        // `A & B --> C` is the flowchart's own separator syntax and has nothing to do
        // with a reference; the entity step must not have eaten it.
        let art = drawn("flowchart LR\n    A & B --> C\n");
        assert_eq!(boxes(&art), 3, "three nodes:\n{art}");
    }
}

/// A class member's text is decoded like every other label's.
///
/// A member is the one piece of drawn Mermaid text that is not a `Label`: it is
/// reassembled in the layouter out of plain `String`s the parser read apart. That is why
/// it was the last family still drawing `Vec&lt;T&gt;` at the reader, and why these tests
/// assert the canvas — the AST alone cannot say what the compartment shows.
mod class_members {
    use super::drawn;

    /// The width of the drawn box, in columns, borders included.
    ///
    /// The canvas is padded out to the full render width, so the box is measured from its
    /// own top border rather than from the line it sits on. Box art is one column per
    /// glyph, so counting characters is counting columns.
    #[track_caller]
    fn box_columns(art: &str) -> usize {
        let top = art
            .lines()
            .find(|line| line.contains('┌'))
            .unwrap_or_else(|| panic!("no box was drawn:\n{art}"));
        top.trim_end().chars().count()
    }

    #[test]
    fn a_named_entity_in_a_field_type_decodes() {
        let art = drawn("classDiagram\n    class Box {\n        +Vec&lt;T&gt; items\n    }\n");
        assert!(art.contains("+items: Vec<T>"), "the type decodes:\n{art}");
        assert!(!art.contains("&lt;"), "and nothing is left encoded:\n{art}");
    }

    #[test]
    fn a_numeric_entity_in_a_return_type_decodes() {
        // `&amp;` is the one input a wrong implementation can look right on, because the
        // `&` that opens it is also the character it draws. A numeric reference cannot.
        let art = drawn("classDiagram\n    class Box {\n        +get() &#65;rray\n    }\n");
        assert!(art.contains("+get(): Array"), "decimal decodes:\n{art}");
        let art = drawn("classDiagram\n    class Box {\n        +get() &#x42;ag\n    }\n");
        assert!(art.contains("+get(): Bag"), "and hex decodes:\n{art}");
    }

    #[test]
    fn mermaids_own_hash_spelling_decodes() {
        // Mermaid documents `#…;` as a second spelling of the same escapes.
        let art = drawn("classDiagram\n    class Box {\n        +get() Vec#lt;T#gt;\n    }\n");
        assert!(art.contains("+get(): Vec<T>"), "the return decodes:\n{art}");
    }

    #[test]
    fn a_parameter_name_and_type_decode() {
        let art = drawn(
            "classDiagram\n    class Box {\n        +add(Vec&lt;T&gt; items, &#65;) void\n    }\n",
        );
        assert!(
            art.contains("+add(items: Vec<T>, A): void"),
            "both the typed parameter and the bare one decode:\n{art}"
        );
    }

    #[test]
    fn a_member_name_decodes() {
        let art = drawn("classDiagram\n    class Box {\n        +int a&amp;b\n    }\n");
        assert!(art.contains("+a&b: int"), "the field name decodes:\n{art}");
        // A method's name is a sixth string, read on the other side of the `(`.
        let art = drawn("classDiagram\n    class Box {\n        +get&#65;ll() int\n    }\n");
        assert!(
            art.contains("+getAll(): int"),
            "and so does a method's:\n{art}"
        );
    }

    #[test]
    fn an_ampersand_that_opens_nothing_is_drawn_as_written() {
        // The other direction, and the one a too-eager decoder breaks: a literal `&` in
        // prose or a trade name names no character and must reach the screen intact.
        let art = drawn("classDiagram\n    class Box {\n        +owner: AT&T\n    }\n");
        assert!(art.contains("+owner: AT&T"), "a bare `&` survives:\n{art}");
        let art = drawn("classDiagram\n    class Box {\n        +label: a & b\n    }\n");
        assert!(
            art.contains("+label: a & b"),
            "and so does a lone one:\n{art}"
        );
        // A body with no terminator inside the window is not a reference at all.
        let art = drawn("classDiagram\n    class Box {\n        +who: Tom &amp Jerry\n    }\n");
        assert!(art.contains("+who: Tom &amp Jerry"), "unterminated:\n{art}");
    }

    #[test]
    fn a_hash_that_names_nothing_is_drawn_as_written() {
        let art = drawn("classDiagram\n    class Box {\n        +issue: #7 filed\n    }\n");
        assert!(art.contains("+issue: #7 filed"), "a bare `#`:\n{art}");
    }

    #[test]
    fn a_decoded_character_does_not_restructure_the_member() {
        // Decoding happens on the leaves, after the member has been read apart, so a
        // character an entity names is text and never syntax. An author who wrote
        // `&#40;` asked for a `(` inside a name, not for a method — decoding the whole
        // member first turns that one into an unbalanced signature and fails the parse.
        let art = drawn("classDiagram\n    class Box {\n        +int a&#40;b\n    }\n");
        assert!(
            art.contains("+a(b: int"),
            "a decoded `(` opens no parameter list:\n{art}"
        );
        // And `&#44;` asked for a comma inside one parameter, not for two of them.
        let art = drawn("classDiagram\n    class Box {\n        +add(a&#44;b) void\n    }\n");
        assert!(
            art.contains("+add(a,b): void"),
            "a decoded `,` separates nothing:\n{art}"
        );
    }

    #[test]
    fn the_box_is_measured_from_the_decoded_text() {
        // The reason this is drawn-output and not AST: a class box is as wide as its
        // widest member, so decoding does not merely change the characters, it moves the
        // border. The encoded spelling draws a 24-column box around a 20-column member;
        // each of `&lt;` and `&gt;` gives back three columns, so the member is 14 and the
        // box — a space of padding and a border on each side — is 18.
        let art = drawn("classDiagram\n    class Box {\n        +get() Vec&lt;T&gt;\n    }\n");
        assert_eq!(
            box_columns(&art),
            18,
            "the box fits the decoded member:\n{art}"
        );
    }
}

/// `Label::spans_for` maps a *piece* of a drawn line back to source bytes, which is what
/// makes a drag inside a diagram label copy the characters it went over (design spec
/// §2.2). The entity is where the mapping stops being a byte offset and starts being a
/// walk, so it is tested here beside the decoding it has to survive.
mod label_provenance {
    use super::*;

    /// `Parse & draw`, drawn from `Parse &amp; draw` at byte 17 of some Mermaid block.
    fn label() -> Label {
        Label::parse_at("Parse &amp; draw", 17)
    }

    /// A run as `(source range, column, columns)`.
    fn runs(label: &Label, at: usize, text: &str) -> Vec<((usize, usize), usize, usize)> {
        label
            .spans_for(0, at, text)
            .into_iter()
            .map(|span| ((span.source.start, span.source.end), span.col, span.cols))
            .collect()
    }

    #[test]
    fn a_piece_of_a_line_is_cut_into_runs_at_the_entity() {
        let label = label();
        assert_eq!(label.lines, vec!["Parse & draw".to_string()]);
        assert_eq!(
            runs(&label, 0, "Parse & draw"),
            vec![((17, 23), 0, 6), ((23, 28), 6, 1), ((28, 33), 7, 5)],
            "`Parse ` copies its bytes, `&amp;` is five bytes in one column, ` draw` \
             copies its bytes and starts one column after the entity"
        );
    }

    #[test]
    fn a_piece_shorter_than_the_line_is_clipped_to_what_it_drew() {
        // The middle of a word: neither end of the piece is a run boundary.
        assert_eq!(
            runs(&label(), 1, "ars"),
            vec![((18, 21), 0, 3)],
            "three characters in, three bytes out, columns relative to the piece"
        );
        // Up to and including the entity, which is whole or not at all.
        assert_eq!(
            runs(&label(), 4, "e & d"),
            vec![((21, 23), 0, 2), ((23, 28), 2, 1), ((28, 30), 3, 2)]
        );
    }

    #[test]
    fn a_piece_the_line_does_not_contain_is_declined() {
        // The caller's contract is that `text` is the piece of `lines[index]` at `at`.
        // A caller that gets it wrong gets nothing, rather than bytes chosen by
        // arithmetic over a string this label never drew — a future family layouter
        // (design spec §6) is exactly who this is for.
        assert!(runs(&label(), 3, "nope").is_empty());
        assert!(runs(&label(), 40, "Parse").is_empty());
        assert!(label().spans_for(7, 0, "Parse").is_empty(), "no such line");
    }

    #[test]
    fn a_label_that_was_never_read_from_a_source_declines() {
        assert!(
            runs(&Label::line("Parse & draw"), 0, "Parse & draw").is_empty(),
            "an empty source range is the contract's `synthesised`"
        );
        assert!(
            Label::from_lines(vec!["Parse".into()], 17..40)
                .spans_for(0, 0, "Parse")
                .is_empty(),
            "and a range that is a hull over already-split lines is not a mapping"
        );
    }
}

/// A label written with padding inside its brackets draws trimmed, and its spans have to
/// name the text rather than the padding.
///
/// `Label::parse` trims each line before decoding it, so the raw text a label records is
/// wider than the text it draws — at both ends of the label and at both sides of a
/// `<br>`. A mapping that ignored that would slide every run of the line left by as many
/// bytes as the author happened to indent by.
#[test]
fn a_padded_label_maps_its_lines_past_the_padding() {
    let label = Label::parse_at("  One  <br>  Two  ", 100);
    assert_eq!(label.lines, vec!["One".to_string(), "Two".to_string()]);
    let ranges: Vec<(usize, usize)> = [0usize, 1]
        .iter()
        .flat_map(|index| label.spans_for(*index, 0, &label.lines[*index]))
        .map(|span| (span.source.start, span.source.end))
        .collect();
    assert_eq!(
        ranges,
        vec![(102, 105), (113, 116)],
        "each line names its own three bytes, and neither names a space"
    );
}
