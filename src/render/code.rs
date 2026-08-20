// SPDX-License-Identifier: MIT
//! Fenced and indented code blocks, and the Mermaid fences routed out of them.
//!
//! Code never wraps (design spec §8): a line wider than the frame is clipped and the
//! last column carries an overflow marker. The clip happens to the code *area*, before
//! the frame is drawn around it, so the marker lands inside the box and the box still
//! closes — a table, whose borders are laid out with its content, has to close its cut
//! rules explicitly instead (see `render::table`).
//! A ```` ```mermaid ```` fence goes to the diagram renderer instead; when that fails
//! the block degrades to a syntax-highlighted code block with a dim caption naming the
//! reason (design spec §6).
//!
//! With [`RenderOptions::line_numbers`](super::RenderOptions::line_numbers) on, a
//! themed gutter is drawn to the left of the code. The gutter is *outside* the
//! clipped region: it is written at a fixed position and the code area shrinks by its
//! width, so this renderer's own clip cuts the code and never the numbers.
//!
//! That is a claim about this file and nothing else — it used to be written as "scrolling
//! a long line horizontally never scrolls the numbers away", which was true of the clip
//! and false of the pager, where the horizontal offset moved every column of a row alike
//! and carried the gutter off the left edge with the code. Keeping the numbers on screen
//! there is `tui`'s job, and this file tells it where they end: [`pin_gutter`] records the
//! seam on the canvas as a [`Pin`](crate::canvas::Pin), the third metadata channel beside
//! anchors and search spans, and `tui::draw` holds those columns still while the rest of
//! each row scrolls under them.
//!
//! The pager used to *infer* the seam instead, by matching cell styles on the drawn
//! canvas. Do not restore that: `theme.code.line_number` is not unique — both shipped
//! themes give `code.operator` the same value — so an unnumbered fence containing an `=`
//! was read as having a gutter, and the inferred prefix was spread over a contiguous run
//! of non-blank rows, which inside a list item is this fence and the block after it.
//! `tui::tests::the_gutter_rule_matches_the_renderer` pins the published column to the
//! layout drawn here.

use crate::canvas::{Atom, BorderSet, Canvas, SearchSpan};
use crate::doc::SourceSpan;
use crate::error::MermaidError;
use crate::mermaid::Fit;
use crate::text::{Line, Span, display_width, graphemes};
use crate::theme::Style;

use super::{Ctx, bridge, button};

/// The info-string language that routes a fence to the Mermaid renderer.
const MERMAID: &str = "mermaid";

/// The marker shown at the right edge of a clipped code line.
pub(crate) const OVERFLOW_MARKER: &str = "›";

/// The glyph separating the line-number gutter from the code.
const GUTTER_RULE: &str = "│";

/// Whether a fence's info string routes it to the diagram renderer.
///
/// The one place that decision is made. [`super::diagram::diagram`] has to ask the same
/// question one layer up, and a second spelling of it there is exactly the kind of
/// duplicated predicate that drifts apart unnoticed.
pub(crate) fn is_mermaid(language: Option<&str>) -> bool {
    language == Some(MERMAID)
}

/// Renders a code block, routing Mermaid fences to the diagram renderer.
///
/// Diagrams are drawn under [`Fit::COMPACT`]: this path has nowhere to scroll, so a
/// squeezed drawing beats a dump of Mermaid source. The pager's top-level fences go
/// through [`super::diagram::diagram`] instead and are drawn under [`Fit::ROOMY`].
pub(crate) fn render_code_block(
    language: Option<&str>,
    literal: &str,
    fenced: bool,
    origins: &[SourceSpan],
    block: SourceSpan,
    width: u16,
    ctx: Ctx<'_>,
) -> Canvas {
    if is_mermaid(language) {
        return match bridge::mermaid(literal, width, ctx.theme, Fit::COMPACT) {
            Ok(canvas) => diagram_block(canvas, width, literal, origins, block, ctx),
            Err(error) => fallback(
                literal,
                Some(MERMAID),
                &caption(&error),
                origins,
                width,
                ctx,
            ),
        };
    }
    framed_code(language, literal, fenced, origins, width, ctx)
}

