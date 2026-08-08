//! Line masks and the box-drawing glyphs they map to.
//!
//! The routing engine never picks a glyph directly. It records, for every character
//! cell an edge passes through, *which sides of the cell the line leaves through* as a
//! [`Mask`]. Two edges sharing a cell simply union their masks, which is what makes
//! merges (`├`, `┬`, `┼`) fall out of the routing rather than having to be special
//! cased. Node borders are read back into masks with [`mask_of`], so an edge that lands
//! on a box border turns `─` into `┬` automatically.

use std::ops::{BitOr, BitOrAssign};

/// The four sides of a character cell a line can leave through.
///
/// Combine with `|`; test with [`Mask::contains`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Mask(u8);

impl Mask {
    /// No line at all.
    pub const NONE: Self = Self(0);
    /// A line leaving through the top of the cell.
    pub const UP: Self = Self(1);
    /// A line leaving through the bottom of the cell.
    pub const DOWN: Self = Self(2);
    /// A line leaving through the left of the cell.
    pub const LEFT: Self = Self(4);
    /// A line leaving through the right of the cell.
    pub const RIGHT: Self = Self(8);

    /// True when this mask carries no direction at all.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// True when `other`'s directions are all present in `self`.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The raw bit pattern, for table lookups.
    pub fn bits(self) -> u8 {
        self.0
    }
}

impl BitOr for Mask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Mask {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// One of the four movement directions on the character grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dir {
    /// Towards row 0.
    Up,
    /// Towards the last row.
    Down,
    /// Towards column 0.
    Left,
    /// Towards the last column.
    Right,
}

impl Dir {
    /// The mask of the cell side this direction leaves through.
    pub fn mask(self) -> Mask {
        match self {
            Self::Up => Mask::UP,
            Self::Down => Mask::DOWN,
            Self::Left => Mask::LEFT,
            Self::Right => Mask::RIGHT,
        }
    }

    /// The opposite direction.
    pub fn flip(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// True when the direction runs along a column rather than a row.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Up | Self::Down)
    }

    /// Steps one cell in this direction, saturating at the grid origin.
    pub fn step(self, row: usize, col: usize) -> (usize, usize) {
        match self {
            Self::Up => (row.saturating_sub(1), col),
            Self::Down => (row + 1, col),
            Self::Left => (row, col.saturating_sub(1)),
            Self::Right => (row, col + 1),
        }
    }
}

/// How an edge line is drawn (design spec §6.1: `-->`, `-.->`, `==>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Stroke {
    /// A light continuous line.
    #[default]
    Solid,
    /// A dashed line, for `-.->`.
    Dotted,
    /// A heavy line, for `==>`.
    Thick,
}

impl Stroke {
    /// The stroke that wins when two edges share a cell.
    ///
    /// Heavier beats lighter, and a solid line beats a dotted one, so a junction never
    /// looks like it interrupts the more prominent edge.
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Thick, _) | (_, Self::Thick) => Self::Thick,
            (Self::Solid, _) | (_, Self::Solid) => Self::Solid,
            _ => Self::Dotted,
        }
    }
}

/// Light glyphs indexed by mask bits: `UP=1`, `DOWN=2`, `LEFT=4`, `RIGHT=8`.
const LIGHT: [char; 16] = [
    ' ', '│', '│', '│', '─', '╯', '╮', '┤', '─', '╰', '╭', '├', '─', '┴', '┬', '┼',
];

/// Dashed glyphs; Unicode has no dashed corners, so junctions borrow the light set.
const DOTTED: [char; 16] = [
    ' ', '┊', '┊', '┊', '┄', '╯', '╮', '┤', '┄', '╰', '╭', '├', '┄', '┴', '┬', '┼',
];

/// Heavy glyphs; the heavy set has no rounded corners, so they are square.
const HEAVY: [char; 16] = [
    ' ', '┃', '┃', '┃', '━', '┛', '┓', '┫', '━', '┗', '┏', '┣', '━', '┻', '┳', '╋',
];

