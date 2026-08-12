//! The engine's input types: what a caller describes, and how nodes are drawn.
//!
//! A caller builds a [`GraphSpec`] — pure topology, no geometry — and supplies a
//! [`NodeArt`] that knows how to draw one node. Everything visual about a node lives
//! behind that one seam, which is what lets flowchart, class, ER and state diagrams
//! share the whole layout engine (design spec §6.3, §6.4, §6.7).

use crate::canvas::Canvas;
use crate::mermaid::ast::{Direction, Label};
use crate::mermaid::chrome::{self, Piece};
use crate::theme::Theme;

pub use super::glyph::Stroke;

/// A label the engine draws itself: the rows to paint, and the label they came from.
///
/// The engine draws two kinds of text of its own — the label in the middle of an edge
/// and the title on a frame — and both used to arrive here as a bare `Vec<String>` the
/// caller had already wrapped. That threw away which bytes of the document drew which
/// row, so an edge label and a subgraph title could not be selected the way a node label
/// can. Keeping the [`Label`] beside its wrapped rows is what puts
/// [`Label::spans_for`] back within reach of the point of drawing.
///
/// The rows are [`Piece`]s, so wrapping is the one in
/// [`chrome::label_pieces`](crate::mermaid::chrome::label_pieces) that every family
/// already goes through, and the spans come out of
/// [`chrome::label_spans`](crate::mermaid::chrome::label_spans) — one per run, never one
/// naming the whole label, which is the only shape a *wrapped* label can be selected
/// through.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawnLabel {
    /// The label these rows were drawn from, provenance and all.
    pub label: Label,
    /// One piece per drawn row, in drawing order.
    pub rows: Vec<Piece>,
}

impl DrawnLabel {
    /// A label wrapped to `width` columns, the way an edge label is drawn.
    pub fn wrapped(label: &Label, width: usize) -> Self {
        Self {
            label: label.clone(),
            rows: chrome::label_pieces(label, width),
        }
    }

    /// A label whose own `<br>`-separated lines are the rows, the way a frame title is.
    pub fn whole(label: &Label) -> Self {
        Self {
            label: label.clone(),
            rows: chrome::label_rows(label),
        }
    }

    /// True when there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// How many rows are drawn.
    pub fn height(&self) -> usize {
        self.rows.len()
    }

    /// The width of the widest drawn row, in columns.
    pub fn width(&self) -> usize {
        self.rows
            .iter()
            .map(|row| crate::text::display_width(&row.text))
            .max()
            .unwrap_or(0)
    }

    /// The drawn rows as text, for callers that only need what is painted.
    pub fn lines(&self) -> Vec<&str> {
        self.rows.iter().map(|row| row.text.as_str()).collect()
    }
}

/// Identifies a node. Ids are dense: `NodeIdx(0)` .. `NodeIdx(node_count - 1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIdx(pub usize);

/// The graph to lay out.
///
/// Nodes are addressed by index only; their appearance comes from the [`NodeArt`]
/// passed alongside. Every node index must appear in exactly one group of [`root`].
///
/// [`root`]: GraphSpec::root
#[derive(Debug, Clone, PartialEq)]
pub struct GraphSpec {
    /// The direction the graph flows in.
    pub direction: Direction,
    /// How many nodes there are; valid ids are `NodeIdx(0..node_count)`.
    pub node_count: usize,
    /// Every edge, in a caller-chosen deterministic order.
    pub edges: Vec<EdgeSpec>,
    /// The implicit top-level container; its `title` is `None`.
    pub root: GroupSpec,
}

impl GraphSpec {
    /// An empty graph flowing in `direction`.
    pub fn new(direction: Direction) -> Self {
        Self {
            direction,
            node_count: 0,
            edges: Vec::new(),
            root: GroupSpec::default(),
        }
    }
}

/// A container of nodes drawn as a titled frame — a flowchart `subgraph` or a
/// composite state.
///
/// Containers are laid out recursively: a group is laid out on its own and then takes
/// part in its parent's layout as a single node-sized box. Nesting therefore needs no
/// special handling anywhere else in the engine.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupSpec {
    /// The frame title, split into rows. `None` draws no frame, which is what the
    /// implicit root group uses.
    ///
    /// **Carries its provenance**: the [`Label`] travels with the rows, so the title a
    /// flowchart subgraph or a composite state draws into its top edge maps back to the
    /// document it was written in. Only the first row is drawn — a frame has one top
    /// edge — and it is clipped to that edge, so the span names the clipped text and not
    /// the whole title.
    pub title: Option<DrawnLabel>,
    /// A direction override for the group's own layout.
    pub direction: Option<Direction>,
    /// Nodes belonging directly to this group, in declaration order.
    pub nodes: Vec<NodeIdx>,
    /// Nested groups, in declaration order.
    pub children: Vec<GroupSpec>,
}

