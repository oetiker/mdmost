//! The chrome: table-of-contents pane, status bar and help overlay.
//!
//! All three are pure functions of [`App`]; the help overlay in particular is built
//! from [`super::help::sections`], which reads the live key table, so it cannot drift
//! from the bindings actually in force (design spec §10).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style as TermStyle;
use ratatui::text::{Line as TermLine, Span as TermSpan};
use ratatui::widgets::{Block, BorderType, Clear, Widget};

use crate::text::{display_width, truncate_to_width};
use crate::theme::Theme;

use super::app::{App, Focus, Overlay};
use super::draw::term_style;
use super::help;
use super::icons::{Icons, meter};

/// Draws the table-of-contents pane.
pub fn draw_toc(buffer: &mut Buffer, area: Rect, app: &App) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let theme = app.theme();
    let icons = Icons::new(app.icons());
    let focused = app.focus() == Focus::Toc;
    let border = if focused {
        theme.ui.toc_active
    } else {
        theme.ui.toc_border
    };
    let title = if app.toc_filter().is_empty() {
        format!(" {} Contents ", icons.toc)
    } else {
        format!(" {} {} ", icons.search, app.toc_filter())
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(term_style(border))
        .title(TermSpan::styled(title, term_style(theme.ui.help_title)))
        .render(area, buffer);

    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    if inner.height == 0 {
        return;
    }
    if app.toc_hits().is_empty() {
        let text = if app.toc().is_empty() {
            "no headings"
        } else {
            "no match"
        };
        buffer.set_string(inner.x, inner.y, text, term_style(theme.text.dim));
        return;
    }

    let current = app.current_heading();
    let first = app.toc_first_visible(usize::from(inner.height));
    for (offset, hit) in app
        .toc_hits()
        .iter()
        .skip(first)
        .take(usize::from(inner.height))
        .enumerate()
    {
        let Some(entry) = app.toc().entries().get(hit.index) else {
            continue;
        };
        let selected = first + offset == app.toc_cursor();
        let is_current = current == Some(hit.index);
        let base = match (selected && focused, is_current) {
            (true, _) => theme.ui.toc_active.reverse(),
            (false, true) => theme.ui.toc_active,
            (false, false) => theme.ui.toc_item,
        };
        let marker = if selected {
            icons.selected
        } else {
            icons.unselected
        };
        let indent = "  ".repeat(entry.depth.min(4));
        let prefix = format!("{marker}{indent} ");
        let room = usize::from(inner.width).saturating_sub(display_width(&prefix));
        let mut spans = vec![TermSpan::styled(prefix, term_style(base))];
        spans.extend(highlighted(
            truncate_to_width(&entry.text, room),
            &hit.positions,
            base,
            theme,
        ));
        let line = TermLine::from(spans).style(term_style(base));
        buffer.set_line(inner.x, inner.y + offset as u16, &line, inner.width);
    }
}

/// Splits `text` into spans so that fuzzy-match positions stand out.
fn highlighted(
    text: &str,
    positions: &[usize],
    base: crate::theme::Style,
    theme: &Theme,
) -> Vec<TermSpan<'static>> {
    if positions.is_empty() {
        return vec![TermSpan::styled(text.to_string(), term_style(base))];
    }
    let matched = term_style(base.patch(theme.ui.toc_match));
    let plain = term_style(base);
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut run_matched = false;
    for (index, ch) in text.chars().enumerate() {
        let is_match = positions.contains(&index);
        if is_match != run_matched && !run.is_empty() {
            spans.push(TermSpan::styled(
                std::mem::take(&mut run),
                if run_matched { matched } else { plain },
            ));
        }
        run_matched = is_match;
        run.push(ch);
    }
    if !run.is_empty() {
        spans.push(TermSpan::styled(
            run,
            if run_matched { matched } else { plain },
        ));
    }
    spans
}

/// Draws the status bar, or the prompt when one is open.
pub fn draw_status(buffer: &mut Buffer, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let theme = app.theme();
    let bar = term_style(theme.ui.status_bar);
    buffer.set_style(area, bar);

    if let Overlay::Prompt { kind, input } = app.overlay() {
        let line = TermLine::from(vec![
            TermSpan::styled(
                format!(" {} ", kind.sigil()),
                term_style(theme.ui.prompt.reverse()),
            ),
            TermSpan::styled(format!(" {input}"), term_style(theme.ui.prompt)),
            TermSpan::styled("\u{2588}", term_style(theme.ui.status_accent)),
        ]);
        buffer.set_line(area.x, area.y, &line, area.width);
        return;
    }

    let icons = Icons::new(app.icons());
    let sep = TermSpan::styled(
        format!(" {} ", icons.separator),
        term_style(theme.ui.status_bar.dim()),
    );

    let mut left = vec![
        TermSpan::styled(
            format!(" {} ", icons.file),
            term_style(theme.ui.status_accent),
        ),
        TermSpan::styled(
            app.title().to_string(),
            term_style(theme.ui.status_accent.bold()),
        ),
        sep.clone(),
        TermSpan::styled(
            format!("{:>3}%", (app.progress() * 100.0).round() as u16),
            term_style(theme.ui.status_bar),
        ),
        TermSpan::styled(" ", bar),
        TermSpan::styled(
            meter(app.progress(), 8),
            term_style(theme.ui.scrollbar_thumb),
        ),
    ];

    if let Some(notice) = app.notice() {
        let style = if notice.is_error {
            theme.ui.error
        } else {
            theme.ui.status_bar
        };
        left.push(sep.clone());
        if notice.is_error {
            left.push(TermSpan::styled(
                format!("{} ", icons.warning),
                term_style(style),
            ));
        }
        left.push(TermSpan::styled(notice.text.clone(), term_style(style)));
    } else if let Some(index) = app.current_heading()
        && let Some(entry) = app.toc().entries().get(index)
    {
        left.push(sep.clone());
        left.push(TermSpan::styled(
            format!("{} ", icons.heading),
            term_style(theme.ui.status_bar.dim()),
        ));
        left.push(TermSpan::styled(
            entry.text.clone(),
            term_style(theme.ui.status_bar),
        ));
    }

    let mut right = Vec::new();
    if !app.search().query().is_empty() {
        let position = app
            .search_index()
            .map(|index| format!("{}/{}", index + 1, app.search().len()))
            .unwrap_or_else(|| format!("{}", app.search().len()));
        right.push(TermSpan::styled(
            format!("{} {} {position}", icons.search, app.search().query()),
            term_style(theme.ui.status_bar),
        ));
        right.push(sep.clone());
    }
    let help_key = app
        .config()
        .keys
        .keys_for(crate::config::Action::Help)
        .first()
        .map(|key| key.label())
        .unwrap_or_else(|| "?".to_string());
    right.push(TermSpan::styled(
        format!("{help_key} help "),
        term_style(theme.ui.status_key),
    ));

    let left_width: usize = left.iter().map(|span| display_width(&span.content)).sum();
    let right_width: usize = right.iter().map(|span| display_width(&span.content)).sum();
    let gap = usize::from(area.width).saturating_sub(left_width + right_width);
    let mut spans = left;
    spans.push(TermSpan::styled(" ".repeat(gap), bar));
    spans.extend(right);
    let line = TermLine::from(spans).style(bar);
    buffer.set_line(area.x, area.y, &line, area.width);
}

