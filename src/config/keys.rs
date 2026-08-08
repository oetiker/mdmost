//! Key chords and the actions they trigger.
//!
//! Deliberately independent of `crossterm`: the design spec makes [`crate::tui`] the
//! only module allowed to depend on the terminal crates, so key handling can be
//! unit-tested without a terminal. The TUI converts a `crossterm::event::KeyEvent`
//! into a [`Key`] at its edge.

use std::collections::BTreeMap;
use std::fmt;

/// Everything the pager can be asked to do.
///
/// The variant names double as the identifiers accepted in the `[keys]` table of the
/// configuration file, in `snake_case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Action {
    /// Scroll one line down.
    LineDown,
    /// Scroll one line up.
    LineUp,
    /// Scroll half a screen down.
    HalfPageDown,
    /// Scroll half a screen up.
    HalfPageUp,
    /// Scroll a full screen down.
    PageDown,
    /// Scroll a full screen up.
    PageUp,
    /// Jump to the first line.
    Top,
    /// Jump to the last line.
    Bottom,
    /// Scroll one column left (wide tables and code blocks).
    ScrollLeft,
    /// Scroll one column right (wide tables and code blocks).
    ScrollRight,
    /// Start a forward search.
    SearchForward,
    /// Start a backward search.
    SearchBackward,
    /// Jump to the next match.
    NextMatch,
    /// Jump to the previous match.
    PrevMatch,
    /// Switch between literal and regular-expression searching.
    ToggleSearchMode,
    /// Show or hide the table-of-contents pane.
    ToggleToc,
    /// Confirm: jump to the selected heading, or accept the search prompt.
    Confirm,
    /// Jump to the previous heading.
    PrevHeading,
    /// Jump to the next heading.
    NextHeading,
    /// Switch to the next configured theme.
    CycleTheme,
    /// Show or hide the help overlay.
    Help,
    /// Leave the pager.
    Quit,
    /// Close the topmost overlay, or leave the pager when there is none.
    Cancel,
}

