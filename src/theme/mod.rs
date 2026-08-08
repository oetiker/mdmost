//! Palette and semantic style lookup.
//!
//! A [`Theme`] is a plain data structure: a [`Palette`] of raw colours plus a set of
//! *semantic* [`Style`] slots grouped by the part of the UI they describe. Renderers
//! never pick colours themselves — they ask the theme for the semantic slot they need.
//!
//! ```
//! use mdless::theme::Theme;
//!
//! let theme = Theme::default_dark();
//! let h1 = theme.heading(1);
//! assert!(h1.fg.is_some());
//! ```

mod builtin;
mod style;

pub use style::{Attributes, Color, Style};

use crate::error::ThemeError;

/// The raw colours a theme is built from.
///
/// Semantic slots are derived from the palette by the built-in theme constructors, and
/// may be overridden individually by user configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// Page background.
    pub bg: Color,
    /// Slightly raised background (code blocks, table headers).
    pub surface: Color,
    /// Further raised background (status bar, selections).
    pub overlay: Color,
    /// Primary body text.
    pub fg: Color,
    /// Secondary text (captions, line numbers).
    pub muted: Color,
    /// Borders and rules.
    pub border: Color,
    /// Primary accent.
    pub accent: Color,
    /// Red / error hue.
    pub red: Color,
    /// Orange hue.
    pub orange: Color,
    /// Yellow / warning hue.
    pub yellow: Color,
    /// Green / success hue.
    pub green: Color,
    /// Cyan hue.
    pub cyan: Color,
    /// Blue hue.
    pub blue: Color,
    /// Purple hue.
    pub purple: Color,
    /// Magenta hue.
    pub magenta: Color,
}

impl Palette {
    /// The rotating accent hues, used for diagram series, chart bars and nesting levels.
    pub fn accents(&self) -> [Color; 6] {
        [
            self.accent,
            self.green,
            self.orange,
            self.purple,
            self.cyan,
            self.magenta,
        ]
    }
}

/// Styles for inline text spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyles {
    /// Ordinary body text.
    pub body: Style,
    /// `*emphasis*`.
    pub emphasis: Style,
    /// `**strong**`.
    pub strong: Style,
    /// `~~strikethrough~~`.
    pub strikethrough: Style,
    /// Link text.
    pub link: Style,
    /// The link target shown next to or below the link text.
    pub link_url: Style,
    /// `` `inline code` ``.
    pub code: Style,
    /// Footnote reference markers.
    pub footnote_ref: Style,
    /// Alt text inside an image placeholder.
    pub image_alt: Style,
    /// Dimmed / de-emphasised text used for captions and hints.
    pub dim: Style,
}

/// Styles for block-level constructs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockStyles {
    /// The vertical bar drawn to the left of a block quote.
    pub quote_bar: Style,
    /// Text inside a block quote.
    pub quote_text: Style,
    /// Bullet / ordinal markers of list items.
    pub list_marker: Style,
    /// The checkbox of a checked task list item.
    pub task_checked: Style,
    /// The checkbox of an unchecked task list item.
    pub task_unchecked: Style,
    /// Thematic break (`---`).
    pub rule: Style,
    /// The rule drawn beneath H1 and H2.
    pub heading_rule: Style,
    /// The glyph prefix in front of a heading.
    pub heading_prefix: Style,
    /// The footnote definition label.
    pub footnote_label: Style,
    /// Caption text under images, diagrams and degraded blocks.
    pub caption: Style,
    /// Border of the placeholder box that stands in for an image.
    pub image_border: Style,
}

/// Styles for fenced code blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeStyles {
    /// Default code text (also the fallback for unknown languages).
    pub text: Style,
    /// Background applied to the whole code block area.
    pub background: Style,
    /// The frame around a code block.
    pub frame: Style,
    /// The language name drawn into the frame's top edge.
    pub language: Style,
    /// Optional line numbers in the gutter.
    pub line_number: Style,
    /// Marker shown when a code line is horizontally truncated.
    pub overflow_marker: Style,
    /// Keywords, as mapped from the syntax highlighter.
    pub keyword: Style,
    /// String literals.
    pub string: Style,
    /// Numeric literals.
    pub number: Style,
    /// Comments.
    pub comment: Style,
    /// Function and method names.
    pub function: Style,
    /// Type names.
    pub type_name: Style,
    /// Variables and identifiers.
    pub variable: Style,
    /// Constants and enum members.
    pub constant: Style,
    /// Operators.
    pub operator: Style,
    /// Attributes, annotations and preprocessor directives.
    pub attribute: Style,
    /// Text the highlighter flagged as invalid.
    pub invalid: Style,
    /// Macro and preprocessor-macro names.
    ///
    /// Separate from [`CodeStyles::function`] because a macro invocation is a
    /// different kind of event from a call, and Rust-heavy documents show many of both.
    pub macro_name: Style,
    /// Brackets, separators and terminators.
    ///
    /// Quieter than [`CodeStyles::operator`]: an operator says what the code *does*,
    /// a bracket only says where it starts.
    pub punctuation: Style,
    /// Namespace, module and package names.
    ///
    /// A paler relative of [`CodeStyles::type_name`], since a namespace names a
    /// container rather than a value's type.
    pub namespace: Style,
    /// Escape sequences inside string literals.
    ///
    /// Deliberately breaks out of [`CodeStyles::string`]: `\n` is code, not text.
    pub escape: Style,
}

