//! The pager's state machine.
//!
//! Everything here is pure state plus pure transitions: no `ratatui`, no `crossterm`,
//! no terminal. That separation is what makes the pager testable — every test in this
//! module drives the real application logic, and [`super::draw`] is left with nothing
//! but painting.

use crate::canvas::{Canvas, HotspotKind};
use crate::config::{Action, Config, Key, KeyCode};
use crate::doc::Doc;
use crate::render::RenderOptions;
use crate::search::{Search, SearchMode};
use crate::theme::Theme;
use crate::toc::{FilterHit, Toc};

use super::cache::RenderCache;
use super::draw::Offsets;
use super::popup::{self, Popup};
use super::select::{Extract, Pos, Selection};

/// The narrowest terminal the table-of-contents pane is offered in.
const MIN_TOC_TERMINAL_WIDTH: u16 = 40;

/// How many columns one press of the horizontal scroll keys moves.
const HSCROLL_STEP: u16 = 8;

/// The most digits a repeat count may have, so a leaned-on key cannot allocate.
const MAX_COUNT_DIGITS: usize = 6;

/// The most times one repeat count re-runs an action.
///
/// Movement is expressed as a single clamped jump, so this only bounds the actions
/// that genuinely have to be stepped — matches and headings.
const MAX_REPEAT: usize = 10_000;

/// How long the `[copied]` label stays up, in milliseconds.
///
/// The event loop redraws every poll interval whether or not anything happened, so the
/// flash clears itself without any new timer: the next tick after the deadline draws
/// the label back. Nothing here schedules a wake-up.
pub const FLASH_FOR: u64 = 600;

/// Whether `key` is a bare digit, and so part of a repeat count.
fn is_count_digit(key: Key) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch.is_ascii_digit()) && key.mods.is_empty()
}

/// How far `action` scrolls a footnote popup, or `None` if it is not a movement key.
///
/// The movement keys only. `Top` and `Bottom` are deliberately *not* here: a reader who
/// presses `G` with a popup up means the end of the document, and taking them would make
/// the popup modal in the one way it is not (see [`Overlay`]).
fn popup_scroll(action: Action, height: usize, times: usize) -> Option<isize> {
    let rows = |per: usize| isize::try_from(per.saturating_mul(times)).unwrap_or(isize::MAX);
    match action {
        Action::LineDown => Some(rows(1)),
        Action::LineUp => Some(-rows(1)),
        Action::HalfPageDown => Some(rows((height / 2).max(1))),
        Action::HalfPageUp => Some(-rows((height / 2).max(1))),
        Action::PageDown => Some(rows(height.saturating_sub(1).max(1))),
        Action::PageUp => Some(-rows(height.saturating_sub(1).max(1))),
        _ => None,
    }
}

/// Which pane the keyboard is talking to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The document viewport.
    Document,
    /// The table-of-contents pane.
    Toc,
}

/// What the user is being asked to type, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// A forward search.
    SearchForward,
    /// A backward search.
    SearchBackward,
    /// A fuzzy filter over the table of contents.
    TocFilter,
}

/// A control that a full click landed on, handed to the event loop to act upon.
///
/// The state machine touches no terminal and no display server (design spec §13), so it
/// cannot copy, open or jump: it says *which* control was activated and hands that over,
/// exactly as [`Extract`] does for a finished drag. Keeping the decision here is also
/// what lets a test click a button without taking ownership of anybody's clipboard.
///
/// **Changed 2026-08-12 (Task 5).** This replaces a `HotspotCopy` produced by the *press*.
/// A copy button is no longer a parallel mechanism with its own firing edge (design spec
/// §2): every kind of control now activates the same way, on a release that landed on the
/// control the press started on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    /// The canvas row the released hotspot was drawn on, for the flash.
    pub row: usize,
    /// The canvas column it starts at, for the flash.
    pub col: u16,
    /// What activating it does.
    pub kind: HotspotKind,
}

/// What handling one key produced, for [`super::term`] to act on.
///
/// [`App::on_key`] cannot fire a control itself — the state machine touches no
/// terminal or display server (design spec §13) — so it hands back what an `enter` on
/// the keyboard cursor produced, the same way [`App::release_hotspot`] hands back a
/// mouse click. Most keys produce an empty outcome; [`Default`] is that outcome.
#[derive(Debug, Default)]
pub struct KeyOutcome {
    activation: Option<Activation>,
}

impl KeyOutcome {
    /// Whether the key fired a control under the keyboard cursor.
    pub fn fired_activation(&self) -> bool {
        self.activation.is_some()
    }

    /// Takes the activation, for the caller that carries out `Open`/`Copy` I/O and
    /// dispatches the rest through [`App::activate`] — the one place that logic lives,
    /// whether the click came from a mouse or a keyboard.
    pub fn into_activation(self) -> Option<Activation> {
        self.activation
    }
}

impl PromptKind {
    /// The sigil shown in front of the input.
    ///
    /// A regular-expression search is spelled `re/` rather than `/`, so the mode in
    /// force is legible at the prompt itself and never has to be guessed. `mode` is
    /// ignored by the table-of-contents filter, which is always fuzzy.
    pub fn sigil(self, mode: SearchMode) -> &'static str {
        match (self, mode) {
            (PromptKind::SearchForward, SearchMode::Literal) => "/",
            (PromptKind::SearchForward, SearchMode::Regex) => "re/",
            (PromptKind::SearchBackward, SearchMode::Literal) => "?",
            (PromptKind::SearchBackward, SearchMode::Regex) => "re?",
            (PromptKind::TocFilter, _) => "toc /",
        }
    }
}

/// The overlay currently covering the document, if any.
///
/// # Why the footnote popup is not one of these
///
/// It lives beside this enum, in [`App::popup`], and the difference is not cosmetic.
///
/// * **These are modal; the popup is not.** [`App::on_key`] routes *every* key to the
///   prompt or the help overlay before the bindings are consulted. The popup must not
///   do that: `q` still quits, `/` still searches, and scrolling the document is how the
///   reader *dismisses* the popup — a key path that could not exist if the popup owned
///   the keyboard the way these two do.
/// * **These fill the screen; the popup is anchored to a cell.** It carries a rectangle,
///   a rendered canvas and a scroll offset, and it must stay on the document area it is
///   anchored inside. None of that has a meaning for `Help` or `Prompt`.
/// * **An enum is one-at-a-time.** A popup and a prompt can be up together — the reader
///   opens a footnote and then presses `/` — and the help overlay deliberately covers
///   the popup. Making them variants of one type would forbid combinations nobody asked
///   to forbid, and would put a [`Canvas`] inside a `Clone + PartialEq` enum that
///   [`App::on_key`] clones on every keystroke at a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    /// Nothing is covering the document.
    None,
    /// The help overlay is up.
    Help,
    /// The user is typing at a prompt.
    Prompt {
        /// What is being asked for.
        kind: PromptKind,
        /// What has been typed so far.
        input: String,
    },
}

/// A scrollbar drag in progress: where the press landed, and where the document was.
///
/// Both halves are needed and neither is rewritten while the drag runs. Tracking the
/// pointer's *absolute* position instead would make the thumb jump to a fixed grip the
/// moment it moved; rewriting the anchor on every event would accumulate the rounding
/// of every intermediate row, and a drag delivers one event per row crossed.
#[derive(Debug, Clone, Copy)]
struct BarGrab {
    /// The track row the button went down on.
    row: u16,
    /// The scroll offset once that press had been handled.
    scroll: usize,
}

/// A transient message shown in place of the status bar's usual contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// The text to show.
    pub text: String,
    /// Whether it reports a problem, which the theme colours differently.
    pub is_error: bool,
}

/// How the application was started.
#[derive(Debug, Clone)]
pub struct AppOptions {
    /// The name shown in the status bar.
    pub title: String,
    /// Whether Nerd Font glyphs may be drawn.
    ///
    /// Applies to the chrome *and* to the document: it is passed to the renderer as
    /// part of [`RenderOptions`], so `--no-icons` changes task boxes and code-fence
    /// icons too, not merely the status bar. List bullets are ASCII either way
    /// (see `render::glyphs`).
    pub icons: bool,
    /// The theme to start in.
    pub theme: String,
    /// Whether the table-of-contents pane starts open.
    pub toc_open: bool,
    /// The configuration file the settings are saved to, when the reader asks.
    ///
    /// `--config PATH` when one was named, otherwise the platform's default location,
    /// which is created on demand. `None` only on a platform with no home directory to
    /// speak of, where saving reports that instead of guessing at a path.
    pub config_path: Option<std::path::PathBuf>,
    /// A forced render width (`--width`), independent of the terminal size.
    ///
    /// It sets the width blocks are *laid out* at; a block that still does not fit is
    /// widened further by [`super::wide`]. Content beyond the viewport is reached with
    /// the horizontal scroll keys either way, so forcing a width never hides anything.
    pub width: Option<u16>,
}

