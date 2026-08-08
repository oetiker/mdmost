//! The rendered-document cache.
//!
//! Design spec §3 makes rendering a pure function of `(document, width, theme)`, so the
//! cache key is exactly that triple and nothing else. The cache is a performance
//! detail: [`RenderCache::refresh`] recomputes on any key change, and everything
//! derived from a render — table-of-contents rows, search positions — is recomputed by
//! the caller whenever `refresh` reports a new render. Dropping the cache therefore
//! changes nothing visible.

use crate::canvas::Canvas;

/// A single-entry cache holding the most recent render.
#[derive(Debug)]
pub struct RenderCache {
    key: Option<(u64, u16, String)>,
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
        render: impl FnOnce() -> Canvas,
    ) -> bool {
        let wanted = (version, width, theme.to_string());
        if self.key.as_ref() == Some(&wanted) {
            return false;
        }
        self.canvas = render();
        self.key = Some(wanted);
        true
    }

    /// The cached canvas. Empty until the first [`RenderCache::refresh`].
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    /// Forgets the cached render.
    pub fn invalidate(&mut self) {
        self.key = None;
    }
}
