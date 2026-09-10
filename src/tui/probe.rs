// SPDX-License-Identifier: MIT
//! Measuring what this terminal does with an emoji-presentation sequence.
//!
//! # Why measure at all
//!
//! `U+FE0F` asks for the emoji form of a character that also has a text form, and the
//! standard makes the result two columns wide. Several terminals draw it in one and
//! advance the cursor by one. Nothing downstream can paper over that: `unicode-width`
//! says two, `ratatui` skips the second cell on that authority, and the terminal is
//! then one column out for the rest of the run it was handed — which is what leaves
//! stale glyphs across a scrolled screen.
//!
//! There is no capability string for this, so the only honest source is the terminal
//! itself: draw the sequence, ask where the cursor ended up, and take the difference.
//!
//! # What is asked and when
//!
//! The probe writes at the start of a line and erases what it wrote, so nothing of it
//! survives on screen. It is skipped altogether where the question cannot be put or the
//! answer would mean nothing: no terminal on either standard input or standard output,
//! no `TERM`, or a `TERM` that says up front there is nothing to ask (`dumb`, the Linux
//! console). Anyone who would rather not be asked can settle it in the configuration
//! file with `narrow_emoji`, and then this never runs.
//!
//! The reply is read here rather than through `crossterm::cursor::position`, which
//! cannot be used for this: its loop treats a failed wait as nothing to report and goes
//! round again, so a terminal that has been destroyed under it spins at 100 % of a core
//! for ever — the very fault [`super::term`] exists to keep out of this program, and one
//! the pty test catches. The wait here is a `poll` with a deadline that also watches for
//! the hangup the kernel flags, and a terminal that stays silent costs a fifth of a
//! second, once, at startup.

use std::io::{self, IsTerminal, Write};
#[cfg(unix)]
use std::time::{Duration, Instant};

/// The sequence the terminal is asked to draw.
///
/// A heart rather than the wheel that started this: both are a narrow base plus the
/// selector, and this one is in every font that has any emoji at all — a probe the
/// font cannot draw would measure the terminal's tofu instead of its intent.
const PROBE: &str = "\u{2764}\u{FE0F}";

/// How long a terminal is given to answer before the question is dropped.
#[cfg(unix)]
const PATIENCE: Duration = Duration::from_millis(200);

/// How many columns this terminal gives [`PROBE`], or `None` if it would not say.
///
/// `Some(1)` is the disagreement this exists to find; `Some(2)` is the standard
/// behaviour. Anything else — an unanswerable question, a terminal that stays silent,
/// a reply that makes no sense — is `None`, and the caller is expected to carry on as
/// though the terminal were the ordinary sort. This is deliberately lopsided in the
/// same way [`crate::nerdfont`] is: a wrong "narrow" would strip selectors on a
/// terminal that wanted them, so only a clear answer changes anything.
pub fn emoji_columns() -> Option<u16> {
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        return None;
    }
    match std::env::var("TERM").ok()?.as_str() {
        "" | "dumb" | "linux" => return None,
        _ => {}
    }
    // Raw mode for the reading: the reply is not a line and nobody typed a newline
    // after it, and echoing it would print it over the screen it was measured on.
    crossterm::terminal::enable_raw_mode().ok()?;
    let answer = measure();
    let _ = crossterm::terminal::disable_raw_mode();
    // Erase the whole line rather than the columns the probe is believed to occupy:
    // what it occupies is the very thing in question. Unconditional, so a probe that
    // was drawn and then not answered still leaves nothing behind.
    let mut out = io::stdout();
    let _ = write!(out, "\r\x1b[K");
    let _ = out.flush();
    // The report counts from one, and the probe started in the first column.
    answer?.checked_sub(1)
}

/// Draws the probe from the start of the line and reads back the column it ended in.
fn measure() -> Option<u16> {
    let mut out = io::stdout();
    // From the start of the line, so the reply *is* the width of the probe, with no
    // arithmetic against a prompt that may itself contain anything.
    write!(out, "\r{PROBE}\x1b[6n").ok()?;
    out.flush().ok()?;
    reply()
}

/// The column from the terminal's report, if one arrives in time.
///
/// The reply can be split across reads, and other input can arrive with it, so bytes are
/// accumulated until they hold a report or the deadline passes. Anything else that came
/// with it is dropped — a keystroke typed into the first fraction of a second of startup,
/// which is the same cost every implementation of this question pays.
#[cfg(unix)]
fn reply() -> Option<u16> {
    use rustix::event::{PollFd, PollFlags, poll};

    let stdin = rustix::stdio::stdin();
    let deadline = Instant::now() + PATIENCE;
    let mut seen = Vec::with_capacity(32);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
        let timeout = rustix::event::Timespec {
            tv_sec: left.as_secs().try_into().ok()?,
            tv_nsec: left.subsec_nanos().into(),
        };
        match poll(&mut fds, Some(&timeout)) {
            // Nothing arrived within the deadline, or the terminal is gone: either way
            // there is no answer, and neither is an error worth reporting.
            Ok(0) => return None,
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return None,
        }
        if fds[0]
            .revents()
            .intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL)
        {
            return None;
        }
        let mut buf = [0u8; 32];
        match rustix::io::read(stdin, &mut buf) {
            Ok(0) | Err(_) => return None,
            Ok(read) => seen.extend_from_slice(&buf[..read]),
        }
        if let Some(column) = column_of(&seen) {
            return Some(column);
        }
        // A cap, so a terminal that streams something else for ever cannot grow this
        // without bound. The report is short and arrives first.
        if seen.len() > 256 {
            return None;
        }
    }
}

/// No `poll` to lean on, so the question is not put at all.
///
/// Windows, where asking would mean waiting without a deadline — the one thing this
/// module refuses to do. `narrow_emoji` in the configuration file settles it there.
#[cfg(not(unix))]
fn reply() -> Option<u16> {
    None
}

/// The column reported by `ESC [ rows ; cols R`, if `bytes` holds such a report.
///
/// Scans for the last complete report rather than assuming the reply arrived alone: a
/// terminal may answer something else first, and a keystroke may land in the same read.
pub(super) fn column_of(bytes: &[u8]) -> Option<u16> {
    let mut answer = None;
    for start in 0..bytes.len().saturating_sub(1) {
        if bytes[start] != 0x1b || bytes[start + 1] != b'[' {
            continue;
        }
        let rest = &bytes[start + 2..];
        let Some(end) = rest.iter().position(|&byte| byte == b'R') else {
            continue;
        };
        let report = &rest[..end];
        let Some((rows, columns)) = split_once(report, b';') else {
            continue;
        };
        if let (Some(_), Some(column)) = (number(rows), number(columns)) {
            answer = Some(column);
        }
    }
    answer
}

/// `bytes` split around the first `at`, or `None` if it does not contain one.
fn split_once(bytes: &[u8], at: u8) -> Option<(&[u8], &[u8])> {
    let index = bytes.iter().position(|&byte| byte == at)?;
    Some((&bytes[..index], &bytes[index + 1..]))
}

/// `bytes` as a decimal number, or `None` if it is not one that fits.
fn number(bytes: &[u8]) -> Option<u16> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}
