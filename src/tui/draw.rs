//! Painting the application onto a `ratatui` frame.
//!
//! Nothing here decides anything: every value drawn comes from [`super::app::App`].
//! That is the separation design spec §13 asks for — the state machine is testable
//! without a terminal, and this module is testable by eye.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color as TermColor, Modifier, Style as TermStyle};

use crate::canvas::Canvas;
use crate::theme::{Attributes, Color, Style};

use super::app::{App, Overlay};
use super::chrome;

/// Drawn in the first column when content is scrolled off to the left.
const LEFT_MARKER: &str = "\u{2039}";
/// Drawn in the last column when content continues past the right edge.
const RIGHT_MARKER: &str = "\u{203a}";

/// Draws one frame.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    app.resize(area.width, area.height);
    // Styles are `Copy`, so the handful the document needs are taken here rather than
    // cloning the whole theme — eighty-odd styles and a `String` — on every frame.
    let base = app.theme().base();
    let marker_style = term_style(app.theme().code.overflow_marker);
    let dim_style = term_style(app.theme().text.dim);

    let buffer = frame.buffer_mut();
    buffer.set_style(area, term_style(base));

    let toc_width = app.toc_width();
    let status_height = area.height.min(1);
    let body_height = area.height.saturating_sub(status_height);

    let toc_area = Rect::new(area.x, area.y, toc_width, body_height);
    let doc_area = Rect::new(
        area.x + toc_width,
        area.y,
        area.width.saturating_sub(toc_width).saturating_sub(1),
        body_height,
    );
    let bar_area = Rect::new(
        area.x + area.width.saturating_sub(1),
        area.y,
        area.width.min(1),
        body_height,
    );
    let status_area = Rect::new(area.x, area.y + body_height, area.width, status_height);

    // Render before reading scroll extents, so the first frame is already correct.
    let scroll = app.scroll();
    let _ = app.canvas();
    // One offset per row, so that the block that is too wide scrolls and the prose
    // around it does not. Computed once and shared: the document, its edge markers and
    // its search highlights disagreeing about where a row starts would paint chevrons
    // and highlights on the wrong columns.
    let hscroll = Offsets::new(app, doc_area.width);
    blit(buffer, doc_area, app.rendered(), scroll, &hscroll, base);
    edge_markers(
        buffer,
        doc_area,
        app.rendered(),
        scroll,
        &hscroll,
        marker_style,
    );
    highlight_matches(buffer, doc_area, app, scroll, &hscroll);
    scrollbar(buffer, bar_area, app);
    if app.rendered().is_empty() {
        empty_notice(buffer, doc_area, dim_style);
    }

    if toc_width > 0 {
        chrome::draw_toc(buffer, toc_area, app);
    }
    chrome::draw_status(buffer, status_area, app);

    if *app.overlay() == Overlay::Help {
        // The document area only: the status bar keeps its `h help` hint visible.
        chrome::draw_help(
            buffer,
            Rect::new(area.x, area.y, area.width, body_height),
            app,
        );
    }
}

/// Draws the frame shown while the document is still being laid out.
///
/// Deliberately reads nothing that would trigger a render: its whole purpose is to
/// reach the terminal *before* the expensive work, so that opening a large document
/// looks like opening a large document rather than like a hang.
pub fn draw_splash(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let theme = app.theme();
    let buffer = frame.buffer_mut();
    buffer.set_style(area, term_style(theme.base()));
    if area.height == 0 || area.width == 0 {
        return;
    }
    let row = area.y + area.height.saturating_sub(1);
    buffer.set_style(
        Rect::new(area.x, row, area.width, 1),
        term_style(theme.ui.status_bar),
    );
    buffer.set_string(
        area.x,
        row,
        crate::text::truncate_to_width(
            &format!(" {} — rendering…", app.title()),
            usize::from(area.width),
        ),
        term_style(theme.ui.status_bar),
    );
}

