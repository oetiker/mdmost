//! The typed Mermaid diagram AST.
//!
//! [`parse`](crate::mermaid::parse::parse) turns Mermaid source into a [`Diagram`];
//! [`layout`](crate::mermaid::layout) turns a [`Diagram`] into a
//! [`Canvas`](crate::canvas::Canvas). These types are the contract between the two,
//! and they follow the central architectural rule of design spec §3: they carry
//! *semantics*, never geometry. Nothing here knows about widths, columns or glyphs.
//!
//! Three conventions hold throughout:
//!
//! * **Arenas plus trees.** Diagram families with nesting (flowchart subgraphs,
//!   composite states) store every node once in a flat `Vec` addressed by a copyable
//!   id newtype, and describe nesting with a separate tree of ids. Edges therefore
//!   never need to know how deeply their endpoints are nested.
//! * **Declaration order is preserved.** Arenas, statement lists and attribute lists
//!   are in source order, because Mermaid itself is order-sensitive (participant
//!   columns, pie slices, gantt task chains).
//! * **No stringly-typed leftovers.** Shapes, arrow terminators, cardinalities,
//!   visibility markers and directions are enums. The only `String`s left are things
//!   that genuinely are free text: identifiers, labels and format strings.

/// A block of label text.
///
/// Mermaid labels may contain the literal markup `<br>` / `<br/>` / `<br />`, which is
/// a line break inside the label and *not* HTML to be rendered (design spec §2 forbids
/// HTML rendering). The parser splits on it, so a renderer only ever sees plain lines.
///
/// Character entities such as `&lt;` are decoded here too, for the reasons set out in
/// [`entity`](crate::mermaid::entity) — after the split, so that an author who wrote
/// `&lt;br&gt;` gets the visible text `<br>` rather than a line break.
///
/// A label never contains an empty `lines` vector: an empty label is `lines == [""]`
/// only if the source really said so; otherwise use [`Label::is_empty`].
#[derive(Debug, Clone, Default)]
pub struct Label {
    /// The label's lines, in order, without trailing newlines.
    pub lines: Vec<String>,
    /// Where the raw label text sat in the mermaid source, before `<br>` splitting and
    /// entity decoding.
    ///
    /// The range covers the text as written, so `A[Parse]` gives the range of `Parse`.
    /// It is relative to the mermaid block, not the document; `render::diagram`
    /// rebases it. An empty range means "synthesised, not from the source" — what a
    /// label gets unless it records a real offset, whether because it was never read
    /// from a document ([`Label::line`]) or because [`Label::parse`] was asked to
    /// place it without one.
    pub source: std::ops::Range<usize>,
    /// The raw label text [`source`](Label::source) names, exactly as written.
    ///
    /// Kept because `lines` cannot be mapped back to bytes without it: `<br>` splitting,
    /// trimming and entity decoding all move text, and the label is the only place that
    /// knows how far. [`Label::spans_for`] is what it is for.
    ///
    /// The invariant that makes it usable is `raw.len() == source.len()` — the raw text
    /// *is* those source bytes. A label built from lines that were already split
    /// ([`Label::from_lines`]) carries a hull rather than one slice and holds an empty
    /// `raw`, which `spans_for` reads as "no per-line provenance" and fails closed on.
    raw: String,
}

/// Equality and hashing consider only the visible text, not where it came from.
///
/// `source` is provenance metadata, not part of a label's identity: a hand-built
/// [`Label::line`] and a parsed label with the same lines must compare equal, which is
/// how most of the test suite already asserts on labels. Deriving `PartialEq` /
/// `Hash` over both fields would make every such comparison depend on byte offsets
/// nobody wrote down.
///
/// A consequence worth knowing before writing a test: `assert_eq!(label, Label::line("Parse"))`
/// (or against any other hand-built `Label`) proves nothing about `source` — it passes
/// no matter what byte range the label actually carries. A test that cares about
/// provenance must assert on `label.source` directly.
impl PartialEq for Label {
    fn eq(&self, other: &Self) -> bool {
        self.lines == other.lines
    }
}

impl Eq for Label {}

impl std::hash::Hash for Label {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.lines.hash(state);
    }
}

impl Label {
    /// Builds a label from raw label text, splitting on `<br>` variants and newlines
    /// and decoding character entities in each resulting line.
    ///
    /// Leading and trailing whitespace is trimmed from every resulting line. Trimming
    /// also precedes decoding, so a leading `&nbsp;` survives as a visible space.
    ///
    /// For a call site that knows where `text` sat in the mermaid source, prefer
    /// [`Label::parse_at`] — this gives an empty [`Label::source`], the same "not from
    /// the source" value [`Label::line`] gets, rather than falsely claiming `text`
    /// began at the very start of a document.
    pub fn parse(text: &str) -> Self {
        Self {
            lines: split_lines(text),
            source: Default::default(),
            raw: text.to_string(),
        }
    }

