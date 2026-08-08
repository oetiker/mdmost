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
    #[error("unsupported syntax on line {line}: {message}")]
    Unsupported {
        /// The 1-based line number within the Mermaid source.
        line: usize,
        /// A human-readable description of the unsupported construct.
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
