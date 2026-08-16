// SPDX-License-Identifier: MIT
//! Block-level rendering: headings, paragraphs, lists, quotes, rules, code blocks and
//! image placeholders.
//!
//! Every function here obeys the same contract: it is handed a width budget and
//! returns a [`Canvas`] that is exactly that many columns wide. Nesting is expressed
//! by rendering the children at a reduced budget and composing with the canvas
//! operations, which is why quotes inside lists inside table cells need no special
//! case.
//!
//! Nodes of kind [`NodeKind::SkippedHtml`] never reach the canvas: they collapse to a
//! dim `⟨html⟩` marker (design spec §2).

use crate::canvas::{Anchor, BorderSet, Canvas};
use crate::doc::{ListInfo, Node, NodeKind};
use crate::numbering::Numbering;
use crate::text::{Align, Line, Span, display_width, pad_to_width, repeat_to_width};
use crate::theme::{Style, Theme};

use super::inline::{HTML_MARKER, render_inline};
use super::{Ctx, MAX_TABLE_DEPTH, RenderOptions, code, inline, table};

/// The vertical bar drawn to the left of a block quote.
pub(crate) const QUOTE_BAR: &str = "▌";

/// The rule drawn beneath a heading, by level; `None` where a level draws none.
///
/// **Changed on 2026-08-09 at the owner's request** (design spec §9): headings used to
/// carry a prefix glyph and a rule under levels 1 and 2 only. The prefix is gone —
/// "the special character before the sectioning lines is a strange habit… nobody does
/// that" — so the rule is now the whole of the level signal, and there is one for
/// every level that a reader plausibly nests to.
///
/// The ladder steps down in *ink*, which is the only property that survives being
/// read at a glance: a solid heavy bar, a solid light bar, then the same light bar
/// broken into two, three and four dashes per cell. Level 6 gets none at all — after
/// five distinguishable rules the next step down is nothing, and a document that
/// nests six deep is better served by the blank line than by a sixth near-invisible
/// dash pattern.
///
/// Which levels have a rule is [`Theme::heading_has_rule`](crate::theme::Theme::heading_has_rule)'s
/// policy; a test in this module asserts this table agrees with it, because two
/// answers to that question would eventually disagree.
const HEADING_RULES: [Option<&str>; 6] = [
    Some("━"), // U+2501 heavy horizontal
    Some("─"), // U+2500 light horizontal
    Some("╌"), // U+254C light double dash
    Some("┄"), // U+2504 light triple dash
    Some("┈"), // U+2508 light quadruple dash
    None,
];

/// The rule glyph for a heading level, or `None` if that level draws none.
pub(crate) fn heading_rule(level: u8) -> Option<&'static str> {
    HEADING_RULES[usize::from(level.clamp(1, 6)) - 1]
}

/// Renders one block at `width` columns.
///
/// The result is exactly `width` columns wide, and empty for a node that renders to
/// nothing.
pub fn render_block(node: &Node, width: u16, theme: &Theme, options: &RenderOptions) -> Canvas {
    render_block_ctx(node, width, Ctx::new(theme, options))
}

/// Renders one block of a document whose sections are numbered (design spec §9.3).
///
/// The same as [`render_block`] except that a heading in `numbers` is drawn with its
/// number in front of it. This is what a caller assembling the top level block by block
/// — the pager's [`crate::render::render_document`] — needs: the numbering is a
/// property of the whole document, so it is computed there, once, and handed down.
pub fn render_block_numbered(
    node: &Node,
    width: u16,
    theme: &Theme,
    options: &RenderOptions,
    numbers: &Numbering,
) -> Canvas {
    render_block_ctx(node, width, Ctx::new(theme, options).numbered(numbers))
}

/// Renders a sequence of blocks at `width` columns, separated by blank rows.
pub fn render_blocks(nodes: &[Node], width: u16, theme: &Theme, options: &RenderOptions) -> Canvas {
    render_sequence(nodes, width, Ctx::new(theme, options), true)
}

