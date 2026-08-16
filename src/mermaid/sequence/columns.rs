// SPDX-License-Identifier: MIT
//! Horizontal layout: where every lifeline sits.
//!
//! One solver turns the whole diagram into a set of *distance constraints* between
//! neighbouring lifelines — header box widths, message labels that must fit between
//! two lifelines, self-message hooks, notes, and the margin the nested block frames
//! need — and then satisfies them all at once. Everything the painter does afterwards
//! is expressed in terms of the column numbers this module produces.
//!
//! When the constraints do not fit the width budget the solver retries with a tighter
//! *profile*: less breathing space between lifelines and a harder cap on label width.
//! Only when the tightest profile still overflows does it give up with
//! [`MermaidError::TooNarrow`].

use crate::error::MermaidError;
use crate::mermaid::ast::{Label, NotePlacement, ParticipantKind, SequenceDiagram, SequenceItem};
use crate::mermaid::chrome::{self, Piece};
use crate::text::{display_width, distribute_evenly};

/// Columns a self-message hook reaches out from its lifeline.
pub(super) const HOOK: usize = 3;
/// Columns of inset between two nested block frames.
pub(super) const FRAME_INSET: usize = 2;
/// The smallest distance any two lifelines may ever be apart.
const MIN_DISTANCE: usize = 4;

/// A layout attempt: how much room to leave and how long labels may get.
///
/// Tried in order, so the first entry is the roomy look and the last is the
/// last-ditch attempt before the diagram is declared too wide to draw.
const PROFILES: [Profile; 4] = [
    Profile {
        gap: 3,
        label_cap: 32,
        self_label: 32,
    },
    Profile {
        gap: 3,
        label_cap: 20,
        self_label: 20,
    },
    Profile {
        gap: 2,
        label_cap: 14,
        self_label: 6,
    },
    Profile {
        gap: 1,
        label_cap: 8,
        self_label: 0,
    },
];

/// One rung of the degradation ladder.
#[derive(Debug, Clone, Copy)]
struct Profile {
    /// Blank columns kept between two neighbouring header boxes.
    gap: usize,
    /// The widest a message or note label may be before it is truncated.
    label_cap: usize,
    /// How much room a self-message label is *guaranteed*. Beyond this it is
    /// truncated to whatever the finished layout happens to leave.
    self_label: usize,
}

/// The head of one lifeline: the participant box or actor figure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Header {
    /// The participant's label, whole, so a drawn row can name the bytes behind it.
    pub label: Label,
    /// The label, already wrapped to the profile's cap, piece by piece.
    pub pieces: Vec<Piece>,
    /// The total width of the drawn head. Always odd, so the lifeline is centred.
    pub width: usize,
    /// Whether to draw a box or a stick figure.
    pub kind: ParticipantKind,
}

impl Header {
    /// Columns the head reaches to the left of its lifeline.
    fn left_half(&self) -> usize {
        (self.width - 1) / 2
    }

    /// Columns the head reaches to the right of its lifeline.
    fn right_half(&self) -> usize {
        self.width - 1 - self.left_half()
    }

    /// Rows the head occupies.
    pub fn height(&self) -> usize {
        self.pieces.len() + 2
    }
}

/// The solved horizontal layout of a sequence diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Columns {
    /// One head per participant, in column order.
    pub headers: Vec<Header>,
    /// The column each lifeline is drawn in.
    pub centers: Vec<usize>,
    /// The total content width.
    pub width: usize,
    /// Rows the header block occupies; heads are bottom-aligned within it.
    pub header_height: usize,
    /// How deeply block frames nest, `0` when the diagram has no frames.
    pub frame_levels: usize,
    /// Columns of content reaching left of the leftmost lifeline.
    pub left_reach: usize,
    /// Columns of content reaching right of the rightmost lifeline.
    pub right_reach: usize,
    /// The widest a label may be under the profile that succeeded.
    pub label_cap: usize,
}

impl Columns {
    /// The left edge of the block frame drawn at nesting `depth`.
    ///
    /// Frames clear everything that reaches outside the lifelines — self-message
    /// hooks and notes attached to the outer participants — so a frame edge never
    /// cuts through diagram content.
    pub fn frame_left(&self, depth: usize) -> usize {
        let inset = FRAME_INSET * (self.frame_levels.saturating_sub(depth));
        self.centers
            .first()
            .copied()
            .unwrap_or(0)
            .saturating_sub(self.left_reach + inset)
    }

    /// The right edge of the block frame drawn at nesting `depth`.
    pub fn frame_right(&self, depth: usize) -> usize {
        let inset = FRAME_INSET * (self.frame_levels.saturating_sub(depth));
        (self.centers.last().copied().unwrap_or(0) + self.right_reach + inset)
            .min(self.width.saturating_sub(1))
    }
}

