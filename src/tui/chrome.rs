//! The chrome: table-of-contents pane, status bar and help overlay.
//!
//! The table-of-contents pane and the status bar are pure functions of [`App`]. The
//! help overlay takes `&mut App` for one reason: how far it can scroll depends on how
//! many rows and columns it just laid out at this terminal size, which only the
//! drawing code knows, so it clamps [`App::help_scroll`] on the way past. Its content
//! is still built from [`super::help::sections`], which reads the live key table, so
//! it cannot drift from the bindings actually in force (design spec §10).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style as TermStyle;
use ratatui::text::{Line as TermLine, Span as TermSpan};
use ratatui::widgets::{Block, BorderType, Clear, Widget};

use crate::canvas::meter::meter;
use crate::search::SearchMode;
use crate::text::{Align, display_width, truncate_to_width};
use crate::theme::Theme;

use super::app::{App, Focus, Overlay};
use super::draw::term_style;
use super::help;
use super::icons::Icons;

/// Shortens `text` to `width` columns, marking the cut with an ellipsis.
///
/// One truncation idiom for all the chrome: the visual review counted three across the
/// program and rightly called that an accident rather than a decision.
fn fit(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    match width {
        0 => String::new(),
        _ => format!("{}\u{2026}", truncate_to_width(text, width - 1)),
    }
}

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
    // The pane carries the theme's own background rather than the terminal's, or a
    // light theme on a dark terminal reads as a hole (visual review B1).
    buffer.set_style(area, term_style(theme.base()));
    let title = if app.toc_filter().is_empty() {
        format!(" {} Contents ", icons.toc)
    } else {
        // A filtered pane says how much of the map it is still showing; without a
        // count a one-hit filter is indistinguishable from a broken table of contents.
        format!(
            " {} {} {}/{} ",
            icons.search,
            fit(app.toc_filter(), usize::from(area.width).saturating_sub(12)),
            app.toc_hits().len(),
            app.toc().len()
        )
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
        // One column of right padding, so entries never weld themselves to the border.
        // The indent gives way before the text does: a deep heading in a narrow pane
        // must still say *something*, and an entry indented off the edge is a blank
        // row the reader cannot tell from a bug.
        let usable = usize::from(inner.width).saturating_sub(1);
        let fixed = display_width(marker) + 1;
        let depth = entry
            .depth
            .min(4)
            .min(usable.saturating_sub(fixed + MIN_TOC_TEXT) / 2);
        let prefix = format!("{marker}{}{}", "  ".repeat(depth), " ");
        let mut room = usable.saturating_sub(display_width(&prefix));
        let mut spans = vec![TermSpan::styled(prefix, term_style(base))];
        // The section number, in the same quiet slot the page draws it in (design spec
        // §9.3) — patched over `base`, so a selected or current row keeps its wash and
        // the number goes on being the quieter half of the line. It gives way before
        // the text does, on the same reasoning as the indent above: an entry that is
        // all number and no words is a row the reader cannot use.
        if let Some(number) = &entry.number {
            let text = format!("{number} ");
            let cost = display_width(&text);
            if room.saturating_sub(cost) >= MIN_TOC_TEXT {
                room -= cost;
                spans.push(TermSpan::styled(
                    text,
                    term_style(base.patch(theme.heading_number)),
                ));
            }
        }
        spans.extend(highlighted(
            &fit(&entry.text, room),
            &hit.positions,
            base,
            theme,
        ));
        let line = TermLine::from(spans).style(term_style(base));
        buffer.set_line(inner.x, inner.y + offset as u16, &line, inner.width);
    }
}

