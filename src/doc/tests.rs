// SPDX-License-Identifier: MIT
//! Unit tests for the owned document tree.

use super::*;

/// Finds the first node matching `predicate`, depth first.
fn find<'a>(node: &'a Node, predicate: &dyn Fn(&Node) -> bool) -> Option<&'a Node> {
    if predicate(node) {
        return Some(node);
    }
    node.children.iter().find_map(|c| find(c, predicate))
}

fn count(doc: &Doc, predicate: impl Fn(&Node) -> bool) -> usize {
    let mut total = 0;
    doc.root().walk(&mut |node| {
        if predicate(node) {
            total += 1;
        }
    });
    total
}

#[test]
fn parses_a_document_root() {
    let doc = Doc::parse("hello\n");
    assert!(matches!(doc.root().kind, NodeKind::Document));
    assert_eq!(doc.source(), "hello\n");
    assert_eq!(count(&doc, |n| matches!(n.kind, NodeKind::Paragraph)), 1);
}

#[test]
fn headings_get_stable_unique_ids() {
    let doc = Doc::parse("# Intro\n\n## Setup\n\n## Setup\n");
    let ids: Vec<&str> = doc.headings().iter().map(|h| h.id.as_str()).collect();
    assert_eq!(ids, vec!["intro", "setup", "setup-1"]);
    assert_eq!(doc.headings()[1].level, 2);
    assert_eq!(
        doc.heading("setup-1").map(|h| h.text.as_str()),
        Some("Setup")
    );
    assert!(doc.heading("missing").is_none());
}

#[test]
fn heading_text_includes_inline_markup_as_plain_text() {
    let doc = Doc::parse("# A *fancy* `title`\n");
    assert_eq!(doc.headings()[0].text, "A fancy title");
    assert_eq!(doc.headings()[0].id, "a-fancy-title");
}

#[test]
fn source_offsets_point_at_the_original_bytes() {
    let source = "# Title\n\nA paragraph.\n";
    let doc = Doc::parse(source);
    let heading = &doc.headings()[0];
    assert_eq!(&source[heading.source.start..heading.source.end], "# Title");

    let paragraph =
        find(doc.root(), &|n| matches!(n.kind, NodeKind::Paragraph)).expect("paragraph exists");
    assert_eq!(
        &source[paragraph.source.start..paragraph.source.end],
        "A paragraph."
    );
}

#[test]
fn source_offsets_survive_multibyte_text() {
    let source = "日本語の見出し\n=====\n\n本文です。\n";
    let doc = Doc::parse(source);
    let heading = &doc.headings()[0];
    let slice = &source[heading.source.start..heading.source.end];
    assert!(slice.starts_with("日本語の見出し"), "got {slice:?}");
}

#[test]
fn html_blocks_and_inline_html_are_marked_skipped_and_carry_no_children() {
    let doc = Doc::parse("<div>\nhidden\n</div>\n\ntext <b>bold</b> more\n");
    let mut blocks = 0;
    let mut inlines = 0;
    doc.root().walk(&mut |node| match node.kind {
        NodeKind::SkippedHtml { block: true, .. } => {
            assert!(node.children.is_empty(), "skipped HTML keeps no children");
            blocks += 1;
        }
        NodeKind::SkippedHtml { block: false, .. } => {
            assert!(node.children.is_empty(), "skipped HTML keeps no children");
            inlines += 1;
        }
        _ => {}
    });
    assert_eq!(blocks, 1, "the <div> block is skipped");
    assert_eq!(inlines, 2, "both <b> and </b> are skipped");
}

#[test]
fn skipped_html_contributes_no_plain_text() {
    let doc = Doc::parse("a <b>bold</b> c\n");
    let paragraph =
        find(doc.root(), &|n| matches!(n.kind, NodeKind::Paragraph)).expect("paragraph exists");
    // The tags vanish; the text between them is ordinary Markdown text and stays.
    assert_eq!(paragraph.plain_text(), "a bold c");
}

