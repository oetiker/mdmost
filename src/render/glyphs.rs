//! The two glyph sets the renderer draws with.
//!
//! Design spec §9 asks for Nerd Font glyphs on list markers, task boxes and code-fence
//! language icons, and for `--no-icons` / `icons = false` to substitute plain Unicode.
//! Both sets live here so the substitution is a table lookup rather than a conditional
//! at every draw site.
//!
//! # The width rule
//!
//! **Every glyph in both sets is exactly one display column**, and the two sets are
//! the same shape. Turning icons off therefore changes what a glyph looks like and
//! nothing about where anything sits; a test in this module asserts it for every
//! entry, so a badly chosen replacement fails the build rather than the layout.
//!
//! Box-drawing characters — frames, quote bars, heading rules, the overflow marker —
//! are not icons and are identical in both sets, so they are not listed here.
//!
//! # The disjointness rule
//!
//! The two marker families each own a distinct *shape* vocabulary, and no glyph is
//! ever shared between them:
//!
//! | family      | shape                              | plain             | nerd            |
//! |-------------|------------------------------------|-------------------|-----------------|
//! | list bullet | disc, dash, square — one per depth | `● – ▪ ▫`         | the same        |
//! | task box    | a box big enough to hold a tick    | ballot boxes      | squares         |
//!
//! A reader must never see one marker mean two things, so a test in this module
//! asserts the two families are disjoint in both sets.
//!
//! There used to be a third family, the prefix glyph in front of a heading. It was
//! **removed on 2026-08-09 at the owner's request** — "the special character before
//! the sectioning lines is a strange habit… nobody does that" — and the level a
//! heading belongs to is now carried by the rule *under* it (design spec §9).
//!
//! # Why bullets are not icons any more
//!
//! The bullet ladder is the same text in both sets, and that is deliberate rather than
//! an oversight. The size of the level-one bullet has been in question twice now, in
//! both directions, and plain Unicode is the set that has the grades to answer it:
//! `·`, `•` and `●` are three distinct sizes of the same mark, whereas every filled
//! circle a Nerd Font offers — `nf-fa-circle`, `nf-md-circle`, `nf-md-circle_medium` —
//! is a heavy disc drawn at icon size, one answer rather than a ladder. This also
//! keeps the two sets *identical* here,
//! which is the strongest possible form of the parity rule above, and it removes four
//! private-use code points from what [`Glyphs::nerd_glyphs`] makes font detection
//! demand. The theme already treats bullets this way: "bullets are punctuation, not
//! accents".

/// The bullet at each nesting depth, shared by both glyph sets.
///
/// A *shape* ladder rather than a ring of circles, changed on 2026-08-09 at the
/// owner's request: "the bullet character chosen is too prominent… then for the second
/// level maybe a heavy `-` and for the third a small square".
///
/// * `●` U+25CF — a filled disc. **Changed on 2026-08-09 at the owner's request**: the
///   previous `·` U+00B7 was too small to read as a marker. The owner picked `⦁`
///   U+2981 Z NOTATION SPOT from a rendered selection — bigger than `·`, smaller than
///   `●` — and it cannot ship: U+2981 is **absent from `CommitMono` Nerd Font** (and
///   from `DejaVu`; only `FreeSans`, STIX, Asana Math and `Noto`'s maths faces carry it), so
///   in the terminal this is written for it draws as a blank. Rasterised at a 200-unit
///   em, the four candidates measure `·` 32, `•` 48, `●` 108 in `CommitMono`, against
///   `·` 22, `•` 72, `⦁` 88, `●` 146 in `FreeSans` — so the owner's pick is 0.44 em and
///   `●` at 0.54 em is the closest thing the shipping font has to it (`•` is 0.24 em,
///   twice as far the other way, and was rejected once already as too prominent
///   before this ladder existed). This is the same defect that removed `◦` U+25E6:
///   **rasterise a candidate in the font before believing it exists.**
/// * `–` U+2013 — a dash with body. `‒` U+2012 and `−` U+2212 are indistinguishable
///   from it at terminal sizes, and `━` U+2501 is a full-cell bar that reads as a
///   rule rather than as a marker.
/// * `▪` U+25AA — a small filled square: a change of shape, not another round thing.
/// * `▫` U+25AB — the same square hollowed out; the faintest of the four, and so the
///   deepest. `◦` U+25E6, the previous depth-two bullet, is *absent* from at least one
///   popular patched font and draws as a blank there.
///
/// Each is a single code point of display width 1, which a test below enforces.
const BULLETS: [&str; 4] = ["●", "–", "▪", "▫"];