/// Splits `text` into spans so that fuzzy-match positions stand out.
///
/// Walks grapheme clusters, not `char`s. [`crate::toc::FilterHit::positions`] are
/// character indices, so a cluster counts as matched when any of its characters does —
/// which keeps a base character and its combining marks in the same span. Splitting
/// them across two spans is how a highlighted accented heading loses its accent.
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
    let mut index = 0usize;
    for cluster in crate::text::graphemes(text) {
        let chars = cluster.chars().count();
        let is_match = (index..index + chars).any(|at| positions.contains(&at));
        index += chars;
        if is_match != run_matched && !run.is_empty() {
            spans.push(TermSpan::styled(
                std::mem::take(&mut run),
                if run_matched { matched } else { plain },
            ));
        }
        run_matched = is_match;
        run.push_str(cluster);
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
                format!(" {} ", kind.sigil(app.search_mode())),
                term_style(theme.ui.prompt.reverse()),
            ),
            TermSpan::styled(format!(" {input}"), term_style(theme.ui.prompt)),
            TermSpan::styled("\u{2588}", term_style(theme.ui.status_accent)),
        ]);
        buffer.set_line(area.x, area.y, &line, area.width);
        return;
    }

    let icons = Icons::new(app.icons());
    let sep = |spans: &mut Vec<TermSpan<'static>>| {
        spans.push(TermSpan::styled(
            format!(" {} ", icons.separator),
            term_style(theme.ui.status_bar.dim()),
        ));
    };

    // Every segment carries a drop priority. Nothing is ever cut mid-word at the far
    // end of the bar (usability P1, visual P11/P15c): whole segments go, cheapest
    // first, and the quit hint is the one thing that never goes at all (P2).
    let mut left: Vec<Segment> = Vec::new();
    let mut right: Vec<Segment> = Vec::new();

    // The file name is the one segment that can lose characters and still mean
    // something, so it is elided rather than let any segment from `ELIDE_TO_KEEP` up be
    // dropped, and given up altogether only when eliding it away is still not enough.
    // Segments cheaper than that — a breadcrumb, a search chip, the meter — go first,
    // because each of them is restated somewhere the reader can already see.
    left.push(Segment::new(
        Drop::Title,
        vec![
            TermSpan::styled(
                format!(" {} ", icons.file),
                term_style(theme.ui.status_accent),
            ),
            TermSpan::styled(
                app.title().to_string(),
                term_style(theme.ui.status_accent.bold()),
            ),
        ],
    ));

    let mut position = Vec::new();
    sep(&mut position);
    position.push(TermSpan::styled(
        format!("{:>4}", position_label(app)),
        term_style(theme.ui.status_bar),
    ));
    left.push(Segment::new(Drop::Never, position));

    // The meter sits on a visible trough: eight blank cells at 0 % read as a hole in
    // the bar rather than as an empty gauge (visual review P9), and the whole gauge
    // takes the bar's own background so it is part of the bar, not a patch on it.
    //
    // The part-filled cell is the exception, and the reason `meter` returns three runs.
    // An eighth block paints only the left fraction of its cell; the rest shows the
    // cell's background, so on the bar's background that one cell had a bar-coloured
    // gap in it while the trough beside it was track-coloured — a discontinuity at
    // exactly the boundary the eye reads the value off (owner report). Putting the
    // track colour behind it makes the cell read as "part filled" instead.
    let gauge = meter(f64::from(app.progress()), METER_WIDTH);
    let thumb = on_bar(theme.ui.scrollbar_thumb, theme);
    left.push(Segment::new(
        Drop::Meter,
        vec![
            TermSpan::styled(" ", bar),
            TermSpan::styled(gauge.full, term_style(thumb)),
            TermSpan::styled(
                gauge.partial,
                term_style(crate::theme::Style {
                    bg: theme.ui.scrollbar_track.fg,
                    ..thumb
                }),
            ),
            TermSpan::styled(
                gauge.trough,
                term_style(on_bar(theme.ui.scrollbar_track, theme)),
            ),
        ],
    ));

    // Design spec §10: a document with content past the right edge must say so, or a
    // reader who bumps the arrow key cannot tell why part of the page moved — and a
    // reader who never bumps it is never told the content was there. So the chip
    // appears at offset 0 as well, reading `↔ 0/57`. It outranks the breadcrumb when
    // the bar is too narrow for both: the heading is on screen, the missing columns
    // are not.
    if app.hscroll_max() > 0 {
        let mut spans = Vec::new();
        sep(&mut spans);
        spans.push(TermSpan::styled(
            format!(
                "{} {}/{}",
                icons.horizontal,
                app.hscroll(),
                app.hscroll_max()
            ),
            term_style(theme.ui.status_accent),
        ));
        left.push(Segment::new(Drop::Hscroll, spans));
    }

    if let Some(notice) = app.notice() {
        let style = if notice.is_error {
            theme.ui.error
        } else {
            theme.ui.status_bar
        };
        let mut spans = Vec::new();
        sep(&mut spans);
        if notice.is_error {
            spans.push(TermSpan::styled(
                format!("{} ", icons.warning),
                term_style(style),
            ));
        }
        spans.push(TermSpan::styled(notice.text.clone(), term_style(style)));
        left.push(Segment::new(Drop::Context, spans));
    } else if let Some(index) = app.current_heading()
        && let Some(entry) = app.toc().entries().get(index)
    {
        let mut spans = Vec::new();
        sep(&mut spans);
        spans.push(TermSpan::styled(
            format!("{} ", icons.heading),
            term_style(theme.ui.status_bar.dim()),
        ));
        spans.push(TermSpan::styled(
            entry.text.clone(),
            term_style(theme.ui.status_bar),
        ));
        left.push(Segment::new(Drop::Context, spans));
    }

    if !app.search().query().is_empty() {
        let count = app
            .search_index()
            .map(|index| format!("{}/{}", index + 1, app.search().len()))
            .unwrap_or_else(|| format!("{}", app.search().len()));
        let mut spans = Vec::new();
        // The mode is named, never inferred: `re` marks a regular expression, and its
        // absence marks a literal search.
        if app.search().mode() == SearchMode::Regex {
            spans.push(TermSpan::styled(
                " re ",
                term_style(theme.ui.status_key.reverse()),
            ));
        }
        spans.push(TermSpan::styled(
            format!(" {} {} {count} ", icons.search, app.search().query()),
            term_style(theme.ui.status_bar),
        ));
        right.push(Segment::new(Drop::Search, spans));
    }

    let help_key = app
        .config()
        .keys
        .keys_for(crate::config::Action::Help)
        .first()
        .map(|key| key.label())
        .unwrap_or_else(|| "?".to_string());
    right.push(Segment::new(
        Drop::Never,
        // Patched over the bar so the hint is a chip *in* the bar rather than six
        // columns of the terminal's own background (visual review P10).
        vec![TermSpan::styled(
            format!(" {help_key} help "),
            term_style(on_bar(theme.ui.status_key, theme)),
        )],
    ));

    let spans = lay_out(left, right, usize::from(area.width), bar);
    let line = TermLine::from(spans).style(bar);
    buffer.set_line(area.x, area.y, &line, area.width);
}

