//! Hotspot geometry is render-time: assert on `canvas.hotspots()`, no terminal needed.
//!
//! Every position here is a *document* position, so it carries
//! [`mdmost::render::DOCUMENT_MARGIN`]: the body is inset by one column, and a link at
//! the start of a paragraph therefore starts at column 1, not column 0.

use mdmost::canvas::HotspotKind;
use mdmost::render::RenderOptions;

/// Renders `markdown` at `width` and returns its hotspots, cheapest path only.
fn hotspots(markdown: &str, width: u16) -> Vec<(usize, u16, u16, HotspotKind, usize)> {
    render(markdown, width, &RenderOptions::default())
}

/// The same, with the render options spelled out.
fn render(
    markdown: &str,
    width: u16,
    options: &RenderOptions,
) -> Vec<(usize, u16, u16, HotspotKind, usize)> {
    let doc = mdmost::Doc::parse(markdown);
    // `None` for the body cap: no ceiling on the prose measure beyond the width itself.
    let canvas =
        mdmost::render::render_document(&doc, width, None, &mdmost::Theme::default_dark(), options);
    canvas
        .hotspots()
        .iter()
        .map(|spot| {
            (
                spot.row,
                spot.col,
                spot.cols,
                spot.kind.clone(),
                spot.target,
            )
        })
        .collect()
}

/// The text actually drawn in the cells a hotspot claims.
///
/// A hotspot is a claim about *drawn cells*, so an assertion on its column alone proves
/// half of it — and the half that survives an offset applied to the claim and not to the
/// cells, which is exactly the alignment bug the table tests are looking for.
fn cells_at(markdown: &str, width: u16, row: usize, col: u16, cols: u16) -> String {
    let doc = mdmost::Doc::parse(markdown);
    let canvas = mdmost::render::render_document(
        &doc,
        width,
        None,
        &mdmost::Theme::default_dark(),
        &RenderOptions::default(),
    );
    canvas
        .row_text(row)
        .chars()
        .skip(usize::from(col))
        .take(usize::from(cols))
        .collect()
}

#[test]
fn a_link_records_a_hotspot_over_its_text_and_its_printed_url() {
    let spots = hotspots("[docs](https://example.com/a)\n", 60);
    assert_eq!(spots.len(), 1, "one link, one row, one hotspot");
    let (row, col, cols, kind, _) = &spots[0];
    assert_eq!(*row, 0);
    // Column 1, not 0: the document body is inset by `DOCUMENT_MARGIN`.
    assert_eq!(*col, 1);
    // "docs" is 4 columns, " (https://example.com/a)" is 24. The suffix is a
    // synthetic decoration carrying no source span, and it is still part of the
    // control (design spec §2.1).
    assert_eq!(*cols, 28);
    assert_eq!(
        *kind,
        HotspotKind::Open {
            url: "https://example.com/a".to_string()
        }
    );
}

#[test]
fn a_hotspot_carries_the_full_url_even_when_the_drawn_one_is_elided() {
    // elide_middle puts a `…` in the middle of a long URL for display. The status
    // bar must show the whole thing (§8) and the opener must receive the whole
    // thing (§7), so the hotspot may not carry what was drawn.
    let long = "https://example.com/a/very/long/path/that/will/certainly/be/elided/for/display/purposes/index.html";
    let spots = hotspots(&format!("[x]({long})\n"), 60);
    assert_eq!(
        spots[0].3,
        HotspotKind::Open {
            url: long.to_string()
        },
        "the hotspot carries the drawn, elided URL instead of the real one"
    );
    assert!(
        spots[0].2 < u16::try_from(long.len()).unwrap(),
        "the drawn region is the elided one, so it is shorter than the URL it opens"
    );
}