    /// Builds a label from raw label text taken from offset `at` in the mermaid
    /// source, recording that as [`Label::source`].
    pub fn parse_at(text: &str, at: usize) -> Self {
        Self {
            lines: split_lines(text),
            source: at..at + text.len(),
            raw: text.to_string(),
        }
    }

    /// A single-line label holding `text` verbatim, with an empty [`Label::source`]:
    /// this builds labels that were never read from a document, chiefly in tests.
    pub fn line(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            lines: vec![text.clone()],
            source: Default::default(),
            raw: text,
        }
    }

    /// A label whose lines were split by the caller, from `source` bytes that are a
    /// *hull* over them rather than one slice of raw label text.
    ///
    /// The state diagram's multi-line `note … end note` is the case: its range grows
    /// line by line and covers the `note` keyword's own line endings, so no single
    /// stretch of the document is "the label text". Such a label carries no
    /// [`raw`](Label::raw) and therefore no per-line provenance — [`Label::spans_for`]
    /// declines rather than answering from a mapping it does not have.
    pub fn from_lines(lines: Vec<String>, source: std::ops::Range<usize>) -> Self {
        Self {
            lines,
            source,
            raw: String::new(),
        }
    }

    /// True when the label carries no visible text at all.
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|line| line.is_empty())
    }

    /// The label as one string, lines joined by `\n`.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// The source bytes behind one drawn piece of line `index`, run by run.
    ///
    /// A layout draws a label by wrapping [`lines`](Label::lines) and putting the
    /// resulting pieces on the canvas; this answers, for one such piece, which source
    /// bytes each part of it came from and where that part sits inside it. It is what
    /// lets a selection copy the characters a reader dragged over rather than the whole
    /// label (design spec §2.2), and a wrapped label is exactly the case that needs it:
    /// its rows would otherwise all name the same range and no column arithmetic inside
    /// one could be right.
    ///
    /// `at` is where `text` starts in `self.lines[index]`, in bytes.
    ///
    /// **Every run it returns is a byte-for-byte copy of the cells it names, or one
    /// column drawn by one entity reference.** That is the property the selection's
    /// column walks depend on (`select::offset_at`, `select::highlighted_columns`,
    /// `search::segments_for` all convert between bytes and columns *inside* a span by
    /// walking its source), so a decoded entity is cut out into a run of its own instead
    /// of being left inside a run whose bytes and cells no longer line up. An entity
    /// that draws more than one column — `&#x1F600;` — is the one thing with no honest
    /// answer and is dropped, leaving its cell dark, which is the same call
    /// `render::inline` makes for the same reason.
    ///
    /// Empty when this label has no per-line provenance to give ([`Label::raw`]), or
    /// when `text` is not the piece of line `index` it claims to be. No provenance is
    /// always better than provenance from somewhere else in the document.
    pub fn spans_for(&self, index: usize, at: usize, text: &str) -> Vec<LabelSpan> {
        if self.source.is_empty() || self.raw.len() != self.source.len() {
            return Vec::new();
        }
        let (Some(line), Some(raw)) = (self.lines.get(index), self.line_source(index)) else {
            return Vec::new();
        };
        let piece = at..at + text.len();
        if line.get(piece.clone()) != Some(text) {
            return Vec::new();
        }
        let base = self.source.start + raw.start;
        let (decoded, runs) = crate::mermaid::entity::decode_runs(&self.raw[raw.clone()]);
        if decoded != *line {
            return Vec::new();
        }
        let mut out = Vec::new();
        for run in runs {
            let lo = run.text.start.max(piece.start);
            let hi = run.text.end.min(piece.end);
            if lo >= hi {
                continue;
            }
            let cols = crate::text::display_width(&line[lo..hi]);
            let source = if run.faithful {
                let start = base + run.source.start;
                start + (lo - run.text.start)..start + (hi - run.text.start)
            } else if cols == 1 && (lo, hi) == (run.text.start, run.text.end) {
                base + run.source.start..base + run.source.end
            } else {
                continue;
            };
            out.push(LabelSpan {
                source,
                col: crate::text::display_width(&line[piece.start..lo]),
                cols,
            });
        }
        out
    }

    /// Where line `index` sits in [`raw`](Label::raw), trimmed as the line was.
    ///
    /// The same split and the same trim [`split_lines`] made, so the two cannot
    /// disagree about which bytes became which line.
    fn line_source(&self, index: usize) -> Option<std::ops::Range<usize>> {
        let raw = *split_raw(&self.raw).get(index)?;
        let text = &self.raw[raw.0..raw.1];
        let start = raw.0 + (text.len() - text.trim_start().len());
        Some(start..start + text.trim().len())
    }
}

