//! Rendering tests for `gantt` charts (design spec §6.6).
//!
//! Snapshots are named `<fixture>@<width>` and are reviewed with `cargo insta review`.

use mdmost::canvas::Canvas;
use mdmost::mermaid::ast::{GanttChart, GanttSection, GanttTask, TaskProgress};
use mdmost::mermaid::gantt;
use mdmost::mermaid::gantt::time::{DAY, days_from_civil};
use mdmost::theme::Theme;
use proptest::prelude::*;

/// The widths every fixture is rendered at.
const WIDTHS: [u16; 3] = [40, 80, 120];

/// Midnight UTC of a civil date, in seconds since the Unix epoch.
fn at(year: i64, month: u32, day: u32) -> i64 {
    days_from_civil(year, month, day) * DAY
}

/// A planned, non-critical task spanning two dates.
fn task(name: &str, start: i64, end: i64) -> GanttTask {
    GanttTask {
        name: name.to_string(),
        id: None,
        progress: TaskProgress::Planned,
        critical: false,
        milestone: false,
        start,
        end,
    }
}

/// Renders at every snapshot width, checking the canvas contract each time.
fn snapshot(name: &str, chart: &GanttChart) {
    let theme = Theme::default_dark();
    for width in WIDTHS {
        let canvas = gantt::draw(chart, width, &theme).expect("gantt fits at snapshot widths");
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
            mdmost::text::display_width(&canvas.row_text(row)),
            usize::from(width),
            "row {row} is not {width} columns wide"
        );
    }
}

/// The fixture exercising sections, every progress state, `crit` and a milestone.
fn release_plan() -> GanttChart {
    GanttChart {
        title: Some("Release plan".into()),
        axis_format: None,
        sections: vec![
            GanttSection {
                title: Some("Design".into()),
                tasks: vec![
                    GanttTask {
                        progress: TaskProgress::Done,
                        ..task("Spec", at(2024, 1, 1), at(2024, 1, 9))
                    },
                    GanttTask {
                        progress: TaskProgress::Active,
                        ..task("Review", at(2024, 1, 9), at(2024, 1, 15))
                    },
                ],
            },
            GanttSection {
                title: Some("Build".into()),
                tasks: vec![
                    GanttTask {
                        critical: true,
                        ..task("Core engine", at(2024, 1, 15), at(2024, 2, 12))
                    },
                    task("Docs", at(2024, 1, 22), at(2024, 2, 5)),
                    GanttTask {
                        milestone: true,
                        ..task("Ship", at(2024, 2, 12), at(2024, 2, 12))
                    },
                ],
            },
        ],
    }
}

#[test]
fn a_release_plan_with_every_task_state() {
    snapshot("release_plan", &release_plan());
}

#[test]
fn an_axis_format_is_honoured() {
    let chart = GanttChart {
        axis_format: Some("%d %b".into()),
        ..release_plan()
    };
    snapshot("axis_format", &chart);
}

#[test]
fn tasks_before_the_first_section_need_no_heading() {
    snapshot(
        "unsectioned",
        &GanttChart {
            title: None,
            axis_format: None,
            sections: vec![GanttSection {
                title: None,
                tasks: vec![
                    task("Kick off", at(2024, 3, 1), at(2024, 3, 4)),
                    task("Wrap up", at(2024, 3, 4), at(2024, 3, 8)),
                ],
            }],
        },
    );
}

#[test]
fn a_single_day_chart_still_draws_a_bar() {
    snapshot(
        "single_day",
        &GanttChart {
            title: Some("One day".into()),
            axis_format: None,
            sections: vec![GanttSection {
                title: None,
                tasks: vec![task("All of it", at(2024, 5, 6), at(2024, 5, 7))],
            }],
        },
    );
}