#[test]
fn gfm_tables_are_parsed_with_alignments() {
    let doc = Doc::parse("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n");
    let table = find(doc.root(), &|n| matches!(n.kind, NodeKind::Table(_))).expect("table exists");
    let NodeKind::Table(info) = &table.kind else {
        unreachable!("matched above")
    };
    assert_eq!(info.columns, 3);
    assert_eq!(
        info.alignments,
        vec![Some(Align::Left), Some(Align::Center), Some(Align::Right)]
    );
    assert_eq!(table.children.len(), 2);
    assert!(matches!(
        table.children[0].kind,
        NodeKind::TableRow { header: true }
    ));
    assert_eq!(count(&doc, |n| matches!(n.kind, NodeKind::TableCell)), 6);
}

#[test]
fn markdown_inside_table_cells_stays_structured() {
    let doc = Doc::parse("| a |\n|---|\n| *em* `code` |\n");
    assert_eq!(count(&doc, |n| matches!(n.kind, NodeKind::Emph)), 1);
    assert_eq!(count(&doc, |n| matches!(n.kind, NodeKind::Code { .. })), 1);
}

#[test]
fn strikethrough_task_lists_and_autolinks_are_enabled() {
    let doc = Doc::parse("~~gone~~\n\n- [x] done\n- [ ] todo\n\n<https://example.com>\n");
    assert_eq!(
        count(&doc, |n| matches!(n.kind, NodeKind::Strikethrough)),
        1
    );
    assert_eq!(
        count(&doc, |n| matches!(
            n.kind,
            NodeKind::TaskItem { checked: true }
        )),
        1
    );
    assert_eq!(
        count(&doc, |n| matches!(
            n.kind,
            NodeKind::TaskItem { checked: false }
        )),
        1
    );
    assert_eq!(count(&doc, |n| matches!(n.kind, NodeKind::Link { .. })), 1);
}

#[test]
fn footnotes_are_enabled() {
    let doc = Doc::parse("text[^1]\n\n[^1]: the note\n");
    assert_eq!(
        count(&doc, |n| matches!(
            n.kind,
            NodeKind::FootnoteReference { .. }
        )),
        1
    );
    assert_eq!(
        count(&doc, |n| matches!(
            n.kind,
            NodeKind::FootnoteDefinition { .. }
        )),
        1
    );
}

#[test]
fn code_blocks_expose_language_and_literal() {
    let doc = Doc::parse("```Rust ignore\nfn main() {}\n```\n");
    let block = find(doc.root(), &|n| {
        matches!(n.kind, NodeKind::CodeBlock { .. })
    })
    .expect("code block exists");
    let NodeKind::CodeBlock {
        info,
        language,
        literal,
        fenced,
        ..
    } = &block.kind
    else {
        unreachable!("matched above")
    };
    assert_eq!(info, "Rust ignore");
    assert_eq!(language.as_deref(), Some("rust"));
    assert_eq!(literal, "fn main() {}\n");
    assert!(fenced);
}

#[test]
fn an_indented_code_block_has_no_language() {
    let doc = Doc::parse("    indented\n");
    let block = find(doc.root(), &|n| {
        matches!(n.kind, NodeKind::CodeBlock { .. })
    })
    .expect("code block exists");
    let NodeKind::CodeBlock {
        language, fenced, ..
    } = &block.kind
    else {
        unreachable!("matched above")
    };
    assert!(language.is_none());
    assert!(!fenced);
}

#[test]
fn lists_report_their_numbering() {
    let doc = Doc::parse("3. three\n4. four\n");
    let list = find(doc.root(), &|n| matches!(n.kind, NodeKind::List(_))).expect("list exists");
    let NodeKind::List(info) = &list.kind else {
        unreachable!("matched above")
    };
    assert!(info.ordered);
    assert_eq!(info.start, 3);
    assert_eq!(list.children.len(), 2);
}

