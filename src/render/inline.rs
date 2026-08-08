//! Inline spans to styled, wrapped lines.
//!
//! Inline content is flattened into [`Span`]s carrying semantic styles, wrapped by
//! [`crate::text::wrap_spans`] — the single wrapping implementation in `mdless` — and
//! written onto a [`Canvas`] that is exactly the requested width.
//!
//! While flattening, every run remembers the byte range of the source it came from.
//! After wrapping, the rendered graphemes are reconciled against that annotated stream
//! so the canvas can carry [`SearchSpan`]s mapping source offsets to `(row, col)`.
//! Composition (`blit`, `append`, `indent`, `hconcat`) translates those spans for
//! free, so no other module in `render` does offset arithmetic.

use crate::canvas::{Canvas, SearchSpan};
use crate::doc::{Node, NodeKind, SourceSpan};
use crate::text::{Line, Span, display_width, grapheme_width, graphemes, wrap_spans};
use crate::theme::Style;

use super::Ctx;

/// The marker shown in place of raw HTML, which `mdless` never renders (spec §2).
pub(crate) const HTML_MARKER: &str = "⟨html⟩";

/// Wraps styled runs into lines of at most `width` display columns.
///
/// This is the interface named in design spec §5. It delegates to
/// [`crate::text::wrap_spans`]; there is deliberately no second wrapping
/// implementation in the crate.
pub fn wrap(spans: &[Span], width: usize) -> Vec<Line> {
    wrap_spans(spans, width)
}

/// A styled run together with the source bytes it was produced from.
#[derive(Debug, Clone)]
struct Piece {
    /// The text of the run.
    text: String,
    /// The style the run is drawn in, overlaid on whatever is underneath.
    style: Style,
    /// Byte offset in the document source of `text[0]`, when the run maps to the
    /// source one byte at a time. `None` for synthesised text such as link targets.
    origin: Option<usize>,
}

impl Piece {
    /// A run with no source of its own.
    fn synthetic(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            origin: None,
        }
    }

    /// A run that reproduces `source` verbatim, if the lengths agree.
    fn anchored(text: String, style: Style, source: SourceSpan) -> Self {
        let origin = (source.len() == text.len()).then_some(source.start);
        Self {
            text,
            style,
            origin,
        }
    }
}

/// One grapheme cluster of the flattened inline stream, with its source offset.
#[derive(Debug, Clone, Copy)]
struct Anchored<'a> {
    text: &'a str,
    origin: Option<usize>,
}

/// Flattens inline nodes into styled runs.
pub(crate) fn inline_spans(nodes: &[Node], ctx: Ctx<'_>) -> Vec<Span> {
    pieces(nodes, ctx)
        .into_iter()
        .map(|piece| Span::new(piece.text, piece.style))
        .collect()
}

/// Renders inline nodes into a left-aligned canvas of exactly `width` columns.
///
/// `base` is the style the canvas is filled with; span styles are overlaid on it, so
/// a block quote or a heading simply passes its own base style down.
pub(crate) fn render_inline(nodes: &[Node], width: u16, base: Style, ctx: Ctx<'_>) -> Canvas {
    let mut pieces = pieces(nodes, ctx);
    trim_edges(&mut pieces);
    render_pieces(&pieces, width, base)
}

/// Drops whitespace at the two ends of an inline run.
///
/// A run is a paragraph, a heading or the text either side of an image, and leading
/// or trailing spaces there are an artefact of where the run was cut, not content.
/// Source origins are shifted by what was removed so search spans stay exact.
fn trim_edges(pieces: &mut Vec<Piece>) {
    while let Some(first) = pieces.first_mut() {
        let trimmed = first.text.trim_start();
        let removed = first.text.len() - trimmed.len();
        if removed > 0 {
            first.origin = first.origin.map(|start| start + removed);
            first.text = trimmed.to_string();
        }
        if first.text.is_empty() {
            pieces.remove(0);
        } else {
            break;
        }
    }
    while let Some(last) = pieces.last_mut() {
        last.text.truncate(last.text.trim_end().len());
        if last.text.is_empty() {
            pieces.pop();
        } else {
            break;
        }
    }
}

/// Renders already-flattened runs, recording search spans.
fn render_pieces(pieces: &[Piece], width: u16, base: Style) -> Canvas {
    let spans: Vec<Span> = pieces
        .iter()
        .map(|piece| Span::new(piece.text.clone(), piece.style))
        .collect();
    let lines = wrap(&spans, usize::from(width));
    let mut canvas = Canvas::new(width, lines.len(), base);
    for (row, line) in lines.iter().enumerate() {
        canvas.write_line(row, 0, line, base);
    }
    for span in reconcile(&lines, &flatten(pieces)) {
        canvas.add_span(span);
    }
    canvas
}

/// Splits the runs into per-grapheme entries carrying absolute source offsets.
fn flatten(pieces: &[Piece]) -> Vec<Anchored<'_>> {
    let mut out = Vec::new();
    for piece in pieces {
        let mut offset = 0usize;
        for cluster in graphemes(&piece.text) {
            out.push(Anchored {
                text: cluster,
                origin: piece.origin.map(|start| start + offset),
            });
            offset += cluster.len();
        }
    }
    out
}

