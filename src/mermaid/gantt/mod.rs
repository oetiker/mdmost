//! `gantt` charts, drawn as a time axis with per-section bar rows (design spec §6.6).
//!
//! Task dates arrive fully resolved from the parser, so this module's job is purely
//! geometric: choose a tick spacing the available width can carry, label it with
//! `axisFormat`, then map each task's interval of seconds onto columns.
//!
//! ```text
//!             2024-01-01   2024-01-08   2024-01-15
//!            ├───────────┬────────────┬────────────
//! Design     │
//!   Spec     │░░░░░░░░
//!   Review   │        ▓▓▓▓▓▓
//! Build      │
//!   Core     │              ████████████
//!   Ship     │                          ◆
//! ```
//!
//! Bars are whole cells: their texture (`░` done, `█` active, `▒` planned, `▓`
//! critical) carries the task state even in a plain-text dump, where colour cannot.
//! A `crit` task uses the critical texture whatever its progress, matching Mermaid,
//! where criticality is the louder signal.

pub mod time;

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::{GanttChart, GanttTask, TaskProgress};
use crate::mermaid::chrome;
use crate::text::display_width;
use crate::theme::{Style, Theme};
use time::{DAY, DateTime, HOUR, WEEK};

/// The narrowest plot area worth drawing.
const MIN_PLOT: usize = 12;
/// The narrowest task-name gutter worth drawing.
const MIN_GUTTER: usize = 6;
/// Columns a task name is indented by inside a titled section.
const TASK_INDENT: usize = 2;
/// Blank columns kept between two neighbouring tick labels.
const TICK_PADDING: usize = 2;
/// The marker drawn for a milestone.
const MILESTONE: &str = "◆";
/// The text shown for a chart that declares no tasks.
const EMPTY_TEXT: &str = "(no tasks)";
/// Upper bound on generated ticks, so a pathological span cannot loop for long.
const MAX_TICKS: usize = 1024;

/// Renders a gantt chart into a canvas exactly `width` columns wide.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when `width` cannot carry a task gutter and a
/// legible plot area side by side.
pub fn draw(chart: &GanttChart, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    let Some((start, end)) = chart.span() else {
        let body = chrome::placeholder(EMPTY_TEXT, width, theme);
        return chrome::compose(chart.title.as_deref(), &body, width, theme);
    };
    // A chart whose tasks are all milestones on the same instant has a zero-length
    // span; widen it to a day so the axis has something to divide.
    let end = if end > start {
        end
    } else {
        time::clamp_instant(start.saturating_add(DAY))
    };
    let body = plot(chart, start, end, width, theme)?;
    chrome::compose(chart.title.as_deref(), &body, width, theme)
}

/// How wide the gutter and the plot area are.
#[derive(Debug, Clone, Copy)]
struct Columns {
    gutter: usize,
    plot: usize,
}

impl Columns {
    /// The column the separator rule is drawn in.
    fn separator(self) -> usize {
        self.gutter
    }

    /// The column the plot area starts at.
    fn plot_start(self) -> usize {
        self.gutter + 1
    }

    /// The total content width.
    fn total(self) -> usize {
        self.gutter + 1 + self.plot
    }
}

/// Splits `width` between the task-name gutter and the plot area.
fn negotiate(chart: &GanttChart, width: u16) -> Result<Columns, MermaidError> {
    let budget = usize::from(width);
    let natural = chart
        .sections
        .iter()
        .flat_map(|section| {
            section
                .title
                .iter()
                .map(|title| display_width(title))
                .chain(section.tasks.iter().map(|task| {
                    display_width(&task.name) + usize::from(section.title.is_some()) * TASK_INDENT
                }))
        })
        .max()
        .unwrap_or(MIN_GUTTER);

    let cap = (budget * 2 / 5).max(MIN_GUTTER);
    let gutter = natural.clamp(MIN_GUTTER, cap);
    for gutter in [gutter, MIN_GUTTER] {
        if let Some(plot) = budget.checked_sub(gutter + 1)
            && plot >= MIN_PLOT
        {
            return Ok(Columns { gutter, plot });
        }
    }
    Err(MermaidError::TooNarrow { width })
}

