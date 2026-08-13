//! The footnote popup: where the box goes, and what is in it.
//!
//! Two things live here, and neither of them draws anything or touches an [`App`]:
//! the pure geometry ([`place`]) that answers where a box anchored to a marker cell
//! goes, and the [`Popup`] the pager keeps while one is up.
//!
//! # The one idea
//!
//! **The note renders through the ordinary renderer at the popup's width.** Rendering
//! is a pure function of `(AST, width, theme, options)` (design spec §3), so a popup is
//! *another width*, not a second rendering path: [`super::app::App`] hands the footnote
//! definition's children to [`crate::render::render_blocks`] — the same
//! `render_sequence` walk [`crate::render::render_document`] enters for every top-level
//! block — at the box's inner width. Emphasis, code spans, nested lists and tables
//! inside a footnote therefore work without a line of code here knowing they exist.
//!
//! This module holds no renderer of its own, and if one ever appears in it that is the
//! signal the shared path has been left.

use crate::canvas::Canvas;
use crate::doc::{Node, NodeKind};
use crate::theme::Style;

/// The widest the box is ever drawn, borders included.
///
/// A footnote is an aside; a box that grows to a 200-column terminal stops reading as
/// one and starts covering the sentence the reader is in the middle of. Sixty columns
/// is the measure prose is comfortable at, which is the same argument `body_width`
/// makes about the document itself.
pub const MAX_WIDTH: u16 = 60;

/// The tallest the box is ever drawn, borders included.
///
/// Past this the note scrolls inside the box rather than growing: a popup that fills
/// the screen has become a second document, and the reader has lost the paragraph they
/// asked the question from.
pub const MAX_HEIGHT: u16 = 12;

/// Columns the box spends on chrome: one border and one pad on each side.
///
/// Nothing in this program is welded to its own border — a code frame, a table cell and
/// an image placeholder all keep the same single column of interior padding.
pub const CHROME_COLS: u16 = 4;

/// Rows the box spends on chrome: the two border rows.
pub const CHROME_ROWS: u16 = 2;

/// The narrowest note that is still worth wrapping into a box.
///
/// Below this the box is all border and the note is one letter per line, which is not a
/// footnote any more. A document area this small refuses to open one instead — see
/// [`fits`].
pub const MIN_INNER_WIDTH: u16 = 8;

/// The shortest box that can hold a border and one row of note.
pub const MIN_HEIGHT: u16 = CHROME_ROWS + 1;

/// A rectangle of the document area, in cells.
///
/// Deliberately not `ratatui::layout::Rect`: the geometry is state, and
/// [`super::app::App`] touches no terminal crate (design spec §13). The painter
/// converts at its own edge, exactly as it does for a [`crate::theme::Style`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    /// The first row the box occupies.
    pub top: u16,
    /// The first column it occupies.
    pub left: u16,
    /// How many columns wide it is, borders included.
    pub width: u16,
    /// How many rows tall it is, borders included.
    pub height: u16,
}

impl Area {
    /// Whether cell `(x, y)` of the document area is inside the box.
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.left
            && x < self.left.saturating_add(self.width)
            && y >= self.top
            && y < self.top.saturating_add(self.height)
    }

    /// The region the note itself is drawn in, inside the border and its padding.
    pub fn inner(&self) -> Area {
        Area {
            top: self.top.saturating_add(1),
            left: self.left.saturating_add(CHROME_COLS / 2),
            width: self.width.saturating_sub(CHROME_COLS),
            height: self.height.saturating_sub(CHROME_ROWS),
        }
    }
}

/// Whether a document area this size has room for a popup at all.
///
/// A terminal too small is told so in the status bar rather than shown a box that is
/// all border: the status bar never lies, and silently doing nothing is the failure
/// mode the contents pane's "too narrow for the contents pane" notice exists to avoid.
pub fn fits(screen: (u16, u16)) -> bool {
    screen.0 >= MIN_INNER_WIDTH + CHROME_COLS && screen.1 >= MIN_HEIGHT
}

/// The width the note is rendered at, for a document area `screen_width` columns wide.
///
/// The cap first, then the screen: a narrow terminal gets a narrower note rather than a
/// box that hangs off the edge, and the note is what decides the *final* width — see
/// [`place`], which shrinks the box back onto whatever the note actually used.
pub fn inner_width(screen_width: u16) -> u16 {
    MAX_WIDTH
        .min(screen_width)
        .saturating_sub(CHROME_COLS)
        .max(MIN_INNER_WIDTH)
}

