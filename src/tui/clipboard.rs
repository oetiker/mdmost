//! Putting text on the clipboard, and being honest about whether it landed.
//!
//! # Why OSC 52 first
//!
//! `mdmost` is a terminal pager, and a terminal pager is very often read over SSH.
//! OSC 52 is a message *to the terminal emulator*, so it crosses the connection with
//! the rest of the session and lands on the clipboard of the machine the reader is
//! sitting at. [`arboard`] talks to a local display server (X11 via `x11rb`, Wayland
//! via `wl-clipboard-rs`, `AppKit`, `Win32`) and — verified against the 3.6.1 sources
//! vendored here — has **no OSC 52 path** at all. Over SSH it therefore does not fail
//! cleanly: it copies into the *remote* machine's clipboard and returns `Ok`. That is
//! the asymmetry that fixes the order. It is also why the `arboard` attempt is skipped
//! outright inside an SSH session rather than merely tried second.
//!
//! # Why the message is worded the way it is
//!
//! OSC 52 is fire-and-forget. There is no reply, so a successful *write* proves the
//! bytes reached the terminal's input, not that the terminal honoured them — `xterm`
//! ships with `allowWindowOps` off, and `tmux` drops the sequence unless
//! `set-clipboard` is `on`. This project's standing rule is that the status bar never
//! lies, so the two cases get two different words: [`Delivery::Confirmed`] is
//! "copied", [`Delivery::Sent`] is "sent to the terminal" and says it is unconfirmed.
//! Probing the terminal's capability was the alternative — OSC 52 *can* be queried with
//! `?` — but the reply arrives as input the pager would have to parse out of the
//! reader's keystrokes, at startup, on every terminal, to earn one adjective. Wording
//! the claim correctly is cheaper and cannot be wrong.
//!
//! When a local display server is present and the session is not remote, both paths run:
//! `arboard` then upgrades the report from "sent" to "copied" for free.
//!
//! # Why the clipboard is kept alive
//!
//! An X11 clipboard is not a place you put bytes. The copying process *owns* the
//! selection and is expected to answer `SelectionRequest` events for as long as it
//! holds it; the server stores nothing. Wayland's data-control protocol works the same
//! way. So a program that sets the clipboard and immediately drops it has, in effect,
//! copied nothing — the reader pastes and gets whatever was there before. That is
//! precisely what `mdmost` used to do, and `arboard` 3.6.1 says so out loud from its
//! `Drop` impl ("Clipboard was dropped very quickly after writing"), which is how the
//! bug was reported: as advice painted over a sequence diagram.
//!
//! The fix is the first thing that advice suggests: the [`arboard::Clipboard`] is kept
//! in [`LOCAL`] for the life of the process. `arboard`'s X11 backend runs a thread that
//! serves selection requests as long as any handle is alive, so keeping one costs one
//! idle thread and one X connection, and the pager never blocks — the alternative
//! `SetExtLinux::wait`, which serves the selection on the calling thread until someone
//! takes it, cannot run on the thread that has to keep drawing.
//!
//! # What that can and cannot promise
//!
//! *While `mdmost` runs*, a copy is readable by any other process, immediately and for
//! as long as the reader takes to get round to pasting. Verified from a separate
//! process on X11.
//!
//! *After `mdmost` exits*, the local clipboard survives only if something else took
//! ownership in the meantime — a clipboard manager, which most desktops run and which
//! grabs a new selection at once. [`release`] gives it the best chance available by
//! dropping the handle deliberately on the way out, which is where `arboard` asks the
//! manager to take the data over; without that the process would simply die holding it.
//! On a desktop with no manager at all, quitting still loses the copy, and no amount of
//! effort inside `mdmost` can change that: X11 ownership dies with the process. This is
//! the one case the status bar is careful about — see [`Delivery::LocalOnly`].
//!
//! OSC 52 has none of this trouble, which is worth remembering before spending more on
//! the local path: it hands the bytes to the terminal emulator, which owns them
//! afterwards and outlives the pager. It is written first, it is written on every
//! platform, and when it lands the local clipboard is a redundant second copy.

