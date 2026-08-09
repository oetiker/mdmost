//! Table rendering: column-width negotiation, recursive per-cell rendering, borders.
//!
//! The algorithm is design spec §7, in order:
//!
//! 1. measure every cell's *minimum* width (its longest unbreakable run) and its
//!    *natural* width (unwrapped);
//! 2. distribute the available columns — satisfy the minimums, grow towards natural
//!    proportionally, then spread the leftover slack;
//! 3. when even the minimums do not fit, lay the table out at its minimum widths and
//!    clip, so it can be scrolled horizontally rather than mangled;
//! 4. render each cell by recursing into the block renderer at the negotiated width,
//!    which is what makes emphasis, code, links, lists and nested tables work inside
//!    cells with no special case at all;
//! 5. draw rounded borders and honour the GFM per-column alignment;
//! 6. size each row to its tallest cell, top-aligning the shorter ones;
//! 7. put a blank row between the body rows when any of them wraps, carrying the zebra
//!    stripe through it with a half block — see [`gap_row`], which is where that whole
//!    rule and its trade-offs are written down.

use crate::canvas::{BorderSet, Canvas, Cell, CutMark, Rule, Side};
use crate::doc::{Node, NodeKind, TableInfo};
use crate::text::{Align, display_width};
use crate::theme::{Color, Style, Theme};

use super::code::OVERFLOW_MARKER;
use super::{Ctx, RenderOptions, block, inline};

/// Columns consumed by one column's chrome: its left border and the two pad spaces.
const COLUMN_CHROME: usize = 3;

/// Upper half block: shades the top half of a row gap, joining it to the row above.
const UPPER_HALF: char = '\u{2580}';

/// Lower half block: shades the bottom half of a row gap, joining it to the row below.
const LOWER_HALF: char = '\u{2584}';

/// Renders a [`NodeKind::Table`] at `width` columns, clipping if it cannot fit.
///
/// A node of any other kind renders as nothing.
pub fn render_table(node: &Node, width: u16, theme: &Theme, options: &RenderOptions) -> Canvas {
    match &node.kind {
        NodeKind::Table(info) => render_table_node(node, info, width, Ctx::new(theme, options)),
        _ => Canvas::empty(width),
    }
}

/// Renders a table with an explicit context, clipping it to `width`.
pub(crate) fn render_table_node(node: &Node, info: &TableInfo, width: u16, ctx: Ctx<'_>) -> Canvas {
    let Layout {
        mut canvas,
        rules,
        gaps,
    } = lay_out(node, info, width, ctx);
    // A rule cut short closes with its own corner or tee; a row gap, which is shading
    // and nothing else, is cut in silence; only the *content* rows, which are what
    // actually gets cut off, carry the "there is more to the right" chevron. Both kinds
    // of row are named from the layout rather than sniffed out of the finished canvas:
    // the renderer knows exactly which rows it drew, and a canvas full of box art inside
    // a table cell must not be mistaken for one.
    let set = BorderSet::ROUNDED;
    canvas.clip_with_edges(
        width,
        OVERFLOW_MARKER,
        ctx.theme.table.overflow_marker,
        |row| {
            // `gaps` is built in row order and there is one per body row, so a linear
            // scan would be quadratic in the height of a long table.
            if gaps.binary_search(&row).is_ok() {
                return CutMark::Bare;
            }
            rules
                .iter()
                .find(|(at, _)| *at == row)
                .map_or(CutMark::Marker, |(_, rule)| {
                    CutMark::Glyph(set.close(*rule, Side::Right))
                })
        },
    );
    canvas.resize_width(width, ctx.base);
    canvas
}

/// A laid-out table, and the rows of it that carry no content.
struct Layout {
    /// The table at the width its columns negotiated, which may exceed the budget.
    canvas: Canvas,
    /// Every rule row, by index into `canvas`, and which edge of the box it is.
    rules: Vec<(usize, Rule)>,
    /// Every row gap, by index into `canvas` (see [`gap_row`]).
    gaps: Vec<usize>,
}

/// One source row of a table.
struct Row<'a> {
    header: bool,
    cells: Vec<&'a Node>,
}