/// Renders a sequence of sibling blocks.
///
/// `spaced` inserts one blank row between blocks, which is what separates paragraphs
/// in a document and what a *tight* list suppresses inside one of its items. The
/// spacing *between* list items is not decided here — see [`list`].
///
/// A block may also ask to be set off from its neighbours even where `spaced` is
/// false: a spaced list does, so that the item introducing it is not welded to it.
/// See [`list`]'s docs for why that belongs to the list rather than to the sequence.
pub(crate) fn render_sequence(nodes: &[Node], width: u16, ctx: Ctx<'_>, spaced: bool) -> Canvas {
    let fill = ctx.base;
    let mut out = Canvas::empty(width);
    // Whether the block last appended asked to be set off, so the *next* seam gets a
    // blank row as well as the one before it.
    let mut previous_set_off = false;
    let mut push = |part: &Canvas, set_off: bool, out: &mut Canvas| {
        if part.is_empty() {
            return;
        }
        if !out.is_empty() && (spaced || set_off || previous_set_off) {
            out.push_blank_row(fill);
        }
        out.append(part, fill);
        previous_set_off = set_off;
    };
    let mut index = 0usize;
    while index < nodes.len() {
        if is_inline(&nodes[index]) {
            // Consecutive inline siblings — the children of a paragraph, or the bare
            // inline content of a table cell — wrap together as one run.
            let start = index;
            while index < nodes.len() && is_inline(&nodes[index]) {
                index += 1;
            }
            push(
                &render_inline(&nodes[start..index], width, ctx.base, ctx),
                false,
                &mut out,
            );
        } else {
            let (part, set_off) = render_block_set_off(&nodes[index], width, ctx);
            push(&part, set_off, &mut out);
            index += 1;
        }
    }
    out
}

/// Renders one block, and says whether it asks to be set off from its neighbours.
///
/// Only a list answers `true`, and only when it is spaced; everything else is placed
/// by its sequence's own rule. Keeping the question here rather than inspecting the
/// node means the answer is a *measurement of the drawn block* — the same thing
/// [`list`] measured to reach it — and not a second guess at it.
fn render_block_set_off(node: &Node, width: u16, ctx: Ctx<'_>) -> (Canvas, bool) {
    let NodeKind::List(info) = &node.kind else {
        return (render_block_ctx(node, width, ctx), false);
    };
    let (mut canvas, spaced) = list(node, *info, width, ctx);
    canvas.resize_width(width, ctx.base);
    (canvas, spaced)
}

/// Whether a node belongs to an inline run rather than being a block of its own.
///
/// An image is inline, so that its bracketed alt text stays in the sentence it was
/// written in — as is inline HTML, so that its collapsed marker does. **Changed
/// 2026-08-09**: an image used to be a block whatever it was written in, which cut
/// every sentence containing one into three blocks with a full-width box between
/// them. An image that is a paragraph of its own still gets that box; [`paragraph`]
/// is the one place that decides so, because it is the only place that can see the
/// image is alone.
pub(crate) fn is_inline(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::Text(_)
            | NodeKind::Image { .. }
            | NodeKind::SoftBreak
            | NodeKind::LineBreak
            | NodeKind::Code { .. }
            | NodeKind::Emph
            | NodeKind::Strong
            | NodeKind::Strikethrough
            | NodeKind::Link { .. }
            | NodeKind::FootnoteReference { .. }
            | NodeKind::SkippedHtml { block: false, .. }
    )
}

