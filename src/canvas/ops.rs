// SPDX-License-Identifier: MIT
//! Composition operations on canvases.
//!
//! These are the operations block, table and diagram renderers assemble their output
//! from. Every one of them preserves the canvas contract described in
//! [`crate::canvas`].

use super::{Anchor, Atom, BorderSet, Canvas, Cell, Hotspot, Pin, SearchSpan, TargetRebase};
use crate::error::CanvasError;
use crate::text::{Align, Line, display_width};
use crate::theme::Style;

/// What a row that lost content to [`Canvas::clip_with_edges`] shows in its last column.
///
/// Three answers, because a clipped canvas has three kinds of row and they say different
/// things. Content rows lost something the reader wants and say so; a frame closes
/// itself, because a box that ends in a chevron reads as a rendering fault rather than
/// as a box that continues; and a row of pure decoration — a table's row gap — lost
/// nothing at all, so it says nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutMark {
    /// Stamp the caller's "there is more to the right" marker.
    Marker,
    /// Draw this frame glyph, in the style the cut cell already had.
    Glyph(char),
    /// Draw nothing; the cut took only decoration.
    Bare,
}

impl Canvas {
    /// Grows the canvas to `width` columns by padding every row on the right.
    ///
    /// # Errors
    ///
    /// Returns [`CanvasError::Narrowing`] if `width` is smaller than the current
    /// width; use [`Canvas::truncate_width`] when narrowing is intended.
    pub fn pad_to_width(&mut self, width: u16, style: Style) -> Result<(), CanvasError> {
        if width < self.width {
            return Err(CanvasError::Narrowing {
                current: self.width,
                requested: width,
            });
        }
        let extra = usize::from(width - self.width);
        for cells in &mut self.rows {
            cells.extend((0..extra).map(|_| Cell::blank(style)));
        }
        self.width = width;
        Ok(())
    }

    /// Shrinks the canvas to `width` columns, clipping on the right.
    ///
    /// A double-width cell straddling the new edge is replaced by a blank, so the
    /// result is still exactly `width` columns wide. Widening is a no-op.
    ///
    /// A [`Hotspot`] over cells the narrowing took is clamped, and dropped when the whole
    /// of it went: a hotspot is a claim on *drawn* cells, and these are not drawn any
    /// more. Search spans are left alone, because their contract is the opposite one —
    /// they are translated verbatim and consumers clamp.
    pub fn truncate_width(&mut self, width: u16, style: Style) {
        if width >= self.width {
            return;
        }
        let keep = usize::from(width);
        for cells in &mut self.rows {
            cells.truncate(keep);
            if cells.last().is_some_and(|c| c.width() == 2) {
                let last = cells.len() - 1;
                cells[last] = Cell::blank(style);
            }
        }
        self.width = width;
        self.clamp_hotspots();
    }

    /// Clips to `width` and stamps `marker` on every row that lost content.
    ///
    /// This is the "the line goes on past here" gesture, and it is deliberately one
    /// operation rather than a truncate followed by a loop: the code renderer and the
    /// table renderer each grew their own version, they disagreed about whether to
    /// guard the zero-width case, and the unguarded one computed `width - 1` on a
    /// `width` of `0`. Guarding it here is structural — a caller cannot forget.
    ///
    /// Rows that fitted are left alone, so a short row in a clipped canvas does not
    /// sprout a misleading marker. A `width` of `0`, or a marker too wide for the last
    /// column, clips without stamping. Widening is a no-op, since nothing was lost.
    pub fn clip_with_marker(&mut self, width: u16, marker: &str, style: Style) {
        self.clip_with_edges(width, marker, style, |_| CutMark::Marker);
    }

