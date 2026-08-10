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
