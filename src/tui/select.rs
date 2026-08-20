// SPDX-License-Identifier: MIT
//! Mouse text selection, mapped back onto the document source.
//!
//! With mouse capture on, the terminal's own drag-select is gone (see
//! [`Config::mouse`](crate::config::Config::mouse)), so the pager draws its own. What
//! makes that worth doing rather than merely a replacement: the pager knows which
//! *source* bytes produced each cell, so a drag over a rendered heading `◆ Wide
//! diagram` yields `# Wide diagram` and a drag over **bold** yields `**bold**`. The
//! reader copies the Markdown they came for, not a screenshot of it.
//!
//! # How the mapping works
//!
//! It is an inversion of search, not new plumbing. [`Canvas`] already carries
//! [`SearchSpan`]s — *source byte range → row, col, cols* — recorded by
//! [`render::inline`](crate::render::inline) and translated through every `blit`,
//! `indent`, `append` and `slice_rows` by `Canvas::merge_metadata`. `search` walks them
//! forwards, source → cells; [`extract`] walks them backwards, cells → source.
//!
//! # The four decisions
//!
//! **1. A selection is a hull of source bytes, taken verbatim.** Every span the
//! selected cells touch contributes the byte range it actually covers; the result is
//! `source[lo..hi]` where `lo` is the lowest and `hi` the highest byte covered. Nothing
//! is synthesised and nothing between the ends is dropped. This is what gives
//! multi-row selections *source* line structure rather than the renderer's: a reflowed
//! paragraph has wraps that exist nowhere in the source, and the hull simply does not
//! contain them — it contains the newlines the author typed. It is also why dragging
//! from a paragraph above a code fence to one below it yields the fence, verbatim,
//! including its fence lines: they lie between the ends of the hull. The one
//! qualification is decision 2b's: inside a diagram the container prefix comes off, so
//! what is copied out of a block quote is Mermaid a reader can paste.
//!
//! **2. Markup adjacent to a selected edge comes with it.** After the hull is taken,
//! each end is extended outward over bytes that *no span rendered* — `#`, `**`, `- `,
//! `[`, `](url)` — stopping at a newline. The rule is self-limiting and needs no
//! special case for partial selections: if the reader's drag cut into the middle of a
//! rendered run, the byte just outside the hull *is* inside a span, so no extension
//! happens and the covered range is returned verbatim. Dragging half of `**bold**` from
//! its start gives `**bol`; dragging its middle gives `ol`. A pager must not invent
//! syntax, and unbalanced-looking output here is the honest report of an unbalanced
//! drag. What it may do is include the delimiters the reader could not have selected
//! because they were never drawn.
//!
//! **2b. A diagram is atomic.** Decision 2 is a rule for prose, where the bytes nothing
//! drew are two asterisks. On a Mermaid line almost every byte is undrawn, so the same
//! walk over a drag on `    A[Read] --> B[Draw]` lit `Read` and copied `    A[Read] --> B[`
//! — a truncated token, and the exact see/get divergence this module exists to remove.
//! So a diagram records an [`Atom`]: its drawn rectangle, and its whole fenced block.
//! A drag confined to one label copies what it went over, that label being as far as it
//! can reach; any wider drag, and any drag *pressed*
//! anywhere else inside the rectangle, takes the diagram whole — the fenced block on the
//! clipboard, opener and closer included and its container prefix stripped off every
//! line, and the whole rectangle washed on screen. [`resolve`] is where that is decided,
//! once, for both; [`Resolved::text`] is where the prefix comes off.
//!
//! **3. Content with no spans falls back to what is on screen.** Spans are recorded by
//! the inline renderer and, per line, by `render::code::code_area` (design spec §3), so
//! a Mermaid diagram and a table's frame carry none, but a fenced or indented code
//! block does (a table *cell* is a nested inline render and does too). When a selection
//! touches no span at all, the rendered text of the selected cells is returned instead,
//! and [`Extract::from_source`] says so, so the status bar can too. What is left for it
//! to answer is chrome that belongs to no atom — a table's frame, a thematic break — for
//! which the rendered cells are at least what the reader pointed at. A diagram's box art
//! no longer reaches it: decision 2b claims the whole rectangle, so a press on a border
//! resolves to the fenced block instead.
//!
//! The known limitation this leaves, stated plainly because a doc comment that hid it
//! would be the defect class this project keeps catching in itself: a drag that starts
//! in prose and *ends inside* spanless content copies only as far as the last byte the
//! renderer mapped — the paragraph, not the half of the diagram below it (a code fence
//! no longer demonstrates this: it carries spans now, so a drag ending inside one hits
//! decision 1 instead). There is no honest way to do better: the hull's far end is a
//! source offset and the cells below it have none, so any guess would either over-copy
//! the rest of the block or invent an offset. Ending the drag past the block, or inside
//! it on both ends, both give the right answer.
//!
//! **4. Coordinates are canvas coordinates, not viewport ones.** The selection is
//! anchored to the document, so scrolling — vertically or horizontally — during a drag
//! moves the viewport over a selection that stays put, which is the only behaviour that
//! makes selecting more than a screenful possible. A *resize* is the opposite case and
//! the selection is dropped: rendering is a pure function of width (design spec §3), so
//! after a reflow those canvas cells hold different text. Re-anchoring through the
//! source range is possible but would silently change what is highlighted, because the
//! hull at 100 columns is not the hull at 40; discarding is the honest answer and the
//! reader has already had the release event that copies.