/// The pager.
pub struct App {
    doc: Doc,
    config: Config,
    options: AppOptions,
    theme: Theme,
    cache: RenderCache,
    toc: Toc,
    search: Search,
    /// Viewport size in cells, status bar included.
    size: (u16, u16),
    scroll: usize,
    hscroll: u16,
    /// The scrollbar drag in progress, if the reader is holding the thumb.
    bar_grab: Option<BarGrab>,
    focus: Focus,
    toc_open: bool,
    toc_cursor: usize,
    toc_filter: String,
    toc_hits: Vec<FilterHit>,
    overlay: Overlay,
    /// The footnote popup that is up, if any. See [`Overlay`] for why it is not one.
    ///
    /// Dropped by anything that moves the marker it is anchored to — a document scroll,
    /// a horizontal scroll, a reflow — because a box pointing at a sentence that is no
    /// longer under it is worse than no box (design spec §6).
    popup: Option<Popup>,
    /// The first help row on screen, for a help overlay taller than the terminal.
    help_scroll: usize,
    /// The digits typed in front of a movement key, `less`-style.
    pending_count: String,
    notice: Option<Notice>,
    search_index: Option<usize>,
    search_backward: bool,
    search_mode: SearchMode,
    /// The mouse selection, in canvas coordinates. See [`super::select`].
    selection: Option<Selection>,
    /// Text a finished drag produced, waiting for the event loop to put it on the
    /// clipboard.
    ///
    /// The state machine touches no terminal and no display server (design spec §13),
    /// so it cannot copy; it hands the text over and is told the outcome through
    /// [`App::report_copy`].
    pending_copy: Option<Extract>,
    /// Whether the copy buttons may be drawn at all.
    ///
    /// Pager state rather than an [`AppOptions`] field, because it is not a setting
    /// anybody chooses: [`super::term`] sets it from whether mouse capture was actually
    /// granted, and a button nobody can click is worse than no button (design spec §4).
    copy_button: bool,
    /// The control a press landed on, as a [`Hotspot::target`](crate::canvas::Hotspot),
    /// for as long as a release could still turn it into a click.
    ///
    /// A target id rather than a hotspot index, because a wrapped link is *several*
    /// hotspots sharing one id: pressing its first row and releasing on its second is one
    /// click on one control and must fire, while an index would call those two controls.
    /// `None` means no click is in flight — either none was started, or a drag cancelled
    /// the one that was, permanently for that gesture.
    pressed: Option<usize>,
    /// The control showing its `[copied]` flash, and when it started.
    copied_flash: Option<(usize, u16, std::time::Instant)>,
    /// The control the pointer is over, as an index into the canvas's hotspots.
    ///
    /// An index rather than a copy of the hotspot, because what the painter needs is
    /// *which* control, and the canvas is the only thing entitled to say where that
    /// control is. It is dropped whenever a render replaces the canvas, alongside the
    /// selection and for the same reason: the indices belong to the canvas they were
    /// resolved against.
    hover: Option<usize>,
    /// The control the keyboard cursor sits on, as a
    /// [`Hotspot::target`](crate::canvas::Hotspot), for a reader with no mouse.
    ///
    /// A target rather than a hotspot index — unlike [`App::hover`] — because `f`/`F`
    /// must step *per control*, and a control that wraps across rows or inside a
    /// centred table cell is several hotspots sharing one target (design spec §2.2,
    /// §4). Stepping by index would stall on the second row of a wrapped link instead
    /// of advancing past it. Dropped on reflow for the same reason `hover` and
    /// `pressed` are: a target id is issued per canvas.
    cursor: Option<usize>,
    /// The [`Activation`] a keyboard `Confirm` just produced, for [`App::on_key`]'s
    /// caller to carry out.
    ///
    /// Mirrors [`App::pending_copy`]: the state machine touches no terminal or
    /// display server (design spec §13), so firing a control from the keyboard is
    /// handed over the same way a finished drag is, rather than acted on here.
    pending_activation: Option<Activation>,
    quit: bool,
}

impl App {
    /// Creates the pager over a parsed document.
    ///
    /// The requested theme is resolved through the configuration; an unknown name
    /// falls back to the built-in dark theme and leaves a notice in the status bar
    /// rather than refusing to start.
    pub fn new(doc: Doc, config: Config, options: AppOptions) -> Self {
        let (theme, notice) = match config.resolve_theme(&options.theme) {
            Ok(theme) => (theme, None),
            Err(error) => (
                Theme::default_dark(),
                Some(Notice {
                    text: error.to_string(),
                    is_error: true,
                }),
            ),
        };
        // The pane is numbered from the same computation the page is, gated by the same
        // setting: two derivations of a section number is two chances to disagree.
        let toc = Toc::from_doc(
            &doc,
            &crate::numbering::Numbering::enabled(&doc, config.section_numbers),
        );
        let toc_open = options.toc_open;
        let mut app = Self {
            doc,
            config,
            options,
            theme,
            cache: RenderCache::default(),
            toc,
            search: Search::empty(),
            size: (80, 24),
            scroll: 0,
            hscroll: 0,
            bar_grab: None,
            focus: if toc_open {
                Focus::Toc
            } else {
                Focus::Document
            },
            toc_open,
            toc_cursor: 0,
            toc_filter: String::new(),
            toc_hits: Vec::new(),
            overlay: Overlay::None,
            popup: None,
            help_scroll: 0,
            pending_count: String::new(),
            notice,
            search_index: None,
            search_backward: false,
            search_mode: SearchMode::Literal,
            selection: None,
            pending_copy: None,
            copy_button: false,
            pressed: None,
            copied_flash: None,
            hover: None,
            cursor: None,
            pending_activation: None,
            quit: false,
        };
        app.refilter_toc();
        app
    }

    /// The parsed document.
    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    /// The active configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The active theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The name shown in the status bar.
    pub fn title(&self) -> &str {
        &self.options.title
    }

    /// Whether Nerd Font glyphs may be drawn.
    pub fn icons(&self) -> bool {
        self.options.icons
    }

    /// The capability flags the document is rendered under.
    ///
    /// Distinct from the theme on purpose: the theme decides what things look like,
    /// these decide what the renderer may draw at all. They are part of the render
    /// cache key, so changing one re-renders.
    pub fn render_options(&self) -> RenderOptions {
        RenderOptions::new(self.options.icons, self.config.line_numbers)
            .with_title_banner(self.config.title_banner)
            .with_section_numbers(self.config.section_numbers)
            .with_copy_button(self.copy_button)
    }

    /// Turns the copy buttons on or off.
    ///
    /// Set from whether mouse capture was actually granted, not from what the
    /// configuration asked for: the control is only drawn where a reader can press it.
    /// No cache to invalidate by hand — [`RenderOptions`] is part of the render key, so
    /// the canvas drawn without the buttons is dropped by the next render.
    pub fn set_copy_button(&mut self, on: bool) {
        self.copy_button = on;
    }

    /// Turns Nerd Font glyphs on or off, in the chrome and the document alike.
    ///
    /// Deliberately not bound to a key: whether the terminal's font has the glyphs is
    /// a property of the terminal, not a reading preference, so it is settled once by
    /// `--icons` / `--no-icons` / `icons` in the configuration file rather than being
    /// something to discover by accident mid-document.
    pub fn set_icons(&mut self, icons: bool) {
        self.options.icons = icons;
    }

    /// Whether the user has asked to leave.
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// The table of contents, with rows attached for the current render.
    pub fn toc(&self) -> &Toc {
        &self.toc
    }

    /// The current search.
    pub fn search(&self) -> &Search {
        &self.search
    }

    /// The index of the match the viewport is sitting on, if any.
    pub fn search_index(&self) -> Option<usize> {
        self.search_index
    }

    /// How the search query is currently interpreted.
    ///
    /// This is an explicit, user-visible mode, not a guess: the prompt and the status
    /// bar both spell it out, and `toggle_search_mode` switches it.
    pub fn search_mode(&self) -> SearchMode {
        self.search_mode
    }

    /// The overlay currently up.
    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// Which pane has the keyboard.
    pub fn focus(&self) -> Focus {
        self.focus
    }

    /// Whether the table-of-contents pane is showing.
    pub fn toc_is_open(&self) -> bool {
        self.toc_open
    }

    /// The width of the table-of-contents pane, clamped to leave the document room.
    pub fn toc_width(&self) -> u16 {
        if !self.toc_open {
            return 0;
        }
        self.config.toc_width.min(self.size.0.saturating_sub(20))
    }

    /// The table-of-contents entries surviving the current filter.
    pub fn toc_hits(&self) -> &[FilterHit] {
        &self.toc_hits
    }

    /// The position of the selection within [`App::toc_hits`].
    pub fn toc_cursor(&self) -> usize {
        self.toc_cursor
    }

    /// The index into [`App::toc_hits`] drawn on the pane's first row.
    ///
    /// Derived rather than stored so that drawing and click hit-testing cannot
    /// disagree about which entry is where.
    pub fn toc_first_visible(&self, height: usize) -> usize {
        let len = self.toc_hits.len();
        if height == 0 || len <= height {
            return 0;
        }
        self.toc_cursor.saturating_sub(height / 2).min(len - height)
    }

    /// The transient status-bar message, if any.
    pub fn notice(&self) -> Option<&Notice> {
        self.notice.as_ref()
    }

    /// The first document row on screen.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// The horizontal offset, for content wider than the viewport.
    pub fn hscroll(&self) -> u16 {
        self.hscroll
    }

    /// How many columns of content sit beyond the right edge of the viewport.
    ///
    /// Zero when everything fits, which is how the status bar knows to say nothing
    /// about horizontal position. Non-zero when `--width` forced a wider render, or
    /// when [`super::wide`] widened a block that would otherwise have been clipped.
    ///
    /// Measured from the widest thing actually drawn rather than from the canvas
    /// width, which also counts the right-hand margin past it: scrolling into that
    /// margin moves nothing, and a readout that counted it would promise columns
    /// there is nothing to see in.
    ///
    /// The document is drawn in [`super::draw::Offsets::content`] columns, not in the
    /// whole viewport: one column on each side is the rail the edge markers are painted
    /// in, so that they never stand on a column of the document. A maximum measured
    /// against the viewport would stop the reader one column short of the far edge of a
    /// wide block — and, being what [`App::clamp`] clamps to, would do it silently.
    pub fn hscroll_max(&self) -> u16 {
        self.cache
            .max_reach()
            .saturating_sub(self.content_columns())
    }

    /// How many columns of the viewport the document itself is drawn in.
    ///
    /// The same arithmetic [`super::draw::Offsets`] paints with; see its `content`.
    fn content_columns(&self) -> u16 {
        let viewport = self.viewport_width();
        viewport.saturating_sub(crate::render::margins(viewport))
    }

    /// How far each row of the rendered document may be scrolled sideways.
    ///
    /// One entry per canvas row; see [`crate::render::document::scroll_reach`]. The viewport uses
    /// it to leave rows that fit at column 0 while an over-wide block scrolls.
    pub fn reach(&self) -> &[u16] {
        self.cache.reach()
    }

    /// How many leading columns of each row the horizontal offset must leave alone.
    ///
    /// One entry per canvas row; see [`crate::render::document::pinned_prefix`]. This is what keeps
    /// a code block's line-number gutter on screen while its long lines scroll under it.
    pub fn pinned(&self) -> &[u16] {
        self.cache.pinned()
    }

    /// The width the document is rendered at.
    ///
    /// The terminal's, unless `--width` forced another; the surplus of a forced width
    /// is reached by scrolling horizontally (design spec §11).
    pub fn content_width(&self) -> u16 {
        self.options.width.unwrap_or_else(|| self.viewport_width())
    }

    /// The number of columns of document actually on screen.
    ///
    /// This is what the reader can see; the canvas may be wider. It exceeds
    /// [`App::content_width`] never, and falls short of the *canvas* width whenever
    /// `--width` forced a wider render or [`super::wide`] widened an over-wide block.
    /// Either way the surplus is reached by scrolling horizontally — as far as there
    /// is anything drawn in it, which is what [`App::hscroll_max`] measures rather
    /// than the canvas width itself.
    pub fn viewport_width(&self) -> u16 {
        // One column is the scrollbar's gutter.
        self.size
            .0
            .saturating_sub(self.toc_width())
            .saturating_sub(1)
            .max(1)
    }

    /// The number of document rows on screen, status bar excluded.
    pub fn viewport_height(&self) -> usize {
        usize::from(self.size.1.saturating_sub(1)).max(1)
    }

    /// The rendered document. Renders on demand and caches the result.
    pub fn canvas(&mut self) -> &Canvas {
        self.ensure_rendered();
        self.cache.canvas()
    }

    /// The rendered document, assuming a render has already happened this frame.
    pub fn rendered(&self) -> &Canvas {
        self.cache.canvas()
    }