/// A drawn diagram as a block of the document: the canvas, padded to the block width,
/// with its labels' spans rebased onto the document and its drawn rectangle recorded as
/// an [`Atom`].
///
/// Shared with [`super::diagram::diagram`], which builds the same block at a width the
/// viewport does not have, so that the two cannot disagree about what a diagram block
/// *is* — the rebasing included, and the floating `[copy]` button, which is why both
/// happen here and not at either call site.
///
/// `block` is the fence's own extent in the document, opener and closer included — the
/// node's `source`, not anything re-derived by hunting for backticks. It is what a
/// selection wider than one label copies (design spec §2.2), and it is recorded
/// *before* the canvas is padded out to the block width so that the rectangle is the
/// diagram's own, not the page's.
pub(crate) fn diagram_block(
    mut canvas: Canvas,
    width: u16,
    literal: &str,
    origins: &[SourceSpan],
    block: SourceSpan,
    ctx: Ctx<'_>,
) -> Canvas {
    rebase_spans(&mut canvas, literal, origins);
    if let Some((row, rows, col, cols)) = drawn_bounds(&canvas) {
        canvas.add_atom(Atom {
            row,
            rows,
            col,
            cols,
            source_start: block.start,
            source_end: block.end,
            // comrak's own prefix-stripping, carried over verbatim. What a container
            // stripped from each line is not recoverable from the source range alone, and
            // a selection left to guess it copies Mermaid a reader cannot paste — see
            // [`Atom::content`].
            content: literal.to_string(),
        });
    }
    canvas.resize_width(width, ctx.base);
    // A diagram has no frame to hang the button on, so it floats at the top right of the
    // block — which makes the drawing itself the only other occupant of that row. The
    // occupied extent is therefore measured off the drawn row rather than assumed to be
    // zero, so `place` can decline on a diagram that reaches the right edge instead of
    // blanking a box. Placed after the resize because the button's column is reckoned
    // from the block's width, not from the layout's.
    //
    // A block inside a table cell is not offered one at all, exactly as a code frame is
    // not. Until 2026-08-12 (Task 2b) that was forced by `Canvas::blit` dropping
    // hotspots — the label would have kept its cells and lost the claim behind it. A blit
    // now carries a hotspot, so the guard stands as a product decision: one button per
    // top-level block. `render::table::render_table_node` writes it down at length.
    //
    // The height check is the same rule read once more: `Canvas::write_str` no-ops on a
    // row that does not exist while `place` would still record the hotspot, so an empty
    // canvas would get a hotspot behind no label. A successful Mermaid layout always
    // draws at least one row, so no test reaches it; it costs one comparison to keep the
    // "both or neither" contract true by construction rather than by argument.
    if ctx.options.copy_button && ctx.table_depth == 0 && canvas.height() > 0 {
        let occupied =
            u16::try_from(display_width(canvas.row_text(0).trim_end())).unwrap_or(u16::MAX);
        button::place(
            &mut canvas,
            0,
            occupied,
            ctx.theme.code.frame,
            // The mermaid source, opener and closer excluded: all three copy buttons
            // carry the block's content and not its fences (owner ruling, 2026-08-12).
            literal.to_string(),
            None,
        );
    }
    canvas
}

/// The bounding box of the cells a diagram actually drew: `(row, rows, col, cols)`.
///
/// A layout hands back a canvas as wide as the space it was offered, so the drawing sits
/// in the top-left of a mostly blank rectangle. Recording *that* as the atom would wash
/// the empty margin beside the diagram, which reads as a highlight bug rather than as a
/// diagram taken whole — so the box is measured from the glyphs.
///
/// Blank rows and columns *inside* the box are part of it: the gap between two nodes is
/// the diagram's own, and the wash is meant to be solid (design spec §2.2). Only the
/// margin around the drawing is trimmed. `None` when nothing was drawn at all.
fn drawn_bounds(canvas: &Canvas) -> Option<(usize, usize, u16, u16)> {
    let mut top: Option<usize> = None;
    let mut bottom = 0usize;
    let mut left = u16::MAX;
    let mut right = 0u16;
    for row in 0..canvas.height() {
        let text = canvas.row_text(row);
        let drawn = text.trim_end();
        let lead = drawn.len() - drawn.trim_start().len();
        if lead == drawn.len() {
            continue;
        }
        let start = u16::try_from(display_width(&drawn[..lead])).unwrap_or(u16::MAX);
        let end = u16::try_from(display_width(drawn)).unwrap_or(u16::MAX);
        top.get_or_insert(row);
        bottom = row;
        left = left.min(start);
        right = right.max(end);
    }
    let top = top?;
    (right > left).then(|| (top, bottom - top + 1, left, right - left))
}