/// Where each document row starts, horizontally, in the viewport.
///
/// The horizontal offset is a single number the reader moves with `←`/`→`, but applying
/// it to every row drags the whole page sideways for the sake of one wide block. A row
/// is therefore moved only as far as it has anywhere to go — see
/// [`super::wide::scroll_reach`] for what "anywhere" means and why it is a property of
/// a run of rows rather than of one row.
struct Offsets<'a> {
    /// How far each row may be scrolled; one entry per canvas row.
    reach: &'a [u16],
    /// The offset the reader has scrolled to.
    offset: u16,
    /// The number of columns of document on screen.
    viewport: u16,
}

impl<'a> Offsets<'a> {
    /// Reads the offsets for the current frame.
    fn new(app: &'a App, viewport: u16) -> Self {
        Self {
            reach: app.reach(),
            offset: app.hscroll(),
            viewport,
        }
    }

    /// The first canvas column drawn on `row`.
    fn at(&self, row: usize) -> u16 {
        let reach = self.reach.get(row).copied().unwrap_or(0);
        self.offset.min(reach.saturating_sub(self.viewport))
    }
}

/// Copies a vertical slice of the document canvas into the frame buffer.
///
/// This is the only place canvas cells become terminal cells. Double-width characters
/// keep their trailing continuation cell, and a wide character sliced in half by the
/// horizontal offset is drawn as a space rather than a broken glyph.
fn blit(
    buffer: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    top: usize,
    left: &Offsets<'_>,
    base: Style,
) {
    for y in 0..area.height {
        let row = top + usize::from(y);
        let Some(cells) = canvas.row(row) else { break };
        let left = left.at(row);
        for x in 0..area.width {
            let column = usize::from(left) + usize::from(x);
            let Some(target) = buffer.cell_mut((area.x + x, area.y + y)) else {
                continue;
            };
            let Some(cell) = cells.get(column) else {
                break;
            };
            target.set_style(term_style(base.patch(cell.style())));
            if cell.is_continuation() {
                // Either the lead cell is on screen — in which case ratatui expects an
                // empty symbol here — or it was scrolled off to the left.
                target.set_symbol(if x == 0 { " " } else { "" });
            } else if cell.width() == 2 && x + 1 >= area.width {
                target.set_symbol(" ");
            } else {
                target.set_symbol(cell.text());
            }
        }
    }
}

/// Paints the left and right cut-off markers on rows that reach past the viewport.
///
/// The document canvas is now wider than the viewport wherever a block asked to be
/// (see [`super::wide`]), so the truncation the reader can see is a property of the
/// *window*, not of the render. Marking both edges is what tells them there is more
/// to the right and — the half that was missing entirely — that something is already
/// off to the left. Each row is measured against its own offset, so a row that stayed
/// at column 0 while a wide block scrolled past it is marked as what it is: whole.
fn edge_markers(
    buffer: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    top: usize,
    left: &Offsets<'_>,
    style: TermStyle,
) {
    if area.width == 0 {
        return;
    }
    for y in 0..area.height {
        let row = top + usize::from(y);
        let Some(cells) = canvas.row(row) else {
            break;
        };
        let left = left.at(row);
        let occupied = |range: std::ops::Range<usize>| {
            cells
                .get(range)
                .is_some_and(|slice| slice.iter().any(|cell| !cell.text().trim().is_empty()))
        };
        if left > 0
            && occupied(0..usize::from(left))
            && let Some(cell) = buffer.cell_mut((area.x, area.y + y))
        {
            cell.set_symbol(LEFT_MARKER);
            cell.set_style(style);
        }
        let right = usize::from(left) + usize::from(area.width);
        if occupied(right..cells.len())
            && let Some(cell) = buffer.cell_mut((area.x + area.width - 1, area.y + y))
        {
            cell.set_symbol(RIGHT_MARKER);
            cell.set_style(style);
        }
    }
}

