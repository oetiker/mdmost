//! Putting text on the clipboard, and being honest about whether it landed.
//!
//! # Why OSC 52 first
//!
//! `mdless` is a terminal pager, and a terminal pager is very often read over SSH.
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

use std::io::{self, Write};

/// What became of a copy request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// A clipboard that answers accepted the text.
    Confirmed,
    /// OSC 52 was written to the terminal, which does not acknowledge.
    Sent,
    /// Nothing worked; the string says what went wrong.
    Failed(String),
}

impl Delivery {
    /// How the status bar should report this, given the byte count.
    ///
    /// The wording is the whole point of the type: see the module docs.
    pub fn message(&self, bytes: usize, from_source: bool) -> (String, bool) {
        let what = if from_source {
            "Markdown source"
        } else {
            "rendered text"
        };
        match self {
            Delivery::Confirmed => (format!("copied {bytes} bytes of {what}"), false),
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
    let osc = write_osc52(text);
    let local = local_clipboard(text);
    match (osc, local) {
        (_, Some(Ok(()))) => Delivery::Confirmed,
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
#[cfg(feature = "clipboard")]
fn local_clipboard(text: &str) -> Option<Result<(), String>> {
    if is_remote_session() {
        return None;
    }
    Some(
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(text.to_string()))
            .map_err(|error| error.to_string()),
    )
}

/// Built without the `clipboard` feature: OSC 52 is the only route.
#[cfg(not(feature = "clipboard"))]
fn local_clipboard(_text: &str) -> Option<Result<(), String>> {
    None
}

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