/// Styles for tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableStyles {
    /// Box-drawing glyphs of the table frame.
    pub border: Style,
    /// Header row text.
    pub header: Style,
    /// Ordinary cell text.
    pub cell: Style,
    /// Background applied to every second body row, for banding.
    pub row_alt: Style,
    /// Marker shown when the table is horizontally scrolled.
    pub overflow_marker: Style,
}

/// Styles for Mermaid diagrams.
///
/// The diagram engines share this slot set; a family-specific slot (for example
/// [`DiagramStyles::milestone`]) is simply unused by families that have no such concept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagramStyles {
    /// Edge and connector lines, including junction glyphs.
    pub line: Style,
    /// Arrowheads and other edge terminators.
    pub arrow: Style,
    /// Node / box borders.
    pub node_border: Style,
    /// Text inside a node.
    pub node_text: Style,
    /// Border of a subgraph, composite state or block frame.
    pub group_border: Style,
    /// Title of a subgraph, composite state or block frame.
    pub group_title: Style,
    /// Labels attached to edges.
    pub edge_label: Style,
    /// Notes (`Note over …`, `note left of …`).
    pub note: Style,
    /// Sequence-diagram lifelines.
    pub lifeline: Style,
    /// Sequence-diagram activation bars.
    pub activation: Style,
    /// Class / entity compartment separators and attribute rows.
    pub compartment: Style,
    /// Stereotype annotations such as `<<interface>>`.
    pub stereotype: Style,
    /// Diagram title.
    pub title: Style,
    /// Axis lines and tick labels (gantt, pie legend).
    pub axis: Style,
    /// Legend text.
    pub legend: Style,
    /// A completed gantt task.
    pub task_done: Style,
    /// An active gantt task.
    pub task_active: Style,
    /// A critical gantt task.
    pub task_crit: Style,
    /// A gantt milestone diamond.
    pub milestone: Style,
}

/// Styles for chrome: status bar, TOC pane, help overlay and search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiStyles {
    /// The status bar background and default text.
    pub status_bar: Style,
    /// Emphasised segment of the status bar (file name).
    pub status_accent: Style,
    /// Key hints in the status bar and help overlay.
    pub status_key: Style,
    /// Scrollbar track.
    pub scrollbar_track: Style,
    /// Scrollbar thumb.
    pub scrollbar_thumb: Style,
    /// Border between the TOC pane and the document.
    pub toc_border: Style,
    /// An ordinary TOC entry.
    pub toc_item: Style,
    /// The TOC entry for the section currently in view.
    pub toc_active: Style,
    /// Characters of a TOC entry matched by the fuzzy filter.
    pub toc_match: Style,
    /// Help overlay background and border.
    pub help_border: Style,
    /// Help overlay section titles.
    pub help_title: Style,
    /// Ordinary search matches.
    pub search_match: Style,
    /// The search match the cursor is on.
    pub search_current: Style,
    /// Prompt line for search input.
    pub prompt: Style,
    /// Error messages shown in the UI.
    pub error: Style,
    /// Warning messages shown in the UI.
    pub warning: Style,
}