/// One run of a drawn label piece, and the source bytes that drew it.
///
/// Columns are relative to the start of the piece [`Label::spans_for`] was asked about;
/// the layout knows where that piece landed and adds its own origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelSpan {
    /// The source byte range, in the same space as [`Label::source`].
    pub source: std::ops::Range<usize>,
    /// The first display column of the piece this run drew.
    pub col: usize,
    /// How many display columns it drew.
    pub cols: usize,
}

/// Splits raw label text into lines on `<br>` variants and newlines, trimming and
/// entity-decoding each one. Shared by [`Label::parse`] and [`Label::parse_at`], which
/// differ only in what they record as [`Label::source`].
fn split_lines(text: &str) -> Vec<String> {
    split_raw(text)
        .into_iter()
        .map(|(start, end)| line_text(&text[start..end]))
        .collect()
}

/// Where each line of `text` sits in it, before trimming: the split alone.
///
/// Split out of [`split_lines`] so that [`Label::line_source`] answers from the same
/// walk rather than from a second one that would have to be kept in step with it.
fn split_raw(text: &str) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    let mut at = 0usize;
    loop {
        match find_break(&text[at..]) {
            Some((to, len)) => {
                lines.push((at, at + to));
                at += to + len;
            }
            None => {
                lines.push((at, text.len()));
                break;
            }
        }
    }
    lines
}

/// Trims one line of a label and decodes its character entities.
fn line_text(text: &str) -> String {
    crate::mermaid::entity::decode(text.trim()).into_owned()
}

/// Finds the next line break marker, returning its byte offset and byte length.
fn find_break(text: &str) -> Option<(usize, usize)> {
    let lower = text.to_ascii_lowercase();
    let newline = text.find('\n').map(|at| (at, 1));
    let mut best = newline;
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<br") {
        let at = from + rel;
        let after = &lower[at + 3..];
        let close = after.find('>');
        match close {
            // Only `<br>`, `<br/>` and `<br />` count; `<brain>` does not.
            Some(end) if after[..end].trim().is_empty() || after[..end].trim() == "/" => {
                let len = 3 + end + 1;
                if best.is_none_or(|(b, _)| at < b) {
                    best = Some((at, len));
                }
                break;
            }
            _ => from = at + 3,
        }
    }
    best
}

/// A parsed Mermaid diagram, one variant per supported family (design spec §6).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Diagram {
    /// `flowchart`/`graph` — design spec §6.1.
    Flowchart(Flowchart),
    /// `sequenceDiagram` — design spec §6.2.
    Sequence(SequenceDiagram),
    /// `classDiagram` — design spec §6.3.
    Class(ClassDiagram),
    /// `erDiagram` — design spec §6.4.
    Er(ErDiagram),
    /// `pie` — design spec §6.5.
    Pie(PieChart),
    /// `gantt` — design spec §6.6.
    Gantt(GanttChart),
    /// `stateDiagram-v2` — design spec §6.7.
    State(StateDiagram),
}

/// The direction a graph-like diagram flows in.
///
/// Mermaid's `TD` ("top down") and `TB` ("top bottom") are the same direction and both
/// map to [`Direction::TopToBottom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    /// `TD` or `TB`. The default when a header omits the direction.
    #[default]
    TopToBottom,
    /// `BT`.
    BottomToTop,
    /// `LR`.
    LeftToRight,
    /// `RL`.
    RightToLeft,
}

// ---------------------------------------------------------------------------
// §6.1 flowchart / graph
// ---------------------------------------------------------------------------

/// Identifies a node in [`Flowchart::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

/// A `flowchart` / `graph` diagram (design spec §6.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Flowchart {
    /// The direction from the header, e.g. `flowchart LR`.
    pub direction: Direction,
    /// Every node, in declaration order. Index with [`NodeId`].
    pub nodes: Vec<FlowNode>,
    /// Every edge, in declaration order. Endpoints may live in different subgraphs.
    pub edges: Vec<FlowEdge>,
    /// The implicit top-level container. Its `key` and `title` are always `None`.
    ///
    /// Every [`NodeId`] appears in exactly one group in this tree.
    pub root: Group,
}

