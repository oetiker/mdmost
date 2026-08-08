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
        heading: ["◆", "◈", "▸", "▹", "•", "·"],
        bullets: ["•", "◦", "‣", "·"],
        task_checked: "☑",
        task_unchecked: "☐",
        code_icons: false,
    };

    /// Nerd Font glyphs, the default look.
    ///
    /// The code points are the classic Font Awesome 4 block that every Nerd Font
    /// patch carries, named in the comments so they can be checked against
    /// <https://www.nerdfonts.com/cheat-sheet>. Headings step down in visual weight
    /// exactly as the plain set does.
    pub const NERD: Self = Self {
        heading: [
            "\u{f0c8}", // nf-fa-square
            "\u{f096}", // nf-fa-square_o
            "\u{f111}", // nf-fa-circle
            "\u{f10c}", // nf-fa-circle_o
            "\u{f0da}", // nf-fa-caret_right
            "\u{f105}", // nf-fa-angle_right
        ],
        bullets: [
            "\u{f192}", // nf-fa-dot_circle_o
            "\u{f1db}", // nf-fa-circle_thin
            "\u{f0da}", // nf-fa-caret_right
            "\u{f105}", // nf-fa-angle_right
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
        Some(match language? {
            "rust" | "rs" => "\u{e7a8}",                   // nf-dev-rust
            "python" | "py" => "\u{e73c}",                 // nf-dev-python
            "javascript" | "js" | "jsx" => "\u{e74e}",     // nf-dev-javascript
            "typescript" | "ts" | "tsx" => "\u{e628}",     // nf-seti-typescript
            "go" => "\u{e724}",                            // nf-dev-go
            "java" => "\u{e738}",                          // nf-dev-java
            "ruby" | "rb" => "\u{e739}",                   // nf-dev-ruby
            "php" => "\u{e73d}",                           // nf-dev-php
            "html" => "\u{e736}",                          // nf-dev-html5
            "css" | "scss" | "sass" => "\u{e749}",         // nf-dev-css3
            "markdown" | "md" => "\u{e73e}",               // nf-dev-markdown
            "json" => "\u{e60b}",                          // nf-seti-json
            "yaml" | "yml" | "toml" | "ini" => "\u{e615}", // nf-seti-config
            "sql" | "postgres" | "mysql" => "\u{e706}",    // nf-dev-database
            "docker" | "dockerfile" => "\u{e7b0}",         // nf-dev-docker
            "git" | "diff" | "patch" => "\u{e702}",        // nf-dev-git
            "sh" | "bash" | "zsh" | "fish" | "shell" | "console" => "\u{e795}", // nf-dev-terminal
            "c" | "h" | "cpp" | "cc" | "hpp" | "cxx" => "\u{e61e}", // nf-custom-c
            _ => "\u{f121}",                               // nf-fa-code
        })
    }
}

impl Default for Glyphs {
    fn default() -> Self {
        Self::NERD
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::grapheme_width;

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
                assert_eq!(
                    grapheme_width(glyph),
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