    /// [`clip_with_marker`](Canvas::clip_with_marker), except that `edge` decides per
    /// row what a cut leaves in the last column.
    ///
    /// A box drawn wider than the room it has is cut on a *rule* row as readily as on a
    /// content row, and stamping "there is more to the right" over the rule leaves a
    /// `╭` with no `╮`: the shape stops reading as a table and starts reading as a
    /// rendering fault (the 2026-08-09 visual review §11). Closing the rule with its own
    /// corner or tee says the same thing more honestly — the box is whole, and the
    /// *content* is what continues.
    ///
    /// A row that lost nothing but decoration wants neither: a table's row gap carries
    /// no content, so [`CutMark::Bare`] cuts it in silence rather than claiming
    /// something continues where nothing does.
    ///
    /// `edge` is asked about every row, by index into the canvas as it was *before*
    /// clipping, and only consulted for rows that actually lost something. A glyph it
    /// returns is drawn in the style the cut cell already had, which is the frame's own
    /// style — a border closing itself in the overflow-marker colour would be a
    /// different kind of wrong.
    ///
    /// A [`Hotspot`] over a column the cut took is gone with it
    /// ([`Canvas::truncate_width`]), and a hotspot over the column the marker or the
    /// glyph *stayed* in is shortened to stop before it: that cell shows the chevron now,
    /// not the control, and a cell that opens a link without looking like one is the same
    /// fault as a claim on a cell that is not there.
    ///
    /// **Callers that own a clipped-block detector must keep the marker reachable.**
    /// `render::document::ClipTest` decides whether to re-render a block wider by looking for
    /// the marker, so a caller that answered anything but [`CutMark::Marker`] for
    /// *every* row would produce a clipped canvas carrying no marker at all and silently
    /// lose horizontal scrolling. Sparing a table's rules and row gaps is safe: a box
    /// always has content rows between them, and they are cut by exactly the same
    /// amount.
    pub fn clip_with_edges(
        &mut self,
        width: u16,
        marker: &str,
        style: Style,
        edge: impl Fn(usize) -> CutMark,
    ) {
        if width >= self.width {
            return;
        }
        let keep = usize::from(width);
        // Which rows actually lose something has to be answered before the cells go.
        let clipped: Vec<bool> = self
            .rows
            .iter()
            .map(|cells| cells.iter().skip(keep).any(|cell| !cell.is_blank()))
            .collect();

        self.truncate_width(width, style);

        let marker_width = display_width(marker);
        // Where each row's own content stops once the edge glyph has taken its columns.
        // Collected in row order so the clamp below can binary-search it.
        let mut edges: Vec<(usize, u16)> = Vec::new();
        for (row, _) in clipped.iter().enumerate().filter(|(_, cut)| **cut) {
            match edge(row) {
                CutMark::Bare => continue,
                // A frame glyph is one column, so it fits wherever a row survives at
                // all, even where the marker would not.
                CutMark::Glyph(glyph) if keep > 0 => {
                    if let Some(cut) = self.rows[row].get(keep - 1) {
                        let style = cut.style();
                        self.write_str(row, keep - 1, &glyph.to_string(), style);
                        edges.push((row, u16::try_from(keep - 1).unwrap_or(u16::MAX)));
                        continue;
                    }
                }
                _ => {}
            }
            if keep == 0 || marker_width == 0 || marker_width > keep {
                continue;
            }
            self.write_str(row, keep - marker_width, marker, style);
            edges.push((row, u16::try_from(keep - marker_width).unwrap_or(u16::MAX)));
        }
        self.revoke_hotspots_over(&edges);
    }

    /// Shortens or drops every hotspot reaching into the columns an edge glyph took.
    ///
    /// [`Canvas::truncate_width`] has already answered for the columns that went; this
    /// answers for the one or two that stayed and stopped showing the control. A claim
    /// over the "there is more to the right" chevron is a cell that opens a link and does
    /// not look like one, which is the same fault as a claim over a cell that is not
    /// there — the chevron is simply a more visible way to have it.
    ///
    /// `edges` is `(row, first overwritten column)` in ascending row order.
    fn revoke_hotspots_over(&mut self, edges: &[(usize, u16)]) {
        if edges.is_empty() {
            return;
        }
        self.hotspots.retain_mut(|spot| {
            let Ok(index) = edges.binary_search_by_key(&spot.row, |(row, _)| *row) else {
                return true;
            };
            match clamped_claim(spot.col, spot.cols, edges[index].1) {
                Some((_, cols)) => {
                    spot.cols = cols;
                    true
                }
                None => false,
            }
        });
    }