/// Solves the horizontal layout inside `budget` columns.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when even the tightest profile overflows.
pub(super) fn solve(diagram: &SequenceDiagram, budget: u16) -> Result<Columns, MermaidError> {
    let mut narrowest = None;
    for profile in PROFILES {
        let columns = attempt(diagram, profile);
        if columns.width <= usize::from(budget) {
            return Ok(columns);
        }
        let width = u16::try_from(columns.width).unwrap_or(u16::MAX);
        narrowest = Some(narrowest.map_or(width, |at: u16| at.min(width)));
    }
    Err(MermaidError::TooNarrow {
        width: budget,
        needed: narrowest,
    })
}

/// Builds a layout under one profile, without checking it against the budget.
fn attempt(diagram: &SequenceDiagram, profile: Profile) -> Columns {
    let headers = build_headers(diagram, profile);
    let count = headers.len();
    let header_height = headers.iter().map(Header::height).max().unwrap_or(0);
    let frame_levels = nesting(&diagram.items);

    // Start from the distance two neighbouring heads need not to touch.
    let mut distance: Vec<usize> = (0..count.saturating_sub(1))
        .map(|index| {
            (headers[index].right_half() + headers[index + 1].left_half() + profile.gap)
                .max(MIN_DISTANCE)
        })
        .collect();
    // Room needed *outside* the outermost lifelines for notes and self-message
    // hooks. Block frames claim their own margin there too, so the two add up.
    let mut left_reach = 0usize;
    let mut right_reach = 0usize;
    let mut spans = Vec::new();
    collect(
        &diagram.items,
        &headers,
        profile,
        &mut distance,
        &mut left_reach,
        &mut right_reach,
        &mut spans,
    );
    let frame_margin = FRAME_INSET * frame_levels;
    let left_pad = headers
        .first()
        .map_or(0, Header::left_half)
        .max(frame_margin + left_reach);
    let right_pad = headers
        .last()
        .map_or(0, Header::right_half)
        .max(frame_margin + right_reach);
    satisfy(&mut distance, &spans);

    let mut centers = Vec::with_capacity(count);
    let mut at = left_pad;
    for index in 0..count {
        if index > 0 {
            at += distance[index - 1];
        }
        centers.push(at);
    }
    let width = centers.last().map_or(left_pad, |last| last + right_pad) + 1;

    Columns {
        headers,
        centers,
        width,
        header_height,
        frame_levels,
        left_reach,
        right_reach,
        label_cap: profile.label_cap,
    }
}

/// Wraps every participant label and sizes its head.
fn build_headers(diagram: &SequenceDiagram, profile: Profile) -> Vec<Header> {
    diagram
        .participants
        .iter()
        .map(|participant| {
            let pieces = chrome::label_pieces_or_blank(&participant.label, profile.label_cap);
            let text = pieces
                .iter()
                .map(|piece| display_width(&piece.text))
                .max()
                .unwrap_or(0);
            let width = match participant.kind {
                // `│ label │`
                ParticipantKind::Participant => text + 4,
                // The stick figure is three columns wide.
                ParticipantKind::Actor => text.max(3),
            };
            Header {
                label: participant.label.clone(),
                pieces,
                // An odd width puts the lifeline exactly in the middle.
                width: width | 1,
                kind: participant.kind,
            }
        })
        .collect()
}

/// Raises the minimum distance of one gap, ignoring an out-of-range index.
fn widen(distance: &mut [usize], index: usize, least: usize) {
    if let Some(slot) = distance.get_mut(index) {
        *slot = (*slot).max(least);
    }
}

/// A "these two lifelines must be at least this far apart" constraint.
#[derive(Debug, Clone, Copy)]
struct Span {
    from: usize,
    to: usize,
    least: usize,
}

