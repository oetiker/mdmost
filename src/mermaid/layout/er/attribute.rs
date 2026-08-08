//! The attribute table inside an entity box.
//!
//! Design spec §6.4 describes an entity's `{ … }` block as a table, so it is drawn as
//! one: type, name, key markers and comment each get a column sized to the widest entry
//! in that entity, and every row is padded to those widths. Formatting each attribute
//! independently would leave the block ragged and much harder to scan.
//!
//! Only columns that some attribute actually uses are drawn, so an entity whose
//! attributes carry no keys and no comments is a plain two-column table rather than one
//! with two empty columns of padding.

use crate::mermaid::ast::{ErAttribute, ErKey};
use crate::text::{Align, display_width, pad_to_width};

/// Blank columns between two table columns.
const GAP: usize = 2;

/// Lays the attributes out as aligned table rows.
///
/// Returns one string per attribute, all of the same display width.
pub(super) fn table(attributes: &[ErAttribute]) -> Vec<String> {
    if attributes.is_empty() {
        return Vec::new();
    }

    let keys: Vec<String> = attributes.iter().map(|a| markers(&a.keys)).collect();
    let comments: Vec<String> = attributes.iter().map(comment).collect();

    let ty_width = width_of(attributes.iter().map(|a| a.ty.as_str()));
    let name_width = width_of(attributes.iter().map(|a| a.name.as_str()));
    let key_width = width_of(keys.iter().map(String::as_str));
    let comment_width = width_of(comments.iter().map(String::as_str));

    attributes
        .iter()
        .enumerate()
        .map(|(at, attribute)| {
            let mut row = pad_to_width(&attribute.ty, ty_width, Align::Left);
            push_column(&mut row, &attribute.name, name_width);
            push_column(&mut row, &keys[at], key_width);
            push_column(&mut row, &comments[at], comment_width);
            row.trim_end().to_string()
        })
        .collect()
}

/// Appends a gap and a padded column, unless the column is unused entirely.
fn push_column(row: &mut String, text: &str, width: usize) {
    if width == 0 {
        return;
    }
    row.push_str(&" ".repeat(GAP));
    row.push_str(&pad_to_width(text, width, Align::Left));
}

/// The widest entry in a column.
fn width_of<'a>(texts: impl Iterator<Item = &'a str>) -> usize {
    texts.map(display_width).max().unwrap_or(0)
}

/// The key markers of one attribute, e.g. `PK` or `PK,FK`.
fn markers(keys: &[ErKey]) -> String {
    keys.iter()
        .map(|key| match key {
            ErKey::Primary => "PK",
            ErKey::Foreign => "FK",
            ErKey::Unique => "UK",
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The quoted comment of one attribute, or an empty string.
fn comment(attribute: &ErAttribute) -> String {
    match attribute.comment.as_deref().map(str::trim) {
        Some(text) if !text.is_empty() => format!("\"{text}\""),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attribute(ty: &str, name: &str, keys: Vec<ErKey>, comment: Option<&str>) -> ErAttribute {
        ErAttribute {
            ty: ty.to_string(),
            name: name.to_string(),
            keys,
            comment: comment.map(str::to_string),
        }
    }

    #[test]
    fn columns_line_up_across_rows() {
        let rows = table(&[
            attribute("string", "name", vec![ErKey::Primary], None),
            attribute("int", "age", Vec::new(), None),
        ]);
        assert_eq!(rows, vec!["string  name  PK", "int     age"]);
    }

    #[test]
    fn an_unused_column_takes_no_space() {
        let rows = table(&[
            attribute("string", "name", Vec::new(), None),
            attribute("int", "age", Vec::new(), None),
        ]);
        assert_eq!(rows, vec!["string  name", "int     age"]);
    }

    #[test]
    fn several_key_markers_are_joined() {
        assert_eq!(markers(&[ErKey::Primary, ErKey::Foreign]), "PK,FK");
        assert_eq!(markers(&[ErKey::Unique]), "UK");
        assert_eq!(markers(&[]), "");
    }

    #[test]
    fn a_comment_is_quoted_and_a_blank_one_is_dropped() {
        let rows = table(&[
            attribute("string", "a", Vec::new(), Some("hello")),
            attribute("string", "b", Vec::new(), Some("  ")),
        ]);
        assert_eq!(rows[0], "string  a  \"hello\"");
        assert_eq!(rows[1], "string  b");
    }

    #[test]
    fn an_empty_block_produces_no_rows() {
        assert!(table(&[]).is_empty());
    }

    #[test]
    fn wide_characters_are_measured_by_display_width() {
        let rows = table(&[
            attribute("文字列", "名前", Vec::new(), None),
            attribute("int", "age", Vec::new(), None),
        ]);
        let widths: Vec<usize> = rows.iter().map(|row| display_width(row)).collect();
        // Trailing padding is trimmed, so only the first row's columns are full width.
        assert_eq!(display_width(&rows[0]), widths[0]);
        assert!(rows[1].starts_with("int   "), "{:?}", rows[1]);
    }
}
