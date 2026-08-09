//! The pager must not outlive the terminal it is drawing on.
//!
//! A terminal can go away impolitely: a pty master closed without a hangup signal, an
//! emulator that crashes, a session torn down under a running pager. When that
//! happened the pager used to stay alive at 100 % of a core, holding the document
//! open, until somebody noticed and killed it. These tests give the binary a real pty,
//! destroy it, and insist that the process is gone shortly afterwards without having
//! burned a core to get there.
//!
//! They need a pty, so they cannot be unit tests. Linux only, for two reasons: the CPU
//! measurement reads `/proc`, and the fix itself is Linux-only because `poll` on
//! `/dev/tty` is documented not to work on macOS (see `tui::term::Input`). Every wait
//! has a hard deadline and every failure path kills the child: a future regression
//! must fail the suite rather than hang it or leave a runaway behind.

#![cfg(target_os = "linux")]

use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How long the pager gets to draw and go quiet before the terminal is destroyed.
const STARTUP_DEADLINE: Duration = Duration::from_secs(20);
/// How long to let the pager run after its first frame before pulling the terminal.
/// Long enough that startup — parse, layout, first draw — is certainly behind it.
const SETTLE: Duration = Duration::from_millis(1500);
/// How long a single wait for output may block.
const QUIET_MILLIS: i32 = 400;
/// How long the pager gets to notice its terminal is gone and exit. Generous on
/// purpose: this runs on shared machines, and the claim being tested is "it exits at
/// all", not "it exits fast".
const EXIT_DEADLINE: Duration = Duration::from_secs(10);
/// How much CPU time the pager may use between losing its terminal and exiting.
///
/// A spinning process burns 100 jiffies per second per core, so half a second of CPU
/// separates "it tidied up and left" from "it span" by a wide margin, and the bound is
/// a ratio against a physical constant rather than a wall-clock guess.
const MAX_EXIT_JIFFIES: u64 = 50;

/// A document with enough in it to lay out, small enough to start quickly.
const SAMPLE: &str = "\
# Title

Some prose that is long enough to be worth wrapping at a narrow width indeed.

## Section

- one
- two
";

/// The terminal is destroyed while the pager holds it as its controlling terminal, the
/// way a terminal emulator crashing does. The kernel raises `SIGHUP` in this case, but
/// the pager must not depend on it: the signal only interrupts the wait, and the loop
/// used never to get far enough afterwards to look at the flag.
#[test]
fn exits_when_the_controlling_terminal_is_destroyed() {
    check_hangup(Ctty::Yes);
}

/// The same, minus the `SIGHUP`: the pager has no controlling terminal, so closing the
/// master is entirely silent. Nothing but noticing the hangup on the input descriptor
/// can save the pager here.
#[test]
fn exits_when_a_terminal_without_a_hangup_signal_is_destroyed() {
    check_hangup(Ctty::No);
}

/// Whether the pty becomes the child's controlling terminal.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctty {
    Yes,
    No,
}

impl Ctty {
    fn label(self) -> &'static str {
        match self {
            Ctty::Yes => "ctty",
            Ctty::No => "no-ctty",
        }
    }
}

