// SPDX-License-Identifier: MIT
//! Keeping other people's diagnostics off the alternate screen.
//!
//! While the pager runs, the screen is a canvas `mdmost` owns outright: every cell on
//! it was put there by [`draw`](super::draw), and anything else that reaches the
//! terminal corrupts the frame until the next full repaint. A dependency does not know
//! that. `arboard` 3.6.1, for one, prints a paragraph of advice straight to standard
//! error from a `Drop` impl — the user's report that started this module has it painted
//! through the middle of a sequence diagram.
//!
//! # Two layers, because one is not enough
//!
//! The obvious fix is to install a [`log`] logger, since `arboard` uses the `log`
//! crate. It is not sufficient, and the very code that motivated this shows why: it
//! picks its channel at run time, `eprintln!` when standard error is a terminal and
//! `log::warn!` when it is not. A logger alone would have missed the reported case
//! exactly; a redirection alone would have turned the *other* branch into silence,
//! because with standard error redirected to a file the `log` branch is the one taken
//! and nothing is listening. So both layers are defended, and they share one buffer:
//!
//! * **the descriptor** — file descriptor 2 is pointed at a scratch file for the
//!   lifetime of the pager, which catches anything at all that writes to standard
//!   error, whatever crate it came from and whether or not it knows about `log`;
//! * **the `log` crate** — a logger that appends records to the same buffer, so a
//!   library that reasons about `is_terminal()` the way `arboard` does is caught on
//!   whichever branch it takes.
//!
//! # What this does not cover
//!
//! * **Standard output.** It cannot be redirected: it *is* the screen — `ratatui` draws
//!   through it and OSC 52 is written to it. A library that writes to standard output
//!   while the pager is up will still corrupt the frame. Nothing in the dependency tree
//!   does today; there is no defence available if one starts.
//! * **A library that opens `/dev/tty` itself** and writes there, bypassing descriptor
//!   2 altogether.
//! * **Windows**, where the descriptor layer is not implemented and only the `log`
//!   layer applies. Every path here is `cfg(unix)`; elsewhere the buffer still collects
//!   `log` records and the redirection is simply not attempted.
//! * **A hard exit.** `_exit`, `SIGKILL` and an abort skip the `Drop` that reports the
//!   buffer. An ordinary panic does not: the hook writes to the captured descriptor and
//!   the guard, which is dropped last, prints it afterwards.
//!
//! # Nothing is discarded
//!
//! A swallowed error is worse than an ugly one. What was captured is written to the
//! real standard error once the terminal has been restored, under a header that says
//! where it came from, so a diagnostic that would have corrupted a frame arrives one
//! screen later instead of never. When no capture is running — before the pager starts
//! and after it exits — the `log` layer writes straight through to standard error
//! rather than buffering into nowhere.

use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The most captured output that is kept.
///
/// A library in a loop could otherwise fill memory with advice. What is kept is the
/// beginning, not the end: the first complaint is the one that explains the rest.
const LIMIT: usize = 64 * 1024;

/// Whether a capture is running, and so whether [`deliver`] buffers or writes through.
static CAPTURING: AtomicBool = AtomicBool::new(false);

/// Records collected by the `log` layer, waiting for the pager to finish.
static LOGGED: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Sends one diagnostic wherever it can currently do no harm.
///
/// Buffered while the alternate screen is up, written straight out otherwise: the
/// buffer exists to protect the screen, not to hide anything, so with no screen to
/// protect there is no reason to delay.
fn deliver(text: &str) {
    if !CAPTURING.load(Ordering::Relaxed) {
        let _ = write!(std::io::stderr(), "{text}");
        return;
    }
    let mut buffer = LOGGED.lock().unwrap_or_else(|error| error.into_inner());
    let room = LIMIT.saturating_sub(buffer.len());
    if room == 0 {
        return;
    }
    let bytes = text.as_bytes();
    buffer.extend_from_slice(&bytes[..bytes.len().min(room)]);
}

/// The `log` half of the defence.
struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        deliver(&format!(
            "{}: {} [{}]\n",
            record.level(),
            record.args(),
            record.target()
        ));
    }

    fn flush(&self) {}
}