/// Draws the axis, the section headings and the task bars.
fn plot(
    chart: &GanttChart,
    start: i64,
    end: i64,
    width: u16,
    theme: &Theme,
) -> Result<Canvas, MermaidError> {
    let columns = negotiate(chart, width)?;
    let axis = Axis::choose(start, end, columns.plot, chart.axis_format.as_deref());
    let content = u16::try_from(columns.total()).unwrap_or(u16::MAX);
    let base = theme.base();
    let mut body = Canvas::new(content, 2, base);

    // Row 0 carries the tick labels, row 1 the axis rule itself.
    body.write_str(1, columns.separator(), "├", theme.diagram.axis);
    body.hline(
        1,
        columns.plot_start(),
        columns.plot,
        "─",
        theme.diagram.axis,
    );
    for tick in &axis.ticks {
        let at = columns.plot_start() + tick.column;
        let text = chrome::fit(&tick.label, columns.plot);
        let span = display_width(&text);
        let left = columns.plot_start() + tick_left(tick.column, span, columns.plot);
        body.write_str(0, left, &text, theme.diagram.axis);
        // Column zero is already marked by the axis' own left terminator.
        if tick.column > 0 {
            body.write_str(1, at, "┬", theme.diagram.axis);
        }
    }

    for (index, section) in chart.sections.iter().enumerate() {
        if index > 0 {
            let row = body.push_blank_row(base);
            body.write_str(row, columns.separator(), "│", theme.diagram.axis);
        }
        if let Some(title) = &section.title {
            let row = body.push_blank_row(base);
            body.write_str(
                row,
                0,
                &chrome::fit(title, columns.gutter),
                theme.diagram.group_title,
            );
            body.write_str(row, columns.separator(), "│", theme.diagram.axis);
        }
        let indent = usize::from(section.title.is_some()) * TASK_INDENT;
        for task in &section.tasks {
            let row = body.push_blank_row(base);
            body.write_str(
                row,
                indent,
                // `MIN_GUTTER` exceeds `TASK_INDENT`, so this cannot currently reach
                // zero; saturating keeps that a local fact rather than a global one.
                &chrome::fit(&task.name, columns.gutter.saturating_sub(indent)),
                theme.diagram.node_text,
            );
            body.write_str(row, columns.separator(), "│", theme.diagram.axis);
            draw_task(&mut body, row, &columns, task, start, end, theme);
        }
    }

    Ok(body)
}

/// Paints one task's bar or milestone marker into the plot area.
fn draw_task(
    body: &mut Canvas,
    row: usize,
    columns: &Columns,
    task: &GanttTask,
    start: i64,
    end: i64,
    theme: &Theme,
) {
    let last = columns.plot.saturating_sub(1);
    let from = column_of(task.start, start, end, columns.plot).min(last);
    if task.milestone {
        body.write_str(
            row,
            columns.plot_start() + from,
            MILESTONE,
            theme.diagram.milestone,
        );
        return;
    }
    let to = column_of(task.end, start, end, columns.plot);
    let len = to.saturating_sub(from).max(1).min(columns.plot - from);
    body.fill(
        row,
        columns.plot_start() + from,
        len,
        task_glyph(task),
        task_style(task, theme),
    );
}

/// Maps an instant onto a plot column.
fn column_of(at: i64, start: i64, end: i64, plot: usize) -> usize {
    let span = (end - start).max(1) as f64;
    let offset = (at - start).max(0) as f64;
    let column = (offset / span * plot as f64).round();
    // The product is finite and non-negative, so the cast is exact after clamping.
    (column.max(0.0) as usize).min(plot)
}

/// The fill texture of a task bar.
fn task_glyph(task: &GanttTask) -> &'static str {
    if task.critical {
        return "▓";
    }
    match task.progress {
        TaskProgress::Done => "░",
        TaskProgress::Active => "█",
        TaskProgress::Planned => "▒",
    }
}