impl Action {
    /// Every action, in the order the help overlay presents them.
    pub const ALL: &'static [Action] = &[
        Action::LineDown,
        Action::LineUp,
        Action::HalfPageDown,
        Action::HalfPageUp,
        Action::PageDown,
        Action::PageUp,
        Action::Top,
        Action::Bottom,
        Action::ScrollLeft,
        Action::ScrollRight,
        Action::PrevHeading,
        Action::NextHeading,
        Action::SearchForward,
        Action::SearchBackward,
        Action::NextMatch,
        Action::PrevMatch,
        Action::ToggleSearchMode,
        Action::ToggleToc,
        Action::Confirm,
        Action::CycleTheme,
        Action::Help,
        Action::Cancel,
        Action::Quit,
    ];

    /// The `snake_case` identifier used in the configuration file.
    pub fn name(self) -> &'static str {
        match self {
            Action::LineDown => "line_down",
            Action::LineUp => "line_up",
            Action::HalfPageDown => "half_page_down",
            Action::HalfPageUp => "half_page_up",
            Action::PageDown => "page_down",
            Action::PageUp => "page_up",
            Action::Top => "top",
            Action::Bottom => "bottom",
            Action::ScrollLeft => "scroll_left",
            Action::ScrollRight => "scroll_right",
            Action::SearchForward => "search_forward",
            Action::SearchBackward => "search_backward",
            Action::NextMatch => "next_match",
            Action::PrevMatch => "prev_match",
            Action::ToggleSearchMode => "toggle_search_mode",
            Action::ToggleToc => "toggle_toc",
            Action::Confirm => "confirm",
            Action::PrevHeading => "prev_heading",
            Action::NextHeading => "next_heading",
            Action::CycleTheme => "cycle_theme",
            Action::Help => "help",
            Action::Quit => "quit",
            Action::Cancel => "cancel",
        }
    }

    /// The one-line description shown in the help overlay.
    pub fn description(self) -> &'static str {
        match self {
            Action::LineDown => "Scroll down one line",
            Action::LineUp => "Scroll up one line",
            Action::HalfPageDown => "Scroll down half a screen",
            Action::HalfPageUp => "Scroll up half a screen",
            Action::PageDown => "Scroll down one screen",
            Action::PageUp => "Scroll up one screen",
            Action::Top => "Go to the top of the document",
            Action::Bottom => "Go to the bottom of the document",
            Action::ScrollLeft => "Scroll left (wide tables and code)",
            Action::ScrollRight => "Scroll right (wide tables and code)",
            Action::SearchForward => "Search forward",
            Action::SearchBackward => "Search backward",
            Action::NextMatch => "Go to the next match",
            Action::PrevMatch => "Go to the previous match",
            Action::ToggleSearchMode => "Switch literal / regex search",
            Action::ToggleToc => "Show or hide the table of contents",
            Action::Confirm => "Jump to the selected heading",
            Action::PrevHeading => "Go to the previous heading",
            Action::NextHeading => "Go to the next heading",
            Action::CycleTheme => "Switch to the next theme",
            Action::Help => "Show or hide this help",
            Action::Quit => "Quit",
            Action::Cancel => "Close the overlay, or quit",
        }
    }

    /// The help section this action is grouped under.
    pub fn group(self) -> ActionGroup {
        match self {
            Action::LineDown
            | Action::LineUp
            | Action::HalfPageDown
            | Action::HalfPageUp
            | Action::PageDown
            | Action::PageUp
            | Action::Top
            | Action::Bottom
            | Action::ScrollLeft
            | Action::ScrollRight => ActionGroup::Movement,
            Action::PrevHeading | Action::NextHeading | Action::ToggleToc | Action::Confirm => {
                ActionGroup::Navigation
            }
            Action::SearchForward
            | Action::SearchBackward
            | Action::NextMatch
            | Action::PrevMatch
            | Action::ToggleSearchMode => ActionGroup::Search,
            Action::CycleTheme | Action::Help | Action::Quit | Action::Cancel => ActionGroup::View,
        }
    }

    /// Parses the `snake_case` identifier used in the configuration file.
    pub fn parse(name: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The heading an action appears under in the help overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionGroup {
    /// Scrolling.
    Movement,
    /// Structural navigation and panes.
    Navigation,
    /// Searching.
    Search,
    /// Appearance and lifecycle.
    View,
}

impl ActionGroup {
    /// Every group, in the order the help overlay presents them.
    pub const ALL: &'static [ActionGroup] = &[
        ActionGroup::Movement,
        ActionGroup::Navigation,
        ActionGroup::Search,
        ActionGroup::View,
    ];

    /// The title shown above the group.
    pub fn title(self) -> &'static str {
        match self {
            ActionGroup::Movement => "Movement",
            ActionGroup::Navigation => "Navigation",
            ActionGroup::Search => "Search",
            ActionGroup::View => "View",
        }
    }
}

/// The modifier keys held down alongside a [`KeyCode`].
///
/// `SHIFT` is only ever recorded for non-character keys: a shifted letter arrives as
/// the upper-case character itself, so recording both would make `G` unmatchable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct KeyMods(u8);

impl KeyMods {
    /// No modifiers.
    pub const NONE: Self = Self(0);
    /// The control key.
    pub const CTRL: Self = Self(1 << 0);
    /// The alt (meta) key.
    pub const ALT: Self = Self(1 << 1);
    /// The shift key.
    pub const SHIFT: Self = Self(1 << 2);

    /// Whether no modifier at all is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every modifier in `other` is set here.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The union of two modifier sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for KeyMods {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

/// A key, independent of any terminal library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum KeyCode {
    /// A printable character.
    Char(char),
    /// The return key.
    Enter,
    /// The escape key.
    Esc,
    /// The tab key.
    Tab,
    /// Shift-tab, which most terminals report as its own code.
    BackTab,
    /// The backspace key.
    Backspace,
    /// The delete key.
    Delete,
    /// The insert key.
    Insert,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// The home key.
    Home,
    /// The end key.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// A function key, `1..=12`.
    Function(u8),
}