use std::ops::Range;

use crate::canvas::{Atom, Canvas};
use crate::text::{display_width, graphemes};

/// A position in document-canvas coordinates.
///
/// Canvas, not viewport: see the module's fourth decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    /// The canvas row.
    pub row: usize,
    /// The canvas column.
    pub col: u16,
}

impl Pos {
    /// A position at `row`, `col`.
    pub fn new(row: usize, col: u16) -> Self {
        Self { row, col }
    }
}

/// A drag in progress, or one the reader has finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Where the button went down.
    anchor: Pos,
    /// Where the pointer is now, inclusive.
    head: Pos,
    /// Whether the button is still down.
    dragging: bool,
}

impl Selection {
    /// Starts a drag at `anchor`.
    pub fn started(anchor: Pos) -> Self {
        Self {
            anchor,
            head: anchor,
            dragging: true,
        }
    }

    /// Moves the loose end.
    pub fn drag_to(&mut self, head: Pos) {
        self.head = head;
    }

    /// Ends the drag, leaving the highlight up.
    pub fn finish(&mut self) {
        self.dragging = false;
    }

    /// Whether the button is still down.
    pub fn is_dragging(self) -> bool {
        self.dragging
    }

    /// Whether the drag never moved off the cell it started on.
    ///
    /// A click is not a selection: it would copy one character, which nobody meant.
    pub fn is_click(self) -> bool {
        self.anchor == self.head
    }

    /// The two ends in document order.
    fn ordered(self) -> (Pos, Pos) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// The rows the selection touches.
    pub fn rows(self) -> std::ops::RangeInclusive<usize> {
        let (start, end) = self.ordered();
        start.row..=end.row
    }

    /// The columns of `row` that are selected, as a half-open interval.
    ///
    /// The selection flows like text rather than covering a rectangle: the first row
    /// runs from the anchor to the end of the row, the last row from its start to the
    /// pointer, and every row between them entirely. A block selection would be the
    /// wrong shape for prose, which is what this pager is mostly showing.
    ///
    /// This is a *cell* interval — it makes no distinction between text and chrome —
    /// so [`highlighted_columns`] does not call it: the wash is built from
    /// [`source_hull`] instead, which only ever covers spans. What still calls this is
    /// [`rendered_text`], the spanless fallback (design spec §2, decision 3): a
    /// diagram's box art has no source hull to consult, so what the reader dragged
    /// over is answered the only way left — by the cells themselves.
    pub fn columns_on(self, row: usize, width: u16) -> Option<Range<u16>> {
        let (start, end) = self.ordered();
        if row < start.row || row > end.row {
            return None;
        }
        let last = end.col.saturating_add(1).min(width);
        let first = start.col.min(width);
        let range = match (row == start.row, row == end.row) {
            (true, true) => first..last.max(first),
            (true, false) => first..width,
            (false, true) => 0..last,
            (false, false) => 0..width,
        };
        (range.start < range.end).then_some(range)
    }
}

/// The text a selection yielded, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extract {
    /// The text to put on the clipboard.
    pub text: String,
    /// Whether it is document source (`true`) or the rendered cells (`false`).
    ///
    /// The status bar reports the difference: telling a reader they copied Markdown
    /// when they copied box art would be a lie of exactly the kind this project keeps
    /// finding in its own doc comments.
    pub from_source: bool,
}