/// The style of a task bar.
fn task_style(task: &GanttTask, theme: &Theme) -> Style {
    if task.critical {
        return theme.diagram.task_crit;
    }
    match task.progress {
        TaskProgress::Done => theme.diagram.task_done,
        TaskProgress::Active => theme.diagram.task_active,
        TaskProgress::Planned => theme.diagram.line,
    }
}

// ---------------------------------------------------------------------------
// Axis
// ---------------------------------------------------------------------------

/// One labelled tick on the time axis.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tick {
    column: usize,
    label: String,
}

/// The chosen tick spacing together with its rendered ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Axis {
    ticks: Vec<Tick>,
}

/// A candidate tick spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// A fixed number of seconds.
    Seconds(i64),
    /// A whole number of calendar months.
    Months(i64),
}

/// Tick spacings from finest to coarsest.
const STEPS: [Step; 17] = [
    Step::Seconds(HOUR),
    Step::Seconds(2 * HOUR),
    Step::Seconds(3 * HOUR),
    Step::Seconds(6 * HOUR),
    Step::Seconds(12 * HOUR),
    Step::Seconds(DAY),
    Step::Seconds(2 * DAY),
    Step::Seconds(WEEK),
    Step::Seconds(2 * WEEK),
    Step::Months(1),
    Step::Months(3),
    Step::Months(6),
    Step::Months(12),
    Step::Months(24),
    Step::Months(60),
    Step::Months(120),
    Step::Months(600),
];

impl Step {
    /// The step's approximate length in seconds, for spacing arithmetic.
    fn seconds(self) -> i64 {
        match self {
            Self::Seconds(seconds) => seconds,
            // The mean Gregorian month, good enough to choose a spacing with.
            Self::Months(months) => months * 2_629_746,
        }
    }

    /// The format string to use when the chart gave no `axisFormat`.
    fn default_format(self) -> &'static str {
        match self {
            Self::Seconds(seconds) if seconds < DAY => "%H:%M",
            Self::Seconds(_) => "%Y-%m-%d",
            Self::Months(months) if months < 12 => "%b %Y",
            Self::Months(_) => "%Y",
        }
    }

    /// The instants this step puts a tick on within `start..=end`.
    fn ticks(self, start: i64, end: i64) -> Vec<i64> {
        let mut out = Vec::new();
        match self {
            Self::Seconds(step) => {
                let mut at = time::align_up(start, step);
                while at <= end && out.len() < MAX_TICKS {
                    out.push(at);
                    at += step;
                }
            }
            Self::Months(step) => {
                // Step back to the month boundary `start` sits in, then align that
                // down to a whole multiple of the step so quarters and years land on
                // January rather than on whatever month the chart happens to open in.
                let step = step.max(1);
                let month = time::month_start(start);
                let mut at = time::add_months(month, -months_past_boundary(month, step));
                while out.len() < MAX_TICKS {
                    if at > end {
                        break;
                    }
                    if at >= start {
                        out.push(at);
                    }
                    at = time::add_months(at, step);
                }
            }
        }
        out
    }
}

/// How many whole months `seconds` sits past the previous multiple of `step`.
///
/// Months are counted from January of year 0, so a step of 3 puts ticks on calendar
/// quarters and a step of 12 puts them on January.
fn months_past_boundary(seconds: i64, step: i64) -> i64 {
    let at = DateTime::from_epoch(seconds);
    let index = at.year * 12 + i64::from(at.month) - 1;
    index - index.div_euclid(step.max(1)) * step.max(1)
}

/// The plot-local column a tick label starts at, given its width.
///
/// The label is centred on its tick and then nudged back inside the plot area. Tick
/// placement and the thinning pass that decides which ticks survive must agree about
/// this exactly, or labels are dropped for collisions that never happen — so both go
/// through here.
fn tick_left(column: usize, span: usize, columns: usize) -> usize {
    column
        .saturating_sub(span / 2)
        .min(columns.saturating_sub(span))
}

