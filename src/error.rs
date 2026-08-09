//! Library error types.
//!
//! Every fallible operation in the `mdless` library reports failures through one of
//! the types in this module. The binary edge (`main.rs`) converts them to
//! [`anyhow::Error`]; the library itself never uses `anyhow`.

use std::path::PathBuf;

/// The catch-all error type of the `mdless` library.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The input document could not be read.
    #[error("cannot read input {path}: {source}")]
    Input {
        /// The path that could not be read (`-` for standard input).
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },

    /// A configuration file could not be loaded or was invalid.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// A theme could not be resolved or was invalid.
    #[error(transparent)]
    Theme(#[from] ThemeError),

    /// A Mermaid diagram could not be parsed or drawn.
    #[error(transparent)]
    Mermaid(#[from] MermaidError),

    /// A canvas operation violated the canvas contract.
    #[error(transparent)]
    Canvas(#[from] CanvasError),

    /// An I/O failure that is not tied to a specific input path.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// A convenient result alias for library operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Failures while loading or merging configuration.
///
/// Per the design spec, configuration problems never prevent startup: the caller is
/// expected to report the error and fall back to defaults.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The configuration file could not be read.
    #[error("cannot read config {path}: {source}")]
    Read {
        /// The configuration file path.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },

    /// The configuration file could not be written.
    #[error("cannot write config {path}: {source}")]
    Write {
        /// The configuration file path.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },

    /// The configuration this version would have written does not read back the same.
    ///
    /// The writer edits the file the reader wrote, and it checks its own work before
    /// touching the disk: it parses the text it is about to write and compares the
    /// settings that come back with the ones it meant to save. A mismatch means the
    /// edit would have changed the file's meaning in some way nobody predicted, and
    /// the only safe answer is to leave the reader's file alone and say so.
    #[error("refusing to write {path}: `{key}` would not read back the same")]
    RoundTrip {
        /// The configuration file path, left untouched.
        path: PathBuf,
        /// The setting that did not survive the round trip.
        key: String,
    },

    /// The configuration file could not be parsed.
    #[error("{path}:{line}: invalid config{}: {message}", .key.as_ref().map(|k| format!(" key `{k}`")).unwrap_or_default())]
    Parse {
        /// The configuration file path.
        path: PathBuf,
        /// The 1-based line number of the offending entry.
        line: usize,
        /// The offending key, when one could be identified.
        key: Option<String>,
        /// A human-readable description of the problem.
        message: String,
    },
}

/// Failures while resolving a theme.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ThemeError {
    /// No built-in or configured theme has the requested name.
    #[error("unknown theme `{0}`")]
    UnknownTheme(String),

    /// A colour literal was not a valid `#rrggbb` (or `#rgb`) value.
    #[error("invalid colour `{0}`: expected `#rgb` or `#rrggbb`")]
    InvalidColor(String),
}

/// Failures while parsing or drawing a Mermaid diagram.
///
/// A Mermaid failure is always recoverable: the block renderer falls back to a
/// captioned code block using [`MermaidError::reason`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MermaidError {
    /// The diagram family (first keyword) is not one of the supported families.
    #[error("unsupported diagram type `{0}`")]
    UnsupportedFamily(String),

    /// A construct inside a supported family is outside the implemented subset.
    ///
    /// This variant blames the *source*, so it must only be used when there really is a
    /// line of the author's Mermaid to point at. `line` is 1-based and must never be
    /// `0`: a message reading "on line 0" sends the reader hunting for a typo on a line
    /// that does not exist. A failure that is not the author's fault belongs in
    /// [`MermaidError::Internal`] instead.
    #[error("unsupported syntax on line {line}: {message}")]
    Unsupported {
        /// The 1-based line number within the Mermaid source.
        line: usize,
        /// A human-readable description of the unsupported construct.
        message: String,
    },

    /// `mdless` built an inconsistent drawing request for its own layout engine.
    ///
    /// Nothing the author wrote can cause this — it means a renderer handed the graph
    /// engine a specification that violates the engine's contract, for instance a node
    /// claimed by two containers. It is kept recoverable rather than made a panic,
    /// because a wrecked diagram must never take down a document the reader is trying
    /// to read (design spec §12). There is deliberately no line number: attaching one
    /// would be inventing a location in source that is not at fault.
    #[error("mdless could not draw this diagram: {message}")]
    Internal {
        /// What was inconsistent, for a bug report rather than for the author.
        message: String,
    },

    /// The Mermaid source could not be parsed at all.
    #[error("syntax error on line {line}: {message}")]
    Syntax {
        /// The 1-based line number within the Mermaid source.
        line: usize,
        /// A human-readable description of the problem.
        message: String,
    },

    /// The diagram cannot be drawn within the available width.
    #[error("diagram does not fit in {width} columns")]
    TooNarrow {
        /// The width budget the diagram was given.
        width: u16,
        /// The narrowest drawing the renderer managed, when it knows one.
        ///
        /// A **lower bound**, never a promise: it is the smallest canvas any
        /// degradation step produced at `width`, and every step draws at least as wide
        /// when given more room, so no width below this can ever fit. Telling the
        /// reader "at least N" turns widening the terminal from a guessing game into
        /// one move. Renderers that cannot name such a bound leave it `None` rather
        /// than invent one.
        needed: Option<u16>,
    },
}

impl MermaidError {
    /// The caption text shown under the fallback code block.
    ///
    /// The block renderer is expected to render `unsupported mermaid syntax: {reason}`
    /// in the dim caption style.
    pub fn reason(&self) -> String {
        self.to_string()
    }
}

/// Failures raised by canvas operations that would break the canvas contract.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CanvasError {
    /// A canvas was asked to shrink below the width it already occupies.
    #[error("cannot pad canvas of width {current} to smaller width {requested}")]
    Narrowing {
        /// The canvas' current width.
        current: u16,
        /// The requested (smaller) width.
        requested: u16,
    },
}