#[test]
fn images_keep_their_target_and_alt_text() {
    let doc = Doc::parse("![alt text](pic.png \"caption\")\n");
    let image =
        find(doc.root(), &|n| matches!(n.kind, NodeKind::Image { .. })).expect("image exists");
    let NodeKind::Image { url, title } = &image.kind else {
        unreachable!("matched above")
    };
    assert_eq!(url, "pic.png");
    assert_eq!(title, "caption");
    assert_eq!(image.plain_text(), "alt text");
}

#[test]
fn soft_and_hard_breaks_are_distinguished() {
    let doc = Doc::parse("a\nb\\\nc\n");
    assert_eq!(count(&doc, |n| matches!(n.kind, NodeKind::SoftBreak)), 1);
    assert_eq!(count(&doc, |n| matches!(n.kind, NodeKind::LineBreak)), 1);
}

#[test]
fn version_tracks_the_source() {
    assert_eq!(Doc::parse("a\n").version(), Doc::parse("a\n").version());
    assert_ne!(Doc::parse("a\n").version(), Doc::parse("b\n").version());
}

#[test]
fn parsing_is_deterministic() {
    let source = "# Title\n\n| a | b |\n|---|---|\n| 1 | *2* |\n\n- [ ] task\n";
    assert_eq!(Doc::parse(source), Doc::parse(source));
}

#[test]
fn empty_input_parses_to_an_empty_document() {
    let doc = Doc::parse("");
    assert!(doc.root().children.is_empty());
    assert!(doc.headings().is_empty());
}

#[test]
fn source_span_helpers() {
    let span = SourceSpan::new(4, 2);
    assert!(span.is_empty());
    assert_eq!(span.len(), 0);
    let span = SourceSpan::new(2, 5);
    assert_eq!(span.len(), 3);
    assert!(span.contains(4));
    assert!(!span.contains(5));
}

/// The `lines` of the first code block in `markdown`, as the text they point at.
fn code_line_texts(markdown: &str) -> Vec<String> {
    let doc = Doc::parse(markdown);
    let block = find(doc.root(), &|n| {
        matches!(n.kind, NodeKind::CodeBlock { .. })
    })
    .expect("a code block");
    let NodeKind::CodeBlock { lines, .. } = &block.kind else {
        unreachable!()
    };
    lines
        .iter()
        .map(|s| doc.source()[s.start..s.end].to_string())
        .collect()
}

#[test]
fn a_fenced_block_maps_each_line_to_its_source() {
    let texts = code_line_texts("```rust\nlet a = 1;\nlet b = 2;\n```\n");
    assert_eq!(texts, ["let a = 1;", "let b = 2;"]);
}

#[test]
fn an_indented_block_maps_past_the_stripped_indent() {
    let texts = code_line_texts("    let a = 1;\n    let b = 2;\n");
    assert_eq!(texts, ["let a = 1;", "let b = 2;"]);
}

#[test]
fn a_quoted_fence_maps_past_the_quote_marker() {
    let texts = code_line_texts("> ```\n> let a = 1;\n> ```\n");
    assert_eq!(texts, ["let a = 1;"]);
}

#[test]
fn a_fence_in_a_list_item_maps_past_the_item_indent() {
    let texts = code_line_texts("- item\n\n  ```\n  let a = 1;\n  ```\n");
    assert_eq!(texts, ["let a = 1;"]);
}

#[test]
fn a_blank_code_line_gets_an_empty_span() {
    let doc = Doc::parse("```\na\n\nb\n```\n");
    let block = find(doc.root(), &|n| {
        matches!(n.kind, NodeKind::CodeBlock { .. })
    })
    .expect("a code block");
    let NodeKind::CodeBlock { lines, .. } = &block.kind else {
        unreachable!()
    };
    assert_eq!(lines.len(), 3, "one entry per literal line");
    assert!(lines[1].is_empty(), "the blank line points at nothing");
    assert_eq!(&doc.source()[lines[0].start..lines[0].end], "a");
    assert_eq!(&doc.source()[lines[2].start..lines[2].end], "b");
}

