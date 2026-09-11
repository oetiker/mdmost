// SPDX-License-Identifier: MIT
//! Global macros: which display blocks export their definitions, and to whom.
//!
//! Design spec §16, under the owner's ruling of 2026-09-10: **a macro defined in a display
//! block that draws nothing is visible to every formula after it**, display and inline,
//! table cells included. A block that draws something keeps its macros to itself.
//!
//! `pulldown-latex`'s `MacroContext` is private and is rebuilt per `Parser`, so nothing
//! can be carried between formulas through it. Nothing needs to be: the kept blocks are
//! prepended to each later formula's source as a preamble, one owned `String` per formula
//! and no upstream change. The ruling is what makes that safe. The preamble is
//! concatenated into the later formula's *source*, so a mixed block —
//! `\newcommand{\R}{\mathbb{R}} \quad x = 1` — would draw its `x = 1` a second time inside
//! every formula after it. Only a block that draws nothing has nothing to leak.
//!
//! Two sides share the work. [`crate::doc::Doc`] collects the *candidates* — every display
//! block whose literal mentions `\newcommand` or `\def`, a textual pre-filter — because
//! `doc` knows nothing of `math` and must not learn. This module applies the rule, once per
//! document render, by drawing each candidate at its natural width and keeping the ones
//! that come back empty (spec §16.3's "draws nothing", stated over the result). The kept
//! list reaches every block through [`super::Ctx`], the way section numbers do.

use crate::doc::Doc;
use crate::theme::Theme;

use super::bridge;

/// A macro definition every later formula sees: a candidate block that drew nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Definition {
    /// Where the block starts in the document source; a formula sees it only from there on.
    start: usize,
    /// The block's LaTeX, exactly what is prepended.
    source: String,
}

/// Decides, once per document, which candidate blocks export their macros.
///
/// Each candidate is drawn at its natural width, under the preamble the candidates before
/// it have already earned — so a definition-only block that leans on an earlier macro is
/// judged with that macro defined, as it will be wherever it is used — and is kept when
/// it comes back `Ok` and empty. Document order is the list's order, which is what gives
/// "visible after the defining block, not before" for free in [`preamble`].
///
/// This is the one place a candidate is rendered for the decision, so the cost is one
/// natural render per candidate per document render, never per formula.
pub(crate) fn definitions(doc: &Doc, theme: &Theme) -> Vec<Definition> {
    let mut kept: Vec<Definition> = Vec::new();
    for candidate in doc.macro_candidates() {
        let so_far = preamble(&kept, candidate.start);
        let drawn = bridge::math_natural(&so_far, &candidate.literal, u16::MAX, theme);
        if drawn.is_ok_and(|canvas| canvas.is_empty()) {
            kept.push(Definition {
                start: candidate.start,
                source: candidate.literal.clone(),
            });
        }
    }
    kept
}

/// The preamble for a formula starting at byte `before`: every kept definition strictly
/// before it, in document order, joined with newlines.
///
/// Strictly before, so the defining block does not see itself and a definition that
/// appears after its use is not found. That is the rule design spec §16.2 states rather
/// than a limitation of one pass: it is the rule a reader can predict without knowing how
/// the document is walked. Empty — and free — for a document with no definitions.
pub(crate) fn preamble(definitions: &[Definition], before: usize) -> String {
    definitions
        .iter()
        .take_while(|definition| definition.start < before)
        .map(|definition| definition.source.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
