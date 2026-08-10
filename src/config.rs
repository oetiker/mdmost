//! Configuration: TOML loading and merging, themes, key bindings.
//!
//! The file lives at `~/.config/mdmost/config.toml` (or wherever the platform's
//! configuration directory is; see [`Config::default_path`]).
//!
//! Per design spec §12 a configuration problem **never** prevents startup. Loading
//! therefore returns a [`Loaded`] value carrying both a usable [`Config`] and the list
//! of [`ConfigError`]s the caller should report. Problems are as local as possible: a
//! single unusable key binding costs you that binding, not the whole file.
//!
//! ```
//! use mdmost::config::Config;
//!
//! let loaded = Config::parse_str("theme = \"light\"\n", std::path::Path::new("x.toml"));
//! assert!(loaded.problems.is_empty());
//! assert_eq!(loaded.config.theme, "light");
//! ```

mod keys;
mod write;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{ConfigError, ThemeError};
use crate::theme::{Color, Theme};

pub use keys::{Action, ActionGroup, Key, KeyBindings, KeyCode, KeyMods};

/// The default width of the table-of-contents pane, in columns.
pub const DEFAULT_TOC_WIDTH: u16 = 30;

/// The narrowest and widest the table-of-contents pane may be configured.
const TOC_WIDTH_RANGE: std::ops::RangeInclusive<u16> = 12..=80;

/// The default cap on the width of the document body, in columns.
///
/// Seventy-two columns sits inside the band typography calls readable — a measure of
/// roughly 45–90 characters — rather than at the top of it. The eye finds the start of
/// the next line by how far it has to travel back, and on a wide terminal that journey
/// is the whole complaint. This caps prose on an 80-column terminal too, not only on
/// the wide ones; tables, diagrams and long code lines are exempt from the cap and are
/// unaffected.
pub const DEFAULT_BODY_WIDTH: u16 = 72;

/// The narrowest and widest the body may be capped at.
///
/// Below twenty columns there is no prose left to read, and above a thousand the cap
/// is not capping anything. `0` is not in the range because it is the spelling of
/// "no cap at all", handled before the range is consulted.
pub const BODY_WIDTH_RANGE: std::ops::RangeInclusive<u16> = 20..=1000;

/// The effective configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The name of the theme to start in.
    pub theme: String,
    /// Whether Nerd Font glyphs may be used, when the reader has said.
    ///
    /// `None` — the default — means nobody has said, and the answer is detected instead
    /// (see [`crate::nerdfont`]), falling back to plain Unicode whenever it cannot be
    /// established. `icons = true` or `icons = false` in the config file settles it
    /// without detection, and `--icons` / `--no-icons` override even that.
    ///
    /// This is deliberately tri-state: `Some(false)` and "unset" both draw plain
    /// Unicode today, but only the first must keep doing so on a machine where the
    /// glyphs would in fact render.
    pub icons: Option<bool>,
    /// Whether fenced code blocks are drawn with a line-number gutter.
    pub line_numbers: bool,
    /// Whether a document titled by a lone `#` heading opens with a `FIGlet` banner.
    ///
    /// **Off by default**, and deliberately so: a banner is a decoration the reader did
    /// not ask for, applied to somebody else's document. A title set in art is a strong
    /// opinion about how a document should open, and a default is the wrong place to
    /// hold one — so this is the key you turn *on*, not the one you turn off. Turned on
    /// it applies only where it fits: the document's first block must be its one and
    /// only level-1 heading, and a title too wide for the measure is wrapped between
    /// words, declining only when a single word will not fit.
    pub title_banner: bool,
    /// Whether a deeply nested document gets section numbers (design spec §9.3).
    ///
    /// On by default, unlike `title_banner`: numbering is navigation rather than
    /// decoration, and it applies itself only where it earns its keep — a document has to
    /// use three or more distinct heading levels below its title before a single digit
    /// is drawn, so the flat documents most people read are untouched. What the key is
    /// for is the reader who would rather see the author's headings and nothing else,
    /// and the writer checking their document against a renderer that adds nothing.
    pub section_numbers: bool,
    /// Whether the table-of-contents pane starts open.
    pub toc_open: bool,
    /// The width of the table-of-contents pane, in columns.
    pub toc_width: u16,
    /// Whether the mouse wheel scrolls, the scrollbar drags, clicks select in the TOC, and
    /// dragging over the document selects text and copies its Markdown source.
    ///
    /// Off by default, but no longer for the reason it originally was. Capturing the
    /// mouse does take the terminal's own drag-select away, and selecting text is the
    /// main thing anyone does with a read-only viewer that is not scrolling it — so the
    /// pager now does the selecting itself, and gives back more than it took: a drag
    /// copies the **Markdown source** behind the cells rather than the rendered glyphs
    /// (see [`tui::select`](crate::tui::select)). Dragging over `◆ Wide diagram` yields
    /// `# Wide diagram`.
    ///
    /// What is left of the original reason, and why this stays off by default: the
    /// terminal's own selection is the one the reader already knows, it survives the
    /// pager exiting, and it is the one their muscle memory and their window manager
    /// agree about. `less` does not capture either. Turn it on with `--mouse` or
    /// `mouse = true`.
    pub mouse: bool,
    /// How many document lines one mouse-wheel notch scrolls.
    pub scroll_step: u16,
    /// The widest the document body is laid out, however wide the terminal is.
    ///
    /// `None` means no cap: the body uses the whole terminal, as it always did.
    /// Otherwise prose is laid out at this many columns and centred, while blocks that
    /// cannot be reflowed take the full width — see [`crate::tui::wide`], which owns
    /// the rule and is the only place it is applied.
    ///
    /// Not to be confused with `--width`, which changes the width the document is
    /// rendered at altogether. This is a cap on the body within that width.
    pub body_width: Option<u16>,
    /// The key table.
    pub keys: KeyBindings,
    /// Themes defined in the configuration file, by name.
    pub themes: BTreeMap<String, Theme>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            icons: None,
            line_numbers: false,
            title_banner: false,
            section_numbers: true,
            toc_open: false,
            toc_width: DEFAULT_TOC_WIDTH,
            mouse: false,
            scroll_step: 3,
            body_width: Some(DEFAULT_BODY_WIDTH),
            keys: KeyBindings::defaults(),
            themes: BTreeMap::new(),
        }
    }
}