/// The horizontal padding inside the help overlay's border, per side.
const HELP_PADDING: u16 = 2;
/// The gap between two help columns.
const HELP_GUTTER: u16 = 3;

/// Draws the help overlay, centred over the document.
///
/// The overlay never clips: when the sections do not fit the terminal's height they
/// are dealt into as many columns as the width allows, because the binding a reader
/// cannot see is the one they opened the help for.
pub fn draw_help(buffer: &mut Buffer, area: Rect, app: &App) {
    let theme = app.theme();
    let icons = Icons::new(app.icons());
    let sections = help::sections(&app.config().keys);
    let key_width = help::key_column_width(&sections);
    let column_width = sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .map(|row| key_width + 2 + display_width(row.description))
        .max()
        .unwrap_or(20) as u16;

    let room = area.width.saturating_sub(2 + HELP_PADDING * 2);
    let fit_columns = usize::from((room + HELP_GUTTER) / (column_width + HELP_GUTTER)).max(1);
    let rows = usize::from(area.height.saturating_sub(4)).max(1);
    let columns = help::columns(sections, rows, fit_columns);

    let used = columns.len() as u16;
    let width = (column_width * used + HELP_GUTTER * (used - 1) + 2 + HELP_PADDING * 2)
        .min(area.width)
        .max(3);
    let tallest = columns
        .iter()
        .map(|column| help::line_count(column))
        .max()
        .unwrap_or(0) as u16;
    let height = (tallest + 2).min(area.height).max(3);
    let overlay = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    Clear.render(overlay, buffer);
    buffer.set_style(overlay, term_style(theme.ui.status_bar));
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(term_style(theme.ui.help_border))
        .title(TermSpan::styled(
            format!(" {} Keys ", icons.help),
            term_style(theme.ui.help_title),
        ))
        .render(overlay, buffer);

    let inner = Rect::new(
        overlay.x + 1 + HELP_PADDING,
        overlay.y + 1,
        overlay.width.saturating_sub(2 + HELP_PADDING * 2),
        overlay.height.saturating_sub(2),
    );
    for (index, column) in columns.iter().enumerate() {
        let x = inner.x + index as u16 * (column_width + HELP_GUTTER);
        if x >= inner.x + inner.width {
            break;
        }
        let width = column_width.min(inner.x + inner.width - x);
        draw_help_column(
            buffer,
            Rect::new(x, inner.y, width, inner.height),
            column,
            key_width,
            theme,
        );
    }
}

/// Draws one column of the help overlay.
fn draw_help_column(
    buffer: &mut Buffer,
    area: Rect,
    sections: &[help::HelpSection],
    key_width: usize,
    theme: &Theme,
) {
    let mut y = 0u16;
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            y += 1;
        }
        if y >= area.height {
            return;
        }
        buffer.set_string(
            area.x,
            area.y + y,
            section.title,
            term_style(theme.ui.help_title),
        );
        y += 1;
        for row in &section.rows {
            if y >= area.height {
                return;
            }
            let line = TermLine::from(vec![
                TermSpan::styled(
                    format!("{:>width$}", row.keys, width = key_width),
                    term_style(theme.ui.status_key),
                ),
                TermSpan::styled("  ", TermStyle::default()),
                TermSpan::styled(row.description, term_style(theme.text.body)),
            ]);
            buffer.set_line(area.x, area.y + y, &line, area.width);
            y += 1;
        }
    }
}

/// Whether `column` falls inside the table-of-contents pane.
pub fn in_toc(app: &App, column: u16) -> bool {
    app.toc_is_open() && column < app.toc_width()
}

/// The table-of-contents list row a mouse click at `row` landed on, if any.
pub fn toc_row_at(app: &App, area_height: u16, row: u16) -> Option<usize> {
    if !app.toc_is_open() || row == 0 || area_height < 3 {
        return None;
    }
    let list_height = area_height.saturating_sub(3);
    let index = row.checked_sub(1)?;
    (index < list_height).then_some(usize::from(index))
}
