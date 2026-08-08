//! The pager's state machine.
//!
//! Everything here is pure state plus pure transitions: no `ratatui`, no `crossterm`,
//! no terminal. That separation is what makes the pager testable — every test in this
//! module drives the real application logic, and [`super::draw`] is left with nothing
//! but painting.

use crate::canvas::Canvas;
use crate::config::{Action, Config, Key, KeyCode};
use crate::doc::Doc;
use crate::render::RenderOptions;
use crate::search::{Search, SearchMode};
use crate::theme::Theme;
use crate::toc::{FilterHit, Toc};

use super::cache::RenderCache;

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

/// Whether `key` is a bare digit, and so part of a repeat count.
fn is_count_digit(key: Key) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch.is_ascii_digit()) && key.mods.is_empty()
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
    /// part of [`RenderOptions`], so `--no-icons` strips heading bullets, list markers
    /// and code-fence icons too, not merely the status bar.
    pub icons: bool,
    /// The theme to start in.
    pub theme: String,
    /// Whether the table-of-contents pane starts open.
    pub toc_open: bool,
    /// A forced render width (`--width`), independent of the terminal size.
    ///
    /// Content wider than the viewport is reached with the horizontal scroll keys, so
    /// forcing a width never hides anything.
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
    focus: Focus,
    toc_open: bool,
    toc_cursor: usize,
    toc_filter: String,
    toc_hits: Vec<FilterHit>,
    overlay: Overlay,
    /// The first help row on screen, for a help overlay taller than the terminal.
    help_scroll: usize,
    /// The digits typed in front of a movement key, `less`-style.
    pending_count: String,
    notice: Option<Notice>,
    search_index: Option<usize>,
    search_backward: bool,
    search_mode: SearchMode,
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
        let toc = Toc::from_doc(&doc);
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
            help_scroll: 0,
            pending_count: String::new(),
            notice,
            search_index: None,
            search_backward: false,
            search_mode: SearchMode::Literal,
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
    /// about horizontal position.
    pub fn hscroll_max(&self) -> u16 {
        self.cache
            .canvas()
            .width()
            .saturating_sub(self.viewport_width())
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
    /// Equal to [`App::content_width`] unless `--width` forced a wider render, in which
    /// case the surplus is reached by scrolling horizontally.
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
        let stale =
            self.cache
                .refresh(self.doc.version(), width, &self.theme.name, options, || {
                    super::wide::render_scrollable(&self.doc, width, &self.theme, &options)
                });
        if stale {
            self.toc.attach_anchors(self.cache.canvas().anchors());
            self.search
                .locate(self.doc.source(), self.cache.canvas().spans());
            self.clamp();
        }
    }

    /// Clamps the scroll offsets into range.
    fn clamp(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
        let overflow = self
            .cache
            .canvas()
            .width()
            .saturating_sub(self.viewport_width());
        self.hscroll = self.hscroll.min(overflow);
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
        self.ensure_rendered();
        self.scroll = row.min(self.max_scroll());
        self.track_toc();
    }

    /// Brings `row` into view, leaving a little context above it when scrolling up.
    pub fn reveal(&mut self, row: usize) {
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
    pub fn on_key(&mut self, key: Key) {
        self.clear_notice();
        if let Overlay::Prompt { kind, input } = &self.overlay {
            let (kind, input) = (*kind, input.clone());
            self.prompt_key(kind, input, key);
            return;
        }
        if self.overlay == Overlay::Help {
            self.help_key(key);
            return;
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
    }

    /// Handles one action, dispatching on the focused pane.
    pub fn act(&mut self, action: Action) {
        self.act_with_count(action, None);
    }

    /// Handles one action, repeated or parameterised by a leading count.
    pub fn act_with_count(&mut self, action: Action, count: Option<usize>) {
        let height = self.viewport_height();
        let times = count.unwrap_or(1).max(1);
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
            Action::Confirm => {}
            Action::ScrollLeft => {
                self.hscroll = self
                    .hscroll
                    .saturating_sub(HSCROLL_STEP.saturating_mul(u16::try_from(times).unwrap_or(1)));
            }
            Action::ScrollRight => {
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
            Action::Top => self.scroll_to(count.map_or(0, |line| line.saturating_sub(1))),
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
        if let Some(row) = self.search.hits()[index].row() {
            self.reveal_centered(row);
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