/// Rewrites a diagram's spans from offsets into `literal` to offsets into the document.
///
/// A layout family emits a span at the offset its label had in the Mermaid block it was
/// parsed from (`mermaid::ast::Label::source`). The document's spans are absolute, and
/// the two differ by more than one constant: comrak strips a container prefix — four
/// spaces, `> `, a list indent — from every line of the literal independently, so there
/// is no single delta to add. `origins` already carries the answer per line, built by
/// `doc::convert::code_lines`, which locates each line as a *suffix* of a source line;
/// that is the same mapping `code_area` uses for a code block's own spans, and it is
/// where the container indent has already been solved. (Line endings are no longer part
/// of that problem: they are normalised where the document is read.)
///
/// It is reused here rather than re-derived, but not by analogy: `code_area` maps a
/// *row* to a line and needs no column arithmetic on the source side, whereas a label
/// sits at an arbitrary byte offset inside its line. What transfers is `origins`
/// itself; the offset-within-the-line walk below is this function's own.
///
/// Fails closed in every case it cannot answer — no mapping for the block, no origin
/// for the line, an offset past the line's end — by dropping the span. A diagram with
/// no provenance falls back to the drawn cells and says so (design spec §3.1); a
/// diagram with *wrong* provenance copies bytes from somewhere else in the document.
fn rebase_spans(canvas: &mut Canvas, literal: &str, origins: &[SourceSpan]) {
    if canvas.spans().is_empty() {
        return;
    }
    let lines = crate::doc::literal_lines(literal);
    canvas.map_spans(|span| {
        let start = document_offset(&lines, origins, span.source_start)?;
        let end = document_offset(&lines, origins, span.source_end)?;
        // A unit is a source range like any other and is rebased with the span that
        // names it. Failing closed drops the whole span rather than just its unit: a
        // label whose pieces disagreed about which label they belong to would read as a
        // drag across several boxes, and a reader pointing at one word would get the
        // whole chart.
        let unit = match span.unit {
            Some((from, to)) => Some((
                document_offset(&lines, origins, from)?,
                document_offset(&lines, origins, to)?,
            )),
            None => None,
        };
        (start <= end).then_some(SearchSpan {
            source_start: start,
            source_end: end,
            unit,
            ..*span
        })
    });
}

/// The document offset of byte `at` of the Mermaid literal, if it has one.
///
/// `lines` is the literal split by [`crate::doc::literal_lines`] — the crate's one
/// definition of "a line of `literal`", and the same split `origins` was built against,
/// so index `n` names the same line in both. The guard below is against the **origin's
/// own end** rather than against the line's length, because the two are the same length
/// only when the line was located: a line `code_lines` could not find contributes an
/// empty origin, and an offset inside it must be dropped rather than measured against a
/// range that has nothing to do with it. (It also used to absorb the `\r` a CRLF literal
/// carried past the end of its origin; line endings are normalised at the read now, so
/// that case no longer arises and the guard stands on the reason above alone.)
fn document_offset(lines: &[&str], origins: &[SourceSpan], at: usize) -> Option<usize> {
    let mut base = 0usize;
    for (row, line) in lines.iter().enumerate() {
        let end = base + line.len();
        if at <= end {
            let origin = origins.get(row).filter(|origin| !origin.is_empty())?;
            let offset = origin.start + (at - base);
            return (offset <= origin.end).then_some(offset);
        }
        // The `\n` that `literal_lines` split on, and that the last line may not have.
        base = end + 1;
    }
    None
}