/// The glyph a cell shows for `mask` when drawn in `stroke`.
///
/// Returns a space for the empty mask.
pub fn glyph(mask: Mask, stroke: Stroke) -> char {
    let table = match stroke {
        Stroke::Solid => &LIGHT,
        Stroke::Dotted => &DOTTED,
        Stroke::Thick => &HEAVY,
    };
    table[usize::from(mask.bits()) & 0x0f]
}

/// The mask a box-drawing glyph already on the canvas stands for.
///
/// This is the inverse of [`glyph`] over every glyph `mdless` draws — the light,
/// heavy, dashed and rounded sets used by [`BorderSet`](crate::canvas::BorderSet) and
/// by this module. Cells holding anything else (text, shape glyphs such as `◇`) return
/// `None`, and the router then leaves them alone.
pub fn mask_of(ch: char) -> Option<Mask> {
    let m = match ch {
        '│' | '┃' | '┊' | '┋' | '╎' | '╏' | '║' => Mask::UP | Mask::DOWN,
        '─' | '━' | '┄' | '┅' | '╌' | '╍' | '═' => Mask::LEFT | Mask::RIGHT,
        '╭' | '┌' | '┏' | '╔' => Mask::DOWN | Mask::RIGHT,
        '╮' | '┐' | '┓' | '╗' => Mask::DOWN | Mask::LEFT,
        '╰' | '└' | '┗' | '╚' => Mask::UP | Mask::RIGHT,
        '╯' | '┘' | '┛' | '╝' => Mask::UP | Mask::LEFT,
        '├' | '┣' | '╠' => Mask::UP | Mask::DOWN | Mask::RIGHT,
        '┤' | '┫' | '╣' => Mask::UP | Mask::DOWN | Mask::LEFT,
        '┬' | '┳' | '╦' => Mask::DOWN | Mask::LEFT | Mask::RIGHT,
        '┴' | '┻' | '╩' => Mask::UP | Mask::LEFT | Mask::RIGHT,
        '┼' | '╋' | '╬' => Mask::UP | Mask::DOWN | Mask::LEFT | Mask::RIGHT,
        _ => return None,
    };
    Some(m)
}

/// The stroke a box-drawing glyph already on the canvas was drawn in.
///
/// Used so that merging an edge into a heavy border keeps the border heavy.
pub fn stroke_of(ch: char) -> Stroke {
    match ch {
        '┃' | '━' | '┏' | '┓' | '┗' | '┛' | '┣' | '┫' | '┳' | '┻' | '╋' | '┅' | '┋' => {
            Stroke::Thick
        }
        '┊' | '┄' | '╌' | '╎' => Stroke::Dotted,
        _ => Stroke::Solid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_round_trip_through_glyphs() {
        for bits in 0u8..16 {
            let mask = Mask(bits);
            let ch = glyph(mask, Stroke::Solid);
            if bits == 0 {
                assert_eq!(ch, ' ');
                continue;
            }
            let back = mask_of(ch).expect("solid glyph is known");
            // Stubs widen to the full run, everything else round-trips exactly.
            assert!(back.contains(mask), "{bits:04b} -> {ch} -> {back:?}");
        }
    }

    #[test]
    fn heavy_glyphs_report_thick_stroke() {
        for bits in 1u8..16 {
            let ch = glyph(Mask(bits), Stroke::Thick);
            assert_eq!(stroke_of(ch), Stroke::Thick, "{ch}");
        }
    }

    #[test]
    fn strokes_merge_by_prominence() {
        assert_eq!(Stroke::Dotted.merge(Stroke::Solid), Stroke::Solid);
        assert_eq!(Stroke::Solid.merge(Stroke::Thick), Stroke::Thick);
        assert_eq!(Stroke::Dotted.merge(Stroke::Dotted), Stroke::Dotted);
    }
}