    /// Stamps a hollow rectangle onto the canvas, in place.
    ///
    /// The complement of [`framed`](Canvas::framed): `framed` *wraps* a canvas and
    /// hands back a bigger one, which is right for a box built around finished
    /// content, and useless when the box has to be drawn over a canvas that already
    /// exists — a sequence-diagram `loop`/`alt` frame, a graph-layout container. Both
    /// of those were stamping corners one `write_str` at a time.
    ///
    /// Degenerate sizes degrade instead of panicking: a `height` or `width` of `1`
    /// draws the single line that fits, and `0` draws nothing.
    ///
    /// **The canvas does not grow.** A rectangle reaching past the right or bottom edge
    /// is clipped, so the part that fits is drawn and the rest is dropped. Callers
    /// coming from [`framed`](Canvas::framed), which returns a *larger* canvas, should
    /// size their canvas first.
    pub fn rect(
        &mut self,
        top: usize,
        left: usize,
        height: usize,
        width: usize,
        border: BorderSet,
        style: Style,
    ) {
        if height == 0 || width == 0 {
            return;
        }
        let bottom = top + height - 1;
        let right = left + width - 1;
        let horizontal = border.horizontal.to_string();
        let vertical = border.vertical.to_string();

        if height == 1 {
            self.hline(top, left, width, &horizontal, style);
            return;
        }
        if width == 1 {
            self.vline(top, left, height, &vertical, style);
            return;
        }

        let inner = width - 2;
        self.write_str(top, left, &border.top_left.to_string(), style);
        self.hline(top, left + 1, inner, &horizontal, style);
        self.write_str(top, right, &border.top_right.to_string(), style);

        self.write_str(bottom, left, &border.bottom_left.to_string(), style);
        self.hline(bottom, left + 1, inner, &horizontal, style);
        self.write_str(bottom, right, &border.bottom_right.to_string(), style);

        for row in top + 1..bottom {
            self.write_str(row, left, &vertical, style);
            self.write_str(row, right, &vertical, style);
        }
    }

    /// Builds one horizontal rule of a grid, with a junction over every column break.
    ///
    /// `widths` are the *content* widths of the columns; each is drawn with one cell of
    /// padding on either side, which is the table renderer's convention. `left`,
    /// `junction` and `right` choose the row: `╭ ┬ ╮` for a top edge, `├ ┼ ┤` for a
    /// separator, `╰ ┴ ╯` for a bottom edge.
    ///
    /// Returns the text rather than a canvas so the caller can place it wherever it
    /// belongs; the run is built with
    /// [`repeat_to_width`](crate::text::repeat_to_width) rather than a push loop, so a
    /// multi-column border glyph cannot overshoot.
    pub fn grid_border_row(
        widths: &[usize],
        left: char,
        junction: char,
        right: char,
        border: BorderSet,
    ) -> String {
        /// Cells of padding either side of a column's content.
        const PADDING: usize = 2;

        let mut text = String::new();
        text.push(left);
        for (index, width) in widths.iter().enumerate() {
            if index > 0 {
                text.push(junction);
            }
            text.push_str(&crate::text::repeat_to_width(
                &border.horizontal.to_string(),
                width + PADDING,
            ));
        }
        text.push(right);
        text
    }

    /// Sets the canvas width, padding or clipping as needed.
    pub fn resize_width(&mut self, width: u16, style: Style) {
        if width >= self.width {
            // Padding cannot fail once we know we are not narrowing.
            let _ = self.pad_to_width(width, style);
        } else {
            self.truncate_width(width, style);
        }
    }

