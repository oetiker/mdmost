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
//! 6. size each row to its tallest cell, top-aligning the shorter ones.

use crate::canvas::{BorderSet, Canvas};
use crate::doc::{Node, NodeKind, TableInfo};
use crate::text::{Align, display_width};
use crate::theme::{Style, Theme};

use super::code::OVERFLOW_MARKER;
use super::{Ctx, RenderOptions, block, inline};

/// Columns consumed by one column's chrome: its left border and the two pad spaces.
const COLUMN_CHROME: usize = 3;

/// Renders a [`NodeKind::Table`] at `width` columns, clipping if it cannot fit.
///
/// A node of any other kind renders as nothing.
pub fn render_table(node: &Node, width: u16, theme: &Theme, options: &RenderOptions) -> Canvas {
    match &node.kind {
        NodeKind::Table(info) => render_table_node(node, info, width, Ctx::new(theme, options)),
        _ => Canvas::empty(width),
    }
}

/// Renders a table at its natural size, without clipping it to `width`.
///
/// The returned canvas may be wider than `width`; `width` is still the budget the
/// columns are negotiated for, so the surplus only appears when the minimums genuinely
/// do not fit.
///
/// **Nothing in the viewport calls this.** Horizontal scrolling (design spec §7.3) is
/// done by [`tui::wide::render_scrollable`](crate::tui::wide::render_scrollable), which
/// re-renders every over-wide *block* — tables and code alike — at a larger budget. The
/// doc comment here used to claim the viewport used this function, which was untrue for
/// the whole of the project's life; it is kept for now because it is a reasonable
/// public entry point and one test exercises it, but it has no production caller.
pub fn render_table_full(
    node: &Node,
    width: u16,
    theme: &Theme,
    options: &RenderOptions,
) -> Canvas {
    match &node.kind {
        NodeKind::Table(info) => draw(node, info, width, Ctx::new(theme, options)),
        _ => Canvas::empty(width),
    }
}

/// Renders a table with an explicit context, clipping it to `width`.
pub(crate) fn render_table_node(node: &Node, info: &TableInfo, width: u16, ctx: Ctx<'_>) -> Canvas {
    let mut canvas = draw(node, info, width, ctx);
    canvas.clip_with_marker(width, OVERFLOW_MARKER, ctx.theme.table.overflow_marker);
    canvas.resize_width(width, ctx.base);
    canvas
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
fn draw(node: &Node, info: &TableInfo, width: u16, ctx: Ctx<'_>) -> Canvas {
    let rows = rows(node);
    let columns = rows
        .iter()
        .map(|row| row.cells.len())
        .chain(std::iter::once(info.columns))
        .max()
        .unwrap_or(0);
    if columns == 0 || rows.is_empty() {
        return Canvas::empty(width);
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
    let mut body_index = 0usize;
    for (index, row) in rows.iter().enumerate() {
        let banded = !row.header && body_index % 2 == 1;
        out.append(&render_row(row, &widths, info, banded, inner), ctx.base);
        if !row.header {
            body_index += 1;
        }
        // The rule under the header is drawn even when no body row follows: a header
        // resting straight on the bottom border reads as a broken box, not as an
        // empty table.
        let last_header = row.header && !rows.get(index + 1).is_some_and(|next| next.header);
        if last_header {
            out.append(
                &border_row(&widths, set.tee_right, set.cross, set.tee_left, set, border),
                ctx.base,
            );
        }
    }
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
            theme.table.border,
        );
        out.blit(0, col + 2, cell, style);
        col += widths[index] + COLUMN_CHROME;
    }
    out.vline(
        0,
        col,
        height,
        &BorderSet::ROUNDED.vertical.to_string(),
        theme.table.border,
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
            let alt = display_width(&node.plain_text());
            (6, alt.max(display_width(url)) + 4)
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
