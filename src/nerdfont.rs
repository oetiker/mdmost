//! Deciding whether this terminal can draw Nerd Font glyphs.
//!
//! # Why this is a guess
//!
//! There is no escape sequence that asks a terminal which font it is using, let alone
//! which code points that font covers. Nerd Font glyphs live in the Unicode private-use
//! areas, so a terminal without a patched font draws them as tofu boxes — and it draws
//! them at whatever width it likes, which is worse than ugly, because the status bar is
//! laid out by measuring glyphs that are all supposed to be one column wide.
//!
//! So this module guesses, and the guess is deliberately lopsided: **it answers "yes"
//! only on positive evidence and falls back to plain Unicode whenever it cannot tell.**
//! Plain Unicode looks slightly less pretty on a machine that could have had glyphs;
//! Nerd Font glyphs on a machine that cannot draw them look broken. The costs are not
//! symmetric, so the tie does not go to the prettier answer.
//!
//! Anyone who knows better can say so, and their answer is taken without argument:
//! `--icons` / `--no-icons`, `MDLESS_ICONS=1` / `MDLESS_ICONS=0`, or `icons` in the
//! config file. Detection only decides when nobody has.
//!
//! # What counts as evidence
//!
//! One thing: does an installed font cover *every* private-use code point mdless can
//! draw? That question is put to fontconfig, and the code points are enumerated from the
//! glyph tables themselves rather than written out again here, so adding a glyph
//! automatically makes the probe stricter instead of quietly escaping it.
//!
//! Three things veto it regardless:
//!
//! * a terminal that cannot be expected to draw glyphs at all (`dumb`, the Linux
//!   console);
//! * output that is not going to a terminal, where the question is not merely unanswered
//!   but meaningless — there is no terminal whose font could be probed;
//! * an SSH session, where the fonts installed on *this* machine say nothing about the
//!   terminal drawing the pixels, which is somewhere else entirely.
//!
//! The SSH case is the one where a wrong guess is most annoying and the reason
//! `MDLESS_ICONS=1` exists: it is the natural thing to export in the shell profile on a
//! server you always reach from the same well-equipped terminal.

use std::process::{Command, Stdio};

use crate::render::glyphs::Glyphs;
use crate::tui::icons::{Icons, is_private_use};

/// What could be learned about this terminal's ability to draw Nerd Font glyphs.
///
/// Gathering the evidence and judging it are kept apart so the judgement can be tested
/// without a terminal, a font or a subprocess.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Signals {
    /// The value of `TERM`, if it is set.
    pub term: Option<String>,
    /// Whether output is going to a terminal at all.
    pub stdout_is_terminal: bool,
    /// Whether this looks like an SSH session.
    pub remote: bool,
    /// Whether an installed font covers every glyph mdless draws — `None` when the
    /// question could not be put, which is not the same as `Some(false)` but leads to
    /// the same answer.
    pub fonts_cover_our_glyphs: Option<bool>,
}

impl Signals {
    /// Gathers the evidence from the environment. This is the only impure part.
    pub fn probe() -> Self {
        let stdout_is_terminal = std::io::IsTerminal::is_terminal(&std::io::stdout());
        let remote = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        let term = std::env::var("TERM").ok();

        // Only ask fontconfig when the answer can still change the outcome. The query
        // spawns a process, and startup latency is a feature of a pager.
        let fonts_cover_our_glyphs = if stdout_is_terminal && !remote && !term_is_hopeless(&term) {
            fonts_cover_our_glyphs()
        } else {
            None
        };

        Self {
            term,
            stdout_is_terminal,
            remote,
            fonts_cover_our_glyphs,
        }
    }
}

/// Judges gathered [`Signals`]. Pure, and the whole of the policy.
pub fn decide(signals: &Signals) -> bool {
    if !signals.stdout_is_terminal || signals.remote || term_is_hopeless(&signals.term) {
        return false;
    }
    signals.fonts_cover_our_glyphs.unwrap_or(false)
}

/// Whether the terminal is one that will not draw private-use glyphs whatever font is
/// installed.
fn term_is_hopeless(term: &Option<String>) -> bool {
    match term.as_deref() {
        None | Some("") | Some("dumb") => true,
        // The Linux virtual console draws from a 256- or 512-glyph font loaded into the
        // adapter; it has no access to the system's fonts at all.
        Some("linux") | Some("console") => true,
        Some(_) => false,
    }
}

/// Every private-use code point mdless can draw, gathered from the glyph tables.
///
/// Duplicates are left in; the query is built from a small set and fontconfig does not
/// care. What matters is that nothing is *missing*, which is guaranteed by reading the
/// same tables the drawing code reads.
pub fn required_code_points() -> Vec<u32> {
    let mut points: Vec<u32> = Glyphs::nerd_glyphs()
        .chain(Icons::nerd_glyphs())
        .flat_map(str::chars)
        .filter(|ch| is_private_use(*ch))
        .map(u32::from)
        .collect();
    points.sort_unstable();
    points.dedup();
    points
}