/// The width of the status bar's progress meter, in cells.
const METER_WIDTH: usize = 8;

/// How readily a status-bar segment is given up when the terminal is narrow.
///
/// Ordered least valuable first, which is the order they are dropped in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Drop {
    /// The heading, or the transient notice standing in for it.
    Context,
    /// The search chip.
    Search,
    /// The progress meter, whose percentage is stated next to it anyway.
    Meter,
    /// The horizontal-offset chip.
    Hscroll,
    /// The file name, which is elided before it is given up altogether.
    Title,
    /// The position and the way out.
    Never,
}

/// The cheapest segment the file name is shortened in order to keep.
///
/// Everything below it goes before the name loses a character, because everything below
/// it is said somewhere else: the breadcrumb is on the page, the search chip is what the
/// reader just typed, and the meter's value is printed beside it in words. From here up
/// nothing else on the bar can shrink and nothing else says what it says — so the name,
/// the one segment that can lose characters and still mean something, pays instead.
///
/// This is deliberately *not* "elide before anything at all is dropped": that reading
/// spends thirteen columns of a file name to keep an eight-column gauge whose percentage
/// is already on screen.
const ELIDE_TO_KEEP: Drop = Drop::Hscroll;

/// One status-bar segment, dropped or kept whole.
struct Segment {
    /// When this segment is given up.
    priority: Drop,
    /// How many columns it occupies.
    width: usize,
    /// What it draws.
    spans: Vec<TermSpan<'static>>,
}

impl Segment {
    /// Measures a segment as it is built.
    fn new(priority: Drop, spans: Vec<TermSpan<'static>>) -> Self {
        let width = spans.iter().map(|span| display_width(&span.content)).sum();
        Self {
            priority,
            width,
            spans,
        }
    }
}

/// The file name as it currently reads, while it is still on the bar.
///
/// It is the leftmost segment's last span — the icon in front of it is not elidable.
fn title(left: &[Segment]) -> Option<&str> {
    left.first()
        .filter(|segment| segment.priority == Drop::Title)
        .and_then(|segment| segment.spans.last())
        .map(|span| span.content.as_ref())
}