use std::io::{self, Write};

/// What became of a copy request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// A clipboard that answers accepted the text, and the terminal was told as well.
    Confirmed,
    /// Only the local display server took it: OSC 52 was not written.
    ///
    /// Worth its own variant because it is the one case where `mdmost` is the sole
    /// owner of the copy, and an X11 or Wayland selection is owned by a *process*: when
    /// this one exits, a desktop with no clipboard manager has nothing left. The
    /// terminal emulator, which does outlive the pager, was not given a copy here.
    LocalOnly,
    /// OSC 52 was written to the terminal, which does not acknowledge.
    Sent,
    /// Nothing worked; the string says what went wrong.
    Failed(String),
}

/// What was copied, for the status bar to name.
///
/// A type rather than a `bool` because there are now four answers and because the
/// wording is the whole point: telling a reader they copied Markdown when they copied
/// box art is the kind of lie this project keeps finding in its own doc comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    /// A selection that mapped back to the document source.
    Source,
    /// A selection that did not, so the drawn cells were taken instead.
    Rendered,
    /// A whole code block, from its button.
    Code,
    /// A whole table, from its button.
    Table,
}

impl Copied {
    /// The noun the status bar uses.
    fn what(self) -> &'static str {
        match self {
            Copied::Source => "Markdown source",
            Copied::Rendered => "rendered text",
            Copied::Code => "code",
            Copied::Table => "table",
        }
    }
}

impl Delivery {
    /// How the status bar should report this, given the byte count.
    ///
    /// The wording is the whole point of the type: see the module docs.
    pub fn message(&self, bytes: usize, copied: Copied) -> (String, bool) {
        let what = copied.what();
        match self {
            Delivery::Confirmed => (format!("copied {bytes} bytes of {what}"), false),
            Delivery::LocalOnly => (
                format!(
                    "copied {bytes} bytes of {what} to the desktop clipboard (held while mdmost runs)"
                ),
                false,
            ),
            Delivery::Sent => (
                format!("sent {bytes} bytes of {what} to the terminal clipboard (unconfirmed)"),
                false,
            ),
            Delivery::Failed(why) => (format!("could not copy: {why}"), true),
        }
    }
}

/// The largest payload OSC 52 is offered.
///
/// Terminals impose their own limits on the sequence — `xterm`'s default is 1 000 000
/// bytes of *sequence*, and several others are far lower — and an over-long sequence is
/// not truncated, it is discarded whole. Refusing here means the reader is told, rather
/// than being told "sent" about bytes that were never going to arrive.
const OSC52_LIMIT: usize = 96 * 1024;

/// Puts `text` on the clipboard by the best route available.
pub fn copy(text: &str) -> Delivery {
    copy_rich(text, None)
}

/// Copies `text`, offering `html` as a richer flavour where a local clipboard exists.
///
/// The asymmetry is not an oversight. OSC 52 is one escape sequence carrying one
/// plain-text payload — it has no MIME flavours — and it is the route that survives SSH,
/// which is why it is written first and unconditionally. The HTML is therefore an upgrade
/// for a reader at a local display server, and **nobody ever receives less than `text`**.
pub fn copy_rich(text: &str, html: Option<&str>) -> Delivery {
    classify(write_osc52(text), local_clipboard(text, html))
}

/// What to claim, given what each route did.
///
/// Split out from [`copy`] because [`copy`] has two side effects and this has none:
/// the decision table is the part worth pinning down in a test, and a test of it must
/// not write an escape sequence to whatever terminal the suite is running on.
fn classify(osc: Result<(), String>, local: Option<Result<(), String>>) -> Delivery {
    match (osc, local) {
        (Ok(()), Some(Ok(()))) => Delivery::Confirmed,
        // The bytes never left this process by any route that outlives it.
        (Err(_), Some(Ok(()))) => Delivery::LocalOnly,
        (Ok(()), _) => Delivery::Sent,
        (Err(osc), Some(Err(local))) => Delivery::Failed(format!("{osc}; {local}")),
        (Err(osc), None) => Delivery::Failed(osc),
    }
}