#[test]
fn a_chart_of_only_milestones_has_a_zero_length_span() {
    snapshot(
        "only_milestones",
        &GanttChart {
            title: None,
            axis_format: None,
            sections: vec![GanttSection {
                title: Some("Gates".into()),
                tasks: vec![
                    GanttTask {
                        milestone: true,
                        ..task("Alpha", at(2024, 6, 1), at(2024, 6, 1))
                    },
                    GanttTask {
                        milestone: true,
                        ..task("Beta", at(2024, 6, 1), at(2024, 6, 1))
                    },
                ],
            }],
        },
    );
}

#[test]
fn no_tasks_degrades_to_a_placeholder() {
    snapshot(
        "empty",
        &GanttChart {
            title: Some("Nothing planned".into()),
            axis_format: None,
            sections: vec![],
        },
    );
}

#[test]
fn a_multi_year_span_picks_a_coarse_axis() {
    snapshot(
        "multi_year",
        &GanttChart {
            title: Some("The long haul".into()),
            axis_format: None,
            sections: vec![GanttSection {
                title: Some("Phases".into()),
                tasks: vec![
                    task("Research", at(2019, 1, 1), at(2021, 6, 1)),
                    task("Delivery", at(2021, 6, 1), at(2025, 1, 1)),
                ],
            }],
        },
    );
}

#[test]
fn an_hourly_span_picks_a_fine_axis() {
    let start = at(2024, 7, 4);
    snapshot(
        "hourly",
        &GanttChart {
            title: Some("Cutover".into()),
            axis_format: None,
            sections: vec![GanttSection {
                title: None,
                tasks: vec![
                    task("Drain", start, start + 4 * 3600),
                    task("Switch", start + 4 * 3600, start + 5 * 3600),
                    task("Verify", start + 5 * 3600, start + 12 * 3600),
                ],
            }],
        },
    );
}

#[test]
fn long_names_and_wide_clusters_are_truncated_not_overflowed() {
    snapshot(
        "long_names",
        &GanttChart {
            title: Some("多言語のプロジェクト".into()),
            axis_format: None,
            sections: vec![GanttSection {
                title: Some("設計".into()),
                tasks: vec![
                    task(
                        "A task name that is far too long to fit in any gutter",
                        at(2024, 1, 1),
                        at(2024, 1, 20),
                    ),
                    task("日本語のタスク名 🚀", at(2024, 1, 20), at(2024, 2, 1)),
                ],
            }],
        },
    );
}

#[test]
fn a_chart_too_narrow_to_draw_reports_it() {
    assert!(gantt::draw(&release_plan(), 12, &Theme::default_dark()).is_err());
}

#[test]
fn rendering_is_deterministic() {
    let theme = Theme::default_dark();
    let chart = release_plan();
    let first = gantt::draw(&chart, 80, &theme).expect("chart fits");
    for _ in 0..5 {
        assert_eq!(gantt::draw(&chart, 80, &theme).expect("chart fits"), first);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Whatever the tasks and the width, drawing never panics and never breaks the
    /// canvas contract.
    #[test]
    fn never_panics_and_always_fills_the_width(
        spans in prop::collection::vec((-4_000_000_000i64..4_000_000_000, 0i64..40_000_000), 0..10),
        width in 4u16..200,
        milestone in any::<bool>(),
    ) {
        let tasks: Vec<GanttTask> = spans
            .iter()
            .enumerate()
            .map(|(index, (start, length))| GanttTask {
                name: format!("task {index}"),
                id: None,
                progress: TaskProgress::Planned,
                critical: index % 3 == 0,
                milestone: milestone && index % 2 == 0,
                start: *start,
                end: start + length,
            })
            .collect();
        let chart = GanttChart {
            title: None,
            axis_format: None,
            sections: vec![GanttSection { title: Some("s".into()), tasks }],
        };
        if let Ok(canvas) = gantt::draw(&chart, width, &Theme::default_dark()) {
            prop_assert_eq!(canvas.width(), width);
            prop_assert_eq!(canvas.check_invariants(), Ok(()));
        }
    }
}
