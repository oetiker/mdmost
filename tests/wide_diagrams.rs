// SPDX-License-Identifier: MIT
//! Wide diagrams are laid out wide and scrolled to, rather than dumped as source.
//!
//! The pager may make a block wider than the viewport (`render::document`), so a diagram that
//! does not fit has a better answer available than a dump of its own Mermaid source.
//! These tests pin which diagrams get that answer, which ones deliberately do not, and
//! that the numbers everybody is told are true.

use mdmost::canvas::Canvas;
use mdmost::doc::Doc;
use mdmost::error::MermaidError;
use mdmost::mermaid::{Fit, render_mermaid, render_mermaid_with};
use mdmost::render::document::scroll_reach;
use mdmost::render::{RenderOptions, render_document};
use mdmost::theme::Theme;

/// A chart that exhausts the fit ladder at every terminal width a reader has.
///
/// Deliberately not the seven-node chart of `visual-review-3.md` §1: budget bisection
/// made that one draw from inner 59, so it no longer exercises this path at all.
const PIPELINE: &str = include_str!("corpus/pipeline.mmd");

/// The seven-node chart, which fits a viewport only by breaking words.
const SEVEN: &str = "flowchart LR\n    A[Start] --> B[Read file]\n    B --> C[Parse Markdown]\n    C --> D[Lay out]\n    D --> E[Draw]\n    E --> F[Paint]\n    F --> G[Quit]\n";

/// The options every test here renders with: no Nerd Font glyphs, no line numbers.
const PLAIN: RenderOptions = RenderOptions::new(false, false);

/// A document with prose on both sides of one Mermaid fence.
fn document(source: &str) -> Doc {
    Doc::parse(&format!(
        "# The rendering pipeline\n\nThe stages a document goes through, start to finish.\n\n```mermaid\n{source}```\n\nEach stage is a pure function of the one before it.\n"
    ))
}

/// The document rendered the way the pager renders it — which is the only way there is.
fn paged(source: &str, width: u16) -> Canvas {
    render_document(
        &document(source),
        width,
        None,
        &Theme::default_dark(),
        &PLAIN,
    )
}

#[test]
fn a_chart_that_exhausts_the_ladder_is_drawn_wide_rather_than_dumped() {
    let canvas = paged(PIPELINE, 80);
    let text = canvas.plain_text();
    assert!(
        canvas.width() > 80,
        "the canvas should carry the diagram's surplus, but is {} wide",
        canvas.width()
    );
    assert!(
        !text.contains("flowchart LR"),
        "the Mermaid source was dumped instead of drawn:\n{text}"
    );
    assert!(
        text.contains("┌") && text.contains("▶"),
        "no box art in the rendered document:\n{text}"
    );
    // The prose is still prose: widening one block must not reflow the rest.
    assert!(text.contains("The stages a document goes through"));
}

#[test]
fn a_syntax_error_still_dumps_its_source_at_viewport_width() {
    let canvas = paged("flowchart LR\n    A --> \n", 80);
    assert_eq!(canvas.width(), 80, "a broken fence must not be widened");
    let text = canvas.plain_text();
    assert!(text.contains("flowchart LR"), "the source is the content");
    assert!(text.contains("line"), "the caption names a line:\n{text}");
}

#[test]
fn an_unsupported_family_still_dumps_its_source_at_viewport_width() {
    let canvas = paged("journey\n    title My day\n    section Go\n", 80);
    assert_eq!(canvas.width(), 80, "an unknown family must not be widened");
    let text = canvas.plain_text();
    assert!(text.contains("journey"), "the source is the content");
    assert!(
        text.contains("not a diagram type"),
        "the caption is missing:\n{text}"
    );
}

#[test]
fn a_diagram_wider_than_the_cap_dumps_its_source() {
    // Three viewports of 38 columns is 114, and this chart cannot draw below 188.
    let text = paged(PIPELINE, 40).plain_text();
    assert!(
        text.contains("flowchart LR"),
        "past the width cap the source dump is the answer, not this:\n{text}"
    );
    assert!(
        !text.contains('▶'),
        "the diagram was drawn past the width cap:\n{text}"
    );
}

#[test]
fn a_renderer_that_reports_no_floor_stays_inside_the_width_cap() {
    // `pie` answers `needed: None`, so the search has nothing to aim at and doubles
    // instead. What stops it is the probe cap and the width cap, not the renderer: this
    // pins the second of those. Without the first, this test would hang rather than
    // fail.
    let canvas = paged("pie title Votes\n    \"Yes\" : 10\n    \"No\" : 3\n", 16);
    assert!(
        canvas.width() <= 3 * 14,
        "the search ran past three viewports: {} columns",
        canvas.width()
    );
    // The probe cap itself is defence in depth — this chart resolves in two layouts,
    // which `a_renderer_that_reports_no_floor_stays_inside_the_probe_cap` pins exactly.
    let text = canvas.plain_text();
    assert!(
        text.contains("Total") || text.contains("pie title"),
        "neither a pie nor its source:\n{text}"
    );
}

#[test]
fn the_pager_refuses_a_squeeze_the_pipe_accepts() {
    let theme = Theme::default_dark();
    // The body of an 80-column viewport, after the document margins.
    let squeezed = render_mermaid(SEVEN, 78, &theme).expect("the pipe squeezes it in");
    assert!(
        !squeezed.plain_text().contains("Markdown"),
        "expected a broken word at the compact policy:\n{}",
        squeezed.plain_text()
    );
    let refused = render_mermaid_with(SEVEN, 78, &theme, Fit::ROOMY);
    assert!(
        matches!(refused, Err(MermaidError::TooNarrow { .. })),
        "the roomy policy accepted a drawing that breaks words"
    );
    // And in the pager, where there is somewhere to scroll, it is drawn whole instead.
    let text = paged(SEVEN, 80).plain_text();
    assert!(
        text.contains("Parse Markdown") || text.contains("Markdown"),
        "the widened chart still breaks its labels:\n{text}"
    );
}

