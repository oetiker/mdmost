//! The clickable `[copy]` drawn into the top edge of a code frame or a table.
//!
//! ASCII, unconditionally — not a Nerd Font glyph behind detection and not a lone
//! Unicode symbol. This is the rule that already governs bullets and task boxes: a mark
//! a reader has to *act on* looks the same in every terminal and can never arrive as
//! tofu. The language label beside it keeps its detected icon, because that one is
//! decoration.
//!
//! One module owns the label, the geometry and the hotspot **because they must not be
//! decided separately**: a drawn label with no hotspot behind it is a control that does
//! nothing, which is exactly what the mouse gate in `RenderOptions::copy_button` exists
//! to prevent. [`place`] emits both or neither.

use crate::canvas::{Canvas, Hotspot};
use crate::theme::Style;

/// What the button says at rest.
pub(crate) const LABEL: &str = "[copy]";

/// What it says just after a copy. Drawn by `tui::draw`, never by a renderer.
pub(crate) const FLASH: &str = "[copied]";

/// Inner columns reserved at the right of a top edge.
///
/// Wider than [`LABEL`] because [`FLASH`] has to fit in the same place without a
/// re-render: `[copied]` plus its margin is what this reservation makes room for, and
/// it is what makes the overwrite possible.
pub(crate) const REGION: u16 = 9;

/// Draws the button into `row` and records its hotspot, or does neither.
///
/// `occupied_until` is the first column to the right of everything already in that edge —
/// the language label, the gutter junction — so the button can decline rather than
/// overwrite it. Returns whether it was placed.
pub(crate) fn place(
    out: &mut Canvas,
    row: usize,
    occupied_until: u16,
    style: Style,
    text: String,
    html: Option<String>,
) -> bool {
    let width = out.width();
    // The region ends one column left of the right corner. Two spare columns are asked
    // for beyond whatever already occupies the edge, so the button never sits flush
    // against the language label.
    let Some(region_start) = width.checked_sub(REGION + 1) else {
        return false;
    };
    if region_start < occupied_until.saturating_add(2) {
        return false;
    }
    let label_len = u16::try_from(LABEL.chars().count()).unwrap_or(6);
    let label_col = width.saturating_sub(label_len + 2);
    out.write_str(row, usize::from(label_col), LABEL, style);
    out.add_hotspot(Hotspot {
        row,
        col: label_col,
        cols: label_len,
        text,
        html,
    });
    true
}