    /// The largest valid scroll offset.
    pub fn max_scroll(&self) -> usize {
        self.cache
            .canvas()
            .height()
            .saturating_sub(self.viewport_height())
    }

    /// How far through the document the viewport is, as a fraction in `0.0..=1.0`.
    pub fn progress(&self) -> f32 {
        let max = self.max_scroll();
        if max == 0 {
            1.0
        } else {
            (self.scroll as f32 / max as f32).clamp(0.0, 1.0)
        }
    }

    /// The column the document scrollbar is drawn in.
    ///
    /// The bar has the last column of the terminal to itself — see `draw`'s
    /// `bar_area` — which is also why [`App::viewport_width`] subtracts one.
    pub fn scrollbar_column(&self) -> u16 {
        self.size.0.saturating_sub(1)
    }

    /// Where the scrollbar's thumb sits, in half-cells, for a track `height` cells tall.
    ///
    /// `(start, length)`, measured in half-cells from the top of the track, because
    /// the painter positions the thumb to half-cell precision with the half-block
    /// glyphs. Both the painter and the mouse hit test go through this, so the two
    /// cannot disagree about which rows the thumb occupies — the same reason
    /// [`App::toc_first_visible`] is derived rather than stored.
    ///
    /// `length` depends only on how much of the document fits, never on where the
    /// reader is, which is what lets a drag compute its gain once and keep it.
    pub fn scrollbar_thumb(&self, height: u16) -> (usize, usize) {
        let halves = usize::from(height) * 2;
        if halves == 0 {
            return (0, 0);
        }
        let total = self.rendered().height().max(1);
        let visible = self.viewport_height().min(total);
        let length = ((visible * halves) / total).clamp(2, halves);
        let start = ((halves - length) as f32 * self.progress()).round() as usize;
        (start, length)
    }

    /// How far the thumb's top may travel, in half-cells. Zero when it cannot move.
    fn scrollbar_span(&self, height: u16) -> usize {
        let (_, length) = self.scrollbar_thumb(height);
        (usize::from(height) * 2).saturating_sub(length)
    }

    /// The scroll offset a press on track row `row` means.
    ///
    /// The thumb is *centred* on the pointer, clamped to the track. That is the
    /// proportional mapping written in the painter's own coordinates rather than in
    /// the track's, and it is worth the arithmetic for three reasons: it is the exact
    /// inverse of [`App::scrollbar_thumb`], so the thumb always lands under the
    /// pointer; it uses the same gain a drag does, so click-then-drag has no seam;
    /// and for any document long enough for the thumb to sit at its two-half-cell
    /// minimum it *is* the naive `row / height * max` mapping, which is the case that
    /// matters.
    ///
    /// The first and last rows of the track are the two ends of the document, taken
    /// before any arithmetic. A thumb with extent cannot centre itself on either end
    /// row, so rounding alone leaves the last line unreachable by up to half a row's
    /// worth of gain; there is nothing above the first row or below the last, so the
    /// only honest reading of a press there is "the top" and "the bottom".
    fn scrollbar_scroll_at(&self, height: u16, row: u16) -> usize {
        let max = self.max_scroll();
        if height == 0 || max == 0 {
            return 0;
        }
        if row == 0 {
            return 0;
        }
        if usize::from(row) + 1 >= usize::from(height) {
            return max;
        }
        let (_, length) = self.scrollbar_thumb(height);
        // Quarter-cells, so that the half-cell the pointer's own centre sits at stays
        // an integer: the pointer's centre is `2·row + ½` half-cells down, and the
        // thumb's is `start + length/2`.
        let span = self.scrollbar_span(height) * 2;
        if span == 0 {
            return 0;
        }
        let start = (4 * i64::from(row) + 1 - length as i64).clamp(0, span as i64) as usize;
        (start * max + span / 2) / span
    }

    /// Whether a scrollbar drag is in progress.
    ///
    /// The event loop asks this instead of testing the pointer's column, which is what
    /// keeps the grab sticky: once the thumb is held, the drag survives the pointer
    /// wandering off a bar one column wide, which it does constantly.
    pub fn scrollbar_grabbed(&self) -> bool {
        self.bar_grab.is_some()
    }

    /// Handles a left button press on track row `row` of a scrollbar `height` tall.
    ///
    /// A press on the thumb grabs it where it is and moves nothing — snapping the
    /// thumb's top to the pointer would throw the document a screenful the moment it
    /// was touched. A press anywhere else on the track jumps there first and then
    /// grabs, so the reader can carry straight on dragging.
    pub fn scrollbar_press(&mut self, height: u16, row: u16) {
        // The scrollbar is outside the popup, and a click outside dismisses it. Said
        // here rather than left to the scroll that usually follows, because grabbing the
        // thumb without moving it scrolls nothing and would otherwise leave the box up.
        self.close_popup();
        self.ensure_rendered();
        let (start, length) = self.scrollbar_thumb(height);
        let top = usize::from(row) * 2;
        // The cell covers half-cells `top` and `top + 1`; the thumb covers
        // `start..start + length`.
        let on_thumb = length > 0 && top < start + length && top + 1 >= start;
        if !on_thumb {
            let target = self.scrollbar_scroll_at(height, row);
            self.scroll_to(target);
        }
        self.bar_grab = Some(BarGrab {
            row,
            scroll: self.scroll,
        });
    }

    /// Handles the pointer moving to row `row` while the thumb is held.
    ///
    /// Relative to the anchor taken at the press, never to the pointer's absolute
    /// position, so the thumb keeps whatever grip the reader took on it. The anchor is
    /// never rewritten, which is what makes dragging past the end and back land
    /// exactly where it started rather than a rounding error away from it.
    pub fn scrollbar_drag(&mut self, height: u16, row: u16) {
        let Some(grab) = self.bar_grab else {
            return;
        };
        self.ensure_rendered();
        let max = self.max_scroll();
        if max == 0 {
            return;
        }
        // The ends of the track are the ends of the document, for the reason given at
        // `scrollbar_scroll_at` — and this is also where a pointer dragged off the
        // bottom of the terminal arrives.
        // The ends of the track are the ends of the document, for the reason given at
        // `scrollbar_scroll_at` — and this is also where a pointer dragged off the
        // bottom of the terminal arrives.
        if row == 0 {
            self.scroll_to(0);
            return;
        }
        if usize::from(row) + 1 >= usize::from(height) {
            self.scroll_to(max);
            return;
        }
        let span = self.scrollbar_span(height) as i64;
        if span == 0 {
            return;
        }
        // One row of pointer is two half-cells of thumb.
        let numerator = 2 * (i64::from(row) - i64::from(grab.row)) * max as i64;
        let moved = if numerator >= 0 {
            (numerator + span / 2) / span
        } else {
            -((-numerator + span / 2) / span)
        };
        let target = (grab.scroll as i64 + moved).clamp(0, max as i64) as usize;
        self.scroll_to(target);
    }

    /// Ends a scrollbar drag.
    pub fn scrollbar_release(&mut self) {
        self.bar_grab = None;
    }

    /// The heading whose section the viewport is in.
    pub fn current_heading(&self) -> Option<usize> {
        self.toc.current(self.heading_probe_row())
    }

    /// The row the current section is read from.
    ///
    /// Normally the top of the viewport, which is what makes the status bar agree
    /// with what the reader is about to read. At the very end of the document the
    /// top row can sit several sections above everything on screen, and naming a
    /// heading the reader scrolled past reads as simply wrong (usability P13), so
    /// the last row is used there instead.
    fn heading_probe_row(&self) -> usize {
        let max = self.max_scroll();
        if max > 0 && self.scroll >= max {
            self.cache.canvas().height().saturating_sub(1)
        } else {
            self.scroll
        }
    }

    /// Resizes the viewport, keeping the reader where they were.
    ///
    /// The scroll offset is *not* carried across verbatim: a reflow moves every row.
    /// Instead the source offset of the topmost visible text is remembered, the
    /// document is re-rendered at the new width, and the viewport is put back on
    /// whichever row that text now occupies. This is what makes a resize quiet
    /// (design spec §3 and §9).
    pub fn resize(&mut self, width: u16, height: u16) {
        if self.size == (width, height) {
            return;
        }
        self.ensure_rendered();
        // A drag cannot survive this: its anchor is a row of a track that is about to
        // change height, and the reflow moves every line under it.
        self.bar_grab = None;
        // Nor can the footnote popup, and for the same reason one step over: its anchor
        // is a *viewport* row, and its rectangle was measured against a document area
        // that is about to change shape. Said here rather than left to
        // `ensure_rendered`'s stale block, which is what covers reflow: staleness is
        // keyed on the render width, so a height-only resize never fires it — and under
        // `--width` no resize fires it at all. The box would then survive with geometry
        // the viewport no longer has: clipped away by the painter, invisible, and still
        // eating every movement key from a state the reader has no way to see.
        self.popup = None;
        let anchor = self.source_offset_at(self.scroll);
        self.size = (width, height);
        self.ensure_rendered();
        if let Some(offset) = anchor {
            self.scroll = self.row_for_source_offset(offset);
        }
        self.clamp();
        self.track_toc();
    }

    /// Renders the document if the cache is stale, then re-attaches anchors and hits.
    ///
    /// Dropping the cache changes nothing visible: everything derived from a render is
    /// recomputed here, never carried over.
    fn ensure_rendered(&mut self) {
        let width = self.content_width();
        let options = self.render_options();
        let stale = self.cache.refresh(
            self.doc.version(),
            width,
            self.config.body_width,
            &self.theme,
            options,
            || {
                crate::render::render_document(
                    &self.doc,
                    width,
                    self.config.body_width,
                    &self.theme,
                    &options,
                )
            },
        );
        if stale {
            self.toc.attach_anchors(self.cache.canvas().anchors());
            self.search
                .locate(self.doc.source(), self.cache.canvas().spans());
            // A mouse selection is a rectangle of *cells*, and a new render is a new
            // set of cells: rendering is a pure function of width (design spec §3), so
            // a reflow moves every row and the cells the reader picked out now hold
            // different text. Search survives because it is anchored in the source and
            // re-projected above; a selection cannot be, because re-projecting it would
            // silently change what is highlighted — the source hull behind a screenful
            // at 100 columns is not the hull behind a screenful at 40. Dropping it is
            // the honest answer, and the reader has already had the release event that
            // put their text on the clipboard.
            self.selection = None;
            // And the hover, which names a control by its position in the old canvas's
            // hotspot list. The pointer has not moved, but what is under it may have,
            // and a stale index would paint the highlight onto a different button.
            self.hover = None;
            // And the keyboard cursor, for the same reason: it too names a control by a
            // target id issued per canvas, and a reflow issues a new one.
            self.cursor = None;
            // And a click in flight, for the same reason one step further on: a target id
            // is issued per canvas, so the id a press recorded may name a different
            // control on the new one — or none. A reflow mid-click is a reflow the reader
            // caused (a resize, a toggled option) and the click is theirs to repeat; a
            // click that fired the wrong link would not be.
            self.pressed = None;
            // And the footnote popup, one step further still: it is anchored to the cell
            // its marker was drawn in, and a reflow moves that cell. The box would keep
            // its place on screen while the sentence it belongs to moved out from under
            // it.
            self.popup = None;
            self.clamp();
        }
    }

