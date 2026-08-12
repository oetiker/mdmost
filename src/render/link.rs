//! Which link targets become controls, and which stay inert.
//!
//! An allowlist, and deliberately a short one. Only `http` and `https` get a
//! hotspot: this is a security decision as much as a scope one, because a document
//! the reader did not write must not be able to choose which desktop handler the
//! pager launches (design spec §8). Everything unrecognised is inert, so a scheme
//! nobody has thought of yet fails closed.

use crate::canvas::HotspotKind;

/// The control a link target earns, or `None` for an inert link.
pub(crate) fn classify(url: &str) -> Option<HotspotKind> {
    let target = url.trim();
    if let Some(slug) = target.strip_prefix('#') {
        return (!slug.is_empty()).then(|| HotspotKind::Anchor {
            slug: slug.to_ascii_lowercase(),
        });
    }
    let scheme = target.split_once("://").map(|(scheme, _)| scheme)?;
    matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https").then(|| HotspotKind::Open {
        url: target.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_and_https_are_the_only_schemes_that_open() {
        assert_eq!(
            classify("https://example.com"),
            Some(HotspotKind::Open {
                url: "https://example.com".to_string()
            })
        );
        assert_eq!(
            classify("http://example.com"),
            Some(HotspotKind::Open {
                url: "http://example.com".to_string()
            })
        );
    }

    #[test]
    fn every_other_scheme_is_inert() {
        // A hotspot here would let a document the reader did not write choose which
        // desktop handler the pager launches (design spec §8).
        for url in [
            "mailto:a@b.c",
            "ftp://example.com/x",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "vscode://file/etc/passwd",
        ] {
            assert_eq!(classify(url), None, "{url} must record no hotspot");
        }
    }

    #[test]
    fn a_scheme_is_matched_case_insensitively() {
        // `HTTPS://` is a legal URL and a reader would expect it to work; more to
        // the point, a case-sensitive check is an allowlist with a hole in it.
        assert!(matches!(
            classify("HTTPS://example.com"),
            Some(HotspotKind::Open { .. })
        ));
    }

    #[test]
    fn a_fragment_is_an_anchor() {
        assert_eq!(
            classify("#some-heading"),
            Some(HotspotKind::Anchor {
                slug: "some-heading".to_string()
            })
        );
    }

    #[test]
    fn a_local_markdown_link_is_wholly_inert() {
        // Not "lights up and declines" -- a control that appears live and refuses is
        // worse than one never offered (design spec §1.1). Until the navigation
        // spec lands this records nothing at all.
        assert_eq!(classify("./other.md"), None);
        assert_eq!(classify("other.md"), None);
        assert_eq!(classify("/abs/other.md"), None);
    }
}
