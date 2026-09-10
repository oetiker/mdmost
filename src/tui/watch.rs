// SPDX-License-Identifier: MIT
//! Noticing that the file behind the document changed.
//!
//! The event loop already wakes every [`POLL_INTERVAL`](super::term) to look at the
//! termination flag, so watching costs one `stat` per tick and no dependency at all.
//! An inotify-style watcher would buy earlier notice of a file a reader is *reading*,
//! which is not a deadline anybody can feel, at the price of a crate that has to be
//! ported per platform.
//!
//! What the tick reports is deliberately one tick behind: a change is remembered when
//! it is first seen and only acted on when the next look finds the same file, so a
//! document is never parsed half-written. See [`Watcher::changed`].

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// What is compared to decide whether a file changed.
///
/// Modification time and length, which is what a `stat` gives cheaply. An edit that
/// preserves the length *and* lands inside the same modification-time tick is missed;
/// on the nanosecond timestamps Linux, macOS and Windows all keep, that is a write
/// racing itself rather than a case anyone reaches by editing a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl Stamp {
    /// The file's current stamp, or `None` if it cannot be looked at right now.
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            modified: meta.modified().ok(),
            len: meta.len(),
        })
    }
}

/// A file, and what it looked like the last time the pager agreed with it.
#[derive(Debug)]
pub(super) struct Watcher {
    path: PathBuf,
    /// The stamp of the document currently on screen.
    seen: Option<Stamp>,
    /// A stamp seen once and not yet confirmed by a second look.
    pending: Option<Stamp>,
    /// How long a file that is being written has to be still before it is re-read.
    settle: Duration,
    /// When [`Watcher::pending`] was last set to something new.
    moved: Option<Instant>,
    /// Whether the pending change arrived out of a quiet spell rather than mid-burst.
    after_quiet: bool,
}

impl Watcher {
    /// Starts watching `path` as it stands now.
    pub(super) fn new(path: &Path, settle: Duration) -> Self {
        Self {
            path: path.to_path_buf(),
            seen: Stamp::of(path),
            pending: None,
            settle,
            moved: None,
            after_quiet: true,
        }
    }

    /// The file being watched.
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the file has changed and settled since the last time this said so.
    ///
    /// One `stat`. A stamp that differs from the document on screen is remembered and
    /// reported only when the *next* call finds it unchanged, which is what keeps a
    /// file that is still being written from being read: a write in progress moves the
    /// stamp again and the wait starts over.
    ///
    /// A path that cannot be looked at — the window between the temporary file and the
    /// rename that editors save through — is not a change and is not an error. The
    /// document stays as it is and the next tick looks again.
    pub(super) fn changed(&mut self) -> bool {
        self.changed_at(Instant::now())
    }

    /// [`Watcher::changed`], against a clock the caller supplies.
    ///
    /// The time is passed in rather than read here so that a test can cross the settle
    /// window in a microsecond and get the same answer on every run.
    ///
    /// Settling is two questions, not one. The first is whether the file is *momentarily*
    /// still — the same stamp two looks running — which is what keeps a half-written save
    /// from being parsed. The second is [`Watcher::settle`]: a change that arrives out of
    /// a quiet spell is taken up as soon as it is momentarily still, and one that arrives
    /// while the file is already being written is ridden out until the writing stops for
    /// the whole window. A file written without pause is therefore never taken up after
    /// the first change, which is the point: every re-read would be thrown away by the
    /// next write, at the cost of a re-render and a status-bar flash apiece.
    pub(super) fn changed_at(&mut self, at: Instant) -> bool {
        let Some(now) = Stamp::of(&self.path) else {
            self.pending = None;
            return false;
        };
        if Some(now) == self.seen {
            self.pending = None;
            return false;
        }
        if self.pending != Some(now) {
            // The stamp moved. Whether this is the start of a burst or a lone save is
            // decided here, while the gap back to the previous move is still known.
            self.after_quiet = self
                .moved
                .is_none_or(|moved| at.saturating_duration_since(moved) >= self.settle);
            self.pending = Some(now);
            self.moved = Some(at);
            return false;
        }
        // Momentarily still. Take it up if it came out of the quiet, or if the writing
        // has now stopped for the whole window.
        let still_for = self
            .moved
            .map_or(self.settle, |moved| at.saturating_duration_since(moved));
        if !self.after_quiet && still_for < self.settle {
            return false;
        }
        self.seen = Some(now);
        self.pending = None;
        true
    }
}
