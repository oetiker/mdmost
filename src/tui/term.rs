//! Terminal lifecycle and the event loop.
//!
//! Design spec §12 treats a wrecked terminal as a release blocker, so restoration is
//! wired in three independent ways and each of them is complete on its own:
//!
//! * a [`Restore`] guard, for the ordinary return and for `?`;
//! * a panic hook, installed before `ratatui`'s so it runs after it;
//! * a `SIGTERM`/`SIGHUP`/`SIGINT` flag polled by the loop, plus `Ctrl-C`, which raw
//!   mode delivers as an ordinary key event rather than as a signal.
//!
//! A terminal can also go away without asking. `SIGHUP` covers the polite case and is
//! not enough on its own: it only interrupts the wait, and the loop used never to get
//! back out of that wait to look at the flag. So the loop does its own waiting on the
//! terminal's input descriptor and leaves when the kernel says it has hung up — see
//! [`Input`], which is also the whole of why the pager no longer spins at 100 % of a
//! core after its terminal is destroyed.
//!
//! "Restored" means all of: raw mode off, alternate screen left, mouse capture off,
//! cursor shown. Forgetting the mouse is the one that leaves a terminal spewing escape
//! codes afterwards, so it is part of the single shared [`restore`] routine rather
//! than open-coded anywhere.

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode as XKeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;

use crate::config::{Key, KeyCode, KeyMods};

use super::app::{App, Focus};
use super::{chrome, draw};

/// How long the loop waits for input before checking the termination flag.
const POLL_INTERVAL: Duration = Duration::from_millis(120);

/// What waiting for input found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    /// Something arrived, or `crossterm` may have something buffered.
    Ready,
    /// The interval passed with nothing to report.
    Timeout,
    /// The terminal is gone: the descriptor is hung up, in error, or not a
    /// descriptor at all.
    Gone,
}

/// The error a vanished terminal produces.
///
/// Deliberately not `BrokenPipe`, which the binary treats as the ordinary
/// `mdless x.md | head` case and exits 0 for: losing the terminal mid-document is
/// nobody's fault but it did not finish, so it exits non-zero and says so — best
/// effort, because by definition there may be no terminal left to say it to.
fn terminal_gone() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionAborted,
        "the terminal went away while the pager was running",
    )
}

/// The terminal's input descriptor, watched for a hangup.
///
/// The pager cannot leave the waiting to `crossterm`. When a pty master closes, the
/// slave stays readable for ever and every read of it fails with `EIO`;
/// `crossterm`'s reader treats an error that is neither `WouldBlock` nor `EINTR` as
/// nothing at all and goes round again, so `event::poll` never returns and the `?`
/// on it is never reached. That is the 100 %-of-a-core spin this type exists to
/// prevent: it does the waiting itself, on the same descriptor `crossterm` reads,
/// and reports the hangup the kernel is already flagging.
///
/// The hangup is read from `POLLHUP`/`POLLERR`/`POLLNVAL` and nothing else. That
/// matters: it is a fact about the descriptor, not an inference from timing, so no
/// amount of latency on a live terminal — a laggy link, a reader who leaves the pager
/// open for an hour — can make it fire.
///
/// Linux only. `poll` is documented not to work on `/dev/tty` on macOS, and `mdless`
/// reads keys from `/dev/tty` whenever the document came in on standard input; a
/// wrong "your terminal is gone" would throw a reader out of their document, which is
/// worse than the spin. Elsewhere the loop waits the way it always did.
struct Input {
    #[cfg(target_os = "linux")]
    tty: TtyFd,
}

/// Either borrowed standard input or an owned `/dev/tty`, whichever `crossterm` reads.
#[cfg(target_os = "linux")]
enum TtyFd {
    Stdin,
    DevTty(std::fs::File),
}