/// A complete theme: palette plus every semantic style slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// The theme's name, as used by `--theme` and the config file.
    pub name: String,
    /// Whether the theme is designed for a dark terminal background.
    pub is_dark: bool,
    /// Raw colours.
    pub palette: Palette,
    /// Style of headings, indexed by level 1..=6.
    ///
    /// One hue family that dims with depth, so a deeper heading recedes instead of
    /// competing with the one above it. Read it through [`Theme::heading`].
    pub headings: [Style; 6],
    /// Style of the glyph in front of a heading, indexed by level 1..=6.
    ///
    /// Derived from the heading's own colour, so the marker encodes the level it
    /// belongs to. Read it through [`Theme::heading_prefix`].
    pub heading_prefixes: [Style; 6],
    /// Style of the rule drawn beneath a heading, indexed by level 1..=6.
    ///
    /// Also derived from the heading's colour, and deliberately no fainter than body
    /// text: a rule under the signature heading must not be the least visible thing on
    /// the line. Read it through [`Theme::heading_rule`].
    pub heading_rules: [Style; 6],
    /// Inline text styles.
    pub text: TextStyles,
    /// Block-level styles.
    pub block: BlockStyles,
    /// Code block styles.
    pub code: CodeStyles,
    /// Table styles.
    pub table: TableStyles,
    /// Diagram styles.
    pub diagram: DiagramStyles,
    /// Chrome styles.
    pub ui: UiStyles,
}