/// Draws the framed, highlighted code block.
fn framed_code(
    language: Option<&str>,
    literal: &str,
    fenced: bool,
    origins: &[SourceSpan],
    width: u16,
    ctx: Ctx<'_>,
) -> Canvas {
    let theme = ctx.theme;
    let lines = bridge::highlight(language, literal, theme);
    // Below four columns there is no room for a frame plus content; the code is shown
    // bare rather than as a box with nothing inside it.
    if width < 4 {
        return code_area(&lines, origins, literal, width, false, ctx);
    }
    // The frame takes two columns and the interior padding one more on each side, so
    // code sits inside its box the way a table cell sits inside its column.
    let padding = if width > 2 + 2 * CODE_PADDING {
        CODE_PADDING
    } else {
        0
    };
    let area_width = width - 2 - 2 * padding;
    let area = code_area(
        &lines,
        origins,
        literal,
        area_width,
        ctx.options.line_numbers,
        ctx,
    );
    let gutter = gutter_width(lines.len(), area_width, ctx.options.line_numbers);
    let inner = area.indent(padding, padding, theme.code.background);
    let title = fenced
        .then_some(language)
        .flatten()
        .map(|name| title(name, ctx));
    let mut out = inner.framed(
        BorderSet::ROUNDED,
        theme.code.frame,
        title.as_ref(),
        theme.code.background,
    );
    join_gutter(&mut out, gutter, padding, title.as_ref(), None, ctx);
    pin_gutter(&mut out, gutter, padding, title.as_ref());
    // The label and the junction have already taken what they need of the top edge; the
    // button is the third occupant and the only optional one, so it is the one that
    // yields. A block inside a table cell is not offered one at all — a product decision
    // since Task 2b taught `Canvas::blit` to carry a hotspot; see
    // `render::table::render_table_node`.
    if ctx.options.copy_button && ctx.table_depth == 0 {
        let occupied = top_edge_occupied(&out, title.as_ref());
        button::place(
            &mut out,
            0,
            occupied,
            theme.code.frame,
            literal.to_string(),
            None,
        );
    }
    out
}

/// The first column of the top edge that nothing has claimed yet.
///
/// Read back off the drawn row for the same reason `pin_gutter` does it: the label's
/// column depends on whether it collided with the gutter junction, and a second copy of
/// that arithmetic is what would drift.
fn top_edge_occupied(out: &Canvas, title: Option<&Line>) -> u16 {
    let Some(title) = title else { return 1 };
    let text = out.row_text(0);
    let start = title
        .spans
        .first()
        .and_then(|span| text.find(span.text.as_str()))
        .map_or(2, |byte| display_width(&text[..byte]));
    u16::try_from(start + title.width() + 1).unwrap_or(u16::MAX)
}

/// Publishes the columns of this block that are chrome, for the pager to hold still.
///
/// The seam between gutter and code is arithmetic only this function has: the frame's
/// column, the padding, and the gutter [`code_area`] drew. Handing it to the pager as a
/// [`Canvas`] pin is what lets `tui::draw` keep the numbers on screen while a long line
/// scrolls under them, without the pager having to guess where the gutter ended.
///
/// It used to guess, by matching cell styles: the digits were "the only cells painted in
/// `theme.code.line_number`". They are not — `theme.code.operator` is the same value in
/// both shipped themes — and the guess was then spread over a contiguous run of non-blank
/// rows, which inside a list item is this fence *and the table under it*. A published pin
/// is per block by construction and rests on no style at all.
fn pin_gutter(out: &mut Canvas, gutter: usize, padding: u16, title: Option<&Line>) {
    if gutter == 0 {
        return;
    }
    // Frame, padding, gutter: the column the code starts at, the blank column after the
    // gutter's rule included, so offset zero is byte-identical to no pinning at all.
    let prefix = u16::try_from(1 + usize::from(padding) + gutter)
        .unwrap_or(u16::MAX)
        .min(out.width());
    for row in 0..out.height() {
        out.add_pin(row, prefix);
    }
    // The label in the top rule is chrome for the same reason the numbers are, and a
    // prefix stopping short of it leaves a fragment of a word standing in a box rule —
    // `╭  ru────╮`. Where it starts depends on whether it collided with the gutter's
    // junction: `framed` writes it one column in from the corner, but `join_gutter`
    // moves it to just after the junction when the two want the same column. Both cases
    // are read back off the drawn row rather than re-derived here, because a second copy
    // of that arithmetic is exactly what would drift.
    if let Some(title) = title {
        let text = out.row_text(0);
        let start = title
            .spans
            .first()
            .and_then(|span| text.find(span.text.as_str()))
            .map_or(2, |byte| crate::text::display_width(&text[..byte]));
        let label = u16::try_from(start + title.width() + 1)
            .unwrap_or(u16::MAX)
            .min(out.width());
        out.add_pin(0, label.max(prefix));
    }
}

/// Blank columns between the code frame and the code inside it.
const CODE_PADDING: u16 = 1;

