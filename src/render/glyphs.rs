//! The two glyph sets the renderer draws with.
//!
//! Design spec §9 asks for Nerd Font glyphs on heading bullets, list markers and
//! code-fence language icons, and for `--no-icons` / `icons = false` to substitute
//! plain Unicode. Both sets live here so the substitution is a table lookup rather
//! than a conditional at every draw site.
//!
//! # The width rule
//!
//! **Every glyph in both sets is exactly one display column**, and the two sets are
//! the same shape. Turning icons off therefore changes what a glyph looks like and
//! nothing about where anything sits; a test in this module asserts it for every
//! entry, so a badly chosen replacement fails the build rather than the layout.
//!
//! Box-drawing characters — frames, quote bars, rules, the overflow marker — are not
//! icons and are identical in both sets, so they are not listed here.
//!
//! # The disjointness rule
//!
//! The three marker families each own a distinct *shape* vocabulary, and no glyph is
//! ever shared between them:
//!
//! | family          | shape        | plain           | nerd                    |
//! |-----------------|--------------|-----------------|-------------------------|
//! | heading prefix  | angular      | diamonds, triangles, chevron | diamond, play, carets, angles |
//! | list bullet     | round        | dots and rings  | circles                 |
//! | task box        | square       | ballot boxes    | squares                 |
//!
//! A reader must never see one marker mean two things, so a test in this module
//! asserts the three families are pairwise disjoint in both sets.

/// The glyphs used for one rendering pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Glyphs {
    /// The prefix in front of a heading, indexed by level `1..=6`.
    pub heading: [&'static str; 6],
    /// The bullet of an unordered list item, indexed by nesting depth.
    pub bullets: [&'static str; 4],
    /// The box of a ticked task list item.
    pub task_checked: &'static str,
    /// The box of an unticked task list item.
    pub task_unchecked: &'static str,
    /// Whether a code fence shows a language icon in front of its name.
    pub code_icons: bool,
}

impl Glyphs {
    /// Plain Unicode, for terminals without a Nerd Font (`--no-icons`).
    pub const PLAIN: Self = Self {
        // Angular for headings (solid diamond → hollow diamond → solid triangle →
        // hollow triangle → chevron), round for bullets, boxes for tasks.
        heading: ["◆", "◈", "◇", "▸", "▹", "❯"],
        bullets: ["•", "◦", "⁃", "·"],
        task_checked: "☑",
        task_unchecked: "☐",
        code_icons: false,
    };

    /// Nerd Font glyphs, the default look.
    ///
    /// The code points are the classic Font Awesome 4 block that every Nerd Font
    /// patch carries, named in the comments so they can be checked against
    /// <https://www.nerdfonts.com/cheat-sheet>. Headings step down in visual weight
    /// exactly as the plain set does, and — as in the plain set — headings are
    /// angular, bullets are round and task boxes are square, so the three families
    /// never share a glyph.
    pub const NERD: Self = Self {
        heading: [
            "\u{f219}", // nf-fa-diamond
            "\u{f04b}", // nf-fa-play
            "\u{f0da}", // nf-fa-caret_right
            "\u{f054}", // nf-fa-chevron_right
            "\u{f101}", // nf-fa-angle_double_right
            "\u{f105}", // nf-fa-angle_right
        ],
        bullets: [
            "\u{f111}", // nf-fa-circle
            "\u{f192}", // nf-fa-dot_circle_o
            "\u{f10c}", // nf-fa-circle_o
            "\u{f1db}", // nf-fa-circle_thin
        ],
        task_checked: "\u{f046}",   // nf-fa-check_square_o
        task_unchecked: "\u{f096}", // nf-fa-square_o
        code_icons: true,
    };

    /// The set to use for the given `icons` setting.
    pub const fn new(icons: bool) -> Self {
        if icons { Self::NERD } else { Self::PLAIN }
    }

    /// The prefix glyph of a heading, for any level.
    pub fn heading(&self, level: u8) -> &'static str {
        self.heading[usize::from(level.clamp(1, 6)) - 1]
    }

    /// The bullet glyph at a nesting depth; the sequence repeats when nesting deepens.
    pub fn bullet(&self, depth: usize) -> &'static str {
        self.bullets[depth % self.bullets.len()]
    }

    /// The task box for a checked or unchecked item.
    pub fn task(&self, checked: bool) -> &'static str {
        if checked {
            self.task_checked
        } else {
            self.task_unchecked
        }
    }

    /// The icon shown in front of a code fence's language name.
    ///
    /// `None` when icons are off, or when the language has no icon of its own — the
    /// fence then shows its name alone, which is the plain-Unicode behaviour.
    pub fn language(&self, language: Option<&str>) -> Option<&'static str> {
        if !self.code_icons {
            return None;
        }
        let language = language?;
        Some(
            LANGUAGE_ICONS
                .iter()
                .find(|(names, _)| names.contains(&language))
                .map_or(GENERIC_LANGUAGE_ICON, |(_, icon)| *icon),
        )
    }

    /// Every Nerd Font glyph this type can draw.
    ///
    /// Whether the terminal has a font that can draw them is detected by asking whether
    /// an installed font covers all of them (see [`crate::nerdfont`]), so this iterator
    /// is what keeps detection honest: a glyph added to [`Self::NERD`] or to
    /// [`LANGUAGE_ICONS`] is a glyph the probe immediately starts requiring, with no
    /// second list that has to be remembered.
    pub fn nerd_glyphs() -> impl Iterator<Item = &'static str> {
        let set = Self::NERD;
        set.heading
            .into_iter()
            .chain(set.bullets)
            .chain([set.task_checked, set.task_unchecked])
            .chain(LANGUAGE_ICONS.iter().map(|(_, icon)| *icon))
            .chain([GENERIC_LANGUAGE_ICON])
    }
}

