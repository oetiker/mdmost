//! Rendering tests for `pie` charts (design spec §6.5).
//!
//! Snapshots are named `<fixture>@<width>` and are reviewed with `cargo insta review`.
//! Every case is rendered at widths 40, 80 and 120 per design spec §13.2.

use mdless::canvas::Canvas;
use mdless::mermaid::ast::{PieChart, PieSlice};
use mdless::mermaid::pie;
use mdless::theme::Theme;
use proptest::prelude::*;

/// The widths every fixture is rendered at.
const WIDTHS: [u16; 3] = [40, 80, 120];

/// Builds a chart from `(label, value)` pairs.
fn chart(title: Option<&str>, show_data: bool, slices: &[(&str, f64)]) -> PieChart {
    PieChart {
        title: title.map(str::to_string),
        show_data,
        slices: slices
            .iter()
            .map(|(label, value)| PieSlice {
                label: (*label).to_string(),
                value: *value,
            })
            .collect(),
    }
}

/// Renders at every snapshot width, checking the canvas contract each time.
fn snapshot(name: &str, chart: &PieChart) {
    let theme = Theme::default_dark();
    for width in WIDTHS {
        let canvas = pie::draw(chart, width, &theme).expect("pie fits at snapshot widths");
        assert_contract(&canvas, width);
        insta::assert_snapshot!(format!("{name}@{width}"), canvas.plain_text());
    }
}

/// Asserts the canvas contract: exactly `width` columns on every row.
fn assert_contract(canvas: &Canvas, width: u16) {
    assert_eq!(canvas.width(), width);
    assert_eq!(canvas.check_invariants(), Ok(()));
    for row in 0..canvas.height() {
        assert_eq!(
            mdless::text::display_width(&canvas.row_text(row)),
            usize::from(width),
            "row {row} is not {width} columns wide"
        );
    }
}

#[test]
fn typical_chart_with_values() {
    snapshot(
        "typical",
        &chart(
            Some("Where the votes went"),
            true,
            &[
                ("Cats", 245.0),
                ("Dogs", 170.0),
                ("Birds", 106.0),
                ("Other", 74.0),
            ],
        ),
    );
}

#[test]
fn percentages_only_without_show_data() {
    snapshot(
        "no_show_data",
        &chart(
            None,
            false,
            &[("Rust", 61.0), ("C", 21.0), ("Assembly", 18.0)],
        ),
    );
}

#[test]
fn slices_are_sorted_and_ties_keep_declaration_order() {
    let theme = Theme::default_dark();
    let chart = chart(
        None,
        false,
        &[("small", 1.0), ("big", 9.0), ("tie a", 5.0), ("tie b", 5.0)],
    );
    let text = pie::draw(&chart, 80, &theme)
        .expect("chart fits")
        .plain_text();
    let order: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    assert_eq!(&order[..4], ["big", "tie", "tie", "small"]);
    let a = text.find("tie a").expect("first tie present");
    let b = text.find("tie b").expect("second tie present");
    assert!(a < b, "equal values must keep declaration order");
}

#[test]
fn a_single_slice_is_a_full_bar() {
    snapshot(
        "single_slice",
        &chart(Some("Only one"), true, &[("All", 7.0)]),
    );
}

#[test]
fn no_slices_degrades_to_a_placeholder() {
    snapshot("empty", &chart(Some("Nothing to show"), true, &[]));
}

#[test]
fn all_zero_values_do_not_divide_by_zero() {
    snapshot(
        "all_zero",
        &chart(None, true, &[("a", 0.0), ("b", 0.0), ("c", 0.0)]),
    );
}

#[test]
fn very_long_labels_are_truncated_not_overflowed() {
    snapshot(
        "long_labels",
        &chart(
            Some("A title that is itself far too long to fit into forty columns"),
            true,
            &[
                (
                    "An extremely long slice label that cannot possibly fit",
                    5.0,
                ),
                ("short", 3.0),
            ],
        ),
    );
}

#[test]
fn wide_and_zero_width_clusters_survive() {
    snapshot(
        "cjk_emoji",
        &chart(
            Some("多言語"),
            true,
            &[
                ("日本語のラベル", 40.0),
                ("emoji 🚀🚀 label", 35.0),
                ("café\u{301}", 25.0),
            ],
        ),
    );
}

#[test]
fn fractional_values_keep_their_precision() {
    snapshot(
        "fractional",
        &chart(None, true, &[("a", 1.25), ("b", 0.5), ("c", 0.125)]),
    );
}

#[test]
fn a_chart_too_narrow_to_draw_reports_it() {
    let theme = Theme::default_dark();
    let chart = chart(None, true, &[("a", 1.0)]);
    assert!(pie::draw(&chart, 8, &theme).is_err());
}

#[test]
fn rendering_is_deterministic() {
    let theme = Theme::default_dark();
    let chart = chart(Some("t"), true, &[("a", 3.0), ("b", 3.0), ("c", 1.0)]);
    let first = pie::draw(&chart, 80, &theme).expect("chart fits");
    for _ in 0..5 {
        assert_eq!(pie::draw(&chart, 80, &theme).expect("chart fits"), first);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Whatever the slices and the width, drawing never panics and never breaks the
    /// canvas contract.
    #[test]
    fn never_panics_and_always_fills_the_width(
        values in prop::collection::vec(0.0f64..1e6, 0..12),
        labels in prop::collection::vec("[a-zA-Z日本 ]{0,30}", 0..12),
        width in 4u16..200,
        show_data in any::<bool>(),
    ) {
        let slices: Vec<PieSlice> = values
            .iter()
            .zip(labels.iter().cycle())
            .map(|(value, label)| PieSlice { label: label.clone(), value: *value })
            .collect();
        let chart = PieChart { title: None, show_data, slices };
        if let Ok(canvas) = pie::draw(&chart, width, &Theme::default_dark()) {
            prop_assert_eq!(canvas.width(), width);
            prop_assert_eq!(canvas.check_invariants(), Ok(()));
        }
    }
}