/// A key together with its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    /// Which key was pressed.
    pub code: KeyCode,
    /// Which modifiers were held.
    pub mods: KeyMods,
}

impl Key {
    /// A key with no modifiers.
    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            mods: KeyMods::NONE,
        }
    }

    /// A character key with no modifiers.
    pub const fn char(ch: char) -> Self {
        Self::plain(KeyCode::Char(ch))
    }

    /// A character key held with control.
    pub const fn ctrl(ch: char) -> Self {
        Self {
            code: KeyCode::Char(ch),
            mods: KeyMods::CTRL,
        }
    }

    /// Parses a chord such as `ctrl-d`, `pgdn`, `space` or `G`.
    ///
    /// Modifier prefixes are separated by `-` or `+` and may appear in any order and
    /// any case. Returns `None` when the chord cannot be understood.
    pub fn parse(text: &str) -> Option<Key> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        // A lone separator is the key itself, not an empty modifier list.
        if text.chars().count() == 1 {
            return Some(Key::char(text.chars().next()?));
        }

        let mut mods = KeyMods::NONE;
        let mut rest = text;
        while let Some(cut) = rest.find(['-', '+']) {
            // Trailing separator: the remainder *is* the key (e.g. `ctrl--`).
            if cut + 1 >= rest.len() {
                break;
            }
            let (head, tail) = (&rest[..cut], &rest[cut + 1..]);
            let modifier = match head.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "c" => KeyMods::CTRL,
                "alt" | "meta" | "m" | "a" => KeyMods::ALT,
                "shift" | "s" => KeyMods::SHIFT,
                _ => break,
            };
            mods = mods.union(modifier);
            rest = tail;
        }

        let code = parse_code(rest)?;
        // Control chords are matched case-insensitively: terminals report `ctrl-D`
        // and `ctrl-d` identically, and users write both.
        let code = match (code, mods.contains(KeyMods::CTRL)) {
            (KeyCode::Char(ch), true) => KeyCode::Char(ch.to_ascii_lowercase()),
            (other, _) => other,
        };
        Some(Key { code, mods })
    }

    /// The chord in the canonical form [`Key::parse`] accepts.
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        if self.mods.contains(KeyMods::CTRL) {
            out.push_str("ctrl-");
        }
        if self.mods.contains(KeyMods::ALT) {
            out.push_str("alt-");
        }
        if self.mods.contains(KeyMods::SHIFT) {
            out.push_str("shift-");
        }
        out.push_str(&code_name(self.code));
        out
    }

    /// The chord as the help overlay and status bar show it.
    ///
    /// Uses arrow glyphs and title case, which reads better than the canonical form.
    pub fn label(&self) -> String {
        let base = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char(ch) => ch.to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "S-Tab".to_string(),
            KeyCode::Backspace => "Bksp".to_string(),
            KeyCode::Delete => "Del".to_string(),
            KeyCode::Insert => "Ins".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::Function(n) => format!("F{n}"),
        };
        let mut out = String::new();
        if self.mods.contains(KeyMods::CTRL) {
            out.push_str("Ctrl-");
        }
        if self.mods.contains(KeyMods::ALT) {
            out.push_str("Alt-");
        }
        if self.mods.contains(KeyMods::SHIFT) && !matches!(self.code, KeyCode::Char(_)) {
            out.push_str("Shift-");
        }
        out.push_str(&base);
        out
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

/// Parses the non-modifier part of a chord.
fn parse_code(text: &str) -> Option<KeyCode> {
    let mut chars = text.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        return Some(KeyCode::Char(ch));
    }
    let lower = text.to_ascii_lowercase();
    if let Some(digits) = lower.strip_prefix('f')
        && let Ok(n) = digits.parse::<u8>()
        && (1..=12).contains(&n)
    {
        return Some(KeyCode::Function(n));
    }
    Some(match lower.as_str() {
        "space" | "spc" => KeyCode::Char(' '),
        "enter" | "return" | "cr" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" | "shifttab" => KeyCode::BackTab,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pgup" | "pageup" => KeyCode::PageUp,
        "pgdn" | "pagedown" | "pgdown" => KeyCode::PageDown,
        _ => return None,
    })
}

