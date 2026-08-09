//! Box-drawing glyph sets.

/// Which horizontal edge of a box a rule row is.
///
/// A rule cut short has to be closed with the glyph that belongs to *its* edge — a top
/// rule ends in a top corner, an interior separator in a side tee, a bottom rule in a
/// bottom corner. Ending all three with the same "there is more to the right" chevron is
/// what made a clipped table read as a broken box rather than a scrollable one
/// (`docs/qa/visual-review-3.md` §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// The top edge of a box.
    Top,
    /// An interior separator, such as the rule under a table header.
    Middle,
    /// The bottom edge of a box.
    Bottom,
}

/// The side of a row a cut falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Content continues to the left of the cut.
    Left,
    /// Content continues to the right of the cut.
    Right,
}

/// The glyphs used to draw a rectangular frame.
///
/// Every renderer that draws a box — code blocks, tables, image placeholders,
/// diagram nodes, sequence-diagram frames — uses one of these sets, so that box art
/// is consistent across the whole document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderSet {
    /// Horizontal edge.
    pub horizontal: char,
    /// Vertical edge.
    pub vertical: char,
    /// Top-left corner.
    pub top_left: char,
    /// Top-right corner.
    pub top_right: char,
    /// Bottom-left corner.
    pub bottom_left: char,
    /// Bottom-right corner.
    pub bottom_right: char,
    /// T-junction pointing down (top edge, column separator).
    pub tee_down: char,
    /// T-junction pointing up (bottom edge, column separator).
    pub tee_up: char,
    /// T-junction pointing right (left edge, row separator).
    pub tee_right: char,
    /// T-junction pointing left (right edge, row separator).
    pub tee_left: char,
    /// Four-way crossing.
    pub cross: char,
}

impl BorderSet {
    /// Rounded corners — the default look of `mdless`, per design spec §7.5.
    pub const ROUNDED: Self = Self {
        horizontal: '─',
        vertical: '│',
        top_left: '╭',
        top_right: '╮',
        bottom_left: '╰',
        bottom_right: '╯',
        tee_down: '┬',
        tee_up: '┴',
        tee_right: '├',
        tee_left: '┤',
        cross: '┼',
    };

    /// Square corners.
    pub const PLAIN: Self = Self {
        top_left: '┌',
        top_right: '┐',
        bottom_left: '└',
        bottom_right: '┘',
        ..Self::ROUNDED
    };

    /// Heavy lines, for emphasis such as a critical gantt task frame.
    pub const HEAVY: Self = Self {
        horizontal: '━',
        vertical: '┃',
        top_left: '┏',
        top_right: '┓',
        bottom_left: '┛',
        bottom_right: '┛',
        tee_down: '┳',
        tee_up: '┻',
        tee_right: '┣',
        tee_left: '┫',
        cross: '╋',
    };

    /// Double lines.
    pub const DOUBLE: Self = Self {
        horizontal: '═',
        vertical: '║',
        top_left: '╔',
        top_right: '╗',
        bottom_left: '╚',
        bottom_right: '╝',
        tee_down: '╦',
        tee_up: '╩',
        tee_right: '╠',
        tee_left: '╣',
        cross: '╬',
    };

    /// Dashed lines, for the dotted Mermaid edge forms (`-.->`, `..>`).
    pub const DASHED: Self = Self {
        horizontal: '╌',
        vertical: '╎',
        ..Self::ROUNDED
    };

    /// Every set, so a glyph on a finished canvas can be traced back to the set that
    /// drew it.
    ///
    /// Order matters only where sets share a glyph: [`ROUNDED`](Self::ROUNDED) and
    /// [`DASHED`](Self::DASHED) share every corner and tee, so either answer closes a
    /// rule with the same character.
    pub const ALL: [Self; 5] = [
        Self::ROUNDED,
        Self::PLAIN,
        Self::HEAVY,
        Self::DOUBLE,
        Self::DASHED,
    ];

