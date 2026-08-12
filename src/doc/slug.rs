//! Stable heading identifiers.

use std::collections::HashMap;

/// Assigns GitHub-compatible, collision-free heading ids.
#[derive(Debug, Default)]
pub struct Slugger {
    seen: HashMap<String, usize>,
}

impl Slugger {
    /// Creates an empty slugger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a unique id for `text`.
    ///
    /// The base slug follows GitHub's rules: lower-cased, punctuation removed,
    /// whitespace turned into hyphens. Repeated headings get `-1`, `-2`, … appended,
    /// so ids are stable for a given document and unique within it.
    pub fn slug(&mut self, text: &str) -> String {
        let base = base_slug(text);
        let base = if base.is_empty() {
            "section".to_string()
        } else {
            base
        };
        let count = self.seen.entry(base.clone()).or_insert(0);
        let id = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        id
    }
}

/// The GitHub-style slug of `text`, before de-duplication.
///
/// `pub(crate)` so [`crate::render::link::classify`] can fold an `#anchor` fragment
/// through the identical rule a heading's own id was built with. The two must never
/// be able to drift apart: an anchor link is only ever correct if it was folded
/// exactly the way the heading it targets was, and a second, hand-written case-fold
/// in the classifier would be exactly that drift waiting to happen (a Unicode
/// heading was the concrete case that caught the first version of this).
pub(crate) fn base_slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_follow_github_rules() {
        let mut slugger = Slugger::new();
        assert_eq!(slugger.slug("Hello, World!"), "hello-world");
        assert_eq!(slugger.slug("A_b-c 1"), "a_b-c-1");
    }

    #[test]
    fn duplicates_get_suffixes() {
        let mut slugger = Slugger::new();
        assert_eq!(slugger.slug("Setup"), "setup");
        assert_eq!(slugger.slug("Setup"), "setup-1");
        assert_eq!(slugger.slug("Setup"), "setup-2");
    }

    #[test]
    fn empty_headings_get_a_placeholder() {
        let mut slugger = Slugger::new();
        assert_eq!(slugger.slug("***"), "section");
        assert_eq!(slugger.slug(""), "section-1");
    }

    #[test]
    fn non_ascii_headings_keep_their_letters() {
        let mut slugger = Slugger::new();
        assert_eq!(slugger.slug("Grüße 日本"), "grüße-日本");
    }
}