/// Collects the rows of a table, ignoring anything that is not a row.
fn rows(node: &Node) -> Vec<Row<'_>> {
    node.children
        .iter()
        .filter_map(|child| match child.kind {
            NodeKind::TableRow { header } => Some(Row {
                header,
                cells: child.children.iter().collect(),
            }),
            _ => None,
        })
        .collect()
}

/// Lays a table out at the width its columns negotiated, which may exceed `width`.
///
/// The rule rows are reported alongside the canvas because the clip that follows has to
/// close them rather than mark them, and by then they are indistinguishable from any
/// other row of box art.
fn lay_out(node: &Node, info: &TableInfo, width: u16, ctx: Ctx<'_>) -> Layout {
    let rows = rows(node);
    let columns = rows
        .iter()
        .map(|row| row.cells.len())
        .chain(std::iter::once(info.columns))
        .max()
        .unwrap_or(0);
    if columns == 0 || rows.is_empty() {
        return Layout {
            canvas: Canvas::empty(width),
            rules: Vec::new(),
            gaps: Vec::new(),
        };
    }

    let inner = ctx.in_table();
    let (mins, naturals) = measure_columns(&rows, columns, inner);
    let chrome = COLUMN_CHROME * columns + 1;
    let budget = usize::from(width).saturating_sub(chrome);
    let widths = distribute(&mins, &naturals, budget);
    let full = u16::try_from(widths.iter().sum::<usize>() + chrome).unwrap_or(u16::MAX);

    let border = ctx.theme.table.border;
    let set = BorderSet::ROUNDED;
    let mut out = Canvas::empty(full);
    let mut rules = vec![(out.height(), Rule::Top)];
    out.append(
        &border_row(
            &widths,
            set.top_left,
            set.tee_down,
            set.top_right,
            set,
            border,
        ),
        ctx.base,
    );
    let drawn = draw_rows(&rows, &widths, info, inner);
    // Air between the rows only when the rows need it — see [`gap_row`].
    let spaced = drawn
        .iter()
        .any(|row| !row.header && row.canvas.height() > 1);
    let mut gaps = Vec::new();
    for (index, row) in drawn.iter().enumerate() {
        out.append(&row.canvas, ctx.base);
        let next = drawn.get(index + 1);
        // The rule under the header separates the header from the body, so it is drawn
        // only when there *is* a body. **Changed 2026-08-09:** it used to be drawn
        // unconditionally, on the argument that a header resting straight on the bottom
        // border reads as a broken box. It is the other way round — a `├───┤` with a
        // `╰───╯` directly beneath it is two rules with nothing between them, which is
        // the box art for an empty row, and three reviews read it as broken. A header
        // above the bottom border reads as a one-row table, which is what it is.
        let last_header = row.header && !next.is_some_and(|next| next.header) && next.is_some();
        if last_header {
            rules.push((out.height(), Rule::Middle));
            out.append(
                &border_row(&widths, set.tee_right, set.cross, set.tee_left, set, border),
                ctx.base,
            );
        } else if spaced
            && !row.header
            && let Some(next) = next
            && !next.header
        {
            gaps.push(out.height());
            out.append(
                &gap_row(&widths, full, row.banded, next.banded, ctx),
                ctx.base,
            );
        }
    }
    rules.push((out.height(), Rule::Bottom));
    out.append(
        &border_row(
            &widths,
            set.bottom_left,
            set.tee_up,
            set.bottom_right,
            set,
            border,
        ),
        ctx.base,
    );
    Layout {
        canvas: out,
        rules,
        gaps,
    }
}

/// One drawn table row: its canvas, and the two facts the spacing rule needs about it.
struct Drawn {
    header: bool,
    /// Whether the zebra put the stripe on this row.
    banded: bool,
    canvas: Canvas,
}