    /// Copies `src` onto `self` with its top-left corner at `(top, left)`.
    ///
    /// * `self` grows downwards with blank rows if `src` does not fit vertically.
    /// * Content that would fall off the right edge is clipped.
    /// * `src`'s anchors and spans are translated by the offset and merged into
    ///   `self`'s.
    /// * Search spans are translated verbatim, including spans whose cells were
    ///   clipped at the right edge; consumers must clamp a span to the canvas width
    ///   before highlighting it.
    /// * `src`'s [`Hotspot`]s are translated by the offset and kept, with their `target`
    ///   rebased ([`Canvas::merge_hotspots`]). A hotspot carries its own `col`, so once
    ///   translated it names specific destination cells — and after the blit the control's
    ///   characters really are drawn in exactly those cells. That is the same claim a
    ///   search span makes, and the same reason this operation has always translated one.
    ///   Unlike a search span, a hotspot is **clamped** to the destination width and
    ///   dropped when nothing of it survives the clip: a region that reacts to the pointer
    ///   while showing nothing of the control is worse than no region.
    /// * `src`'s [`Pin`]s and [`Atom`]s are **dropped**. A pin claims the leading columns
    ///   of a row *from column zero* and an atom a rectangle of rows, and a blit puts
    ///   `src` somewhere on rows it may well share with other content — a table cell is
    ///   the case that matters — where it has no standing to make either claim. The
    ///   operations that keep them are [`Canvas::append`] and [`Canvas::indent`], which
    ///   move whole rows.
    /// * Cells of `src` that are blank *and* carry no style still overwrite the
    ///   destination; use [`Canvas::blit_opaque`] semantics deliberately — a canvas is
    ///   a rectangle, not a sprite with transparency.
    pub fn blit(&mut self, top: usize, left: usize, src: &Canvas, fill: Style) {
        self.blit_rebased(top, left, src, fill, &mut TargetRebase::new());
    }

    /// [`blit`](Canvas::blit), with the hotspot target remap supplied by the caller.
    ///
    /// For the caller that places one source canvas with several blits and needs a
    /// control spanning them to stay one control; see [`TargetRebase`]. Every other
    /// caller wants [`Canvas::blit`], which is this with a remap of its own.
    pub fn blit_rebased(
        &mut self,
        top: usize,
        left: usize,
        src: &Canvas,
        fill: Style,
        rebase: &mut TargetRebase,
    ) {
        self.pad_to_height(top + src.height(), fill);
        let width = usize::from(self.width);
        for (offset, cells) in src.rows.iter().enumerate() {
            let row = top + offset;
            let mut col = left;
            for cell in cells {
                if col >= width {
                    break;
                }
                match cell.width() {
                    0 => {}
                    w if col + usize::from(w) <= width => {
                        // Re-drawing through `write_str` keeps every repair rule in one
                        // place, including the double-width straddle handling.
                        self.write_str(row, col, cell.text(), cell.style());
                        col += usize::from(w);
                    }
                    _ => {
                        // A double-width cell straddling the right edge: blank it.
                        self.write_str(row, col, " ", cell.style());
                        col += 1;
                    }
                }
            }
        }
        self.merge_metadata(src, top, left);
        self.merge_hotspots(src, top, u16::try_from(left).unwrap_or(u16::MAX), rebase);
    }

    /// Translates and merges `src`'s anchors and spans into `self`.
    fn merge_metadata(&mut self, src: &Canvas, top: usize, left: usize) {
        self.anchors.extend(src.anchors.iter().map(|a| Anchor {
            id: a.id.clone(),
            level: a.level,
            row: a.row + top,
        }));
        let left16 = u16::try_from(left).unwrap_or(u16::MAX);
        self.spans.extend(src.spans.iter().map(|s| SearchSpan {
            row: s.row + top,
            col: s.col.saturating_add(left16),
            ..*s
        }));
    }

    /// Translates and merges `src`'s pins into `self`.
    ///
    /// Separate from [`Canvas::merge_metadata`] because pins travel with whole rows and
    /// the other two channels travel with cells: only the operations that place `src`
    /// across the full width of the destination rows may call this.
    fn merge_pins(&mut self, src: &Canvas, top: usize, left: u16) {
        self.pins.extend(src.pins.iter().map(|pin| Pin {
            row: pin.row + top,
            cols: pin.cols.saturating_add(left),
        }));
    }