/// Joins the line-number gutter rule to the frame with `┬`/`┴` junctions.
///
/// Without this the gutter is a bar floating between two horizontal edges it does not
/// meet; with it the block reads as one piece of chrome.
///
/// The junctions and the edge labels want the same columns — a four-column gutter puts
/// the `┬`/`┴` under the third letter of `rust`, or of a caption — and the rule used to
/// be that the label won and the junction was simply dropped. That left the gutter
/// closed on one edge and open on the other, which reads as a box that failed to draw
/// rather than as a label that took precedence, so the two are no longer in competition:
/// the label is moved to the *right* of the junction, and the top edge comes out
/// `╭───┬ rust ───╮`, the mirror of `╰───┴ reason ──╯` beneath it. Both labels are
/// therefore re-drawn here rather than being handed to `Canvas::framed_captioned`, which
/// knows only one place to put each.
///
/// **2026-08-19**: the bottom edge used to skip this — `write_str(last, col, tee_up,
/// ..)` ran unconditionally, with no check that a caption was standing on `col` the way
/// the top edge already checked for a title. A caption starts in the same column as a
/// title (`Canvas::framed_captioned` writes both one column in from their corner), so
/// any caption long enough to reach the gutter's junction column had a character of its
/// own text overwritten by `┴` — found by rendering a math fallback with line numbers
/// on, where `display math is not laid out yet` came out `di┴play`. The same corruption
/// was already shipping for every Mermaid failure caption long enough to reach that
/// column (`render::tests::the_caption_is_not_corrupted_by_the_gutter_junction_with_line_numbers_on`
/// pins both). `join_edge` is what both edges now share, so a fix to one is a fix to
/// both.
fn join_gutter(
    out: &mut Canvas,
    gutter: usize,
    padding: u16,
    title: Option<&Line>,
    caption: Option<&Line>,
    ctx: Ctx<'_>,
) {
    if gutter == 0 {
        return;
    }
    // Inside the frame and the padding, the rule sits two columns left of the code.
    let col = 1 + usize::from(padding) + gutter - 2;
    let frame = ctx.theme.code.frame;
    let last = out.height().saturating_sub(1);
    let inner = usize::from(out.width()).saturating_sub(2);
    let set = BorderSet::ROUNDED;
    join_edge(out, 0, col, inner, title, set.tee_down, frame);
    join_edge(out, last, col, inner, caption, set.tee_up, frame);
}

/// Draws one junction glyph into one horizontal edge, first moving that edge's label out
/// of the way if it is standing on the junction column. See [`join_gutter`].
fn join_edge(
    out: &mut Canvas,
    row: usize,
    col: usize,
    inner: usize,
    label: Option<&Line>,
    junction: char,
    frame: Style,
) {
    let set = BorderSet::ROUNDED;
    if col < inner
        && out.row_text(row).chars().nth(col) != Some(set.horizontal)
        && let Some(label) = label
    {
        // The label is standing on the junction column. Lay the whole edge again — the
        // old label has to go completely, not be partly overwritten — and put the label
        // back down after the junction.
        out.hline(row, 1, inner, &set.horizontal.to_string(), frame);
        // Re-ellipsize rather than `Line::truncated`. A caption is already ellipsized
        // once, in `fallback`, against the frame's full width — before anyone here
        // knows whether the gutter is even going to shift it. Shifting it right of the
        // junction shrinks its budget a second time, and a hard `truncated` on an
        // already-ellipsized string just chops more characters off the end with no new
        // `…` and no room reserved for the trailing space before the corner — found by
        // rendering an over-long Mermaid caption with line numbers on, where the fixed
        // corruption (`di┴play`) was replaced by a caption cut straight into `╯` with
        // no mark that it had been shortened at all. Ellipsizing the label's own text
        // against the *actual*, post-shift room is the same "shorten and mark it" every
        // other label in the program uses, applied where the shift is actually decided
        // — one truncation, not two disagreeing ones. Every label reaching this branch
        // in the program is one style throughout (`Line::styled`, or `title`'s icon and
        // name sharing `theme.code.language`, which `Line::push` already merges into a
        // single span) so concatenating loses nothing; a label that were not could
        // still only lose *inner* style boundaries, never characters.
        let text: String = label.spans.iter().map(|span| span.text.as_str()).collect();
        let style = label.spans.first().map_or(frame, |span| span.style);
        let room = inner.saturating_sub(col);
        let mut spaced = Line::empty();
        spaced.push(Span::new(" ", frame));
        spaced.push(Span::new(
            crate::text::ellipsize(&text, room.saturating_sub(2)),
            style,
        ));
        spaced.push(Span::new(" ", frame));
        out.write_line(row, col + 1, &spaced, frame);
    }
    // Only once the column is clear (never occupied, or just cleared above) does the
    // junction get drawn — the label wins the column either way, which is what keeps a
    // caption's own text from ever losing a character to `┴`.
    if out.row_text(row).chars().nth(col) == Some(set.horizontal) {
        out.write_str(row, col, &junction.to_string(), frame);
    }
}

