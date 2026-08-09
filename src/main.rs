//! The `mdless` binary.
//!
//! This is the CLI edge of the program: argument parsing, stdin handling and the
//! `--render-once` dump mode live here, and this is the only place `anyhow` is used.
//!
//! Design spec §11 in one paragraph: the document comes from a file or from standard
//! input; keyboard input is read from `/dev/tty` when standard input is a pipe, so
//! `cat x.md | mdless` and `PAGER=mdless` both work; when standard output is not a
//! terminal, `--render-once` is implied so `mdless x.md | cat` produces text rather
//! than escape soup. Exit codes are 0 for success, 1 for unreadable input and 2 for
//! bad arguments; the reader of a pipe closing it early (`mdless x.md | head`) is not
//! a failure and also exits 0, silently.

use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

use mdless::config::Config;
use mdless::doc::Doc;
use mdless::render::RenderOptions;
use mdless::theme::Theme;
use mdless::tui::{self, App, AppOptions, dump};

/// Exit code for an input that could not be read.
const EXIT_INPUT: u8 = 1;
/// Exit code for arguments that made no sense.
const EXIT_USAGE: u8 = 2;
/// The width used for a one-shot render when nothing else says otherwise.
const FALLBACK_WIDTH: u16 = 80;

/// A full-screen terminal pager for a single Markdown document.
#[derive(Debug, Parser)]
#[command(name = "mdless", version, about, long_about = None)]
struct Cli {
    /// The document to show. Omit it, or pass `-`, to read standard input.
    file: Option<PathBuf>,

    /// Render one frame to standard output and exit. Needs no terminal.
    #[arg(long)]
    render_once: bool,

    /// Render at this width instead of the terminal's.
    #[arg(long, value_name = "N")]
    width: Option<u16>,

    /// The theme to start in.
    #[arg(long, value_name = "NAME")]
    theme: Option<String>,

    /// Use plain Unicode instead of Nerd Font glyphs, at the same display width.
    ///
    /// By default mdless checks whether an installed font has the glyphs and uses plain
    /// Unicode whenever it cannot tell — over SSH, for instance, where the fonts on this
    /// machine say nothing about the terminal at the other end. Set MDLESS_ICONS=1 or
    /// `icons = true` to override the check for good.
    #[arg(long)]
    no_icons: bool,

    /// Use Nerd Font glyphs even if none appears to be installed.
    #[arg(long, conflicts_with = "no_icons")]
    icons: bool,

    /// Capture the mouse: wheel scrolls, clicks select in the contents pane.
    ///
    /// Off by default because capturing takes the terminal's own drag-select away.
    #[arg(long)]
    mouse: bool,

    /// Start with the table-of-contents pane open.
    #[arg(long)]
    toc: bool,

    /// Read configuration from this file instead of the default location.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

fn main() -> ExitCode {
    // `clap` exits with 2 on a usage error of its own accord; see `Cli::command`
    // below for the explicit setting.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(if error.use_stderr() { EXIT_USAGE } else { 0 });
        }
    };
    match run(cli) {
        Ok(code) => code,
        // `mdless x.md | head` is the ordinary `$PAGER` idiom, and the reader closing
        // the pipe is not a failure: pagers exit quietly (usability P12, visual P17).
        Err(error) if is_broken_pipe(&error) => ExitCode::SUCCESS,
        Err(error) => {
            // The terminal may or may not have been taken over; restoring twice is
            // harmless and restoring not at all is not.
            tui::restore_terminal();
            let _ = writeln!(io::stderr(), "mdless: {error:#}");
            ExitCode::from(EXIT_INPUT)
        }
    }
}

/// Whether to draw Nerd Font glyphs, from the four places that may have an opinion.
///
/// In order of authority: the command line, then `MDLESS_ICONS`, then the config file,
/// then — only if nobody has said — detection, which falls back to plain Unicode
/// whenever it cannot establish that the glyphs will render (see [`mdless::nerdfont`]).
///
/// The nearer answer wins outright rather than combining with the others, so
/// `--no-icons` turns glyphs off for one run of a config that enables them, and
/// `--icons` turns them on where detection would have given up.
fn resolve_icons(cli: &Cli, configured: Option<bool>) -> bool {
    if cli.no_icons {
        return false;
    }
    if cli.icons {
        return true;
    }
    if let Some(from_env) = env_icons() {
        return from_env;
    }
    configured.unwrap_or_else(mdless::nerdfont::detect)
}