#[test]
fn a_wrapped_link_is_several_hotspots_sharing_one_target() {
    // Narrow enough that the link's text and its suffix cannot share a row.
    let spots = hotspots(
        "[a fairly long link label](https://example.com/somewhere)\n",
        24,
    );
    assert!(spots.len() >= 2, "expected the link to wrap, got {spots:?}");
    let target = spots[0].4;
    assert!(
        spots.iter().all(|s| s.4 == target),
        "every row of one link shares one target id, or it breaks in half under \
         the pointer (design spec §2.2)"
    );
    let rows: Vec<usize> = spots.iter().map(|s| s.0).collect();
    assert!(rows.windows(2).all(|w| w[0] != w[1]), "one hotspot per row");
}

#[test]
fn two_links_in_one_paragraph_do_not_share_a_target() {
    // The counterweight to the test above. `Ctx` is `Copy`, so a control counter
    // carried on it by value would give every inline subtree its own numbering and
    // both links here would answer to id 0 — hovering one would light the other.
    let spots = hotspots(
        "[a](https://example.com/a) and [b](https://example.com/b)\n",
        60,
    );
    assert_eq!(spots.len(), 2, "two links, two hotspots, got {spots:?}");
    assert_ne!(
        spots[0].4, spots[1].4,
        "two different links must not share a target id"
    );
}

#[test]
fn a_rendered_block_does_not_reissue_a_target_it_is_already_using() {
    // `Hotspot::target` is documented as unique per canvas, and `Canvas::next_target`
    // is what issues them: a control placed on this canvas next must not collide with a
    // link already on it. The rebase that holds this up for the *inline* canvas is
    // covered by `render::tests::an_inline_canvas_numbers_its_controls_from_its_own_counter`
    // — every consumer of an inline canvas merges it, and the merge rebases again, so
    // that one is only observable from inside the crate.
    let doc = mdmost::Doc::parse("[a](https://example.com/a) and [b](https://example.com/b)\n");
    let paragraph = &doc.root().children[0];
    let mut canvas = mdmost::render::render_block(
        paragraph,
        60,
        &mdmost::Theme::default_dark(),
        &RenderOptions::default(),
    );
    let used: Vec<usize> = canvas.hotspots().iter().map(|spot| spot.target).collect();
    assert_eq!(used.len(), 2, "two links, two hotspots");
    let next = canvas.next_target();
    assert!(
        !used.contains(&next),
        "the canvas offered {next} while its links already hold {used:?}"
    );
}

#[test]
fn an_autolink_records_a_hotspot_although_it_prints_no_suffix() {
    // `link()` returns early when the text already is the target. The suffix is
    // absent; the link is not.
    let spots = hotspots("<https://example.com/a>\n", 60);
    assert_eq!(spots.len(), 1, "an autolink is still a link");
    assert_eq!(spots[0].2, 21, "the hotspot covers the drawn text");
    assert_eq!(
        spots[0].3,
        HotspotKind::Open {
            url: "https://example.com/a".to_string()
        }
    );
}

#[test]
fn a_link_in_a_table_cell_records_a_hotspot_over_its_drawn_cells() {
    // **Inverted 2026-08-12 (Task 2b)**, from
    // `a_link_in_a_table_cell_records_no_hotspot_because_a_cell_is_blitted`, which pinned
    // the defect: `render_table` places each cell with `Canvas::blit`, and a blit used to
    // drop hotspots. It carries them now — a hotspot names specific cells, and after the
    // blit the link's characters are drawn in exactly those cells.
    //
    // `link()` returns early when table_depth > 0, so no ` (url)` suffix is printed and
    // the claim is the label alone.
    let markdown = "| a |\n| --- |\n| [go](https://example.com/a) |\n";
    let spots = hotspots(markdown, 60);
    assert_eq!(spots.len(), 1, "one link in one cell: {spots:?}");
    let (row, col, cols, kind, _) = &spots[0];
    assert_eq!(
        *kind,
        HotspotKind::Open {
            url: "https://example.com/a".to_string()
        }
    );
    assert_eq!(*cols, 2, "the label `go`, with no printed target beside it");
    assert_eq!(
        cells_at(markdown, 60, *row, *col, *cols),
        "go",
        "the claim has to be over the cells the link was drawn into"
    );
}