/// The label drawn into the frame's top edge: the language, with its icon if enabled.
fn title(language: &str, ctx: Ctx<'_>) -> Line {
    let theme = ctx.theme;
    let mut line = Line::empty();
    if let Some(icon) = ctx.glyphs.language(Some(language)) {
        line.push(Span::new(format!("{icon} "), theme.code.language));
    }
    line.push(Span::new(language, theme.code.language));
    line
}

/// Writes code lines at `width` columns, clipping rather than wrapping.
///
/// When `numbered` is set and there is room for it, a gutter of right-aligned line
/// numbers is drawn first and the code is clipped to what remains.
fn code_area(
    lines: &[Line],
    origins: &[SourceSpan],
    literal: &str,
    width: u16,
    numbered: bool,
    ctx: Ctx<'_>,
) -> Canvas {
    let theme = ctx.theme;
    let budget = usize::from(width);
    let digits = digit_count(lines.len());
    let gutter = gutter_width(lines.len(), width, numbered);
    // Lines are written at their full length onto an over-wide canvas and the whole
    // block is then clipped in one operation, so the "line goes on" marker rule lives
    // in `Canvas::clip_with_marker` rather than being re-derived here.
    let natural = lines.iter().map(Line::width).max().unwrap_or(0) + gutter;
    let mut out = Canvas::new(
        u16::try_from(natural.max(budget)).unwrap_or(u16::MAX),
        lines.len(),
        theme.code.background,
    );
    // The raw (unexpanded) text of each line of `literal`, split exactly the way
    // `NodeKind::CodeBlock.lines` was built, so `raw.get(row)` names the same line as
    // `origins.get(row)`. `bridge::highlight`'s `lines` above have had tabs expanded to
    // spaces (`highlight::expand_tabs`), which is a display concern; a `SearchSpan`
    // points at document bytes, and a tab is one document byte, not `TAB_WIDTH` of
    // them, so the byte offset below has to be measured against this text instead —
    // measuring against the expanded line landed `source_end` past the end of the
    // line, into whatever source bytes followed it, on any code containing a tab.
    //
    // Skipped entirely when `origins` is empty: nothing below ever indexes `raw` in
    // that case, so splitting `literal` would be a `Vec` allocation a ten-thousand-line
    // fragment (or a construction site with no mapping at all) would pay for and never
    // use.
    let raw = if origins.is_empty() {
        Vec::new()
    } else {
        crate::doc::literal_lines(literal)
    };
    for (row, line) in lines.iter().enumerate() {
        if gutter > 0 {
            let number = format!("{:>digits$} ", row + 1, digits = digits);
            out.write_str(row, 0, &number, theme.code.line_number);
            out.write_str(row, digits + 1, GUTTER_RULE, theme.code.frame);
        }
        out.write_line(row, gutter, line, theme.code.background);
        // The gutter is chrome and is not in the document, so the span starts where the
        // code does. `origins` is empty for a block rendered without a mapping — a
        // fragment, or a construction site that has none — and then this block behaves
        // exactly as it did before spans existed.
        if let Some(origin) = origins.get(row).filter(|o| !o.is_empty()) {
            // Whether *this* row loses anything to the clip below is a per-row question
            // — `Canvas::clip_with_edges` only marks a row that actually had a
            // non-blank cell past `width`, and a code block's rows are rarely all the
            // same length. Deciding it from the block's widest line (`natural`) instead
            // used to report a row one column and one byte short of what was actually
            // drawn, whenever a shorter row happened to fit exactly.
            //
            // It is also a question about *content*, not about width: `is_blank` is
            // true of a space, so a line whose only overflow is trailing whitespace is
            // not marked either, and asking `line.width()` — which counts those spaces
            // — deducted a marker column that was never drawn and left the last
            // character of the line outside its own span, unreachable to search and to
            // copy. Trimming to content asks the canvas's question.
            let code_budget = budget.saturating_sub(gutter);
            let content_width = display_width(line.text().trim_end_matches(' '));
            let clipped = content_width > code_budget;
            let drawn = if clipped {
                code_budget.saturating_sub(display_width(OVERFLOW_MARKER))
            } else {
                content_width
            };
            if drawn > 0 {
                let text = raw.get(row).copied().unwrap_or_default();
                // Belt and braces: even a correct walk below cannot exceed `origin`'s
                // own bytes, but nothing else guarantees that structurally, and this is
                // the one invariant — never span past the line the mapping named — that
                // should hold by construction rather than by care.
                let source_end = (origin.start + bytes_for_columns(text, drawn)).min(origin.end);
                out.add_span(SearchSpan {
                    source_start: origin.start,
                    source_end,
                    unit: None,
                    row,
                    col: u16::try_from(gutter).unwrap_or(u16::MAX),
                    cols: u16::try_from(drawn).unwrap_or(u16::MAX),
                    copied: true,
                });
            }
        }
    }
    // The gutter sits left of the clip point, so the clip below cuts code and never
    // numbers. The pager pins the same columns against its own horizontal offset; see
    // the module header.
    debug_assert!(gutter < budget || budget == 0);
    out.clip_with_marker(width, OVERFLOW_MARKER, theme.code.overflow_marker);
    out.resize_width(width, theme.code.background);
    out
}