/// The outcome of loading configuration: a usable config plus anything that went wrong.
#[derive(Debug)]
pub struct Loaded {
    /// The configuration to use. Always usable, even when `problems` is non-empty.
    pub config: Config,
    /// Problems to report to the user before the pager starts.
    pub problems: Vec<ConfigError>,
    /// The file the configuration was read from, if any.
    pub path: Option<PathBuf>,
}

impl Loaded {
    /// The built-in defaults, with no file involved.
    pub fn defaults() -> Self {
        Self {
            config: Config::default(),
            problems: Vec::new(),
            path: None,
        }
    }
}

impl Config {
    /// The path configuration is read from when none is given on the command line.
    ///
    /// Returns `None` when the platform has no home directory to speak of.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "mdmost")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Loads the configuration from [`Config::default_path`].
    ///
    /// A missing file is not a problem here: nobody asked for it, so it yields the
    /// defaults silently. An explicitly named file is a different matter — see
    /// [`Config::load_from`].
    pub fn load() -> Loaded {
        match Self::default_path() {
            Some(path) if path.exists() => Self::load_from(&path),
            _ => Loaded::defaults(),
        }
    }

    /// Loads the configuration from an explicit path.
    ///
    /// A missing file yields defaults plus a [`ConfigError::Read`] problem, because a
    /// file the user named and that is not there is a typo, not an optional extra
    /// (usability review P10).
    pub fn load_from(path: &Path) -> Loaded {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse_str(&text, path),
            Err(source) => Loaded {
                config: Config::default(),
                problems: vec![ConfigError::Read {
                    path: path.to_path_buf(),
                    source,
                }],
                path: Some(path.to_path_buf()),
            },
        }
    }

    /// Parses configuration from TOML text.
    ///
    /// `path` is used only for error messages.
    pub fn parse_str(text: &str, path: &Path) -> Loaded {
        let mut problems = Vec::new();
        let give_up = |problems: Vec<ConfigError>| Loaded {
            config: Config::default(),
            problems,
            path: Some(path.to_path_buf()),
        };

        // The file is read as a plain table first, and keys this version does not know
        // are dropped with a problem each, before it is deserialised into `RawConfig`.
        //
        // Deserialising straight into `RawConfig` is stricter than it looks: its
        // `deny_unknown_fields` makes one unrecognised key fail the *whole* parse, so a
        // single typo silently cost the reader their theme, their icon setting and every
        // key binding they had written — which is the opposite of what this function
        // promises, and of what it already does for an unknown action or an unknown
        // theme name. Dropping the offending key and keeping the rest makes the three
        // cases behave alike.
        let Ok(table) = text.parse::<toml::Table>() else {
            // Only a syntax error can reach here; that really is unrecoverable, because
            // nothing after the broken line can be trusted to mean what it says.
            let error = text
                .parse::<toml::Table>()
                .expect_err("the parse just failed");
            return give_up(vec![toml_problem(text, path, &error)]);
        };
        let table = strip_unknown_keys(table, text, path, &mut problems);

        let raw: RawConfig = match table.try_into() {
            Ok(raw) => raw,
            Err(error) => {
                // A value of the wrong type, e.g. `scroll_step = "fast"`. The key is
                // known, so this is a genuine mistake about that key rather than a key
                // from a future version, and there is no sensible value to carry on with.
                problems.push(toml_problem(text, path, &error));
                return give_up(problems);
            }
        };
        let config = raw.into_config(text, path, &mut problems);
        Loaded {
            config,
            problems,
            path: Some(path.to_path_buf()),
        }
    }

    /// Resolves a theme by name, preferring configuration-defined themes.
    ///
    /// # Errors
    ///
    /// Returns [`ThemeError::UnknownTheme`] when neither the configuration nor the
    /// built-ins define a theme by that name.
    pub fn resolve_theme(&self, name: &str) -> Result<Theme, ThemeError> {
        match self.themes.get(name) {
            Some(theme) => Ok(theme.clone()),
            None => Theme::builtin(name),
        }
    }

    /// Every theme name that can be cycled through, configured themes last.
    ///
    /// Names are unique. A configuration theme may deliberately shadow a built-in by
    /// reusing its name — [`Config::resolve_theme`] prefers the configured one — and
    /// listing that name twice would stall the `t` cycle on it forever.
    pub fn theme_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Theme::builtin_names()
            .iter()
            .map(|name| (*name).to_string())
            .collect();
        for name in self.themes.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
    }

    /// The theme that follows `current` when the user cycles themes.
    pub fn next_theme_name(&self, current: &str) -> String {
        let names = self.theme_names();
        let index = names.iter().position(|name| name == current);
        match index {
            Some(index) => names[(index + 1) % names.len()].clone(),
            None => names.first().cloned().unwrap_or_else(|| "dark".to_string()),
        }
    }
}