#[test]
fn a_link_in_a_right_aligned_cell_lands_on_the_cells_it_was_drawn_into() {
    // The `align_canvas` path: a non-left-aligned cell is re-placed row by row, each row
    // by its own offset. An offset applied to the cells and not to the claim would leave
    // the hotspot back at the column the block renderer drew the link at, which is where
    // left-aligned content sits — so this only fails in the alignment path, and only if
    // the hotspot did not travel with its row.
    let markdown = "| a | b |\n| --- | ---: |\n| padding here | [go](https://example.com/a) |\n";
    let spots = hotspots(markdown, 60);
    assert_eq!(spots.len(), 1, "one link: {spots:?}");
    let (row, col, cols, _, _) = &spots[0];
    assert_eq!(
        cells_at(markdown, 60, *row, *col, *cols),
        "go",
        "the hotspot must follow the alignment offset, not the drawing order"
    );
}

#[test]
fn a_link_in_a_centred_cell_lands_on_the_cells_it_was_drawn_into() {
    let markdown = "| a | b |\n| --- | :---: |\n| padding here | [go](https://example.com/a) |\n";
    let spots = hotspots(markdown, 60);
    assert_eq!(spots.len(), 1, "one link: {spots:?}");
    let (row, col, cols, _, _) = &spots[0];
    assert_eq!(cells_at(markdown, 60, *row, *col, *cols), "go");
}

#[test]
fn a_link_in_a_table_clipped_off_the_page_records_nothing() {
    // A narrow cell does not clip a link, it *wraps* it, and `render::document` widens a
    // block rather than cutting it — so the only way a table cell is cut off at document
    // level is a table wider than `MAX_BLOCK_WIDTH`. There the link goes with the cells,
    // rather than surviving as a region that reacts to a pointer while showing nothing.
    //
    // The partial cut — half a link drawn, half taken — is not reachable from a document:
    // the cut lands on a column boundary and a column is either in or out. It is covered
    // one level in, on a nested table clipped inside its cell, by
    // `render::tests::a_link_clipped_inside_a_cell_claims_only_the_cells_it_kept`, and at
    // the canvas contract by `canvas::tests::a_blit_clamps_a_hotspot_whose_cells_were_clipped`.
    let columns = 300;
    let head: String = (0..columns).map(|i| format!("| C{i:04} ")).collect();
    let rule: String = (0..columns).map(|_| "| --- ".to_string()).collect();
    let body: String = (0..columns)
        .map(|i| {
            if i == columns - 1 {
                "| [go](https://example.com/a) ".to_string()
            } else {
                format!("| v{i:04} ")
            }
        })
        .collect();
    let markdown = format!("{head}|\n{rule}|\n{body}|\n");

    let doc = mdmost::Doc::parse(&markdown);
    let canvas = mdmost::render::render_document(
        &doc,
        80,
        None,
        &mdmost::Theme::default_dark(),
        &RenderOptions::default(),
    );
    assert!(
        !canvas.plain_text().contains("go"),
        "the premise: the link's own cells were cut away"
    );
    assert!(
        canvas.hotspots().is_empty(),
        "and the claim went with them: {:?}",
        canvas.hotspots()
    );
}

#[test]
fn two_links_in_two_cells_do_not_share_a_target() {
    // Each cell canvas numbers its controls from zero, and the two arrive by two separate
    // blits. Without the rebase both would answer to id 0 and hovering one would light
    // the other — the collision `Canvas::merge_hotspots` exists to prevent, now reachable
    // by a second route.
    let spots = hotspots(
        "| a | b |\n| --- | --- |\n| [x](https://example.com/x) | [y](https://example.com/y) |\n",
        60,
    );
    assert_eq!(spots.len(), 2, "two links, two hotspots: {spots:?}");
    assert_ne!(
        spots[0].4, spots[1].4,
        "two links in two cells must not share a target id"
    );
}

// The nested-table case is not reachable from a document — GFM has no syntax for a table
// inside a table cell, and `render::tests::table_with_cell` splices one in by hand — so it
// is tested from inside the crate, by
// `render::tests::a_link_in_a_nested_table_cell_records_a_hotspot`.

