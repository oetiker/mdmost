// SPDX-License-Identifier: MIT
//! The glyph set used by the chrome.
//!
//! Two sets exist and they are structurally identical, so nothing in the drawing code
//! needs to know which one is in force: turning icons off simply swaps Nerd Font glyphs
//! for plain Unicode of the same display width.
//!
//! Which set is in force is settled before drawing starts — by `--no-icons`, by
//! `MDMOST_ICONS`, by `icons` in the config file, or, if nobody has said, by
//! [`crate::nerdfont`] detecting whether a font that can draw them is installed.

/// The glyphs the status bar, table of contents and help overlay draw with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icons {
    /// Marks the file name in the status bar.
    pub file: &'static str,
    /// Marks the table-of-contents pane title.
    pub toc: &'static str,
    /// Marks the search state in the status bar.
    pub search: &'static str,
    /// Marks the current heading in the status bar.
    pub heading: &'static str,
    /// Marks the help overlay title.
    pub help: &'static str,
    /// Drawn in front of the selected table-of-contents entry.
    pub selected: &'static str,
    /// Drawn in front of an unselected table-of-contents entry.
    pub unselected: &'static str,
    /// Separates status-bar segments.
    pub separator: &'static str,
    /// Marks a warning or error notice.
    pub warning: &'static str,
    /// Marks the horizontal offset when content is scrolled sideways.
    pub horizontal: &'static str,
}

impl Icons {
    /// The Nerd Font glyph set.
    pub const NERD: Icons = Icons {
        file: "\u{f0219}",
        toc: "\u{f02d}",
        search: "\u{f002}",
        heading: "\u{f0f6}",
        help: "\u{f059}",
        // Deliberately not a Nerd Font glyph: the marker sits in a column the rest of
        // the pane aligns against, and private-use glyphs are ambiguous-width.
        selected: "\u{25b8}",
        unselected: " ",
        separator: "\u{e0b1}",
        warning: "\u{f071}",
        // Plain in both sets: an arrow that renders double-width in some terminals
        // would shift the whole status bar.
        horizontal: "\u{2194}",
    };

    /// The plain-Unicode fallback, for terminals without a Nerd Font.
    pub const PLAIN: Icons = Icons {
        file: "\u{25a4}",
        toc: "\u{2261}",
        search: "\u{2315}",
        heading: "\u{00a7}",
        help: "?",
        selected: "\u{25b8}",
        unselected: " ",
        separator: "\u{2502}",
        warning: "!",
        horizontal: "\u{2194}",
    };

    /// Picks a glyph set.
    pub fn new(nerd_font: bool) -> Self {
        if nerd_font { Self::NERD } else { Self::PLAIN }
    }

    /// Every glyph in this set, in field order.
    ///
    /// Enumerating the fields in one place is what lets the width rule be tested for the
    /// whole set rather than for whichever entries someone remembered to list.
    pub fn all(&self) -> [&'static str; 10] {
        [
            self.file,
            self.toc,
            self.search,
            self.heading,
            self.help,
            self.selected,
            self.unselected,
            self.separator,
            self.warning,
            self.horizontal,
        ]
    }

    /// The glyphs in this set that need a patched font.
    ///
    /// Those are the private-use code points: everything else is ordinary Unicode that
    /// any terminal font can be expected to draw. Nerd Font detection asks whether an
    /// installed font covers these (see [`crate::nerdfont`]), so listing them here — off
    /// the same fields the drawing code reads — is what stops the probe and the glyphs
    /// from drifting apart.
    pub fn nerd_glyphs() -> impl Iterator<Item = &'static str> {
        Self::NERD
            .all()
            .into_iter()
            .filter(|glyph| glyph.chars().any(is_private_use))
    }
}

/// Whether `ch` is in one of the Unicode private-use areas, which is where every
/// Nerd Font glyph lives.
pub(crate) fn is_private_use(ch: char) -> bool {
    matches!(ch, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::display_width;

    #[test]
    fn every_glyph_is_exactly_one_display_column() {
        // `chrome.rs` lays the status bar out by measuring its segments with
        // `display_width` and right-aligns from the end, so a single double-width glyph
        // here would shift every segment to its right. The renderer's own glyph table
        // has had this test from the start and it caught a real bug; the chrome's table
        // had none, which is the only reason this one is new rather than old.
        for set in [Icons::NERD, Icons::PLAIN] {
            for glyph in set.all() {
                assert_eq!(
                    display_width(glyph),
                    1,
                    "{glyph:?} draws {} columns, not 1",
                    display_width(glyph)
                );
                assert_eq!(
                    glyph.chars().count(),
                    1,
                    "{glyph:?} is more than one character"
                );
            }
        }
    }

    #[test]
    fn the_two_sets_are_the_same_shape() {
        // Turning icons off must change what a glyph looks like and nothing about where
        // anything sits.
        assert_eq!(Icons::NERD.all().len(), Icons::PLAIN.all().len());
        assert_eq!(Icons::new(true), Icons::NERD);
        assert_eq!(Icons::new(false), Icons::PLAIN);
    }

    #[test]
    fn only_the_nerd_set_needs_a_patched_font() {
        // The plain set is the fallback for terminals without one, so nothing in it may
        // be a private-use code point.
        for glyph in Icons::PLAIN.all() {
            assert!(
                !glyph.chars().any(is_private_use),
                "the plain fallback must not need a patched font, but has {glyph:?}"
            );
        }
        let nerd: Vec<&str> = Icons::nerd_glyphs().collect();
        assert!(
            !nerd.is_empty(),
            "the nerd set must have glyphs to detect, or detection tests nothing"
        );
        for glyph in &nerd {
            assert!(glyph.chars().any(is_private_use));
        }
    }
}
