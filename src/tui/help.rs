//! The help overlay's content.
//!
//! The overlay is *generated from the live key table* (design spec §10), so a rebound
//! key shows up in the help without anybody having to remember to edit a list. There
//! is deliberately no hand-written help text anywhere in the program.

use crate::config::{Action, ActionGroup, KeyBindings};

/// One line of the help overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpRow {
    /// The chords bound to the action, joined for display.
    pub keys: String,
    /// What the action does.
    pub description: &'static str,
}

/// A titled block of help rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpSection {
    /// The section heading.
    pub title: &'static str,
    /// The rows, in the order [`Action::ALL`] lists them.
    pub rows: Vec<HelpRow>,
}

/// Builds the help overlay from the bindings currently in force.
///
/// Actions with no bound key are omitted: showing a chordless action would be telling
/// the user about something they cannot do.
pub fn sections(bindings: &KeyBindings) -> Vec<HelpSection> {
    ActionGroup::ALL
        .iter()
        .filter_map(|group| {
            let rows: Vec<HelpRow> = Action::ALL
                .iter()
                .filter(|action| action.group() == *group)
                .filter_map(|action| {
                    let keys = bindings.keys_for(*action);
                    if keys.is_empty() {
                        return None;
                    }
                    Some(HelpRow {
                        keys: keys
                            .iter()
                            .map(|key| key.label())
                            .collect::<Vec<_>>()
                            .join(" "),
                        description: action.description(),
                    })
                })
                .collect();
            (!rows.is_empty()).then_some(HelpSection {
                title: group.title(),
                rows,
            })
        })
        .collect()
}

/// One drawn line of a help column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpLine<'a> {
    /// Vertical space between two sections.
    Blank,
    /// A section heading.
    Title(&'a str),
    /// A binding.
    Row(&'a HelpRow),
}

/// Flattens sections into the lines they draw as, spacing included.
///
/// Turning the overlay into a flat list of lines is what lets it scroll: a scrolled
/// overlay has to be able to start half way down a section, which a nested title/rows
/// walk cannot express.
pub fn lines(sections: &[HelpSection]) -> Vec<HelpLine<'_>> {
    let mut out = Vec::new();
    for section in sections {
        if !out.is_empty() {
            out.push(HelpLine::Blank);
        }
        out.push(HelpLine::Title(section.title));
        out.extend(section.rows.iter().map(HelpLine::Row));
    }
    out
}

/// The widest key column across every section, for aligning the overlay.
pub fn key_column_width(sections: &[HelpSection]) -> usize {
    sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .map(|row| crate::text::display_width(&row.keys))
        .max()
        .unwrap_or(0)
}

/// The number of lines a run of sections needs, titles and spacing included.
pub fn line_count(sections: &[HelpSection]) -> usize {
    sections
        .iter()
        .map(|section| section.rows.len() + 2)
        .sum::<usize>()
        .saturating_sub(1)
}

/// Splits the sections into columns so the overlay fits `max_rows` rows.
///
/// A clipped help overlay is worse than no help overlay: the keys the reader cannot
/// see are exactly the ones they came looking for. Rather than truncate, the sections
/// are dealt into as many columns as needed and as `max_columns` allows, never
/// splitting a section across a column boundary.
pub fn columns(
    sections: Vec<HelpSection>,
    max_rows: usize,
    max_columns: usize,
) -> Vec<Vec<HelpSection>> {
    if sections.is_empty() || max_rows == 0 || max_columns == 0 {
        return vec![sections];
    }
    if line_count(&sections) <= max_rows || max_columns == 1 {
        return vec![sections];
    }
    let mut columns: Vec<Vec<HelpSection>> = Vec::new();
    let mut current: Vec<HelpSection> = Vec::new();
    for section in sections {
        let would_be = line_count(&current) + section.rows.len() + 2;
        if !current.is_empty() && would_be > max_rows && columns.len() + 1 < max_columns {
            columns.push(std::mem::take(&mut current));
        }
        current.push(section);
    }
    columns.push(current);
    columns
}
