//! Inline spans to styled, wrapped lines.
//!
//! Inline content is flattened into [`Span`]s carrying semantic styles, wrapped by
//! [`crate::text::wrap_spans`] — the single wrapping implementation in `mdmost` — and
//! written onto a [`Canvas`] that is exactly the requested width.
//!
//! While flattening, every run remembers the byte range of the source it came from.
//! After wrapping, the rendered graphemes are reconciled against that annotated stream
//! so the canvas can carry [`SearchSpan`]s mapping source offsets to `(row, col)`.
//! Composition (`blit`, `append`, `indent`, `hconcat`) translates those spans for
//! free, so no other module in `render` does offset arithmetic.

use crate::canvas::{Canvas, Hotspot, HotspotKind, SearchSpan};
use crate::doc::{Node, NodeKind, SourceSpan};
use crate::text::{Line, Span, display_width, graphemes, wrap_spans};
use crate::theme::Style;

use super::Ctx;

/// The marker shown in place of raw HTML, which `mdmost` never renders (spec §2).
pub(crate) const HTML_MARKER: &str = "⟨html⟩";

/// Wraps styled runs into lines of at most `width` display columns.
///
/// This is the interface named in design spec §5. It delegates to
/// [`crate::text::wrap_spans`]; there is deliberately no second wrapping
/// implementation in the crate.
pub fn wrap(spans: &[Span], width: usize) -> Vec<Line> {
    wrap_spans(spans, width)
}

/// Where the text of a run came from in the document source.
#[derive(Debug, Clone, Copy)]
enum Origin {
    /// The run reproduces the source byte for byte from this offset, so every cluster
    /// of it can be given an offset of its own.
    Copied(usize),
    /// The whole run — exactly one grapheme, exactly one column — was *transcribed*
    /// from these bytes: `&amp;` drawing an `&`. Nothing in it copies anything, so it
    /// maps as an indivisible unit and never joins a neighbouring run.
    Transcribed(SourceSpan),
}

/// The control a piece belongs to, if any.
///
/// Parallel to [`Origin`], and deliberately not part of it: `Origin` answers "which
/// source bytes did this draw", and a link's printed ` (url)` suffix belongs to the
/// control while belonging to no source bytes at all (design spec §2.1). That is the
/// one place a [`Hotspot`] and a [`SearchSpan`] are allowed to disagree, and giving
/// the suffix a span instead would break the rule that a span's source is a
/// byte-for-byte copy of the cells it names.
#[derive(Debug, Clone)]
struct Control {
    /// Which control — one id per link, shared by all of its pieces.
    ///
    /// Unique within one flattened inline run, which is all the uniqueness needed:
    /// [`render_pieces`] rebases these onto the canvas's own numbering before the
    /// hotspots are recorded, and every composition that merges two canvases rebases
    /// them again.
    id: usize,
    /// What activating it does.
    kind: HotspotKind,
}

/// A styled run together with the source bytes it was produced from.
#[derive(Debug, Clone)]
struct Piece {
    /// The text of the run.
    text: String,
    /// The style the run is drawn in, overlaid on whatever is underneath.
    style: Style,
    /// What in the source drew this run. `None` for synthesised text such as link
    /// targets, and for anything whose mapping is not knowable.
    origin: Option<Origin>,
    /// The control this run is part of, if it is part of one.
    control: Option<Control>,
}

impl Piece {
    /// A run with no source of its own.
    fn synthetic(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            origin: None,
            control: None,
        }
    }

    /// A run that reproduces `source` verbatim, if the lengths agree.
    fn anchored(text: String, style: Style, source: SourceSpan) -> Self {
        let origin = (source.len() == text.len()).then_some(Origin::Copied(source.start));
        Self {
            text,
            style,
            origin,
            control: None,
        }
    }

    /// A run of document text, which may be a transcription rather than a copy.
    ///
    /// [`crate::doc`] has already cut every text node at its escapes and character
    /// references, so a run whose lengths still disagree is one of two things: a
    /// transcription — one character drawn from a `&…;` that copies none of it — or a
    /// run whose alignment was declined, which keeps no origin at all.
    ///
    /// A transcription is anchored only while it draws **one column**. That is the
    /// only case with no interior position: a span's source is otherwise a
    /// byte-for-byte copy of the cells it names, which is what lets `select` and
    /// `search` convert between bytes and columns inside it, and a two-column
    /// transcription would hand that arithmetic a body it cannot walk. Wider ones —
    /// an emoji reference — decline and stay dark, one cell wide.
    fn transcribable(text: String, style: Style, source: SourceSpan) -> Self {
        if source.len() == text.len() {
            return Self {
                text,
                style,
                origin: Some(Origin::Copied(source.start)),
                control: None,
            };
        }
        // One grapheme drawing one column: the two halves of "this run has no interior
        // position", which is what makes a span whose source is not a copy of its text
        // safe for the column arithmetic in `select` and `search`.
        let clusters: Vec<&str> = graphemes(&text).collect();
        let single = matches!(clusters[..], [only] if display_width(only) == 1);
        let origin = (single && !source.is_empty()).then_some(Origin::Transcribed(source));
        Self {
            text,
            style,
            origin,
            control: None,
        }
    }
}