/// Says so, rather than showing a screenful of nothing (usability P14).
fn empty_notice(buffer: &mut Buffer, area: Rect, style: TermStyle) {
    if area.width < 4 || area.height == 0 {
        return;
    }
    buffer.set_string(area.x, area.y, "(empty document)", style);
}

/// Repaints search matches on top of the document.
fn highlight_matches(buffer: &mut Buffer, area: Rect, app: &App, top: usize, left: &Offsets<'_>) {
    let theme = app.theme();
    let current = app.search_index();
    for y in 0..area.height {
        let row = top + usize::from(y);
        let left = left.at(row);
        for (index, segment) in app.search().segments_on_row(row) {
            let style = if Some(index) == current {
                theme.ui.search_current
            } else {
                theme.ui.search_match
            };
            let Some(start) = segment.col.checked_sub(left) else {
                continue;
            };
            for offset in 0..segment.cols {
                let x = start + offset;
                if x >= area.width {
                    break;
                }
                if let Some(cell) = buffer.cell_mut((area.x + x, area.y + y)) {
                    cell.set_style(patch_term(cell.style(), style));
                }
            }
        }
    }
}

/// Draws the document scrollbar in its one-column gutter.
///
/// The thumb is positioned to half-cell precision using the upper and lower half-block
/// glyphs, so scrolling a long document moves it smoothly rather than in whole rows.
fn scrollbar(buffer: &mut Buffer, area: Rect, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = app.theme();
    let track = term_style(theme.ui.scrollbar_track);
    let thumb = term_style(theme.ui.scrollbar_thumb);

    let halves = usize::from(area.height) * 2;
    let total = app.rendered().height().max(1);
    let visible = app.viewport_height().min(total);
    let length = ((visible * halves) / total).clamp(2, halves);
    let start = ((halves - length) as f32 * app.progress()).round() as usize;

    for y in 0..area.height {
        let upper = usize::from(y) * 2;
        let filled_upper = (start..start + length).contains(&upper);
        let filled_lower = (start..start + length).contains(&(upper + 1));
        let (symbol, style) = match (filled_upper, filled_lower) {
            (true, true) => ("\u{2588}", thumb),
            (true, false) => ("\u{2580}", thumb),
            (false, true) => ("\u{2584}", thumb),
            (false, false) => ("\u{2502}", track),
        };
        if let Some(cell) = buffer.cell_mut((area.x, area.y + y)) {
            cell.set_symbol(symbol);
            cell.set_style(style);
        }
    }
}

/// Converts an `mdless` style into a `ratatui` style.
pub fn term_style(style: Style) -> TermStyle {
    let mut out = TermStyle::default();
    if let Some(fg) = style.fg {
        out = out.fg(term_color(fg));
    }
    if let Some(bg) = style.bg {
        out = out.bg(term_color(bg));
    }
    let mut modifiers = Modifier::empty();
    for (attribute, modifier) in [
        (Attributes::BOLD, Modifier::BOLD),
        (Attributes::DIM, Modifier::DIM),
        (Attributes::ITALIC, Modifier::ITALIC),
        (Attributes::UNDERLINE, Modifier::UNDERLINED),
        (Attributes::STRIKETHROUGH, Modifier::CROSSED_OUT),
        (Attributes::REVERSE, Modifier::REVERSED),
    ] {
        if style.attrs.contains(attribute) {
            modifiers |= modifier;
        }
    }
    out.add_modifier(modifiers)
}

/// Lays an `mdless` style over an existing `ratatui` style, keeping what it leaves unset.
fn patch_term(under: TermStyle, over: Style) -> TermStyle {
    let mut out = under;
    if let Some(fg) = over.fg {
        out = out.fg(term_color(fg));
    }
    if let Some(bg) = over.bg {
        out = out.bg(term_color(bg));
    }
    out.patch(term_style(Style {
        fg: None,
        bg: None,
        attrs: over.attrs,
    }))
}

/// Converts a palette colour into a `ratatui` colour.
fn term_color(color: Color) -> TermColor {
    TermColor::Rgb(color.r, color.g, color.b)
}
