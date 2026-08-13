//! Painting the application onto a `ratatui` frame.
//!
//! Nothing here decides anything: every value drawn comes from [`super::app::App`].
//! That is the separation design spec §13 asks for — the state machine is testable
//! without a terminal, and this module is testable by eye.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color as TermColor, Modifier, Style as TermStyle};
use ratatui::text::Span as TermSpan;
use ratatui::widgets::{Block, BorderType, Clear, Widget};

use crate::canvas::{BorderSet, Canvas, Cell, Rule, Side};
use crate::theme::{Attributes, Color, Style};

use super::app::{App, Overlay};
use super::chrome;
use super::select;

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
    let marks = Marks {
        style: marker_style,
        frames: &frame_styles,
        stripe: app.theme().table.row_alt.bg,
    };

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
    edge_markers(buffer, doc_area, app.rendered(), scroll, &hscroll, &marks);
    // Before the flash, and that ordering is the precedence: the pointer is necessarily
    // over the button at the moment it is clicked, so a hover painted afterwards would
    // repaint the one piece of feedback the press produces. `[copied]` wins.
    //
    // A control can be several hotspots — a link wrapped across rows, or wrapped inside
    // a centred table cell (design spec §2.2) — sharing one `target`. Resolving the
    // hovered index to its target and painting every hotspot that shares it is what
    // keeps such a control looking like one thing under the pointer, rather than
    // visibly breaking at the row boundary that lit it.
    let hovered_spots: Vec<(usize, u16, u16)> = app
        .hovered()
        .and_then(|index| app.rendered().hotspots().get(index))
        .map(|hovered| {
            app.rendered()
                .hotspots()
                .iter()
                .filter(|spot| spot.target == hovered.target)
                .map(|spot| (spot.row, spot.col, spot.cols))
                .collect()
        })
        .unwrap_or_default();
    hover_highlight(
        buffer,
        doc_area,
        app.rendered(),
        scroll,
        &hscroll,
        base,
        app.theme(),
        &hovered_spots,
    );
    // The keyboard cursor, resolved to every hotspot of its control exactly as the
    // pointer's hover is above — a wrapped link is several hotspots sharing one
    // target, and `f`/`F` land on the control, not on one row of it (design spec §4).
    let cursor_spots: Vec<(usize, u16, u16)> = app
        .cursor_target()
        .map(|target| {
            app.rendered()
                .hotspots()
                .iter()
                .filter(|spot| spot.target == target)
                .map(|spot| (spot.row, spot.col, spot.cols))
                .collect()
        })
        .unwrap_or_default();
    cursor_highlight(
        buffer,
        doc_area,
        app.rendered(),
        scroll,
        &hscroll,
        base,
        app.theme(),
        &cursor_spots,
    );
    // Under the search wash and the selection, which say where the reader's attention
    // is; the flash only says what their last click did.
    copied_flash(
        buffer,
        doc_area,
        app.rendered(),
        scroll,
        &hscroll,
        base,
        app.copied_flash(),
    );
    highlight_matches(buffer, doc_area, app, scroll, &hscroll);
    // Over the search wash, not under it: the selection is the thing the reader's hand
    // is on right now, and a match underneath it is still legible as an orange cell one
    // column past the drag.
    highlight_selection(buffer, doc_area, app, scroll, &hscroll);
    scrollbar(buffer, bar_area, app);
    if app.rendered().is_empty() {
        empty_notice(buffer, doc_area, dim_style);
    }
    // The footnote popup goes over the document and under everything else. Over,
    // because it is a box the reader asked for and the page is its background; under
    // the chrome, because the status bar is not a place a box may cover and the help
    // overlay is modal — it dims the whole document area, popup included.
    footnote_popup(buffer, doc_area, app);

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
/// [`crate::render::document::scroll_reach`] for what "anywhere" means and why it is a property of
/// a run of rows rather than of one row.
///
/// A row may also keep a *prefix* out of the offset altogether: a code block's
/// line-number gutter stays welded to the left edge while its long lines scroll
/// underneath it. See [`crate::render::document::pinned_prefix`]. Everything a row start is needed
/// for goes through [`Offsets::column`] or its inverse [`Offsets::x_of`], so no painter
/// can hold a different opinion about which canvas column a viewport column shows.
///
/// The document's own margin — [`crate::render::DOCUMENT_MARGIN`], applied by
/// [`crate::render::render_document`] exactly as the renderer applies it — is held out of
/// the scroll on *every* row, for the same reason a gutter is. It is one blank column on
/// each side, and it is where the edge markers are painted: a marker drawn inside the
/// document's own columns can only be drawn over something, and a pager that silently
/// swaps a character for a chevron is not showing you the document. Pinning it is what
/// makes that column blank at every offset rather than only at rest, because the offset
/// would otherwise drag the margin off the screen along with everything else.
pub(super) struct Offsets<'a> {
    /// How far each row may be scrolled; one entry per canvas row.
    reach: &'a [u16],
    /// How many leading columns of each row the offset leaves alone.
    pinned: &'a [u16],
    /// The offset the reader has scrolled to.
    offset: u16,
    /// The number of columns of document on screen, marker rails included.
    viewport: u16,
    /// The width of the marker rail on each side: the document's own margin, or zero on
    /// a viewport too narrow for the renderer to have inset one.
    margin: u16,
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
            // Read from the viewport rather than passed in, so a test cannot hand the
            // painters a rail the pager would not have given them. `render::margins` is
            // the same function `render_document` insets the canvas with.
            margin: crate::render::margins(viewport),
        }
    }

    /// The width of the marker rail on each side of the document.
    pub(super) fn margin(&self) -> u16 {
        self.margin
    }

    /// The number of viewport columns the document itself is drawn in.
    ///
    /// The rail is taken off the right; the left one is the first column of the pinned
    /// prefix, which every row has, so it costs nothing here.
    pub(super) fn content(&self) -> u16 {
        self.viewport.saturating_sub(self.margin)
    }

    /// How far into the row the reader has scrolled, past whatever is pinned.
    ///
    /// Measured against [`Offsets::content`] rather than the whole viewport: the row is
    /// fully scrolled when its last drawn column reaches the last column the document is
    /// drawn in, not when it reaches the rail beyond that.
    pub(super) fn at(&self, row: usize) -> u16 {
        let reach = self.reach.get(row).copied().unwrap_or(0);
        self.offset.min(reach.saturating_sub(self.content()))
    }

    /// How many leading columns of `row` are a *gutter* — pinned chrome the row draws
    /// itself, as opposed to the margin rail every row has.
    ///
    /// Clamped away entirely when the viewport is no wider than the prefix: a gutter
    /// filling the whole pane would put the code behind it out of reach, which is worse
    /// than losing the numbers on a terminal that narrow.
    fn gutter(&self, row: usize) -> u16 {
        let pinned = self.pinned.get(row).copied().unwrap_or(0);
        if pinned >= self.content() { 0 } else { pinned }
    }

    /// How many leading columns of `row` the offset leaves where they are.
    pub(super) fn pinned(&self, row: usize) -> u16 {
        self.gutter(row).max(self.margin.min(self.content()))
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
    pub(super) fn x_of(&self, row: usize, col: u16) -> Option<u16> {
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
///
/// The document stops one column short of the right edge, and starts one column in: those
/// two columns are [`edge_markers`]'s rail. At rest they are the document's own margin and
/// nothing changes; scrolled, they are the difference between a marker that sits beside
/// the text and one that sits on top of a character of it.
fn blit(
    buffer: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    top: usize,
    left: &Offsets<'_>,
    base: Style,
) {
    let content = left.content();
    for y in 0..area.height {
        let row = top + usize::from(y);
        let Some(cells) = canvas.row(row) else { break };
        let pinned = left.pinned(row);
        // The right-hand rail carries no document, at any offset. Whatever the canvas
        // holds out there is one arrow press away, and drawing it under a marker is how
        // a column of the document came to be lost in the first place.
        for x in content..area.width {
            if let Some(target) = buffer.cell_mut((area.x + x, area.y + y)) {
                target.set_style(term_style(base));
                target.set_symbol(" ");
            }
        }
        for x in 0..content {
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
                // prefix is a second such seam, but only while something is scrolled
                // behind it: at offset zero the prefix is contiguous with the region
                // after it, the lead at `pinned - 1` really is on screen, and blanking
                // the half after it would hand ratatui's diff a space it drops (it skips
                // the cells a wide symbol owns), leaving the terminal on a stale glyph.
                let orphan = x == 0 || (x == pinned && left.at(row) > 0);
                target.set_symbol(if orphan { " " } else { "" });
            } else if cell.width() == 2 && x + 1 >= content {
                target.set_symbol(" ");
            } else {
                target.set_symbol(cell.text());
            }
        }
    }
}

/// Paints `[copied]` over the button at canvas `(row, col)`, when there is one.
///
/// Nine columns were reserved at render time (`render::button::REGION`) precisely so
/// this fits without a re-render: rendering is a pure function of `(AST, width, theme,
/// options)`, and "this block was copied 300 ms ago" is pager state that must not enter
/// it. `col` is the label's first column and the reserved region starts two to the left
/// of it, which is where the longer word begins.
///
/// Every column goes through the same [`Offsets`] as [`blit`], so the flash lands on the
/// cells the label was drawn in whatever the row is scrolled to, and columns that are
/// off screen — behind the pinned prefix, past the rail, or on a row that is not on
/// screen at all — are simply not painted.
fn copied_flash(
    buffer: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    top: usize,
    left: &Offsets<'_>,
    base: Style,
    at: Option<(usize, u16)>,
) {
    let Some((row, col)) = at else { return };
    let Some(y) = row.checked_sub(top).and_then(|y| u16::try_from(y).ok()) else {
        return;
    };
    let Some(cells) = canvas.row(row).filter(|_| y < area.height) else {
        return;
    };
    // The label's own style, so the flash arrives in the frame's colour rather than in
    // whatever a bare write would leave it.
    let style = term_style(
        cells
            .get(usize::from(col))
            .map_or(base, |cell| base.patch(cell.style())),
    );
    let start = col.saturating_sub(2);
    for (index, ch) in crate::render::button::FLASH.bytes().enumerate() {
        let Some(column) = u16::try_from(index).ok().map(|index| start + index) else {
            break;
        };
        let Some(x) = left.x_of(row, column).filter(|x| *x < left.content()) else {
            continue;
        };
        if let Some(target) = buffer.cell_mut((area.x + x, area.y + y)) {
            target.set_style(style);
            target.set_symbol(char::from(ch).encode_utf8(&mut [0u8; 4]));
        }
    }
}

/// Repaints the control under the pointer in the theme's hovered shade.
///
/// Paint-time, exactly like [`copied_flash`] and for the same reason: the pointer is not
/// one of the inputs a render is a function of, and threading it into one would make the
/// cache key a mouse position. What arrives here is the region the renderer already drew
/// — row, first column, width — so nothing is measured a second time and no cell can
/// move: each column keeps its symbol and takes a new style.
///
/// The shade is derived from the cell's *own* style rather than from a slot, so the
/// button at a table's edge hovers in the table's border colour and the one on a code
/// fence in the fence's frame colour, without this function knowing there are two.
/// Columns that are off screen — scrolled behind a pinned prefix, past the rail, or on a
/// row above or below the viewport — are simply not painted, which [`Offsets`] answers.
///
/// `at` is every region of the hovered control, not one: a wrapped link is several
/// hotspots sharing a `target`, and the caller has already resolved that grouping — this
/// function just paints each region it is handed. An empty slice paints nothing, which is
/// what an unhovered frame gets.
#[allow(clippy::too_many_arguments)]
fn hover_highlight(
    buffer: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    top: usize,
    left: &Offsets<'_>,
    base: Style,
    theme: &crate::theme::Theme,
    at: &[(usize, u16, u16)],
) {
    for &(row, col, cols) in at {
        let Some(y) = row.checked_sub(top).and_then(|y| u16::try_from(y).ok()) else {
            continue;
        };
        let Some(cells) = canvas.row(row).filter(|_| y < area.height) else {
            continue;
        };
        for column in col..col.saturating_add(cols) {
            let Some(x) = left.x_of(row, column).filter(|x| *x < left.content()) else {
                continue;
            };
            let under = cells
                .get(usize::from(column))
                .map_or(base, |cell| base.patch(cell.style()));
            if let Some(target) = buffer.cell_mut((area.x + x, area.y + y)) {
                target.set_style(term_style(theme.hovered(under)));
            }
        }
    }
}

/// Paints the keyboard cursor onto every hotspot of the control it is on.
///
/// The same shape as [`hover_highlight`] — a control the cursor sits on gets the same
/// tint a hovered one does, plus reverse video: a reader without a mouse needs a mark
/// they can find at a glance, and one indistinguishable from a pointer that is not
/// there would not be a cursor at all. `at` is every region of the control, already
/// resolved by the caller from its `target`, exactly as hover's grouping is.
#[allow(clippy::too_many_arguments)]
fn cursor_highlight(
    buffer: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    top: usize,
    left: &Offsets<'_>,
    base: Style,
    theme: &crate::theme::Theme,
    at: &[(usize, u16, u16)],
) {
    for &(row, col, cols) in at {
        let Some(y) = row.checked_sub(top).and_then(|y| u16::try_from(y).ok()) else {
            continue;
        };
        let Some(cells) = canvas.row(row).filter(|_| y < area.height) else {
            continue;
        };
        for column in col..col.saturating_add(cols) {
            let Some(x) = left.x_of(row, column).filter(|x| *x < left.content()) else {
                continue;
            };
            let under = cells
                .get(usize::from(column))
                .map_or(base, |cell| base.patch(cell.style()));
            if let Some(target) = buffer.cell_mut((area.x + x, area.y + y)) {
                target.set_style(term_style(theme.hovered(under).with(Attributes::REVERSE)));
            }
        }
    }
}

/// What [`edge_markers`] needs to know about the theme, gathered once per frame.
///
/// Together rather than one argument each: they are three answers to the same question —
/// what a row that runs past the viewport shows at the edge — and the chevron's colour
/// alone never told the whole story.
pub(super) struct Marks<'a> {
    /// The style a chevron is painted in.
    pub style: TermStyle,
    /// The styles a box's own glyphs are painted in, so a cut rule can close itself.
    pub frames: &'a [Style],
    /// The colour a table's row gap carries the zebra stripe through in, so a cut gap
    /// can be passed over in silence. See [`crate::render::table::is_row_gap`].
    pub stripe: Option<Color>,
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
/// (`docs/qa/visual-review-3.md` §11).
///
/// The same argument runs one step further, and this is what makes the marking *per row*
/// rather than per block. A widened code fence or table carries its own wall down the far
/// side of the canvas, so on every row of it there is something past the viewport's edge —
/// and marking that fact gave a blank line and a closing `}` a `›` apiece, between a `╮`
/// and a `╯` that both said the box ended there. Three glyphs, two of them lying. What is
/// hidden on such a row is *the box*, not the row's content, so the box is what the edge
/// draws: its own wall, exactly where the renderer would have drawn it, which is what
/// `--render-once` shows. A chevron is kept for the rows where content really is cut —
/// [`hidden_wall`] is the test, and it is the row's own cells that answer it, not the run
/// the row scrolls with. Scrolling stays per run ([`crate::render::document::scroll_reach`]); only the
/// marking is per row. They were never the same question.
///
/// A table's row gap is marked on neither edge. It is shading between two rows and
/// carries nothing to be cut off, so a chevron there would claim content continues where
/// none does — and, the gaps being between rows that all earn one, it would weld the
/// markers into an unbroken rail that says the same thing about every row of the table
/// whether or not it has anything more to show. The same argument as for the rules, one
/// step further: `render::table::is_row_gap` recognises the gap off the drawn canvas,
/// exactly as [`frame_close`] recognises a rule.
///
/// On a row with a pinned prefix the left marker moves right by that prefix, so it marks
/// the left edge of the *scrolling region* rather than of the window. Both markers sit in
/// the rail [`blit`] leaves them — the document's own margin, and on a pinned row the
/// blank column the gutter already keeps between its rule and the code. Neither is a
/// column the document draws in, which is the whole point: the marker that told you
/// content was cut used to be standing on the first character of it.
pub(super) fn edge_markers(
    buffer: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    top: usize,
    left: &Offsets<'_>,
    marks: &Marks<'_>,
) {
    if area.width == 0 {
        return;
    }
    let (style, frames) = (marks.style, marks.frames);
    for y in 0..area.height {
        let row = top + usize::from(y);
        let Some(cells) = canvas.row(row) else {
            break;
        };
        if crate::render::table::is_row_gap(cells, marks.stripe) {
            continue;
        }
        let offset = left.at(row);
        let pinned = left.pinned(row);
        let content = left.content();
        let occupied = |range: std::ops::Range<usize>| {
            cells
                .get(range)
                .is_some_and(|slice| slice.iter().any(|cell| !cell.text().trim().is_empty()))
        };
        let mark =
            |buffer: &mut Buffer, x: u16, col: usize, hidden: std::ops::Range<usize>, side| {
                let Some(cell) = buffer.cell_mut((x, area.y + y)) else {
                    return;
                };
                let glyph = frame_close(cells, col, side, frames)
                    .or_else(|| hidden_wall(cells, hidden, side, frames));
                match glyph {
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
        //
        // The rail is the column immediately left of the scrolling region, which is blank
        // on every row that has one: the document's margin on a plain row, and the gutter's
        // own separator — the blank column `render::code` deliberately keeps inside the pin
        // it publishes, between the rule and the code — on a pinned one. Only a viewport
        // too narrow for the renderer
        // to have inset a margin at all has no rail, and there the old stamp-over-content
        // behaviour is what is left; at two columns there is nothing to protect.
        let hidden = usize::from(pinned)..usize::from(pinned) + usize::from(offset);
        let closed =
            left.gutter(row) > 0 && frame_close(cells, hidden.end, Side::Left, frames).is_some();
        if offset > 0 && !closed && occupied(hidden.clone()) {
            let rail = area.x + pinned - u16::from(pinned > 0 && left.margin() > 0);
            mark(buffer, rail, hidden.end, hidden.clone(), Side::Left);
        }
        let right = usize::from(offset) + usize::from(content);
        if occupied(right..cells.len()) {
            // A double-width glyph whose lead lands in the second-to-last column owns the
            // last one, and is painted straight over anything stamped there — so on every
            // second terminal width a CJK or emoji document was given no cut indication
            // at all. With a rail this cannot arise: the marker's column is not one the
            // document is drawn in, and `blit` blanks a lead whose other half would fall
            // outside. Without one — a viewport too narrow to have a margin — the old
            // defence is still needed, and blanking the lead is still better than moving
            // the marker, which would leave the rail zigzagging by one column from row to
            // row depending on what each row happens to end in.
            if left.margin() == 0
                && area.width >= 2
                && cells
                    .get(right - 1)
                    .is_some_and(|cell| cell.is_continuation())
                && let Some(lead) = buffer.cell_mut((area.x + area.width - 2, area.y + y))
            {
                lead.set_symbol(" ");
            }
            let rail = area.x + content - u16::from(left.margin() == 0);
            mark(buffer, rail, right - 1, right..cells.len(), Side::Right);
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

/// The box glyph to draw at the edge when the *only* thing the edge hides is that box.
///
/// This is the per-row half of the marking. A widened fence or table draws its wall down
/// the far side of the canvas, so `hidden` is never empty on any row of it — and answering
/// "is there something past the edge?" with a chevron marked every row of the block,
/// including the blank ones, as cut. The question the reader is actually asking is whether
/// any of *their document* is out there. So: nothing hidden but frame glyphs means the row
/// is whole and only its box was cut, and the honest thing to draw is the box's own wall,
/// which is what the renderer draws at the same place when it clips the block itself. One
/// non-frame glyph out there and it is a chevron again, because then content really is
/// missing.
///
/// The glyph is taken from the hidden cell nearest the edge — the wall, not whatever it
/// encloses — and is returned with its own style, so it is painted as frame rather than as
/// a marker. [`frame_close`] gets first refusal for the rows where the cut lands on a rule
/// and a corner or a tee is the better glyph; this is the case that is left, where the cut
/// lands on the blank interior of a box whose wall is off screen.
fn hidden_wall(
    cells: &[Cell],
    hidden: std::ops::Range<usize>,
    side: Side,
    frames: &[Style],
) -> Option<(char, Style)> {
    let drawn = cells
        .get(hidden)?
        .iter()
        .filter(|cell| !cell.is_blank() && !cell.is_continuation());
    let mut wall = None;
    for cell in drawn {
        if !frames.contains(&cell.style()) {
            return None;
        }
        if wall.is_none() || side == Side::Left {
            wall = Some((cell.text().chars().next()?, cell.style()));
        }
    }
    wall
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

/// Paints the footnote popup over the document.
///
/// Paint-time, like [`hover_highlight`] and [`copied_flash`] before it: the note was
/// laid out once by the ordinary renderer when the popup opened, and this copies cells.
/// Nothing here measures, wraps or styles a single character of the footnote — that is
/// what "a popup is another width, not a second rendering path" means in practice, and a
/// special case for a list or a code span appearing in this function would be the sign
/// that the shared path had been left.
///
/// The body is painted in the *document's* own background rather than in the help
/// overlay's panel wash. Two reasons: the note's cells were rendered against that
/// background, so they need no patching and a code fence inside the note keeps its own
/// frame colour; and a framed inset over the page is the visual language the document
/// already speaks — a code frame, a table and an image placeholder are all exactly this.
/// Only the border and its title borrow the help overlay's slots, so the pager's boxes
/// agree with each other without a new theme slot that every theme would have to grow.
fn footnote_popup(buffer: &mut Buffer, area: Rect, app: &App) {
    let Some(popup) = app.popup() else { return };
    let theme = app.theme();
    let at = popup.area();
    let rect = Rect::new(
        area.x.saturating_add(at.left),
        area.y.saturating_add(at.top),
        at.width.min(area.width.saturating_sub(at.left)),
        at.height.min(area.height.saturating_sub(at.top)),
    );
    if rect.width < 3 || rect.height < 3 {
        return;
    }
    let base = theme.base();
    let on_page = |style: Style| Style {
        bg: base.bg,
        ..style
    };
    Clear.render(rect, buffer);
    buffer.set_style(rect, term_style(base));
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(term_style(on_page(theme.ui.help_border)))
        .title(TermSpan::styled(
            format!(" [{}] ", popup.label()),
            term_style(on_page(theme.ui.help_title)),
        ));
    // A note taller than its box says how much more there is and how to reach it, the
    // way the help overlay does. A box that silently held back half a footnote would be
    // the pager lying by omission about the document.
    let hidden = popup.hidden_rows();
    if popup.max_scroll() > 0 {
        let note = if hidden > 0 {
            format!(" \u{2193} {hidden} more ")
        } else {
            " \u{2191} back ".to_string()
        };
        block = block.title_bottom(TermSpan::styled(
            note,
            term_style(on_page(theme.ui.help_title)),
        ));
    }
    block.render(rect, buffer);

    // The note itself: canvas cells straight into buffer cells, from the row the reader
    // has scrolled to. The inner region is the border plus one column of padding on each
    // side — nothing in this program is welded to its own border.
    let inner = Rect::new(
        rect.x.saturating_add(crate::tui::popup::CHROME_COLS / 2),
        rect.y.saturating_add(1),
        rect.width.saturating_sub(crate::tui::popup::CHROME_COLS),
        rect.height.saturating_sub(crate::tui::popup::CHROME_ROWS),
    );
    let canvas = popup.canvas();
    for y in 0..inner.height {
        let Some(cells) = canvas.row(popup.scroll() + usize::from(y)) else {
            break;
        };
        for x in 0..inner.width {
            let Some(cell) = cells.get(usize::from(x)) else {
                break;
            };
            let Some(target) = buffer.cell_mut((inner.x + x, inner.y + y)) else {
                continue;
            };
            target.set_style(term_style(base.patch(cell.style())));
            if cell.is_continuation() {
                // The lead is always on screen here — the note starts at its own column
                // zero — so ratatui wants the owned cell left empty.
                target.set_symbol("");
            } else if cell.width() == 2 && x + 1 >= inner.width {
                // A wide glyph whose other half falls outside the box, drawn as a space
                // rather than as half a character. Same rule as `blit`'s.
                target.set_symbol(" ");
            } else {
                target.set_symbol(cell.text());
            }
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
                // The rails are not the document's columns; a highlight painted there
                // would put a search background on a chevron.
                if x >= left.content() {
                    continue;
                }
                if let Some(cell) = buffer.cell_mut((area.x + x, area.y + y)) {
                    cell.set_style(patch_term(cell.style(), style));
                }
            }
        }
    }
}

/// Repaints the mouse selection on top of the document.
///
/// Painted from canvas columns through the same [`Offsets`] as everything else, so a
/// selection made before the reader scrolled sideways stays on the text it was made on
/// rather than on the columns it happened to occupy at the time.
fn highlight_selection(buffer: &mut Buffer, area: Rect, app: &App, top: usize, left: &Offsets<'_>) {
    let Some(selection) = app.selection() else {
        return;
    };
    let style = app.theme().ui.selection;
    let canvas = app.rendered();
    for y in 0..area.height {
        let row = top + usize::from(y);
        if canvas.row(row).is_none() {
            break;
        }
        let ranges = select::highlighted_columns(canvas, app.doc().source(), selection, row);
        for columns in ranges {
            for col in columns {
                let Some(x) = left.x_of(row, col) else {
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
/// Where it sits is [`App::scrollbar_thumb`]'s answer, not this function's, because the
/// mouse has to invert exactly this mapping to make the bar draggable.
fn scrollbar(buffer: &mut Buffer, area: Rect, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = app.theme();
    let track = term_style(theme.ui.scrollbar_track);
    let thumb = term_style(theme.ui.scrollbar_thumb);

    // Geometry lives on `App` because the mouse needs to invert it; see
    // [`App::scrollbar_thumb`].
    let (start, length) = app.scrollbar_thumb(area.height);

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

/// Converts an `mdmost` style into a `ratatui` style.
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

/// Lays an `mdmost` style over an existing `ratatui` style, keeping what it leaves unset.
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
