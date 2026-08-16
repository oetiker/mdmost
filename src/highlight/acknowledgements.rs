// SPDX-License-Identifier: MIT
//! Attribution for the third-party syntax definitions compiled into the binary.
//!
//! [`BUNDLED_SYNTAXES`](super::BUNDLED_SYNTAXES) embeds a couple of hundred syntax
//! definitions written by other people. Most are under Sublime's permissive notice or the
//! Unlicense, which ask for nothing; a substantial minority are MIT, BSD-2, BSD-3 or
//! Apache-2.0, and every one of those requires the copyright notice and permission text
//! to be reproduced **in binary distributions** — which is exactly what `mdmost` is. A
//! `LICENSE` file in the source tree does not discharge that: the binary is what people
//! are given, so the notices have to travel inside it.
//!
//! Hence [`syntax_acknowledgements`], surfaced as `mdmost --licenses`. `two-face` keeps
//! the notice set in step with the definitions it ships, so this cannot drift out of date
//! the way a hand-maintained `THIRD-PARTY` file would; the cost is about 11 KiB of
//! embedded text, and only in a binary that keeps the function.
//!
//! The output is deliberately plain Markdown, with no HTML in it. `two-face`'s own
//! `to_md` wraps each licence in a `<details>` element, and `mdmost` does not render HTML
//! (design spec §1) — so `mdmost --licenses | mdmost -` would have swallowed the very
//! text this exists to display.

use two_face::acknowledgement::License;

/// Every third-party notice that has to travel with the binary, as Markdown.
///
/// Licences that ask for no acknowledgement (Sublime's own, the Unlicense, WTFPL) are
/// left out: including them would bury the ones that matter. The full listing, including
/// those, is linked at the end.
///
/// ```
/// let text = mdmost::highlight::syntax_acknowledgements();
/// assert!(text.contains("Permission is hereby granted"));
/// assert!(!text.contains("<details>"));
/// ```
pub fn syntax_acknowledgements() -> String {
    let listing = two_face::acknowledgement::listing();
    let mut out = String::with_capacity(16 * 1024);
    out.push_str(
        "# Third-party syntax definitions\n\
         \n\
         `mdmost` highlights fenced code with syntax definitions curated by the\n\
         [`bat` project](https://github.com/sharkdp/bat) and packaged by\n\
         [`two-face`](https://codeberg.org/CosmicHarper/two-face). They are compiled into\n\
         this binary, and the licences below are reproduced as those licences require.\n\
         \n\
         `mdmost` itself is MIT-licensed. The TOML and Dockerfile definitions in\n\
         `assets/syntaxes/` are part of `mdmost` and covered by that licence.\n\n",
    );

    let mut notices: Vec<&License> = listing
        .for_syntaxes()
        .iter()
        .filter(|licence| licence.needs_acknowledgement())
        .collect();
    notices.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    for licence in notices {
        out.push_str("## ");
        out.push_str(&licence.rel_path.display().to_string());
        out.push_str("\n\n```text\n");
        out.push_str(licence.text.trim_end());
        out.push_str("\n```\n\n");
    }

    out.push_str("## The complete listing\n\nEvery licence of every bundled definition, ");
    out.push_str("including the ones that ask for no acknowledgement:\n\n");
    out.push_str(two_face::acknowledgement::url());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The notices must actually be there — an empty listing would look like compliance
    /// and be none.
    #[test]
    fn the_listing_carries_real_licence_text() {
        let text = syntax_acknowledgements();
        assert!(
            text.len() > 4_000,
            "acknowledgement listing is only {} bytes; that is not a set of licences",
            text.len()
        );
        // The permission grant common to MIT and the BSD family.
        assert!(text.contains("Permission is hereby granted"));
        assert!(text.contains("Copyright"));
    }

    /// No HTML, because `mdmost` renders none and this is meant to be read in `mdmost`.
    #[test]
    fn the_listing_is_plain_markdown() {
        let text = syntax_acknowledgements();
        for tag in ["<details>", "<summary>", "</details>"] {
            assert!(
                !text.contains(tag),
                "acknowledgement listing contains {tag}"
            );
        }
    }

    /// Nothing that asks for no acknowledgement should be padding the list out.
    #[test]
    fn only_licences_that_require_acknowledgement_are_listed() {
        let listing = two_face::acknowledgement::listing();
        let required = listing
            .for_syntaxes()
            .iter()
            .filter(|licence| licence.needs_acknowledgement())
            .count();
        assert!(required > 0, "no syntax licence requires acknowledgement?");
        // Counted outside the fences: several of the licence texts are themselves
        // Markdown-ish and contain lines starting with `##`.
        let mut in_fence = false;
        let mut sections = 0;
        for line in syntax_acknowledgements().lines() {
            if line.starts_with("```") {
                in_fence = !in_fence;
            } else if !in_fence && line.starts_with("## ") {
                sections += 1;
            }
        }
        assert_eq!(
            sections,
            required + 1,
            "one section per required licence, plus the link to the full listing"
        );
    }

    /// The link has to name the version whose definitions are actually embedded.
    #[test]
    fn the_full_listing_link_is_version_pinned() {
        let text = syntax_acknowledgements();
        let url = two_face::acknowledgement::url();
        assert!(text.contains(url));
        assert!(url.contains("/tag/v"), "not pinned to a release: {url}");
    }
}