#[cfg(target_os = "linux")]
impl Input {
    /// Opens the descriptor `crossterm` will read from, by the same rule it uses:
    /// standard input when that is a terminal, `/dev/tty` otherwise — which is the
    /// `cat x.md | mdless` case, where standard input is the document.
    fn open() -> io::Result<Self> {
        use std::io::IsTerminal;

        if io::stdin().is_terminal() {
            return Ok(Self { tty: TtyFd::Stdin });
        }
        let tty = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")?;
        Ok(Self {
            tty: TtyFd::DevTty(tty),
        })
    }

    /// Waits up to `timeout` for input, or for the terminal to go away.
    fn wait(&self, timeout: Duration) -> io::Result<Wait> {
        use rustix::event::{PollFd, PollFlags, Timespec, poll};
        use std::os::fd::AsFd;

        let stdin = io::stdin();
        let borrowed = match &self.tty {
            TtyFd::Stdin => stdin.as_fd(),
            TtyFd::DevTty(file) => file.as_fd(),
        };
        let mut fds = [PollFd::from_borrowed_fd(borrowed, PollFlags::IN)];
        let deadline = Timespec {
            tv_sec: timeout.as_secs().try_into().unwrap_or(i64::MAX),
            tv_nsec: timeout.subsec_nanos().into(),
        };
        let ready = match poll(&mut fds, Some(&deadline)) {
            Ok(ready) => ready,
            // A signal cut the wait short. The caller checks the termination flag
            // every time round, so reporting "nothing happened" is both true and what
            // makes `SIGTERM` act at once rather than up to 120 ms later.
            Err(rustix::io::Errno::INTR) => return Ok(Wait::Timeout),
            Err(error) => return Err(error.into()),
        };
        if ready == 0 {
            // Nothing arrived — but `crossterm` may still hold events parsed out of
            // an earlier read, so this is not "go straight back to sleep".
            return Ok(Wait::Timeout);
        }
        let gone = PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL;
        if fds[0].revents().intersects(gone) {
            return Ok(Wait::Gone);
        }
        Ok(Wait::Ready)
    }
}

#[cfg(not(target_os = "linux"))]
impl Input {
    /// Nothing to open: `crossterm` keeps doing the waiting.
    fn open() -> io::Result<Self> {
        Ok(Self {})
    }

    /// Falls back to `crossterm`'s own wait, and so cannot report a hangup.
    fn wait(&self, timeout: Duration) -> io::Result<Wait> {
        if crossterm::event::poll(timeout)? {
            Ok(Wait::Ready)
        } else {
            Ok(Wait::Timeout)
        }
    }
}

/// Runs the pager until the user leaves, the process is asked to terminate, or the
/// terminal goes away.
///
/// # Errors
///
/// Returns any I/O failure from the terminal, including [`terminal_gone`] when the
/// terminal is hung up under the pager. The terminal is restored either way.
pub fn run(app: &mut App) -> io::Result<()> {
    install_panic_hook();
    let terminate = Arc::new(AtomicBool::new(false));
    // A failure to register is not worth refusing to start over; the guard and the
    // panic hook still cover every other exit path.
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&terminate));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&terminate));
    // `Ctrl-C` arrives as a key event under raw mode, but `kill -INT` from another
    // terminal does not; without this it would leave the alternate screen up.
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&terminate));

    let input = Input::open()?;
    let mut terminal = ratatui::try_init()?;
    // Asked for and refused is worth saying: silently having no mouse looks like the
    // configuration was ignored.
    if app.config().mouse && execute!(io::stdout(), EnableMouseCapture).is_err() {
        app.notify("this terminal refused mouse capture", true);
    }
    let _guard = Restore;

    let result = event_loop(app, &mut terminal, &input, &terminate);
    if result.is_err() {
        // `ratatui`'s `Terminal` complains into standard error from its destructor
        // when it cannot show the cursor — and when a terminal has just died, that
        // write fails too. `eprintln!` panics on a failed write, and a panic raised
        // while the panic hook is running aborts the process, so the pager would
        // leave by `SIGABRT` and possibly a core file. The `Restore` guard below has
        // already put every one of those settings back, so the destructor has nothing
        // left to do but that; skipping it costs one leaked frame buffer on a path
        // that is about to exit anyway.
        std::mem::forget(terminal);
    }
    result
}

