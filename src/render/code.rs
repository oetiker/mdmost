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
//! there is `tui`'s job: `tui::wide::pinned_prefix` finds this block's chrome by reading
//! the drawn canvas, by style — the digits are the only cells in
//! `theme.code.line_number`, the rule closing them is `GUTTER_RULE` in
//! `theme.code.frame`, and the label in the top rule is the only `theme.code.language` —
//! and `tui::draw` holds those columns still while the rest of each row scrolls under
//! them. Changing a glyph, a style, or the order they are written in will move that seam;
//! `tui::tests::the_gutter_rule_matches_the_renderer` is the tripwire.

use crate::canvas::{BorderSet, Canvas};
use crate::error::MermaidError;
use crate::mermaid::Fit;
use crate::text::{Line, Span, display_width};

use super::{Ctx, bridge};

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
    width: u16,
    ctx: Ctx<'_>,
) -> Canvas {
    if is_mermaid(language) {
        return match bridge::mermaid(literal, width, ctx.theme, Fit::COMPACT) {
            Ok(canvas) => diagram_block(canvas, width, ctx),
            Err(error) => fallback(literal, &error, width, ctx),
        };
    }
    framed_code(language, literal, fenced, width, ctx)
}

/// A drawn diagram as a block of the document: the canvas, padded to the block width.
///
/// Shared with [`super::diagram::diagram`], which builds the same block at a width the
/// viewport does not have, so that the two cannot disagree about what a diagram block
/// *is*.
pub(crate) fn diagram_block(mut canvas: Canvas, width: u16, ctx: Ctx<'_>) -> Canvas {
    canvas.resize_width(width, ctx.base);
    canvas
}

/// Draws the framed, highlighted code block.
fn framed_code(
    language: Option<&str>,
    literal: &str,
    fenced: bool,
    width: u16,
    ctx: Ctx<'_>,
) -> Canvas {
    let theme = ctx.theme;
    let lines = bridge::highlight(language, literal, theme);
    // Below four columns there is no room for a frame plus content; the code is shown
    // bare rather than as a box with nothing inside it.
    if width < 4 {
        return code_area(&lines, width, false, ctx);
    }
    // The frame takes two columns and the interior padding one more on each side, so
    // code sits inside its box the way a table cell sits inside its column.
    let padding = if width > 2 + 2 * CODE_PADDING {
        CODE_PADDING
    } else {
        0
    };
    let area_width = width - 2 - 2 * padding;
    let area = code_area(&lines, area_width, ctx.options.line_numbers, ctx);
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
    join_gutter(&mut out, gutter, padding, title.as_ref(), ctx);
    out
}

/// Blank columns between the code frame and the code inside it.
const CODE_PADDING: u16 = 1;