impl Axis {
    /// Chooses the finest tick spacing whose labels still fit side by side.
    fn choose(start: i64, end: i64, plot: usize, format: Option<&str>) -> Self {
        let mut fallback = None;
        for step in STEPS {
            let instants = step.ticks(start, end);
            if instants.is_empty() {
                continue;
            }
            let pattern = format.unwrap_or_else(|| step.default_format());
            let labels: Vec<String> = instants
                .iter()
                .map(|at| DateTime::from_epoch(*at).format(pattern))
                .collect();
            let widest = chrome::lines_width(&labels);
            let ticks: Vec<Tick> = instants
                .iter()
                .zip(labels)
                .map(|(at, label)| Tick {
                    column: column_of(*at, start, end, plot).min(plot.saturating_sub(1)),
                    label,
                })
                .collect();
            let mut axis = Self { ticks };
            let spacing = step.seconds() as f64 / (end - start).max(1) as f64 * plot as f64;
            if spacing >= (widest + TICK_PADDING) as f64 {
                axis.thin(plot);
                return axis;
            }
            fallback = Some(axis);
        }
        // Nothing fits comfortably: keep the coarsest spacing that produced any tick.
        let mut axis = fallback.unwrap_or(Self { ticks: Vec::new() });
        axis.thin(plot);
        axis
    }

    /// Drops ticks whose labels would overlap the previous one.
    ///
    /// Labels are nudged back inside the plot area when they are drawn, so the two
    /// outermost ones can still collide with their neighbours even when the nominal
    /// spacing was comfortable. Thinning models that same nudge.
    fn thin(&mut self, plot: usize) {
        let mut kept: Vec<Tick> = Vec::new();
        let mut next_free = 0usize;
        for tick in self.ticks.drain(..) {
            if tick.column >= plot {
                continue;
            }
            let span = display_width(&tick.label);
            let left = tick_left(tick.column, span, plot);
            if left >= next_free {
                next_free = left + span + TICK_PADDING;
                kept.push(tick);
            }
        }
        self.ticks = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::grapheme_width;

    #[test]
    fn every_bar_glyph_is_one_column_wide() {
        for glyph in ["░", "▒", "▓", "█", MILESTONE, "┬", "├", "│", "─"] {
            assert_eq!(grapheme_width(glyph), 1, "{glyph:?} must be one column");
        }
    }

    #[test]
    fn columns_map_endpoints_onto_the_plot() {
        assert_eq!(column_of(0, 0, 100, 10), 0);
        assert_eq!(column_of(50, 0, 100, 10), 5);
        assert_eq!(column_of(100, 0, 100, 10), 10);
        // Out-of-range instants are clamped rather than wrapping.
        assert_eq!(column_of(-50, 0, 100, 10), 0);
        assert_eq!(column_of(500, 0, 100, 10), 10);
    }

    #[test]
    fn a_zero_length_span_does_not_divide_by_zero() {
        assert_eq!(column_of(5, 5, 5, 10), 0);
    }

    #[test]
    fn month_ticks_land_on_calendar_boundaries() {
        let march = time::days_from_civil(2024, 3, 1) * DAY;
        assert_eq!(months_past_boundary(march, 1), 0);
        // March is the third month of its quarter-aligned run, so a quarterly step
        // walks back two months to January.
        assert_eq!(months_past_boundary(march, 3), 2);
        let quarter = time::add_months(march, -months_past_boundary(march, 3));
        assert_eq!(quarter, time::days_from_civil(2024, 1, 1) * DAY);
    }

    #[test]
    fn a_tick_label_is_centred_then_nudged_inside_the_plot() {
        // Centred in the middle of the plot.
        assert_eq!(tick_left(10, 4, 20), 8);
        // Never off the left edge, never past the right edge.
        assert_eq!(tick_left(0, 4, 20), 0);
        assert_eq!(tick_left(20, 4, 20), 16);
        // A label wider than the plot starts at zero instead of panicking.
        assert_eq!(tick_left(3, 40, 20), 0);
    }

    #[test]
    fn a_daily_span_gets_day_ticks() {
        let start = time::days_from_civil(2024, 1, 1) * DAY;
        let axis = Axis::choose(start, start + 7 * DAY, 80, None);
        assert!(axis.ticks.len() >= 2);
        assert!(axis.ticks.windows(2).all(|w| w[0].column < w[1].column));
    }
}
