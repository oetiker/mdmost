//! Box-drawing glyph sets.

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