    /// Puts away anything anchored to a document cell, because the document just moved.
    ///
    /// Called by every path that changes where the document sits under the viewport —
    /// the vertical scroll, the horizontal scroll, and (in [`App::ensure_rendered`]) a
    /// reflow. Design spec §6 lists "scrolling the document" as one of the three ways a
    /// footnote popup is dismissed, and the reason is the anchor: the box points at the
    /// marker it was opened from, and a box left pointing at whatever has scrolled into
    /// that cell would be a claim about the document that is no longer true.
    fn moved_document(&mut self) {
        self.popup = None;
    }

    /// Clamps the scroll offsets into range.
    fn clamp(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
        self.hscroll = self.hscroll.min(self.hscroll_max());
    }

    /// The smallest source offset drawn at or below `row`.
    fn source_offset_at(&self, row: usize) -> Option<usize> {
        self.cache
            .canvas()
            .spans()
            .iter()
            .filter(|span| span.row >= row)
            .map(|span| span.source_start)
            .min()
    }

    /// The row that best represents `offset` in the current render.
    fn row_for_source_offset(&self, offset: usize) -> usize {
        self.cache
            .canvas()
            .spans()
            .iter()
            .filter(|span| span.source_end > offset)
            .map(|span| span.row)
            .min()
            .unwrap_or(self.scroll)
            .min(self.max_scroll())
    }

    /// Scrolls by `delta` rows, clamped at both ends.
    pub fn scroll_by(&mut self, delta: isize) {
        self.moved_document();
        self.ensure_rendered();
        let max = self.max_scroll();
        let target = if delta >= 0 {
            self.scroll.saturating_add(delta.unsigned_abs())
        } else {
            self.scroll.saturating_sub(delta.unsigned_abs())
        };
        self.scroll = target.min(max);
        self.track_toc();
    }

    /// Puts `row` at the top of the viewport, clamped.
    pub fn scroll_to(&mut self, row: usize) {
        self.moved_document();
        self.ensure_rendered();
        self.scroll = row.min(self.max_scroll());
        self.track_toc();
    }

    /// Brings `row` into view, leaving a little context above it when scrolling up.
    pub fn reveal(&mut self, row: usize) {
        self.moved_document();
        self.ensure_rendered();
        let height = self.viewport_height();
        let margin = (height / 5).min(3);
        if row < self.scroll + margin {
            self.scroll = row.saturating_sub(margin);
        } else if row >= self.scroll + height {
            self.scroll = row + margin + 1 - height;
        }
        self.clamp();
        self.track_toc();
    }

    /// Brings `row` into view with context on both sides of it.
    ///
    /// A match landing on the last visible row, with nothing after it, tells the
    /// reader almost nothing (visual review P16); centring is what makes a hit
    /// readable in its surroundings. A row already comfortably on screen is left
    /// where it is, so stepping through nearby matches does not lurch.
    pub fn reveal_centered(&mut self, row: usize) {
        self.moved_document();
        self.ensure_rendered();
        let height = self.viewport_height();
        let margin = (height / 4).max(1);
        let comfortable = row >= self.scroll.saturating_add(margin)
            && row + margin < self.scroll.saturating_add(height);
        if comfortable {
            return;
        }
        self.scroll = row.saturating_sub(height / 2).min(self.max_scroll());
        self.track_toc();
    }

    /// Brings the canvas columns `col..col + cols` of `row` into view sideways.
    ///
    /// Vertical reveal alone is not "reaching" a match. A hit in the over-wide part of a
    /// table or a long code line is off screen to the *right*, and jumping to it moved
    /// only the scroll — so pressing `n` changed the counter on the status bar and not a
    /// single character of the page, which is what made match navigation look broken to
    /// the owner. Nothing is moved when the span is already fully drawn, or the page
    /// would jolt sideways for every hit inside one paragraph.
    ///
    /// The arithmetic is [`super::draw::Offsets`]' own, from the other end: that painter
    /// draws canvas column `c` of a row at viewport column `c - at(row)`, for `c` at or
    /// past the row's pinned prefix, and only while that lands inside
    /// [`App::content_columns`]. So the span is visible exactly when the row's applied
    /// offset lies in `(end - content, col - pinned]`, and the reveal picks the offset
    /// that centres the span in that window when it does not.
    fn reveal_columns(&mut self, row: usize, col: u16, cols: u16) {
        let content = self.content_columns();
        let reach = self.reach().get(row).copied().unwrap_or(0);
        // A row with nowhere to go is pinned at offset zero by `Offsets::at` whatever the
        // reader has scrolled to, so its content is on screen already or nowhere at all.
        let Some(furthest) = reach.checked_sub(content).filter(|max| *max > 0) else {
            return;
        };
        let pinned = self.pinned_columns(row);
        let end = col.saturating_add(cols);
        let applied = self.hscroll.min(furthest);
        let visible = col >= applied.saturating_add(pinned) && end <= applied + content;
        if visible {
            return;
        }
        // Centred in the columns the document is drawn in, then held to the window that
        // actually shows the span: a match wider than the viewport is put flush against
        // the pinned prefix rather than centred out of reach on both sides.
        let centre = i32::from(col) + i32::from(cols) / 2 - i32::from(content) / 2;
        let latest = i32::from(col).saturating_sub(i32::from(pinned));
        let earliest = i32::from(end) - i32::from(content);
        let target = centre.clamp(earliest.min(latest), latest);
        self.hscroll = u16::try_from(target.max(0))
            .unwrap_or(u16::MAX)
            .min(furthest);
    }

    /// How many leading columns of `row` the horizontal offset leaves alone.
    ///
    /// [`super::draw::Offsets::pinned`] in the state layer: a gutter wider than the
    /// document's own columns is given up entirely, and every row keeps the margin rail
    /// the edge markers are painted in.
    fn pinned_columns(&self, row: usize) -> u16 {
        let content = self.content_columns();
        let gutter = self.pinned().get(row).copied().unwrap_or(0);
        let gutter = if gutter >= content { 0 } else { gutter };
        gutter.max(crate::render::margins(self.viewport_width()).min(content))
    }

    /// Keeps the table-of-contents selection on the section being read.
    ///
    /// Design spec §9 asks for "the current section highlighted"; a map that stops
    /// updating the moment the reader scrolls is worse than no map at all. The
    /// selection is left alone while the pane has the keyboard, so tracking never
    /// pulls the cursor out from under someone navigating it.
    fn track_toc(&mut self) {
        if self.focus == Focus::Toc {
            return;
        }
        self.select_current_heading();
    }

    /// Puts the selection on the section the viewport is in, whatever has focus.
    fn select_current_heading(&mut self) {
        if let Some(current) = self.current_heading()
            && let Some(position) = self.toc_hits.iter().position(|hit| hit.index == current)
        {
            self.toc_cursor = position;
        }
    }

    /// Sets a transient status-bar message.
    pub fn notify(&mut self, text: impl Into<String>, is_error: bool) {
        self.notice = Some(Notice {
            text: text.into(),
            is_error,
        });
    }

    /// Clears any transient status-bar message.
    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    /// Asks the pager to exit at the next opportunity.
    pub fn quit(&mut self) {
        self.quit = true;
    }
}

/// Key and mouse handling.
impl App {
    /// Handles one key press.
    pub fn on_key(&mut self, key: Key) -> KeyOutcome {
        self.clear_notice();
        if let Overlay::Prompt { kind, input } = &self.overlay {
            let (kind, input) = (*kind, input.clone());
            self.prompt_key(kind, input, key);
            return KeyOutcome::default();
        }
        if self.overlay == Overlay::Help {
            self.help_key(key);
            return KeyOutcome::default();
        }
        match self.config.keys.action(&key) {
            Some(action) => {
                let count = self.take_count();
                self.act_with_count(action, count);
            }
            // Digits in front of a movement key are a repeat count, the way `less`
            // and vi read them. A digit that the user has bound to something wins,
            // which is why the binding is consulted first.
            None if is_count_digit(key) => self.push_count(key),
            // Silence is the worst answer: a reader who typed something the pager
            // does not know needs to be told, not left wondering whether it hung.
            None => {
                self.pending_count.clear();
                self.notify(
                    format!("{} is not bound — press h for help", key.label()),
                    false,
                );
            }
        }
        KeyOutcome {
            activation: self.pending_activation.take(),
        }
    }

    /// Handles one action, dispatching on the focused pane.
    pub fn act(&mut self, action: Action) {
        self.act_with_count(action, None);
    }

