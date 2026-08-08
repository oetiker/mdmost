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
use crate::text::{Align, Line, Span, display_width, pad_to_width};
use crate::theme::{Style, Theme};

use super::inline::{HTML_MARKER, render_inline};
use super::{Ctx, MAX_TABLE_DEPTH, code, inline, table};

/// The glyph drawn in front of a heading, indexed by level `1..=6`.
///
/// Plain Unicode rather than Nerd Font glyphs: icon selection is configuration's job
/// (`--no-icons`, design spec §9) and the renderer has no access to the config.
const HEADING_PREFIX: [&str; 6] = ["◆", "◈", "▸", "▹", "•", "·"];

/// The glyph a bullet list item is marked with, by nesting depth.
const BULLETS: [&str; 4] = ["•", "◦", "‣", "·"];

/// The vertical bar drawn to the left of a block quote.
const QUOTE_BAR: &str = "▌";

/// The box of a ticked task list item.
const TASK_CHECKED: &str = "☑";

/// The box of an unticked task list item.
const TASK_UNCHECKED: &str = "☐";

/// The rule drawn beneath a heading, by level.
const HEADING_RULE: [&str; 2] = ["━", "─"];

/// Renders one block at `width` columns.
///
/// The result is exactly `width` columns wide, and empty for a node that renders to
/// nothing.
pub fn render_block(node: &Node, width: u16, theme: &Theme) -> Canvas {
    render_block_ctx(node, width, Ctx::new(theme))
}

/// Renders a sequence of blocks at `width` columns, separated by blank rows.
pub fn render_blocks(nodes: &[Node], width: u16, theme: &Theme) -> Canvas {
    render_sequence(nodes, width, Ctx::new(theme), true)
}

/// Renders a sequence of sibling blocks.
///
/// `spaced` inserts one blank row between blocks, which is what separates paragraphs
/// in a document and what a *tight* list suppresses between its items.
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
/// An image is deliberately *not* inline: it becomes a framed placeholder box
/// (design spec §2). Inline HTML is, so that its collapsed marker stays in the
/// sentence it was dropped from.
fn is_inline(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::Text(_)
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
        NodeKind::FootnoteDefinition { name } => footnote(node, name, width, ctx),
        NodeKind::Image { url, .. } => image(node, url, width, ctx),
        NodeKind::SkippedHtml { .. } => html_marker(width, ctx),
        // Anything else in a block position is inline content: a bare text run in a
        // table cell, for instance.
        _ => render_inline(std::slice::from_ref(node), width, ctx.base, ctx),
    };
    canvas.resize_width(width, ctx.base);
    canvas
}

/// A heading: prefix glyph, coloured text, and a rule under levels 1 and 2.
fn heading(node: &Node, level: u8, id: &str, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let style = theme.heading(level);
    let prefix = HEADING_PREFIX[usize::from(level.clamp(1, 6)) - 1];
    let marker = Line::new(vec![Span::new(
        format!("{prefix} "),
        theme.block.heading_prefix,
    )]);
    let indent = u16::try_from(marker.width()).unwrap_or(0).min(width);
    let text = render_inline(&node.children, width - indent, style, ctx);
    let mut out = hanging(&marker, &text, style);
    if out.is_empty() {
        out.push_blank_row(style);
    }
    out.add_anchor(Anchor {
        id: id.to_string(),
        level,
        row: 0,
    });
    if theme.heading_has_rule(level) {
        let glyph = HEADING_RULE[usize::from(level.clamp(1, 2)) - 1];
        out.push_rule(glyph, theme.block.heading_rule);
    }
    out
}

/// A paragraph.
///
/// An image that is a direct child of the paragraph becomes a framed placeholder box
/// of its own (design spec §2), splitting the paragraph around it; images nested
/// deeper degrade to their alt text, which the inline renderer handles.
fn paragraph(node: &Node, width: u16, ctx: Ctx<'_>) -> Canvas {
    render_sequence(&node.children, width, ctx, false)
}