/// The canonical spelling of a key code.
fn code_name(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(ch) => ch.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "backtab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pgup".to_string(),
        KeyCode::PageDown => "pgdn".to_string(),
        KeyCode::Function(n) => format!("f{n}"),
    }
}

/// The key-to-action table.
///
/// The map is ordered so that the help overlay, which is generated from this very
/// table, lists chords deterministically and can therefore never drift from the
/// bindings actually in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings {
    bindings: BTreeMap<Key, Action>,
}

impl KeyBindings {
    /// The bindings of design spec §10.
    pub fn defaults() -> Self {
        let mut bindings = BTreeMap::new();
        let table: &[(Key, Action)] = &[
            (Key::char('j'), Action::LineDown),
            (Key::plain(KeyCode::Down), Action::LineDown),
            (Key::char('k'), Action::LineUp),
            (Key::plain(KeyCode::Up), Action::LineUp),
            (Key::char('d'), Action::HalfPageDown),
            (Key::ctrl('d'), Action::HalfPageDown),
            (Key::char('u'), Action::HalfPageUp),
            (Key::ctrl('u'), Action::HalfPageUp),
            (Key::char(' '), Action::PageDown),
            (Key::plain(KeyCode::PageDown), Action::PageDown),
            (Key::ctrl('f'), Action::PageDown),
            (Key::char('b'), Action::PageUp),
            (Key::plain(KeyCode::PageUp), Action::PageUp),
            (Key::ctrl('b'), Action::PageUp),
            (Key::char('g'), Action::Top),
            (Key::plain(KeyCode::Home), Action::Top),
            (Key::char('G'), Action::Bottom),
            (Key::plain(KeyCode::End), Action::Bottom),
            (Key::plain(KeyCode::Left), Action::ScrollLeft),
            (Key::plain(KeyCode::Right), Action::ScrollRight),
            (Key::char('/'), Action::SearchForward),
            (Key::char('?'), Action::SearchBackward),
            (Key::char('n'), Action::NextMatch),
            (Key::char('N'), Action::PrevMatch),
            (Key::ctrl('r'), Action::ToggleSearchMode),
            (Key::plain(KeyCode::Tab), Action::ToggleToc),
            (Key::plain(KeyCode::Enter), Action::Confirm),
            (Key::char('t'), Action::CycleTheme),
            (Key::char('['), Action::PrevHeading),
            (Key::char(']'), Action::NextHeading),
            (Key::char('h'), Action::Help),
            (Key::plain(KeyCode::Function(1)), Action::Help),
            (Key::char('q'), Action::Quit),
            (Key::plain(KeyCode::Esc), Action::Cancel),
        ];
        for (key, action) in table {
            bindings.insert(*key, *action);
        }
        Self { bindings }
    }

    /// An empty table, for tests and for a configuration that starts from scratch.
    pub fn empty() -> Self {
        Self {
            bindings: BTreeMap::new(),
        }
    }

    /// Binds `key` to `action`, replacing any previous binding of that key.
    pub fn bind(&mut self, key: Key, action: Action) {
        self.bindings.insert(key, action);
    }

    /// Removes any binding of `key`.
    pub fn unbind(&mut self, key: &Key) {
        self.bindings.remove(key);
    }

    /// The action `key` triggers, if any.
    pub fn action(&self, key: &Key) -> Option<Action> {
        self.bindings.get(key).copied()
    }

    /// Every binding, in canonical key order.
    pub fn iter(&self) -> impl Iterator<Item = (Key, Action)> + '_ {
        self.bindings.iter().map(|(k, a)| (*k, *a))
    }

    /// Every key bound to `action`, in canonical key order.
    ///
    /// This is what makes the help overlay impossible to desynchronise: it reads the
    /// live table rather than a hand-written list.
    pub fn keys_for(&self, action: Action) -> Vec<Key> {
        self.bindings
            .iter()
            .filter(|(_, a)| **a == action)
            .map(|(k, _)| *k)
            .collect()
    }

    /// The number of bound keys.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether nothing is bound.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::defaults()
    }
}