#[test]
fn ordinary_text_in_a_table_cell_records_nothing() {
    // The negative half of the table case: making a cell's controls survive must not
    // make a cell into a control.
    assert!(
        hotspots("| a | b |\n| --- | --- |\n| one | two |\n", 60).is_empty(),
        "a cell is not a control"
    );
}

#[test]
fn a_link_in_a_block_quote_records_a_hotspot_at_its_drawn_column() {
    let spots = hotspots("> [go](https://example.com/a)\n", 60);
    assert_eq!(spots.len(), 1);
    assert!(
        spots[0].1 >= 2,
        "the quote bar and its gap sit left of the link, so the hotspot cannot \
         start at column 0; got {}",
        spots[0].1
    );
}

#[test]
fn a_link_in_a_list_item_records_a_hotspot_past_the_marker() {
    let spots = hotspots("- [go](https://example.com/a)\n", 60);
    assert_eq!(spots.len(), 1);
    assert!(spots[0].1 >= 2, "the bullet sits left of the link");
}

#[test]
fn ordinary_prose_records_no_hotspot() {
    // The coverage asymmetry §9.1 warns about: test that a non-hotspot cell does
    // NOT react, not only that a hotspot does.
    assert!(hotspots("just some words\n", 60).is_empty());
}

#[test]
fn an_inert_link_records_no_hotspot_but_still_draws() {
    // `mailto:` and a relative path carry no `://` at all, so they would stay inert
    // even against a classifier that dropped the scheme check entirely; `ftp://` is
    // here to keep this a real exercise of the allowlist itself, not just of the
    // "no scheme" early-out.
    let markdown = "[mail](mailto:a@b.c) and [doc](./other.md) and [f](ftp://example.com/x)\n";
    let spots = hotspots(markdown, 60);
    assert!(spots.is_empty(), "inert schemes record nothing: {spots:?}");

    // Not a control, but not invisible either: a link that lost its hotspot must
    // still print its own text, or "inert" would mean "gone".
    let doc = mdmost::Doc::parse(markdown);
    let canvas = mdmost::render::render_document(
        &doc,
        60,
        None,
        &mdmost::Theme::default_dark(),
        &RenderOptions::default(),
    );
    let text = canvas.plain_text();
    assert!(text.contains("mail"), "the link text still draws: {text:?}");
    assert!(text.contains("doc"), "the link text still draws: {text:?}");
    assert!(text.contains("f"), "the link text still draws: {text:?}");
}

#[test]
fn render_once_options_still_record_link_hotspots() {
    // A control nobody can click is worse than no control: `--render-once` draws no
    // copy button, and design spec §4 says in one line that it records no hotspot
    // either.
    //
    // It cannot be implemented that way today, and the spec contradicts itself about
    // it. `--render-once` is not a render-time fact at all: `main::render_once` builds
    // a plain `RenderOptions`, and the *only* thing that distinguishes it from the
    // pager is `copy_button: false` — which the pager also sets whenever mouse capture
    // was not granted. Gating link hotspots on that flag would therefore hide links in
    // every no-mouse terminal, which is exactly what the paragraph above that line in
    // §4 forbids ("links are never hidden — the principle is satisfied by a different
    // route", the keyboard cursor of Task 8).
    //
    // So links record hotspots under render-once options, and this test pins that
    // rather than a rule the code does not have. It is a deliberate open question for
    // the spec owner, not an oversight: nothing reads a hotspot in a dump — the canvas
    // is written to a stream by `tui::dump` — so the recording is unobservable there.
    // Resolving it means either a new `RenderOptions` field naming the dump, or
    // striking the line from §4.
    let dump = RenderOptions::new(true, false);
    assert!(!dump.copy_button, "a dump offers no copy button");
    let spots = render("[docs](https://example.com/a)\n", 60, &dump);
    assert_eq!(spots.len(), 1, "links are not gated on the copy button");
}
