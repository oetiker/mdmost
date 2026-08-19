// SPDX-License-Identifier: MIT
//! The `\(…\)` and `\[…\]` delimiters, found by reading the source back.
//!
//! Design spec §3.1. These are `MathJax`'s own defaults, and they are how mathematics
//! arrives in a Markdown file somebody pasted an assistant's answer into. No Markdown
//! renderer of note accepts them — which is a weaker argument for a renderer than it is
//! for a pager, because this one renders whatever is on disk.
//!
//! # Why the source and not the text
//!
//! `CommonMark` treats `\(` as a backslash escape of `(`, so by the time comrak has
//! produced a text node the backslashes are gone and its content reads `(\pi r^2)`. The
//! delimiters are unrecoverable from the node. They are still in the document, so this
//! reads `source[node.source]` instead.
//!
//! # Why nothing is rewritten
//!
//! Every byte offset in this application — a [`SourceSpan`](super::SourceSpan), a search
//! hit, the clipboard — indexes one unmodified string. A pre-pass that rewrote `\(` to
//! `$` would shift every offset after it by one byte and break all three at once. So the
//! spans this produces are **subdivisions of the span it was given**, and no offset ever
//! moves.
//!
//! # Why it runs before `split_transcriptions` — and why that stopped being required
//!
//! `split_transcriptions` cuts a node of its own around every backslash escape, and
//! `\(` is one; a scan confined to *one text node's own source* run after it would find
//! nothing, because by then no node's source holds the two bytes it is looking for. That
//! was true of this pass's first version, and it is why the design required running
//! first, on the whole text node, reusing [`super::convert::align`] — the very function
//! `split_transcriptions` uses — to take comrak's resolved text along for the ride.
//!
//! The run-grouping this module settled on (below) re-derives everything from `align`
//! over the raw source every time, regardless of how finely a prior pass has already
//! cut the run apart — so it no longer actually depends on going first. Moving the call
//! to *after* `split_transcriptions` and rerunning every test in this module produced
//! byte-identical trees. It still runs first: that was the original intent, moving it
//! would buy nothing, and leaving a comment claiming an ordering constraint the code no
//! longer needs is exactly the kind of unverified claim this plan has shipped before.
//!
//! # Why a *run* of siblings, not one text node
//!
//! `\(…\)` stays merged in one comrak `Text` node — confirmed by hand-tracing `align`
//! against it — but `\[…\]` does not: `[` and `]` also open comrak's link-bracket
//! matching, and even a failed match leaves the paragraph as **separate** `Text` AST
//! nodes whose `sourcepos` already excludes the backslash, at parse time, before either
//! this pass or `split_transcriptions` runs. `\[ E = mc^2 \]` alone in a paragraph
//! arrives as two siblings, `"[ E = mc^2 "` at `1..12` and `"]"` at `13..14` — byte `0`,
//! the opening backslash, is not in *either* span. A scan confined to one node's own
//! span can never see it.
//!
//! So this groups every maximal run of consecutive `Text` siblings and, on the left,
//! extends to the previous non-text sibling's end where there is one. Where there is
//! not — the run is the parent's first children — it does **not** fall back to the
//! parent's own span. A `Heading`'s span includes its `#`/`##` marker; a GFM
//! `TableCell`'s inline parsing turns out to drop the escaping backslash the same way
//! `\[…\]` does, even for `\(…\)` (confirmed against the real parser: `\(x\)` alone in a
//! cell arrives as two siblings, `"(x"` and `")"`, same shape as the bracket case).
//! Falling back to the parent's full span would have fixed the bracket case but broken
//! both of these silently — `align` desyncs at byte zero against the marker or the
//! surrounding cell syntax and fails closed over the *entire* run, so `math_backslash`
//! would find nothing in any heading or any table cell, with no panic and no other
//! symptom than the flag quietly doing nothing.
//!
//! The one thing ever actually missing is the *escaping backslash itself* — always
//! exactly one byte, always immediately before the run's own natural edge, never after
//! it (`CommonMark`'s escape is one backslash plus one punctuation character, and the
//! punctuation is what the affected node's own `sourcepos` keeps). So a run with no
//! preceding sibling recovers that one byte, and only that one byte, by checking whether
//! it actually is a backslash — [`recover_dropped_backslash`] — rather than reaching for
//! whatever boundary happens to be nearest. A `\(` run of one untouched node is
//! unaffected either way; a `\[` run, or a `\(` run inside a table cell, now sees the
//! byte comrak split away from it, and nothing else does.
//!
//! # No escape mechanism
//!
//! There is no way to write a literal `\(` in the source once `math_backslash` is on —
//! spec §3.1 defines none, unlike `$…$`, which leans on comrak's own spacing heuristics.
//! Every `\(…\)` and `\[…\]` pair found in the raw bytes is read as a formula. That
//! includes a pair produced by a genuine escaped backslash (`\\(` draws a literal `\`
//! followed by `(`, and the second backslash's raw byte still forms the two-byte pattern
//! `\(` this scan looks for) — a known false positive this pass does not special-case.

use super::convert::align;
use super::{Node, NodeKind, SourceSpan};