/// How many bytes of the raw (unexpanded) source line `text` the first `columns`
/// **display** columns occupy.
///
/// `text` is a line of `literal` as comrak handed it to us — tabs still tabs, nothing
/// expanded — because that is what a `SearchSpan`'s byte range must measure against:
/// the document, not `bridge::highlight`'s rendering of it. Tab stops are tracked the
/// same way `highlight::expand_tabs` computes them when it built the *drawn* line, so
/// the two walks land on the same column for the same byte even though one produces
/// spaces and the other counts them.
///
/// Grapheme-wise otherwise, and for the reason `tui::select::byte_at_column` gives in
/// the other direction: a double-width cluster is two columns and one boundary, so
/// counting bytes or `char`s would land inside it and cut a span mid-character.
fn bytes_for_columns(text: &str, columns: usize) -> usize {
    let mut used = 0usize;
    let mut offset = 0usize;
    let mut column = 0usize;
    for cluster in graphemes(text) {
        let width = if cluster == "\t" {
            crate::highlight::TAB_WIDTH - (column % crate::highlight::TAB_WIDTH)
        } else {
            display_width(cluster)
        };
        if used + width > columns {
            break;
        }
        used += width;
        column += width;
        offset += cluster.len();
    }
    offset
}

/// How many columns the line-number gutter `NNN │ ` occupies, zero when there is none.
///
/// The gutter is dropped entirely rather than squeezing the code into nothing when the
/// block is too narrow to carry both.
fn gutter_width(lines: usize, width: u16, numbered: bool) -> usize {
    if !numbered {
        return 0;
    }
    let digits = digit_count(lines);
    if digits + 4 > usize::from(width) {
        0
    } else {
        digits + 3
    }
}

/// How many columns the largest line number needs.
fn digit_count(lines: usize) -> usize {
    lines.max(1).ilog10() as usize + 1
}

/// A block's source shown as a framed code block, with the reason in its bottom edge.
///
/// The frame's top edge already names the language, so the caption says what happened,
/// not what the block is. Two callers: a diagram that would not draw, and display math
/// that cannot be laid out yet — design spec §9's one failure rendering, reached for two
/// reasons.
pub(super) fn fallback(
    literal: &str,
    language: Option<&str>,
    caption: &dyn std::fmt::Display,
    origins: &[SourceSpan],
    width: u16,
    ctx: Ctx<'_>,
) -> Canvas {
    let theme = ctx.theme;
    let lines = bridge::highlight(language, literal, theme);
    if width < 4 {
        return code_area(&lines, origins, literal, width, false, ctx);
    }
    let padding = if width > 2 + 2 * CODE_PADDING {
        CODE_PADDING
    } else {
        0
    };
    let area_width = width - 2 - 2 * padding;
    let inner = code_area(
        &lines,
        origins,
        literal,
        area_width,
        ctx.options.line_numbers,
        ctx,
    )
    .indent(padding, padding, theme.code.background);
    let title = Line::styled(language.unwrap_or_default(), theme.code.language);
    // The bottom edge is as long as the block; a caption longer than that is elided
    // rather than hard-cut, so it never ends mid-word against the corner glyph.
    let room = usize::from(width).saturating_sub(4);
    let caption = Line::styled(
        crate::text::ellipsize(&caption.to_string(), room),
        theme.block.caption,
    );
    let mut out = inner.framed_captioned(
        BorderSet::ROUNDED,
        theme.code.frame,
        Some(&title),
        Some(&caption),
        theme.code.background,
    );
    let gutter = gutter_width(lines.len(), area_width, ctx.options.line_numbers);
    join_gutter(&mut out, gutter, padding, Some(&title), Some(&caption), ctx);
    pin_gutter(&mut out, gutter, padding, Some(&title));
    // The fallback is a highlighted code block showing the block's own source — exactly
    // what a reader who just saw the failure caption wants to copy — so it gets a button
    // the same way any other framed code block does.
    if ctx.options.copy_button && ctx.table_depth == 0 {
        let occupied = top_edge_occupied(&out, Some(&title));
        button::place(
            &mut out,
            0,
            occupied,
            theme.code.frame,
            literal.to_string(),
            None,
        );
    }
    out
}