/// Renders one block with an explicit context.
pub(crate) fn render_block_ctx(node: &Node, width: u16, ctx: Ctx<'_>) -> Canvas {
    let mut canvas = match &node.kind {
        NodeKind::Document => render_sequence(&node.children, width, ctx, true),
        NodeKind::Heading { level, id } => heading(node, *level, id, width, ctx),
        NodeKind::Paragraph => paragraph(node, width, ctx),
        NodeKind::BlockQuote => quote(node, width, ctx),
        NodeKind::List(info) => list(node, *info, width, ctx).0,
        // A bare item outside a list (which comrak does not produce, but a table cell
        // fragment might) renders as its content.
        NodeKind::Item | NodeKind::TaskItem { .. } | NodeKind::TableCell => {
            render_sequence(&node.children, width, ctx, true)
        }
        NodeKind::CodeBlock {
            language,
            literal,
            fenced,
            lines,
            ..
        } => code::render_code_block(
            language.as_deref(),
            literal,
            *fenced,
            lines,
            node.source,
            width,
            ctx,
        ),
        NodeKind::ThematicBreak => rule(width, ctx),
        NodeKind::Table(info) if ctx.table_depth < MAX_TABLE_DEPTH => {
            table::render_table_node(node, info, width, ctx)
        }
        NodeKind::Table(_) => render_sequence(&node.children, width, ctx, false),
        NodeKind::TableRow { .. } => render_sequence(&node.children, width, ctx, false),
        NodeKind::FootnoteDefinition { name, number } => {
            footnote(node, &footnote_label(name, *number), width, ctx)
        }
        NodeKind::Image { url, .. } => image(node, url, width, ctx),
        NodeKind::SkippedHtml { .. } => html_marker(width, ctx),
        // Anything else in a block position is inline content: a bare text run in a
        // table cell, for instance.
        _ => render_inline(std::slice::from_ref(node), width, ctx.base, ctx),
    };
    canvas.resize_width(width, ctx.base);
    canvas
}

/// The narrowest the text of a numbered heading may be squeezed to.
///
/// A `1.2.3 ` prefix costs six columns of a forty-column line, and a deep enough
/// document can make that prefix longer than the heading. Below this many columns of
/// text the number stops being orientation and starts being the thing you read, so the
/// heading drops it and keeps its words — the number is an aid, and an aid that eats
/// the content it annotates has stopped helping. The threshold is on the *text*, not
/// on the prefix, so it degrades by width rather than by number length.
const NUMBER_MIN_TEXT: u16 = 8;

/// A heading: coloured text starting at the margin, over a rule that says which level
/// it is (design spec §9), and — in a deeply nested document — its section number in
/// front of it (§9.3).
///
/// The number is a hanging marker, exactly as a list ordinal is: the heading's second
/// line wraps under its own first word rather than under the digits, so the text stays
/// a block the eye can follow and the numbers stay a column of their own.
pub(crate) fn heading(node: &Node, level: u8, id: &str, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let style = theme.heading(level);
    let marker = section_number(id, width, ctx);
    let text_width = marker.as_ref().map_or(width, |marker| {
        width - u16::try_from(marker.width()).unwrap_or(width)
    });
    let mut out = render_inline(&node.children, text_width, style, ctx);
    if out.is_empty() {
        out.push_blank_row(style);
    }
    if let Some(marker) = marker {
        out = hanging(&marker, &out, ctx.base);
    }
    out.add_anchor(Anchor {
        id: id.to_string(),
        level,
        row: 0,
    });
    if let Some(glyph) = heading_rule(level) {
        // The level-aware rule style, not `block.heading_rule`: the rule is now the
        // hierarchy signal, so it has to be tinted by the level it belongs to.
        out.push_rule(glyph, theme.heading_rule(level));
    }
    out
}

/// The section-number marker for a heading, if it has one and it fits.
///
/// `None` for an unnumbered document, for the title of a titled one, for a fragment
/// rendered outside a document (a table cell), and for a heading too narrow to carry
/// its number and still say anything — see [`NUMBER_MIN_TEXT`].
fn section_number(id: &str, width: u16, ctx: Ctx<'_>) -> Option<Line> {
    let label = ctx.numbers?.label(id)?;
    let text = format!("{label} ");
    let cost = u16::try_from(display_width(&text)).unwrap_or(u16::MAX);
    if width.saturating_sub(cost) < NUMBER_MIN_TEXT {
        return None;
    }
    Some(Line::new(vec![Span::new(text, ctx.theme.heading_number)]))
}

/// A paragraph.
///
/// A paragraph that is *nothing but* an image becomes the framed placeholder box design
/// spec §2 asks for. An image with words around it does not: it stays in the sentence as
/// a bracketed alt text, which the inline renderer draws. The distinction is taken here
/// because this is the only renderer that can see whether the image is alone — the
/// inline renderer sees one node at a time, and the block dispatcher sees an image
/// without its siblings.
fn paragraph(node: &Node, width: u16, ctx: Ctx<'_>) -> Canvas {
    if let Some(image) = sole_image(node) {
        return render_block_ctx(image, width, ctx);
    }
    render_sequence(&node.children, width, ctx, false)
}