/// Lays segments out across the bar, dropping the cheapest until they fit.
fn lay_out(
    mut left: Vec<Segment>,
    mut right: Vec<Segment>,
    width: usize,
    bar: TermStyle,
) -> Vec<TermSpan<'static>> {
    let total = |segments: &[Segment]| -> usize { segments.iter().map(|s| s.width).sum() };
    // What the file name could give up if it were elided away entirely, measured through
    // `fit` so it cannot drift from what the elision below actually reclaims.
    let elidable = |left: &[Segment]| -> usize {
        title(left).map_or(0, |name| {
            display_width(name).saturating_sub(display_width(&fit(name, 0)))
        })
    };
    // One column of clear air between the two halves, always — hence the strict `<`.
    // `slack` is what the file name is willing to give up, when it is willing to.
    let fits = |left: &[Segment], right: &[Segment], slack: usize| {
        total(left) + total(right) < width + slack
    };
    loop {
        if fits(&left, &right, 0) {
            break;
        }
        let worst = left
            .iter()
            .chain(right.iter())
            .map(|segment| segment.priority)
            .filter(|priority| *priority != Drop::Never)
            .min();
        let Some(worst) = worst else { break };
        // From `ELIDE_TO_KEEP` up, the file name gives up its own columns rather than let
        // a segment go: the elision used to run only *after* this loop, so a forty-column
        // terminal silently traded the `↔ n/N` chip — on the one terminal where
        // horizontal scrolling matters most — for a long file name, and left ten columns
        // of the bar empty doing it.
        if worst >= ELIDE_TO_KEEP && fits(&left, &right, elidable(&left)) {
            break;
        }
        left.retain(|segment| segment.priority != worst);
        right.retain(|segment| segment.priority != worst);
    }
    // Now the file name — the one segment that can lose characters and still mean
    // something — gives up exactly the difference the loop above counted on it for. The
    // quit hint is never what goes (usability P2). Guarded on the segment still *being*
    // the file name: once it has been dropped, the leftmost segment is the position, and
    // eliding `0%` would be nonsense.
    let mut used = total(&left) + total(&right);
    if used + 1 > width
        && let Some(name) = left
            .first_mut()
            .filter(|segment| segment.priority == Drop::Title)
            .and_then(|segment| segment.spans.last_mut())
    {
        let room = display_width(&name.content).saturating_sub(used + 1 - width);
        let short = fit(&name.content, room);
        used -= display_width(&name.content) - display_width(&short);
        name.content = short.into();
    }
    let gap = width.saturating_sub(used);
    let mut spans: Vec<TermSpan<'static>> = Vec::new();
    for segment in left {
        spans.extend(segment.spans);
    }
    spans.push(TermSpan::styled(" ".repeat(gap), bar));
    for segment in right {
        spans.extend(segment.spans);
    }
    spans
}

/// Puts a style's foreground on the status bar's own background.
fn on_bar(style: crate::theme::Style, theme: &Theme) -> crate::theme::Style {
    crate::theme::Style {
        bg: theme.ui.status_bar.bg,
        ..style
    }
}

/// Puts a style's foreground on the help overlay panel's own background.
///
/// The panel is washed with `ui.status_bar`, but every style the panel then draws in —
/// `text.body`, `ui.help_title`, `ui.help_border` — carries the *page* background,
/// because that is the right background everywhere else they are used. Painting them
/// unpatched put a slab of page background behind each string, exactly as wide as the
/// string: the overlay read as if its text had been gone over with a marker pen
/// (visual review, finding 2). Same shape as [`on_bar`]; the overlay simply borrows the
/// bar's surface.
fn on_panel(style: crate::theme::Style, theme: &Theme) -> crate::theme::Style {
    on_bar(style, theme)
}

/// How the position reads: a percentage, or a word when a percentage would mislead.
///
/// A document that fits on one screen is not "100 % read" with the cursor at the top
/// (usability P13); `less` says `(END)` and this says `All`.
fn position_label(app: &App) -> String {
    if app.max_scroll() == 0 {
        return "All".to_string();
    }
    if app.scroll() >= app.max_scroll() {
        return "End".to_string();
    }
    format!("{}%", (app.progress() * 100.0).round() as u16)
}