/// A flowchart node.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowNode {
    /// The identifier used in the source, e.g. `A` in `A[Start]`.
    pub key: String,
    /// The displayed label. Defaults to the key when the source gives no label.
    pub label: Label,
    /// The node's outline shape.
    pub shape: NodeShape,
}

/// The outline shape of a flowchart node.
///
/// Shapes outside this list (`{{hexagon}}`, `[/parallelogram/]`, `>flag]`, …) are
/// accepted and degrade to [`NodeShape::Rect`] with their label intact, as required by
/// design spec §6.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NodeShape {
    /// `A[text]`, and the fallback for unsupported shapes.
    #[default]
    Rect,
    /// `A(text)`.
    Round,
    /// `A([text])`.
    Stadium,
    /// `A{text}`.
    Rhombus,
    /// `A((text))`.
    Circle,
    /// `A[[text]]`.
    Subroutine,
    /// `A[(text)]`.
    Cylinder,
}

/// A flowchart edge.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowEdge {
    /// The source node.
    pub from: NodeId,
    /// The target node.
    pub to: NodeId,
    /// How the connecting line is drawn.
    pub stroke: EdgeStroke,
    /// The terminator at the [`from`](FlowEdge::from) end; [`ArrowHead::None`] unless
    /// the source used a back arrow such as `<-->`.
    pub tail: ArrowHead,
    /// The terminator at the [`to`](FlowEdge::to) end.
    pub head: ArrowHead,
    /// The edge label from `A -->|text| B` or `A -- text --> B`.
    pub label: Option<Label>,
}

/// How an edge line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EdgeStroke {
    /// `-->` / `---`.
    #[default]
    Solid,
    /// `-.->` / `-.-`.
    Dotted,
    /// `==>` / `===`.
    Thick,
}

/// The terminator drawn at one end of a flowchart edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ArrowHead {
    /// No terminator, as in `A --- B`.
    #[default]
    None,
    /// A plain arrowhead, as in `A --> B`.
    Arrow,
}

/// A `subgraph` … `end` container, or the implicit top-level container.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    /// The subgraph identifier, e.g. `one` in `subgraph one [Title]`. `None` for the
    /// implicit root group and for anonymous subgraphs.
    pub key: Option<String>,
    /// The subgraph title. `None` for the implicit root group.
    pub title: Option<Label>,
    /// A `direction` statement inside the subgraph, if any.
    pub direction: Option<Direction>,
    /// Nodes declared directly in this container, in declaration order.
    pub nodes: Vec<NodeId>,
    /// Nested subgraphs, in declaration order.
    pub children: Vec<Group>,
}

// ---------------------------------------------------------------------------
// §6.2 sequenceDiagram
// ---------------------------------------------------------------------------

/// Identifies a participant in [`SequenceDiagram::participants`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParticipantId(pub usize);

/// A `sequenceDiagram` (design spec §6.2).
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceDiagram {
    /// The `title` statement, if any.
    ///
    /// A plain `String`, like every other chart title and section heading here: titles
    /// are drawn by the shared chrome in [`chrome::compose`](crate::mermaid::chrome),
    /// which centres and ellipsizes them, and none of them is a label an author selects
    /// text out of. Making them [`Label`]s would buy provenance for the one piece of a
    /// diagram that is furniture rather than content.
    pub title: Option<String>,
    /// Participants in column order: declared ones first in declaration order, then
    /// implicit ones in order of first use.
    pub participants: Vec<Participant>,
    /// The diagram body, in source order.
    pub items: Vec<SequenceItem>,
}

/// A lifeline column.
#[derive(Debug, Clone, PartialEq)]
pub struct Participant {
    /// The identifier used in messages, e.g. `A` in `participant A as Alice`.
    pub key: String,
    /// The displayed label; equal to the key when there is no `as` alias.
    pub label: Label,
    /// Whether the participant was declared with `actor` rather than `participant`.
    pub kind: ParticipantKind,
}

/// How a participant is drawn at the head of its lifeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ParticipantKind {
    /// `participant A`, or an implicitly created participant: a box.
    #[default]
    Participant,
    /// `actor A`: a stick figure.
    Actor,
}

/// One statement in a sequence diagram body.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SequenceItem {
    /// A message between two participants; `from == to` is a self-message.
    Message(Message),
    /// A `Note left of|right of|over …` statement.
    Note(Note),
    /// An explicit `activate X` statement.
    Activate(ParticipantId),
    /// An explicit `deactivate X` statement.
    Deactivate(ParticipantId),
    /// A `loop`/`alt`/`opt`/`par`/`critical` frame.
    Block(SequenceBlock),
}