/// The diagram families `mdmost` draws, spelled as a reader would write them.
///
/// Named in the caption when the first word of a block is not a family at all, because
/// "unknown diagram type" alone leaves the reader no way to discover what *would* have
/// worked. [`the_advertised_families_are_the_ones_that_actually_parse`] pins this list
/// to what the parser accepts, so it cannot drift into advertising vapour.
///
/// [`the_advertised_families_are_the_ones_that_actually_parse`]: super::tests
pub(crate) const FAMILIES: [&str; 7] = [
    "flowchart",
    "sequenceDiagram",
    "classDiagram",
    "erDiagram",
    "stateDiagram-v2",
    "pie",
    "gantt",
];

/// What the bottom edge of an undrawable Mermaid block says.
///
/// Two things a reader needs kept apart: *mdmost cannot draw this* and *this diagram is
/// wrong*. The first is our failure and must never be phrased as a syntax complaint —
/// that sends the reader hunting for a typo in a correct diagram. The second names the
/// line and quotes the offending text, which is what a compiler would do.
fn caption(error: &MermaidError) -> String {
    match error {
        // Every family in `FAMILIES` parses, so an unknown keyword is not a diagram we
        // have yet to implement — it is not a diagram. Say what *is* one.
        MermaidError::UnsupportedFamily(_) => {
            format!("not a diagram type — mdmost draws {}", FAMILIES.join(", "))
        }
        // The old wording — "needs more than {width}" — restated the width the reader
        // already had, so widening the terminal was a guessing game with no way to know
        // when to stop. Name the target instead, and name it flatly: the hedge this
        // used to carry ("at least") was true of a search that stopped at the first
        // rung it liked, and false of the one that replaced it. Every renderer that
        // reports a floor now reports the exact width its diagram starts drawing at,
        // which `every_reported_floor_is_the_width_the_diagram_starts_drawing_at`
        // checks across all seven families — a hedge here would be a worse answer than
        // the truth, because the reader has to act on it.
        MermaidError::TooNarrow {
            width,
            needed: Some(needed),
        } if *needed > *width => {
            format!("needs {needed} columns to draw — this block has {width}")
        }
        MermaidError::TooNarrow { width, .. } => {
            format!("needs more than {width} columns to draw")
        }
        // Our bug, not the author's. Naming it as ours is the whole point of the
        // variant: the same failure used to arrive as `Unsupported { line: 0 }` and
        // read as a complaint about a diagram that was perfectly correct.
        MermaidError::Internal { message } => {
            format!("mdmost could not draw this diagram — please report: {message}")
        }
        MermaidError::Unsupported { line, message } => located(*line, message),
        MermaidError::Syntax { line, message } => located(*line, message),
    }
}

/// Prefixes a message with its source line, when there is one.
///
/// Lines are 1-based to a reader, so a zero is not a location: it is an internal error
/// with no line to offer, and printing "line 0" sends the reader looking for somewhere
/// that cannot exist.
fn located(line: usize, message: &str) -> String {
    if line == 0 {
        message.to_string()
    } else {
        format!("line {line}: {message}")
    }
}

/// The natural width of a code block at these options: frame, padding and gutter
/// included.
///
/// The table column negotiator needs this so a code block in a cell asks for the
/// right amount of room; it must therefore track every column
/// [`framed_code`] spends on chrome.
pub(crate) fn natural_width(literal: &str, ctx: Ctx<'_>) -> usize {
    let longest = literal.lines().map(display_width).max().unwrap_or(0);
    let gutter = if ctx.options.line_numbers {
        digit_count(literal.lines().count()) + 3
    } else {
        0
    };
    longest + gutter + chrome_width()
}

/// The columns a framed code block spends on chrome: two border columns plus padding.
pub(crate) const fn chrome_width() -> usize {
    2 + 2 * CODE_PADDING as usize
}
