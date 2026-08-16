// SPDX-License-Identifier: MIT
//! Writing the settings back to the configuration file, on request.
//!
//! Design spec §12.1. The reader presses `S` and next time the pager comes up the way
//! they left it. The hard part is not writing TOML — it is writing TOML *into a file
//! somebody else wrote*, which may carry comments, an ordering they chose, and keys a
//! newer version of mdmost understands and this one does not (the loader keeps such a
//! key with a warning rather than discarding the file, and the writer must not undo
//! that).
//!
//! Three structural protections, in place of a confirmation prompt the pager has no way
//! to ask:
//!
//! 1. **The file is edited, not regenerated.** Every line the writer does not have an
//!    opinion about is copied through byte for byte, trailing comments on the lines it
//!    does have an opinion about included. A setting with no line of its own is
//!    inserted into the section it belongs to, after that section's last real content,
//!    so a comment block introducing the *next* section keeps introducing it.
//! 2. **The writer checks its own work before touching the disk.** The text it is about
//!    to write is parsed back with the ordinary loader and compared, setting by setting,
//!    with what it meant to save. Anything that does not match means the edit changed
//!    the file's meaning in a way nobody predicted, and the answer is to leave the
//!    reader's file exactly as it was and say so ([`ConfigError::RoundTrip`]). This is
//!    the property `tests/config_save.rs` also asserts from the outside; asserting it
//!    here as well is what makes it true of files no test thought of.
//! 3. **The previous file is kept**, as `config.toml.bak`, and the new one arrives by
//!    rename, so an interrupted save cannot leave a half-written configuration behind.

use std::path::{Path, PathBuf};

use crate::error::ConfigError;

use super::Config;

/// The suffix of the copy kept of the file that was replaced.
const BACKUP_SUFFIX: &str = "bak";

/// The suffix of the file the new text is written to before being renamed into place.
const TEMP_SUFFIX: &str = "tmp";

/// One setting as it appears in the file.
struct Entry {
    /// The `[section]` it lives in, or `None` for the top level.
    section: Option<&'static str>,
    /// The key, exactly as it is spelled in the file.
    key: &'static str,
    /// The value to write, or `None` to leave the file's answer — whatever it is — be.
    value: Option<String>,
}

impl Config {
    /// Saves the settings the reader can change into the configuration file at `path`.
    ///
    /// The file is created — directories and all — when it is not there, and edited in
    /// place when it is: see the module documentation for what "in place" guarantees.
    /// The key table and any `[themes.*]` are never written; they are the reader's, and
    /// nothing in the pager changes them.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Write`] when the file cannot be written, and
    /// [`ConfigError::RoundTrip`] when the text this would have written does not parse
    /// back to the settings it was asked to save — in which case nothing is written.
    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        let failed = |source: std::io::Error| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        };
        let existing = match std::fs::read_to_string(path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(failed(error)),
        };
        let text = self.edited(existing.as_deref().unwrap_or_default());
        self.verify(&text, path)?;

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        if existing.is_some() {
            std::fs::copy(path, with_suffix(path, BACKUP_SUFFIX)).map_err(failed)?;
        }
        // Written beside the target and renamed over it, so a save interrupted halfway
        // leaves the old configuration rather than half of the new one.
        let temporary = with_suffix(path, TEMP_SUFFIX);
        std::fs::write(&temporary, &text).map_err(failed)?;
        std::fs::rename(&temporary, path).map_err(failed)
    }

    /// The settings written back, in the order a file that has none of them gets them.
    fn entries(&self) -> Vec<Entry> {
        let quoted = |text: &str| toml::Value::from(text).to_string();
        vec![
            Entry {
                section: None,
                key: "theme",
                value: Some(quoted(&self.theme)),
            },
            Entry {
                section: None,
                // An unset `icons` is the answer "nobody has said, ask detection"
                // (design spec §2.1), and writing detection's answer down would freeze
                // it on a machine that later grows a Nerd Font. The command line and
                // `MDMOST_ICONS` *are* statements, and `main` records them here before
                // the pager starts, so saving keeps them.
                key: "icons",
                value: self.icons.map(|icons| icons.to_string()),
            },
            Entry {
                section: None,
                key: "line_numbers",
                value: Some(self.line_numbers.to_string()),
            },
            Entry {
                section: None,
                key: "mouse",
                value: Some(self.mouse.to_string()),
            },
            Entry {
                section: None,
                key: "scroll_step",
                value: Some(self.scroll_step.to_string()),
            },
            Entry {
                section: None,
                key: "body_width",
                // `0` is how the file spells "no cap"; see `Config::body_width`.
                value: Some(self.body_width.unwrap_or(0).to_string()),
            },
            Entry {
                section: Some("toc"),
                key: "open",
                value: Some(self.toc_open.to_string()),
            },
            Entry {
                section: Some("toc"),
                key: "width",
                value: Some(self.toc_width.to_string()),
            },
        ]
    }

    /// `existing` with every setting brought up to date and everything else untouched.
    fn edited(&self, existing: &str) -> String {
        let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
        for entry in self.entries() {
            let Some(value) = entry.value.as_deref() else {
                continue;
            };
            match find_key(&lines, entry.section, entry.key) {
                Some(index) => lines[index] = rewritten(&lines[index], entry.key, value),
                None => {
                    let at = insertion_point(&mut lines, entry.section);
                    lines.insert(at, format!("{} = {value}", entry.key));
                }
            }
        }
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        text
    }

    /// Refuses the write unless the text parses back to the settings it means to save.
    fn verify(&self, text: &str, path: &Path) -> Result<(), ConfigError> {
        let back = Config::parse_str(text, path).config;
        let refuse = |key: &str| {
            Err(ConfigError::RoundTrip {
                path: path.to_path_buf(),
                key: key.to_string(),
            })
        };
        // Listed one by one rather than compared whole, so the message can name the
        // setting that did not survive. `icons` is compared as written: a setting the
        // writer deliberately leaves out must still be absent when it is read back.
        if back.theme != self.theme {
            return refuse("theme");
        }
        if back.icons != self.icons {
            return refuse("icons");
        }
        if back.line_numbers != self.line_numbers {
            return refuse("line_numbers");
        }
        if back.mouse != self.mouse {
            return refuse("mouse");
        }
        if back.scroll_step != self.scroll_step {
            return refuse("scroll_step");
        }
        if back.body_width != self.body_width {
            return refuse("body_width");
        }
        if back.toc_open != self.toc_open {
            return refuse("toc.open");
        }
        if back.toc_width != self.toc_width {
            return refuse("toc.width");
        }
        // Nothing here writes either of these, so a difference means the edit damaged
        // part of the file that was none of its business.
        if back.keys != self.keys {
            return refuse("keys");
        }
        if back.themes != self.themes {
            return refuse("themes");
        }
        Ok(())
    }
}