/// The horizontal padding inside the help overlay's border, per side.
const HELP_PADDING: u16 = 2;
/// The gap between two help columns.
const HELP_GUTTER: u16 = 3;

/// The narrowest a table-of-contents entry's text is ever squeezed to.
const MIN_TOC_TEXT: usize = 6;

/// The clear space kept between the overlay and the edge of the document area.
const HELP_MARGIN: u16 = 2;
/// Below this many rows the overlay gives up on being a panel.
const HELP_MIN_PANEL_HEIGHT: u16 = 7;

/// Draws the help overlay over the document, leaving the status bar visible.
///
/// The overlay never hides the way out. It reflows into as many columns as the width
/// allows, scrolls when even that is not enough, says so in its bottom border, and at
/// a height where no panel can be honest it collapses to a single strip of the keys
/// that matter. `area` is the document area only: covering the status bar as well
/// (usability B4) would take away the last hint a trapped reader has.
pub fn draw_help(buffer: &mut Buffer, area: Rect, app: &mut App) {
    if area.width < 12 || area.height == 0 {
        return;
    }
    let theme = app.theme().clone();
    scrim(buffer, area, &theme);
    if area.height < HELP_MIN_PANEL_HEIGHT {
        draw_help_strip(buffer, area, app, &theme);
        return;
    }

    let icons = Icons::new(app.icons());
    let sections = help::sections(&app.config().keys);
    let key_width = help::key_column_width(&sections);
    let column_width = sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .map(|row| key_width + 2 + display_width(row.description))
        .max()
        .unwrap_or(20) as u16;

    let outer = Rect::new(
        area.x + HELP_MARGIN,
        area.y + 1,
        area.width.saturating_sub(HELP_MARGIN * 2),
        area.height.saturating_sub(2),
    );
    let room = outer.width.saturating_sub(2 + HELP_PADDING * 2);
    let column_width = column_width.min(room.max(1));
    let fit_columns = usize::from((room + HELP_GUTTER) / (column_width + HELP_GUTTER)).max(1);
    let rows = usize::from(outer.height.saturating_sub(2)).max(1);
    let columns = help::columns(sections, rows, fit_columns);

    let used = columns.len() as u16;
    let width = (column_width * used + HELP_GUTTER * (used - 1) + 2 + HELP_PADDING * 2)
        .min(outer.width)
        .max(3);
    let lines: Vec<Vec<help::HelpLine<'_>>> = columns.iter().map(|c| help::lines(c)).collect();
    let tallest = lines.iter().map(Vec::len).max().unwrap_or(0);
    let height = (u16::try_from(tallest).unwrap_or(u16::MAX).saturating_add(2))
        .min(outer.height)
        .max(3);
    let visible = usize::from(height.saturating_sub(2));
    app.clamp_help_scroll(tallest.saturating_sub(visible));
    let scroll = app.help_scroll();

    let overlay = Rect::new(
        outer.x + (outer.width.saturating_sub(width)) / 2,
        outer.y + (outer.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    Clear.render(overlay, buffer);
    buffer.set_style(overlay, term_style(theme.ui.status_bar));
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(term_style(on_panel(theme.ui.help_border, &theme)))
        .title(TermSpan::styled(
            format!(" {} Keys ", icons.help),
            term_style(on_panel(theme.ui.help_title, &theme)),
        ));
    if tallest > visible {
        let hidden = tallest - visible - scroll;
        let note = if hidden > 0 {
            format!(" \u{2193} {hidden} more — j k scroll ")
        } else {
            " \u{2191} k scrolls back ".to_string()
        };
        block = block.title_bottom(TermSpan::styled(
            note,
            term_style(on_panel(theme.ui.help_title, &theme)),
        ));
    }
    block.render(overlay, buffer);

    let inner = Rect::new(
        overlay.x + 1 + HELP_PADDING,
        overlay.y + 1,
        overlay.width.saturating_sub(2 + HELP_PADDING * 2),
        overlay.height.saturating_sub(2),
    );
    for (index, column) in lines.iter().enumerate() {
        let x = inner.x + index as u16 * (column_width + HELP_GUTTER);
        if x >= inner.x + inner.width {
            break;
        }
        let width = column_width.min(inner.x + inner.width - x);
        draw_help_column(
            buffer,
            Rect::new(x, inner.y, width, inner.height),
            column,
            scroll,
            key_width,
            &theme,
        );
    }
}

/// Dims the document behind the overlay so the panel reads as a panel.
///
/// Without this the document's own table and code borders run straight through the
/// overlay's frame and out the other side (visual review B7).
fn scrim(buffer: &mut Buffer, area: Rect, theme: &Theme) {
    // An explicit muted colour rather than the DIM attribute, which a fair number of
    // terminals quietly ignore.
    let style = term_style(crate::theme::Style {
        fg: theme.text.dim.fg,
        bg: theme.base().bg,
        attrs: crate::theme::Attributes::NONE,
    });
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_style(style);
            }
        }
    }
}

