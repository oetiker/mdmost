//! Vertical layout: which rows every element of the body occupies.
//!
//! The planner walks the diagram once, keeping a row cursor and a stack of open
//! activations per participant. Everything it emits is expressed in *body rows*,
//! counted from the first row under the participant heads, so the painter can place
//! the header block and the repeated footer block wherever it likes.
//!
//! Nesting is handled by recursion: a block frame records the row of its top edge,
//! recurses into each branch, and records the row of its bottom edge afterwards, so a
//! frame always encloses exactly the rows its contents used.

use crate::mermaid::ast::{
    BlockKind, Label, MessageHead, MessageLine, NotePlacement, SequenceDiagram, SequenceItem,
};
use crate::mermaid::chrome::{self, Piece};
use crate::text::ellipsize;

use super::columns::Columns;

/// One message arrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Arrow {
    /// The body row the shaft is drawn on. A label sits on the row above; a
    /// self-message uses this row and the two below it.
    pub row: usize,
    /// Index of the sending participant.
    pub from: usize,
    /// Index of the receiving participant.
    pub to: usize,
    /// How the shaft is drawn.
    pub line: MessageLine,
    /// The terminator at the receiving end.
    pub head: MessageHead,
    /// The message text, already shortened to the layout's label cap.
    pub label: String,
    /// The message's label, whole, so the drawn text can name its source bytes.
    pub source: Label,
}

/// One branch of a block frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Branch {
    /// The body row the branch caption is written on. For the first branch this is
    /// the frame's own top edge.
    pub row: usize,
    /// The branch label, when the source gave one.
    pub label: Option<String>,
}

/// One `loop` / `alt` / `opt` / `par` / `critical` frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    /// Body row of the top edge.
    pub top: usize,
    /// Body row of the bottom edge.
    pub bottom: usize,
    /// Nesting depth, `0` for an outermost frame.
    pub depth: usize,
    /// Which keyword opened the frame.
    pub kind: BlockKind,
    /// The frame's branches, in source order.
    pub branches: Vec<Branch>,
}

/// One note box, already placed horizontally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NoteBox {
    /// Body row of the box's top edge.
    pub top: usize,
    /// Leftmost column of the box.
    pub left: usize,
    /// Total width of the box, borders included.
    pub width: usize,
    /// The wrapped note text, piece by piece.
    pub pieces: Vec<Piece>,
    /// The note's label, whole, so a drawn row can name the bytes behind it.
    pub source: Label,
}

/// One activation bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Bar {
    /// Index of the activated participant.
    pub participant: usize,
    /// How many activations of the same participant are already open, which is how
    /// far right of the lifeline this bar is drawn.
    pub depth: usize,
    /// First body row of the bar.
    pub top: usize,
    /// Last body row of the bar, inclusive.
    pub bottom: usize,
}

/// Everything the painter needs, in body-row coordinates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Plan {
    /// How many body rows the diagram uses.
    pub rows: usize,
    /// Block frames, outermost first.
    pub frames: Vec<Frame>,
    /// Message arrows in source order.
    pub arrows: Vec<Arrow>,
    /// Note boxes in source order.
    pub notes: Vec<NoteBox>,
    /// Activation bars, in the order they were opened.
    pub bars: Vec<Bar>,
}

/// Plans the vertical layout of a diagram body.
pub(super) fn build(diagram: &SequenceDiagram, columns: &Columns) -> Plan {
    let mut builder = Builder {
        columns,
        row: 0,
        open: vec![Vec::new(); columns.centers.len()],
        plan: Plan::default(),
    };
    builder.walk(&diagram.items, 0);
    builder.finish()
}

/// The planner's mutable state.
struct Builder<'a> {
    columns: &'a Columns,
    row: usize,
    /// Per participant, the body rows at which still-open activations started.
    open: Vec<Vec<usize>>,
    plan: Plan,
}

