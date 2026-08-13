//! Opening a link in the reader's browser, and being honest about whether it worked.
//!
//! # Why the command is split from the spawn
//!
//! Mirrors `src/tui/clipboard.rs`'s [`classify`](super::clipboard) seam: the decision
//! of *what to run* is pulled out of the code that actually runs it, so the decision can
//! be pinned down in a test that spawns nothing. [`command_for`] is that pure half —
//! program name and argv, and nothing else — and [`open`] is the thin wrapper that
//! spawns exactly what it returns.
//!
//! # A URL is attacker-controlled data
//!
//! The URL reaching this module has already passed [`crate::render::link::classify`], so
//! it is `http` or `https` — but nothing else about it is constrained. A Markdown
//! document is written by someone other than the reader, and it may contain spaces,
//! semicolons, quotes, backticks, newlines, `$(...)`, a leading dash, or a stray control
//! character. None of that may reach a shell: the URL is handed to the opener as
//! exactly one `argv` entry, on every platform, and no platform's command line here is
//! built by joining strings that a shell would then re-parse.
//!
//! # Why detached
//!
//! The UI must never block on the opener. `xdg-open`, `open` and `cmd /c start` all
//! return once the request has been handed off — to a running desktop session, a
//! D-Bus service, or a freshly spawned browser — but "handed off" is not "finished", and
//! waiting for the child would freeze the pager for however long the browser takes to
//! start. So the child is spawned with its standard streams detached from the pager's
//! and never waited on; [`std::process::Child`] is dropped immediately; on unix an
//! orphaned child is reparented to init and reaped there, not by `mdmost`.
//!
//! # Where its stderr goes
//!
//! Piping the child's stderr and reading it would mean waiting on the child, which the
//! previous section rules out. It is instead pointed at the same file descriptor
//! `mdmost`'s own standard error is redirected to while the alternate screen is up
//! ([`super::stderr`]) — inherited via [`std::process::Stdio::inherit`], so whatever the
//! opener prints lands wherever `mdmost`'s own diagnostics currently do, and never
//! corrupts the frame.

use std::process::{Command, Stdio};

/// What became of an attempt to open a URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The opener command was launched. This does not mean a browser window appeared —
    /// only that handing the request off did not fail outright.
    Launched,
    /// The command could not even be spawned, with the reason.
    Failed(String),
}

/// The program and argv that would open `url`, without running it.
///
/// Split out from [`open`] because [`open`] has a side effect and this has none: the
/// decision table is the part worth pinning down in a test, and a test of it must not
/// launch a browser. See the module docs for why the URL is always exactly one `argv`
/// entry and never passed through a shell.
///
/// No `--` end-of-options marker is inserted before `url`, so in isolation a URL like
/// `-h` would be read by the opener as a flag rather than a target. That is not
/// reachable today only because of a safety property this function does not itself
/// hold: [`crate::render::link::classify`] emits `HotspotKind::Open` — the only route a
/// `url` takes to get here — solely for a target whose scheme is `http` or `https`
/// followed by `://`, so every string this function ever receives already begins with
/// that scheme text and cannot open with a dash. A future relaxation of that allowlist
/// (a bare `//example.com`, say) would reopen this silently; this comment is the record
/// that the dependency exists, for whoever touches either end of it next.
fn command_for(url: &str) -> (&'static str, Vec<String>) {
    if cfg!(target_os = "macos") {
        ("open", vec![url.to_string()])
    } else if cfg!(target_os = "windows") {
        // `cmd /c start` treats its first quoted argument as the new window's title,
        // not as the thing to open — a URL that itself begins with a quote or looks
        // like a switch would otherwise be misread as that title. The empty title is
        // the documented way to say "no title, the next argument is the target", and
        // this is the one platform where a fourth argv entry is correct rather than a
        // smell: `url` is still exactly one argument, on its own, after it.
        (
            "cmd",
            vec![
                "/c".to_string(),
                "start".to_string(),
                String::new(),
                url.to_string(),
            ],
        )
    } else {
        ("xdg-open", vec![url.to_string()])
    }
}

/// Opens `url` in the reader's browser.
///
/// Spawns the platform opener directly — never a shell — with `url` as its sole
/// argument, detached so the pager never waits on it, and with its standard streams
/// pointed away from the alternate screen. Failure is returned rather than reported
/// directly: the caller owns the status bar, and this module touches only the process
/// table.
pub fn open(url: &str) -> Outcome {
    let (program, args) = command_for(url);
    match Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => {
            // Not waited on: an orphaned child is reparented to init (unix) or simply
            // outlives its handle (Windows) and the pager never blocks on it. Dropping
            // the `Child` here does not kill it — only `Child::kill` would.
            drop(child);
            Outcome::Launched
        }
        Err(error) => Outcome::Failed(format!("could not open {url}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_is_built_with_a_direct_argv_and_no_shell() {
        // Nothing in a URL may be interpolated into a command line. A URL is attacker
        // -controlled data: `; rm -rf ~` is a legal thing to find in a document.
        let argv = command_for("https://example.com/a;%20rm%20-rf%20~");
        assert_eq!(argv.1.len(), 1, "exactly one argument, the url itself");
        assert_eq!(argv.1[0], "https://example.com/a;%20rm%20-rf%20~");
        assert!(
            !argv.0.contains("sh") && !argv.0.contains("cmd"),
            "no shell may be involved on unix; got {}",
            argv.0
        );
    }

    #[test]
    fn a_url_that_looks_like_a_flag_is_still_one_argument() {
        // `--foo` as a URL must not be read as an option by the opener.
        let argv = command_for("https://example.com/--version");
        assert_eq!(argv.1.len(), 1);
    }

    #[test]
    fn a_url_with_shell_metacharacters_survives_untouched() {
        // The pure half is where every security assertion lives; it must see the exact
        // bytes a shell would have chewed up, unmangled.
        let url = "https://example.com/`id`$(whoami)|cat;\n--flag";
        let argv = command_for(url);
        assert_eq!(argv.1, vec![url.to_string()]);
    }

    #[test]
    fn the_program_is_never_a_shell_on_any_recognised_platform() {
        let (program, _) = command_for("https://example.com");
        assert!(
            !["sh", "bash", "zsh", "/bin/sh", "cmd.exe", "powershell"].contains(&program),
            "opener program must never be a shell; got {program}"
        );
    }
}