/// The image a paragraph consists of, if an image is all it consists of.
///
/// Whitespace and the line breaks between `![a](b)` and the end of its line are not
/// content; anything else is, and makes the image part of a sentence.
fn sole_image(node: &Node) -> Option<&Node> {
    let mut image = None;
    for child in &node.children {
        match &child.kind {
            NodeKind::Image { .. } if image.is_none() => image = Some(child),
            NodeKind::Text(text) if text.trim().is_empty() => {}
            NodeKind::SoftBreak | NodeKind::LineBreak => {}
            _ => return None,
        }
    }
    image
}

/// A block quote, drawn with a coloured gutter bar that shifts hue with nesting depth.
fn quote(node: &Node, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let inner = ctx.in_quote();
    // Each level owns one bar column, tinted by its depth. The separating space is
    // spent once, by the level that actually carries text, so a chain of four quotes
    // costs five columns rather than the eight it used to — a fifth of a 40-column
    // line went on gutters alone.
    let carries_text = node
        .children
        .iter()
        .any(|child| !matches!(child.kind, NodeKind::BlockQuote));
    let gutter = if carries_text { QUOTE_GUTTER } else { 1 }.min(width);
    let content = render_sequence(&node.children, width - gutter, inner, true);
    let mut out = content.indent(gutter, 0, inner.base);
    if out.is_empty() {
        out.push_blank_row(inner.base);
    }
    if gutter == 0 {
        return out;
    }
    let bar = theme.block.quote_bar.patch(theme.accent(ctx.quote_depth));
    for row in 0..out.height() {
        out.write_str(row, 0, QUOTE_BAR, bar);
    }
    out
}

/// Columns the block-quote bar and its trailing space occupy.
const QUOTE_GUTTER: u16 = 2;

/// A bullet, ordered or task list.
///
/// # Spacing between items (design spec §9)
///
/// A list whose items each occupy a single row is drawn dense. **As soon as any one
/// item is taller than one row, a blank row is placed between every pair of items in
/// that list.** Multi-line items packed edge to edge read as one grey mass: the reader
/// cannot see where one item ends and the next begins, because the only remaining cue
/// is the marker column.
///
/// Four decisions this rule rests on, none of them free:
///
/// * **Per list, not per item.** Spacing only the items adjacent to a tall one gives
///   ragged gaps that track item length rather than structure — worse than either
///   extreme. The whole list switches together.
/// * **Composed with `CommonMark` looseness by disjunction.** A list the source already
///   made loose (blank lines between its items) is spaced by `!info.tight` alone; this
///   rule can only turn a tight list loose, never add a second blank row to a list that
///   is already spaced. Intra-item spacing stays on `info.tight` — the presentation
///   rule is about the seam *between* items and must not silently restyle the blocks
///   inside one.
/// * **"More than one line" means the rendered item is taller than one row** — a
///   wrapped paragraph, but equally a nested list, a code block, a table, several
///   paragraphs. Measuring the drawn height rather than enumerating node kinds is what
///   keeps this honest: the criterion is exactly the crowding the reader sees, and it
///   cannot fall behind as new block kinds are added. A consequence worth naming: an
///   item carrying a sublist is always taller than one row, so a list with nesting is
///   always spaced at the level that does the nesting, at every width.
/// * **Each level decides for itself.** A wrapping outer item does not force its
///   children apart; the sublist is measured on its own items and stays dense if they
///   are short. Cascading would make one long outer item blow up an entire subtree, and
///   since "wraps" is width-dependent that subtree would inflate and collapse on
///   resize. Deciding locally means the spacing appears exactly where the crowding is.
/// * **A spaced list is separated from everything it touches, not only from itself.**
///   The blank row belongs to the *seams of the list*, and a list has one more seam
///   than it has gaps between items: the one against the item that introduces it.
///   Placing the row only between items left that seam bare, which is a defect the
///   owner reported on 2026-08-09 from a five-deep nest — every level below the
///   wrapping item breathed, while the descent to it was packed solid, so the block
///   that most needed structure was the one that read as a grey mass. Since a spaced
///   list is returned to its parent's sequence *marked as set off*
///   ([`render_block_set_off`]), the row also appears on the far side, against any
///   content following the sublist inside the same item. The alternative considered
///   and rejected was to cascade *upwards* — let a wrapping item space the list that
///   contains it — which is a no-op on exactly this document: an item carrying a
///   sublist is already taller than one row, so every ancestor list was spaced
///   already. The missing rows were never the ancestors'; they were the parent-child
///   seam, and only this rule places them.
///
/// The decision is width-dependent by construction — the same list is dense at 120
/// columns and spaced at 60 — which is intended, because narrow is precisely when the
/// items wrap and look cramped. It is taken here, during layout, at a known width, and
/// never at parse time (design spec §3); the render cache is keyed on width, so a
/// resize re-renders and re-decides rather than serving a stale choice.
///
/// Returns the drawn list and whether it came out spaced, which is the answer
/// [`render_block_set_off`] passes to the sequence that owns it.
fn list(node: &Node, info: ListInfo, width: u16, ctx: Ctx<'_>) -> (Canvas, bool) {
    let inner = ctx.in_list();
    let field = marker_field(&node.children, info, ctx);
    let boxes = if info.ordered {
        task_field(&node.children, ctx)
    } else {
        0
    };
    let indent = u16::try_from(field).unwrap_or(u16::MAX).min(width);
    // Rendered up front because the spacing decision needs every item's drawn height
    // before the first item can be placed. No item is rendered twice.
    let items: Vec<Canvas> = node
        .children
        .iter()
        .map(|item| render_sequence(&item.children, width - indent, inner, !info.tight))
        .collect();
    let spaced = !info.tight || items.iter().any(|item| item.height() > 1);
    let mut out = Canvas::empty(width);
    for (index, (item, content)) in node.children.iter().zip(&items).enumerate() {
        let marker = marker_line(item, info, index, field, boxes, ctx);
        let part = hanging(&marker, content, ctx.base);
        if spaced && !out.is_empty() {
            out.push_blank_row(ctx.base);
        }
        out.append(&part, ctx.base);
    }
    (out, spaced)
}