/// A message arrow.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// The sending participant.
    pub from: ParticipantId,
    /// The receiving participant.
    pub to: ParticipantId,
    /// How the arrow shaft is drawn.
    pub line: MessageLine,
    /// The terminator at the receiving end.
    pub head: MessageHead,
    /// The message text (the part after the `:`).
    pub label: Label,
    /// `true` for the `+` shorthand, which activates [`to`](Message::to).
    pub activates: bool,
    /// `true` for the `-` shorthand, which deactivates [`to`](Message::to).
    pub deactivates: bool,
}

/// How a message arrow's shaft is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MessageLine {
    /// `->`, `->>`, `-x`.
    #[default]
    Solid,
    /// `-->`, `-->>`, `--x`.
    Dotted,
}

/// The terminator at the receiving end of a message arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MessageHead {
    /// A bare line end, as in `A->B` / `A-->B`.
    #[default]
    None,
    /// A filled arrowhead, as in `A->>B` / `A-->>B`.
    Arrow,
    /// A cross, as in `A-xB` / `A--xB`.
    Cross,
}

/// A `Note` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Where the note sits relative to its participants.
    pub placement: NotePlacement,
    /// The participants the note is attached to. `Note over A,B` yields two.
    pub participants: Vec<ParticipantId>,
    /// The note text.
    pub text: Label,
}

/// Where a note is placed relative to the thing it annotates.
///
/// Shared by sequence diagrams (all three variants) and state diagrams, which use only
/// [`NotePlacement::LeftOf`] and [`NotePlacement::RightOf`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotePlacement {
    /// `note left of X`.
    LeftOf,
    /// `note right of X`.
    RightOf,
    /// `note over X[,Y]`.
    Over,
}

/// A labelled frame around a run of sequence statements.
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceBlock {
    /// Which keyword opened the frame.
    pub kind: BlockKind,
    /// The frame's branches. `loop` and `opt` always have exactly one; `alt`, `par`
    /// and `critical` have one per `else` / `and` / `option` continuation.
    pub branches: Vec<Branch>,
}

/// The keyword that opened a [`SequenceBlock`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// `loop … end`.
    Loop,
    /// `alt … else … end`.
    Alt,
    /// `opt … end`.
    Opt,
    /// `par … and … end`.
    Par,
    /// `critical … option … end`.
    Critical,
}

/// One branch of a [`SequenceBlock`].
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    /// The text after the opening or continuation keyword, e.g. `alt is sick` gives
    /// `is sick`. `None` when the keyword carried no text.
    pub label: Option<Label>,
    /// The statements inside this branch, in source order.
    pub items: Vec<SequenceItem>,
}

// ---------------------------------------------------------------------------
// §6.3 classDiagram
// ---------------------------------------------------------------------------

/// Identifies a class in [`ClassDiagram::classes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassId(pub usize);

/// A `classDiagram` (design spec §6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDiagram {
    /// A `direction` statement, if any.
    pub direction: Option<Direction>,
    /// Every class, in declaration order. Index with [`ClassId`].
    pub classes: Vec<Class>,
    /// Every relation, in declaration order.
    pub relations: Vec<ClassRelation>,
}

/// A class box: name, optional annotation, fields and methods.
///
/// [`members`](Class::members) carry no provenance: `+int age` is drawn as `+age: int`,
/// reordered out of tokens the source wrote apart, so no stretch of the document is a
/// copy of the drawn cells. Only [`name`](Class::name) maps back.
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    /// The class name, e.g. `Animal`. A generic parameter written `Square~Shape~` is
    /// *not* part of the name — Mermaid identifies the class as `Square` — and lives
    /// in [`generic`](Class::generic) instead.
    pub name: Label,
    /// The generic parameter from `Square~Shape~`, without its tildes. Member types
    /// keep theirs inline, rewritten as `List<int>`.
    pub generic: Option<String>,
    /// An `<<interface>>` / `<<abstract>>` style annotation.
    pub annotation: Option<ClassAnnotation>,
    /// Members in declaration order; the renderer splits them into the field and
    /// method compartments itself.
    pub members: Vec<Member>,
}

/// A `<<…>>` stereotype on a class.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ClassAnnotation {
    /// `<<interface>>`.
    Interface,
    /// `<<abstract>>`.
    Abstract,
    /// `<<enumeration>>`.
    Enumeration,
    /// `<<service>>`.
    Service,
    /// Any other stereotype, kept verbatim without the angle brackets.
    Other(String),
}

/// A class member.
#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    /// An attribute, e.g. `+int age` or `+age: int`.
    Field(Field),
    /// An operation, e.g. `+isMammal() bool`.
    Method(Method),
}

