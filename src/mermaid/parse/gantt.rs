//! `gantt` parsing (design spec §6.6).
//!
//! Supported: `title`, `dateFormat`, `axisFormat`, `section`, and tasks with the
//! `name : [tags,] [id,] [start,] [end|duration]` metadata form, where the start is an
//! explicit date or `after id [id …]`, and the end is a date, a duration such as `3d`,
//! or `until id`. The `done`, `active`, `crit` and `milestone` tags are recognised.
//!
//! Dates are resolved here, not in the renderer: every [`GanttTask`] leaves the parser
//! with absolute `start`/`end` instants (design spec §3 forbids layout at parse time,
//! and calendar arithmetic is semantics, not layout).
//!
//! Skipped silently: `excludes`, `includes`, `todayMarker`, `tickInterval`, `weekday`,
//! `inclusiveEndDates`, `topAxis`, `displayMode`.

use crate::error::MermaidError;
use crate::mermaid::ast::{GanttChart, GanttSection, GanttTask, TaskProgress};
use crate::mermaid::entity;

use super::date;
use super::lex::{self, Nesting, SrcLine};

/// The `dateFormat` assumed when a chart does not set one.
const DEFAULT_DATE_FORMAT: &str = "YYYY-MM-DD";

/// Parses a whole `gantt` chart.
pub fn parse(lines: &[SrcLine<'_>], src: &str) -> Result<GanttChart, MermaidError> {
    let Some((header, body)) = lines.split_first() else {
        return Err(lex::syntax(1, "empty diagram"));
    };
    let mut builder = Builder {
        src,
        ..Builder::default()
    };
    let (_, rest) = lex::split_word(header.text);
    if !rest.is_empty() {
        builder.statement(rest, header.number)?;
    }
    for line in body {
        builder.statement(line.text, line.number)?;
    }
    Ok(GanttChart {
        title: builder.title,
        axis_format: builder.axis_format,
        sections: builder.sections,
    })
}

/// Accumulates sections while resolving task dates.
#[derive(Debug)]
struct Builder<'a> {
    /// The full mermaid source, passed to `lex::label_at` to compute a task name's
    /// byte range.
    src: &'a str,
    title: Option<String>,
    date_format: String,
    axis_format: Option<String>,
    sections: Vec<GanttSection>,
    /// Resolved ends by task id, for `after`/`until` references.
    ends: Vec<(String, i64, i64)>,
    /// The end of the most recent task, the implicit start of the next one.
    previous_end: Option<i64>,
}

impl Default for Builder<'_> {
    fn default() -> Self {
        Self {
            src: "",
            title: None,
            date_format: DEFAULT_DATE_FORMAT.to_string(),
            axis_format: None,
            sections: Vec::new(),
            ends: Vec::new(),
            previous_end: None,
        }
    }
}

