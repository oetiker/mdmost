// SPDX-License-Identifier: MIT
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
/// whole cluster.
///
/// §4 also says grapheme clusters are never split. That holds for wrapping, but not
/// literally for filling cells: a cluster measuring more than two columns has to be
/// divided across cells, or replaced when it cannot be divided (see
/// [`cell_clusters`](crate::text::cell_clusters)). The rule §4 is really protecting —
/// a cell draws exactly the width it claims, and a row is exactly `width` columns — is
/// unchanged, and is what the assertion below and `Canvas::check_invariants` enforce.
///
/// # Invariants
///
/// * `width` is `0`, `1` or `2`.
/// * A `width == 2` cell is always followed in its row by exactly one *continuation*
///   cell (`width == 0`, empty text).
/// * A cell whose text is non-empty always has `width == grapheme_width(text)` for its
///   leading cluster; zero-width clusters (combining marks) are appended to the text
///   of the cell they modify rather than getting a cell of their own.
/// * A cell holds no control character. `width` describes what the *terminal* will
///   draw, and a control character is an instruction rather than a glyph: a `TAB`
///   measures one column and draws up to eight, an `ESC` measures one column and draws
///   whatever the sequence behind it says. `cell_clusters` substitutes a printable
///   column of the same width for each of them.
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

    /// A cell holding one *one-cell piece* of text.
    ///
    /// The piece must be no wider than two columns, which is what
    /// [`cell_clusters`](crate::text::cell_clusters) yields — a whole grapheme cluster
    /// where that fits in a cell, a part of one where the cluster can be divided without
    /// changing its width, and a same-width marker run where it cannot. Handing this a
    /// three-column cluster (a wide base plus a spacing mark, or U+17D8 on its own)
    /// produces a cell that claims two columns and draws three, which
    /// [`Canvas::check_invariants`](crate::canvas::Canvas::check_invariants) rejects.
    ///
    /// A zero-width piece yields a zero-width cell; callers that need the canvas
    /// invariants should go through
    /// [`Canvas::write_str`](crate::canvas::Canvas::write_str) instead, which splits
    /// its input properly and merges zero-width pieces into the preceding cell.
    pub fn new(cluster: &str, style: Style) -> Self {
        debug_assert!(
            crate::text::display_width(cluster) <= 2,
            "a cell cannot hold {cluster:?}: split it with text::cell_clusters first"
        );
        debug_assert!(
            !cluster.chars().any(char::is_control),
            "a cell cannot hold the control character in {cluster:?}: split it with \
             text::cell_clusters first, which substitutes one for a printable column"
        );
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