/// The configuration file's shape, before validation.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    theme: Option<String>,
    icons: Option<bool>,
    line_numbers: Option<bool>,
    title_banner: Option<bool>,
    section_numbers: Option<bool>,
    mouse: Option<bool>,
    scroll_step: Option<u16>,
    body_width: Option<u16>,
    #[serde(default)]
    toc: RawToc,
    #[serde(default)]
    keys: BTreeMap<String, String>,
    #[serde(default)]
    themes: BTreeMap<String, RawTheme>,
}

/// The `[toc]` table.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawToc {
    open: Option<bool>,
    width: Option<u16>,
}

/// A `[themes.<name>]` table.
///
/// Missing colours are inherited from `base`, so a theme can be a two-line tweak of a
/// built-in rather than a full fifteen-colour palette.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTheme {
    base: Option<String>,
    dark: Option<bool>,
    bg: Option<String>,
    surface: Option<String>,
    overlay: Option<String>,
    fg: Option<String>,
    muted: Option<String>,
    border: Option<String>,
    accent: Option<String>,
    red: Option<String>,
    orange: Option<String>,
    yellow: Option<String>,
    green: Option<String>,
    cyan: Option<String>,
    blue: Option<String>,
    purple: Option<String>,
    magenta: Option<String>,
}

impl RawConfig {
    /// Validates the raw file into a [`Config`], collecting per-entry problems.
    fn into_config(self, text: &str, path: &Path, problems: &mut Vec<ConfigError>) -> Config {
        let mut config = Config {
            // Carried straight across as an `Option`, unlike every setting below it: an
            // absent `icons` key must stay absent so it reaches detection, rather than
            // being resolved here to a fixed answer.
            icons: self.icons,
            ..Config::default()
        };

        if let Some(line_numbers) = self.line_numbers {
            config.line_numbers = line_numbers;
        }
        if let Some(title_banner) = self.title_banner {
            config.title_banner = title_banner;
        }
        if let Some(section_numbers) = self.section_numbers {
            config.section_numbers = section_numbers;
        }
        if let Some(mouse) = self.mouse {
            config.mouse = mouse;
        }
        if let Some(step) = self.scroll_step {
            if step == 0 {
                problems.push(problem(text, path, "scroll_step", "must be at least 1"));
            } else {
                config.scroll_step = step;
            }
        }
        if let Some(cap) = self.body_width {
            // Zero is the spelling of "no cap", so that turning the cap off in a file
            // is a value like any other rather than a key you have to know to delete.
            if cap == 0 {
                config.body_width = None;
            } else if BODY_WIDTH_RANGE.contains(&cap) {
                config.body_width = Some(cap);
            } else {
                problems.push(problem(
                    text,
                    path,
                    "body_width",
                    &format!(
                        "body width must be 0 (no cap) or between {} and {}",
                        BODY_WIDTH_RANGE.start(),
                        BODY_WIDTH_RANGE.end()
                    ),
                ));
            }
        }
        if let Some(open) = self.toc.open {
            config.toc_open = open;
        }
        if let Some(width) = self.toc.width {
            if TOC_WIDTH_RANGE.contains(&width) {
                config.toc_width = width;
            } else {
                problems.push(problem(
                    text,
                    path,
                    "width",
                    &format!(
                        "table-of-contents width must be between {} and {}",
                        TOC_WIDTH_RANGE.start(),
                        TOC_WIDTH_RANGE.end()
                    ),
                ));
            }
        }

        config.themes = self.themes.into_config(text, path, problems);
        config.keys = merge_keys(self.keys, text, path, problems);

        if let Some(theme) = self.theme {
            if config.themes.contains_key(&theme) || Theme::builtin(&theme).is_ok() {
                config.theme = theme;
            } else {
                problems.push(problem(
                    text,
                    path,
                    "theme",
                    &format!("unknown theme `{theme}`, using `{}`", config.theme),
                ));
            }
        }

        config
    }
}