/// Draws every row, deciding as it goes which ones the zebra stripes.
///
/// All of them are drawn before any is placed: the spacing rule below needs every body
/// row's *drawn height*, and a gap needs to know which of the two rows it separates
/// carries the stripe. No row is drawn twice.
fn draw_rows(rows: &[Row<'_>], widths: &[usize], info: &TableInfo, ctx: Ctx<'_>) -> Vec<Drawn> {
    let mut body_index = 0usize;
    rows.iter()
        .map(|row| {
            let banded = !row.header && body_index % 2 == 1;
            if !row.header {
                body_index += 1;
            }
            Drawn {
                header: row.header,
                banded,
                canvas: render_row(row, widths, info, banded, ctx),
            }
        })
        .collect()
}

/// Whether a drawn row is a table's row gap (see [`gap_row`]).
///
/// `stripe` is `theme.table.row_alt.bg`.
///
/// Read off the finished canvas rather than handed down, because by the time the *pager*
/// has to decide what a viewport edge cuts through, the drawn document is all it has —
/// the same seam `tui::wide` reads extents and gutters through. The signal is a half
/// block painted in the stripe colour **as a foreground**, which nothing else in a
/// document produces: the shading is the only place `table.row_alt`'s colour is ever a
/// foreground, and matching the glyph alone would catch a document that merely contains
/// the character.
pub fn is_row_gap(cells: &[Cell], stripe: Option<Color>) -> bool {
    let Some(stripe) = stripe else { return false };
    cells.iter().any(|cell| {
        cell.style().fg == Some(stripe) && cell.text().starts_with([UPPER_HALF, LOWER_HALF])
    })
}

/// One blank row between two body rows, with the zebra stripe carried through it by a
/// half block (design spec §7.7).
///
/// # When there is a gap at all
///
/// **A table is spaced exactly when at least one of its body rows is taller than one
/// line at the current width**, and then *every* pair of adjacent body rows gets a gap.
/// Rows that each fit on one line already show their own boundaries — the next row
/// starts where the last one stopped — and air between them would only make the table
/// taller. As soon as one row wraps, that cue is gone: six content lines packed edge to
/// edge read as one block of prose, and `pager` beginning directly under `canvas of
/// styled cells` says nothing about where the row boundary is.
///
/// Per *table*, not per row: spacing only the neighbours of a tall row gives ragged gaps
/// that track row length rather than structure, which is worse than either extreme. The
/// height that decides it is the drawn one, so a cell containing a list, a code block or
/// a nested table counts as readily as a wrapped sentence — the criterion is exactly the
/// crowding the reader sees, and it cannot fall behind as block kinds are added.
///
/// Only *body* rows are measured and only body rows are separated. A header is already
/// fenced off from the body by its own `├───┼───┤` rule, which does this job better than
/// a gap could, so a header that wraps cannot blur a boundary and does not earn one; and
/// a gap laid against that rule, or against the top or bottom border, would be padding
/// rather than structure. Nothing is inserted between two header rows either.
///
/// The decision is width-dependent by construction — the same table is dense at 120
/// columns and spaced at 60 — which is intended, because narrow is precisely when the
/// rows wrap and look cramped. It is taken here, during layout, at a known width, never
/// at parse time (design spec §3); the render cache is keyed on width, so a resize
/// re-renders and re-decides.
///
/// # Which half is shaded
///
/// The stripe on a body row is a *background*, and a background cannot be applied to
/// half a row. The half block is a **foreground glyph** instead: `▀`/`▄` painted in the
/// stripe colour on the page background give a band across the top or the bottom half of
/// the gap, and the same colour value makes it continuous with the neighbouring row's
/// background.
///
/// The shaded half must be the one *adjacent to the striped row*, or the band detaches
/// from the rows it is grouping and reads as a rule of its own:
///
/// * the row above is striped → `▀`, so the band hangs off the bottom of that row;
/// * the row below is striped → `▄`, so the band sits on top of that row;
/// * **neither is striped → the gap stays blank.** There is nothing to carry through it;
///   shading it in the stripe colour would invent a band for two plain rows.
/// * both striped → the whole gap takes the stripe as a background, since a band that
///   has to reach both neighbours is not half a row high.
///
/// The last two cases are unreachable today — the zebra stripes every second body row,
/// so of any two adjacent body rows exactly one is striped — and they are written down
/// rather than asserted because the answer follows from what the shading is *for*, and a
/// future banding rule should inherit it rather than rediscover it.
///
/// A theme whose `row_alt` sets no background gets a blank gap: there is no stripe to
/// carry, and the air alone is still the improvement.
///
/// # The column separators
///
/// They are drawn in the gap as they are on any other row. Left out, every vertical rule
/// in the table would have a row-high hole in it and the box would stop reading as a
/// table — a far worse defect than the one this trades against, which is that the
/// separator's cell is page background and so notches the band by one column at each
/// rule. The notch is half a row high at most and sits in decoration;
/// `a_striped_row_is_shaded_from_border_to_border` is about the *content* rows, where
/// the stripe is a background and the hole was full height, and that property is
/// untouched.
fn gap_row(widths: &[usize], full: u16, above: bool, below: bool, ctx: Ctx<'_>) -> Canvas {
    let shade = match (above, below) {
        (true, false) => Some(UPPER_HALF),
        (false, true) => Some(LOWER_HALF),
        _ => None,
    };
    let fill = if above && below {
        ctx.base.patch(ctx.theme.table.row_alt)
    } else {
        ctx.base
    };
    let mut out = Canvas::new(full, 1, fill);
    if let Some(glyph) = shade
        && let Some(stripe) = ctx.theme.table.row_alt.bg
    {
        out.fill(
            0,
            0,
            usize::from(full),
            &glyph.to_string(),
            Style {
                fg: Some(stripe),
                ..fill
            },
        );
    }
    // The rules take the gap's own background and keep the border's own attributes,
    // exactly as in `render_row`. On a gap the background is the page's, because the
    // shading here is a foreground — see "The column separators" above for what that
    // costs and why it is still the right trade.
    let separator = Style {
        bg: fill.bg,
        ..ctx.theme.table.border
    };
    let mut col = 0usize;
    for width in widths {
        out.vline(
            0,
            col,
            1,
            &BorderSet::ROUNDED.vertical.to_string(),
            separator,
        );
        col += width + COLUMN_CHROME;
    }
    out.vline(
        0,
        col,
        1,
        &BorderSet::ROUNDED.vertical.to_string(),
        separator,
    );
    out
}

/// Draws one horizontal border row.
fn border_row(
    widths: &[usize],
    left: char,
    middle: char,
    right: char,
    set: BorderSet,
    style: Style,
) -> Canvas {
    let text = Canvas::grid_border_row(widths, left, middle, right, set);
    let columns = u16::try_from(display_width(&text)).unwrap_or(u16::MAX);
    Canvas::from_text(columns, &text, style)
}

/// Renders one table row: every cell, then the vertical borders between them.
fn render_row(
    row: &Row<'_>,
    widths: &[usize],
    info: &TableInfo,
    banded: bool,
    ctx: Ctx<'_>,
) -> Canvas {
    let theme = ctx.theme;
    let mut style = if row.header {
        theme.table.header
    } else {
        theme.table.cell
    };
    if banded {
        style = style.patch(theme.table.row_alt);
    }
    // The vertical rules belong to the row they divide, so they take the row's own
    // background. `theme.table.border` carries the *page* background, and painting it
    // straight onto a striped row punched a one-column hole in the stripe at every
    // separator: the band read as two separate shaded boxes rather than one row, and
    // in the light theme as two selected cells (visual review, finding 5). Attributes
    // stay the border's own, so a header row's bold does not leak onto its rules.
    let separator = Style {
        bg: style.bg,
        ..theme.table.border
    };
    let cells: Vec<Canvas> = widths
        .iter()
        .enumerate()
        .map(|(index, &width)| {
            let budget = u16::try_from(width).unwrap_or(u16::MAX);
            let content = match row.cells.get(index) {
                Some(cell) => {
                    block::render_sequence(&cell.children, budget, ctx.in_cell(style), true)
                }
                None => Canvas::empty(budget),
            };
            align_canvas(&content, alignment(info, index), style)
        })
        .collect();

    let height = cells.iter().map(Canvas::height).max().unwrap_or(0).max(1);
    let total = widths.iter().sum::<usize>() + COLUMN_CHROME * widths.len() + 1;
    let full = u16::try_from(total).unwrap_or(u16::MAX);
    let mut out = Canvas::new(full, height, style);
    let mut col = 0usize;
    for (index, cell) in cells.iter().enumerate() {
        out.vline(
            0,
            col,
            height,
            &BorderSet::ROUNDED.vertical.to_string(),
            separator,
        );
        out.blit(0, col + 2, cell, style);
        col += widths[index] + COLUMN_CHROME;
    }
    out.vline(
        0,
        col,
        height,
        &BorderSet::ROUNDED.vertical.to_string(),
        separator,
    );
    out
}

/// The alignment declared for a column, defaulting to left.
fn alignment(info: &TableInfo, column: usize) -> Align {
    info.alignments
        .get(column)
        .copied()
        .flatten()
        .unwrap_or(Align::Left)
}

/// Re-aligns every row of a rendered cell inside its column width.
///
/// The block renderer always produces left-aligned content; centring and right-
/// alignment are applied here so the alignment rule lives in exactly one place.
fn align_canvas(src: &Canvas, align: Align, fill: Style) -> Canvas {
    if align == Align::Left || src.is_empty() {
        return src.clone();
    }
    let width = usize::from(src.width());
    let mut out = Canvas::new(src.width(), src.height(), fill);
    for row in 0..src.height() {
        let text = src.row_text(row);
        let offset = crate::canvas::align_offset(width, display_width(text.trim_end()), align);
        out.blit(row, offset, &src.slice_rows(row, 1), fill);
    }
    out
}

/// The per-column minimum and natural widths, as the maximum over all rows.
fn measure_columns(rows: &[Row<'_>], columns: usize, ctx: Ctx<'_>) -> (Vec<usize>, Vec<usize>) {
    let mut mins = vec![0usize; columns];
    let mut naturals = vec![0usize; columns];
    for row in rows {
        for (index, cell) in row.cells.iter().enumerate().take(columns) {
            let (min, natural) = measure(&cell.children, ctx);
            mins[index] = mins[index].max(min);
            naturals[index] = naturals[index].max(natural);
        }
    }
    (mins, naturals)
}

/// Distributes `budget` columns over the negotiated column widths (design spec §7.2).
fn distribute(mins: &[usize], naturals: &[usize], budget: usize) -> Vec<usize> {
    // Every column needs at least one column of content, or there is nothing to see.
    let mins: Vec<usize> = mins.iter().map(|min| (*min).max(1)).collect();
    let naturals: Vec<usize> = naturals
        .iter()
        .zip(&mins)
        .map(|(natural, min)| (*natural).max(*min))
        .collect();
    let total_min: usize = mins.iter().sum();
    if budget <= total_min {
        // The minimums do not fit: lay out at the minimum and let the caller scroll.
        return mins;
    }
    let total_natural: usize = naturals.iter().sum();
    if total_natural <= budget {
        // Everything fits unwrapped: stop at natural width. Padding the columns out to
        // the terminal would draw an eighty-column box around the letter "a".
        return naturals;
    }
    let demand: usize = naturals
        .iter()
        .zip(&mins)
        .map(|(natural, min)| natural - min)
        .sum();
    let extra = budget - total_min;
    if demand == 0 {
        return mins;
    }
    // Share the growth in proportion to how much each column still wants, settling the
    // rounding with largest remainders so equal demands get equal widths rather than a
    // left-to-right stagger.
    apportion(&mins, &naturals, extra, demand)
}

/// Grows every column from its minimum toward its natural width, sharing `extra`
/// columns in proportion to `natural - min` (design spec §7.2).
///
/// Rounding is settled by the largest-remainder method: columns with the largest
/// fractional claim get the leftover columns first, so three columns with identical
/// content come out identically wide.
fn apportion(mins: &[usize], naturals: &[usize], extra: usize, demand: usize) -> Vec<usize> {
    let claims: Vec<usize> = naturals
        .iter()
        .zip(mins)
        .map(|(natural, min)| (natural - min) * extra)
        .collect();
    let mut widths: Vec<usize> = mins
        .iter()
        .zip(&claims)
        .map(|(min, claim)| min + claim / demand)
        .collect();
    let mut leftover = extra - claims.iter().map(|claim| claim / demand).sum::<usize>();
    // Ties break leftmost-first, which `sort_by_key` gives for free by being stable.
    let mut order: Vec<usize> = (0..widths.len()).collect();
    order.sort_by_key(|&index| std::cmp::Reverse(claims[index] % demand));
    for index in order {
        if leftover == 0 {
            break;
        }
        widths[index] += 1;
        leftover -= 1;
    }
    widths
}

/// The minimum and natural width of a sequence of blocks.
///
/// The minimum is the narrowest budget the content can be laid out in without
/// splitting a word; the natural width is what it would occupy unwrapped.
pub(crate) fn measure(nodes: &[Node], ctx: Ctx<'_>) -> (usize, usize) {
    let mut min = 0usize;
    let mut natural = 0usize;
    let mut index = 0usize;
    while index < nodes.len() {
        // Consecutive inline siblings wrap together as one run, exactly as
        // `block::render_sequence` lays them out; measuring them one at a time would
        // report the width of the longest *word* instead of the width of the sentence.
        let (node_min, node_natural) = if block::is_inline(&nodes[index]) {
            let start = index;
            while index < nodes.len() && block::is_inline(&nodes[index]) {
                index += 1;
            }
            measure_inline(&nodes[start..index], ctx)
        } else {
            index += 1;
            measure_block(&nodes[index - 1], ctx)
        };
        min = min.max(node_min);
        natural = natural.max(node_natural);
    }
    (min, natural)
}

/// The minimum and natural width of one block.
fn measure_block(node: &Node, ctx: Ctx<'_>) -> (usize, usize) {
    match &node.kind {
        NodeKind::Heading { .. } => {
            let (min, natural) = measure_inline(&node.children, ctx);
            (min + 2, natural + 2)
        }
        NodeKind::Paragraph => measure_inline(&node.children, ctx),
        NodeKind::BlockQuote => offset_by(measure(&node.children, ctx), 2),
        NodeKind::List(info) => {
            let field = if info.ordered { 4 } else { 2 };
            offset_by(measure(&node.children, ctx), field)
        }
        NodeKind::Item | NodeKind::TaskItem { .. } | NodeKind::TableCell | NodeKind::Document => {
            measure(&node.children, ctx)
        }
        NodeKind::CodeBlock { literal, .. } => {
            // Code is clipped rather than wrapped, so a narrow column is survivable:
            // its minimum is the chrome itself.
            (
                super::code::chrome_width(),
                super::code::natural_width(literal, ctx),
            )
        }
        NodeKind::Table(info) if ctx.table_depth < super::MAX_TABLE_DEPTH => {
            measure_table(node, info, ctx.in_table())
        }
        NodeKind::ThematicBreak => (1, 1),
        NodeKind::Image { url, .. } => {
            // Below the framed placeholder's own threshold an image degrades to bare
            // text, so its minimum is the narrowest frame that still draws.
            let alt = display_width(&node.plain_text());
            (
                block::IMAGE_MIN_WIDTH,
                alt.max(display_width(url)) + block::IMAGE_CHROME,
            )
        }
        NodeKind::FootnoteDefinition { name, number } => {
            let label = block::footnote_label(name, *number);
            offset_by(measure(&node.children, ctx), display_width(&label) + 3)
        }
        NodeKind::SkippedHtml { .. } => {
            let marker = display_width(inline::HTML_MARKER);
            (marker, marker)
        }
        // Anything else measured in a block position is inline content.
        _ => measure_inline(std::slice::from_ref(node), ctx),
    }
}

/// The minimum and natural width of a whole nested table.
fn measure_table(node: &Node, info: &TableInfo, ctx: Ctx<'_>) -> (usize, usize) {
    let rows = rows(node);
    let columns = rows
        .iter()
        .map(|row| row.cells.len())
        .chain(std::iter::once(info.columns))
        .max()
        .unwrap_or(0);
    if columns == 0 {
        return (0, 0);
    }
    let (mins, naturals) = measure_columns(&rows, columns, ctx);
    let chrome = COLUMN_CHROME * columns + 1;
    (
        mins.iter().map(|min| (*min).max(1)).sum::<usize>() + chrome,
        naturals.iter().sum::<usize>() + chrome,
    )
}

/// The minimum and natural width of inline content.
fn measure_inline(nodes: &[Node], ctx: Ctx<'_>) -> (usize, usize) {
    (
        inline::min_width(nodes, ctx),
        inline::natural_width(nodes, ctx),
    )
}

/// Adds a fixed gutter to a measurement.
fn offset_by(measurement: (usize, usize), gutter: usize) -> (usize, usize) {
    (measurement.0 + gutter, measurement.1 + gutter)
}
