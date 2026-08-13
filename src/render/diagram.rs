//! Finding the width a diagram wants, for a caller that can give it one.
//!
//! Every other renderer in this module answers "draw this into `width` columns". This
//! one answers a different question — *what width does this want?* — and it exists for
//! one caller: the pager, which can lay a block out wider than the viewport and let the
//! reader scroll to it (design spec §7.3, §8). A Mermaid fence that will not fit is
//! otherwise shown as a dump of its own source, and the pager then side-scrolls the
//! reader through raw Mermaid.
//!
//! Two things keep this honest:
//!
//! * **Only [`MermaidError::TooNarrow`] earns a wider canvas.** A syntax error, an
//!   unsupported family and an internal error all keep dumping source at viewport
//!   width, because for those the source *is* the content and a wider canvas would only
//!   scroll the reader through the same broken text.
//! * **The canvas comes back with the width.** Returning only the width would make the
//!   caller render every fitting diagram a second time — measured at +43 % startup on a
//!   diagram-heavy document, for a feature that is supposed to cost nothing when nothing
//!   is too wide.
//!
//! The policy — how wide is too wide, how many layouts may be spent looking — is the
//! caller's, passed in as [`Limits`]. This module owns the question, not the answer to
//! it: `render` must not depend on `tui`, where those constants live.

use crate::canvas::Canvas;
use crate::doc::{Node, NodeKind};
use crate::error::MermaidError;
use crate::mermaid::Fit;
use crate::theme::Theme;

use super::{Ctx, RenderOptions, bridge, code};

/// What a caller is willing to spend on making a diagram fit.
///
/// Both bounds are needed and they bound different failures. `width` bounds the
/// *reader*: a diagram 116 columns past the right edge is 116 arrow presses to cross,
/// which is not reading, and — because [`Canvas::append`](crate::canvas::Canvas::append)
/// pads every row of the document to the widest part — one enormous diagram inflates the
/// whole document canvas. `probes` bounds the *search*: `pie` reports no floor at all
/// and `gantt` reports one that does not depend on width, so without it the search
/// degenerates into a linear scan for exactly the renderers whose answer will not
/// improve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    /// The widest canvas the caller will accept. Past it, the source dump wins.
    width: u16,
    /// The most layouts the search may spend, including the first.
    probes: u8,
}

impl Limits {
    /// Limits of at most `width` columns and at most `probes` layouts.
    pub(crate) const fn new(width: u16, probes: u8) -> Self {
        Self { width, probes }
    }
}

/// The diagram this block draws, and the width it needed, at the narrowest width of at
/// least `from` that works.
///
/// The returned canvas is a document block exactly that many columns wide, assembled
/// the same way [`render_block`](super::render_block) assembles one, so the caller can
/// use it directly — including in the common case where the answer is `from` itself and
/// nothing needs to scroll.
///
/// It is not always the canvas `render_block` would have produced at that width, because
/// diagrams are laid out here under [`Fit::ROOMY`]: identical whenever a rung both
/// policies share is what fits, more generous when only the budget bisection fits, and
/// widened — or refused — where the compact policy would have minced the labels. A
/// caller who can be wide has no business squeezing.
///
/// `None` when the node is not a Mermaid fence, when the fence fails for any reason
/// other than being too narrow, and when no width within `limits` draws it. In every
/// one of those cases the caller should render the block normally, which produces the
/// captioned source dump.
pub(crate) fn diagram(
    node: &Node,
    from: u16,
    limits: Limits,
    theme: &Theme,
    options: &RenderOptions,
) -> Option<(u16, Canvas)> {
    let NodeKind::CodeBlock {
        language,
        literal,
        lines,
        ..
    } = &node.kind
    else {
        return None;
    };
    if !code::is_mermaid(language.as_deref()) {
        return None;
    }
    let ctx = Ctx::new(theme, options);
    let mut at = from;
    for _ in 0..limits.probes {
        if at > limits.width {
            return None;
        }
        match bridge::mermaid(literal, at, theme, Fit::ROOMY) {
            Ok(canvas) => {
                return Some((
                    at,
                    code::diagram_block(canvas, at, literal, lines, node.source, ctx),
                ));
            }
            // `needed` is the exact width this diagram starts drawing at, so the usual
            // search is one further layout. It is treated as a hint rather than a
            // promise all the same: a renderer that reports a floor it does not honour,
            // or none at all, falls back to doubling, and the probe cap catches both.
            Err(MermaidError::TooNarrow { needed, .. }) => {
                at = match needed {
                    Some(needed) if needed > at => needed,
                    _ => at.saturating_mul(2),
                };
            }
            Err(_) => return None,
        }
    }
    None
}
