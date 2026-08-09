//! The two glyph sets the renderer draws with.
//!
//! Design spec §9 asks for Nerd Font glyphs on list markers, task boxes and code-fence
//! language icons, and for `--no-icons` / `icons = false` to substitute plain Unicode.
//! Both sets live here so the substitution is a table lookup rather than a conditional
//! at every draw site.
//!
//! # The width rule
//!
//! **Every glyph occupies the same display width in both sets**, and the two sets are
//! the same shape. Turning icons off therefore changes what a glyph looks like and
//! nothing about where anything sits; a test in this module asserts it for every entry,
//! so a badly chosen replacement fails the build rather than the layout.
//!
//! Single-cell glyphs — the bullets, the language icons — are additionally asserted to
//! be exactly one column, which is what keeps the canvas's cell arithmetic simple. The
//! task box is three columns (`[ ]`), the same three in both sets.
//!
//! This rule was briefly weakened, on 2026-08-09, to admit a task box that was *drawn*
//! two cells wide while measuring one — the Nerd Fonts patch draws its icon ranges at
//! double advance, and `unicode-width` has no data for private-use code points. The
//! layout had to be told the true width by hand, and two long-standing parity tests
//! were weakened to accommodate it. Replacing that pictograph with `[ ]` and `[x]`
//! (see [`TASK_BOXES`]) deleted the discrepancy, the hand-carried reservation and the
//! exception together, and the two tests are back on the full corpus. **A glyph whose
//! drawn width and measured width disagree will do this to you again; prefer one where
//! they cannot.**
//!
//! Box-drawing characters — frames, quote bars, heading rules, the overflow marker —
//! are not icons and are identical in both sets, so they are not listed here.
//!
//! # The disjointness rule
//!
//! The two marker families each own a distinct *shape* vocabulary, and no glyph is
//! ever shared between them:
//!
//! | family      | shape                            | plain       | nerd     |
//! |-------------|----------------------------------|-------------|----------|
//! | list bullet | ASCII punctuation, one per depth | `* > + -`   | the same |
//! | task box    | ASCII brackets around the state  | `[ ]` `[x]` | the same |
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
//! The bullet ladder is **ASCII, unconditionally**, and it is the same text in both
//! sets — deliberately, not by oversight. There is no reason for a bullet to vary with
//! font detection when the entire point of the choice is that it renders everywhere;
//! that the Nerd set no longer differs from the plain one here is the intended
//! outcome, and it is the strongest possible form of the parity rule above. It also
//! removes four private-use code points from what [`Glyphs::nerd_glyphs`] makes font
//! detection demand. The full argument is on [`BULLETS`]. The theme already treats
//! bullets this way: "bullets are punctuation, not accents".

/// The bullet at each nesting depth, shared by both glyph sets.
///
/// **ASCII**, chosen by the owner on 2026-08-09: "since lists are so important… why
/// play games at all… how about we use `*`, `>`, `+`, `-`".
///
/// Lists appear in very nearly every document, which makes the bullet the element on
/// the page that can least afford to be invisible. ASCII is the only character class
/// with genuinely universal coverage: no font survey to run, no fallback to depend on,
/// and a single code point of display width 1 everywhere, guaranteed rather than
/// checked. Three of the four — `*`, `+`, `-` — are also the literal bullet characters
/// of Markdown source, so the rendered marker echoes the syntax the author typed.
/// Reliability beats refinement for the commonest element on the page.
///
/// The same four are used whether or not a Nerd Font is detected. Bullets have no
/// business varying with font detection when the entire point of the choice is that
/// they render everywhere.
///
/// # The lesson this cost three rounds to learn
///
/// **Do not choose glyphs by how they measure, or by what one machine's fonts happen
/// to contain — you do not control the reader's font.** For anything load-bearing,
/// prefer characters that render everywhere.
///
/// `mdmost` draws in whatever terminal font the reader has, so ink extents rasterised
/// from any one face predict nothing about `Iosevka`, `JetBrains Mono`, `Menlo` or
/// `Fira Code`; measuring optimises for one machine and silently mis-serves everyone
/// else.
/// Earlier revisions of this comment carried em-fractions to two decimal places and
/// named a specific patched font as "the shipping font". Both were fiction. The font
/// was a guess by an early session — it happened to be installed on the machine that
/// session ran on — and every session afterwards cited the comment back as authority,
/// laundering the guess into a premise by repetition. All of it has been deleted
/// rather than corrected, because a false doc comment is worse than none.
///
/// The one real defect that survives from that era is *coverage*: an invisible bullet
/// is a hard failure, and candidates have genuinely been lost to it — `◦` U+25E6 draws
/// as a blank in at least one popular patched font, and `⦁` U+2981 Z NOTATION SPOT (a
/// formal-methods symbol, never a bullet) is absent from many. ASCII ends that whole
/// class of question.
///
/// Nothing here reaches a regular expression: the only pattern the program builds
/// comes from the reader's search query, and search runs over the Markdown *source*,
/// never over rendered marker cells. `*` and `+` being regex metacharacters is
/// therefore inert.
///
/// Each is a single code point of display width 1, which a test below enforces.
const BULLETS: [&str; 4] = ["*", ">", "+", "-"];