/// The last-resort help: one line naming the keys that get the reader out.
fn draw_help_strip(buffer: &mut Buffer, area: Rect, app: &App, theme: &Theme) {
    let label = |action: crate::config::Action| -> String {
        app.config()
            .keys
            .keys_for(action)
            .first()
            .map(|key| key.label())
            .unwrap_or_else(|| "?".to_string())
    };
    let text = fit(
        &format!(
            " {} quit  {} back  {} help  {} contents ",
            label(crate::config::Action::Quit),
            label(crate::config::Action::Cancel),
            label(crate::config::Action::Help),
            label(crate::config::Action::ToggleToc),
        ),
        usize::from(area.width),
    );
    let row = area.y + area.height / 2;
    let line = TermLine::from(vec![TermSpan::styled(
        text,
        term_style(theme.ui.status_key),
    )]);
    buffer.set_line(area.x, row, &line, area.width);
}

/// Draws one column of the help overlay, from `scroll` lines in.
fn draw_help_column(
    buffer: &mut Buffer,
    area: Rect,
    lines: &[help::HelpLine<'_>],
    scroll: usize,
    key_width: usize,
    theme: &Theme,
) {
    for (y, line) in lines
        .iter()
        .skip(scroll)
        .take(usize::from(area.height))
        .enumerate()
    {
        let y = area.y + y as u16;
        match line {
            help::HelpLine::Blank => {}
            help::HelpLine::Title(title) => {
                buffer.set_string(
                    area.x,
                    y,
                    *title,
                    term_style(on_panel(theme.ui.help_title, theme)),
                );
            }
            help::HelpLine::Row(row) => {
                let text = TermLine::from(vec![
                    // Padded by display columns, not by `char` count: the key column
                    // is measured with `display_width`, and a wide-character binding
                    // would otherwise ragged-edge every row in the column.
                    TermSpan::styled(
                        crate::text::pad_to_width(&row.keys, key_width, Align::Right),
                        term_style(on_panel(theme.ui.status_key, theme)),
                    ),
                    TermSpan::styled("  ", TermStyle::default()),
                    TermSpan::styled(
                        fit(
                            row.description,
                            usize::from(area.width).saturating_sub(key_width + 2),
                        ),
                        term_style(on_panel(theme.text.body, theme)),
                    ),
                ]);
                buffer.set_line(area.x, y, &text, area.width);
            }
        }
    }
}

/// Whether `column` falls inside the table-of-contents pane.
pub fn in_toc(app: &App, column: u16) -> bool {
    app.toc_is_open() && column < app.toc_width()
}

/// Whether `column` falls in the one-column gutter the document scrollbar is drawn in.
///
/// The bar is always there, whatever the contents pane is doing, so unlike
/// [`in_toc`] this asks nothing about state beyond the terminal's width.
pub fn in_scrollbar(app: &App, column: u16) -> bool {
    column == app.scrollbar_column()
}

/// The table-of-contents list row a mouse click at `row` landed on, if any.
///
/// The pane's row 0 and row `area_height - 1` are its border, so the list occupies
/// rows `1..=area_height - 2`, which is `area_height - 2` rows. Getting this bound
/// wrong makes the bottom-most entry silently unclickable.
pub fn toc_row_at(app: &App, area_height: u16, row: u16) -> Option<usize> {
    if !app.toc_is_open() || row == 0 || area_height < 3 {
        return None;
    }
    let list_height = area_height.saturating_sub(2);
    let index = row.checked_sub(1)?;
    (index < list_height).then_some(usize::from(index))
}