/// The columns of air between a task checkbox and the text of its item.
///
/// **Two**, at the owner's request on 2026-08-09: "the checkbox … hugs the text
/// following it… so I guess two spaces would be in order".
///
/// When that was asked, only one of the two spaces was visible. The box was then a Nerd
/// Font pictograph, drawn across two cells while `unicode-width` measured it as one, so
/// it overlapped the column after it and ate a space. Now that the box is `[ ]`/`[x]`
/// (see [`crate::render::glyphs`]) the drawn width and the measured width agree, and
/// both spaces are actually on the screen — which is what the request asked for and had
/// not been getting.
///
/// It is deliberately wider than [`MARKER_GAP`]. A checkbox is a wide, closed shape
/// carrying a state a reader has to *read*, not a dot marking where a line begins, and
/// it sits at the head of the one list kind whose markers differ from item to item.
/// The extra column is what stops a column of boxes reading as a wall.
const TASK_GAP: usize = 2;

/// The columns a plain bullet or ordinal keeps between itself and its text.
const MARKER_GAP: usize = 1;

/// How many columns the marker column of a list occupies, including its trailing space.
///
/// An *ordered* task list spends both fields: the ordinal is the item's identity — it
/// is how the item is referred to — and the box is its state, and neither answers for
/// the other. The two are laid out side by side as `1. [x]  `, so the field is the
/// ordinal field plus the box and its gap. Keeping only the box dropped the number the
/// item is named by and left the ordinal's columns behind as a second space — a list
/// that rendered `[x]   first` where the unordered form rendered `[x]  first`.
///
/// An *unordered* list widens the same way when it holds tasks. The field is a
/// property of the list, not of the item, so a plain bullet among checkboxes is padded
/// to the identical width and every item's text starts in one column — the alternative,
/// giving the box's columns only to the items that have one, would set the text edge
/// ragged in exactly the list where the boxes make the ragging most visible.
fn marker_field(items: &[Node], info: ListInfo, ctx: Ctx<'_>) -> usize {
    if !info.ordered {
        return task_field(items, ctx).max(1 + MARKER_GAP);
    }
    let last = info.start + items.len().saturating_sub(1);
    let ordinal = display_width(&format!("{last}.")) + MARKER_GAP;
    ordinal + task_field(items, ctx)
}