/// The task box, ticked and unticked, shared by both glyph sets.
///
/// **ASCII, and the literal Markdown source syntax**, at the owner's request on
/// 2026-08-09: "hmmm it seems that whole business could be quite fragile … so maybe
/// instead of the fancy checkbox icon we should use `[ ]` and `[x]`?"
///
/// The business was fragile, and this is what deletes it. The Nerd Font boxes were
/// private-use code points that the patch draws at *twice* the advance of an ASCII
/// character while `unicode-width` — which has no data for that range — reports one.
/// Every part of the layout had to be told about that discrepancy by hand: a
/// hand-maintained `task_cells` field carrying the true reservation, a marker field
/// that budgeted it instead of the measured width, and, worst of all, a documented
/// exception to the rule that the two glyph sets never differ in layout. Two
/// long-standing parity tests had to be weakened to accommodate it. All of that existed
/// to serve one pictograph.
///
/// `[ ]` and `[x]` need none of it. They are three ASCII columns that every font on
/// earth draws identically and `unicode-width` measures correctly, so the reservation
/// *is* the measurement, both sets are the same text, and the parity rule is an
/// absolute again — the two tests that guarded it are back on the full corpus. They are
/// also exactly what the author typed in the source, which is the same argument that
/// settled [`BULLETS`]: for something as common as a task list, reliability and
/// familiarity beat decoration.
///
/// Both are three columns wide, so ticked and unticked align by construction rather
/// than by a font's promise — which is the defect that started this whole sequence,
/// when the two Font Awesome boxes turned out to be the same drawing at two sizes.
const TASK_BOXES: (&str, &str) = ("[x]", "[ ]");

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
    /// The task boxes are ASCII and identical to [`Self::NERD`]'s — see [`TASK_BOXES`].
    /// They used to be `☐` U+2610 and `☑` U+2611 here; those were fine characters, but
    /// they made this set differ from the icon set for no benefit the reader could see,
    /// and the whole point of the current pair is that there is only one pair.
    pub const PLAIN: Self = Self {
        bullets: BULLETS,
        task_checked: TASK_BOXES.0,
        task_unchecked: TASK_BOXES.1,
        code_icons: false,
    };

    /// Nerd Font glyphs, the default look where a Nerd Font is detected.
    ///
    /// The code points are named in the comments so they can be checked against
    /// <https://www.nerdfonts.com/cheat-sheet>. Bullets and task boxes are deliberately
    /// ASCII and identical to [`Self::PLAIN`]'s (see [`BULLETS`] and [`TASK_BOXES`]);
    /// what the icons buy is the code-fence language icons, and nothing else.
    ///
    /// # The task boxes used to be here, and were a mistake twice over
    ///
    /// They were `nf-fa-square_o` U+F096 and `nf-fa-check_square_o` U+F046 — the same
    /// drawing at two different sizes, which the owner spotted immediately in the
    /// marker column where nothing hides a mismatch. They were then moved to Material
    /// Design's `checkbox_blank_outline` / `checkbox_marked_outline`, which *are* drawn
    /// as a pair, and that fixed the mismatch but bought a worse problem: the Nerd
    /// Fonts patch draws its icon ranges at twice the advance of an ASCII character,
    /// while `unicode-width` has no data for private-use code points and reports one.
    /// The layout had to be told the truth by hand, and the discrepancy leaked into a
    /// documented exception to the parity rule.
    ///
    /// Both problems were properties of using a pictograph for something that has a
    /// perfectly good ASCII spelling. `[ ]` and `[x]` have neither: see [`TASK_BOXES`].
    ///
    /// One consequence worth knowing. [`Self::nerd_glyphs`] is what font detection
    /// demands coverage of, and the task box used to be the renderer's representative
    /// in it. The renderer is now represented by the code-fence language icons alone.
    /// Detection still rejects a Nerd Fonts v2 patch, because the *chrome*'s file
    /// marker is a five-digit Material code point — but that is now the only thing
    /// holding that line, which a test in [`crate::nerdfont`] pins deliberately.
    pub const NERD: Self = Self {
        bullets: BULLETS,
        task_checked: TASK_BOXES.0,
        task_unchecked: TASK_BOXES.1,
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
    /// Bullets and task boxes are excluded because they are the same ASCII in both
    /// sets: they are not evidence of anything, and requiring them would make detection
    /// ask a question whose answer cannot change the render. That leaves the code-fence
    /// language icons as the renderer's whole contribution to the probe, which is
    /// correct — they are now the only thing the renderer draws that icons change.
    pub fn nerd_glyphs() -> impl Iterator<Item = &'static str> {
        LANGUAGE_ICONS
            .iter()
            .map(|(_, icon)| *icon)
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

    /// The glyphs that must each be exactly one cell: bullets and language icons.
    ///
    /// The task boxes are excluded — they are three columns (`[ ]`) — and have their
    /// own assertions below.
    fn single_cell(set: Glyphs) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = set.bullets.to_vec();
        out.extend(language_icons(set));
        out
    }

    /// Every code-fence icon the set can draw, including the generic fallback.
    fn language_icons(set: Glyphs) -> Vec<&'static str> {
        let mut out = Vec::new();
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
    fn single_cell_glyphs_are_exactly_one_display_column() {
        for set in [Glyphs::PLAIN, Glyphs::NERD] {
            for glyph in single_cell(set) {
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

    /// The two sets must agree on the width of every marker, or `--no-icons` would
    /// shift the document sideways.
    ///
    /// This is the invariant that matters; "everything is one column" was only ever a
    /// convenient way of guaranteeing it, and it stopped being true when the task box
    /// became `[ ]`. Asserted slot by slot rather than in aggregate, so a pair of
    /// compensating errors cannot hide.
    ///
    /// The markers are the bullets and the task boxes: the glyphs that sit in the
    /// document's flow, where a width difference would move text. Code-fence language
    /// icons are deliberately not here — the plain set draws *no* icon at all rather
    /// than a substitute, so the two sets do not even have the same number of glyphs
    /// there. That is safe only because a fence's title sits inside a full-width frame
    /// that absorbs the difference, which is a property of the frame rather than of
    /// the glyph, and so is asserted where the frame is.
    #[test]
    fn the_two_sets_agree_on_the_width_of_every_marker() {
        let markers = |set: Glyphs| {
            let mut out: Vec<&'static str> = set.bullets.to_vec();
            out.push(set.task_checked);
            out.push(set.task_unchecked);
            out
        };
        let plain = markers(Glyphs::PLAIN);
        let nerd = markers(Glyphs::NERD);
        assert_eq!(plain.len(), nerd.len(), "the sets have different shapes");
        for (left, right) in plain.iter().zip(&nerd) {
            assert_eq!(
                display_width(left),
                display_width(right),
                "{left:?} and {right:?} occupy different widths, so turning icons \
                 off would move the text after them"
            );
        }
    }

    /// The ticked and unticked boxes must be the same width as each other.
    ///
    /// This is what keeps a task list's text in one column no matter which items are
    /// done. It is the defect that started the whole checkbox sequence: the original
    /// Font Awesome pair was the same drawing at two different sizes. `[ ]` and `[x]`
    /// satisfy it by construction, which is the point of them.
    #[test]
    fn the_task_boxes_are_the_same_width_as_each_other() {
        for set in [Glyphs::PLAIN, Glyphs::NERD] {
            assert_eq!(
                display_width(set.task(true)),
                display_width(set.task(false)),
                "the boxes must match, or the text would move when an item is ticked"
            );
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