/// One grapheme cluster of the flattened inline stream, with the source that drew it.
#[derive(Debug, Clone, Copy)]
struct Anchored<'a> {
    text: &'a str,
    /// The bytes this one cluster was drawn from, if they are known.
    source: Option<SourceSpan>,
    /// Whether those bytes are a verbatim copy of the cluster. A transcribed cluster
    /// is a span of its own; see [`reconcile`].
    copied: bool,
    /// The control this cluster is part of, borrowed from the piece it came from.
    ///
    /// Borrowed rather than cloned so this stays [`Copy`], which is what the single
    /// forward cursor in [`reconcile`] walks with.
    control: Option<&'a Control>,
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
            first.origin = match first.origin {
                Some(Origin::Copied(start)) => Some(Origin::Copied(start + removed)),
                // A transcription maps as a unit, so a partial trim would leave it
                // claiming bytes for text it no longer draws. Defensive: it is one
                // grapheme, so trimming it is all-or-nothing and the piece below is
                // dropped whole — a `&nbsp;` opening a paragraph is that case.
                Some(Origin::Transcribed(_)) | None => None,
            };
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
    let mut canvas = Canvas::from_lines(width, &lines, base);
    let (found, spots) = reconcile(&lines, &flatten(pieces));
    for span in found {
        canvas.add_span(span);
    }
    // The ids `link` issued number the controls of *this* inline run from zero; the
    // canvas numbers its own from zero too. Rebasing here — the same remap
    // `Canvas::merge_hotspots` does, for the same reason — keeps `Hotspot::target`
    // meaning "unique on the canvas that carries it" everywhere, so `link` never has to
    // know what else is drawn beside it. Distinct source ids get distinct new ones and
    // the rows of one wrapped link keep sharing theirs.
    let mut rebased = std::collections::HashMap::new();
    for mut spot in spots {
        spot.target = *rebased
            .entry(spot.target)
            .or_insert_with(|| canvas.next_target());
        canvas.add_hotspot(spot);
    }
    canvas
}

/// Splits the runs into per-grapheme entries carrying absolute source ranges.
fn flatten(pieces: &[Piece]) -> Vec<Anchored<'_>> {
    let mut out = Vec::new();
    for piece in pieces {
        let clusters: Vec<&str> = graphemes(&piece.text).collect();
        let control = piece.control.as_ref();
        match piece.origin {
            Some(Origin::Copied(start)) => {
                let mut offset = 0usize;
                for cluster in clusters {
                    out.push(Anchored {
                        text: cluster,
                        source: Some(SourceSpan::new(
                            start + offset,
                            start + offset + cluster.len(),
                        )),
                        copied: true,
                        control,
                    });
                    offset += cluster.len();
                }
            }
            // A transcription is one grapheme, which is [`Piece::transcribable`]'s
            // decision to make and the only shape the bytes can be given to whole.
            Some(Origin::Transcribed(source)) => {
                debug_assert_eq!(clusters.len(), 1, "a transcription draws one cluster");
                out.extend(
                    clusters
                        .into_iter()
                        .enumerate()
                        .map(|(index, cluster)| Anchored {
                            text: cluster,
                            source: (index == 0).then_some(source),
                            copied: false,
                            control,
                        }),
                );
            }
            None => out.extend(clusters.into_iter().map(|cluster| Anchored {
                text: cluster,
                source: None,
                copied: false,
                control,
            })),
        }
    }
    out
}