#[test]
fn a_fence_holding_a_fence_maps_the_inner_one() {
    // The opening `~~~` must not be mistaken for the literal line "```".
    let texts = code_line_texts("~~~\n```\n~~~\n");
    assert_eq!(texts, ["```"]);
}

#[test]
fn an_empty_fenced_block_has_no_lines() {
    // comrak gives an empty fenced block ("```\n```\n") the literal "" — zero lines,
    // not one. `"".split('\n')` would otherwise yield a single phantom empty item.
    let doc = Doc::parse("```\n```\n");
    let block = find(doc.root(), &|n| {
        matches!(n.kind, NodeKind::CodeBlock { .. })
    })
    .expect("a code block");
    let NodeKind::CodeBlock { lines, literal, .. } = &block.kind else {
        unreachable!()
    };
    assert_eq!(literal, "", "sanity: comrak's literal for an empty block");
    assert_eq!(lines.len(), 0, "an empty literal has zero lines, not one");
}

#[test]
fn a_fenced_block_holding_one_blank_line_has_exactly_one_empty_span() {
    // Distinct from the empty-block case above: here the literal is "\n" (one blank
    // line), not "" (no lines at all). Both reduce to the same split-on-empty-string
    // shape, so this and the previous test must be pinned down together.
    let doc = Doc::parse("```\n\n```\n");
    let block = find(doc.root(), &|n| {
        matches!(n.kind, NodeKind::CodeBlock { .. })
    })
    .expect("a code block");
    let NodeKind::CodeBlock { lines, literal, .. } = &block.kind else {
        unreachable!()
    };
    assert_eq!(literal, "\n", "sanity: comrak's literal for one blank line");
    assert_eq!(lines.len(), 1, "one literal line, so one entry");
    assert!(lines[0].is_empty(), "a blank line points at nothing");
}

/// Every text node of `source`, as `(text, start, end)`, in document order.
fn text_nodes(source: &str) -> Vec<(String, usize, usize)> {
    let doc = Doc::parse(source);
    let mut out = Vec::new();
    doc.root().walk(&mut |node| {
        if let NodeKind::Text(text) = &node.kind {
            out.push((text.clone(), node.source.start, node.source.end));
        }
    });
    out
}

/// Asserts that every text node either copies its source byte for byte or is a single
/// character transcribed from the whole of the source it names.
fn assert_faithful(source: &str, nodes: &[(String, usize, usize)]) {
    for (text, start, end) in nodes {
        let bytes = source.get(*start..*end).expect("a span inside the source");
        assert!(
            bytes == text || text.chars().count() == 1,
            "the node {text:?} names {bytes:?}, which it neither copies nor transcribes"
        );
    }
}

#[test]
fn an_escape_splits_its_text_node_at_the_backslash() {
    // comrak hands the whole run to one text node whose source is a byte longer than
    // its text, and a node whose lengths disagree can carry no provenance at all — so
    // one escape used to cost the whole paragraph its spans. Split at the escape, the
    // two prose runs are exact copies again and only the backslash is left over, which
    // is undrawn markup like the `**` around a bold word.
    let source = "Alpha \\* beta.\n";
    let nodes = text_nodes(source);
    assert_eq!(
        nodes,
        vec![
            ("Alpha ".to_string(), 0, 6),
            ("*".to_string(), 7, 8),
            (" beta.".to_string(), 8, 14),
        ]
    );
    assert_faithful(source, &nodes);
}

#[test]
fn an_entity_becomes_one_node_naming_the_whole_entity() {
    // Nothing in `&amp;` is a copy of the `&` it draws, so the transcribed character
    // takes the entity entire: those five bytes are exactly what produced that one cell.
    let source = "Alpha &amp; beta.\n";
    let nodes = text_nodes(source);
    assert_eq!(
        nodes,
        vec![
            ("Alpha ".to_string(), 0, 6),
            ("&".to_string(), 6, 11),
            (" beta.".to_string(), 11, 17),
        ]
    );
    assert_eq!(
        source.get(6..11),
        Some("&amp;"),
        "the whole entity, no less"
    );
    assert_faithful(source, &nodes);
}