/// An edge between two nodes.
///
/// Endpoints may live in different groups at any depth; the engine lifts such an edge
/// to the closest group containing both, and aims the port at the real endpoint's
/// position inside the frame.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeSpec {
    /// The source node.
    pub from: NodeIdx,
    /// The target node.
    pub to: NodeIdx,
    /// How the line is drawn.
    pub stroke: Stroke,
    /// The terminator drawn at the [`from`](EdgeSpec::from) end.
    pub tail: Terminator,
    /// The terminator drawn at the [`to`](EdgeSpec::to) end.
    pub head: Terminator,
    /// The label carried in the middle of the edge, wrapped into rows.
    ///
    /// **Carries its provenance** — see [`GroupSpec::title`]. A flowchart edge label, a
    /// class relation label, an ER relationship label and a state transition label all
    /// arrive at their family as a [`Label`] that knows where it came from, and it
    /// travels the rest of the way here. An empty one is an edge with no label.
    pub label: DrawnLabel,
    /// A short label placed beside the [`from`](EdgeSpec::from) end, such as a class
    /// diagram cardinality (`"1"` in `"1" -- "0..*"`).
    ///
    /// **A bare string, with no provenance, deliberately.** The class parser reads a
    /// cardinality as a quoted `String` and never builds a `Label` for it, so there is no
    /// byte range to thread: giving this field a `DrawnLabel` would mean synthesising an
    /// empty `Label` whose empty `source` emits nothing anyway, and dressing "no
    /// provenance" up as provenance is worse than saying it. Should a cardinality ever
    /// want to be selectable, the change belongs in the parser — `lex::label_at` on the
    /// cardinality text — and this field becomes a `DrawnLabel` then.
    pub tail_label: Option<String>,
    /// A short label placed beside the [`to`](EdgeSpec::to) end. See
    /// [`tail_label`](EdgeSpec::tail_label) for why it carries no provenance either.
    pub head_label: Option<String>,
}

impl EdgeSpec {
    /// A plain arrow from `from` to `to`.
    pub fn arrow(from: NodeIdx, to: NodeIdx) -> Self {
        Self {
            from,
            to,
            stroke: Stroke::Solid,
            tail: Terminator::None,
            head: Terminator::Arrow,
            label: DrawnLabel::default(),
            tail_label: None,
            head_label: None,
        }
    }
}

/// What is drawn where an edge meets a node.
///
/// The set covers every family the engine serves: flowchart arrows (§6.1), class
/// relations (§6.3) and ER crow's feet (§6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Terminator {
    /// Nothing; the line simply meets the border.
    #[default]
    None,
    /// A filled arrowhead, `-->`.
    Arrow,
    /// A hollow triangle: inheritance `<|--` and realization `..|>`.
    HollowTriangle,
    /// A filled diamond: composition `*--`.
    FilledDiamond,
    /// A hollow diamond: aggregation `o--`.
    HollowDiamond,
    /// An ER crow's-foot pair, drawn outer marker first.
    CrowFoot {
        /// `{`/`}`: many rather than one.
        many: bool,
        /// `o` rather than `|`: optional rather than mandatory.
        optional: bool,
    },
}

impl Terminator {
    /// The glyphs to draw, ordered from the far end of the line towards the node,
    /// where `dir` is the direction of travel towards the node.
    pub fn glyphs(self, dir: super::glyph::Dir) -> &'static str {
        use super::glyph::Dir;
        match self {
            Self::None => "",
            Self::Arrow => match dir {
                Dir::Up => "▲",
                Dir::Down => "▼",
                Dir::Left => "◀",
                Dir::Right => "▶",
            },
            Self::HollowTriangle => match dir {
                Dir::Up => "△",
                Dir::Down => "▽",
                Dir::Left => "◁",
                Dir::Right => "▷",
            },
            Self::FilledDiamond => "◆",
            Self::HollowDiamond => "◇",
            Self::CrowFoot { many, optional } => match (many, optional, dir) {
                (false, false, _) => "┼┼",
                (false, true, _) => "○┼",
                (true, false, Dir::Up) => "┼∨",
                (true, false, Dir::Down) => "┼∧",
                (true, false, Dir::Left) => "┼>",
                (true, false, Dir::Right) => "┼<",
                (true, true, Dir::Up) => "○∨",
                (true, true, Dir::Down) => "○∧",
                (true, true, Dir::Left) => "○>",
                (true, true, Dir::Right) => "○<",
            },
        }
    }

    /// How many cells [`glyphs`](Terminator::glyphs) occupies along the line.
    pub fn len(self, dir: super::glyph::Dir) -> usize {
        self.glyphs(dir).chars().count()
    }

    /// True when nothing is drawn at this end.
    pub fn is_none(self) -> bool {
        self == Self::None
    }
}

/// Where edges may attach to a node's border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PortPolicy {
    /// Attach anywhere along the border, fanning several edges out across a side.
    ///
    /// Right for boxes with straight sides.
    #[default]
    Spread,
    /// Attach at the middle of each side only.
    ///
    /// Right for shapes whose border is only flat at its midpoint, such as a rhombus
    /// or a circle.
    Center,
}

/// Draws the content of one node.
///
/// The engine calls [`render`](NodeArt::render) to measure *and* to paint, so a node
/// can never be measured differently from how it is drawn. Implementations must be
/// deterministic: the same `(node, budget, theme)` must always produce the same
/// canvas, and the engine may call it several times while narrowing the layout to fit
/// a width budget.
///
/// Any closure `Fn(NodeIdx, u16, &Theme) -> Canvas` is a `NodeArt`.
pub trait NodeArt {
    /// Renders `node` as a self-contained box at most `budget` columns wide.
    fn render(&self, node: NodeIdx, budget: u16, theme: &Theme) -> Canvas;

    /// Where edges may attach to `node`'s border. Defaults to [`PortPolicy::Spread`].
    fn ports(&self, node: NodeIdx) -> PortPolicy {
        let _ = node;
        PortPolicy::Spread
    }
}

impl<F> NodeArt for F
where
    F: Fn(NodeIdx, u16, &Theme) -> Canvas,
{
    fn render(&self, node: NodeIdx, budget: u16, theme: &Theme) -> Canvas {
        self(node, budget, theme)
    }
}