/// The glyphs used for one rendering pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Glyphs {
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
    ///
    /// The task boxes stay `☐` U+2610 and `☑` U+2611. `☒` U+2612 was weighed against
    /// them on 2026-08-09 — it is the box designed as `☐`'s pair, so it was the
    /// natural suspect for the size complaint — and the rasterisation says no: in
    /// `DejaVu` Sans, the fallback a terminal reaches for here (`CommitMono` Nerd Font
    /// carries *none* of the three), all three glyphs are the same 145×146 box at the
    /// same stroke weight, and by ink they run `☐` 27%, `☑` 34%, `☒` 42%. `☑` is
    /// already the closer partner; swapping in `☒` would make the ticked box heavier,
    /// not better matched. The mismatch the owner saw is in [`Self::NERD`].
    pub const PLAIN: Self = Self {
        bullets: BULLETS,
        task_checked: "☑",
        task_unchecked: "☐",
        code_icons: false,
    };

    /// Nerd Font glyphs, the default look where a Nerd Font is detected.
    ///
    /// The code points are named in the comments so they can be checked against
    /// <https://www.nerdfonts.com/cheat-sheet>. The bullets are deliberately the
    /// plain ones (see the module docs); what the icons buy here is the ticked task
    /// box and the code-fence language icons.
    ///
    /// # The task boxes are a *pair*, and were not
    ///
    /// The owner reported on 2026-08-09 that the unticked box looks larger than the
    /// ticked one, and it does. The two boxes used to be `nf-fa-square_o` U+F096 and
    /// `nf-fa-check_square_o` U+F046; rasterised in `CommitMono` Nerd Font at a 200-unit
    /// em they measure **120×121 and 97×97** — the same drawing at two different
    /// sizes, 24% apart in every dimension, sitting one under the other in the marker
    /// column where nothing hides it. Font Awesome 4 has no box that is both outlined
    /// and metrically equal to U+F096, so the pair moves to Material Design's
    /// `checkbox_blank_outline` / `checkbox_marked_outline`, which are drawn as a pair
    /// and measure **72×73 and 72×73**: one box, with and without a tick.
    ///
    /// The cost is that these are five-digit code points, added to Nerd Fonts in v3
    /// (2023). A v2 patch does not carry them, so [`Self::nerd_glyphs`] — which is
    /// what font detection demands coverage of — now answers "no Nerd Font" on a v2
    /// patch, and that reader gets [`Self::PLAIN`]. That is the safe direction of the
    /// detection rule (design spec §2.1: yes only on positive evidence) and it is the
    /// price of the boxes being the same size.
    pub const NERD: Self = Self {
        bullets: BULLETS,
        task_checked: "\u{f0135}",   // nf-md-checkbox_marked_outline
        task_unchecked: "\u{f0131}", // nf-md-checkbox_blank_outline
        code_icons: true,
    };

    /// The set to use for the given `icons` setting.
    pub const fn new(icons: bool) -> Self {
        if icons { Self::NERD } else { Self::PLAIN }
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
    ///
    /// The bullets are excluded because they are the same plain Unicode in both sets:
    /// they are not evidence of anything, and requiring them would make detection ask
    /// a question whose answer cannot change the render.
    pub fn nerd_glyphs() -> impl Iterator<Item = &'static str> {
        let set = Self::NERD;
        [set.task_checked, set.task_unchecked]
            .into_iter()
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
        let mut out: Vec<&'static str> = set.bullets.to_vec();
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
        assert_eq!(Glyphs::PLAIN.bullets.len(), Glyphs::NERD.bullets.len());
        // Identical, not merely parallel: see the module docs.
        assert_eq!(Glyphs::PLAIN.bullets, Glyphs::NERD.bullets);
        assert_eq!(Glyphs::new(true), Glyphs::NERD);
        assert_eq!(Glyphs::new(false), Glyphs::PLAIN);
        assert_eq!(Glyphs::default(), Glyphs::NERD);
    }

    #[test]
    fn bullet_depths_are_total() {
        for set in [Glyphs::PLAIN, Glyphs::NERD] {
            for depth in 0..12usize {
                assert_eq!(set.bullet(depth), set.bullet(depth + set.bullets.len()));
            }
        }
    }

    /// A marker must mean exactly one thing. List bullets and task boxes are two
    /// separate vocabularies; sharing a glyph between them makes the same mark say
    /// "unticked task" in one place and "list item" in another.
    #[test]
    fn the_marker_families_are_disjoint() {
        for set in [Glyphs::PLAIN, Glyphs::NERD] {
            let families: [(&str, Vec<&'static str>); 2] = [
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