/// Extracts the source behind a selection, falling back to the rendered cells.
///
/// Returns `None` when the selection covers nothing at all — an empty region, or one
/// entirely off the bottom of the canvas.
pub fn extract(canvas: &Canvas, source: &str, selection: Selection) -> Option<Extract> {
    if let Some(resolved) = resolve(canvas, source, selection) {
        let text = resolved.text(source);
        if !text.is_empty() {
            return Some(Extract {
                text,
                from_source: true,
            });
        }
    }
    let text = rendered_text(canvas, selection);
    (!text.is_empty()).then_some(Extract {
        text,
        from_source: false,
    })
}

/// What a selection resolves to: the source it copies, and the atoms it takes whole.
///
/// The single answer behind both the clipboard and the highlight. They used to be two
/// computations over the same hull, which is one refactor away from the divergence this
/// whole module exists to remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolved {
    /// First byte of the document source the selection yields.
    lo: usize,
    /// One past the last byte.
    hi: usize,
    /// Rectangles washed in full, box art included. See [`resolve`].
    washed: Vec<Atom>,
}

impl Resolved {
    /// The source bytes the clipboard gets.
    fn range(&self) -> Range<usize> {
        self.lo..self.hi
    }

    /// The text the clipboard gets: those bytes, with each atom's container prefix gone.
    ///
    /// The one place decision 1's "taken verbatim" is qualified, and only inside an atom.
    /// A diagram in a block quote is written `> ```mermaid` / `> flowchart LR`; handing
    /// that over verbatim gives a reader something they cannot paste into another
    /// document, which is the entire purpose of copying a diagram. So the quote markers
    /// come off — and the list indent of an indented one, by the same single rule.
    ///
    /// The range is spliced rather than filtered line by line: everything before a washed
    /// atom's *line* is emitted verbatim, then [`atom_text`] answers for the whole block,
    /// then the walk resumes past its end. Cutting at the line start and not at
    /// `source_start` is what takes the opener's prefix off, and it does so for every
    /// drag shape — a press on the box art, or a drag down from the quoted prose above,
    /// both put those bytes inside the range, which a rule keyed on "the first line of
    /// the range begins past the prefix" gets wrong exactly there.
    ///
    /// Text outside an atom is untouched. A drag that leaves a quoted diagram for the
    /// quoted prose below it keeps the prose's `> ` — that is decision 1, and the reader
    /// selected prose, not a diagram. The prose *above* one keeps it for the same reason.
    ///
    /// [`resolve`] widens the range over every washed atom, so an atom in `washed` is
    /// covered whole; the bounds checks below only keep a hand-built [`Resolved`] honest.
    fn text(&self, source: &str) -> String {
        let raw = source.get(self.range()).unwrap_or_default();
        if self.washed.is_empty() {
            return raw.to_string();
        }
        let mut atoms: Vec<&Atom> = self.washed.iter().collect();
        atoms.sort_by_key(|atom| atom.source_start);
        let mut out = String::with_capacity(raw.len());
        let mut at = self.lo;
        for atom in atoms {
            let head = line_start(source, atom.source_start);
            if at < head {
                out.push_str(source.get(at..head).unwrap_or_default());
            }
            out.push_str(&atom_text(source, atom));
            at = at.max(atom.source_end);
        }
        if at < self.hi {
            out.push_str(source.get(at..self.hi).unwrap_or_default());
        }
        out
    }
}

/// An atom's whole block, as a reader could paste it: no container prefix on any line.
///
/// Three kinds of line, and none of them is guessed from a pattern:
///
/// - **The opener.** comrak records a fenced block as starting at the backticks, which is
///   already past the prefix, so the opener is `source_start` to the end of its line.
/// - **The content.** [`Atom::content`] is what comrak handed the renderer, with the
///   prefix off — one literal line per source line, in order, because a container strips
///   line by line and never merges or splits one. Each is checked back against the
///   document as a *suffix* of its source line, which is `doc::convert::code_lines`'
///   discipline run over the same pair of texts: it recognises `>` and `> ` and a bare
///   `>` on a blank line as the prefixes they each are, rather than requiring every line
///   to repeat the first line's bytes. A line that does not match — a tab-indented one,
///   where comrak's expansion means the content is no longer a suffix of the source — is
///   left exactly as the document has it. That degradation is the point: an unstripped
///   line is a line the reader can still read, where a mis-stripped one is broken
///   Mermaid.
/// - **The closer**, and anything after the content. It carries no literal to match, so
///   it is cut at the first fence character — the byte `source_start` itself points at.
///   A container prefix cannot contain that character (a quote marker is `>`, a list
///   indent is spaces), so the cut is exact rather than a guess about what `> ` looks
///   like. A block that ends at EOF has no closing fence and simply has no such line.
fn atom_text(source: &str, atom: &Atom) -> String {
    let end = atom.source_end.min(source.len());
    let start = atom.source_start.min(end);
    let mut out = String::with_capacity(end - start);
    let mut at = line_end(source, start, end);
    out.push_str(&source[start..at]);
    let fence = source[start..].chars().next();
    let mut content = atom.content.split_inclusive('\n');
    while at < end {
        let stop = line_end(source, at, end);
        let line = &source[at..stop];
        let kept = match content.next() {
            Some(literal) => strip_to_content(line, literal),
            None => fence.map_or(line, |fence| {
                line.find(fence).map_or(line, |cut| &line[cut..])
            }),
        };
        out.push_str(kept);
        at = stop;
    }
    out
}