/// `MDLESS_ICONS` as a yes or a no, or `None` if it is unset or not either.
///
/// An unrecognised value is ignored rather than rejected: this is a convenience meant
/// for a shell profile, and refusing to start over it would be a poor trade.
fn env_icons() -> Option<bool> {
    let raw = std::env::var("MDLESS_ICONS").ok()?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Whether a failure is nothing worse than the reader of our output going away.
fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|io_error| io_error.kind() == io::ErrorKind::BrokenPipe)
    })
}

/// The real entry point.
fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    if cli.width == Some(0) {
        let _ = writeln!(io::stderr(), "mdless: --width must be at least 1");
        return Ok(ExitCode::from(EXIT_USAGE));
    }

    let loaded = match &cli.config {
        Some(path) => Config::load_from(path),
        None => Config::load(),
    };
    for problem in &loaded.problems {
        let _ = writeln!(io::stderr(), "mdless: {problem}");
    }
    let mut config = loaded.config;

    let (source, title) = match read_input(cli.file.as_deref()) {
        Ok(pair) => pair,
        Err(error) => {
            let _ = writeln!(io::stderr(), "mdless: {error}");
            return Ok(ExitCode::from(EXIT_INPUT));
        }
    };
    let doc = Doc::parse_auto(&source);

    let theme_name = cli.theme.clone().unwrap_or_else(|| config.theme.clone());
    let icons = resolve_icons(&cli, config.icons);
    if cli.mouse {
        config.mouse = true;
    }
    let stdout_is_terminal = io::stdout().is_terminal();

    if cli.render_once || !stdout_is_terminal {
        let options =
            RenderOptions::new(icons, config.line_numbers).with_title_banner(config.title_banner);
        return render_once(
            &doc,
            &config,
            &theme_name,
            cli.width,
            stdout_is_terminal,
            &options,
        );
    }

    let config_toc_open = config.toc_open;
    let mut app = App::new(
        doc,
        config,
        AppOptions {
            title,
            icons,
            theme: theme_name,
            // `[toc] open` in the configuration file counts as much as `--toc` does.
            toc_open: cli.toc || config_toc_open,
            width: cli.width,
        },
    );
    tui::run(&mut app)?;
    Ok(ExitCode::SUCCESS)
}

/// Renders one frame to standard output and returns.
///
/// Deterministic by construction: the width is either given or falls back to a fixed
/// 80 columns when there is no terminal to ask, and the output carries colour only
/// when a terminal is there to show it.
fn render_once(
    doc: &Doc,
    config: &Config,
    theme_name: &str,
    width: Option<u16>,
    stdout_is_terminal: bool,
    options: &RenderOptions,
) -> anyhow::Result<ExitCode> {
    let theme = match config.resolve_theme(theme_name) {
        Ok(theme) => theme,
        Err(error) => {
            let _ = writeln!(io::stderr(), "mdless: {error}, using the dark theme");
            Theme::default_dark()
        }
    };
    let width = width.unwrap_or_else(|| {
        if stdout_is_terminal {
            tui::terminal_width().unwrap_or(FALLBACK_WIDTH)
        } else {
            FALLBACK_WIDTH
        }
    });
    let canvas = mdless::render::render_document(doc, width.max(1), &theme, options);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    if stdout_is_terminal {
        dump::write_ansi(&mut out, &canvas, theme.base())?;
    } else {
        dump::write_plain(&mut out, &canvas)?;
    }
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

/// Reads the document and works out what to call it in the status bar.
fn read_input(file: Option<&Path>) -> Result<(String, String), mdless::Error> {
    let from_stdin = match file {
        None => true,
        Some(path) => path.as_os_str() == "-",
    };
    if from_stdin {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|source_error| mdless::Error::Input {
                path: PathBuf::from("-"),
                source: source_error,
            })?;
        return Ok((source, "(standard input)".to_string()));
    }
    let path = file.unwrap_or(Path::new("-"));
    let source = std::fs::read_to_string(path).map_err(|source| mdless::Error::Input {
        path: path.to_path_buf(),
        source,
    })?;
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok((source, title))
}