/// Asks fontconfig whether one installed font covers all of [`required_code_points`].
///
/// `None` when fontconfig could not be asked — it is not installed, or it failed. That
/// is the common case on macOS and in minimal containers, and it means "unknown", which
/// [`decide`] resolves to plain Unicode.
///
/// The query is a single `fc-list` call whose `:charset=` predicate lists every code
/// point at once; fontconfig treats that as "covers all of these", which is exactly the
/// condition for the glyphs to render. Asking about a representative sample instead
/// would pass on fonts that cover the classic Font Awesome block but not the newer
/// Material range the status bar uses.
fn fonts_cover_our_glyphs() -> Option<bool> {
    let points = required_code_points();
    if points.is_empty() {
        return Some(true);
    }
    let charset = points
        .iter()
        .map(|point| format!("{point:x}"))
        .collect::<Vec<_>>()
        .join(" ");

    let output = Command::new("fc-list")
        .arg(format!(":charset={charset}"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(!output.stdout.iter().all(u8::is_ascii_whitespace))
}

/// Whether to draw Nerd Font glyphs, when nothing has been said either way.
pub fn detect() -> bool {
    decide(&Signals::probe())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Signals from a well-equipped local terminal.
    fn equipped() -> Signals {
        Signals {
            term: Some("xterm-256color".to_string()),
            stdout_is_terminal: true,
            remote: false,
            fonts_cover_our_glyphs: Some(true),
        }
    }

    #[test]
    fn a_local_terminal_with_the_fonts_gets_glyphs() {
        assert!(decide(&equipped()));
    }

    #[test]
    fn every_way_of_not_knowing_falls_back_to_plain() {
        // The whole policy in one place: each of these on its own is enough to give up,
        // because the cost of guessing wrong is a screen full of tofu.
        let cases: [(&str, Signals); 6] = [
            (
                "no font covers the glyphs",
                Signals {
                    fonts_cover_our_glyphs: Some(false),
                    ..equipped()
                },
            ),
            (
                "fontconfig could not be asked",
                Signals {
                    fonts_cover_our_glyphs: None,
                    ..equipped()
                },
            ),
            (
                "output is redirected, so there is no terminal to ask about",
                Signals {
                    stdout_is_terminal: false,
                    ..equipped()
                },
            ),
            (
                "the fonts here say nothing about the terminal at the other end",
                Signals {
                    remote: true,
                    ..equipped()
                },
            ),
            (
                "a dumb terminal draws nothing fancy",
                Signals {
                    term: Some("dumb".to_string()),
                    ..equipped()
                },
            ),
            (
                "the linux console has its own tiny font",
                Signals {
                    term: Some("linux".to_string()),
                    ..equipped()
                },
            ),
        ];
        for (why, signals) in cases {
            assert!(!decide(&signals), "should have fallen back to plain: {why}");
        }
    }

    #[test]
    fn an_unset_term_is_not_trusted() {
        for term in [None, Some(String::new())] {
            assert!(!decide(&Signals { term, ..equipped() }));
        }
    }

    #[test]
    fn the_probe_requires_every_glyph_the_program_can_draw() {
        let points = required_code_points();
        assert!(
            !points.is_empty(),
            "an empty probe would mean detection always says yes"
        );

        // Both glyph tables must be represented, or half the program could be drawing
        // tofu with detection none the wiser: the status bar's file marker stands for
        // the chrome, and the renderer's unticked task box for the document. (The
        // renderer's heading markers used to stand for the second half; they were
        // removed on 2026-08-09, and the bullets that might have replaced them are
        // plain Unicode in both sets now.) Both now live in the Material range that
        // only Nerd Fonts v3 carries — the task boxes moved there on 2026-08-09 to be
        // a matched pair — so a v2 patch fails detection and gets plain Unicode,
        // which is the safe direction of the rule.
        assert!(
            points.contains(&0xf0219),
            "the chrome's glyphs are not probed"
        );
        assert!(
            points.contains(&0xf0131),
            "the renderer's glyphs are not probed"
        );
        // The code-fence icons are still classic Font Awesome, so the probe does
        // continue to span both blocks.
        assert!(
            points.iter().any(|point| (0xe000..=0xf8ff).contains(point)),
            "the basic-plane block is not probed"
        );

        // Everything probed must really need a patched font: probing an ordinary
        // character would make the answer always yes.
        for point in &points {
            let ch = char::from_u32(*point).expect("code points come from real glyphs");
            assert!(is_private_use(ch), "{ch:?} is not a private-use code point");
        }

        // And nothing the program can draw may be left out.
        for glyph in Glyphs::nerd_glyphs().chain(Icons::nerd_glyphs()) {
            for ch in glyph.chars().filter(|ch| is_private_use(*ch)) {
                assert!(
                    points.contains(&u32::from(ch)),
                    "{ch:?} is drawn but never probed"
                );
            }
        }
    }

    #[test]
    fn probing_the_real_environment_does_not_panic() {
        // The answer depends on the machine, so there is nothing to assert about it;
        // what matters is that gathering it is safe with or without fontconfig.
        let signals = Signals::probe();
        let _ = decide(&signals);
    }
}