/// One source line of a block, with whatever the container put in front of `literal` off.
///
/// The suffix check is what makes this safe: `literal` is comrak's own answer, but it is
/// only *used* where the document agrees with it, so a line the parser and the source
/// have drifted apart on comes back untouched instead of silently rewritten. The trailing
/// newline is compared off both sides and then taken from the source line, so a blank
/// quoted line — `>` in the document, nothing at all in the literal — becomes the empty
/// line it renders as. Line endings need no case of their own: they are normalised where
/// the document is read, so both sides of the suffix check see the same one byte.
fn strip_to_content<'a>(line: &'a str, literal: &str) -> &'a str {
    let body = line.strip_suffix('\n').unwrap_or(line);
    let wanted = literal.strip_suffix('\n').unwrap_or(literal);
    if body.len() >= wanted.len() && body.ends_with(wanted) {
        &line[body.len() - wanted.len()..]
    } else {
        line
    }
}

/// The offset just past the line `at` starts on, never beyond `end`.
///
/// The newline is part of the line, so the pieces concatenate back into the text.
fn line_end(source: &str, at: usize, end: usize) -> usize {
    source[at..end]
        .find('\n')
        .map_or(end, |newline| at + newline + 1)
}

/// What a selection means, for the clipboard and the highlight alike.
///
/// Three answers, in the order they are tried.
///
/// **A diagram is atomic** (design spec §2.2). A diagram records an [`Atom`] naming its
/// drawn rectangle and its whole fenced block, and its labels are the only spans inside
/// that block. So:
///
/// 1. **The press landed inside the rectangle but on no label.** Box art, an arrow, an
///    interior blank, the padding beside a box: the reader took hold of the *drawing*,
///    not of anything in it, so the drag takes the diagram whole from the outset,
///    wherever it is released. Without this case a drag confined to box art touched no
///    label, found no hull worth the name, and left the clipboard on the drawn-cells
///    fallback while the highlight stayed empty — copying something and showing nothing,
///    which is the see/get shape this module exists to remove.
/// 2. **The hull lies inside one label.** The reader is pointing at one box, and the
///    answer is the hull exactly as it stands: the characters they dragged over, and
///    nothing else. Note what this does *not* do: it does not run [`extend_over_markup`],
///    because on a Mermaid line almost every byte is undrawn and the walk would swallow
///    `A[`, the arrow and half of the next box, none of which the reader saw light up.
///    That exclusion is the whole of the case — a partial hull inside a label is already
///    the right answer, and widening it is the only thing that could spoil it. Nor is
///    anything washed beyond the spans the hull covers, so the untouched half of a label
///    stays dark.
/// 3. **The hull touches a diagram and is wider than one of its labels.** Crossing from
///    one label into another, or leaving the diagram entirely: the diagram contributes
///    its whole fenced block, fence lines included, and the drag's own hull contributes
///    whatever else it covered. Widening the hull to cover the block gives exactly that,
///    in document order and with no second concatenation step — a drag that starts in a
///    diagram and ends below it yields the block and then the text under it.
/// 4. **No diagram involved.** The hull, widened over adjacent markup, as before.
///
/// Case 1 is the *only* thing here judged on a screen position, and it is judged on one
/// cell — the anchor — never on the drag's shape. That matters: comparing the drag's
/// cells against the rectangle would be a second, geometric rule able to disagree with
/// the source-range one, which §2.2 refuses. Asking which atom, if any, holds the cell
/// the button went down on cannot disagree with anything, because it decides *before*
/// there is a range to disagree with. It is also what the reader experiences: the press
/// is the moment they choose what they are selecting.
///
/// The predicate for "confined to one label" is stated on the **hull**, not on the two
/// screen positions, and that is deliberate. An endpoint that landed on chrome — a
/// border, an arrow, a box's blank interior — has already been resolved to a text offset
/// by §2.1, so a drag from a label out onto the arrow beside it is confined and copies
/// that label, which is what the highlight shows. Anything an endpoint resolution can
/// reach is therefore judged the same way as anything a reader dragged over directly,
/// and there is no second, screen-shaped rule to disagree with this one.
///
/// A box holds one label, so two distinct labels are two boxes (or a box and a subgraph
/// title) and always widen. What is counted is each span's [`SearchSpan::unit`] — the
/// label a span is one piece of — and not the span's own range: a label wraps onto
/// several rows and is cut at a decoded entity, so counting ranges would read one box as
/// a crossing between several. A span with no unit stands for itself and is counted as
/// its own label, which is what anything else drawn inside a diagram would be.
pub(crate) fn resolve(canvas: &Canvas, source: &str, selection: Selection) -> Option<Resolved> {
    let pressed_on = pressed_on_chrome_of(canvas, selection);
    // A press on the drawing itself still has a hull when the drag reached text, and it
    // is kept: the diagram is added to what the drag covered, not substituted for it, so
    // a press on a border and a release in the prose below yields the block *and* the
    // prose, exactly as a press on a label and the same release does.
    let (lo, hi) = match (source_hull(canvas, source, selection), pressed_on.as_ref()) {
        (Some(hull), _) => hull,
        (None, Some(atom)) => (atom.source_start, atom.source_end),
        (None, None) => return None,
    };
    if pressed_on.is_none() {
        let mut labels: Vec<(usize, usize)> = canvas
            .spans()
            .iter()
            .filter(|span| span.source_end > lo && span.source_start < hi)
            .filter(|span| canvas.atoms().iter().any(|atom| atom.contains_span(span)))
            .map(|span| span.unit.unwrap_or((span.source_start, span.source_end)))
            .collect();
        labels.sort_unstable();
        labels.dedup();
        if let [(start, end)] = labels[..]
            && lo >= start
            && hi <= end
        {
            return Some(Resolved {
                lo,
                hi,
                washed: Vec::new(),
            });
        }
    }
    let (mut lo, mut hi) = extend_over_markup(canvas, source, lo, hi);
    let mut washed: Vec<Atom> = canvas
        .atoms()
        .iter()
        .filter(|atom| atom.source_end > lo && atom.source_start < hi)
        .cloned()
        .collect();
    if let Some(atom) = pressed_on
        && !washed.contains(&atom)
    {
        washed.push(atom);
    }
    for atom in &washed {
        // The atom's own extent, not its line's. Widening to the line start would put the
        // container prefix in the range for no gain: `Resolved::text` splices a washed
        // atom's block in whole and cuts back to the line start itself, so the prefix
        // comes off whether or not the range happens to contain it — which it does
        // already whenever `extend_over_markup` above walked `lo` back over it, the `> `
        // being undrawn markup like any other.
        lo = lo.min(atom.source_start);
        hi = hi.max(atom.source_end);
    }
    Some(Resolved { lo, hi, washed })
}

