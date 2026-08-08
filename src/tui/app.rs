//! The pager's state machine.
//!
//! Everything here is pure state plus pure transitions: no `ratatui`, no `crossterm`,
//! no terminal. That separation is what makes the pager testable — every test in this
//! module drives the real application logic, and [`super::draw`] is left with nothing
//! but painting.

use crate::canvas::Canvas;
use crate::config::{Action, Config, Key, KeyCode};
use crate::doc::Doc;
use crate::render::{RenderOptions, render_document};
use crate::search::{Search, SearchMode};
use crate::theme::Theme;
use crate::toc::{FilterHit, Toc};

use super::cache::RenderCache;

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
    pub fn content_width(&self) -> u16 {
        // Leave one column for the scrollbar gutter.
        self.options.width.unwrap_or_else(|| {
            self.size
                .0
                .saturating_sub(self.toc_width())
                .saturating_sub(1)
                .max(1)
        })
    }

    /// The number of columns of document actually on screen.
    ///
    /// Equal to [`App::content_width`] unless `--width` forced a wider render, in which
    /// case the surplus is reached by scrolling horizontally.
    pub fn viewport_width(&self) -> u16 {
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
        self.toc.current(self.scroll)
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
                    render_document(&self.doc, width, &self.theme, &options)
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
    }

    /// Puts `row` at the top of the viewport, clamped.
    pub fn scroll_to(&mut self, row: usize) {
        self.ensure_rendered();
        self.scroll = row.min(self.max_scroll());
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
            // Any key other than an explicit re-toggle dismisses the help.
            match self.config.keys.action(&key) {
                Some(Action::Help | Action::Cancel) | None => self.overlay = Overlay::None,
                Some(Action::Quit) => self.quit(),
                Some(_) => self.overlay = Overlay::None,
            }
            return;
        }
        let Some(action) = self.config.keys.action(&key) else {
            return;
        };
        self.act(action);
    }

    /// Handles one action, dispatching on the focused pane.
    pub fn act(&mut self, action: Action) {
        let height = self.viewport_height();
        match action {
            Action::Quit => self.quit(),
            Action::Cancel => {
                if self.focus == Focus::Toc {
                    self.focus = Focus::Document;
                } else {
                    self.quit();
                }
            }
            Action::Help => self.overlay = Overlay::Help,
            Action::ToggleToc => self.toggle_toc(),
            Action::CycleTheme => self.cycle_theme(),
            // Design spec §9: `/` inside the table of contents filters it fuzzily
            // rather than searching the document.
            Action::SearchForward if self.focus == Focus::Toc => self.start_toc_filter(),
            Action::SearchForward => self.open_prompt(PromptKind::SearchForward),
            Action::SearchBackward => self.open_prompt(PromptKind::SearchBackward),
            Action::NextMatch => self.step_match(!self.search_backward),
            Action::PrevMatch => self.step_match(self.search_backward),
            Action::ToggleSearchMode => self.toggle_search_mode(),
            Action::Confirm if self.focus == Focus::Toc => self.jump_to_selected_heading(),
            Action::Confirm => {}
            Action::ScrollLeft => self.hscroll = self.hscroll.saturating_sub(4),
            Action::ScrollRight => {
                self.ensure_rendered();
                self.hscroll = self.hscroll.saturating_add(4);
                self.clamp();
            }
            Action::PrevHeading => self.step_heading(false),
            Action::NextHeading => self.step_heading(true),
            _ if self.focus == Focus::Toc => self.toc_move(action, height),
            Action::LineDown => self.scroll_by(1),
            Action::LineUp => self.scroll_by(-1),
            Action::HalfPageDown => self.scroll_by((height / 2).max(1) as isize),
            Action::HalfPageUp => self.scroll_by(-((height / 2).max(1) as isize)),
            Action::PageDown => self.scroll_by(height.saturating_sub(1).max(1) as isize),
            Action::PageUp => self.scroll_by(-(height.saturating_sub(1).max(1) as isize)),
            Action::Top => self.scroll_to(0),
            Action::Bottom => {
                self.ensure_rendered();
                self.scroll = self.max_scroll();
            }
        }
    }

    /// Moves the selection inside the table-of-contents pane.
    fn toc_move(&mut self, action: Action, height: usize) {
        let last = self.toc_hits.len().saturating_sub(1);
        let step = |cursor: usize, delta: isize| -> usize {
            if delta >= 0 {
                cursor.saturating_add(delta.unsigned_abs()).min(last)
            } else {
                cursor.saturating_sub(delta.unsigned_abs())
            }
        };
        self.toc_cursor = match action {
            Action::LineDown => step(self.toc_cursor, 1),
            Action::LineUp => step(self.toc_cursor, -1),
            Action::HalfPageDown => step(self.toc_cursor, (height / 2).max(1) as isize),
            Action::HalfPageUp => step(self.toc_cursor, -((height / 2).max(1) as isize)),
            Action::PageDown => step(self.toc_cursor, height.max(1) as isize),
            Action::PageUp => step(self.toc_cursor, -(height.max(1) as isize)),
            Action::Top => 0,
            Action::Bottom => last,
            _ => self.toc_cursor,
        };
    }

    /// Shows or hides the table-of-contents pane, moving focus with it.
    fn toggle_toc(&mut self) {
        self.toc_open = !self.toc_open;
        if self.toc_open {
            self.focus = Focus::Toc;
            self.sync_toc_selection();
        } else {
            self.focus = Focus::Document;
        }
        self.clamp();
    }

    /// Puts the selection on the section the viewport is currently in.
    fn sync_toc_selection(&mut self) {
        self.ensure_rendered();
        if let Some(current) = self.current_heading()
            && let Some(position) = self.toc_hits.iter().position(|hit| hit.index == current)
        {
            self.toc_cursor = position;
        }
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
            PromptKind::TocFilter => {
                self.toc_filter = input.to_string();
                self.refilter_toc();
                self.focus = Focus::Toc;
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
            self.reveal(row);
        }
        self.notify(format!("match {}/{}", index + 1, self.search.len()), false);
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
            for _ in 0..step.unsigned_abs() {
                self.toc_move(action, self.viewport_height());
            }
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