/// Installs the logger once, ignoring a second attempt.
///
/// Warnings and errors only. A library's `info!` is not a diagnostic the reader of a
/// pager has any use for, and collecting it would only make the real complaint harder
/// to see. `set_logger` fails if something already installed one, which is fine: that
/// something is then responsible for the records.
fn install_logger() {
    static LOGGER: Logger = Logger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

/// Holds standard error away from the screen until it is dropped.
///
/// Create one before entering the alternate screen and let it drop *after* leaving it —
/// declare it before the restoration guard, since drops run in reverse. Dropping it
/// restores the descriptor and prints whatever was collected.
pub struct Capture {
    #[cfg(unix)]
    redirect: Option<Redirect>,
}

/// The redirected descriptor and the scratch file behind it.
#[cfg(unix)]
struct Redirect {
    /// A duplicate of the original standard error, put back on the way out.
    saved: std::os::fd::OwnedFd,
    /// The scratch file descriptor 2 was pointed at. Already unlinked.
    scratch: std::fs::File,
}

impl Capture {
    /// Starts capturing, best effort.
    ///
    /// A failure to redirect is not a reason to refuse to start the pager: the `log`
    /// layer still applies, and the worst case is the behaviour that was there before.
    pub fn start() -> Self {
        install_logger();
        CAPTURING.store(true, Ordering::Relaxed);
        Self {
            #[cfg(unix)]
            redirect: Redirect::start().ok(),
        }
    }

    /// Stops capturing and returns what was collected, oldest first.
    ///
    /// Called by [`Drop`], and directly by the tests. Safe to call twice; the second
    /// call returns nothing.
    pub fn finish(&mut self) -> Vec<u8> {
        CAPTURING.store(false, Ordering::Relaxed);
        #[cfg(unix)]
        let mut collected = match self.redirect.take() {
            Some(redirect) => redirect.finish(),
            None => Vec::new(),
        };
        #[cfg(not(unix))]
        let mut collected = Vec::new();
        let logged = std::mem::take(&mut *LOGGED.lock().unwrap_or_else(|e| e.into_inner()));
        collected.extend_from_slice(&logged);
        collected
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let collected = self.finish();
        if collected.is_empty() {
            return;
        }
        // Best effort throughout: this runs on the panic path too, and a panic raised
        // while unwinding aborts the process.
        let mut err = std::io::stderr().lock();
        let _ = writeln!(
            err,
            "mdmost: output from a library, held back while the pager had the screen:"
        );
        let _ = err.write_all(&collected);
        if !collected.ends_with(b"\n") {
            let _ = writeln!(err);
        }
        if collected.len() >= LIMIT {
            let _ = writeln!(err, "mdmost: ...and more, which was not kept");
        }
        let _ = err.flush();
    }
}

#[cfg(unix)]
impl Redirect {
    /// Points descriptor 2 at a scratch file, keeping the original to put back.
    ///
    /// A file rather than a pipe, deliberately. A pipe would need a thread to drain it
    /// or the writer would block once 64 kB had accumulated, and it would need that
    /// thread joined at exit — which is a hang waiting to happen, because
    /// `wl-clipboard-rs` forks a child that inherits descriptor 2 and outlives us on
    /// purpose. A file blocks nobody, needs no thread, and does not care who else holds
    /// a copy of the descriptor.
    ///
    /// No `unsafe` anywhere: `rustix` wraps `dup`/`dup2` safely, which is why the
    /// library can keep `#![forbid(unsafe_code)]`.
    fn start() -> std::io::Result<Self> {
        let scratch = scratch_file()?;
        let saved = rustix::io::dup(rustix::stdio::stderr())?;
        rustix::stdio::dup2_stderr(&scratch)?;
        Ok(Self { saved, scratch })
    }

    /// Restores descriptor 2 and reads back what was written meanwhile.
    fn finish(mut self) -> Vec<u8> {
        use std::io::{Read, Seek, SeekFrom};

        // First, so that nothing written from here on lands in the scratch file — and
        // so that the read below is not racing anyone's write offset.
        let _ = rustix::stdio::dup2_stderr(&self.saved);
        if self.scratch.seek(SeekFrom::Start(0)).is_err() {
            return Vec::new();
        }
        let mut collected = Vec::new();
        let _ = Read::take(&mut self.scratch, LIMIT as u64).read_to_end(&mut collected);
        collected
    }
}

/// Creates the scratch file and unlinks it at once.
///
/// Unlinked immediately so that no run of `mdmost` can leave litter in the temporary
/// directory however it exits — the descriptor keeps the file alive, and the space is
/// returned by the kernel when the process ends. Nothing but this process ever has a
/// name for it.
#[cfg(unix)]
fn scratch_file() -> std::io::Result<std::fs::File> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!("mdmost-stderr-{}-{stamp}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    let _ = std::fs::remove_file(&path);
    Ok(file)
}