/// The atom whose drawing the button went down on, if the press missed every label in it.
///
/// The whole of case 1 in [`resolve`]: one cell, one lookup, no comparison of the drag
/// against the rectangle. A press *on* a label is `None` — that drag is judged by its
/// hull like any other, so half a label still copies the label and not the chart.
///
/// The press is placed by **row**, not by the rectangle. An atom's rows are rows the
/// block owns outright — that is the invariant [`Atom`] is dropped by `blit` to keep —
/// whereas its columns are the drawing's bounding box, which the blank margin beside a
/// narrow chart is not inside. Testing the columns too would leave that margin on the
/// drawn-cells fallback, which is the state this case exists to abolish: the reader would
/// press one column left of the border and get a clipboard with nothing lit.
fn pressed_on_chrome_of(canvas: &Canvas, selection: Selection) -> Option<Atom> {
    let at = selection.anchor;
    let atom = canvas.atoms().iter().find(|atom| atom.covers_row(at.row))?;
    let on_label = canvas.spans().iter().any(|span| {
        span.row == at.row
            && at.col >= span.col
            && at.col < span.col.saturating_add(span.cols)
            && atom.contains_span(span)
    });
    (!on_label).then(|| atom.clone())
}

/// The offset of the first byte of the line `at` lies on.
fn line_start(source: &str, at: usize) -> usize {
    source[..at.min(source.len())]
        .rfind('\n')
        .map_or(0, |newline| newline + 1)
}