    /// Handles one action, repeated or parameterised by a leading count.
    pub fn act_with_count(&mut self, action: Action, count: Option<usize>) {
        let height = self.viewport_height();
        let times = count.unwrap_or(1).max(1);
        // "Long footnotes scroll: the wheel over the popup, **or the cursor keys while
        // it is open**" (design spec §6). Only the movement keys are taken, and only
        // while a popup is up: everything else — `q`, `/`, `f`, `Esc` — still reaches
        // the document, which is the whole difference between this and an `Overlay`.
        // Without this the one key a reader would try to scroll a long note with would
        // scroll the document instead and, by doing so, dismiss the note.
        if self.popup.is_some()
            && let Some(delta) = popup_scroll(action, height, times)
        {
            self.scroll_popup(delta);
            return;
        }
        let lines =
            |per: usize| -> isize { isize::try_from(per.saturating_mul(times)).unwrap_or(0) };
        match action {
            Action::Quit => self.quit(),
            Action::Cancel => self.cancel(),
            Action::Help => {
                self.overlay = Overlay::Help;
                self.help_scroll = 0;
            }
            Action::ToggleToc => self.toggle_toc(),
            Action::CycleTheme => self.cycle_theme(),
            Action::ToggleLineNumbers => self.toggle_line_numbers(),
            Action::SaveConfig => self.save_config(),
            Action::ReportPosition => self.report_position(),
            // Design spec §9: `/` inside the table of contents filters it fuzzily
            // rather than searching the document.
            Action::SearchForward if self.focus == Focus::Toc => self.start_toc_filter(),
            Action::SearchForward => self.open_prompt(PromptKind::SearchForward),
            Action::SearchBackward => self.open_prompt(PromptKind::SearchBackward),
            Action::NextMatch => self.repeat(times, |app| app.step_match(!app.search_backward)),
            Action::PrevMatch => self.repeat(times, |app| app.step_match(app.search_backward)),
            Action::ToggleSearchMode => self.toggle_search_mode(),
            Action::Confirm if self.focus == Focus::Toc => self.jump_to_selected_heading(),
            // Fires through the same channel a finished drag uses: `act_with_count` is
            // not allowed to touch a terminal (design spec §13), so the activation is
            // handed to `pending_activation` for `on_key` to hand further to
            // `super::term`, which does the I/O and, for `Anchor`/`Footnote`, calls
            // back into `App::activate`.
            Action::Confirm => self.pending_activation = self.activate_cursor(),
            Action::CursorNext if self.focus == Focus::Document => self.cursor_step(true),
            Action::CursorPrev if self.focus == Focus::Document => self.cursor_step(false),
            // The table of contents has no controls of its own to cycle through: `f`/`F`
            // do nothing while it has focus, rather than reaching into the narrowed
            // document behind it.
            Action::CursorNext | Action::CursorPrev => {}
            Action::ScrollLeft => {
                // Sideways is still the document moving under the anchor, so the popup
                // goes for the same reason it does on a vertical scroll.
                self.moved_document();
                self.hscroll = self
                    .hscroll
                    .saturating_sub(HSCROLL_STEP.saturating_mul(u16::try_from(times).unwrap_or(1)));
            }
            Action::ScrollRight => {
                self.moved_document();
                self.ensure_rendered();
                self.hscroll = self
                    .hscroll
                    .saturating_add(HSCROLL_STEP.saturating_mul(u16::try_from(times).unwrap_or(1)));
                self.clamp();
            }
            Action::PrevHeading => self.repeat(times, |app| app.step_heading(false)),
            Action::NextHeading => self.repeat(times, |app| app.step_heading(true)),
            Action::Percent => self.scroll_to_percent(count.unwrap_or(0)),
            _ if self.focus == Focus::Toc => self.toc_move(action, height, times),
            Action::LineDown => self.scroll_by(lines(1)),
            Action::LineUp => self.scroll_by(-lines(1)),
            Action::HalfPageDown => self.scroll_by(lines((height / 2).max(1))),
            Action::HalfPageUp => self.scroll_by(-lines((height / 2).max(1))),
            Action::PageDown => self.scroll_by(lines(height.saturating_sub(1).max(1))),
            Action::PageUp => self.scroll_by(-lines(height.saturating_sub(1).max(1))),
            // `100g` goes to line 100, as it does in `less`; a bare `g` goes home.
            //
            // Home is both axes. Horizontal scrolling is per-block now, but a reader
            // who has followed a wide table sideways still has no other way back: `0`
            // is the count prefix, and `^` is unbound. Going to a row means arriving
            // at the start of it, whether or not a count named the row.
            Action::Top => {
                self.hscroll = 0;
                self.scroll_to(count.map_or(0, |line| line.saturating_sub(1)));
            }
            Action::Bottom => match count {
                Some(line) => self.scroll_to(line.saturating_sub(1)),
                None => {
                    self.ensure_rendered();
                    let bottom = self.max_scroll();
                    self.scroll_to(bottom);
                }
            },
        }
    }

    /// Runs `step` `times` times.
    fn repeat(&mut self, times: usize, step: impl Fn(&mut Self)) {
        for _ in 0..times.min(MAX_REPEAT) {
            step(self);
        }
    }

    /// Appends a digit to the pending repeat count.
    fn push_count(&mut self, key: Key) {
        if let KeyCode::Char(ch) = key.code
            && self.pending_count.len() < MAX_COUNT_DIGITS
        {
            self.pending_count.push(ch);
        }
        let count = self.pending_count.clone();
        self.notify(format!("{count}…"), false);
    }

    /// Takes and clears the pending repeat count.
    fn take_count(&mut self) -> Option<usize> {
        let count = std::mem::take(&mut self.pending_count);
        count.parse::<usize>().ok()
    }

    /// The repeat count typed so far, for the status bar.
    pub fn pending_count(&self) -> &str {
        &self.pending_count
    }

    /// The first help row on screen.
    pub fn help_scroll(&self) -> usize {
        self.help_scroll
    }

    /// Handles a key while the help overlay is up.
    ///
    /// Movement scrolls the overlay rather than the document, because an overlay too
    /// tall for the terminal that cannot be scrolled is exactly the trap design spec
    /// §10 is trying to avoid.
    fn help_key(&mut self, key: Key) {
        let height = self.viewport_height();
        match self.config.keys.action(&key) {
            Some(Action::Quit) => self.quit(),
            Some(Action::LineDown) => self.help_scroll = self.help_scroll.saturating_add(1),
            Some(Action::LineUp) => self.help_scroll = self.help_scroll.saturating_sub(1),
            Some(Action::HalfPageDown) => {
                self.help_scroll = self.help_scroll.saturating_add((height / 2).max(1));
            }
            Some(Action::HalfPageUp) => {
                self.help_scroll = self.help_scroll.saturating_sub((height / 2).max(1));
            }
            Some(Action::PageDown) => self.help_scroll = self.help_scroll.saturating_add(height),
            Some(Action::PageUp) => self.help_scroll = self.help_scroll.saturating_sub(height),
            Some(Action::Top) => self.help_scroll = 0,
            Some(Action::Bottom) => self.help_scroll = usize::MAX,
            // Anything else — including Esc and a second `h` — puts the help away.
            _ => {
                self.overlay = Overlay::None;
                self.help_scroll = 0;
            }
        }
    }

    /// Clamps the help scroll to what the overlay actually has to show.
    ///
    /// Called by the drawing code, which is the only thing that knows how the rows
    /// were laid out at this terminal size.
    pub fn clamp_help_scroll(&mut self, max: usize) {
        self.help_scroll = self.help_scroll.min(max);
    }

    /// Unwinds one step of state, and never quits.
    ///
    /// Design spec §10 calls `Esc` "cancel". The review that prompted this made the
    /// case that a reader presses it precisely when they are unsure, so destroying
    /// their position is the one thing it must not do: `q` quits, unambiguously and
    /// on purpose. `Esc` therefore peels state off one layer at a time and, when
    /// there is nothing left to peel, says so.
    fn cancel(&mut self) {
        // The outermost layer of all: a box drawn over the document, which is the one
        // piece of state here the reader can see the edges of. `Esc` never quits (design
        // spec §10), which is exactly what leaves it free to close this.
        if self.popup.take().is_some() {
            self.notify("footnote closed", false);
            return;
        }
        // Then the newest: a reader steps the keyboard cursor onto a
        // control to look at it, and `Esc` is how they say "never mind" without
        // following it — the same standing this gives a fresh selection just below.
        if self.cursor.take().is_some() {
            self.notify("cursor dropped", false);
            return;
        }
        if self.selection.take().is_some() {
            self.notify("selection cleared", false);
            return;
        }
        if !self.pending_count.is_empty() {
            self.pending_count.clear();
            return;
        }
        if !self.search.query().is_empty() {
            self.search = Search::empty();
            self.search_index = None;
            self.notify("search cleared", false);
            return;
        }
        if !self.toc_filter.is_empty() {
            self.toc_filter.clear();
            self.refilter_toc();
            self.sync_toc_selection();
            self.notify("filter cleared", false);
            return;
        }
        if self.toc_open && self.focus == Focus::Toc {
            self.focus = Focus::Document;
            return;
        }
        if self.toc_open {
            self.close_toc();
            return;
        }
        self.notify("nothing to cancel — press q to quit", false);
    }

    /// Reports the reading position, the way `less` answers `Ctrl-G`.
    fn report_position(&mut self) {
        self.ensure_rendered();
        let total = self.rendered().height();
        let first = self.scroll + 1;
        let last = (self.scroll + self.viewport_height()).min(total);
        let percent = (self.progress() * 100.0).round() as u16;
        let heading = self
            .current_heading()
            .and_then(|index| self.toc.entries().get(index))
            .map(|entry| format!(" — {}", entry.text))
            .unwrap_or_default();
        self.notify(
            format!(
                "{} lines {first}-{last} of {total} ({percent}%){heading}",
                self.options.title
            ),
            false,
        );
    }

    /// Jumps to a percentage of the document, the way `less` answers `50%`.
    fn scroll_to_percent(&mut self, percent: usize) {
        self.ensure_rendered();
        let max = self.max_scroll();
        let target = max.saturating_mul(percent.min(100)) / 100;
        self.scroll_to(target);
        self.notify(format!("{}%", percent.min(100)), false);
    }

    /// Turns the code-block line-number gutter on or off.
    fn toggle_line_numbers(&mut self) {
        self.config.line_numbers = !self.config.line_numbers;
        self.ensure_rendered();
        self.notify(
            if self.config.line_numbers {
                "line numbers on"
            } else {
                "line numbers off"
            },
            false,
        );
    }

    /// Writes the settings the reader can change back to the configuration file.
    ///
    /// The theme and the state of the contents pane are taken from the live pager
    /// rather than from `self.config`, which still holds the values it started with;
    /// everything else is settled in `self.config` as it is changed. The outcome —
    /// including the path — always reaches the status bar, because a save that says
    /// nothing is indistinguishable from a save that did nothing.
    fn save_config(&mut self) {
        let Some(path) = self.options.config_path.clone() else {
            self.notify("no configuration directory to save to", true);
            return;
        };
        let mut settings = self.config.clone();
        settings.theme = self.theme.name.clone();
        settings.toc_open = self.toc_open;
        match settings.save_to(&path) {
            Ok(()) => self.notify(format!("settings saved to {}", path.display()), false),
            Err(error) => self.notify(error.to_string(), true),
        }
    }

    /// Moves the selection inside the table-of-contents pane.
    fn toc_move(&mut self, action: Action, height: usize, times: usize) {
        let last = self.toc_hits.len().saturating_sub(1);
        let by = |per: usize| -> isize { isize::try_from(per.saturating_mul(times)).unwrap_or(0) };
        let step = |cursor: usize, delta: isize| -> usize {
            if delta >= 0 {
                cursor.saturating_add(delta.unsigned_abs()).min(last)
            } else {
                cursor.saturating_sub(delta.unsigned_abs())
            }
        };
        self.toc_cursor = match action {
            Action::LineDown => step(self.toc_cursor, by(1)),
            Action::LineUp => step(self.toc_cursor, -by(1)),
            Action::HalfPageDown => step(self.toc_cursor, by((height / 2).max(1))),
            Action::HalfPageUp => step(self.toc_cursor, -by((height / 2).max(1))),
            Action::PageDown => step(self.toc_cursor, by(height.max(1))),
            Action::PageUp => step(self.toc_cursor, -by(height.max(1))),
            Action::Top => 0,
            Action::Bottom => last,
            _ => self.toc_cursor,
        };
    }

