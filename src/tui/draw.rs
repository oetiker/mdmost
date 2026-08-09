//! Painting the application onto a `ratatui` frame.
//!
//! Nothing here decides anything: every value drawn comes from [`super::app::App`].
//! That is the separation design spec §13 asks for — the state machine is testable
//! without a terminal, and this module is testable by eye.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color as TermColor, Modifier, Style as TermStyle};

use crate::canvas::{BorderSet, Canvas, Cell, Rule, Side};
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
    // The styles a box's own glyphs are painted in, so a viewport edge that cuts a rule
    // can close it rather than stamp a chevron over it. Diagram box art is deliberately
    // not here, and now that diagrams are scrollable that is a decision rather than a
    // gap: a table or a code fence is *one* box per row, so "the rule this edge cuts"
    // has an answer, while a diagram row carries several node boxes and the routing
    // between them. `leading_rule` scans the row for the first glyph in the style and
    // would happily close the edge with a corner belonging to a different node — a `╮`
    // invented in the middle of a chart, which reads far worse than a chevron. A chevron
    // on box art says only "there is more to the right", which is exactly true. Checked
    // in tmux at 80 columns on a chart widened to 190.
    let frame_styles = [app.theme().code.frame, app.theme().table.border];

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
    // around it does not — and one pinned prefix per row, so a code block's line-number
    // gutter stays put while its long lines scroll under it. Computed once and shared:
    // the document, its edge markers and its search highlights disagreeing about where a
    // row starts would paint chevrons and highlights on the wrong columns.
    let hscroll = Offsets::new(app, doc_area.width);
    blit(buffer, doc_area, app.rendered(), scroll, &hscroll, base);
    edge_markers(
        buffer,
        doc_area,
        app.rendered(),
        scroll,
        &hscroll,
        marker_style,
        &frame_styles,
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
///
/// A row may also keep a *prefix* out of the offset altogether: a code block's
/// line-number gutter stays welded to the left edge while its long lines scroll
/// underneath it. See [`super::wide::pinned_prefix`]. Everything a row start is needed
/// for goes through [`Offsets::column`] or its inverse [`Offsets::x_of`], so no painter
/// can hold a different opinion about which canvas column a viewport column shows.
pub(super) struct Offsets<'a> {
    /// How far each row may be scrolled; one entry per canvas row.
    reach: &'a [u16],
    /// How many leading columns of each row the offset leaves alone.
    pinned: &'a [u16],
    /// The offset the reader has scrolled to.
    offset: u16,
    /// The number of columns of document on screen.
    viewport: u16,
}

impl<'a> Offsets<'a> {
    /// Reads the offsets for the current frame.
    fn new(app: &'a App, viewport: u16) -> Self {
        Self::scrolled_to(app.reach(), app.pinned(), app.hscroll(), viewport)
    }

    /// The offsets for a reader who has scrolled to `offset` over a canvas whose rows
    /// may travel as far as `reach` and hold `pinned` columns still.
    ///
    /// Separate from [`Offsets::new`] so a test can paint a frame without an [`App`];
    /// both go through the same arithmetic, so a test cannot drift from what the pager
    /// actually does.
    pub(super) fn scrolled_to(
        reach: &'a [u16],
        pinned: &'a [u16],
        offset: u16,
        viewport: u16,
    ) -> Self {
        Self {
            reach,
            pinned,
            offset,
            viewport,
        }
    }

    /// How far into the row the reader has scrolled, past whatever is pinned.
    fn at(&self, row: usize) -> u16 {
        let reach = self.reach.get(row).copied().unwrap_or(0);
        self.offset.min(reach.saturating_sub(self.viewport))
    }