    /// Translates and merges `src`'s hotspots into `self`.
    ///
    /// Separate from [`Canvas::merge_metadata`] because a hotspot needs two things that
    /// an anchor and a span do not: its target rebased, and its claim clamped.
    ///
    /// `target` is rebased, not copied verbatim: `src` numbered its controls from zero
    /// on its own canvas, and `self` may already hold controls numbered from zero too
    /// (`document::render` stacks one block canvas after another this way). Copying the
    /// id through unchanged would let an unrelated control in each collide on the same
    /// target, which would then light together on hover. Each *distinct* source target
    /// gets one fresh id in `self`; two source hotspots sharing a target still share
    /// their new one, so a control that wraps across rows stays one control. `rebase`
    /// carries that mapping, and a caller may hand the same one to several merges that
    /// together place one source canvas ([`TargetRebase`]).
    ///
    /// A claim is clamped to `self.width` and dropped when nothing of it survives, by
    /// [`clamped_claim`]. `blit` clips cells at the right edge, and a hotspot over cells
    /// that were clipped away would be a region reacting to a pointer with nothing of the
    /// control under it. `append` and `indent` never clip, so there the clamp is a no-op
    /// they pay one comparison for.
    fn merge_hotspots(&mut self, src: &Canvas, top: usize, left: u16, rebase: &mut TargetRebase) {
        let width = self.width;
        for spot in &src.hotspots {
            let Some((col, cols)) = clamped_claim(spot.col.saturating_add(left), spot.cols, width)
            else {
                continue;
            };
            let target = *rebase
                .map
                .entry(spot.target)
                .or_insert_with(|| self.next_target());
            self.hotspots.push(Hotspot {
                row: spot.row + top,
                col,
                cols,
                kind: spot.kind.clone(),
                target,
            });
        }
    }

    /// Drops or shortens every hotspot claiming a column the canvas no longer has.
    ///
    /// The counterpart of the clamp in [`Canvas::merge_hotspots`], for the other way a
    /// claim can end up outside the canvas: the canvas itself got narrower under it. A
    /// table wider than the page is drawn at the width its columns negotiated and then
    /// [`Canvas::clip_with_edges`] cuts it, and a link in a column that was cut must not
    /// go on claiming cells past the edge.
    fn clamp_hotspots(&mut self) {
        let width = self.width;
        self.hotspots
            .retain_mut(|spot| match clamped_claim(spot.col, spot.cols, width) {
                Some((_, cols)) => {
                    spot.cols = cols;
                    true
                }
                None => false,
            });
    }

    /// Translates and merges `src`'s atoms into `self`.
    ///
    /// Separate from [`Canvas::merge_metadata`] for the reason [`Canvas::merge_pins`] is:
    /// an atom claims a rectangle of rows a block owns outright, so it travels with the
    /// operations that stack and inset whole rows and not with `blit`.
    fn merge_atoms(&mut self, src: &Canvas, top: usize, left: u16) {
        self.atoms.extend(src.atoms.iter().map(|atom| Atom {
            row: atom.row + top,
            col: atom.col.saturating_add(left),
            ..atom.clone()
        }));
    }

    /// Appends `other` below `self`.
    ///
    /// The result is as wide as the wider of the two; the narrower one is padded on
    /// the right with blanks in `fill`.
    ///
    /// Whole rows are stacked, so `other`'s [`Pin`]s come with them unchanged.
    pub fn append(&mut self, other: &Canvas, fill: Style) {
        let width = self.width.max(other.width);
        let _ = self.pad_to_width(width, fill);
        let top = self.height();
        self.rows
            .extend(other.rows.iter().map(|cells| pad_cells(cells, width, fill)));
        self.merge_metadata(other, top, 0);
        self.merge_pins(other, top, 0);
        self.merge_hotspots(other, top, 0, &mut TargetRebase::new());
        self.merge_atoms(other, top, 0);
    }

    /// Stacks canvases vertically, in order.
    ///
    /// The result is as wide as the widest input, and as wide as `min_width` at
    /// minimum.
    pub fn vconcat(parts: &[Canvas], min_width: u16, fill: Style) -> Canvas {
        let width = parts
            .iter()
            .map(Canvas::width)
            .max()
            .unwrap_or(0)
            .max(min_width);
        let mut out = Canvas::empty(width);
        for part in parts {
            out.append(part, fill);
        }
        out
    }

    /// Places canvases side by side, separated by `gap` blank columns.
    ///
    /// Shorter canvases are top-aligned and padded with blank rows, matching the
    /// table renderer's row rule (design spec §7.6).
    pub fn hconcat(parts: &[Canvas], gap: u16, fill: Style) -> Canvas {
        if parts.is_empty() {
            return Canvas::empty(0);
        }
        let height = parts.iter().map(Canvas::height).max().unwrap_or(0);
        // Widened to `usize` before summing and saturated on the way back: a row of
        // wide parts can overflow `u16` in release, where it wraps silently and yields
        // a canvas narrower than its own content.
        let content: usize = parts.iter().map(|part| usize::from(part.width())).sum();
        let gaps = usize::from(gap) * parts.len().saturating_sub(1);
        let total = u16::try_from(content + gaps).unwrap_or(u16::MAX);
        let mut out = Canvas::new(total, height, fill);
        let mut col = 0usize;
        for part in parts {
            out.blit(0, col, part, fill);
            col += usize::from(part.width) + usize::from(gap);
        }
        out
    }