/// The event loop proper, separated so that [`run`] can decide what to do about the
/// terminal on the way out.
fn event_loop(
    app: &mut App,
    terminal: &mut ratatui::DefaultTerminal,
    input: &Input,
    terminate: &Arc<AtomicBool>,
) -> io::Result<()> {
    // Laying out a large document takes real time, and an empty alternate screen is
    // indistinguishable from a hang (usability review B5). One cheap frame first says
    // what is being opened and that something is happening.
    terminal.draw(|frame| draw::draw_splash(frame, app))?;

    while !app.should_quit() && !terminate.load(Ordering::Relaxed) {
        terminal.draw(|frame| draw::draw(frame, app))?;
        // Waiting is ours, not `crossterm`'s, so that a terminal which has gone away
        // is seen as what it is rather than as a descriptor that is endlessly ready.
        if input.wait(POLL_INTERVAL)? == Wait::Gone {
            return Err(terminal_gone());
        }
        // The descriptor was live a moment ago, so `crossterm` may look at it. Zero
        // timeout: the waiting has already been done, and asking even when nothing
        // arrived is what hands over events its parser is still holding from an
        // earlier read.
        //
        // "A moment ago" is the premise, and it is not quite a guarantee. If the
        // terminal dies in the microseconds between the wait above and the read
        // below — or part-way through an escape sequence, where `crossterm` blocks
        // reading the rest — the old spin is reachable again. That window is
        // accepted: it needs input to arrive at the same instant as the hangup,
        // where what this replaced was exposed the whole time it sat idle. Closing
        // it properly means owning the descriptor and parsing terminal input here,
        // which is `crossterm`'s job.
        if !crossterm::event::poll(Duration::ZERO)? {
            continue;
        }
        let mut event = Some(crossterm::event::read()?);
        // A drag of the window edge arrives as a burst of resizes, and every one of
        // them re-lays-out the whole document. Only the last matters, so the rest are
        // thrown away before anything is re-rendered (usability review B5).
        while matches!(event, Some(Event::Resize(..))) && crossterm::event::poll(Duration::ZERO)? {
            event = Some(crossterm::event::read()?);
        }
        match event {
            Some(Event::Key(key)) => on_key(app, key),
            Some(Event::Mouse(mouse)) => {
                let size = terminal.size()?;
                on_mouse(app, mouse, size.width, size.height);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Dispatches a key event.
fn on_key(app: &mut App, event: KeyEvent) {
    if event.kind == KeyEventKind::Release {
        return;
    }
    // Ctrl-C always leaves, whatever the bindings say: a pager that cannot be
    // interrupted is a trap.
    if event.code == XKeyCode::Char('c') && event.modifiers.contains(KeyModifiers::CONTROL) {
        app.quit();
        return;
    }
    if let Some(key) = convert_key(event) {
        app.on_key(key);
    }
}

/// Dispatches a mouse event.
///
/// A left drag inside the document area is a text selection (see [`super::select`]).
/// The table-of-contents pane keeps its click-to-jump and is never a selection source:
/// its entries are a generated map, not document text, so there is no source behind
/// them to copy.
fn on_mouse(app: &mut App, event: MouseEvent, width: u16, height: u16) {
    let in_toc = chrome::in_toc(app, event.column);
    let body_height = height.saturating_sub(1);
    // The document area: the pane on its left, the scrollbar's gutter on its right.
    let doc_x = app.toc_width();
    let doc_width = width.saturating_sub(doc_x).saturating_sub(1);
    let in_doc = event.column >= doc_x
        && event.column < doc_x.saturating_add(doc_width)
        && event.row < body_height;
    let local = || {
        (
            event.column.saturating_sub(doc_x),
            event.row.min(body_height.saturating_sub(1)),
        )
    };
    match event.kind {
        MouseEventKind::ScrollDown => app.on_scroll(1, in_toc),
        MouseEventKind::ScrollUp => app.on_scroll(-1, in_toc),
        MouseEventKind::Down(MouseButton::Left) if in_toc => {
            if let Some(row) = chrome::toc_row_at(app, body_height, event.row) {
                let first = app.toc_first_visible(usize::from(body_height.saturating_sub(2)));
                app.on_toc_click(first, row);
            }
        }
        MouseEventKind::Down(MouseButton::Left) if app.focus() == Focus::Toc && !in_doc => {}
        MouseEventKind::Down(MouseButton::Left) if in_doc => {
            let (x, y) = local();
            app.begin_selection(x, y);
        }
        // A drag is reported even when the pointer has left the window, with the
        // coordinates clamped to it — which is exactly what makes the edge auto-scroll
        // in `App::drag_selection` fire.
        MouseEventKind::Drag(MouseButton::Left) if app.selection().is_some() => {
            let (x, y) = local();
            app.drag_selection(x, y);
        }
        MouseEventKind::Up(MouseButton::Left) if app.selection().is_some() => {
            app.end_selection();
            copy_selection(app);
        }
        _ => {}
    }
}

/// Puts a finished selection on the clipboard and says what happened.
///
/// The I/O lives here rather than in [`App`] because the state machine touches no
/// terminal (design spec §13); the app produces the text and is handed the outcome.
fn copy_selection(app: &mut App) {
    let Some(extract) = app.take_pending_copy() else {
        return;
    };
    let delivery = super::clipboard::copy(&extract.text);
    app.report_copy(extract.text.len(), extract.from_source, &delivery);
}

/// Converts a `crossterm` key event into the terminal-independent [`Key`].
///
/// `SHIFT` is dropped for character keys because the shifted character already carries
/// the information; keeping both would make `G` impossible to bind.
fn convert_key(event: KeyEvent) -> Option<Key> {
    let mut mods = KeyMods::NONE;
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        mods = mods.union(KeyMods::CTRL);
    }
    if event.modifiers.contains(KeyModifiers::ALT) {
        mods = mods.union(KeyMods::ALT);
    }
    let code = match event.code {
        XKeyCode::Char(ch) if mods.contains(KeyMods::CTRL) => {
            KeyCode::Char(ch.to_ascii_lowercase())
        }
        XKeyCode::Char(ch) => KeyCode::Char(ch),
        XKeyCode::Enter => KeyCode::Enter,
        XKeyCode::Esc => KeyCode::Esc,
        XKeyCode::Tab => KeyCode::Tab,
        XKeyCode::BackTab => KeyCode::BackTab,
        XKeyCode::Backspace => KeyCode::Backspace,
        XKeyCode::Delete => KeyCode::Delete,
        XKeyCode::Insert => KeyCode::Insert,
        XKeyCode::Left => KeyCode::Left,
        XKeyCode::Right => KeyCode::Right,
        XKeyCode::Up => KeyCode::Up,
        XKeyCode::Down => KeyCode::Down,
        XKeyCode::Home => KeyCode::Home,
        XKeyCode::End => KeyCode::End,
        XKeyCode::PageUp => KeyCode::PageUp,
        XKeyCode::PageDown => KeyCode::PageDown,
        XKeyCode::F(n) if (1..=12).contains(&n) => KeyCode::Function(n),
        _ => return None,
    };
    if !matches!(code, KeyCode::Char(_)) && event.modifiers.contains(KeyModifiers::SHIFT) {
        mods = mods.union(KeyMods::SHIFT);
    }
    Some(Key { code, mods })
}

/// Restores the terminal when it goes out of scope.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        restore();
    }
}

/// Puts the terminal back exactly as it was found.
///
/// Safe to call more than once and from a panic hook: every step ignores its own
/// failure so that a broken step cannot prevent the remaining ones.
pub fn restore() {
    let mut out = io::stdout();
    let _ = execute!(out, DisableMouseCapture);
    let _ = ratatui::try_restore();
    let _ = out.flush();
}

/// Installs a panic hook that restores the terminal before the message is printed.
///
/// Called before `ratatui::try_init`, which installs its own hook on top; the
/// terminal is therefore restored by both, and this one runs last so mouse capture is
/// off before anything is written.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}