impl Builder<'_> {
    /// Clamps a participant id onto an existing column.
    fn participant(&self, index: usize) -> usize {
        index.min(self.columns.centers.len().saturating_sub(1))
    }

    /// Opens an activation bar for `participant` at `row`.
    fn activate(&mut self, participant: usize, row: usize) {
        self.open[participant].push(row);
    }

    /// Closes the innermost activation of `participant`, ignoring a stray close.
    fn deactivate(&mut self, participant: usize, row: usize) {
        if let Some(top) = self.open[participant].pop() {
            self.plan.bars.push(Bar {
                participant,
                depth: self.open[participant].len(),
                top,
                bottom: row.max(top),
            });
        }
    }

    /// Walks a run of items at nesting `depth`, advancing the row cursor.
    fn walk(&mut self, items: &[SequenceItem], depth: usize) {
        for item in items {
            match item {
                SequenceItem::Message(message) => {
                    let from = self.participant(message.from.0);
                    let to = self.participant(message.to.0);
                    let label = ellipsize(
                        &chrome::label_one_line(&message.label),
                        self.columns.label_cap,
                    );
                    let row = if from == to {
                        let row = self.row;
                        self.row += 3;
                        row
                    } else if label.is_empty() {
                        let row = self.row;
                        self.row += 1;
                        row
                    } else {
                        let row = self.row + 1;
                        self.row += 2;
                        row
                    };
                    self.plan.arrows.push(Arrow {
                        row,
                        from,
                        to,
                        line: message.line,
                        head: message.head,
                        label,
                        source: message.label.clone(),
                    });
                    if message.activates {
                        self.activate(to, row);
                    }
                    if message.deactivates {
                        self.deactivate(to, row);
                    }
                }
                SequenceItem::Note(note) => {
                    if let Some(placed) = self.place_note(note) {
                        self.row += placed.pieces.len() + 2;
                        self.plan.notes.push(placed);
                    }
                }
                SequenceItem::Activate(id) => {
                    let participant = self.participant(id.0);
                    self.activate(participant, self.row);
                }
                SequenceItem::Deactivate(id) => {
                    let participant = self.participant(id.0);
                    let row = self.row.saturating_sub(1);
                    self.deactivate(participant, row);
                }
                SequenceItem::Block(block) => self.walk_block(block.kind, &block.branches, depth),
            }
        }
    }

    /// Walks a block frame, recording its edges around its contents.
    fn walk_block(
        &mut self,
        kind: BlockKind,
        branches: &[crate::mermaid::ast::Branch],
        depth: usize,
    ) {
        let top = self.row;
        self.row += 1;
        let mut marks = Vec::with_capacity(branches.len());
        for (index, branch) in branches.iter().enumerate() {
            let row = if index == 0 {
                top
            } else {
                let row = self.row;
                self.row += 1;
                row
            };
            marks.push(Branch {
                row,
                label: branch
                    .label
                    .as_ref()
                    .map(|label| ellipsize(&chrome::label_one_line(label), self.columns.label_cap)),
            });
            self.walk(&branch.items, depth + 1);
        }
        let bottom = self.row;
        self.row += 1;
        self.plan.frames.push(Frame {
            top,
            bottom,
            depth,
            kind,
            branches: marks,
        });
    }

    /// Works out where a note box sits horizontally, or `None` if it has no target.
    fn place_note(&self, note: &crate::mermaid::ast::Note) -> Option<NoteBox> {
        let mut pieces = chrome::label_pieces(&note.text, self.columns.label_cap);
        if pieces.is_empty() {
            pieces.push(Piece {
                text: String::new(),
                index: 0,
                at: None,
            });
        }
        let mut targets: Vec<usize> = note
            .participants
            .iter()
            .map(|id| self.participant(id.0))
            .collect();
        // `Note over B,A` is legal and means the same as `Note over A,B`.
        targets.sort_unstable();
        let first = *targets.first()?;
        let last = *targets.last()?;

        let natural = pieces
            .iter()
            .map(|piece| crate::text::display_width(&piece.text))
            .max()
            .unwrap_or(0)
            + 4;
        let total = self.columns.width;
        let (width, left) = match note.placement {
            NotePlacement::Over => {
                // The box reaches one column past the outermost lifeline it covers,
                // and grows evenly to both sides when the text needs more than that.
                let from = self.columns.centers[first];
                let to = self.columns.centers[last];
                let covered = to - from + 3;
                let width = natural.max(covered).min(total);
                let left = from
                    .saturating_sub(1 + width.saturating_sub(covered) / 2)
                    .min(total.saturating_sub(width));
                (width, left)
            }
            NotePlacement::LeftOf => {
                let width = natural.min(total);
                let left = self.columns.centers[first]
                    .saturating_sub(1)
                    .saturating_sub(width);
                (width, left)
            }
            NotePlacement::RightOf => {
                let width = natural.min(total);
                let left = (self.columns.centers[last] + 2).min(total.saturating_sub(width));
                (width, left)
            }
        };
        Some(NoteBox {
            top: self.row,
            left,
            width,
            pieces,
            source: note.text.clone(),
        })
    }

    /// Closes any activation left open and returns the finished plan.
    fn finish(mut self) -> Plan {
        let last = self.row.saturating_sub(1);
        for participant in 0..self.open.len() {
            while !self.open[participant].is_empty() {
                self.deactivate(participant, last);
            }
        }
        self.plan.rows = self.row;
        // Outermost frames must be painted first so inner ones draw over them.
        self.plan.frames.sort_by_key(|frame| frame.depth);
        self.plan
    }
}
