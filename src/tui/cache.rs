//! The rendered-document cache.
//!
//! Design spec §3 makes rendering a pure function of `(document, width, theme,
//! options)`, so the cache key is exactly that tuple and nothing else. Leaving any
//! input out would serve a stale canvas the moment it changed — toggling icons is the
//! obvious way to notice.
//!
//! The cache is a performance detail: [`RenderCache::refresh`] recomputes on any key
//! change, and everything derived from a render — table-of-contents rows, search
//! positions — is recomputed by the caller whenever `refresh` reports a new render.
//! Dropping the cache therefore changes nothing visible.

use crate::canvas::Canvas;
use crate::render::RenderOptions;

/// Everything a render depends on.
///
/// Kept as one named type so that adding a render input is a compile error here rather
/// than a silently stale canvas at run time.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheKey {
    /// The document version.
    version: u64,
    /// The width the document was rendered at.
    width: u16,
    /// The name of the theme it was rendered with.
    theme: String,
    /// The capability flags it was rendered under.
    options: RenderOptions,
}

/// A single-entry cache holding the most recent render.
#[derive(Debug)]
pub struct RenderCache {
    key: Option<CacheKey>,
    canvas: Canvas,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self {
            key: None,
            canvas: Canvas::empty(0),
        }
    }
}

impl RenderCache {
    /// Renders if the key changed, and reports whether it did.
    ///
    /// A `true` return means the canvas is new and every position derived from the
    /// previous one is stale.
    pub fn refresh(
        &mut self,
        version: u64,
        width: u16,
        theme: &str,
        options: RenderOptions,
        render: impl FnOnce() -> Canvas,
    ) -> bool {
        // Compared field by field rather than by building a `CacheKey`: `refresh` is
        // called several times per frame and the theme name would otherwise be cloned
        // on every hit, which is every call but the rare one that actually renders.
        let hit = self.key.as_ref().is_some_and(|key| {
            key.version == version
                && key.width == width
                && key.options == options
                && key.theme == theme
        });
        if hit {
            return false;
        }
        self.canvas = render();
        self.key = Some(CacheKey {
            version,
            width,
            theme: theme.to_string(),
            options,
        });
        true
    }

    /// The cached canvas. Empty until the first [`RenderCache::refresh`].
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }
}