    /// The glyph that closes `rule` on `side`.
    ///
    /// This is the one place the "end this row properly" mapping lives: the table
    /// renderer closes its own clipped rules with it, and the pager closes whatever
    /// frame a viewport edge happens to cut. Two hand-written copies would drift.
    pub const fn close(self, rule: Rule, side: Side) -> char {
        match (rule, side) {
            (Rule::Top, Side::Left) => self.top_left,
            (Rule::Top, Side::Right) => self.top_right,
            (Rule::Middle, Side::Left) => self.tee_right,
            (Rule::Middle, Side::Right) => self.tee_left,
            (Rule::Bottom, Side::Left) => self.bottom_left,
            (Rule::Bottom, Side::Right) => self.bottom_right,
        }
    }

    /// The set that drew `glyph` as part of a horizontal rule, and the rule it names.
    ///
    /// `Some((set, None))` is a plain horizontal segment: it says the row *is* a rule
    /// here, but a bare `─` cannot say whether it belongs to a top, middle or bottom
    /// edge — the caller has to find that out from elsewhere in the row. Vertical
    /// glyphs are not rule glyphs at all and yield `None`, which is what keeps an
    /// ordinary table cell row out of this path.
    pub fn rule_glyph(glyph: char) -> Option<(Self, Option<Rule>)> {
        Self::ALL.into_iter().find_map(|set| {
            let rule = match glyph {
                g if g == set.top_left || g == set.top_right || g == set.tee_down => {
                    Some(Rule::Top)
                }
                g if g == set.tee_right || g == set.tee_left || g == set.cross => {
                    Some(Rule::Middle)
                }
                g if g == set.bottom_left || g == set.bottom_right || g == set.tee_up => {
                    Some(Rule::Bottom)
                }
                g if g == set.horizontal => None,
                _ => return None,
            };
            Some((set, rule))
        })
    }
}

impl Default for BorderSet {
    fn default() -> Self {
        Self::ROUNDED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_glyph_names_the_edge_it_closes() {
        let set = BorderSet::ROUNDED;
        // The junctions say which edge they belong to on their own.
        assert_eq!(BorderSet::rule_glyph('┬'), Some((set, Some(Rule::Top))));
        assert_eq!(BorderSet::rule_glyph('┼'), Some((set, Some(Rule::Middle))));
        assert_eq!(BorderSet::rule_glyph('┴'), Some((set, Some(Rule::Bottom))));
        // A bare horizontal is a rule but cannot say which one.
        assert_eq!(BorderSet::rule_glyph('─'), Some((set, None)));
        // A vertical is not a rule at all: it is a cut through content, which takes the
        // overflow marker rather than a corner.
        assert_eq!(BorderSet::rule_glyph('│'), None);
        assert_eq!(BorderSet::rule_glyph('a'), None);
        // Each set closes with its own glyphs, so a heavy or doubled box stays itself.
        assert_eq!(set.close(Rule::Top, Side::Right), '╮');
        assert_eq!(set.close(Rule::Top, Side::Left), '╭');
        assert_eq!(
            BorderSet::rule_glyph('╦'),
            Some((BorderSet::DOUBLE, Some(Rule::Top)))
        );
        assert_eq!(BorderSet::DOUBLE.close(Rule::Middle, Side::Right), '╣');
    }

    #[test]
    fn every_border_glyph_is_single_width() {
        for set in [
            BorderSet::ROUNDED,
            BorderSet::PLAIN,
            BorderSet::HEAVY,
            BorderSet::DOUBLE,
            BorderSet::DASHED,
        ] {
            for glyph in [
                set.horizontal,
                set.vertical,
                set.top_left,
                set.top_right,
                set.bottom_left,
                set.bottom_right,
                set.tee_down,
                set.tee_up,
                set.tee_right,
                set.tee_left,
                set.cross,
            ] {
                let text = glyph.to_string();
                assert_eq!(
                    crate::text::grapheme_width(&text),
                    1,
                    "border glyph {glyph:?} must be one column wide"
                );
            }
        }
    }
}