/// Maps the graphemes of the wrapped lines back onto the annotated input stream.
///
/// Wrapping preserves grapheme order and only ever *drops* clusters (whitespace at a
/// break, a double-width cluster that cannot fit a one-column budget), so the output
/// is a subsequence of the input and a single forward cursor suffices.
fn reconcile(lines: &[Line], flat: &[Anchored<'_>]) -> Vec<SearchSpan> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for (row, line) in lines.iter().enumerate() {
        let text = line.text();
        let mut col = 0u16;
        let mut run: Option<SearchSpan> = None;
        for cluster in graphemes(&text) {
            while cursor < flat.len() && flat[cursor].text != cluster {
                cursor += 1;
            }
            let origin = flat.get(cursor).and_then(|entry| entry.origin);
            cursor = cursor.saturating_add(1);
            let cols = u16::from(grapheme_width(cluster));
            match (origin, run.as_mut()) {
                (Some(start), Some(current))
                    if current.source_end == start && current.col + current.cols == col =>
                {
                    current.source_end = start + cluster.len();
                    current.cols += cols;
                }
                (Some(start), _) => {
                    out.extend(run.take());
                    run = Some(SearchSpan {
                        source_start: start,
                        source_end: start + cluster.len(),
                        row,
                        col,
                        cols,
                    });
                }
                (None, _) => out.extend(run.take()),
            }
            col = col.saturating_add(cols);
        }
        out.extend(run.take());
    }
    out
}

/// Flattens inline nodes into runs, resolving semantic styles.
fn pieces(nodes: &[Node], ctx: Ctx<'_>) -> Vec<Piece> {
    let mut out = Vec::new();
    collect(nodes, Style::NONE, ctx, &mut out);
    out
}

/// Recursively flattens `nodes`, with `style` inherited from the enclosing markup.
fn collect(nodes: &[Node], style: Style, ctx: Ctx<'_>, out: &mut Vec<Piece>) {
    let theme = ctx.theme;
    for node in nodes {
        match &node.kind {
            NodeKind::Text(text) => {
                out.push(Piece::anchored(text.clone(), style, node.source));
            }
            NodeKind::SoftBreak => out.push(Piece::synthetic(" ", style)),
            NodeKind::LineBreak => out.push(Piece::synthetic("\n", style)),
            NodeKind::Code { literal } => {
                out.push(code_piece(
                    literal,
                    style.patch(theme.text.code),
                    node.source,
                ));
            }
            NodeKind::Emph => collect(&node.children, style.patch(theme.text.emphasis), ctx, out),
            NodeKind::Strong => collect(&node.children, style.patch(theme.text.strong), ctx, out),
            NodeKind::Strikethrough => collect(
                &node.children,
                style.patch(theme.text.strikethrough),
                ctx,
                out,
            ),
            NodeKind::Link { url, .. } => link(node, url, style, ctx, out),
            // A nested image (inside a link, a heading, …) degrades to its alt text;
            // an image that is a direct child of a paragraph becomes a framed
            // placeholder box instead, which the block renderer handles.
            NodeKind::Image { .. } => {
                collect(&node.children, style.patch(theme.text.image_alt), ctx, out)
            }
            NodeKind::FootnoteReference { number, .. } => {
                out.push(Piece::synthetic(
                    format!("[{number}]"),
                    style.patch(theme.text.footnote_ref),
                ));
            }
            // One marker per inline run is enough to say "HTML was dropped here";
            // `<b>x</b>` would otherwise bracket its own text with two of them.
            NodeKind::SkippedHtml { .. } => {
                if !out.iter().any(|piece| piece.text == HTML_MARKER) {
                    out.push(Piece::synthetic(HTML_MARKER, style.patch(theme.text.dim)));
                }
            }
            // Any other node appearing in an inline position is a container we do not
            // style specially; its children still render.
            _ => collect(&node.children, style, ctx, out),
        }
    }
}

/// A code span, anchored past the opening backtick fence when that is unambiguous.
fn code_piece(literal: &str, style: Style, source: SourceSpan) -> Piece {
    let padding = source.len().checked_sub(literal.len());
    let origin = match padding {
        Some(extra) if extra.is_multiple_of(2) => Some(source.start + extra / 2),
        _ => None,
    };
    Piece {
        text: literal.to_string(),
        style,
        origin,
    }
}

/// Renders a link as its text, followed by a dim target when the two differ.
fn link(node: &Node, url: &str, style: Style, ctx: Ctx<'_>, out: &mut Vec<Piece>) {
    let theme = ctx.theme;
    let before = out.len();
    collect(&node.children, style.patch(theme.text.link), ctx, out);
    let text: String = out[before..]
        .iter()
        .map(|piece| piece.text.as_str())
        .collect();
    // An autolink renders its own target as its text; showing it twice is noise.
    if text.trim() == url.trim() || url.is_empty() {
        return;
    }
    out.push(Piece::synthetic(
        format!(" ({url})"),
        style.patch(theme.text.link_url),
    ));
}

/// The display width of inline content when it is not wrapped at all.
pub(crate) fn natural_width(nodes: &[Node], ctx: Ctx<'_>) -> usize {
    let spans = inline_spans(nodes, ctx);
    spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>()
        .split('\n')
        .map(display_width)
        .max()
        .unwrap_or(0)
}

/// The narrowest width inline content can be wrapped to without splitting a word.
pub(crate) fn min_width(nodes: &[Node], ctx: Ctx<'_>) -> usize {
    crate::text::spans_min_width(&inline_spans(nodes, ctx))
}