    /// Shows or hides the table-of-contents pane, moving focus with it.
    fn toggle_toc(&mut self) {
        if self.toc_open {
            self.close_toc();
            return;
        }
        // Design spec §9 docks the pane at 30 columns and leaves the document 20; in
        // a terminal too narrow for both, say so instead of doing nothing (P16).
        if self.size.0 < MIN_TOC_TERMINAL_WIDTH {
            self.notify("the terminal is too narrow for the contents pane", false);
            return;
        }
        self.toc_open = true;
        self.focus = Focus::Toc;
        self.sync_toc_selection();
        self.clamp();
    }

    /// Hides the table-of-contents pane, forgetting any filter with it.
    ///
    /// A filter that survives a close/reopen (visual review P15b) leaves the reader
    /// with a permanently one-item map and no obvious way back.
    fn close_toc(&mut self) {
        self.toc_open = false;
        self.focus = Focus::Document;
        if !self.toc_filter.is_empty() {
            self.toc_filter.clear();
            self.refilter_toc();
        }
        self.sync_toc_selection();
        self.clamp();
    }

    /// Puts the selection on the section the viewport is currently in.
    fn sync_toc_selection(&mut self) {
        self.ensure_rendered();
        self.select_current_heading();
    }

    /// Jumps the document to the heading the table of contents has selected.
    fn jump_to_selected_heading(&mut self) {
        let Some(hit) = self.toc_hits.get(self.toc_cursor) else {
            return;
        };
        let index = hit.index;
        self.ensure_rendered();
        match self.toc.row_of(index) {
            Some(row) => {
                self.scroll_to(row);
                self.focus = Focus::Document;
            }
            None => self.notify("that heading was not rendered", true),
        }
    }

    /// Does what activating a hotspot does to the app's own state.
    ///
    /// Kinds that touch the terminal or the display server — [`HotspotKind::Open`],
    /// [`HotspotKind::Copy`] — are handled entirely by [`super::term::activate`], which
    /// owns that I/O (design spec §13); this covers the kinds that are pure state, so a
    /// test can drive them without a terminal. Task 9 gives `Footnote` its behaviour;
    /// until then it is recognised and does nothing, the same stance `term::activate`
    /// took for `Anchor` before this task.
    ///
    /// # Why the whole [`Activation`] and not just its kind
    ///
    /// **Changed 2026-08-13 (Task 9b).** A footnote popup is anchored to the marker that
    /// opened it, so *where* the control was drawn is part of what activating it means —
    /// and the row and column were already in hand at both call sites, the mouse's and
    /// the keyboard's. Passing the kind alone would have meant a second hit test here to
    /// find the control the caller had just resolved.
    pub fn activate(&mut self, activation: Activation) {
        match activation.kind {
            HotspotKind::Anchor { slug } => self.activate_anchor(&slug),
            HotspotKind::Footnote { id } => {
                self.open_footnote(&id, activation.row, activation.col);
            }
            // `Copy` and `Open` touch the clipboard and the display server, which the
            // state machine may not (design spec §13); `super::term::activate` owns them.
            HotspotKind::Copy { .. } | HotspotKind::Open { .. } => {}
        }
    }

    /// Opens the popup holding the footnote named `id`, anchored to the marker drawn at
    /// canvas `(row, col)`.
    ///
    /// # The note comes from the ordinary renderer
    ///
    /// [`crate::render::render_blocks`] lays out the definition's children at the box's
    /// inner width — the same `render_sequence` walk
    /// [`crate::render::render_document`] enters for every top-level block of the page,
    /// entered one level down. That is the whole of the popup's rendering: a popup is
    /// *another width*, not a second rendering path, so a list, a code span or a table
    /// inside a footnote is laid out by the code that lays them out everywhere else.
    ///
    /// `render_document` itself is the wrong entry point here despite what the brief
    /// said, and for a mundane reason: it takes a whole [`Doc`], and a footnote
    /// definition is a node inside one. It would also apply the document's own side
    /// margins, its lone-`#` title banner and its section numbering to the inside of a
    /// popup — three whole-document decisions that a footnote is not a document for.
    ///
    /// # The status bar never lies
    ///
    /// A name matching no definition names itself in the notice and opens nothing, the
    /// same stance [`App::activate_anchor`] takes for a slug that matches no heading.
    fn open_footnote(&mut self, id: &str, row: usize, col: u16) {
        self.ensure_rendered();
        if popup::definition(self.doc.root(), id).is_none() {
            self.notify(format!("no footnote named {id}"), true);
            return;
        }
        let screen = (self.viewport_width(), self.popup_screen_height());
        if !popup::fits(screen) {
            self.notify("the terminal is too small for a footnote popup", false);
            return;
        }
        // A control the reader stepped to with `f`/`F` may be off screen, and a box
        // anchored to a cell that is not on the viewport would be a box pointing
        // nowhere. Bringing the marker into view first is what makes the keyboard route
        // land the same box the pointer route does.
        if self.viewport_cell(row, col).is_none() {
            self.reveal(row);
            self.ensure_rendered();
        }
        let anchor = self.viewport_cell(row, col).unwrap_or((0, 0));
        let width = popup::inner_width(screen.0);
        // `copy_button: false`, whatever the pager is running with: a `[copy]` on a code
        // fence inside the popup would be a control, and controls inside a popup are
        // inert (design spec §1.1). A button that cannot be pressed is worse than no
        // button (§4), so the renderer is told not to draw one.
        let options = RenderOptions {
            copy_button: false,
            ..self.render_options()
        };
        let (canvas, label) = {
            let Some(node) = popup::definition(self.doc.root(), id) else {
                return;
            };
            let label = match &node.kind {
                crate::doc::NodeKind::FootnoteDefinition { name, number } => {
                    crate::render::block::footnote_label(name, *number)
                }
                _ => id.to_string(),
            };
            (
                crate::render::render_blocks(&node.children, width, &self.theme, &options),
                label,
            )
        };
        // `None` when neither the room above the marker nor the room below it can hold a
        // box that does not cover the marker itself (see `popup::place`). Reported, not
        // swallowed: a marker that reacts to a click and then does nothing visible is
        // exactly the control design spec §1.1 refuses to offer.
        match Popup::new(canvas, label, anchor, screen, self.theme.base()) {
            Some(popup) => self.popup = Some(popup),
            None => self.notify("no room beside the marker for the footnote", false),
        }
    }

    /// The rows of the document area a popup may occupy.
    ///
    /// The viewport, not the terminal: the status bar is not a place a box may cover,
    /// and neither is the row the marker itself is on when the box opens downwards.
    fn popup_screen_height(&self) -> u16 {
        u16::try_from(self.viewport_height()).unwrap_or(u16::MAX)
    }

    /// Where canvas cell `(row, col)` is drawn in the document area, when it is drawn.
    ///
    /// The painter's own arithmetic, run forwards: [`Offsets::x_of`] is what
    /// [`super::draw`] positions every highlight with, so a popup can never disagree
    /// with the pixels about which cell its marker was in. `None` for a cell that is
    /// off the top or bottom of the viewport, behind a pinned prefix, or past the
    /// right-hand rail.
    fn viewport_cell(&self, row: usize, col: u16) -> Option<(u16, u16)> {
        let y = u16::try_from(row.checked_sub(self.scroll)?).ok()?;
        if usize::from(y) >= self.viewport_height() {
            return None;
        }
        let offsets = Offsets::scrolled_to(
            self.reach(),
            self.pinned(),
            self.hscroll,
            self.viewport_width(),
        );
        let x = offsets.x_of(row, col)?;
        (x < offsets.content()).then_some((x, y))
    }

    /// Scrolls the heading `slug` names to the top row, or says it found none.
    ///
    /// Slugs are resolved through the same [`Toc`] the table of contents and `[`/`]`
    /// use — [`Toc::index_of`] against [`doc::Heading::id`](crate::doc::Heading), then
    /// [`Toc::row_of`] for where that heading actually rendered — so an anchor and the
    /// TOC can never disagree about which heading a duplicated title means. The status
    /// bar never lies: a slug matching nothing names itself in the notice and leaves
    /// the scroll position untouched, rather than guessing.
    fn activate_anchor(&mut self, slug: &str) {
        self.ensure_rendered();
        match self
            .toc
            .index_of(slug)
            .and_then(|index| self.toc.row_of(index))
        {
            Some(row) => self.scroll_to(row),
            None => self.notify(format!("no heading matches #{slug}"), true),
        }
    }

    /// Jumps to the previous or next heading in the document.
    fn step_heading(&mut self, forward: bool) {
        self.ensure_rendered();
        let target = if forward {
            self.toc.next_after(self.scroll)
        } else {
            self.toc.prev_before(self.scroll)
        };
        match target.and_then(|index| self.toc.row_of(index)) {
            Some(row) => self.scroll_to(row),
            None if forward => self.notify("no further heading", false),
            None => self.notify("no earlier heading", false),
        }
    }

    /// Switches to the next configured theme and re-renders.
    fn cycle_theme(&mut self) {
        let next = self.config.next_theme_name(&self.theme.name);
        match self.config.resolve_theme(&next) {
            Ok(theme) => {
                self.theme = theme;
                self.notify(format!("theme: {next}"), false);
                self.ensure_rendered();
            }
            Err(error) => self.notify(error.to_string(), true),
        }
    }

    /// Opens a prompt, pre-filled with what the user last typed there.
    fn open_prompt(&mut self, kind: PromptKind) {
        let input = match kind {
            PromptKind::TocFilter => self.toc_filter.clone(),
            _ => String::new(),
        };
        self.overlay = Overlay::Prompt { kind, input };
    }