/// The source range a selection covers.
///
/// Two endpoints, resolved to source offsets, and everything between them — which is
/// document order, not screen geometry. A wrapped table cell therefore continues into
/// the *next cell* rather than into whatever sits beside it on the same screen row, and
/// a drag whose corners describe a rectangle still selects what a reader would read
/// between them (design spec §2).
///
/// `end`'s column is a mouse cell — inclusive, like every other column this pager
/// hands to `columns_on` — but `offset_at` resolves a column to the byte *at* it, not
/// past it, so the far endpoint is probed one column beyond where the drag actually
/// ended. That is what makes `Bias::End`'s "end of the previous span" fallback land on
/// the end of the clicked word rather than its last-but-one byte (design spec §2.1:
/// "a release past the end of a line takes the end of the last span on that row").
///
/// The two offsets are used exactly as `offset_at` returns them, with no reordering:
/// `Bias::Start`'s and `Bias::End`'s chrome fallbacks are inverted on purpose so that a
/// drag over chrome alone yields `lo >= hi` (design spec §2, "dragging across only
/// chrome selects nothing"; see `Bias`'s doc comment). Sorting the pair back into
/// ascending order — tempting, since `start`/`end` are already in document order —
/// would silently turn that empty signal into a hull spanning from the fallback's `0`
/// to its `len()`, i.e. the whole document, which is the one answer decision 1
/// explicitly rules out.
pub(crate) fn source_hull(
    canvas: &Canvas,
    source: &str,
    selection: Selection,
) -> Option<(usize, usize)> {
    let (start, end) = selection.ordered();
    let far = Pos {
        row: end.row,
        col: end.col.saturating_add(1),
    };
    let lo = offset_at(canvas, source, start, Bias::Start)?;
    let hi = offset_at(canvas, source, far, Bias::End)?;
    (lo < hi).then_some((lo, hi))
}

/// Which way an endpoint resolves when it lands on a cell no span covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bias {
    /// The near end of the range: take the start of the next text.
    Start,
    /// The far end: take the end of the previous text.
    End,
}

/// The source byte a cell points at.
///
/// A cell inside a span is exact. A cell on chrome — a border, the gutter, padding,
/// the blank tail of a row — has no span to ask, so it resolves to the nearest text in
/// document order in the direction `bias` names. This is the only coordinate in the
/// selection that is interpreted rather than looked up (design spec §2.1).
pub(crate) fn offset_at(canvas: &Canvas, source: &str, pos: Pos, bias: Bias) -> Option<usize> {
    // Exact hit first: the cell is inside some span's drawn columns.
    for span in canvas.spans() {
        let end = span.col.saturating_add(span.cols);
        if span.row == pos.row && pos.col >= span.col && pos.col < end {
            // A span with no interior position is taken whole or not at all. `body`
            // here is `$E = mc^2$` while the cells say `E = mc²`, so counting columns
            // into it lands at an arbitrary byte — the interior indexing design spec
            // §10 rules out. The near end of a drag takes the formula's first byte and
            // the far end its last, which is how a drag that starts or ends anywhere
            // inside one yields all of it.
            if !span.copied {
                return Some(match bias {
                    Bias::Start => span.source_start,
                    Bias::End => span.source_end,
                });
            }
            let body = source
                .get(span.source_start..span.source_end)
                .unwrap_or_default();
            return Some(span.source_start + byte_at_column(body, pos.col - span.col));
        }
    }
    // Chrome. Search in READING ORDER — (row, col) across the whole canvas, not just
    // this row — because "document order" is the whole point and a drag inside a
    // diagram's blank interior has no span on its own rows at all.
    let key = (pos.row, pos.col);
    match bias {
        // The near end takes the first text at or after the cell.
        Bias::Start => canvas
            .spans()
            .iter()
            .filter(|s| (s.row, s.col) >= key)
            .min_by_key(|s| (s.row, s.col))
            .map(|s| s.source_start)
            .or(Some(source.len())),
        // The far end takes the last text at or before it.
        Bias::End => canvas
            .spans()
            .iter()
            .filter(|s| (s.row, s.col) <= key)
            .max_by_key(|s| (s.row, s.col))
            .map(|s| s.source_end)
            .or(Some(0)),
    }
}

/// The byte offset in `text` that `columns` display columns land on.
///
/// The exact inverse of the column arithmetic `search::segments_for` does in the other
/// direction, and grapheme-wise for the same reason: a double-width cluster is two
/// columns and one boundary, so counting bytes or `char`s would land inside it.
fn byte_at_column(text: &str, columns: u16) -> usize {
    let wanted = usize::from(columns);
    let mut used = 0usize;
    let mut offset = 0usize;
    for cluster in graphemes(text) {
        if used >= wanted {
            break;
        }
        used += display_width(cluster);
        offset += cluster.len();
    }
    offset
}

