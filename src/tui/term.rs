//! Terminal lifecycle and the event loop.
//!
//! Design spec §12 treats a wrecked terminal as a release blocker, so restoration is
//! wired in three independent ways and each of them is complete on its own:
//!
//! * a [`Restore`] guard, for the ordinary return and for `?`;
//! * a panic hook, installed before `ratatui`'s so it runs after it;
//! * a `SIGTERM` flag polled by the loop, plus `Ctrl-C`, which raw mode delivers as an
//!   ordinary key event.
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

/// Runs the pager until the user leaves, the process is asked to terminate, or the
/// terminal goes away.
///
/// # Errors
///
/// Returns any I/O failure from the terminal. The terminal is restored either way.
pub fn run(app: &mut App) -> io::Result<()> {
    install_panic_hook();
    let terminate = Arc::new(AtomicBool::new(false));
    // A failure to register is not worth refusing to start over; the guard and the
    // panic hook still cover every other exit path.
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&terminate));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&terminate));

    let mut terminal = ratatui::try_init()?;
    let mouse = app.config().mouse;
    if mouse {
        let _ = execute!(io::stdout(), EnableMouseCapture);
    }
    let _guard = Restore;

    while !app.should_quit() && !terminate.load(Ordering::Relaxed) {
        terminal.draw(|frame| draw::draw(frame, app))?;
        if !crossterm::event::poll(POLL_INTERVAL)? {
            continue;
        }
        match crossterm::event::read()? {
            Event::Key(key) => on_key(app, key),
            Event::Mouse(mouse) => on_mouse(app, mouse, terminal.size()?.height),
            Event::Resize(..) => {}
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
fn on_mouse(app: &mut App, event: MouseEvent, height: u16) {
    let in_toc = chrome::in_toc(app, event.column);
    match event.kind {
        MouseEventKind::ScrollDown => app.on_scroll(1, in_toc),
        MouseEventKind::ScrollUp => app.on_scroll(-1, in_toc),
        MouseEventKind::Down(MouseButton::Left) if in_toc => {
            let body_height = height.saturating_sub(1);
            if let Some(row) = chrome::toc_row_at(app, body_height, event.row) {
                let first = app.toc_first_visible(usize::from(body_height.saturating_sub(2)));
                app.on_toc_click(first, row);
            }
        }
        MouseEventKind::Down(MouseButton::Left) if app.focus() == Focus::Toc => {}
        _ => {}
    }
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