/// `path` with `suffix` added to its file name, e.g. `config.toml.bak`.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

/// The section a line opens, if it is a `[section]` header.
///
/// `[[array]]` headers are recognised too: the name is not what matters, only that
/// everything after such a line belongs to a different table than what came before.
fn section_header(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.trim().trim_start_matches('[').trim_end_matches(']'))
}

/// The key a line assigns to, if it assigns to one.
///
/// Only the bare and the double-quoted spellings are recognised, which is every
/// spelling of every key this writer knows. Anything else is left to the round-trip
/// check to catch.
fn assigned_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let (left, _) = trimmed.split_once('=')?;
    let left = left.trim();
    Some(
        left.strip_prefix('"')
            .and_then(|k| k.strip_suffix('"'))
            .unwrap_or(left),
    )
}

/// The index of the line assigning `key` inside `section`, if there is one.
fn find_key(lines: &[String], section: Option<&str>, key: &str) -> Option<usize> {
    let mut current: Option<&str> = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some(header) = section_header(line) {
            current = Some(header);
            continue;
        }
        if current == section && assigned_key(line) == Some(key) {
            return Some(index);
        }
    }
    None
}

/// Where a new key belongs in `lines`, creating the `[section]` header if it is missing.
///
/// The point is the end of the section's real content: any run of blank lines and
/// comments that trails the section is left below the new key, because such a run is
/// almost always the comment introducing whatever comes next.
fn insertion_point(lines: &mut Vec<String>, section: Option<&str>) -> usize {
    let mut current: Option<&str> = None;
    let mut end: Option<usize> = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some(header) = section_header(line) {
            if current == section {
                end = Some(index);
                break;
            }
            current = Some(header);
        }
    }
    let mut at = match (end, current == section) {
        // The section ends where the next one begins.
        (Some(index), _) => index,
        // It is the last section in the file, or the file has no headers at all.
        (None, true) => lines.len(),
        // The section is not in the file: it has to be opened first, at the end.
        (None, false) => {
            if let Some(name) = section {
                if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                    lines.push(String::new());
                }
                lines.push(format!("[{name}]"));
            }
            return lines.len();
        }
    };
    while at > 0 {
        let previous = lines[at - 1].trim();
        if previous.is_empty() || previous.starts_with('#') {
            at -= 1;
        } else {
            break;
        }
    }
    at
}

/// `line` with its value replaced, keeping its indentation and any trailing comment.
fn rewritten(line: &str, key: &str, value: &str) -> String {
    let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
    let spelling = line
        .trim_start()
        .split_once('=')
        .map_or(key, |(left, _)| left.trim());
    let after = line.split_once('=').map_or("", |(_, right)| right);
    let comment = match comment_start(after) {
        Some(start) => {
            let gap: String = after[..start]
                .chars()
                .rev()
                .take_while(|ch| ch.is_whitespace())
                .collect();
            format!("{gap}{}", &after[start..])
        }
        None => String::new(),
    };
    format!("{indent}{spelling} = {value}{comment}")
}

/// Where a trailing comment starts in the text after a `=`, if one does.
///
/// Quotes are tracked so that a `#` inside a string value — a colour like `#ff8800`,
/// which this project's own themes are full of — is not mistaken for one.
fn comment_start(after: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (index, ch) in after.char_indices() {
        match (quote, ch) {
            (Some(open), ch) if ch == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '#') => return Some(index),
            (None, _) => {}
        }
    }
    None
}