/// The display column that `byte` bytes into `text` land on.
///
/// `byte_at_column`'s inverse, grapheme-wise for the same reason: only whole clusters
/// consumed *before* `byte` count, so a byte offset that lands mid-cluster (which
/// should not happen for a boundary this module produces, but a defensive read is
/// cheaper than a panic) still yields a sane column rather than an inflated one.
fn column_at_byte(text: &str, byte: usize) -> u16 {
    let mut used = 0u16;
    let mut offset = 0usize;
    for cluster in graphemes(text) {
        if offset >= byte {
            break;
        }
        offset += cluster.len();
        used = used.saturating_add(u16::try_from(display_width(cluster)).unwrap_or(u16::MAX));
    }
    used
}

/// The column ranges of `row` that a selection washes.
///
/// Every span the resolved range covers, clipped to the covered part, plus any atom
/// taken whole. Chrome carries no spans, so borders, the line-number gutter, cell
/// padding and the blank tail of a row are not in the answer and no rule had to say so
/// (design spec §2). Consumes [`resolve`]'s answer rather than re-deriving one from
/// `canvas` and `selection`: a second walk would have to reproduce the far endpoint's
/// inclusive-column convention *and* the atomicity rule, and a highlight that disagreed
/// with the clipboard would be far more visible than the same slip on a payload.
///
/// **A washed atom is the one place chrome does light up**, and it is deliberate
/// (design spec §2.2): when a drag has grown past a single label, what the clipboard
/// will get is the whole fenced block, and the only honest way to show that is to fill
/// the whole rectangle — box art, arrows and interior blanks included. A wash that lit
/// only the labels would be the see/get divergence again, wearing the chrome rule as a
/// disguise. Spans inside a washed rectangle are skipped rather than painted twice.
pub(crate) fn highlighted_columns(
    canvas: &Canvas,
    source: &str,
    selection: Selection,
    row: usize,
) -> Vec<Range<u16>> {
    let Some(resolved) = resolve(canvas, source, selection) else {
        return Vec::new();
    };
    let (lo, hi) = (resolved.lo, resolved.hi);
    let mut out: Vec<Range<u16>> = resolved
        .washed
        .iter()
        .filter(|atom| atom.covers_row(row))
        .map(Atom::columns)
        .collect();
    let washed = out.clone();
    for span in canvas.spans() {
        if span.row != row || span.source_end <= lo || span.source_start >= hi {
            continue;
        }
        if washed
            .iter()
            .any(|wash| wash.start <= span.col && span.col < wash.end)
        {
            continue;
        }
        let body = source
            .get(span.source_start..span.source_end)
            .unwrap_or_default();
        let from = column_at_byte(body, lo.saturating_sub(span.source_start));
        let to = if hi >= span.source_end {
            span.cols
        } else {
            column_at_byte(body, hi - span.source_start)
        };
        let (a, b) = (span.col + from, span.col + to);
        if a < b {
            out.push(a..b);
        }
    }
    out.sort_by_key(|r| r.start);
    out
}

/// Widens `lo..hi` over source bytes the renderer never drew.
///
/// `#`, `**`, `- `, `[`, `](url)`, a fence's info string: the reader could not have
/// dragged over them, so a selection that reaches the edge of what *was* drawn is
/// taken to include the markup that made it. The walk stops at a newline, so one word
/// can never swallow the line above it, and it stops the moment it meets a byte that a
/// span does render — which is what makes a partial selection come back verbatim
/// without a special case for it.
fn extend_over_markup(
    canvas: &Canvas,
    source: &str,
    mut lo: usize,
    mut hi: usize,
) -> (usize, usize) {
    let mut covered: Vec<(usize, usize)> = canvas
        .spans()
        .iter()
        .map(|span| (span.source_start, span.source_end))
        .collect();
    covered.sort_unstable();
    let rendered = |offset: usize| {
        covered
            .iter()
            .any(|&(start, end)| (start..end).contains(&offset))
    };
    while lo > 0 {
        let Some(previous) = source[..lo].char_indices().next_back().map(|(at, _)| at) else {
            break;
        };
        if source[previous..].starts_with('\n') || rendered(previous) {
            break;
        }
        lo = previous;
    }
    while hi < source.len() {
        if source[hi..].starts_with('\n') || rendered(hi) {
            break;
        }
        let step = source[hi..].chars().next().map_or(1, char::len_utf8);
        hi += step;
    }
    (lo, hi)
}