    /// Handles a key while a prompt is up.
    fn prompt_key(&mut self, kind: PromptKind, mut input: String, key: Key) {
        match key.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
                if kind == PromptKind::TocFilter {
                    self.toc_filter.clear();
                    self.refilter_toc();
                }
                return;
            }
            KeyCode::Enter => {
                self.overlay = Overlay::None;
                self.accept_prompt(kind, &input);
                return;
            }
            KeyCode::Backspace => {
                if input.pop().is_none() {
                    self.overlay = Overlay::None;
                    return;
                }
            }
            // The mode can be switched mid-query, which is when one usually realises
            // the pattern wants to be a regular expression.
            KeyCode::Char('r') if key.mods.contains(crate::config::KeyMods::CTRL) => {
                if kind != PromptKind::TocFilter {
                    self.search_mode = match self.search_mode {
                        SearchMode::Literal => SearchMode::Regex,
                        SearchMode::Regex => SearchMode::Literal,
                    };
                }
            }
            KeyCode::Char(ch) if !key.mods.contains(crate::config::KeyMods::CTRL) => {
                input.push(ch);
            }
            _ => {}
        }
        // The table-of-contents filter is incremental; searching is not, because
        // re-scanning the whole source on every keystroke is the one thing here that
        // is not cheap.
        if kind == PromptKind::TocFilter {
            self.toc_filter.clone_from(&input);
            self.refilter_toc();
        }
        self.overlay = Overlay::Prompt { kind, input };
    }

    /// Applies a prompt's input.
    fn accept_prompt(&mut self, kind: PromptKind, input: &str) {
        match kind {
            // Design spec §9 says "`Enter` jumps". Committing the filter and then
            // demanding a second `Enter` (usability P5) is not that.
            PromptKind::TocFilter => {
                self.toc_filter = input.to_string();
                self.refilter_toc();
                self.focus = Focus::Toc;
                if !self.toc_hits.is_empty() {
                    self.jump_to_selected_heading();
                }
            }
            PromptKind::SearchForward | PromptKind::SearchBackward => {
                self.search_backward = kind == PromptKind::SearchBackward;
                self.run_search(input);
            }
        }
    }

    /// Switches between literal and regular-expression searching.
    ///
    /// The mode is explicit and sticky: it survives the prompt closing, is spelled out
    /// at the prompt and in the status bar, and re-runs the current query immediately
    /// so the effect of the switch is visible rather than deferred.
    fn toggle_search_mode(&mut self) {
        self.search_mode = match self.search_mode {
            SearchMode::Literal => SearchMode::Regex,
            SearchMode::Regex => SearchMode::Literal,
        };
        let label = match self.search_mode {
            SearchMode::Literal => "search: literal text",
            SearchMode::Regex => "search: regular expression",
        };
        let query = self.search.query().to_string();
        if query.is_empty() {
            self.notify(label, false);
        } else {
            self.run_search(&query);
            self.notify(label, false);
        }
    }

    /// Runs a search and jumps to the first match in the search's direction.
    ///
    /// The query is interpreted strictly according to [`App::search_mode`]. There is
    /// deliberately no silent fall back from a broken pattern to a literal search: a
    /// mode the user cannot predict is worse than an error message they can act on.
    pub fn run_search(&mut self, query: &str) {
        self.ensure_rendered();
        match Search::new(self.doc.source(), query, self.search_mode) {
            Ok(mut search) => {
                search.locate(self.doc.source(), self.cache.canvas().spans());
                self.search = search;
                self.search_index = None;
                if self.search.is_empty() {
                    if !query.is_empty() {
                        self.notify(format!("no match for `{query}`"), true);
                    }
                } else {
                    self.step_match(!self.search_backward);
                }
            }
            Err(error) => self.notify(error.to_string(), true),
        }
    }

    /// Moves to the next or previous match, wrapping around.
    fn step_match(&mut self, forward: bool) {
        self.ensure_rendered();
        if self.search.is_empty() {
            if !self.search.query().is_empty() {
                self.notify("no matches", true);
            }
            return;
        }
        let from = self.search_index.or_else(|| {
            if forward {
                self.search
                    .first_at_or_after(self.scroll, true)
                    .and_then(|index| index.checked_sub(1))
            } else {
                self.search
                    .last_at_or_before(self.scroll, true)
                    .map(|index| index + 1)
            }
        });
        let Some(index) = self.search.step(from, forward) else {
            return;
        };
        self.search_index = Some(index);
        // The first segment is the match's beginning; a hit the renderer wrapped across a
        // line break is reached by its start, which is where the reader's eye goes.
        if let Some(segment) = self.search.hits()[index].segments.first().copied() {
            self.reveal_centered(segment.row);
            self.reveal_columns(segment.row, segment.col, segment.cols);
        }
        // No notice: the status bar already carries `⌕ query n/m` on its right-hand
        // side, and saying the same thing twice thirty columns apart (usability P7)
        // costs the room the help hint needs.
    }

    /// Recomputes the filtered table of contents, keeping the selection in range.
    fn refilter_toc(&mut self) {
        self.toc_hits = self.toc.filter(&self.toc_filter);
        self.toc_cursor = self.toc_cursor.min(self.toc_hits.len().saturating_sub(1));
    }

    /// The text currently in the table-of-contents filter.
    pub fn toc_filter(&self) -> &str {
        &self.toc_filter
    }

    /// Handles a mouse wheel notch. Positive `delta` scrolls down.
    pub fn on_scroll(&mut self, delta: isize, in_toc: bool) {
        if in_toc && self.toc_open {
            let step = isize::try_from(self.config.scroll_step).unwrap_or(1);
            let action = if delta > 0 {
                Action::LineDown
            } else {
                Action::LineUp
            };
            self.toc_move(action, self.viewport_height(), step.unsigned_abs());
        } else {
            self.scroll_by(delta * isize::try_from(self.config.scroll_step).unwrap_or(1));
        }
    }

    /// Handles a click at `row` within the table-of-contents pane's list area.
    ///
    /// `first_visible` is the index into [`App::toc_hits`] drawn on the pane's first
    /// row, which the drawing code owns.
    pub fn on_toc_click(&mut self, first_visible: usize, row: usize) {
        let index = first_visible.saturating_add(row);
        if index >= self.toc_hits.len() {
            return;
        }
        self.focus = Focus::Toc;
        self.toc_cursor = index;
        self.jump_to_selected_heading();
    }

    /// Starts the table-of-contents filter prompt.
    pub fn start_toc_filter(&mut self) {
        self.open_prompt(PromptKind::TocFilter);
    }
}

/// Mouse text selection. See [`super::select`] for what it maps onto and why.
impl App {
    /// The selection currently up, if any.
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// The canvas position drawn at document-area column `x`, row `y`.
    ///
    /// Goes through the same [`Offsets`] the painter uses, so a selection can never
    /// disagree with the pixels about which source a cell came from — including a row
    /// with a pinned line-number gutter, where the arithmetic is not a single offset.
    fn canvas_pos(&self, x: u16, y: u16) -> Pos {
        let offsets = Offsets::scrolled_to(
            self.reach(),
            self.pinned(),
            self.hscroll,
            self.viewport_width(),
        );
        let last = self.cache.canvas().height().saturating_sub(1);
        let row = self.scroll.saturating_add(usize::from(y)).min(last);
        let width = self.cache.canvas().width().saturating_sub(1);
        let col = u16::try_from(offsets.column(row, x))
            .unwrap_or(u16::MAX)
            .min(width);
        Pos::new(row, col)
    }

    /// Begins a drag at document-area column `x`, row `y`.
    pub fn begin_selection(&mut self, x: u16, y: u16) {
        self.ensure_rendered();
        self.clear_notice();
        self.selection = Some(Selection::started(self.canvas_pos(x, y)));
    }

    /// Extends the drag to document-area column `x`, row `y`.
    ///
    /// A drag that leaves the top or bottom of the viewport scrolls it by a row, which
    /// is the only way to select more than a screenful. The selection itself is stored
    /// in canvas coordinates, so the scroll moves the window and not the highlight.
    pub fn drag_selection(&mut self, x: u16, y: u16) {
        let Some(mut selection) = self.selection.filter(|s| s.is_dragging()) else {
            return;
        };
        let height = self.viewport_height();
        if y == 0 {
            self.scroll_by(-1);
        } else if usize::from(y) + 1 >= height {
            self.scroll_by(1);
        }
        selection.drag_to(self.canvas_pos(x, y));
        self.selection = Some(selection);
    }

    /// Ends the drag and queues what it selected for the clipboard.
    ///
    /// A drag that never left the cell it started on is a click, not a selection, and
    /// leaves nothing behind: copying one character is not what anybody meant by it.
    ///
    /// A selection that is not being dragged cannot be ended, exactly as
    /// [`App::drag_selection`] cannot extend one. A *finished* selection stays up as a
    /// highlight until something replaces it, so without the filter every later release
    /// would re-extract it and put it on the clipboard again — over whatever that release
    /// had actually just done. That became reachable when a release started activating
    /// controls (Task 5); it was latent before.
    pub fn end_selection(&mut self) {
        let Some(mut selection) = self.selection.filter(|it| it.is_dragging()) else {
            return;
        };
        selection.finish();
        if selection.is_click() {
            self.selection = None;
            return;
        }
        self.selection = Some(selection);
        self.ensure_rendered();
        self.pending_copy =
            super::select::extract(self.cache.canvas(), self.doc.source(), selection);
        if self.pending_copy.is_none() {
            self.notify("nothing to copy there", false);
        }
    }

    /// Takes the text a finished drag produced, for the event loop to copy.
    pub fn take_pending_copy(&mut self) -> Option<Extract> {
        self.pending_copy.take()
    }

    /// Reports the outcome of a copy in the status bar.
    ///
    /// The wording comes from the [`Delivery`](super::clipboard::Delivery) itself,
    /// because how much a given route lets the pager claim is a property of the route.
    pub fn report_copy(
        &mut self,
        bytes: usize,
        copied: crate::tui::clipboard::Copied,
        delivery: &crate::tui::clipboard::Delivery,
    ) {
        let (text, is_error) = delivery.message(bytes, copied);
        self.notify(text, is_error);
    }
}

/// The copy buttons. See `render::button` for what draws them and design spec §5 for
/// what they carry.
impl App {
    /// The control under a pointer at document-area column `x`, row `y`, if any.
    ///
    /// The pointer goes through [`App::canvas_pos`] — the very translation
    /// [`App::begin_selection`] uses, and nothing else — so a press and a drag can never
    /// disagree about which cell is under the hand. A second opinion here would be a
    /// button that answers one cell to the side of the one it is drawn in, on exactly
    /// the rows where the arithmetic is hardest to check by eye.
    ///
    /// # A hotspot the viewport cut off
    ///
    /// A block too wide even for [`crate::render::document`]'s widening is clipped, and
    /// the clip now takes the claim with the cells it cuts
    /// ([`Canvas::truncate_width`](crate::canvas::Canvas::truncate_width), since Task
    /// 2b), so there is no hotspot left to reach. It is belt and braces either way: the
    /// horizontal scroll only reaches as far as the canvas is *drawn*, and `canvas_pos`
    /// clamps anything beyond it to the last cell that exists, which is never inside a
    /// button — a button sits at least two columns inside its own block's right edge.
    /// That is asserted by
    /// [`super::tests::a_button_clipped_off_the_canvas_cannot_be_pressed_anywhere`],
    /// which sweeps every cell at every offset for one.
    fn hotspot_at(&self, x: u16, y: u16) -> Option<&crate::canvas::Hotspot> {
        self.cache
            .canvas()
            .hotspots()
            .get(self.hotspot_index_at(x, y)?)
    }

    /// The same hit test, answering *which* control rather than handing one over.
    ///
    /// One implementation, so a press and a hover can never disagree about where the
    /// button is — the thing [`App::hotspot_at`]'s note about `canvas_pos` is careful
    /// about, one level further out.
    fn hotspot_index_at(&self, x: u16, y: u16) -> Option<usize> {
        // The popup covers the document, so the controls under it are covered too:
        // nothing beneath the box lights, presses or fires. One guard rather than three,
        // because a press, a release and a hover that disagreed about what the box hides
        // would be a link that highlights through a popup and then opens a browser from
        // a click the reader aimed at a footnote. Links drawn *inside* the popup are
        // inert for the same reason from the other side (design spec §1.1): the note's
        // own canvas is never consulted here at all.
        if self.popup_contains(x, y) {
            return None;
        }
        let pos = self.canvas_pos(x, y);
        self.cache.canvas().hotspots().iter().position(|spot| {
            spot.row == pos.row
                && pos.col >= spot.col
                && pos.col < spot.col.saturating_add(spot.cols)
        })
    }