#[test]
fn a_numeric_entity_is_transcribed_like_a_named_one() {
    for (source, expected) in [
        ("Alpha &#65; beta.\n", ("A".to_string(), 6, 11)),
        ("Alpha &#x41; beta.\n", ("A".to_string(), 6, 12)),
    ] {
        let nodes = text_nodes(source);
        assert_eq!(nodes.len(), 3, "{source:?} splits into three runs");
        assert_eq!(nodes[1], expected, "{source:?}");
        assert_faithful(source, &nodes);
    }
}

#[test]
fn an_escaped_backslash_is_aligned_by_rewinding_onto_it() {
    // `\\` is the case a forward-only walk gets wrong: the first backslash of the pair
    // compares equal to the single backslash it draws, so the walk sails past it and
    // only notices one byte later, with the escape already behind it. The alignment
    // has to be able to step back onto it.
    let source = "Alpha \\\\ beta.\n";
    assert_eq!(source.get(6..8), Some("\\\\"), "fixture: two backslashes");
    let nodes = text_nodes(source);
    assert_eq!(
        nodes,
        vec![
            ("Alpha ".to_string(), 0, 6),
            ("\\".to_string(), 7, 8),
            (" beta.".to_string(), 8, 14),
        ]
    );
    assert_faithful(source, &nodes);
}

#[test]
fn two_entities_in_a_row_are_aligned_by_rewinding_onto_the_second() {
    // The same trap as `\\`, one construct along: the `&` opening the second entity
    // compares equal to the `&` the first one drew.
    let source = "Alpha &amp;&amp; beta\n";
    let nodes = text_nodes(source);
    assert_eq!(
        nodes,
        vec![
            ("Alpha ".to_string(), 0, 6),
            ("&".to_string(), 6, 11),
            ("&".to_string(), 11, 16),
            (" beta".to_string(), 16, 21),
        ]
    );
    assert_faithful(source, &nodes);
}

#[test]
fn a_text_node_that_copies_its_source_is_left_whole() {
    // `&notreal;` is not an entity, so comrak passes it through and the node is
    // already an exact copy. Nothing to split, and no new nodes to surprise a walk.
    let source = "&notreal; x\n";
    assert_eq!(text_nodes(source), vec![("&notreal; x".to_string(), 0, 11)]);
}

#[test]
fn a_text_node_that_cannot_be_aligned_keeps_its_whole_source() {
    // `&fjlig;` expands to *two* characters. The alignment anchors an entity to one,
    // so this one does not re-synchronise and the node is left exactly as comrak
    // reported it — no provenance, which is what it had before, rather than a split
    // at a guessed position. Fail closed, per grapheme where it can and per node
    // where it cannot.
    let source = "Alpha &fjlig; beta\n";
    let nodes = text_nodes(source);
    assert_eq!(
        nodes,
        vec![("Alpha fj beta".to_string(), 0, 18)],
        "one node, unsplit"
    );
    assert_ne!(
        source.get(nodes[0].1..nodes[0].2),
        Some(nodes[0].0.as_str()),
        "and it is still the node that cannot carry provenance"
    );
}

#[test]
fn an_escape_inside_markup_is_split_like_any_other() {
    // The alignment runs over every text node, not only the ones in a bare paragraph.
    for (source, expected) in [
        (
            "# Alpha \\* beta\n",
            vec![("Alpha ", 2, 8), ("*", 9, 10), (" beta", 10, 15)],
        ),
        (
            "> Alpha \\* beta\n",
            vec![("Alpha ", 2, 8), ("*", 9, 10), (" beta", 10, 15)],
        ),
        (
            "- Alpha \\* beta\n",
            vec![("Alpha ", 2, 8), ("*", 9, 10), (" beta", 10, 15)],
        ),
    ] {
        let nodes = text_nodes(source);
        let expected: Vec<(String, usize, usize)> = expected
            .into_iter()
            .map(|(t, a, b)| (t.to_string(), a, b))
            .collect();
        assert_eq!(nodes, expected, "{source:?}");
        assert_faithful(source, &nodes);
    }
}