/// Writes the OSC 52 clipboard sequence to the terminal.
///
/// `\x1b]52;c;<base64>\x07`: selection `c` is the system clipboard. Written to standard
/// output, which is the terminal even when the document arrived on standard input, and
/// flushed, because the pager's next act is to wait for input.
fn write_osc52(text: &str) -> Result<(), String> {
    let payload = base64(text.as_bytes());
    if payload.len() > OSC52_LIMIT {
        return Err(format!(
            "{} bytes is more than the terminal will take",
            text.len()
        ));
    }
    let mut out = io::stdout().lock();
    out.write_all(b"\x1b]52;c;")
        .and_then(|()| out.write_all(payload.as_bytes()))
        .and_then(|()| out.write_all(b"\x07"))
        .and_then(|()| out.flush())
        .map_err(|error| error.to_string())
}

/// The local display server's clipboard, when using it would not be a mistake.
///
/// `None` means "not attempted", which is different from a failure and is reported
/// differently: inside an SSH session there is no local clipboard worth writing to, and
/// writing to the remote one would put the text somewhere the reader will never look.
///
/// `html` is the richer flavour, offered with `text` as its plain-text alternate so that
/// an application which cannot read HTML still gets the payload every route carries.
#[cfg(feature = "clipboard")]
fn local_clipboard(text: &str, html: Option<&str>) -> Option<Result<(), String>> {
    if is_remote_session() {
        return None;
    }
    let mut held = LOCAL.lock().unwrap_or_else(|error| error.into_inner());
    if held.is_none() {
        match arboard::Clipboard::new() {
            Ok(clipboard) => *held = Some(clipboard),
            Err(error) => return Some(Err(error.to_string())),
        }
    }
    let clipboard = held.as_mut().expect("just filled in");
    let result = match html {
        // `arboard` takes the alternate alongside the HTML, so both flavours land in one
        // ownership of the selection; setting them in two calls would leave whichever
        // went second as the only one on the clipboard.
        Some(html) => clipboard
            .set()
            .html(html.to_string(), Some(text.to_string())),
        None => clipboard.set_text(text.to_string()),
    }
    .map_err(|error| error.to_string());
    if result.is_ok() {
        *COPIED_AT.lock().unwrap_or_else(|error| error.into_inner()) =
            Some(std::time::Instant::now());
    }
    if result.is_err() {
        // A connection that has failed once will keep failing; letting it go means the
        // next copy opens a fresh one rather than inheriting a dead display server.
        *held = None;
    }
    Some(result)
}

/// The clipboard handle whose life is what makes a copy readable.
///
/// See the module docs: this is not a cache, it is the copy. Dropping it un-copies the
/// text, which is the whole of the bug this replaced. A [`Mutex`] rather than a thread
/// local because [`release`] runs from wherever the pager finishes.
#[cfg(feature = "clipboard")]
static LOCAL: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);

/// Hands the local clipboard over on the way out, if anyone will take it.
///
/// Called once the pager has stopped. Dropping the handle is what makes `arboard` ask
/// the desktop's clipboard manager to save the contents, so a copy made just before `q`
/// has its one chance here; letting the process die still holding the handle would skip
/// that request entirely. Measured at about 100 ms on a desktop with no manager to
/// answer, which is the worst case and is spent after the last frame.
///
/// It is *only* a chance. With no clipboard manager running, X11 ownership dies with
/// this process whatever we do.
#[cfg(feature = "clipboard")]
pub fn release() {
    let mut held = LOCAL.lock().unwrap_or_else(|error| error.into_inner());
    if held.is_none() {
        return;
    }
    // Hold the selection for a moment first, if the copy is very recent. A clipboard
    // manager grabs a new selection promptly but not instantly, and a reader who
    // pressed `q` in the same breath as releasing the mouse would otherwise hand it
    // over before it had looked. `arboard` picks the same 100 ms as the point below
    // which a hand-over is not worth counting on; this waits past it.
    let copied_at = *COPIED_AT.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(elapsed) = copied_at.map(|at| at.elapsed())
        && let Some(remaining) = HANDOVER_GRACE.checked_sub(elapsed)
    {
        std::thread::sleep(remaining);
    }
    // Dropping is what asks the manager to take the contents over; dying while still
    // holding the handle would skip the request altogether.
    drop(held.take());
}