/// A block quote, drawn with a coloured gutter bar that shifts hue with nesting depth.
fn quote(node: &Node, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let gutter = 2u16.min(width);
    let inner = ctx.in_quote();
    let content = render_sequence(&node.children, width - gutter, inner, true);
    let mut out = content.indent(gutter, 0, inner.base);
    if out.is_empty() {
        out.push_blank_row(inner.base);
    }
    // The bar hue rotates with nesting depth so nested quotes are distinguishable.
    let bar = theme.block.quote_bar.patch(theme.accent(ctx.quote_depth));
    for row in 0..out.height() {
        out.write_str(row, 0, QUOTE_BAR, bar);
    }
    out
}

/// A bullet, ordered or task list.
fn list(node: &Node, info: ListInfo, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let inner = ctx.in_list();
    let field = marker_field(&node.children, info);
    let indent = u16::try_from(field).unwrap_or(u16::MAX).min(width);
    let mut out = Canvas::empty(width);
    for (index, item) in node.children.iter().enumerate() {
        let marker = marker_line(item, info, index, field, ctx.list_depth, theme);
        let content = render_sequence(&item.children, width - indent, inner, !info.tight);
        let part = hanging(&marker, &content, ctx.base);
        if !info.tight && !out.is_empty() {
            out.push_blank_row(ctx.base);
        }
        out.append(&part, ctx.base);
    }
    out
}

/// How many columns the marker column of a list occupies, including its trailing space.
fn marker_field(items: &[Node], info: ListInfo) -> usize {
    if !info.ordered {
        return 2;
    }
    let last = info.start + items.len().saturating_sub(1);
    display_width(&format!("{last}.")) + 1
}

/// The marker of one list item, padded to the marker field width.
fn marker_line(
    item: &Node,
    info: ListInfo,
    index: usize,
    field: usize,
    depth: usize,
    theme: &Theme,
) -> Line {
    if let NodeKind::TaskItem { checked } = item.kind {
        let (glyph, style) = if checked {
            (TASK_CHECKED, theme.block.task_checked)
        } else {
            (TASK_UNCHECKED, theme.block.task_unchecked)
        };
        return Line::new(vec![Span::new(
            pad_to_width(glyph, field, Align::Left),
            style,
        )]);
    }
    let text = if info.ordered {
        // The ordinal is right-aligned in the field, but the separating space always
        // stays on the right, or the marker would touch the text.
        let ordinal = format!("{}.", info.start + index);
        format!(
            "{} ",
            pad_to_width(&ordinal, field.saturating_sub(1), Align::Right)
        )
    } else {
        // The bullet glyph rotates with nesting depth, so a nested list reads as
        // nested even where the indentation alone would be ambiguous.
        pad_to_width(BULLETS[depth % BULLETS.len()], field, Align::Left)
    };
    Line::new(vec![Span::new(text, theme.block.list_marker)])
}

/// A thematic break.
fn rule(width: u16, ctx: Ctx<'_>) -> Canvas {
    let mut out = Canvas::empty(width);
    out.push_rule("─", ctx.theme.block.rule);
    out
}

/// A footnote definition, laid out like a list item with a `[n]` marker.
fn footnote(node: &Node, name: &str, width: u16, ctx: Ctx<'_>) -> Canvas {
    let marker = Line::new(vec![Span::new(
        format!("[{name}] "),
        ctx.theme.block.footnote_label,
    )]);
    let indent = u16::try_from(marker.width()).unwrap_or(u16::MAX).min(width);
    let content = render_sequence(&node.children, width - indent, ctx, true);
    hanging(&marker, &content, ctx.base)
}

/// A framed placeholder standing in for an image (design spec §2).
fn image(node: &Node, url: &str, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let text = node.plain_text();
    let alt = if text.trim().is_empty() {
        "image"
    } else {
        text.as_str()
    };
    if width < 6 {
        return Canvas::from_text(width, alt, theme.text.image_alt);
    }
    let inner_width = width - 2;
    let mut inner = Canvas::empty(inner_width);
    for line in inline::wrap(
        &[Span::new(alt, theme.text.image_alt)],
        usize::from(inner_width),
    ) {
        inner.push_line(&line, Align::Left, ctx.base);
    }
    inner.push_text_ellipsized(url, Align::Left, theme.text.link_url);
    let title = Line::styled("image", theme.block.caption);
    inner.framed(
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
    let indent = u16::try_from(marker.width()).unwrap_or(u16::MAX);
    let mut out = content.indent(indent, 0, fill);
    if out.is_empty() {
        out.push_blank_row(fill);
    }
    out.write_line(0, 0, marker, fill);
    out
}