/// Maps the graphemes of the wrapped lines back onto the annotated input stream.
///
/// Wrapping preserves grapheme order and only ever *drops* clusters (whitespace at a
/// break, a double-width cluster that cannot fit a one-column budget), so the output
/// is a subsequence of the input and a single forward cursor suffices.
fn reconcile(lines: &[Line], flat: &[Anchored<'_>]) -> (Vec<SearchSpan>, Vec<Hotspot>) {
    let mut out = Vec::new();
    let mut spots = Vec::new();
    let mut cursor = 0usize;
    for (row, line) in lines.iter().enumerate() {
        let text = line.text();
        let mut col = 0u16;
        let mut run: Option<SearchSpan> = None;
        // A control run grows while the cluster belongs to the SAME control and stays
        // column-contiguous, and closes at the end of a row exactly as a search run
        // does. That is what makes a wrapped link several hotspots sharing one target
        // — the shape design spec §2.2 asks for, for free.
        let mut control: Option<Hotspot> = None;
        for cluster in graphemes(&text) {
            while cursor < flat.len() && flat[cursor].text != cluster {
                cursor += 1;
            }
            let entry = flat.get(cursor).copied();
            cursor = cursor.saturating_add(1);
            // The cluster must be charged the columns it actually draws, not the
            // one-or-two a single cell can hold: `Canvas::write_str` splits a cluster
            // wider than a cell across several cells, so a clamped width here would
            // walk the cursor left of the text it is describing and drag every
            // following search span with it.
            let cols = u16::try_from(display_width(cluster)).unwrap_or(u16::MAX);
            let span = |source: SourceSpan| SearchSpan {
                source_start: source.start,
                source_end: source.end,
                unit: None,
                row,
                col,
                cols,
            };
            match entry.and_then(|entry| entry.source.map(|source| (source, entry.copied))) {
                // A run grows only while it stays a byte-for-byte copy of the cells it
                // names — `select` and `search` both convert between bytes and columns
                // inside a span by walking its source. A transcribed cluster is
                // therefore a span of its own, and closes the run either side of it.
                Some((source, false)) => {
                    out.extend(run.take());
                    out.push(span(source));
                }
                Some((source, true)) => match run.as_mut() {
                    Some(current)
                        if current.source_end == source.start
                            && current.col + current.cols == col =>
                    {
                        current.source_end = source.end;
                        current.cols += cols;
                    }
                    _ => {
                        out.extend(run.take());
                        run = Some(span(source));
                    }
                },
                None => out.extend(run.take()),
            }
            match entry.and_then(|entry| entry.control) {
                Some(Control { id, kind }) => match control.as_mut() {
                    Some(open) if open.target == *id && open.col + open.cols == col => {
                        open.cols += cols;
                    }
                    _ => {
                        spots.extend(control.take());
                        control = Some(Hotspot {
                            row,
                            col,
                            cols,
                            kind: kind.clone(),
                            target: *id,
                        });
                    }
                },
                None => spots.extend(control.take()),
            }
            col = col.saturating_add(cols);
        }
        out.extend(run.take());
        spots.extend(control.take());
    }
    (out, spots)
}

/// Flattens inline nodes into runs, resolving semantic styles.
fn pieces(nodes: &[Node], ctx: Ctx<'_>) -> Vec<Piece> {
    let mut out = Vec::new();
    // The control counter lives here rather than on `Ctx` because `Ctx` is `Copy`: a
    // counter carried by value would hand every nested inline subtree its own numbering
    // and every link in the run would collide on id 0, which is exactly what a wrapped
    // link's shared target id would then no longer distinguish. A `&mut usize` walked
    // down the recursion is one counter by construction.
    let mut ids = 0usize;
    collect(nodes, Style::NONE, ctx, &mut ids, &mut out);
    out
}

/// Recursively flattens `nodes`, with `style` inherited from the enclosing markup.
///
/// `ids` issues one control id per link met, unique within this run; see [`Control`].
fn collect(nodes: &[Node], style: Style, ctx: Ctx<'_>, ids: &mut usize, out: &mut Vec<Piece>) {
    let theme = ctx.theme;
    for node in nodes {
        match &node.kind {
            NodeKind::Text(text) => {
                out.push(Piece::transcribable(text.clone(), style, node.source));
            }
            // The space a soft break draws is body text, not decoration: the reader
            // sees a word separator and a newline in the source is what produced it.
            // Anchoring it gives that cell a search span, so the selection highlight
            // reaches it like any other separator. `anchored` takes the origin only when
            // the lengths agree — one `\n` for one space — and that now holds for every
            // document: a CRLF break was two bytes and declined until line endings were
            // normalised where the document is read.
            NodeKind::SoftBreak => out.push(Piece::anchored(" ".to_string(), style, node.source)),
            NodeKind::LineBreak => out.push(Piece::synthetic("\n", style)),
            NodeKind::Code { literal } => {
                out.push(code_piece(
                    literal,
                    style.patch(theme.text.code),
                    node.source,
                ));
            }
            NodeKind::Emph => collect(
                &node.children,
                style.patch(theme.text.emphasis),
                ctx,
                ids,
                out,
            ),
            NodeKind::Strong => collect(
                &node.children,
                style.patch(theme.text.strong),
                ctx,
                ids,
                out,
            ),
            NodeKind::Strikethrough => collect(
                &node.children,
                style.patch(theme.text.strikethrough),
                ctx,
                ids,
                out,
            ),
            NodeKind::Link { url, .. } => link(node, url, style, ctx, ids, out),
            NodeKind::Image { .. } => image_marker(node, style, ctx, ids, out),
            NodeKind::FootnoteReference { name, number } => {
                let id = *ids;
                *ids += 1;
                let mut piece =
                    Piece::synthetic(format!("[{number}]"), style.patch(theme.text.footnote_ref));
                // The hotspot carries the *name* a definition is keyed by, never the
                // number: the number is only what is drawn (`NodeKind::FootnoteReference`
                // carries both, `NodeKind::FootnoteDefinition` is keyed by name alone).
                piece.control = Some(Control {
                    id,
                    kind: HotspotKind::Footnote { id: name.clone() },
                });
                out.push(piece);
            }
            // One marker per inline run is enough to say "HTML was dropped here";
            // `<b>x</b>` would otherwise bracket its own text with two of them. The
            // marker is spaced away from the words around it so it reads as a note in
            // the margin of the sentence rather than as a word in it.
            NodeKind::SkippedHtml { .. } => {
                if !out.iter().any(|piece| piece.text.trim() == HTML_MARKER) {
                    let lead = if ends_with_space(out) { "" } else { " " };
                    out.push(Piece::synthetic(
                        format!("{lead}{HTML_MARKER} "),
                        style.patch(theme.text.dim),
                    ));
                }
            }
            // Any other node appearing in an inline position is a container we do not
            // style specially; its children still render.
            _ => collect(&node.children, style, ctx, ids, out),
        }
    }
}

/// The brackets an image wears when it appears inside a sentence.
///
/// The same idiom as [`HTML_MARKER`], and for the same reason: something the terminal
/// cannot show was here, and the sentence has to say so without pretending the words in
/// the brackets are its own. What goes between them is the alt text, which is the
/// author's description of the picture and the only part of an image a reader of a
/// terminal can use.
const IMAGE_OPEN: &str = "⟨";
const IMAGE_CLOSE: &str = "⟩";

/// The word standing in for an image with no alt text at all.
const IMAGE_UNTITLED: &str = "image";

/// An image in an inline position: its alt text, bracketed.
///
/// **Changed 2026-08-09.** Every image used to become a framed placeholder box, which
/// is right for an image that is a paragraph of its own and wrong for one used inside a
/// sentence: a box is a block, so the paragraph was cut into three — the words before,
/// a full-width box, the words after — and a sentence the author wrote as one line was
/// read as three unrelated ones. The box is kept for the block case, decided in
/// [`block::paragraph`](super::block); everything else, including an image nested in a
/// link or a heading and an image in a table cell, arrives here.
fn image_marker(node: &Node, style: Style, ctx: Ctx<'_>, ids: &mut usize, out: &mut Vec<Piece>) {
    let style = style.patch(ctx.theme.text.image_alt);
    let before = out.len();
    collect(&node.children, style, ctx, ids, out);
    let alt: String = out[before..]
        .iter()
        .map(|piece| piece.text.as_str())
        .collect();
    if alt.trim().is_empty() {
        // `⟨⟩` says nothing. An image with no alt text still has to be visible — the
        // reader is being told a picture is missing from the sentence, which is a fact
        // about the document — so it is named instead.
        out.truncate(before);
        out.push(Piece::synthetic(
            format!("{IMAGE_OPEN}{IMAGE_UNTITLED}{IMAGE_CLOSE}"),
            style,
        ));
        return;
    }
    out.insert(before, Piece::synthetic(IMAGE_OPEN, style));
    out.push(Piece::synthetic(IMAGE_CLOSE, style));
}

/// Whether the run so far already ends in whitespace (or is empty).
fn ends_with_space(pieces: &[Piece]) -> bool {
    pieces
        .last()
        .is_none_or(|piece| piece.text.ends_with(char::is_whitespace))
}

/// A code span, anchored past the opening backtick fence when that is unambiguous.
fn code_piece(literal: &str, style: Style, source: SourceSpan) -> Piece {
    let padding = source.len().checked_sub(literal.len());
    let origin = match padding {
        Some(extra) if extra.is_multiple_of(2) => Some(Origin::Copied(source.start + extra / 2)),
        _ => None,
    };
    Piece {
        text: literal.to_string(),
        style,
        origin,
        control: None,
    }
}

/// The longest a link target may be before it is shown elided.
const URL_BUDGET: usize = 34;

/// Renders a link as its text, followed by a dim target when the two differ.
///
/// Inside a table cell the target is dropped altogether: a column is negotiated
/// against every other column in the table, and one long URL would otherwise claim a
/// whole row for itself (design spec §7.2). Elsewhere an over-long target is elided in
/// the middle, which keeps the informative ends — the host and the last path segment.
fn link(node: &Node, url: &str, style: Style, ctx: Ctx<'_>, ids: &mut usize, out: &mut Vec<Piece>) {
    let theme = ctx.theme;
    let before = out.len();
    collect(&node.children, style.patch(theme.text.link), ctx, ids, out);
    let text: String = out[before..]
        .iter()
        .map(|piece| piece.text.as_str())
        .collect();
    // An autolink renders its own target as its text; showing it twice is noise. A
    // bare e-mail address is the same case wearing a scheme: comrak gives `a@b.c` the
    // target `mailto:a@b.c`, and `a@b.c (mailto:a@b.c)` tells the reader nothing.
    let target = url.trim();

    // Tagged *before* the three early returns below, because each of them draws a link
    // that simply prints no ` (url)` suffix — an autolink, a bare e-mail address, a
    // link in a table cell — and a link with no suffix is still a link.
    //
    // The scheme allowlist of design spec §8: only `http`/`https` earn a control, so a
    // document the reader did not write cannot choose which desktop handler the pager
    // launches. `mailto:` and a local `.md` link record no hotspot at all.
    let control = super::link::classify(url).map(|kind| {
        let id = *ids;
        *ids += 1;
        // The FULL url, never the elided form the suffix draws: the status bar shows
        // the whole thing (§8) and the opener receives it (§7). `classify` already
        // carries it in `kind`.
        Control { id, kind }
    });
    for piece in &mut out[before..] {
        piece.control = control.clone();
    }

    if target.is_empty()
        || text.trim() == target
        || text.trim() == target.trim_start_matches("mailto:")
    {
        return;
    }
    if ctx.table_depth > 0 && !text.trim().is_empty() {
        return;
    }
    let mut suffix = Piece::synthetic(
        format!(" ({})", elide_middle(url, URL_BUDGET)),
        style.patch(theme.text.link_url),
    );
    // §2.1: the suffix carries no source span and *is* part of the control.
    suffix.control = control;
    out.push(suffix);
}

/// Shortens `text` to `budget` display columns by replacing its middle with `…`.
///
/// The end-elided sibling of this is [`crate::text::ellipsize`]; a URL needs the
/// *middle* dropped instead, because it carries its meaning at both ends — the host at
/// the front and the document name at the back.
pub(crate) fn elide_middle(text: &str, budget: usize) -> String {
    if display_width(text) <= budget || budget < 3 {
        return text.to_string();
    }
    let head = (budget - 1).div_ceil(2);
    let tail = budget - 1 - head;
    let front = crate::text::truncate_to_width(text, head);
    let back = crate::text::split_at_width(text, display_width(text) - tail).1;
    format!("{front}…{back}")
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