impl Theme {
    /// The names of every built-in theme.
    pub fn builtin_names() -> &'static [&'static str] {
        &["dark", "light"]
    }

    /// Looks up a built-in theme by name.
    ///
    /// # Errors
    ///
    /// Returns [`ThemeError::UnknownTheme`] if no built-in theme has that name.
    pub fn builtin(name: &str) -> Result<Self, ThemeError> {
        match name {
            "dark" => Ok(Self::default_dark()),
            "light" => Ok(Self::default_light()),
            other => Err(ThemeError::UnknownTheme(other.to_string())),
        }
    }

    /// Derives a complete theme from a palette.
    ///
    /// This is the single implementation of palette-to-semantic-style derivation, and
    /// is what user-defined themes in `config.toml` are built from. `is_dark` selects
    /// the contrast direction used when shading derived styles.
    pub fn from_palette(name: &str, is_dark: bool, palette: Palette) -> Self {
        builtin::from_palette(name, is_dark, palette)
    }

    /// The signature dark theme.
    pub fn default_dark() -> Self {
        builtin::dark()
    }

    /// The built-in light theme.
    pub fn default_light() -> Self {
        builtin::light()
    }

    /// The style for a heading of the given level.
    ///
    /// Levels outside `1..=6` are clamped, so callers never need to validate.
    pub fn heading(&self, level: u8) -> Style {
        self.headings[Self::level_index(level)]
    }

    /// The style for the prefix glyph of a heading of the given level.
    ///
    /// A tint of that level's own heading colour, one shade quieter than the text, so
    /// the marker announces the level instead of being one fixed accent everywhere.
    /// Levels outside `1..=6` are clamped.
    pub fn heading_prefix(&self, level: u8) -> Style {
        self.heading_prefixes[Self::level_index(level)]
    }

    /// The style for the rule drawn beneath a heading of the given level.
    ///
    /// Levels outside `1..=6` are clamped; see [`Theme::heading_has_rule`] for whether
    /// a rule is drawn at all.
    pub fn heading_rule(&self, level: u8) -> Style {
        self.heading_rules[Self::level_index(level)]
    }

    /// Whether a rule should be drawn beneath a heading of this level.
    pub fn heading_has_rule(&self, level: u8) -> bool {
        level <= 2
    }

    /// The zero-based index of a heading level, clamped into `1..=6`.
    fn level_index(level: u8) -> usize {
        usize::from(level.clamp(1, 6)) - 1
    }

    /// A rotating accent style, for diagram series, chart bars and nesting depth.
    ///
    /// The index wraps, so any depth or series number is valid.
    pub fn accent(&self, index: usize) -> Style {
        let accents = self.palette.accents();
        Style::new().fg(accents[index % accents.len()])
    }

    /// The style to use as the canvas background fill.
    ///
    /// Carries both [`Palette::fg`] and [`Palette::bg`], so anything drawn with it
    /// paints the theme's own background rather than inheriting the terminal's. Every
    /// surface the theme owns — the viewport, the TOC pane, overlays — must be filled
    /// with this (or with [`Theme::background`]) on every frame, or the page reads as
    /// islands of theme floating in whatever colour the terminal happens to be.
    pub fn base(&self) -> Style {
        self.text.body
    }

    /// A pure background fill: [`Palette::bg`] with no foreground and no attributes.
    ///
    /// Use this to wash an area whose text colour is set elsewhere; use
    /// [`Theme::base`] when the area also needs the default text colour.
    pub fn background(&self) -> Style {
        Style::new().bg(self.palette.bg)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_lookup_round_trips() {
        for name in Theme::builtin_names() {
            let theme = Theme::builtin(name).expect("built-in theme resolves");
            assert_eq!(&theme.name, name);
        }
        assert!(Theme::builtin("nope").is_err());
    }

    #[test]
    fn heading_levels_are_clamped_and_distinct() {
        let theme = Theme::default_dark();
        assert_eq!(theme.heading(0), theme.heading(1));
        assert_eq!(theme.heading(9), theme.heading(6));
        assert_ne!(theme.heading(1), theme.heading(3));
    }

    #[test]
    fn accent_index_wraps() {
        let theme = Theme::default_dark();
        assert_eq!(theme.accent(0), theme.accent(6));
    }

    #[test]
    fn dark_and_light_differ_in_polarity() {
        let dark = Theme::default_dark();
        let light = Theme::default_light();
        assert!(dark.is_dark && !light.is_dark);
        assert!(dark.palette.bg.luminance() < light.palette.bg.luminance());
    }

    #[test]
    fn every_theme_defines_a_background_and_foreground() {
        for name in Theme::builtin_names() {
            let theme = Theme::builtin(name).expect("built-in theme resolves");
            assert!(
                theme.text.body.fg.is_some(),
                "{name}: body needs a foreground"
            );
            assert!(
                theme.text.body.bg.is_some(),
                "{name}: body needs a background"
            );
        }
    }

    /// The token slots the syntax highlighter maps scopes onto.
    ///
    /// Every one of them must be visibly different from every other, in every theme:
    /// a slot that collapses onto its neighbour is a distinction the highlighter makes
    /// and the reader cannot see.
    fn token_slots(theme: &Theme) -> [(&'static str, Style); 16] {
        let c = &theme.code;
        [
            ("text", c.text),
            ("keyword", c.keyword),
            ("string", c.string),
            ("number", c.number),
            ("comment", c.comment),
            ("function", c.function),
            ("type_name", c.type_name),
            ("variable", c.variable),
            ("constant", c.constant),
            ("operator", c.operator),
            ("attribute", c.attribute),
            ("invalid", c.invalid),
            ("macro_name", c.macro_name),
            ("punctuation", c.punctuation),
            ("namespace", c.namespace),
            ("escape", c.escape),
        ]
    }

    #[test]
    fn code_token_slots_are_all_distinct_in_every_theme() {
        for name in Theme::builtin_names() {
            let theme = Theme::builtin(name).expect("built-in theme resolves");
            let slots = token_slots(&theme);
            for (i, (left_name, left)) in slots.iter().enumerate() {
                for (right_name, right) in &slots[i + 1..] {
                    assert_ne!(
                        left, right,
                        "{name}: code slots {left_name} and {right_name} are identical"
                    );
                }
            }
        }
    }

    /// Distinctness is necessary but not sufficient: two slots one RGB step apart pass
    /// the equality test and are invisible on a terminal. Require a real gap.
    #[test]
    fn code_token_slots_differ_perceptibly_in_every_theme() {
        for name in Theme::builtin_names() {
            let theme = Theme::builtin(name).expect("built-in theme resolves");
            let slots = token_slots(&theme);
            for (i, (left_name, left)) in slots.iter().enumerate() {
                for (right_name, right) in &slots[i + 1..] {
                    // Slots that differ in attributes (comment is italic, invalid is
                    // underlined) are already distinguishable without a colour gap.
                    if left.attrs != right.attrs {
                        continue;
                    }
                    let (Some(a), Some(b)) = (left.fg, right.fg) else {
                        continue;
                    };
                    let distance = u32::from(a.r.abs_diff(b.r))
                        + u32::from(a.g.abs_diff(b.g))
                        + u32::from(a.b.abs_diff(b.b));
                    assert!(
                        distance >= 24,
                        "{name}: {left_name} and {right_name} are only {distance} apart"
                    );
                }
            }
        }
    }

    /// A user theme built from a bare palette must get the same derived slots as a
    /// built-in one, since both go through [`Theme::from_palette`].
    #[test]
    fn derived_slots_survive_a_user_defined_palette() {
        let base = Theme::default_dark();
        let custom = Theme::from_palette("custom", true, base.palette.clone());
        assert_eq!(custom.code, base.code);
        assert_ne!(custom.code.number, custom.code.constant);
        assert_ne!(custom.code.variable, custom.code.text);
        assert_ne!(custom.code.escape, custom.code.string);
        assert_ne!(custom.code.punctuation, custom.code.operator);
    }
}