/// A class attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The visibility marker, when the source gave one.
    pub visibility: Option<Visibility>,
    /// The attribute name.
    pub name: String,
    /// The declared type, from either `+int age` or `+age: int`.
    pub ty: Option<String>,
    /// A trailing `$` (static) or `*` (abstract) classifier.
    pub classifier: Option<Classifier>,
}

/// A class operation.
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    /// The visibility marker, when the source gave one.
    pub visibility: Option<Visibility>,
    /// The method name, without parentheses.
    pub name: String,
    /// The declared parameters, in order.
    pub params: Vec<Param>,
    /// The return type written after the parameter list, if any.
    pub returns: Option<String>,
    /// A trailing `$` (static) or `*` (abstract) classifier.
    pub classifier: Option<Classifier>,
}

/// One parameter of a [`Method`].
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// The parameter name, or the whole token when only a type was written.
    pub name: String,
    /// The parameter type, when the source wrote both a type and a name.
    pub ty: Option<String>,
}

/// A UML visibility marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// `+`.
    Public,
    /// `-`.
    Private,
    /// `#`.
    Protected,
    /// `~`.
    PackageInternal,
}

/// A trailing member classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Classifier {
    /// `$` — static.
    Static,
    /// `*` — abstract.
    Abstract,
}

/// A relation between two classes.
///
/// Mermaid writes the terminator of each end into the operator, so the AST stores the
/// two ends independently: `Animal <|-- Duck` is
/// `left = Animal, left_end = Triangle, right = Duck, right_end = None, line = Solid`.
/// Use [`ClassRelation::kind`] for the conventional UML name.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassRelation {
    /// The class written to the left of the operator.
    pub left: ClassId,
    /// The class written to the right of the operator.
    pub right: ClassId,
    /// The terminator drawn at the left class.
    pub left_end: ClassArrow,
    /// The terminator drawn at the right class.
    pub right_end: ClassArrow,
    /// How the connecting line is drawn.
    pub line: LineStyle,
    /// The cardinality quoted before the operator, e.g. `"1"` in `A "1" -- "*" B`.
    pub left_cardinality: Option<String>,
    /// The cardinality quoted after the operator.
    pub right_cardinality: Option<String>,
    /// The relation label written after the `:`.
    pub label: Option<Label>,
}

impl ClassRelation {
    /// The conventional UML name for this relation, derived from its ends.
    ///
    /// Returns `None` for combinations UML has no single name for, such as a relation
    /// with a terminator at both ends.
    pub fn kind(&self) -> Option<ClassRelationKind> {
        let (plain, decorated) = match (self.left_end, self.right_end) {
            (ClassArrow::None, other) => (ClassArrow::None, other),
            (other, ClassArrow::None) => (ClassArrow::None, other),
            _ => return None,
        };
        debug_assert_eq!(plain, ClassArrow::None);
        Some(match (decorated, self.line) {
            (ClassArrow::Triangle, LineStyle::Solid) => ClassRelationKind::Inheritance,
            (ClassArrow::Triangle, LineStyle::Dashed) => ClassRelationKind::Realization,
            (ClassArrow::FilledDiamond, _) => ClassRelationKind::Composition,
            (ClassArrow::HollowDiamond, _) => ClassRelationKind::Aggregation,
            (ClassArrow::Arrow, LineStyle::Solid) => ClassRelationKind::Association,
            (ClassArrow::Arrow, LineStyle::Dashed) => ClassRelationKind::Dependency,
            (ClassArrow::None, LineStyle::Solid) => ClassRelationKind::Link,
            (ClassArrow::None, LineStyle::Dashed) => ClassRelationKind::DashedLink,
        })
    }
}

/// The terminator drawn at one end of a class relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClassArrow {
    /// No terminator.
    #[default]
    None,
    /// `<|` / `|>` — a hollow triangle (inheritance, realization).
    Triangle,
    /// `*` — a filled diamond (composition).
    FilledDiamond,
    /// `o` — a hollow diamond (aggregation).
    HollowDiamond,
    /// `<` / `>` — an open arrowhead (association, dependency).
    Arrow,
}

/// How a class or ER relation line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineStyle {
    /// `--` — a solid line.
    #[default]
    Solid,
    /// `..` — a dashed line.
    Dashed,
}

/// The conventional UML name of a [`ClassRelation`], derived by
/// [`ClassRelation::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ClassRelationKind {
    /// `<|--`.
    Inheritance,
    /// `*--`.
    Composition,
    /// `o--`.
    Aggregation,
    /// `-->`.
    Association,
    /// `..>`.
    Dependency,
    /// `..|>`.
    Realization,
    /// `--` — a plain solid link.
    Link,
    /// `..` — a plain dashed link.
    DashedLink,
}