/// The code-fence icon for a language matching none of [`LANGUAGE_ICONS`].
const GENERIC_LANGUAGE_ICON: &str = "\u{f121}"; // nf-fa-code

/// The code-fence icon for each language, by the names a fence may use for it.
///
/// A table rather than a `match` so the icons can also be *enumerated*, which is what
/// [`Glyphs::nerd_glyphs`] needs. A `match` arm cannot be iterated, and the alternative
/// — a second hand-written list of the same code points — is exactly the kind of
/// duplicate enumeration this project has repeatedly watched drift out of step with the
/// copy that actually draws.
const LANGUAGE_ICONS: &[(&[&str], &str)] = &[
    (&["rust", "rs"], "\u{e7a8}"),                 // nf-dev-rust
    (&["python", "py"], "\u{e73c}"),               // nf-dev-python
    (&["javascript", "js", "jsx"], "\u{e74e}"),    // nf-dev-javascript
    (&["typescript", "ts", "tsx"], "\u{e628}"),    // nf-seti-typescript
    (&["go"], "\u{e724}"),                         // nf-dev-go
    (&["java"], "\u{e738}"),                       // nf-dev-java
    (&["ruby", "rb"], "\u{e739}"),                 // nf-dev-ruby
    (&["php"], "\u{e73d}"),                        // nf-dev-php
    (&["html"], "\u{e736}"),                       // nf-dev-html5
    (&["css", "scss", "sass"], "\u{e749}"),        // nf-dev-css3
    (&["markdown", "md"], "\u{e73e}"),             // nf-dev-markdown
    (&["json"], "\u{e60b}"),                       // nf-seti-json
    (&["yaml", "yml", "toml", "ini"], "\u{e615}"), // nf-seti-config
    (&["sql", "postgres", "mysql"], "\u{e706}"),   // nf-dev-database
    (&["docker", "dockerfile"], "\u{e7b0}"),       // nf-dev-docker
    (&["git", "diff", "patch"], "\u{e702}"),       // nf-dev-git
    (
        &["sh", "bash", "zsh", "fish", "shell", "console"],
        "\u{e795}", // nf-dev-terminal
    ),
    (&["c", "h", "cpp", "cc", "hpp", "cxx"], "\u{e61e}"), // nf-custom-c
];