/// Replaces `\(…\)` and `\[…\]` inside every run of text-node siblings under `node`.
pub(super) fn split_backslash_math(node: &mut Node, source: &str) {
    for child in &mut node.children {
        split_backslash_math(child, source);
    }
    if node.children.is_empty() {
        return;
    }
    // Only ever a *floor*: how far left `recover_dropped_backslash` is allowed to look,
    // never a boundary it is expected to reach. See the module doc's "why it does not
    // fall back to the parent's span" section.
    let floor = node.source.start;
    let mut rebuilt: Vec<Node> = Vec::with_capacity(node.children.len());
    let mut children = std::mem::take(&mut node.children).into_iter().peekable();
    // `None` until a non-text sibling has been seen; a run with no preceding sibling
    // falls back to its own first child's start, not the parent's.
    let mut prev_sibling_end: Option<usize> = None;
    while let Some(child) = children.next() {
        if matches!(child.kind, NodeKind::Text(_)) {
            let natural_left = child.source.start;
            let mut run = vec![child];
            while matches!(children.peek(), Some(c) if matches!(c.kind, NodeKind::Text(_))) {
                let Some(next) = children.next() else { break };
                run.push(next);
            }
            let natural_right = run.last().map_or(natural_left, |n| n.source.end);
            let left = prev_sibling_end
                .unwrap_or_else(|| recover_dropped_backslash(natural_left, floor, source));
            let right = children.peek().map_or(natural_right, |c| c.source.start);
            rebuilt.extend(split_run(run, left, right, source));
        } else {
            prev_sibling_end = Some(child.source.end);
            rebuilt.push(child);
        }
    }
    node.children = rebuilt;
}

/// Extends `left` back by the one byte comrak's own escape handling can drop from a
/// run's leading edge — never past `floor`, and never by more than that one byte.
///
/// The escaping backslash of `\(`, `\)`, `\[` or `\]` is always exactly one byte,
/// always immediately before the character it escapes — never after it, so only a
/// run's *left* edge is ever affected, and a run with a preceding sibling never needs
/// this at all (the sibling's own end already sits past any backslash comrak kept
/// there, or there was nothing to drop in the first place). `floor` keeps this from
/// ever reading a byte that belongs to the parent's own markup — a `Heading`'s `#`
/// marker, say — rather than to a dropped escape.
fn recover_dropped_backslash(left: usize, floor: usize, source: &str) -> usize {
    if left > floor && source.as_bytes().get(left - 1) == Some(&b'\\') {
        left - 1
    } else {
        left
    }
}

/// One run of consecutive text-node siblings, as the sequence of text and math nodes
/// the source between `left` and `right` holds.
///
/// `left` and `right` are the run's true outer edges: a neighbouring sibling's own
/// edge where there is one, or the run's own natural boundary recovered by exactly one
/// byte where there is not (see [`recover_dropped_backslash`]) — never the enclosing
/// parent's full span, which can hold markup (a heading's `#`, a table cell's syntax)
/// this pass must not read as prose.
fn split_run(run: Vec<Node>, left: usize, right: usize, source: &str) -> Vec<Node> {
    let span = SourceSpan::new(left, right);
    let Some(raw) = source.get(span.start..span.end) else {
        return run;
    };
    let text: String = run
        .iter()
        .map(|n| match &n.kind {
            NodeKind::Text(t) => t.as_str(),
            _ => "",
        })
        .collect();
    // `align` returns `None` when it cannot re-synchronise src and text at all — an
    // entity that expands to more than one character, for instance (`&fjlig;` is `fj`;
    // see `convert::align`'s doc comment and `a_text_node_that_cannot_be_aligned_keeps_
    // its_whole_source` in tests.rs for the established case). `split_transcriptions`'s
    // answer to that is to leave the node exactly as comrak reported it — no provenance,
    // rather than a split at a guessed position. This pass follows the same rule: a run
    // `align` cannot walk is left exactly as it arrived, math delimiters inside it
    // included. Scanning `raw` anyway and treating the run as one prose lump would drop
    // every byte of it from `prose()`'s strict span-containment check the moment a
    // formula was found — failing closed here costs one document a formula it would
    // technically be entitled to; that alternative costs an unrelated document its text.
    let Some(runs) = align(raw, &text, span.start) else {
        return run;
    };

    let mut out: Vec<Node> = Vec::new();
    let mut cursor = 0usize;
    let mut plain_start = span.start;
    while let Some((display, body_start)) = find_opener(raw, cursor) {
        let closer = if display { r"\]" } else { r"\)" };
        let Some(rel_end) = raw[body_start..].find(closer) else {
            break;
        };
        let body_end = body_start + rel_end;
        let open_at = span.start + body_start - 2;
        let close_at = span.start + body_end + 2;
        out.extend(prose(&runs, plain_start, open_at));
        out.push(Node::new(
            NodeKind::Math {
                literal: raw[body_start..body_end].trim().to_string(),
                display,
            },
            SourceSpan::new(open_at, close_at),
        ));
        cursor = body_end + 2;
        plain_start = close_at;
    }
    if out.is_empty() {
        return run;
    }
    out.extend(prose(&runs, plain_start, span.end));
    out
}

/// The runs lying wholly inside `start..end`, as text nodes.
///
/// No run ever straddles a boundary: a formula opens at `\(` or `\[`, and both are
/// escapes as far as [`align`] is concerned, so it has already cut there. The `filter`
/// says so rather than assuming it.
fn prose(runs: &[(String, SourceSpan)], start: usize, end: usize) -> Vec<Node> {
    runs.iter()
        .filter(|(_, span)| span.start >= start && span.end <= end)
        .map(|(text, span)| Node::new(NodeKind::Text(text.clone()), *span))
        .collect()
}

/// The next `\(` or `\[` at or after `from`, as `(is_display, offset just past it)`.
fn find_opener(raw: &str, from: usize) -> Option<(bool, usize)> {
    let inline = raw[from..].find(r"\(").map(|at| (false, from + at + 2));
    let display = raw[from..].find(r"\[").map(|at| (true, from + at + 2));
    match (inline, display) {
        (Some(i), Some(d)) => Some(if i.1 <= d.1 { i } else { d }),
        (some, None) | (None, some) => some,
    }
}