// ---------------------------------------------------------------------------
// §6.4 erDiagram
// ---------------------------------------------------------------------------

/// Identifies an entity in [`ErDiagram::entities`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub usize);

/// An `erDiagram` (design spec §6.4).
#[derive(Debug, Clone, PartialEq)]
pub struct ErDiagram {
    /// Every entity, in declaration order. Index with [`EntityId`].
    pub entities: Vec<Entity>,
    /// Every relationship, in declaration order.
    pub relationships: Vec<ErRelationship>,
}

/// An entity box with its attribute block.
#[derive(Debug, Clone, PartialEq)]
///
/// [`attributes`](Entity::attributes) carry no provenance: an attribute row is drawn as
/// a column-aligned table built from four independent tokens, so no stretch of the
/// source is a copy of the drawn cells and there is nothing honest to point at.
pub struct Entity {
    /// The entity name as written, e.g. `CUSTOMER`.
    pub name: Label,
    /// An alias from `CUSTOMER["Customer account"]`, if any.
    pub alias: Option<Label>,
    /// Attributes from the `{ … }` block, in declaration order.
    pub attributes: Vec<ErAttribute>,
}

/// One line of an entity's attribute block: `string name PK "comment"`.
#[derive(Debug, Clone, PartialEq)]
pub struct ErAttribute {
    /// The attribute type, e.g. `string`.
    pub ty: String,
    /// The attribute name, e.g. `name`.
    pub name: String,
    /// Key markers, in the order written. Mermaid allows several, e.g. `PK, FK`.
    pub keys: Vec<ErKey>,
    /// The quoted trailing comment, without its quotes.
    pub comment: Option<String>,
}

/// A key marker on an ER attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErKey {
    /// `PK` — primary key.
    Primary,
    /// `FK` — foreign key.
    Foreign,
    /// `UK` — unique key.
    Unique,
}

/// A relationship between two entities, e.g. `CUSTOMER ||--o{ ORDER : places`.
#[derive(Debug, Clone, PartialEq)]
pub struct ErRelationship {
    /// The entity written to the left of the operator.
    pub left: EntityId,
    /// The entity written to the right of the operator.
    pub right: EntityId,
    /// The crow's-foot cardinality drawn at the left entity.
    pub left_cardinality: ErCardinality,
    /// The crow's-foot cardinality drawn at the right entity.
    pub right_cardinality: ErCardinality,
    /// `--` is an identifying relationship (solid); `..` is non-identifying (dashed).
    pub line: LineStyle,
    /// The relationship label written after the `:`.
    pub label: Option<Label>,
}

/// A crow's-foot cardinality at one end of an ER relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErCardinality {
    /// `|o` / `o|` — zero or one.
    ZeroOrOne,
    /// `||` — exactly one.
    ExactlyOne,
    /// `}o` / `o{` — zero or more.
    ZeroOrMore,
    /// `}|` / `|{` — one or more.
    OneOrMore,
}

// ---------------------------------------------------------------------------
// §6.5 pie
// ---------------------------------------------------------------------------

/// A `pie` chart (design spec §6.5), rendered as a sorted horizontal bar chart.
#[derive(Debug, Clone, PartialEq)]
pub struct PieChart {
    /// The chart title from `pie title X` or a following `title X` line.
    pub title: Option<String>,
    /// Whether `showData` was given, which asks for raw values beside percentages.
    pub show_data: bool,
    /// The slices in declaration order; the renderer sorts them itself.
    pub slices: Vec<PieSlice>,
}

/// One `"label" : value` entry of a pie chart.
#[derive(Debug, Clone, PartialEq)]
pub struct PieSlice {
    /// The quoted slice label, without its quotes.
    pub label: Label,
    /// The slice value. Always finite and non-negative.
    pub value: f64,
}

// ---------------------------------------------------------------------------
// §6.6 gantt
// ---------------------------------------------------------------------------

/// A `gantt` chart (design spec §6.6).
///
/// Task dates are fully resolved at parse time: `dateFormat` has been applied and
/// `after`/duration chains have been followed, so the renderer only has to map an
/// interval of seconds onto columns.
#[derive(Debug, Clone, PartialEq)]
pub struct GanttChart {
    /// The `title` statement, if any.
    pub title: Option<String>,
    /// The `axisFormat` string, kept verbatim for tick labelling.
    pub axis_format: Option<String>,
    /// Sections in declaration order. Tasks written before the first `section`
    /// statement land in a leading section whose `title` is `None`.
    pub sections: Vec<GanttSection>,
}

