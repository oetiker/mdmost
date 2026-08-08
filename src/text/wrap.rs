//! Greedy, grapheme-safe, width-aware line wrapping.
//!
//! [`wrap_spans`] is the single wrapping implementation in `mdless`.
//! `render::inline::wrap` and the Mermaid label layouters delegate to it.

use crate::text::{Line, Span, display_width, graphemes};
use crate::theme::Style;

/// One grapheme cluster together with the style it is drawn in.
struct Cluster<'a> {
    text: &'a str,
    style: Style,
    width: usize,
}

/// A wrapping token: a word, a run of spaces, or an explicit line break.
enum Token<'a> {
    Word(Vec<Cluster<'a>>),
    Space(Vec<Cluster<'a>>),
    Break,
}

/// Wraps a sequence of styled runs into lines of at most `width` display columns.
///
/// * Breaking happens at whitespace; the whitespace itself is dropped at the break.
/// * A word longer than `width` is split on grapheme cluster boundaries, never inside
///   a cluster, so combining marks and ZWJ emoji sequences survive.
/// * A `\n` inside any span forces a break and can produce an empty line.
/// * Styles are preserved across breaks; adjacent runs with equal styles are merged.
///
/// Returns an empty vector if `width` is `0`, or if the input contains no text at all.
/// Otherwise every returned line is at most `width` columns wide.
pub fn wrap_spans(spans: &[Span], width: usize) -> Vec<Line> {
    if width == 0 {
        return Vec::new();
    }
    let tokens = tokenize(spans);
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut lines: Vec<Line> = Vec::new();
    let mut current = Line::empty();
    let mut current_width = 0usize;
    let mut pending: Vec<Cluster<'_>> = Vec::new();
    let mut pending_width = 0usize;
    // Whitespace that lands at a wrap point is dropped; whitespace at the very start
    // of a logical segment (document start, or just after an explicit break) is kept,
    // because it is meaningful indentation the caller put there.
    let mut segment_started = false;

    for token in tokens {
        match token {
            Token::Space(clusters) => {
                pending_width += clusters.iter().map(|c| c.width).sum::<usize>();
                pending.extend(clusters);
            }
            Token::Break => {
                // Trailing whitespace in front of a hard break carries no meaning.
                pending.clear();
                pending_width = 0;
                lines.push(std::mem::take(&mut current));
                current_width = 0;
                segment_started = false;
            }
            Token::Word(clusters) => {
                let word_width: usize = clusters.iter().map(|c| c.width).sum();
                let keep_pending = current_width > 0 || !segment_started;
                let lead = if keep_pending { pending_width } else { 0 };

                if current_width + lead + word_width <= width {
                    if keep_pending {
                        append(&mut current, pending.drain(..));
                        current_width += pending_width;
                    } else {
                        pending.clear();
                    }
                    pending_width = 0;
                    current_width += word_width;
                    append(&mut current, clusters.into_iter());
                } else {
                    // The pending whitespace sits at a break: drop it.
                    pending.clear();
                    pending_width = 0;
                    if current_width > 0 {
                        lines.push(std::mem::take(&mut current));
                        current_width = 0;
                    }
                    if word_width <= width {
                        current_width = word_width;
                        append(&mut current, clusters.into_iter());
                    } else {
                        // Hard-split an overlong word on cluster boundaries.
                        for cluster in clusters {
                            if cluster.width > width {
                                // A double-width cluster in a one-column budget can
                                // never be shown; dropping it keeps the width
                                // guarantee that callers rely on.
                                continue;
                            }
                            if current_width + cluster.width > width {
                                lines.push(std::mem::take(&mut current));
                                current_width = 0;
                            }
                            current_width += cluster.width;
                            current.push(Span::new(cluster.text, cluster.style));
                        }
                    }
                }
                segment_started = true;
            }
        }
    }

    if current_width > 0 || !current.spans.is_empty() {
        lines.push(current);
    }
    lines
}

/// Wraps plain text, returning plain lines.
///
/// A convenience wrapper over [`wrap_spans`] for callers that carry no styling, such
/// as the Mermaid node label layouter.
pub fn wrap_plain(text: &str, width: usize) -> Vec<String> {
    wrap_spans(&[Span::raw(text)], width)
        .iter()
        .map(Line::text)
        .collect()
}

/// Appends clusters to a line, merging runs of equal style.
fn append<'a>(line: &mut Line, clusters: impl Iterator<Item = Cluster<'a>>) {
    for cluster in clusters {
        line.push(Span::new(cluster.text, cluster.style));
    }
}

/// Splits styled runs into words, whitespace runs and explicit breaks.
fn tokenize<'a>(spans: &'a [Span]) -> Vec<Token<'a>> {
    let mut tokens: Vec<Token<'a>> = Vec::new();
    let mut word: Vec<Cluster<'a>> = Vec::new();
    let mut space: Vec<Cluster<'a>> = Vec::new();

    for span in spans {
        for text in graphemes(&span.text) {
            let cluster = Cluster {
                text,
                style: span.style,
                // Priced at its true width, not at a cell's capacity: a cluster wider
                // than two columns still stays whole on one line — that is what design
                // spec §4's "never split a cluster" means — but a line holding one has
                // to be charged for the columns it really draws, or the wrapped line
                // comes out wider than the budget it was given.
                width: display_width(text),
            };
            if text == "\n" {
                flush(&mut tokens, &mut word, &mut space);
                tokens.push(Token::Break);
            } else if text.chars().all(char::is_whitespace) {
                if !word.is_empty() {
                    tokens.push(Token::Word(std::mem::take(&mut word)));
                }
                space.push(cluster);
            } else {
                if !space.is_empty() {
                    tokens.push(Token::Space(std::mem::take(&mut space)));
                }
                word.push(cluster);
            }
        }
    }
    flush(&mut tokens, &mut word, &mut space);
    tokens
}

/// Emits any half-built word or whitespace run.
fn flush<'a>(
    tokens: &mut Vec<Token<'a>>,
    word: &mut Vec<Cluster<'a>>,
    space: &mut Vec<Cluster<'a>>,
) {
    if !word.is_empty() {
        tokens.push(Token::Word(std::mem::take(word)));
    }
    if !space.is_empty() {
        tokens.push(Token::Space(std::mem::take(space)));
    }
}
