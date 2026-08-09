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
use crate::text::{Align, Line, Span, display_width, pad_to_width, repeat_to_width};
use crate::theme::{Style, Theme};

use super::inline::{HTML_MARKER, render_inline};
use super::{Ctx, MAX_TABLE_DEPTH, RenderOptions, code, inline, table};

/// The vertical bar drawn to the left of a block quote.
const QUOTE_BAR: &str = "▌";

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

/// Renders a sequence of blocks at `width` columns, separated by blank rows.
pub fn render_blocks(nodes: &[Node], width: u16, theme: &Theme, options: &RenderOptions) -> Canvas {
    render_sequence(nodes, width, Ctx::new(theme, options), true)
}

/// Renders a sequence of sibling blocks.
///
/// `spaced` inserts one blank row between blocks, which is what separates paragraphs
/// in a document and what a *tight* list suppresses inside one of its items. The
/// spacing *between* list items is not decided here — see [`list`].
pub(crate) fn render_sequence(nodes: &[Node], width: u16, ctx: Ctx<'_>, spaced: bool) -> Canvas {
    let fill = ctx.base;
    let mut out = Canvas::empty(width);
    let push = |part: &Canvas, out: &mut Canvas| {
        if part.is_empty() {
            return;
        }
        if spaced && !out.is_empty() {
            out.push_blank_row(fill);
        }
        out.append(part, fill);
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
                &mut out,
            );
        } else {
            push(&render_block_ctx(&nodes[index], width, ctx), &mut out);
            index += 1;
        }
    }
    out
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
        NodeKind::List(info) => list(node, *info, width, ctx),
        // A bare item outside a list (which comrak does not produce, but a table cell
        // fragment might) renders as its content.
        NodeKind::Item | NodeKind::TaskItem { .. } | NodeKind::TableCell => {
            render_sequence(&node.children, width, ctx, true)
        }
        NodeKind::CodeBlock {
            language,
            literal,
            fenced,
            ..
        } => code::render_code_block(language.as_deref(), literal, *fenced, width, ctx),
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

/// A heading: coloured text starting at the margin, over a rule that says which level
/// it is (design spec §9).
pub(crate) fn heading(node: &Node, level: u8, id: &str, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let style = theme.heading(level);
    let mut out = render_inline(&node.children, width, style, ctx);
    if out.is_empty() {
        out.push_blank_row(style);
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
///
/// The decision is width-dependent by construction — the same list is dense at 120
/// columns and spaced at 60 — which is intended, because narrow is precisely when the
/// items wrap and look cramped. It is taken here, during layout, at a known width, and
/// never at parse time (design spec §3); the render cache is keyed on width, so a
/// resize re-renders and re-decides rather than serving a stale choice.
fn list(node: &Node, info: ListInfo, width: u16, ctx: Ctx<'_>) -> Canvas {
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
    out
}

/// How many columns the marker column of a list occupies, including its trailing space.
///
/// An *ordered* task list spends both fields: the ordinal is the item's identity — it
/// is how the item is referred to — and the box is its state, and neither answers for
/// the other. The two are laid out side by side as `1. ☑ `, so the field is the ordinal
/// field plus the box and its space.
fn marker_field(items: &[Node], info: ListInfo, ctx: Ctx<'_>) -> usize {
    if !info.ordered {
        return 2;
    }
    let last = info.start + items.len().saturating_sub(1);
    let ordinal = display_width(&format!("{last}.")) + 1;
    ordinal + task_field(items, ctx)
}

/// The columns an ordered list's checkbox column takes, or zero if it has no tasks.
fn task_field(items: &[Node], ctx: Ctx<'_>) -> usize {
    let has_task = items
        .iter()
        .any(|item| matches!(item.kind, NodeKind::TaskItem { .. }));
    if !has_task {
        return 0;
    }
    // Both boxes are the same width in both glyph sets, but measuring beats assuming:
    // an icon set whose boxes differed would otherwise set the text ragged.
    let box_width = display_width(ctx.glyphs.task(true)).max(display_width(ctx.glyphs.task(false)));
    box_width + 1
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
    // The ordinal is right-aligned in its own field, but the separating space always
    // stays on the right, or the marker would touch what follows it.
    let ordinal = format!("{}.", info.start + index);
    let mut line = Line::new(vec![Span::new(
        format!(
            "{} ",
            pad_to_width(&ordinal, field.saturating_sub(boxes + 1), Align::Right)
        ),
        theme.block.list_marker,
    )]);
    if boxes > 0 {
        // `1. ☑ `. The ordinal is the item's identity and the box is its state; the
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
pub(crate) fn hanging(marker: &Line, content: &Canvas, fill: Style) -> Canvas {
    // Clamped, as `heading` and `list` already clamp their own marker fields: a
    // pathological footnote label would otherwise allocate a canvas that wide before
    // the caller resizes it back.
    let indent = u16::try_from(marker.width())
        .unwrap_or(u16::MAX)
        .min(content.width());
    let mut out = content.indent(indent, 0, fill);
    if out.is_empty() {
        out.push_blank_row(fill);
    }
    out.write_line(0, 0, marker, fill);
    out
}