impl GanttChart {
    /// The chart's overall time span as `(start, end)` in seconds since the Unix
    /// epoch, or `None` when the chart has no tasks.
    pub fn span(&self) -> Option<(i64, i64)> {
        let mut span: Option<(i64, i64)> = None;
        for task in self
            .sections
            .iter()
            .flat_map(|section| section.tasks.iter())
        {
            span = Some(match span {
                None => (task.start, task.end),
                Some((lo, hi)) => (lo.min(task.start), hi.max(task.end)),
            });
        }
        span
    }
}

/// A `section` of a gantt chart.
#[derive(Debug, Clone, PartialEq)]
pub struct GanttSection {
    /// The section heading; `None` for the implicit leading section.
    pub title: Option<String>,
    /// The section's tasks, in declaration order.
    pub tasks: Vec<GanttTask>,
}

/// One gantt task or milestone.
#[derive(Debug, Clone, PartialEq)]
pub struct GanttTask {
    /// The task name, i.e. the text before the `:`.
    pub name: Label,
    /// The task id, when the metadata gave one; other tasks refer to it with `after`.
    pub id: Option<String>,
    /// The task's progress state from the `done` / `active` tags.
    pub progress: TaskProgress,
    /// Whether the task carried the `crit` tag.
    pub critical: bool,
    /// Whether the task carried the `milestone` tag. Milestones are drawn as a marker
    /// at [`start`](GanttTask::start) regardless of their length.
    pub milestone: bool,
    /// Resolved start, in seconds since the Unix epoch (UTC).
    pub start: i64,
    /// Resolved end, in seconds since the Unix epoch (UTC). Never before `start`.
    pub end: i64,
}

/// A gantt task's progress state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TaskProgress {
    /// No `done` or `active` tag: work not started.
    #[default]
    Planned,
    /// The `active` tag.
    Active,
    /// The `done` tag.
    Done,
}

// ---------------------------------------------------------------------------
// §6.7 stateDiagram-v2
// ---------------------------------------------------------------------------

/// Identifies a state in [`StateDiagram::states`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateId(pub usize);

/// A `stateDiagram-v2` (design spec §6.7).
#[derive(Debug, Clone, PartialEq)]
pub struct StateDiagram {
    /// A top-level `direction` statement, if any.
    pub direction: Option<Direction>,
    /// Every state at every nesting depth, in declaration order. Index with
    /// [`StateId`].
    pub states: Vec<StateNode>,
    /// The top-level scope.
    pub root: StateScope,
}

/// A state box.
#[derive(Debug, Clone, PartialEq)]
pub struct StateNode {
    /// The identifier used in transitions.
    pub key: String,
    /// The description from `state "text" as s` or `s : text`. `None` means the
    /// renderer should display the key.
    pub label: Option<Label>,
    /// What kind of state this is, including composite children.
    pub kind: StateKind,
}

/// The kind of a [`StateNode`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StateKind {
    /// An ordinary state, drawn as a rounded box.
    Simple,
    /// `state X <<choice>>`, drawn as a diamond.
    Choice,
    /// `state X <<fork>>`, drawn as a bar.
    Fork,
    /// `state X <<join>>`, drawn as a bar.
    Join,
    /// `state X { … }`, drawn as a frame around its own scope.
    Composite(StateScope),
}

/// The transitions, child states and notes belonging to one nesting level.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateScope {
    /// A `direction` statement inside this scope, if any.
    pub direction: Option<Direction>,
    /// States declared directly in this scope, in declaration order.
    pub states: Vec<StateId>,
    /// Transitions declared in this scope, in declaration order.
    pub transitions: Vec<Transition>,
    /// Notes declared in this scope, in declaration order.
    pub notes: Vec<StateNote>,
}

/// A transition between two endpoints of the same scope.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// The source endpoint.
    pub from: StateEndpoint,
    /// The target endpoint.
    pub to: StateEndpoint,
    /// The label written after the `:`.
    pub label: Option<Label>,
}

/// One end of a [`Transition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateEndpoint {
    /// `[*]` used as a source: the scope's start marker.
    Initial,
    /// `[*]` used as a target: the scope's end marker.
    Final,
    /// A named state.
    State(StateId),
}

/// A `note left of X` / `note right of X` attached to a state.
#[derive(Debug, Clone, PartialEq)]
pub struct StateNote {
    /// Which side the note sits on; never [`NotePlacement::Over`].
    pub placement: NotePlacement,
    /// The annotated state.
    pub target: StateId,
    /// The note text.
    pub text: Label,
}