/// The plain text of the selected cells, for content that carries no spans.
///
/// Rows are right-trimmed and joined with newlines: the canvas pads every row out to
/// its full width, and copying that padding would put a rectangle of spaces on the
/// clipboard.
fn rendered_text(canvas: &Canvas, selection: Selection) -> String {
    let width = canvas.width();
    let mut rows: Vec<String> = Vec::new();
    for row in selection.rows() {
        let Some(cells) = canvas.row(row) else { break };
        let Some(wanted) = selection.columns_on(row, width) else {
            continue;
        };
        let text: String = cells
            .iter()
            .skip(usize::from(wanted.start))
            .take(usize::from(wanted.end - wanted.start))
            .map(crate::canvas::Cell::text)
            .collect();
        rows.push(text.trim_end().to_string());
    }
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{Bias, Pos, Selection, column_at_byte, offset_at, source_hull};
    use crate::canvas::{Canvas, SearchSpan};
    use crate::theme::Theme;

    /// A one-row canvas carrying a single atomic span — the shape an inline formula
    /// takes: its drawn cells (`E = mc²`, 6 columns at `col`) are not a copy of its
    /// 11-byte source `$E = mc^2$` (design spec §10).
    fn canvas_with_atom(col: u16) -> Canvas {
        let mut canvas = Canvas::new(20, 1, Theme::default_dark().base());
        canvas.add_span(SearchSpan {
            source_start: 10,
            source_end: 21,
            unit: Some((10, 21)),
            row: 0,
            col,
            cols: 6,
            copied: false,
        });
        canvas
    }

    #[test]
    fn offset_at_takes_an_atom_whole_regardless_of_which_column_was_hit() {
        // Every column inside the span's six must answer the same two offsets: the
        // near end always the formula's first byte, the far end always its last.
        // Nothing about *where inside* the six columns the pointer landed may change
        // the answer — that is what "no interior position" means.
        let canvas = canvas_with_atom(4);
        let source = "x".repeat(30);
        for col in 4..10 {
            let pos = Pos::new(0, col);
            assert_eq!(
                offset_at(&canvas, &source, pos, Bias::Start),
                Some(10),
                "column {col}: the near end takes the formula's first byte"
            );
            assert_eq!(
                offset_at(&canvas, &source, pos, Bias::End),
                Some(21),
                "column {col}: the far end takes the formula's last byte"
            );
        }
    }

    #[test]
    fn a_drag_that_starts_and_ends_inside_a_formula_selects_the_whole_formula() {
        // The atom holds: press on column 5 (the formula's second cell), release on
        // column 8 (its fifth) — a drag entirely inside the six columns the formula
        // draws — and the hull is the formula's full source range, not the two or
        // three bytes those columns would name if they were ordinary text.
        let canvas = canvas_with_atom(4);
        let source = "x".repeat(30);
        let mut selection = Selection::started(Pos::new(0, 5));
        selection.drag_to(Pos::new(0, 8));
        let hull = source_hull(&canvas, &source, selection);
        assert_eq!(
            hull,
            Some((10, 21)),
            "a drag confined to the formula's own columns still yields all of it"
        );
    }

    #[test]
    fn column_at_byte_counts_display_width_not_bytes() {
        assert_eq!(column_at_byte("abc", 0), 0);
        assert_eq!(column_at_byte("abc", 2), 2);
        assert_eq!(column_at_byte("abc", 3), 3);
    }

    #[test]
    fn column_at_byte_handles_a_multi_byte_grapheme() {
        // 'é' is two bytes and one display column.
        let text = "café";
        assert_eq!(text.len(), 5, "the fixture must actually be multi-byte");
        assert_eq!(column_at_byte(text, 0), 0);
        assert_eq!(column_at_byte(text, 3), 3, "just before the 'é'");
        assert_eq!(
            column_at_byte(text, 5),
            4,
            "past the 'é', which is two bytes but one column"
        );
    }

    #[test]
    fn column_at_byte_handles_a_wide_grapheme() {
        // U+3000 IDEOGRAPHIC SPACE is three bytes and two display columns.
        let text = "a\u{3000}b";
        assert_eq!(text.len(), 5, "the fixture must actually be wide");
        assert_eq!(column_at_byte(text, 0), 0);
        assert_eq!(column_at_byte(text, 1), 1, "just before the wide space");
        assert_eq!(
            column_at_byte(text, 4),
            3,
            "past the wide space, which is three bytes but two columns"
        );
        assert_eq!(column_at_byte(text, 5), 4, "past the trailing 'b' too");
    }
}