/// Turns `[themes.*]` tables into themes, skipping (and reporting) unusable ones.
trait IntoThemes {
    /// Validates every theme table.
    fn into_config(
        self,
        text: &str,
        path: &Path,
        problems: &mut Vec<ConfigError>,
    ) -> BTreeMap<String, Theme>;
}

impl IntoThemes for BTreeMap<String, RawTheme> {
    fn into_config(
        self,
        text: &str,
        path: &Path,
        problems: &mut Vec<ConfigError>,
    ) -> BTreeMap<String, Theme> {
        let mut themes = BTreeMap::new();
        for (name, raw) in self {
            let (theme, mut trouble) = raw.build(&name, text, path);
            problems.append(&mut trouble);
            themes.insert(name, theme);
        }
        themes
    }
}

impl RawTheme {
    /// Builds the theme, inheriting anything unspecified from its base.
    ///
    /// Problems are reported *per slot* and the theme is kept regardless. Design spec
    /// §9 sells a custom theme as a two-line tweak; throwing the whole thing away over
    /// one mistyped colour, and then reporting the theme as unknown (usability review
    /// P11), makes that promise false exactly when the user is learning the format.
    fn build(self, name: &str, text: &str, path: &Path) -> (Theme, Vec<ConfigError>) {
        let mut problems = Vec::new();
        let base_name = self.base.as_deref().unwrap_or("dark");
        let base = Theme::builtin(base_name).unwrap_or_else(|error| {
            problems.push(problem(
                text,
                path,
                "base",
                &format!("in theme `{name}`: {error}, using the dark theme"),
            ));
            Theme::default_dark()
        });
        let is_dark = self.dark.unwrap_or(base.is_dark);

        let mut palette = base.palette.clone();
        let slots: [(&str, Option<String>, &mut Color); 15] = [
            ("bg", self.bg, &mut palette.bg),
            ("surface", self.surface, &mut palette.surface),
            ("overlay", self.overlay, &mut palette.overlay),
            ("fg", self.fg, &mut palette.fg),
            ("muted", self.muted, &mut palette.muted),
            ("border", self.border, &mut palette.border),
            ("accent", self.accent, &mut palette.accent),
            ("red", self.red, &mut palette.red),
            ("orange", self.orange, &mut palette.orange),
            ("yellow", self.yellow, &mut palette.yellow),
            ("green", self.green, &mut palette.green),
            ("cyan", self.cyan, &mut palette.cyan),
            ("blue", self.blue, &mut palette.blue),
            ("purple", self.purple, &mut palette.purple),
            ("magenta", self.magenta, &mut palette.magenta),
        ];
        for (slot, value, target) in slots {
            let Some(value) = value else { continue };
            match Color::parse(&value) {
                Ok(color) => *target = color,
                // The slot keeps the base theme's colour, which is the smallest
                // correction that still leaves a usable theme.
                Err(error) => problems.push(problem(
                    text,
                    path,
                    slot,
                    &format!("in theme `{name}`: {error}, keeping the `{base_name}` colour"),
                )),
            }
        }
        (Theme::from_palette(name, is_dark, palette), problems)
    }
}