    /// Records a press at document-area column `x`, row `y` as a click in flight.
    ///
    /// Nothing fires here. A click is a press *and a release* on the same control with no
    /// drag in between (design spec §3), so all a press does is remember which control it
    /// landed on; [`App::release_hotspot`] decides. A press on nothing remembers nothing.
    ///
    /// # What the answer means
    ///
    /// Reports whether the control **claimed the press outright**, which is to say
    /// whether the caller should skip [`App::begin_selection`]. Only a `[copy]` button
    /// does: it is chrome the renderer drew, there is no document text under it to
    /// select, and design spec §5 requires that pressing it never leave a selection
    /// behind. A link's cells *are* document text, so a press there both remembers the
    /// click and starts a drag — and then a drag cancels the click while a plain click
    /// leaves a one-cell selection that [`App::end_selection`] discards as a click. That
    /// is what "selection wins every tie" buys: dragging out of a link still selects.
    ///
    /// # A press that lands on a different control than the last one
    ///
    /// The candidate is replaced, never merged. Two presses without a release cannot
    /// happen from one mouse, but a caller that managed it would get the later one, which
    /// is the one the hand is on.
    ///
    /// # A press while a footnote popup is up
    ///
    /// Inside the box, the popup swallows it: no control fires, no selection starts, and
    /// the popup stays. Outside it, the popup is dismissed (design spec §6) and the press
    /// then does what it always did — the click is not eaten by the dismissal, because a
    /// reader who clicks a link beside an open note means to follow it.
    pub fn press_hotspot(&mut self, x: u16, y: u16) -> bool {
        self.ensure_rendered();
        if self.popup.is_some() {
            if self.popup_contains(x, y) {
                // Claimed outright, exactly as a `[copy]` button claims one: there is no
                // document text under the box to select.
                self.pressed = None;
                return true;
            }
            self.close_popup();
        }
        let Some(spot) = self.hotspot_at(x, y) else {
            self.pressed = None;
            return false;
        };
        let target = spot.target;
        let claims = matches!(spot.kind, HotspotKind::Copy { .. });
        self.pressed = Some(target);
        if claims {
            self.clear_notice();
        }
        claims
    }

    /// Ends a click in flight at document-area column `x`, row `y`.
    ///
    /// Fires — returns the [`Activation`] — only if the release landed on the *same
    /// control* the press did. "Same control" is the hotspot's
    /// [`target`](crate::canvas::Hotspot::target), not its position in the list: a link
    /// wrapped across rows is
    /// several hotspots sharing one target, and pressing its first row and releasing on
    /// its second is one click on one control. Releasing on a *different* link, or on no
    /// control at all, fires nothing.
    ///
    /// The candidate is taken whatever the answer: a release ends the gesture, and a
    /// release that missed must not leave a candidate behind for the next one to fire.
    ///
    /// The render comes **before** the take, and the order is load-bearing. A target id is
    /// issued per canvas, so [`App::ensure_rendered`] drops a click in flight along with
    /// the hover; taking first would capture the id out of the old canvas and then match
    /// it against the new one's, which are reused small integers. This way the guard
    /// enforces itself no matter which side the reflow lands on.
    pub fn release_hotspot(&mut self, x: u16, y: u16) -> Option<Activation> {
        self.ensure_rendered();
        let pressed = self.pressed.take()?;
        let spot = self.hotspot_at(x, y)?;
        if spot.target != pressed {
            return None;
        }
        Some(Activation {
            row: spot.row,
            col: spot.col,
            kind: spot.kind.clone(),
        })
    }

    /// Cancels a click in flight, permanently for this gesture.
    ///
    /// Called for every drag, because **any** drag means the hand went travelling and the
    /// gesture is a selection, not a click (design spec §3 — selection wins every tie).
    /// Cancellation is not suspension: moving off the control and back onto it does not
    /// resurrect the candidate, because the state it would need was thrown away here on
    /// the first drag event rather than merely compared against on release.
    pub fn cancel_hotspot_press(&mut self) {
        self.pressed = None;
    }

    /// Puts the pointer at document-area column `x`, row `y`.
    ///
    /// Returns whether the *hovered control changed identity* — not whether the pointer
    /// moved, which it did by definition. A terminal in any-event tracking mode reports
    /// motion cell by cell, and a pager that repainted on each of those would be
    /// re-laying-out the document for a hand sliding across a paragraph. Sweeping along
    /// one six-column label is one change on the way in and one on the way out; the
    /// four columns in between ask for nothing, and [`super::term`] draws nothing.
    pub fn set_pointer(&mut self, x: u16, y: u16) -> bool {
        self.ensure_rendered();
        let at = self.hotspot_index_at(x, y);
        std::mem::replace(&mut self.hover, at) != at
    }

    /// Takes the pointer off the document entirely.
    ///
    /// The pane, the scrollbar and the status bar are not places a control can be, and
    /// a pointer resting on one of them must not leave a button lit behind it. Reports
    /// a change on the same terms as [`App::set_pointer`], so leaving an already-empty
    /// document costs nothing.
    pub fn clear_pointer(&mut self) -> bool {
        self.hover.take().is_some()
    }

    /// The control under the pointer, as an index into `canvas.hotspots()`.
    ///
    /// Read by [`super::draw`], which is where hover is applied: a hovered button is a
    /// *painted* difference, not a rendered one. Rendering is a pure function of
    /// `(AST, width, theme, options)` and the pointer is none of those — the same
    /// argument that keeps the `[copied]` flash out of the renderer.
    pub fn hovered(&self) -> Option<usize> {
        self.hover
    }

    /// The control the keyboard cursor is on, as a
    /// [`Hotspot::target`](crate::canvas::Hotspot).
    ///
    /// Read by [`super::draw`] the same way [`App::hovered`] is: the cursor is a
    /// painted difference, not a rendered one (design spec §4 — the keyboard cursor is
    /// paint-time, exactly like hover).
    pub fn cursor_target(&self) -> Option<usize> {
        self.cursor
    }

    /// Every control on screen, once each, in the order the renderer drew them.
    ///
    /// A control is a `target`, not a hotspot: a wrapped link or one wrapped inside a
    /// centred table cell is several hotspots sharing one, and `f`/`F` must land on
    /// each control once, not once per row it happens to span (design spec §2.2, §4).
    fn control_targets(&self) -> Vec<usize> {
        let mut targets = Vec::new();
        for spot in self.cache.canvas().hotspots() {
            if !targets.contains(&spot.target) {
                targets.push(spot.target);
            }
        }
        targets
    }

    /// Moves the keyboard cursor to the next (`forward`) or previous control on
    /// screen, wrapping at either end.
    ///
    /// This is what makes every link reachable without a mouse (design spec §4): a
    /// `[copy]` button hides when mouse capture was refused because a control nobody
    /// can click is worse than none, but a link is content, not chrome, and hiding it
    /// would hide the document. The cursor is what resolves that for links instead of
    /// repealing the rule for buttons — a button the cursor lands on still only exists
    /// in the canvas when `copy_button` let the renderer draw it.
    fn cursor_step(&mut self, forward: bool) {
        self.ensure_rendered();
        let targets = self.control_targets();
        if targets.is_empty() {
            self.cursor = None;
            return;
        }
        let at = self
            .cursor
            .and_then(|target| targets.iter().position(|&t| t == target));
        let last = targets.len() - 1;
        let next = match (at, forward) {
            (Some(index), true) => {
                if index == last {
                    0
                } else {
                    index + 1
                }
            }
            (Some(index), false) => {
                if index == 0 {
                    last
                } else {
                    index - 1
                }
            }
            (None, true) => 0,
            (None, false) => last,
        };
        self.cursor = Some(targets[next]);
    }

    /// Builds the [`Activation`] an `enter` on the keyboard cursor fires, if the
    /// cursor is on a control.
    ///
    /// One hit test with [`App::release_hotspot`]'s counterpart: both resolve a
    /// target to the first hotspot that carries it, because every hotspot sharing a
    /// target carries the same `kind` — what activating the control does does not
    /// depend on which row of it fired.
    fn activate_cursor(&mut self) -> Option<Activation> {
        self.ensure_rendered();
        let target = self.cursor?;
        let spot = self
            .cache
            .canvas()
            .hotspots()
            .iter()
            .find(|spot| spot.target == target)?;
        Some(Activation {
            row: spot.row,
            col: spot.col,
            kind: spot.kind.clone(),
        })
    }

    /// The footnote popup that is up, if any.
    ///
    /// Read by [`super::draw`], which paints it, and by [`super::term`], which routes
    /// the wheel to whichever of the note and the document the pointer is over.
    pub fn popup(&self) -> Option<&Popup> {
        self.popup.as_ref()
    }

    /// Whether document-area cell `(x, y)` is covered by the popup.
    pub fn popup_contains(&self, x: u16, y: u16) -> bool {
        self.popup
            .as_ref()
            .is_some_and(|popup| popup.contains(x, y))
    }

    /// Scrolls the note inside the popup by `delta` rows, clamped at both ends.
    ///
    /// **The note moves; the document does not.** A wheel notch over the popup that
    /// scrolled the page would scroll the marker out from under the box and, by the
    /// dismissal rule, close the very note the reader was trying to read.
    pub fn scroll_popup(&mut self, delta: isize) {
        if let Some(popup) = self.popup.as_mut() {
            popup.scroll_by(delta);
        }
    }

    /// Puts the popup away, for a click that landed outside it.
    ///
    /// No notice: a click elsewhere is the reader getting on with something, and a
    /// status bar that announced it would be saying what they can already see. `Esc`
    /// does say so, because there the box is all that press did.
    fn close_popup(&mut self) {
        self.popup = None;
    }

    /// Records that the control at canvas `(row, col)` was just used.
    pub fn flash_copied(&mut self, row: usize, col: u16) {
        self.copied_flash = Some((row, col, std::time::Instant::now()));
    }

    /// The control still showing its flash, if the flash has not expired.
    ///
    /// Read by [`super::draw`] on every frame; the event loop redraws on a timer
    /// regardless, so the label comes back on the first tick past the deadline without
    /// anything having been scheduled.
    pub fn copied_flash(&self) -> Option<(usize, u16)> {
        self.copied_flash
            .filter(|(_, _, at)| at.elapsed() < std::time::Duration::from_millis(FLASH_FOR))
            .map(|(row, col, _)| (row, col))
    }
}