// --- Line endings are normalised where the document is read ---------------------

#[test]
fn a_crlf_document_reads_as_the_same_document_as_its_lf_twin() {
    // The whole rule in one assertion. `Doc` derives `PartialEq`, so this compares the
    // stored source, every node's kind, every node's byte range, the heading list and
    // the version hash at once: if a single `\r` survived anywhere, or if one offset
    // were still counted against the file as it was on disk, this would fail.
    let crlf = "# Title\r\n\r\nAlpha beta\r\ndelta zeta\r\n\r\n```rust\r\nlet a = 1;\r\n```\r\n";
    let lf = "# Title\n\nAlpha beta\ndelta zeta\n\n```rust\nlet a = 1;\n```\n";
    assert_eq!(Doc::parse(crlf), Doc::parse(lf));
    assert_eq!(Doc::parse(crlf).source(), lf);
    assert!(!Doc::parse(crlf).source().contains('\r'));
}

#[test]
fn a_lone_carriage_return_is_a_line_ending_too() {
    // Not the owner's case, and normalised anyway on evidence rather than symmetry.
    // `CommonMark` counts a lone `\r` as a line ending and comrak agrees — it reports a
    // `SoftBreak` for `"Alpha\rBeta\n"` — but its sourcepos for what follows is wrong:
    // probed, `Text("Beta")` comes back as the empty span `11..11` in an 11-byte
    // document instead of `6..10`. Leaving the lone `\r` alone would therefore leave a
    // document whose provenance is already broken; normalising costs one branch and
    // makes it a document like any other. It cannot change the document's *shape*,
    // because comrak already breaks the line there.
    assert_eq!(Doc::parse("Alpha\rBeta\n"), Doc::parse("Alpha\nBeta\n"));
    assert_eq!(Doc::parse("Alpha\rBeta\n").source(), "Alpha\nBeta\n");
    // `\n\r` is two line endings, not one: the paragraph must break in two.
    assert_eq!(Doc::parse("Alpha\n\rBeta\n"), Doc::parse("Alpha\n\nBeta\n"));
}

#[test]
fn both_other_ways_into_a_document_normalise_on_the_same_boundary() {
    // `parse` is not the only constructor: `parse_plain` builds its own tree, with its
    // own offsets, and `parse_auto` chooses between them. A `\r` reaching either would
    // be a `\r` on the clipboard of a `git log | mdmost` pipe.
    let crlf = "commit abc\r\n\r\n    a line\r\n";
    let lf = "commit abc\n\n    a line\n";
    assert_eq!(Doc::parse_plain(crlf), Doc::parse_plain(lf));
    assert_eq!(Doc::parse_auto(crlf), Doc::parse_auto(lf));
    assert!(!Doc::parse_auto(crlf).source().contains('\r'));
}

#[test]
fn a_crlf_code_blocks_literal_carries_no_carriage_return() {
    // comrak copies a fenced block's bytes into `literal` verbatim, `\r` included, and
    // `convert::code_lines` used to strip that `\r` back off line by line. Normalising
    // at the read is what makes that strip unnecessary: there is no `\r` left to keep.
    let doc = Doc::parse("```rust\r\nlet a = 1;\r\n\r\nlet b = 2;\r\n```\r\n");
    let block = find(doc.root(), &|n| {
        matches!(n.kind, NodeKind::CodeBlock { .. })
    })
    .expect("a fenced block");
    let NodeKind::CodeBlock { literal, lines, .. } = &block.kind else {
        unreachable!()
    };
    assert_eq!(literal, "let a = 1;\n\nlet b = 2;\n");
    // Provenance survives, and every located line reads back clean from the source.
    let located: Vec<&str> = lines
        .iter()
        .map(|s| &doc.source()[s.start..s.end])
        .collect();
    assert_eq!(located, vec!["let a = 1;", "", "let b = 2;"]);
}