    /// Returns a copy of `self` inset by `left` and `right` blank columns.
    ///
    /// Used for list indentation and block quote gutters.
    ///
    /// Every row moves right by `left` in one piece, so a [`Pin`] moves with it: the
    /// columns the indent added are chrome of the container, and a gutter two spaces into
    /// a list item is still welded to the left edge of the page.
    ///
    /// Hotspots come across in the `blit`, which carries them; only the two channels a
    /// blit drops are merged again here.
    pub fn indent(&self, left: u16, right: u16, fill: Style) -> Canvas {
        let mut out = Canvas::new(self.width + left + right, self.height(), fill);
        out.blit(0, usize::from(left), self, fill);
        out.merge_pins(self, 0, left);
        out.merge_atoms(self, 0, left);
        out
    }

    /// Returns rows `start..start + len` as a canvas of the same width.
    ///
    /// Anchors, spans, pins and hotspots falling inside the slice are translated; the
    /// rest are dropped.
    ///
    /// **A hotspot keeps its `target` verbatim, and that is load-bearing.** The slice is a
    /// different canvas but it is a *view of this one*, so two slices of the same wrapped
    /// control still name it with the same id — which is what lets
    /// `render::table::align_canvas` place a cell row by row and hand every one of those
    /// blits the same [`TargetRebase`], so the control arrives whole. The counter
    /// [`Canvas::next_target`] issues from comes across for the same reason: a slice that
    /// numbered its next control from zero would hand out an id its copied hotspots
    /// already hold.
    pub fn slice_rows(&self, start: usize, len: usize) -> Canvas {
        let end = (start + len).min(self.height());
        let start = start.min(end);
        let mut out = Canvas::empty(self.width);
        out.rows = self.rows[start..end].to_vec();
        out.next_target = self.next_target;
        out.anchors = self
            .anchors
            .iter()
            .filter(|a| (start..end).contains(&a.row))
            .map(|a| Anchor {
                id: a.id.clone(),
                level: a.level,
                row: a.row - start,
            })
            .collect();
        out.spans = self
            .spans
            .iter()
            .filter(|s| (start..end).contains(&s.row))
            .map(|s| SearchSpan {
                row: s.row - start,
                ..*s
            })
            .collect();
        out.pins = self
            .pins
            .iter()
            .filter(|pin| (start..end).contains(&pin.row))
            .map(|pin| Pin {
                row: pin.row - start,
                cols: pin.cols,
            })
            .collect();
        out.hotspots = self
            .hotspots
            .iter()
            .filter(|spot| (start..end).contains(&spot.row))
            .map(|spot| Hotspot {
                row: spot.row - start,
                col: spot.col,
                cols: spot.cols,
                kind: spot.kind.clone(),
                target: spot.target,
            })
            .collect();
        // An atom that hangs off the end of the slice is *clipped*, not dropped: half a
        // diagram on screen is still a diagram, and the source it copies does not
        // depend on how much of it the viewport happens to show.
        out.atoms = self
            .atoms
            .iter()
            .filter_map(|atom| {
                let top = atom.row.max(start);
                let bottom = (atom.row + atom.rows).min(end);
                (top < bottom).then_some(Atom {
                    row: top - start,
                    rows: bottom - top,
                    ..atom.clone()
                })
            })
            .collect();
        out
    }

    /// Draws a frame around `self`, returning a canvas two columns wider and two rows
    /// taller.
    ///
    /// `title`, when given, is written into the top edge one column in from the left
    /// corner, surrounded by a space on each side. A title longer than the top edge is
    /// clipped, so the corner glyphs always survive; a title is only drawn at all when
    /// the content is at least four columns wide.
    pub fn framed(
        &self,
        border: BorderSet,
        border_style: Style,
        title: Option<&Line>,
        fill: Style,
    ) -> Canvas {
        self.framed_captioned(border, border_style, title, None, fill)
    }