/// Where a box holding a `content`-sized note anchored to the marker at `anchor` goes,
/// inside a document area `screen` cells.
///
/// `anchor` is the marker's own cell, in document-area coordinates; `content` is
/// `(columns, rows)` of the note as the renderer actually laid it out.
///
/// Three rules, in this order:
///
/// * **Sized to its content, up to the cap.** A one-line note gets a small box. A long
///   one stops at [`MAX_WIDTH`] / [`MAX_HEIGHT`] and scrolls inside it.
/// * **Below the marker, or above it when below will not fit.** Below is the default
///   because that is where the eye already is; the flip is what keeps a marker on the
///   last row of the viewport from opening a box nobody can see.
/// * **Left-aligned with the marker, or flush right when that will not fit.** Same
///   argument, sideways.
///
/// The box never leaves the document area on any edge, which is the property every one
/// of the three rules is in service of.
pub fn place(anchor: (u16, u16), content: (u16, u16), screen: (u16, u16)) -> Area {
    let width = content
        .0
        .saturating_add(CHROME_COLS)
        .min(MAX_WIDTH)
        .min(screen.0)
        .max(CHROME_COLS.min(screen.0));
    let height = content
        .1
        .saturating_add(CHROME_ROWS)
        .min(MAX_HEIGHT)
        .min(screen.1)
        .max(MIN_HEIGHT.min(screen.1));

    // Below the marker if the box fits there, otherwise above it — and if it fits
    // neither way (a box taller than the whole document area, which the cap makes
    // possible only on a very short terminal) it is pushed onto the screen from the
    // bottom, because a box whose *top* is off screen shows nothing but its own edge.
    let below = anchor.1.saturating_add(1);
    let top = if below.saturating_add(height) <= screen.1 {
        below
    } else {
        anchor.1.saturating_sub(height)
    };
    let top = top.min(screen.1.saturating_sub(height));

    // Left-aligned with the marker, or pushed back onto the screen from the right edge.
    let left = if anchor.0.saturating_add(width) <= screen.0 {
        anchor.0
    } else {
        screen.0.saturating_sub(width)
    };

    Area {
        top,
        left,
        width,
        height,
    }
}

/// The widest row of `canvas` that has anything drawn in it, in columns.
///
/// This is what "sized to its content" measures. A canvas is exactly as wide as the
/// budget it was rendered at, so its own width says nothing about how much of that the
/// note used; a short note in a sixty-column box would otherwise get a sixty-column
/// box.
///
/// A cell counts as drawn when it has a symbol, *or* a background that is not the
/// canvas's own fill: a code fence's frame and a table's zebra stripe are drawn in cells
/// whose text is a space, and trimming those off would cut the box the renderer drew in
/// half. The comparison has to be against `fill` rather than against `Some(_)` — every
/// cell of the canvas carries the page background, so "has a background" is true of all
/// of them and would measure every note as exactly as wide as its budget.
pub fn used_width(canvas: &Canvas, fill: Style) -> u16 {
    let mut used = 0usize;
    for row in canvas.rows() {
        let last = row.iter().rposition(|cell| {
            let style = cell.style();
            !cell.text().trim().is_empty() || (style.bg.is_some() && style.bg != fill.bg)
        });
        if let Some(last) = last {
            used = used.max(last + 1);
        }
    }
    u16::try_from(used).unwrap_or(u16::MAX)
}

/// The footnote definition named `name`, anywhere in the tree under `root`.
///
/// A walk rather than a scan of the root's own children: comrak puts definitions at the
/// top level today, and a lookup that quietly stopped finding them if it ever did
/// otherwise would present as "that footnote does not exist" — a status bar lying about
/// the document.
pub fn definition<'a>(root: &'a Node, name: &str) -> Option<&'a Node> {
    if let NodeKind::FootnoteDefinition { name: found, .. } = &root.kind
        && found == name
    {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|child| definition(child, name))
}

/// The footnote popup that is up.
///
/// It carries its own rendered note. That is the point of the design: the note was laid
/// out by the ordinary renderer at the box's inner width and nothing repaints or
/// re-measures it afterwards, so the painter has only to copy cells.
#[derive(Debug, Clone)]
pub struct Popup {
    /// The note, rendered at [`Area::inner`]'s width by the ordinary renderer.
    canvas: Canvas,
    /// Where the box sits, in document-area cells.
    area: Area,
    /// What the marker drew, for the border's title: the reader clicked `[3]`, so the
    /// box says `[3]` and not the footnote's internal name.
    label: String,
    /// The first note row on show.
    scroll: usize,
}

impl Popup {
    /// Builds a popup holding `canvas`, anchored to the marker at `anchor`.
    pub fn new(
        canvas: Canvas,
        label: String,
        anchor: (u16, u16),
        screen: (u16, u16),
        fill: Style,
    ) -> Self {
        let content = (
            used_width(&canvas, fill),
            u16::try_from(canvas.height()).unwrap_or(u16::MAX),
        );
        Self {
            area: place(anchor, content, screen),
            canvas,
            label,
            scroll: 0,
        }
    }

    /// Where the box sits.
    pub fn area(&self) -> Area {
        self.area
    }

    /// The note as the renderer laid it out.
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    /// The number the marker drew, for the border's title.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The first note row on show.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// How many rows of note the box shows at once.
    pub fn visible_rows(&self) -> usize {
        usize::from(self.area.inner().height)
    }

    /// The largest valid scroll offset: zero for a note that fits.
    pub fn max_scroll(&self) -> usize {
        self.canvas.height().saturating_sub(self.visible_rows())
    }

    /// How many rows of note are still below the box. Zero when it all fits.
    pub fn hidden_rows(&self) -> usize {
        self.max_scroll().saturating_sub(self.scroll)
    }

    /// Scrolls the note by `delta` rows, clamped at both ends.
    ///
    /// The *note* moves, never the document: the box is anchored to a marker cell, so
    /// scrolling the document under it would leave the box pointing at a sentence that
    /// is no longer there — which is why a document scroll dismisses it instead.
    pub fn scroll_by(&mut self, delta: isize) {
        let target = if delta >= 0 {
            self.scroll.saturating_add(delta.unsigned_abs())
        } else {
            self.scroll.saturating_sub(delta.unsigned_abs())
        };
        self.scroll = target.min(self.max_scroll());
    }

    /// Whether document-area cell `(x, y)` is inside the box.
    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.area.contains(x, y)
    }
}