#[test]
fn dollar_math_becomes_a_math_node() {
    let doc = Doc::parse("Einstein wrote $E = mc^2$ here.\n");
    let para = &doc.root().children[0];
    let math = para
        .children
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Math { .. }))
        .expect("no math node");
    let NodeKind::Math { literal, display } = &math.kind else {
        unreachable!()
    };
    assert_eq!(literal, "E = mc^2");
    assert!(!display);
    // The span covers the delimiters, which is what a copy of the formula needs.
    assert_eq!(
        &doc.source()[math.source.start..math.source.end],
        "$E = mc^2$"
    );
}

#[test]
fn double_dollars_are_display_math() {
    let doc = Doc::parse("$$x^2$$\n");
    let math = doc.root().children[0]
        .children
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Math { .. }))
        .expect("no math node");
    let NodeKind::Math { display, .. } = &math.kind else {
        unreachable!()
    };
    assert!(display);
}

#[test]
fn a_math_fence_is_display_math() {
    let doc = Doc::parse("```math\nx^2\n```\n");
    assert!(
        doc.root()
            .children
            .iter()
            .any(|node| matches!(&node.kind, NodeKind::Math { display: true, .. })),
        "a ```math fence must parse as display math"
    );
}

#[test]
fn currency_in_prose_is_not_math() {
    // comrak applies Pandoc's heuristics: no space before a closing `$`. This test is
    // here so that a later change to `options()` cannot silently start eating prose.
    let doc = Doc::parse("It costs $5 and $10 in total.\n");
    assert!(
        !doc.root().children[0]
            .children
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Math { .. })),
        "currency must not parse as math"
    );
}

#[test]
fn math_off_leaves_dollars_as_text() {
    let doc = Doc::parse_with(
        "Einstein wrote $E = mc^2$ here.\n",
        MathSyntax {
            dollars: false,
            backslash: false,
        },
    );
    assert!(
        !doc.root().children[0]
            .children
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Math { .. })),
        "with math off, nothing may parse as math"
    );
}

#[test]
fn math_off_leaves_a_math_fence_as_a_code_block() {
    // The fence arm's `math.dollars` guard is what spec §3 relies on: without it, a
    // reader who turned math off would still see a ```math fence stop being a code
    // block. This is a gate, and a gate is tested by the case where it does not fire.
    let doc = Doc::parse_with(
        "```math\nx^2\n```\n",
        MathSyntax {
            dollars: false,
            backslash: false,
        },
    );
    let block = &doc.root().children[0];
    assert!(
        matches!(block.kind, NodeKind::CodeBlock { .. }),
        "with math off, a ```math fence must stay a code block, not become NodeKind::Math"
    );
}

#[test]
fn backtick_dollar_math_becomes_a_math_node() {
    let doc = Doc::parse("Einstein wrote $`E = mc^2`$ here.\n");
    let math =
        find(doc.root(), &|n| matches!(n.kind, NodeKind::Math { .. })).expect("no math node");
    let NodeKind::Math { literal, display } = &math.kind else {
        unreachable!()
    };
    assert_eq!(literal, "E = mc^2");
    assert!(!display);
}

#[test]
fn a_fence_only_document_counts_as_markup_for_auto() {
    // Before this task a ```math fence was `NodeKind::CodeBlock { fenced: true }`,
    // which `has_markup` counts as markup. It must still count now that it is
    // `NodeKind::Math`, or `parse_auto` would silently reflow a formula as plain text.
    let doc = Doc::parse_auto("```math\nx^2\n```\n");
    assert!(
        find(doc.root(), &|n| matches!(
            &n.kind,
            NodeKind::Math { display: true, .. }
        ))
        .is_some(),
        "a lone math fence must not fall back to the plain-text path"
    );
}

#[test]
fn an_inline_formula_alone_counts_as_markup_for_auto() {
    let doc = Doc::parse_auto("$E = mc^2$\n");
    assert!(
        find(doc.root(), &|n| matches!(n.kind, NodeKind::Math { .. })).is_some(),
        "a lone inline formula must not fall back to the plain-text path"
    );
}
