//! The terminal cell.

use compact_str::CompactString;

use crate::text::grapheme_width;
use crate::theme::Style;

/// One terminal cell.
///
/// # Deviation from the design spec
///
/// The design spec sketches `Cell { ch: char, .. }`. A `char` cannot hold a grapheme
/// cluster — `"é"` (e + U+0301), `"👩‍💻"` (ZWJ sequence) and `"🇨🇭"` (regional indicator
/// pair) are each several `char`s but exactly one cell. `Cell` therefore stores the
/// whole cluster. Everything else about the spec's contract is unchanged.
///
/// # Invariants
///
/// * `width` is `0`, `1` or `2`.
/// * A `width == 2` cell is always followed in its row by exactly one *continuation*
///   cell (`width == 0`, empty text).
/// * A cell whose text is non-empty always has `width == grapheme_width(text)` for its
///   leading cluster; zero-width clusters (combining marks) are appended to the text
///   of the cell they modify rather than getting a cell of their own.
///
/// The invariants are maintained by [`Canvas`](crate::canvas::Canvas); construct cells
/// through the constructors below rather than by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    text: CompactString,
    style: Style,
    width: u8,
}

impl Cell {
    /// A blank cell in the given style.
    pub fn blank(style: Style) -> Self {
        Self {
            text: CompactString::const_new(" "),
            style,
            width: 1,
        }
    }

    /// A cell holding one grapheme cluster.
    ///
    /// The cluster's display width is measured with [`grapheme_width`]. A zero-width
    /// cluster yields a zero-width cell; callers that need the canvas invariants
    /// should go through [`Canvas::write_str`](crate::canvas::Canvas::write_str)
    /// instead, which merges such clusters into the preceding cell.
    pub fn new(cluster: &str, style: Style) -> Self {
        Self {
            text: CompactString::new(cluster),
            style,
            width: grapheme_width(cluster),
        }
    }

    /// The trailing half of a double-width cell.
    pub fn continuation(style: Style) -> Self {
        Self {
            text: CompactString::const_new(""),
            style,
            width: 0,
        }
    }

    /// The cell's text: one grapheme cluster, possibly with trailing combining marks.
    ///
    /// Empty for a continuation cell.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The cell's style.
    pub fn style(&self) -> Style {
        self.style
    }

    /// Replaces the cell's style.
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Overlays `style` on the cell's current style. See [`Style::patch`].
    pub fn patch_style(&mut self, style: Style) {
        self.style = self.style.patch(style);
    }

    /// The cell's display width: `0`, `1` or `2`.
    pub fn width(&self) -> u8 {
        self.width
    }

    /// Whether this is the trailing half of a double-width cell.
    pub fn is_continuation(&self) -> bool {
        self.width == 0 && self.text.is_empty()
    }

    /// Whether the cell shows nothing but blank space.
    pub fn is_blank(&self) -> bool {
        self.text.chars().all(|c| c == ' ')
    }

    /// Appends a zero-width cluster (a combining mark) to this cell's text.
    pub(crate) fn append_zero_width(&mut self, cluster: &str) {
        self.text.push_str(cluster);
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank(Style::NONE)
    }
}