/// Walks the diagram and turns every item into distance constraints.
#[allow(clippy::too_many_arguments)]
fn collect(
    items: &[SequenceItem],
    headers: &[Header],
    profile: Profile,
    distance: &mut [usize],
    left_reach: &mut usize,
    right_reach: &mut usize,
    spans: &mut Vec<Span>,
) {
    let count = headers.len();
    for item in items {
        match item {
            SequenceItem::Message(message) => {
                let from = message.from.0.min(count.saturating_sub(1));
                let to = message.to.0.min(count.saturating_sub(1));
                let label = chrome::label_natural_width(&message.label).min(profile.label_cap);
                if from == to {
                    // A self-message hooks out to the right and hangs its label there.
                    let need = HOOK + 2 + label.min(profile.self_label);
                    if from + 1 < count {
                        widen(distance, from, need + headers[from + 1].left_half());
                    } else {
                        *right_reach = (*right_reach).max(need);
                    }
                } else {
                    spans.push(Span {
                        from: from.min(to),
                        to: from.max(to),
                        least: label + 2,
                    });
                }
            }
            SequenceItem::Note(note) => {
                let text = chrome::label_natural_width(&note.text).min(profile.label_cap);
                let box_width = text + 4;
                let mut targets: Vec<usize> = note
                    .participants
                    .iter()
                    .map(|id| id.0.min(count.saturating_sub(1)))
                    .collect();
                targets.sort_unstable();
                let (Some(&first), Some(&last)) = (targets.first(), targets.last()) else {
                    continue;
                };
                match note.placement {
                    NotePlacement::LeftOf => {
                        if first == 0 {
                            *left_reach = (*left_reach).max(box_width + 1);
                        } else {
                            widen(distance, first - 1, box_width + 3);
                        }
                    }
                    NotePlacement::RightOf => {
                        if last + 1 >= count {
                            *right_reach = (*right_reach).max(box_width + 1);
                        } else {
                            widen(distance, last, box_width + 3);
                        }
                    }
                    NotePlacement::Over if first < last => spans.push(Span {
                        from: first,
                        to: last,
                        least: box_width.saturating_sub(2),
                    }),
                    NotePlacement::Over => {
                        // A note over a single lifeline spreads evenly to both sides.
                        let reach = box_width / 2 + 1;
                        if first == 0 {
                            *left_reach = (*left_reach).max(reach);
                        } else {
                            widen(distance, first - 1, reach + 1);
                        }
                        if last + 1 >= count {
                            *right_reach = (*right_reach).max(reach);
                        } else {
                            widen(distance, last, reach + 1);
                        }
                    }
                }
            }
            SequenceItem::Block(block) => {
                // A frame is only as wide as the lifelines it spans, so its caption
                // has to be part of the horizontal constraints or it would be cut.
                for (index, branch) in block.branches.iter().enumerate() {
                    let label = branch.label.as_ref().map(|label| label.text());
                    let caption = display_width(&super::caption(
                        block.kind,
                        index,
                        label.as_deref().map(str::trim),
                    ));
                    if count > 1 {
                        spans.push(Span {
                            from: 0,
                            to: count - 1,
                            least: caption,
                        });
                    } else {
                        *right_reach = (*right_reach).max(caption + 2);
                    }
                }
                for branch in &block.branches {
                    collect(
                        &branch.items,
                        headers,
                        profile,
                        distance,
                        left_reach,
                        right_reach,
                        spans,
                    );
                }
            }
            SequenceItem::Activate(_) | SequenceItem::Deactivate(_) => {}
        }
    }
}

/// Widens gaps until every span constraint holds.
///
/// Each unsatisfied span spreads its deficit evenly over the gaps it covers, which
/// keeps the diagram visually balanced instead of pushing all the slack into one gap.
/// Constraints only ever grow distances, so a bounded number of passes converges.
fn satisfy(distance: &mut [usize], spans: &[Span]) {
    for _ in 0..spans.len().min(8) + 1 {
        let mut changed = false;
        for span in spans {
            let range = span.from..span.to;
            if range.is_empty() || range.end > distance.len() {
                continue;
            }
            let current: usize = distance[range.clone()].iter().sum();
            let Some(deficit) = span.least.checked_sub(current).filter(|d| *d > 0) else {
                continue;
            };
            distribute_evenly(&mut distance[range], deficit);
            changed = true;
        }
        if !changed {
            return;
        }
    }
}

/// How deeply block frames nest in `items`.
fn nesting(items: &[SequenceItem]) -> usize {
    items
        .iter()
        .map(|item| match item {
            SequenceItem::Block(block) => {
                1 + block
                    .branches
                    .iter()
                    .map(|branch| nesting(&branch.items))
                    .max()
                    .unwrap_or(0)
            }
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satisfy_spreads_a_deficit_evenly() {
        let mut distance = vec![4, 4, 4];
        satisfy(
            &mut distance,
            &[Span {
                from: 0,
                to: 3,
                least: 20,
            }],
        );
        assert_eq!(distance.iter().sum::<usize>(), 20);
        assert_eq!(distance, vec![7, 7, 6]);
    }

    #[test]
    fn satisfy_leaves_satisfied_spans_alone() {
        let mut distance = vec![10, 10];
        satisfy(
            &mut distance,
            &[Span {
                from: 0,
                to: 2,
                least: 5,
            }],
        );
        assert_eq!(distance, vec![10, 10]);
    }
}