    /// How many leading columns of `row` the offset leaves where they are.
    ///
    /// Clamped away entirely when the viewport is no wider than the prefix: a gutter
    /// filling the whole pane would put the code behind it out of reach, which is worse
    /// than losing the numbers on a terminal that narrow.
    fn pinned(&self, row: usize) -> u16 {
        let pinned = self.pinned.get(row).copied().unwrap_or(0);
        if pinned >= self.viewport { 0 } else { pinned }
    }

    /// The canvas column drawn at viewport column `x` of `row`.
    pub(super) fn column(&self, row: usize, x: u16) -> usize {
        if x < self.pinned(row) {
            usize::from(x)
        } else {
            usize::from(self.at(row)) + usize::from(x)
        }
    }

    /// The viewport column canvas column `col` is drawn at, when it is drawn at all.
    ///
    /// `None` for the columns that have scrolled *behind* the pinned prefix, which are
    /// on the canvas but on screen nowhere.
    fn x_of(&self, row: usize, col: u16) -> Option<u16> {
        let pinned = self.pinned(row);
        if col < pinned {
            return Some(col);
        }
        let x = col.checked_sub(self.at(row))?;
        (x >= pinned).then_some(x)
    }
}

/// Copies a vertical slice of the document canvas into the frame buffer.
///
/// This is the only place canvas cells become terminal cells. Double-width characters
/// keep their trailing continuation cell, and a wide character sliced in half by the
/// horizontal offset is drawn as a space rather than a broken glyph.
///
/// A row's pinned prefix is drawn from column zero and the offset applies only after it,
/// so a line-number gutter stays on screen while the code beside it scrolls.
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
        let pinned = left.pinned(row);
        for x in 0..area.width {
            let column = left.column(row, x);
            let Some(target) = buffer.cell_mut((area.x + x, area.y + y)) else {
                continue;
            };
            let Some(cell) = cells.get(column) else {
                break;
            };
            target.set_style(term_style(base.patch(cell.style())));
            if cell.is_continuation() {
                // Either the lead cell is on screen — in which case ratatui expects an
                // empty symbol here — or it was scrolled off to the left. The pinned
                // prefix is a second such seam: the first scrolled column can land on a
                // continuation whose lead is behind the gutter.
                target.set_symbol(if x == 0 || x == pinned { " " } else { "" });
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
///
/// A row cut on a box's *rule* is closed with that box's own corner or tee instead. A
/// chevron sitting where a `╮` belongs turns a table or a code fence into a frame that
/// never closes, which reads as a rendering fault rather than as scrollable content
/// (`docs/qa/visual-review-3.md` §11). The content rows between the rules still carry
/// the chevron, because they are what is actually cut off.
///
/// On a row with a pinned prefix the left marker moves right by that prefix, so it marks
/// the left edge of the *scrolling region* rather than of the window.
pub(super) fn edge_markers(
    buffer: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    top: usize,
    left: &Offsets<'_>,
    style: TermStyle,
    frames: &[Style],
) {
    if area.width == 0 {
        return;
    }
    for y in 0..area.height {
        let row = top + usize::from(y);
        let Some(cells) = canvas.row(row) else {
            break;
        };
        let offset = left.at(row);
        let pinned = left.pinned(row);
        let occupied = |range: std::ops::Range<usize>| {
            cells
                .get(range)
                .is_some_and(|slice| slice.iter().any(|cell| !cell.text().trim().is_empty()))
        };
        let mark = |buffer: &mut Buffer, x: u16, col: usize, side: Side| {
            let Some(cell) = buffer.cell_mut((x, area.y + y)) else {
                return;
            };
            match frame_close(cells, col, side, frames) {
                Some((glyph, frame)) => {
                    cell.set_symbol(&glyph.to_string());
                    cell.set_style(term_style(frame));
                }
                None => {
                    cell.set_symbol(match side {
                        Side::Left => LEFT_MARKER,
                        Side::Right => RIGHT_MARKER,
                    });
                    cell.set_style(style);
                }
            }
        };
        // The left marker goes at the first column that actually moves, not at the
        // viewport's own edge. With a pinned gutter the content scrolls off *behind* the
        // numbers, so a chevron in column zero would sit on the frame border and claim
        // the gutter was cut — while the place the reader can see a break is exactly
        // where the code resumes. Unpinned rows are unaffected: the prefix is zero and
        // this is column zero, as it always was.
        //
        // And on a pinned row the box's own left edge is *already on screen*, inside the
        // prefix: a cut through one of its rules therefore needs neither a chevron nor a
        // second corner stamped into the middle of the rule. What is hidden there is rule
        // and the rule is drawn either side of the seam, so the honest thing is to draw
        // nothing. The content rows between the rules are unaffected — code is not a
        // frame glyph — and keep their chevron.
        let hidden = usize::from(pinned)..usize::from(pinned) + usize::from(offset);
        let closed = pinned > 0 && frame_close(cells, hidden.end, Side::Left, frames).is_some();
        if offset > 0 && !closed && occupied(hidden.clone()) {
            mark(buffer, area.x + pinned, hidden.end, Side::Left);
        }
        let right = usize::from(offset) + usize::from(area.width);
        if occupied(right..cells.len()) {
            // A double-width glyph whose lead lands in the second-to-last column owns the
            // last one, and is painted straight over anything stamped there — so on every
            // second terminal width a CJK or emoji document was given no cut indication
            // at all. `blit` already blanks a lead it cannot fit; do the same here rather
            // than move the marker, which would leave the rail zigzagging by one column
            // from row to row depending on what each row happens to end in.
            if area.width >= 2
                && cells
                    .get(right - 1)
                    .is_some_and(|cell| cell.is_continuation())
                && let Some(lead) = buffer.cell_mut((area.x + area.width - 2, area.y + y))
            {
                lead.set_symbol(" ");
            }
            mark(buffer, area.x + area.width - 1, right - 1, Side::Right);
        }
    }
}

/// The glyph that closes a frame cut at `col`, when the cut lands on one.
///
/// Two conditions, and both are needed. The cut cell has to be *drawn in a frame style*,
/// which is what keeps a code block whose content happens to be box art — this project's
/// own documentation is full of it — from having its text quietly rewritten into a
/// corner. And the glyph has to belong to a horizontal rule: a `│` is a cut through
/// content, not through a rule, and takes the chevron.
///
/// A bare `─` cannot say which edge it is, so the row is scanned for the first glyph in
/// the same style that can: a table's or a fence's top rule always starts `╭`, whatever
/// margin or quote bar precedes it.
fn frame_close(cells: &[Cell], col: usize, side: Side, frames: &[Style]) -> Option<(char, Style)> {
    let cut = cells.get(col)?;
    if !frames.contains(&cut.style()) {
        return None;
    }
    let (set, named) = BorderSet::rule_glyph(cut.text().chars().next()?)?;
    let rule = named.or_else(|| leading_rule(cells, cut.style()))?;
    Some((set.close(rule, side), cut.style()))
}

/// Which edge of a box a row is, taken from the first glyph in it that says.
fn leading_rule(cells: &[Cell], style: Style) -> Option<Rule> {
    cells.iter().find_map(|cell| {
        if cell.style() != style {
            return None;
        }
        BorderSet::rule_glyph(cell.text().chars().next()?)?.1
    })
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
        for (index, segment) in app.search().segments_on_row(row) {
            let style = if Some(index) == current {
                theme.ui.search_current
            } else {
                theme.ui.search_match
            };
            // Mapped column by column through the same `Offsets` `blit` painted with: a
            // match that straddles a pinned prefix has part of itself on screen and part
            // of itself behind the gutter, and only the arithmetic that drew the row can
            // say which is which.
            for offset in 0..segment.cols {
                let Some(x) = left.x_of(row, segment.col.saturating_add(offset)) else {
                    continue;
                };
                if x >= area.width {
                    continue;
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