/// The columns a list's checkbox column takes, or zero if it has no tasks.
fn task_field(items: &[Node], ctx: Ctx<'_>) -> usize {
    let has_task = items
        .iter()
        .any(|item| matches!(item.kind, NodeKind::TaskItem { .. }));
    if !has_task {
        return 0;
    }
    // Measured, not assumed. The boxes are `[ ]`/`[x]` in both sets and a glyph set
    // whose boxes differed would otherwise set the text ragged; measuring also means
    // this needs no hand-maintained width to drift out of step with the glyphs.
    let box_width = display_width(ctx.glyphs.task(true)).max(display_width(ctx.glyphs.task(false)));
    box_width + TASK_GAP
}

/// The marker of one list item, padded to the marker field width.
///
/// `boxes` is the checkbox column an ordered list reserves — zero unless the list has
/// task items — and it is passed in rather than derived from `item` so that a plain
/// item in a list that has tasks keeps its ordinal in the same column as its
/// neighbours'.
fn marker_line(
    item: &Node,
    info: ListInfo,
    index: usize,
    field: usize,
    boxes: usize,
    ctx: Ctx<'_>,
) -> Line {
    let theme = ctx.theme;
    let checked = match item.kind {
        NodeKind::TaskItem { checked } => Some(checked),
        _ => None,
    };
    if let Some(checked) = checked
        && !info.ordered
    {
        let style = task_style(checked, theme);
        return Line::new(vec![Span::new(
            pad_to_width(ctx.glyphs.task(checked), field, Align::Left),
            style,
        )]);
    }
    if !info.ordered {
        // The bullet glyph rotates with nesting depth, so a nested list reads as
        // nested even where the indentation alone would be ambiguous.
        return Line::new(vec![Span::new(
            pad_to_width(ctx.glyphs.bullet(ctx.list_depth), field, Align::Left),
            theme.block.list_marker,
        )]);
    }
    // A plain item in a list that also has task items is numbered in the same columns
    // as its task neighbours, so every ordinal stays in one column no matter what
    // follows it; the checkbox's columns are simply blank there.
    let mut line = Line::new(vec![Span::new(
        ordinal(info, index, field - boxes),
        theme.block.list_marker,
    )]);
    if boxes > 0 {
        // `1. [x]  `. The ordinal is the item's identity and the box is its state; the
        // box used to be drawn *instead of* the ordinal, which silently renumbered the
        // author's list to nothing.
        match checked {
            Some(checked) => line.push(Span::new(
                pad_to_width(ctx.glyphs.task(checked), boxes, Align::Left),
                task_style(checked, theme),
            )),
            None => line.push(Span::new(" ".repeat(boxes), theme.block.list_marker)),
        }
    }
    line
}

/// The style a task box is drawn in.
fn task_style(checked: bool, theme: &Theme) -> Style {
    if checked {
        theme.block.task_checked
    } else {
        theme.block.task_unchecked
    }
}

/// One item's ordinal, right-aligned in `field` columns.
///
/// The separating space always stays on the right, or the marker would touch whatever
/// comes after it.
fn ordinal(info: ListInfo, index: usize, field: usize) -> String {
    let text = format!("{}.", info.start + index);
    format!(
        "{}{}",
        pad_to_width(&text, field.saturating_sub(MARKER_GAP), Align::Right),
        " ".repeat(MARKER_GAP)
    )
}

/// A thematic break.
///
/// Deliberately *not* the full-bleed rule a heading draws: it is inset on both sides
/// and carries a centred lozenge, so a section break and a heading rule can be told
/// apart at a glance (they were the same glyph at the same width before).
fn rule(width: u16, ctx: Ctx<'_>) -> Canvas {
    let mut out = Canvas::empty(width);
    let style = ctx.theme.block.rule;
    let inset = usize::from(width) / 6;
    let span = usize::from(width).saturating_sub(2 * inset);
    if span < 3 {
        out.push_rule("─", style);
        return out;
    }
    let arm = (span - 1) / 2;
    let text = format!(
        "{}{BREAK_MARK}{}",
        repeat_to_width("─", arm),
        repeat_to_width("─", span - 1 - arm)
    );
    let row = out.push_blank_row(style);
    out.write_str(row, inset, &text, style);
    out
}