/// Applies the `[keys]` table on top of the defaults.
///
/// The value `"none"` removes a default binding, which is how a user gets rid of a
/// chord their terminal steals rather than having to redefine the whole table.
fn merge_keys(
    raw: BTreeMap<String, String>,
    text: &str,
    path: &Path,
    problems: &mut Vec<ConfigError>,
) -> KeyBindings {
    let mut bindings = KeyBindings::defaults();
    for (chord, action_name) in raw {
        let Some(key) = Key::parse(&chord) else {
            problems.push(problem(
                text,
                path,
                &chord,
                &format!("`{chord}` is not a key I recognise"),
            ));
            continue;
        };
        if action_name.eq_ignore_ascii_case("none") {
            bindings.unbind(&key);
            continue;
        }
        match Action::parse(&action_name) {
            Some(action) => bindings.bind(key, action),
            None => problems.push(problem(
                text,
                path,
                &chord,
                &format!("unknown action `{action_name}`"),
            )),
        }
    }
    bindings
}

/// The top-level keys this version understands.
const KNOWN_KEYS: &[&str] = &[
    "theme",
    "icons",
    "line_numbers",
    "title_banner",
    "section_numbers",
    "mouse",
    "scroll_step",
    "body_width",
    "toc",
    "keys",
    "themes",
];

/// The keys understood inside `[toc]`.
const KNOWN_TOC_KEYS: &[&str] = &["open", "width"];

/// Drops the keys this version does not understand, recording one problem for each.
///
/// Only the tables with a fixed shape are filtered: `[keys]` and `[themes.*]` accept any
/// key by design and do their own per-entry reporting.
fn strip_unknown_keys(
    mut table: toml::Table,
    text: &str,
    path: &Path,
    problems: &mut Vec<ConfigError>,
) -> toml::Table {
    if let Some(toml::Value::Table(toc)) = table.get_mut("toc") {
        retain_known(toc, KNOWN_TOC_KEYS, text, path, problems);
    }
    retain_known(&mut table, KNOWN_KEYS, text, path, problems);
    table
}

/// Removes every key of `table` outside `known`, reporting each by name and line.
fn retain_known(
    table: &mut toml::Table,
    known: &[&str],
    text: &str,
    path: &Path,
    problems: &mut Vec<ConfigError>,
) {
    let unknown: Vec<String> = table
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .cloned()
        .collect();
    for key in unknown {
        table.remove(&key);
        problems.push(problem(
            text,
            path,
            &key,
            &format!("unknown setting `{key}`, ignored — expected one of {}", {
                known
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }),
        ));
    }
}

/// Builds a problem, locating `key` in the source text so the message can name a line.
fn problem(text: &str, path: &Path, key: &str, message: &str) -> ConfigError {
    ConfigError::Parse {
        path: path.to_path_buf(),
        line: line_of_key(text, key).unwrap_or(1),
        key: Some(key.to_string()),
        message: message.to_string(),
    }
}

/// Converts a `toml` deserialisation failure into a located problem.
fn toml_problem(text: &str, path: &Path, error: &toml::de::Error) -> ConfigError {
    let line = error
        .span()
        .map(|span| line_of_offset(text, span.start))
        .unwrap_or(1);
    ConfigError::Parse {
        path: path.to_path_buf(),
        line,
        key: quoted_key(error.message()),
        message: first_line(error.message()).to_string(),
    }
}

/// The 1-based line containing `offset`.
fn line_of_offset(text: &str, offset: usize) -> usize {
    text.get(..offset.min(text.len()))
        .map(|head| head.bytes().filter(|byte| *byte == b'\n').count() + 1)
        .unwrap_or(1)
}

/// Finds the 1-based line on which `key` appears as a TOML key.
///
/// Matching is textual and therefore best-effort: the `toml` crate does not hand back
/// spans for values we validate ourselves, and a wrong-but-close line number is far
/// more useful to the reader than none at all.
fn line_of_key(text: &str, key: &str) -> Option<usize> {
    let quoted = format!("\"{key}\"");
    text.lines().enumerate().find_map(|(index, line)| {
        let trimmed = line.trim_start();
        let hit = trimmed.starts_with(&quoted)
            || (trimmed.starts_with(key) && trimmed[key.len()..].trim_start().starts_with('='))
            || trimmed.starts_with(&format!("[{key}"));
        hit.then_some(index + 1)
    })
}

/// The first line of a possibly multi-line diagnostic.
fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or(message).trim()
}

/// Extracts a backtick-quoted key from a `toml` diagnostic, when there is one.
fn quoted_key(message: &str) -> Option<String> {
    let start = message.find('`')? + 1;
    let rest = message.get(start..)?;
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}