    /// Draws a frame around `self` with a label on each horizontal edge.
    ///
    /// `title` sits in the top edge and `caption` in the bottom one, both written one
    /// column in from the corner and surrounded by a space, so a block can say what it
    /// *is* at the top and what happened to it at the bottom without either label
    /// becoming a stray line of text outside the box.
    pub fn framed_captioned(
        &self,
        border: BorderSet,
        border_style: Style,
        title: Option<&Line>,
        caption: Option<&Line>,
        fill: Style,
    ) -> Canvas {
        let width = self.width + 2;
        let height = self.height() + 2;
        let mut out = Canvas::new(width, height, fill);
        let inner = usize::from(self.width);

        out.write_str(0, 0, &border.top_left.to_string(), border_style);
        out.hline(0, 1, inner, &border.horizontal.to_string(), border_style);
        out.write_str(0, inner + 1, &border.top_right.to_string(), border_style);

        out.write_str(height - 1, 0, &border.bottom_left.to_string(), border_style);
        out.hline(
            height - 1,
            1,
            inner,
            &border.horizontal.to_string(),
            border_style,
        );
        out.write_str(
            height - 1,
            inner + 1,
            &border.bottom_right.to_string(),
            border_style,
        );

        for row in 1..height - 1 {
            out.write_str(row, 0, &border.vertical.to_string(), border_style);
            out.write_str(row, inner + 1, &border.vertical.to_string(), border_style);
        }

        for (row, label) in [(0, title), (height - 1, caption)] {
            if let Some(label) = label
                && inner >= 4
            {
                let mut spaced = Line::empty();
                spaced.push(crate::text::Span::new(" ", border_style));
                for span in &label.spans {
                    spaced.push(span.clone());
                }
                spaced.push(crate::text::Span::new(" ", border_style));
                // Clip to the inner width so an over-long label can never overwrite
                // the corner glyph.
                out.write_line(row, 1, &spaced.truncated(inner), border_style);
            }
        }

        out.blit(1, 1, self, fill);
        out
    }

    /// Draws a full-width horizontal rule as a new row.
    ///
    /// Returns the index of the new row.
    pub fn push_rule(&mut self, cluster: &str, style: Style) -> usize {
        let row = self.push_blank_row(style);
        self.hline(row, 0, usize::from(self.width), cluster, style);
        row
    }

    /// Appends a row holding `text` aligned within the canvas, truncating with an
    /// ellipsis if it does not fit.
    ///
    /// Returns the index of the new row.
    pub fn push_text_ellipsized(&mut self, text: &str, align: Align, style: Style) -> usize {
        let shortened = crate::text::ellipsize(text, usize::from(self.width));
        self.push_text(&shortened, align, style)
    }
}

/// The columns a hotspot claims, once cut back to end before `limit`.
///
/// `None` when the claim starts at `limit` or past it, so nothing of it is left. One
/// function rather than a rule written three times: a blit's right edge, a narrowing
/// ([`Canvas::truncate_width`]) and the column an edge glyph takes
/// ([`Canvas::revoke_hotspots_over`]) all cut a claim, and they have to cut it
/// identically — "two paths that merely agree today" is how this file grew the bugs it
/// has already removed.
fn clamped_claim(col: u16, cols: u16, limit: u16) -> Option<(u16, u16)> {
    if col >= limit {
        return None;
    }
    let cols = cols.min(limit - col);
    (cols > 0).then_some((col, cols))
}

/// Copies `cells` into a row of exactly `width` columns.
fn pad_cells(cells: &[Cell], width: u16, fill: Style) -> Vec<Cell> {
    let mut out = cells.to_vec();
    let current: usize = out.iter().map(|c| usize::from(c.width())).sum();
    let target = usize::from(width);
    if current < target {
        out.extend((current..target).map(|_| Cell::blank(fill)));
    } else if current > target {
        out.truncate(target);
        if out.last().is_some_and(|c| c.width() == 2) {
            let last = out.len() - 1;
            out[last] = Cell::blank(fill);
        }
    }
    out
}
