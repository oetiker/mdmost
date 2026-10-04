// SPDX-License-Identifier: MIT
//! From an author's paint to the styles a node is drawn in (colour spec §6.1).
//!
//! The parse side keeps what was written; the theme keeps 16 repaired slot inks; this
//! is the one place the two meet, once per diagram, so every family resolves a colour
//! to the same slot and draws it the same way.

use crate::canvas::Canvas;
use crate::mermaid::ast::Paint;
use crate::mermaid::layout::graph::FrameStyle;
use crate::mermaid::paint::{self, Resolution};
use crate::theme::{Color, SlotInk, Style, Theme};

/// How one node box is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NodeStyle {
    /// The outline.
    pub border: Style,
    /// Class and ER compartment dividers. Subroutine bars and cylinder lids are part of
    /// the outline and use `border`.
    pub rule: Style,
    /// The background of every cell between the border cells, or `None` for the page.
    pub fill: Option<Color>,
    /// Draw the outline heavy (`stroke-width` 3px or more).
    pub heavy: bool,
}

impl NodeStyle {
    /// The theme default, which is how every node was drawn before colour lines.
    pub(crate) fn plain(theme: &Theme) -> Self {
        Self {
            border: theme.diagram.node_border,
            rule: theme.diagram.compartment,
            fill: None,
            heavy: false,
        }
    }

    /// Sets the fill as background of every cell inside a finished box: all rows but
    /// the first and last, all columns but `wall` on each side.
    ///
    /// Border cells keep the page (colour spec §6.2): a tinted outline would read as a
    /// thicker box rather than a coloured one.
    pub(crate) fn fill_inside(&self, canvas: &mut Canvas, wall: usize) {
        let Some(fill) = self.fill else {
            return;
        };
        let cols = usize::from(canvas.width()).saturating_sub(2 * wall);
        for row in 1..canvas.height().saturating_sub(1) {
            canvas.patch_style(row, wall, cols, Style::new().bg(fill));
        }
    }
}

/// Resolves one diagram's paints against a theme.
pub(crate) struct Painter<'t> {
    theme: &'t Theme,
    resolution: Resolution,
}

impl<'t> Painter<'t> {
    /// Resolves every paint of the diagram, nodes and frames alike, in one go.
    pub(crate) fn new<'p>(paints: impl IntoIterator<Item = &'p Paint>, theme: &'t Theme) -> Self {
        Self {
            theme,
            resolution: Resolution::of(paints),
        }
    }

    /// The slot `paint` draws in, when it has a hue.
    fn slot(&self, paint: &Paint) -> Option<SlotInk> {
        self.resolution
            .slot(paint)
            .map(|slot| self.theme.diagram_slots[slot])
    }

    /// The tint a paint's `fill` asks for: only a hued fill tints (ruling 13), and a
    /// slot that fell back to the page tints nothing.
    fn tint(&self, paint: &Paint, tint: impl Fn(SlotInk) -> Color) -> Option<Color> {
        let hued = paint
            .fill
            .is_some_and(|fill| paint::hue_of(fill.rgb).is_some());
        self.slot(paint)
            .filter(|_| hued)
            .map(tint)
            .filter(|&color| color != self.theme.palette.bg)
    }

    /// The style of a subgraph or composite state frame with this paint (colour spec
    /// §6.3). A paint without a slot keeps the theme's frame inks, since `ink: None`
    /// means `group_border` and `group_title`.
    pub(crate) fn frame(&self, paint: Option<&Paint>) -> FrameStyle {
        let Some(paint) = paint else {
            return FrameStyle::default();
        };
        FrameStyle {
            ink: self.slot(paint).and_then(|slot| slot.ink),
            heavy: paint.heavy,
            tint: self.tint(paint, |slot| slot.half_tint),
        }
    }

    /// The style of a node with this paint (colour spec §6.1).
    pub(crate) fn node(&self, paint: Option<&Paint>) -> NodeStyle {
        let Some(paint) = paint else {
            return NodeStyle::plain(self.theme);
        };
        let plain = NodeStyle::plain(self.theme);
        // The border takes the slot even when the slot came from `fill`. Dividers follow
        // the slot ink, but a paint without one (`classDef default fill:#fff`) leaves
        // them in `compartment` rather than moving them to the border ink.
        let ink = self.slot(paint).and_then(|slot| slot.ink);
        NodeStyle {
            border: ink.map_or(plain.border, |ink| plain.border.fg(ink)),
            rule: ink.map_or(plain.rule, |ink| plain.rule.fg(ink)),
            fill: self.tint(paint, |slot| slot.full_tint),
            heavy: paint.heavy,
        }
    }
}