/// The lozenge centred on a thematic break.
const BREAK_MARK: &str = "◈";

/// The text between the brackets of a footnote definition's marker.
///
/// A referenced footnote is labelled with the same number its references carry, so the
/// two can be matched by eye; one nothing refers to falls back to its name, which is
/// the only handle a reader has on it.
pub(crate) fn footnote_label(name: &str, number: Option<u32>) -> String {
    number.map_or_else(|| name.to_string(), |number| number.to_string())
}

/// A footnote definition, laid out like a list item with a `[n]` marker.
fn footnote(node: &Node, label: &str, width: u16, ctx: Ctx<'_>) -> Canvas {
    let marker = Line::new(vec![Span::new(
        format!("[{label}] "),
        ctx.theme.block.footnote_label,
    )]);
    let indent = u16::try_from(marker.width()).unwrap_or(u16::MAX).min(width);
    let content = render_sequence(&node.children, width - indent, ctx, true);
    hanging(&marker, &content, ctx.base)
}

/// The narrowest budget an image still draws its frame in; below it, bare text.
pub(crate) const IMAGE_MIN_WIDTH: usize = 8;

/// Columns an image placeholder spends on chrome: two borders plus one pad each side.
pub(crate) const IMAGE_CHROME: usize = 4;

/// A framed placeholder standing in for an image (design spec §2).
fn image(node: &Node, url: &str, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let text = node.plain_text();
    let alt = text.trim();
    if usize::from(width) < IMAGE_MIN_WIDTH {
        let fallback = if alt.is_empty() { url } else { alt };
        return Canvas::from_text(width, fallback, theme.text.image_alt);
    }
    // One column of interior padding on each side, the same as a code frame and a
    // table cell: nothing in the document is welded to its own border.
    let inner_width = width - 4;
    let mut inner = Canvas::empty(inner_width);
    // The frame's label already says "image"; repeating it as the body when there is
    // no alt text says nothing twice. With no alt, the target alone is the caption.
    for line in inline::wrap(
        &[Span::new(alt, theme.text.image_alt)],
        usize::from(inner_width),
    ) {
        inner.push_line(&line, Align::Left, ctx.base);
    }
    inner.push_text_ellipsized(url, Align::Left, theme.text.link_url);
    let title = Line::styled("image", theme.block.caption);
    inner.indent(1, 1, ctx.base).framed(
        BorderSet::ROUNDED,
        theme.block.image_border,
        Some(&title),
        ctx.base,
    )
}

/// The collapsed marker that stands in for raw HTML.
fn html_marker(width: u16, ctx: Ctx<'_>) -> Canvas {
    Canvas::from_text(width, HTML_MARKER, ctx.theme.text.dim)
}

/// Places `marker` in front of `content`, indenting every following row to match.
///
/// This is the hanging indent shared by list items, footnote definitions and
/// headings; `content` must already have been rendered at the reduced width.
///
/// The indent is the marker's full width, **even when the marker is wider than the
/// content beside it** — a `1.1.1.1.1.1 ` section number at twenty columns, or a long
/// footnote label at any width. It used to be clamped to the content's width, on the
/// grounds that a pathological label would otherwise allocate a canvas wider than the
/// caller was ever going to keep; what the clamp actually did was slide the content
/// left *under* the marker, so the marker overwrote its first few columns and the row
/// read `1.1.1.1.1.1 ion`. Every caller already sizes its marker against the width
/// before rendering the content at what is left, which is the right place to stop the
/// pathological case and the only place that knows the width.
pub(crate) fn hanging(marker: &Line, content: &Canvas, fill: Style) -> Canvas {
    let indent = u16::try_from(marker.width()).unwrap_or(u16::MAX);
    let mut out = content.indent(indent, 0, fill);
    if out.is_empty() {
        out.push_blank_row(fill);
    }
    out.write_line(0, 0, marker, fill);
    out
}