impl Default for Glyphs {
    fn default() -> Self {
        Self::NERD
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::display_width;

    /// Every glyph either set can draw.
    fn all(set: Glyphs) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = set.heading.to_vec();
        out.extend(set.bullets);
        out.push(set.task_checked);
        out.push(set.task_unchecked);
        for language in [
            Some("rust"),
            Some("python"),
            Some("javascript"),
            Some("typescript"),
            Some("go"),
            Some("java"),
            Some("ruby"),
            Some("php"),
            Some("html"),
            Some("css"),
            Some("markdown"),
            Some("json"),
            Some("yaml"),
            Some("sql"),
            Some("docker"),
            Some("git"),
            Some("bash"),
            Some("c"),
            Some("nothing-in-particular"),
        ] {
            out.extend(set.language(language));
        }
        out
    }

    #[test]
    fn every_glyph_is_exactly_one_display_column() {
        for set in [Glyphs::PLAIN, Glyphs::NERD] {
            for glyph in all(set) {
                // `display_width`, not `grapheme_width`: the latter clamps to the
                // two columns a cell can hold, so a glyph that genuinely draws three
                // would pass a clamped assertion and then shift every column after it.
                assert_eq!(
                    display_width(glyph),
                    1,
                    "glyph {glyph:?} ({:04x}) must be one column, or turning icons \
                     off would shift the layout",
                    glyph.chars().next().map(u32::from).unwrap_or(0)
                );
                assert_eq!(
                    glyph.chars().count(),
                    1,
                    "glyph {glyph:?} must be a single code point"
                );
            }
        }
    }

    #[test]
    fn the_two_sets_have_the_same_shape() {
        assert_eq!(Glyphs::PLAIN.heading.len(), Glyphs::NERD.heading.len());
        assert_eq!(Glyphs::PLAIN.bullets.len(), Glyphs::NERD.bullets.len());
        assert_eq!(Glyphs::new(true), Glyphs::NERD);
        assert_eq!(Glyphs::new(false), Glyphs::PLAIN);
        assert_eq!(Glyphs::default(), Glyphs::NERD);
    }

    #[test]
    fn heading_levels_and_bullet_depths_are_total() {
        for set in [Glyphs::PLAIN, Glyphs::NERD] {
            for level in 0..=9u8 {
                assert_eq!(set.heading(level).chars().count(), 1);
            }
            for depth in 0..12usize {
                assert_eq!(set.bullet(depth), set.bullet(depth + set.bullets.len()));
            }
        }
    }

    /// A marker must mean exactly one thing. Heading prefixes, list bullets and task
    /// boxes are three separate vocabularies; sharing a glyph between them makes the
    /// same mark say "H5" in one place and "list item" in another.
    #[test]
    fn the_three_marker_families_are_disjoint() {
        for set in [Glyphs::PLAIN, Glyphs::NERD] {
            let families: [(&str, Vec<&'static str>); 3] = [
                ("heading", set.heading.to_vec()),
                ("bullets", set.bullets.to_vec()),
                ("tasks", vec![set.task_checked, set.task_unchecked]),
            ];
            for (i, (left_name, left)) in families.iter().enumerate() {
                for glyph in left {
                    assert_eq!(
                        left.iter().filter(|g| *g == glyph).count(),
                        1,
                        "{left_name} repeats {glyph:?}"
                    );
                }
                for (right_name, right) in &families[i + 1..] {
                    for glyph in left {
                        assert!(
                            !right.contains(glyph),
                            "{glyph:?} is used by both {left_name} and {right_name}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn language_icons_only_appear_when_icons_are_on() {
        assert!(Glyphs::PLAIN.language(Some("rust")).is_none());
        assert!(Glyphs::NERD.language(Some("rust")).is_some());
        assert!(Glyphs::NERD.language(None).is_none());
        assert_ne!(
            Glyphs::NERD.language(Some("rust")),
            Glyphs::NERD.language(Some("python"))
        );
    }
}