// There used to be a `the_piped_renderer_keeps_the_word_breaking_rungs` here, asserting
// that a pipe squeezes this chart rather than dumping its source. It was written when
// `--render-once` had a renderer of its own; that renderer is gone, the pipe draws what
// the pager draws, and a chart too wide for the terminal is now laid out wide on both
// paths by the owner's explicit choice. The widened outcome is pinned above, through
// `paged`, which is the same call `--render-once` makes.

#[test]
fn the_caption_names_a_width_that_actually_draws() {
    // At 60 columns the cap is 174, below this chart's roomy floor, so it dumps — and
    // the caption is the only thing the reader has to go on.
    let text = paged(PIPELINE, 60).plain_text();
    let caption = text
        .lines()
        .find(|line| line.contains("needs"))
        .unwrap_or_else(|| panic!("no caption in:\n{text}"))
        .to_string();
    let needed: u16 = caption
        .split_whitespace()
        .skip_while(|word| *word != "needs")
        .nth(1)
        .and_then(|word| word.parse().ok())
        .unwrap_or_else(|| panic!("no width in the caption: {caption}"));
    assert!(
        caption.contains(&format!("needs {needed} columns")),
        "the caption hedges a number that is exact: {caption}"
    );
    let theme = Theme::default_dark();
    assert!(
        render_mermaid(PIPELINE, needed, &theme).is_ok(),
        "the caption names {needed} columns, which does not draw"
    );
    assert!(
        render_mermaid(PIPELINE, needed - 1, &theme).is_err(),
        "the caption names {needed} columns, but {} already draws",
        needed - 1
    );
}

#[test]
fn every_reported_floor_is_the_width_the_diagram_starts_drawing_at() {
    let theme = Theme::default_dark();
    let sources = [
        PIPELINE,
        SEVEN,
        "flowchart TD\n    A[Start] --> B[Middle step]\n    A --> C[Other step]\n    B --> D[End]\n    C --> D\n",
        "classDiagram\n    class Renderer {\n        +render(width)\n        +theme\n    }\n    class Canvas\n    Renderer --> Canvas\n",
        "erDiagram\n    DOCUMENT ||--o{ BLOCK : contains\n    BLOCK ||--o{ SPAN : contains\n",
        "stateDiagram-v2\n    [*] --> Reading\n    Reading --> Drawing\n    Drawing --> [*]\n",
        "sequenceDiagram\n    participant Reader\n    participant Pager\n    Reader->>Pager: press a key\n    Pager-->>Reader: a new frame\n",
        "gantt\n    title Release\n    dateFormat YYYY-MM-DD\n    section Work\n    Design :a1, 2026-01-01, 30d\n",
    ];
    let mut checked = 0;
    let mut families = 0;
    for source in sources {
        let before = checked;
        for width in [10u16, 20, 40, 60, 78] {
            let Err(MermaidError::TooNarrow {
                needed: Some(needed),
                ..
            }) = render_mermaid(source, width, &theme)
            else {
                continue;
            };
            checked += 1;
            assert!(
                render_mermaid(source, needed, &theme).is_ok(),
                "at {width} columns this reports a floor of {needed}, which does not draw:\n{source}"
            );
            assert!(
                needed == 1 || render_mermaid(source, needed - 1, &theme).is_err(),
                "the floor of {needed} is not the width it starts drawing at:\n{source}"
            );
        }
        families += usize::from(checked > before);
    }
    // A sweep that silently checked nothing would pass just as quietly. Not every
    // family reports a floor at every width — `pie` never reports one at all — so the
    // bar is that most of the corpus took part, not all of it.
    assert!(
        families >= 6 && checked >= 12,
        "only {checked} floors from {families} sources; the sweep is not exercising \
         the families it claims to"
    );
}

#[test]
fn the_roomy_policy_is_monotone_in_width() {
    let theme = Theme::default_dark();
    let mut drawn = false;
    for width in 10u16..=260 {
        let ok = render_mermaid_with(PIPELINE, width, &theme, Fit::ROOMY).is_ok();
        assert!(
            ok || !drawn,
            "the chart drew at a narrower width than {width} and then stopped"
        );
        drawn |= ok;
    }
    assert!(drawn, "the chart never drew at all");
}

#[test]
fn prose_stays_at_column_zero_while_the_diagram_scrolls() {
    // The claim `scroll_reach` was written to make, checked for the first time against a
    // real diagram: its rows have ragged right edges, and they must all scroll together
    // or the arrows slide off the boxes they attach to.
    let canvas = paged(PIPELINE, 80);
    let reach = scroll_reach(&canvas, 80);
    let mut diagram_reaches: Vec<u16> = Vec::new();
    for (row, reach) in reach.iter().copied().enumerate() {
        let text = canvas.row_text(row);
        if text.contains('┌') || text.contains('▶') || text.contains('└') {
            diagram_reaches.push(reach);
        } else if text.contains("The stages a document") || text.contains("Each stage is") {
            assert!(
                reach <= 80,
                "a prose row would be dragged sideways: reach {reach} on {text:?}"
            );
        }
    }
    assert!(
        !diagram_reaches.is_empty(),
        "no diagram rows found in the rendered document"
    );
    assert!(
        diagram_reaches.iter().all(|&at| at == diagram_reaches[0]),
        "the diagram's rows would shear apart: {diagram_reaches:?}"
    );
    assert!(
        diagram_reaches[0] > 80,
        "the diagram is not over-wide at all: {diagram_reaches:?}"
    );
}