/// Joins the line-number gutter rule to the frame with `┬`/`┴` junctions.
///
/// Without this the gutter is a bar floating between two horizontal edges it does not
/// meet; with it the block reads as one piece of chrome.
///
/// The top junction and the language label want the same columns — a four-column gutter
/// puts the `┬` under the third letter of `rust` — and the rule used to be that the
/// label won and the junction was simply dropped. That left the gutter closed at the
/// bottom and open at the top, which reads as a box that failed to draw rather than as
/// a label that took precedence, so the two are no longer in competition: the label is
/// moved to the *right* of the junction, and the top edge comes out `╭───┬ rust ───╮`,
/// the mirror of the `╰───┴─────────╯` beneath it. `title` is therefore re-drawn here
/// rather than being handed to `Canvas::framed`, which knows only one place to put it.
fn join_gutter(out: &mut Canvas, gutter: usize, padding: u16, title: Option<&Line>, ctx: Ctx<'_>) {
    if gutter == 0 {
        return;
    }
    // Inside the frame and the padding, the rule sits two columns left of the code.
    let col = 1 + usize::from(padding) + gutter - 2;
    let set = BorderSet::ROUNDED;
    let frame = ctx.theme.code.frame;
    let last = out.height().saturating_sub(1);
    let inner = usize::from(out.width()).saturating_sub(2);
    if col < inner
        && out.row_text(0).chars().nth(col) != Some(set.horizontal)
        && let Some(title) = title
    {
        // The label is standing on the junction column. Lay the whole top edge again —
        // the old label has to go completely, not be partly overwritten — and put the
        // label back down after the junction.
        out.hline(0, 1, inner, &set.horizontal.to_string(), frame);
        let mut spaced = Line::empty();
        spaced.push(Span::new(" ", frame));
        for span in &title.spans {
            spaced.push(span.clone());
        }
        spaced.push(Span::new(" ", frame));
        let room = inner.saturating_sub(col);
        out.write_line(0, col + 1, &spaced.truncated(room), frame);
    }
    if out.row_text(0).chars().nth(col) == Some(set.horizontal) {
        out.write_str(0, col, &set.tee_down.to_string(), frame);
    }
    out.write_str(last, col, &set.tee_up.to_string(), frame);
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
fn code_area(lines: &[Line], width: u16, numbered: bool, ctx: Ctx<'_>) -> Canvas {
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
    for (row, line) in lines.iter().enumerate() {
        if gutter > 0 {
            let number = format!("{:>digits$} ", row + 1, digits = digits);
            out.write_str(row, 0, &number, theme.code.line_number);
            out.write_str(row, digits + 1, GUTTER_RULE, theme.code.frame);
        }
        out.write_line(row, gutter, line, theme.code.background);
    }
    // The gutter sits left of the clip point, so the clip below cuts code and never
    // numbers. The pager pins the same columns against its own horizontal offset; see
    // the module header.
    debug_assert!(gutter < budget || budget == 0);
    out.clip_with_marker(width, OVERFLOW_MARKER, theme.code.overflow_marker);
    out.resize_width(width, theme.code.background);
    out
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

/// The mermaid source shown as a framed code block, with the reason in its bottom edge.
///
/// The frame's top edge already names the language, so the caption says what happened,
/// not what the block is.
fn fallback(literal: &str, error: &MermaidError, width: u16, ctx: Ctx<'_>) -> Canvas {
    let theme = ctx.theme;
    let lines = bridge::highlight(Some(MERMAID), literal, theme);
    if width < 4 {
        return code_area(&lines, width, false, ctx);
    }
    let padding = if width > 2 + 2 * CODE_PADDING {
        CODE_PADDING
    } else {
        0
    };
    let area_width = width - 2 - 2 * padding;
    let inner = code_area(&lines, area_width, ctx.options.line_numbers, ctx).indent(
        padding,
        padding,
        theme.code.background,
    );
    let title = Line::styled(MERMAID, theme.code.language);
    // The bottom edge is as long as the block; a caption longer than that is elided
    // rather than hard-cut, so it never ends mid-word against the corner glyph.
    let room = usize::from(width).saturating_sub(4);
    let caption = Line::styled(
        crate::text::ellipsize(&caption(error), room),
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
    join_gutter(&mut out, gutter, padding, Some(&title), ctx);
    out
}

/// The diagram families `mdless` draws, spelled as a reader would write them.
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
/// Two things a reader needs kept apart: *mdless cannot draw this* and *this diagram is
/// wrong*. The first is our failure and must never be phrased as a syntax complaint —
/// that sends the reader hunting for a typo in a correct diagram. The second names the
/// line and quotes the offending text, which is what a compiler would do.
fn caption(error: &MermaidError) -> String {
    match error {
        // Every family in `FAMILIES` parses, so an unknown keyword is not a diagram we
        // have yet to implement — it is not a diagram. Say what *is* one.
        MermaidError::UnsupportedFamily(_) => {
            format!("not a diagram type — mdless draws {}", FAMILIES.join(", "))
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
            format!("mdless could not draw this diagram — please report: {message}")
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