impl Builder<'_> {
    /// Handles one source line.
    fn statement(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let (word, rest) = lex::split_word(text);
        match word.to_ascii_lowercase().as_str() {
            "excludes" | "includes" | "todaymarker" | "tickinterval" | "weekday"
            | "inclusiveenddates" | "topaxis" | "displaymode" => return Ok(()),
            "title" => {
                self.title = Some(entity::decode(lex::unquote(rest)).into_owned());
                return Ok(());
            }
            "dateformat" => {
                self.date_format = rest.trim().to_string();
                return Ok(());
            }
            "axisformat" => {
                self.axis_format = Some(rest.trim().to_string());
                return Ok(());
            }
            "section" => {
                let title = entity::decode(lex::unquote(rest).trim()).into_owned();
                if title.is_empty() {
                    return Err(lex::syntax(line, "section has no name".to_string()));
                }
                self.sections.push(GanttSection {
                    title: Some(title),
                    tasks: Vec::new(),
                });
                return Ok(());
            }
            _ => {}
        }
        self.task(text, line)
    }

    /// Parses and resolves one task line.
    fn task(&mut self, text: &str, line: usize) -> Result<(), MermaidError> {
        let Some((name, meta)) = lex::split_once_top_level(text, ':', Nesting::Ignore) else {
            return Err(lex::syntax(
                line,
                format!("cannot read a task from `{text}`"),
            ));
        };
        let spec = self.metadata(meta, line)?;
        let start = match &spec.start {
            Some(Start::At(at)) => *at,
            Some(Start::After(ids)) => self.reference(ids, line, |_, end| end, i64::max)?,
            None => self
                .previous_end
                .ok_or_else(|| lex::syntax(line, "first task has no start date"))?,
        };
        let end = match &spec.end {
            Some(End::At(at)) => *at,
            // Both operands are clamped to the drawable range, but a saturating add
            // makes the absence of overflow local and obvious rather than a property
            // of two other modules (design spec §12).
            Some(End::Duration(seconds)) => {
                crate::mermaid::gantt::time::clamp_instant(start.saturating_add(*seconds))
            }
            Some(End::Until(ids)) => self.reference(ids, line, |start, _| start, i64::min)?,
            None if spec.milestone => start,
            None => return Err(lex::syntax(line, "task has no end date or duration")),
        };
        let end = end.max(start);
        self.previous_end = Some(end);
        if let Some(id) = &spec.id {
            self.ends.push((id.clone(), start, end));
        }
        if self.sections.is_empty() {
            self.sections.push(GanttSection {
                title: None,
                tasks: Vec::new(),
            });
        }
        if let Some(section) = self.sections.last_mut() {
            section.tasks.push(GanttTask {
                name: lex::label_at(self.src, lex::unquote(name)),
                id: spec.id,
                progress: spec.progress,
                critical: spec.critical,
                milestone: spec.milestone,
                start,
                end,
            });
        }
        Ok(())
    }

    /// Resolves an `after`/`until` reference list to a single instant.
    fn reference(
        &self,
        ids: &[String],
        line: usize,
        pick: impl Fn(i64, i64) -> i64,
        combine: impl Fn(i64, i64) -> i64,
    ) -> Result<i64, MermaidError> {
        let mut result = None;
        for id in ids {
            let (start, end) = self
                .ends
                .iter()
                .find(|(known, _, _)| known == id)
                .map(|(_, start, end)| (*start, *end))
                .ok_or_else(|| lex::syntax(line, format!("unknown task id `{id}`")))?;
            let value = pick(start, end);
            result = Some(match result {
                None => value,
                Some(previous) => combine(previous, value),
            });
        }
        result.ok_or_else(|| lex::syntax(line, "reference without a task id"))
    }

    /// Parses the comma-separated metadata after a task's `:`.
    fn metadata(&self, meta: &str, line: usize) -> Result<TaskSpec, MermaidError> {
        let mut spec = TaskSpec::default();
        for token in lex::split_top_level(meta, ',', Nesting::Ignore) {
            match token.to_ascii_lowercase().as_str() {
                "done" => {
                    spec.progress = TaskProgress::Done;
                    continue;
                }
                "active" => {
                    spec.progress = TaskProgress::Active;
                    continue;
                }
                "crit" => {
                    spec.critical = true;
                    continue;
                }
                "milestone" => {
                    spec.milestone = true;
                    continue;
                }
                _ => {}
            }
            if let Some(ids) = reference_ids(token, "after") {
                if spec.start.is_some() {
                    return Err(lex::syntax(line, "task with two start dates"));
                }
                spec.start = Some(Start::After(ids));
                continue;
            }
            if let Some(ids) = reference_ids(token, "until") {
                if spec.end.is_some() {
                    return Err(lex::syntax(line, "task with two end dates"));
                }
                spec.end = Some(End::Until(ids));
                continue;
            }
            if let Some(at) = date::parse_date(token, &self.date_format) {
                if spec.start.is_none() {
                    spec.start = Some(Start::At(at));
                } else if spec.end.is_none() {
                    spec.end = Some(End::At(at));
                } else {
                    return Err(lex::syntax(line, format!("unexpected date `{token}`")));
                }
                continue;
            }
            if let Some(seconds) = date::parse_duration(token, line)? {
                if spec.end.is_some() {
                    return Err(lex::syntax(line, "task with two durations"));
                }
                spec.end = Some(End::Duration(seconds));
                continue;
            }
            if spec.id.is_some() || spec.start.is_some() {
                return Err(lex::syntax(
                    line,
                    format!("cannot read task metadata `{token}`"),
                ));
            }
            spec.id = Some(token.to_string());
        }
        Ok(spec)
    }
}

/// The metadata of one task, before dates are resolved.
#[derive(Debug, Default)]
struct TaskSpec {
    id: Option<String>,
    progress: TaskProgress,
    critical: bool,
    milestone: bool,
    start: Option<Start>,
    end: Option<End>,
}

/// How a task's start is written.
#[derive(Debug)]
enum Start {
    /// An explicit date in the chart's `dateFormat`.
    At(i64),
    /// `after id [id …]` — the latest end of the referenced tasks.
    After(Vec<String>),
}

/// How a task's end is written.
#[derive(Debug)]
enum End {
    /// An explicit date in the chart's `dateFormat`.
    At(i64),
    /// A duration literal such as `3d`.
    Duration(i64),
    /// `until id [id …]` — the earliest start of the referenced tasks.
    Until(Vec<String>),
}

/// Splits `after a b` into its task ids, if `token` uses `keyword`.
fn reference_ids(token: &str, keyword: &str) -> Option<Vec<String>> {
    let rest = lex::strip_keyword(token, keyword)?;
    let ids: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
    (!ids.is_empty()).then_some(ids)
}