/// How long a copy is held before the pager will let go of it.
///
/// Wall-clock, and deliberately not a measurement of anything: it is a floor under how
/// long the selection exists, not a budget. Nothing waits on it but the exit path, and
/// only when the reader copied within the last fraction of a second.
#[cfg(feature = "clipboard")]
const HANDOVER_GRACE: std::time::Duration = std::time::Duration::from_millis(150);

/// When the local clipboard was last written, for [`release`]'s grace period.
#[cfg(feature = "clipboard")]
static COPIED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Built without the `clipboard` feature: OSC 52 is the only route.
#[cfg(not(feature = "clipboard"))]
fn local_clipboard(_text: &str, _html: Option<&str>) -> Option<Result<(), String>> {
    None
}

/// Built without the `clipboard` feature: nothing is held, so nothing is handed over.
#[cfg(not(feature = "clipboard"))]
pub fn release() {}

/// Whether the pager is being read over SSH.
#[cfg(feature = "clipboard")]
fn is_remote_session() -> bool {
    ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// The standard base64 alphabet, padded.
///
/// Hand-rolled on purpose: OSC 52 is one escape sequence and this is twenty lines, so a
/// dependency for it would be all cost. Verified against the RFC 4648 test vectors in
/// [`super::tests`].
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            u32::from(chunk[0]),
            u32::from(chunk.get(1).copied().unwrap_or(0)),
            u32::from(chunk.get(2).copied().unwrap_or(0)),
        ];
        let packed = (b[0] << 16) | (b[1] << 8) | b[2];
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(char::from(ALPHABET[((packed >> shift) & 0x3f) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
pub(super) fn encode_for_test(bytes: &[u8]) -> String {
    base64(bytes)
}

/// The local half of a copy, without the OSC 52 half.
///
/// The tests must not call [`copy`]: it writes an OSC 52 sequence to standard output,
/// which during `cargo test` is either escape soup in the log or — worse, on a terminal
/// that honours it — the suite quietly putting its fixtures on the reader's clipboard.
#[cfg(test)]
pub(super) fn local_for_test(text: &str) -> Option<Result<(), String>> {
    local_clipboard(text, None)
}

/// The local half of a rich copy, likewise without the OSC 52 half.
#[cfg(test)]
pub(super) fn local_rich_for_test(text: &str, html: &str) -> Option<Result<(), String>> {
    local_clipboard(text, Some(html))
}

/// The decision table of [`copy`], without its two side effects.
#[cfg(test)]
pub(super) fn classify_for_test(
    osc: Result<(), String>,
    local: Option<Result<(), String>>,
) -> Delivery {
    classify(osc, local)
}

/// What another application would paste, plain-text flavour, from the held clipboard.
///
/// Only a test wants this: the pager writes clipboards and never reads them.
#[cfg(all(test, feature = "clipboard"))]
pub(super) fn paste_for_test() -> Result<String, String> {
    let mut held = LOCAL.lock().unwrap_or_else(|error| error.into_inner());
    held.as_mut()
        .ok_or_else(|| "nothing held".to_string())?
        .get_text()
        .map_err(|error| error.to_string())
}

/// Whether a local clipboard is currently held open, which is what "copied" rests on.
#[cfg(test)]
pub(super) fn held_for_test() -> bool {
    #[cfg(feature = "clipboard")]
    {
        LOCAL
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
    }
    #[cfg(not(feature = "clipboard"))]
    false
}
