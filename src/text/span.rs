// SPDX-License-Identifier: MIT
//! Styled text runs.
//!
//! [`Span`] and [`Line`] are the currency between anything that produces styled text
//! (the inline renderer, the syntax highlighter, diagram labels) and anything that
//! turns styled text into cells (the [`Canvas`](crate::canvas::Canvas)).

use crate::text::{display_width, min_unbreakable_width};
use crate::theme::Style;

/// A run of text sharing one style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    /// The text of the run. May contain any Unicode, but not control characters other
    /// than `\n`, which wrapping treats as a forced line break.
    pub text: String,
    /// The style the whole run is drawn in.
    pub style: Style,
}

impl Span {
    /// Creates a span.
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    /// Creates an unstyled span, inheriting whatever style is underneath.
    pub fn raw(text: impl Into<String>) -> Self {
        Self::new(text, Style::NONE)
    }

    /// The display width of the run.
    pub fn width(&self) -> usize {
        display_width(&self.text)
    }

    /// Returns `true` if the run contains no text.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// A sequence of styled runs forming one visual line.
///
/// A `Line` carries no width of its own; padding to a width budget happens when it is
/// written onto a [`Canvas`](crate::canvas::Canvas).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    /// The runs, in visual order.
    pub spans: Vec<Span>,
}

impl Line {
    /// Creates a line from its runs.
    pub fn new(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    /// Creates an empty line.
    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    /// Creates a line holding a single styled run.
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self::new(vec![Span::new(text, style)])
    }

    /// The total display width of the line.
    pub fn width(&self) -> usize {
        self.spans.iter().map(Span::width).sum()
    }

    /// Returns `true` if the line has no text at all.
    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(Span::is_empty)
    }

    /// The plain text of the line, with all styling dropped.
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Appends a run, merging it into the previous one when the styles match.
    pub fn push(&mut self, span: Span) {
        if span.text.is_empty() {
            return;
        }
        match self.spans.last_mut() {
            Some(last) if last.style == span.style => last.text.push_str(&span.text),
            _ => self.spans.push(span),
        }
    }

    /// Returns a copy of the line clipped to at most `width` display columns.
    ///
    /// Clipping happens on grapheme cluster boundaries, so a double-width cluster
    /// straddling the limit is dropped whole and the result may be one column
    /// narrower than `width`.
    pub fn truncated(&self, width: usize) -> Line {
        let mut out = Line::empty();
        let mut remaining = width;
        for span in &self.spans {
            if remaining == 0 {
                break;
            }
            let text = crate::text::truncate_to_width(&span.text, remaining);
            remaining -= display_width(text);
            out.push(Span::new(text, span.style));
        }
        out
    }

    /// Overlays `style` on every run of the line.
    ///
    /// See [`Style::patch`] for the overlay semantics.
    pub fn patch_style(&mut self, style: Style) {
        for span in &mut self.spans {
            span.style = span.style.patch(style);
        }
    }
}

impl FromIterator<Span> for Line {
    fn from_iter<T: IntoIterator<Item = Span>>(iter: T) -> Self {
        let mut line = Line::empty();
        for span in iter {
            line.push(span);
        }
        line
    }
}

/// The total display width of a span sequence, ignoring line breaks.
pub fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// The width of the longest unbreakable run across a span sequence.
///
/// Words are allowed to span a style boundary, so this is *not* the maximum of the
/// per-span values: `**bo**ld` is one seven-column word, not two shorter ones.
pub fn spans_min_width(spans: &[Span]) -> usize {
    let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
    joined
        .split('\n')
        .map(min_unbreakable_width)
        .max()
        .unwrap_or(0)
}