/// Starts the pager on a fresh pty, closes the master, and asserts that the process
/// exits promptly and without spinning.
fn check_hangup(ctty: Ctty) {
    let dir = temp_dir(ctty);
    let doc = dir.join("sample.md");
    std::fs::write(&doc, SAMPLE).expect("the sample document should be writable");

    let mut pty = Pty::open();
    let mut child = pty.spawn(&doc, ctty);
    let pid = child.id();

    // Letting the pager settle is what makes this test mean anything: it proves the
    // pager reached its event loop and is waiting there, so the hangup below is not
    // merely racing a process that had not started yet — or, worse, one parked in a
    // write, which fails loudly on its own and would hide the defect entirely.
    if let Err(reason) = pty.wait_until_idle() {
        fail(
            &mut child,
            &dir,
            &format!("the pager never settled: {reason}"),
        );
    }

    let before = cpu_jiffies(pid).unwrap_or(0);
    // The terminal vanishes.
    drop(pty);

    let mut last = before;
    let started = Instant::now();
    let mut exited = false;
    loop {
        if child
            .try_wait()
            .expect("the child should be waitable")
            .is_some()
        {
            exited = true;
            break;
        }
        if let Some(jiffies) = cpu_jiffies(pid) {
            last = jiffies;
        }
        if started.elapsed() > EXIT_DEADLINE {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    if !exited {
        let burned = last.saturating_sub(before);
        fail(
            &mut child,
            &dir,
            &format!(
                "the pager was still running {EXIT_DEADLINE:?} after its terminal was \
                 destroyed, having used {burned} jiffies of CPU since"
            ),
        );
    }
    let burned = last.saturating_sub(before);
    if burned > MAX_EXIT_JIFFIES {
        fail(
            &mut child,
            &dir,
            &format!(
                "the pager exited, but burned {burned} jiffies of CPU on the way out \
                 (at most {MAX_EXIT_JIFFIES} expected)"
            ),
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Cleans up and fails. Never returns.
fn fail(child: &mut Child, dir: &std::path::Path, message: &str) -> ! {
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(dir);
    panic!("{message}");
}

/// Total CPU time a process has used, in jiffies, or `None` where that cannot be read.
fn cpu_jiffies(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name sits in parentheses and may itself contain spaces, so the
    // numbered fields are whatever follows the last ')': field 3 onwards.
    let rest = stat.rsplit(')').next()?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // `utime` is field 14 and `stime` field 15, so index 11 and 12 from here.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// A private temporary directory for one test.
fn temp_dir(ctty: Ctty) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mdmost-pty-hangup-{}-{}",
        std::process::id(),
        ctty.label()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary directory should be creatable");
    dir
}

/// A pty pair. Dropping it closes the master, which is what destroys the terminal.
struct Pty {
    master: OwnedFd,
    slave: Option<OwnedFd>,
}

impl Pty {
    fn open() -> Self {
        let mut master = 0;
        let mut slave = 0;
        // A real window size: at 0×0 the pager has nothing to draw into and never
        // stops redrawing, so it would never look parked.
        let size = libc::winsize {
            ws_row: 40,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let ok = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        };
        assert_eq!(ok, 0, "openpty should succeed");
        // `openpty` hands back inheritable descriptors, and these tests run in
        // parallel: without this, the second test's pager inherits the first test's
        // master, the first pty never hangs up, and the test goes red for a reason
        // that has nothing to do with the pager.
        for fd in [master, slave] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            assert_ne!(flags, -1, "F_GETFD should succeed");
            let ok = unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
            assert_ne!(ok, -1, "F_SETFD should succeed");
        }
        Self {
            master: unsafe { OwnedFd::from_raw_fd(master) },
            slave: Some(unsafe { OwnedFd::from_raw_fd(slave) }),
        }
    }

    /// Starts the pager on the slave side, then closes our own copy of the slave so
    /// that only the child holds the terminal open.
    fn spawn(&mut self, doc: &std::path::Path, ctty: Ctty) -> Child {
        let slave = self.slave.as_ref().expect("the slave is still open");
        let stdin = slave.try_clone().expect("dup should succeed");
        let stdout = slave.try_clone().expect("dup should succeed");
        let stderr = slave.try_clone().expect("dup should succeed");
        let slave_fd = slave.as_raw_fd();
        let mut command = Command::new(env!("CARGO_BIN_EXE_mdmost"));
        command
            .arg(doc)
            .env("TERM", "xterm-256color")
            .env_remove("MDMOST_ICONS")
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        let want_ctty = ctty == Ctty::Yes;
        unsafe {
            command.pre_exec(move || {
                // A session of its own either way, so that a signal aimed at this
                // test's process group cannot reach the pager and confuse the result.
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if want_ctty && libc::ioctl(slave_fd, libc::TIOCSCTTY, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().expect("the binary should be runnable");
        self.slave = None;
        child
    }

    /// Drains the pager's output until it has settled into its event loop, returning
    /// the moment it has just finished a frame.
    ///
    /// Both halves matter. Draining means the pager never blocks writing into a full
    /// pty buffer — a blocked write fails loudly the moment the master closes, which
    /// would let this test pass while the defect it is about went untouched. Returning
    /// right after a frame means the pager is, as near as makes no difference, parked
    /// waiting for input when the terminal is destroyed, which is the path under test.
    fn wait_until_idle(&self) -> Result<(), String> {
        let started = Instant::now();
        let mut buffer = [0u8; 8192];
        let mut first_output: Option<Instant> = None;
        loop {
            let mut fds = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut fds, 1, QUIET_MILLIS) };
            if ready > 0 && fds.revents & libc::POLLIN != 0 {
                // Borrowed, not owned: `File` must not close the master on drop.
                let mut file =
                    std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fds.fd) });
                match file.read(&mut buffer) {
                    Ok(count) if count > 0 => {}
                    Ok(_) => return Err("the pty reached end of file".into()),
                    Err(error) => return Err(error.to_string()),
                }
                let first = *first_output.get_or_insert_with(Instant::now);
                if first.elapsed() > SETTLE {
                    return Ok(());
                }
            } else if ready == 0
                && first_output.is_some_and(|first: Instant| first.elapsed() > SETTLE)
            {
                // Settled and silent. Today `ratatui` writes a little on every frame
                // so this arm is not the one that fires, but the test should not
                // depend on that: silence is if anything better evidence of a pager
                // parked waiting for input.
                return Ok(());
            }
            if started.elapsed() > STARTUP_DEADLINE {
                return Err(format!(
                    "the pager had not settled within {STARTUP_DEADLINE:?} \
                     (drew anything: {})",
                    first_output.is_some()
                ));
            }
        }
    }
}
