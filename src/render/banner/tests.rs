//! Unit tests for the `FIGlet` title banner.
//!
//! The art is checked against `figlet -f Small` rather than against itself: a
//! transcribed font and a hand-written smushing algorithm can agree with each other
//! and still both be wrong, and the reference implementation is the only thing that
//! settles it.

use super::*;
use crate::doc::Doc;

/// Compares art against a reference block, ignoring the trailing blanks each row is
/// padded with and the blank rows `FIGlet` prints for absent descenders.
fn assert_art(banner: &Banner, reference: &str) {
    let expected: Vec<&str> = reference
        .trim_end_matches('\n')
        .lines()
        .map(str::trim_end)
        .collect();
    let expected: Vec<&str> = {
        let mut rows = expected;
        while rows.last().is_some_and(|row| row.is_empty()) {
            rows.pop();
        }
        rows
    };
    let actual: Vec<&str> = banner.rows.iter().map(|row| row.trim_end()).collect();
    assert_eq!(actual, expected, "\nactual:\n{}", banner.rows.join("\n"));
    let width = banner.rows[0].chars().count();
    for row in &banner.rows {
        assert_eq!(row.chars().count(), width, "rows must be equal length");
    }
    assert_eq!(banner.width(), width);
}

/// `figlet -f Small mdmost`.
#[test]
fn matches_figlet_on_the_project_name() {
    let banner = layout("mdmost", 80).expect("mdmost fits in 80 columns");
    assert_art(
        &banner,
        concat!(
            "          _              _   \n",
            r" _ __  __| |_ __  ___ __| |_",
            "\n",
            r"| '  \/ _` | '  \/ _ (_-<  _|",
            "\n",
            r"|_|_|_\__,_|_|_|_\___/__/\__|",
            "\n",
        ),
    );
}

/// `figlet -f Small 'Wq {A_1}!'` — the case that exercises every smushing rule the
/// font asks for: equal characters, an underscore giving way to a border, the
/// hierarchy between `|` and `/`, and an opposite pair.
#[test]
fn matches_figlet_on_punctuation_and_smushing() {
    let banner = layout("Wq {A_1}!", 80).expect("fits in 80 columns");
    assert_art(
        &banner,
        concat!(
            r"__      __        __ _     ___   _ ",
            "\n",
            r"\ \    / /_ _    / //_\   / \ \ | |",
            "\n",
            r" \ \/\/ / _` | _| |/ _ \  | || ||_|",
            "\n",
            r"  \_/\_/\__, |  | /_/ \_\_|_|| |(_)",
            "\n",
            r"           |_|   \_\   |___|/_/    ",
            "\n",
        ),
    );
}

/// Every printable ASCII character has art, and none of it is empty: a title made of
/// them all must lay out rather than falling over one missing entry.
#[test]
fn the_font_covers_printable_ascii_and_nothing_else() {
    for code in 0x20u8..=0x7E {
        let ch = char::from(code);
        let art = glyph(ch).unwrap_or_else(|| panic!("no art for {ch:?}"));
        assert_eq!(art.len(), HEIGHT);
        let width = art[0].chars().count();
        assert!(width > 0, "{ch:?} has no columns");
        for row in art {
            assert_eq!(row.chars().count(), width, "{ch:?} has ragged rows");
        }
    }
    assert!(glyph('\u{1F}').is_none());
    assert!(glyph('\u{7F}').is_none());
    assert!(glyph('é').is_none());
}

/// A title the font cannot draw is declined, not approximated. This is the CJK and
/// emoji case, and it is why the fallback path has to exist.
#[test]
fn declines_a_title_it_cannot_draw() {
    assert!(layout("Über uns", 200).is_none());
    assert!(layout("設計ノート", 200).is_none());
    assert!(layout("Release 1.0 🎉", 200).is_none());
    assert!(layout("", 200).is_none());
    assert!(layout("   ", 200).is_none());
}

/// The banner is never truncated to fit: it is declined, and the caller draws an
/// ordinary heading. A 40-column terminal is the case that matters.
#[test]
fn declines_a_banner_wider_than_the_budget() {
    let wide = "A Title Too Long For A Narrow Pane";
    assert!(layout(wide, 40).is_none());
    assert!(layout("mdmost", 29).is_some(), "exactly 29 columns fits");
    assert!(layout("mdmost", 28).is_none(), "one column short declines");
    assert!(layout("mdmost", 0).is_none());
}

/// The rows carry no blank line at either edge, so the banner occupies exactly the
/// space its art needs.
#[test]
fn the_edges_of_the_art_are_never_blank() {
    for title in ["mdmost", "Design", "Q", "!", "___", "no descenders"] {
        let banner = layout(title, 200).unwrap_or_else(|| panic!("{title} must lay out"));
        assert!(!banner.rows.is_empty());
        assert!(
            banner
                .rows
                .first()
                .is_some_and(|row| !row.trim().is_empty()),
            "{title}: first row is blank"
        );
        assert!(
            banner.rows.last().is_some_and(|row| !row.trim().is_empty()),
            "{title}: last row is blank"
        );
    }
}

/// Every character of the title claims a column range inside the art, in order. This
/// is what a search highlight is drawn from, so a letter claiming the wrong columns
/// would light up the wrong part of the banner.
#[test]
fn every_letter_claims_columns_inside_the_art() {
    let banner = layout("mdmost", 80).expect("lays out");
    let width = banner.width();
    assert_eq!(banner.letters.len(), 6);
    for (index, letter) in banner.letters.iter().enumerate() {
        assert_eq!(letter.index, index);
        assert!(letter.cols > 0, "letter {index} claims no columns");
        assert!(
            usize::from(letter.col) + usize::from(letter.cols) <= width,
            "letter {index} runs past the art"
        );
    }
    for pair in banner.letters.windows(2) {
        assert!(pair[0].col <= pair[1].col, "letters are out of order");
    }
}

/// The walk that records source offsets must produce exactly the text
/// [`Node::plain_text`] does, because the layout is done on one and the offsets are
/// indexed by the other.
#[test]
fn source_offsets_agree_with_the_plain_text() {
    let doc = Doc::parse("# The *quick* `brown` fox\n");
    let heading = &doc.root().children[0];
    let (text, origins) = text_with_origins(heading);
    assert_eq!(text, heading.plain_text());
    assert_eq!(origins.len(), text.chars().count());
    // The first character of the title is `T`, and the source really says `T` there.
    let first = origins[0].expect("plain text keeps its offset");
    assert_eq!(&doc.source()[first..first + 1], "T");
    let source = doc.source();
    for (offset, ch) in origins.iter().zip(text.chars()) {
        let Some(start) = offset else { continue };
        assert_eq!(
            source[*start..*start + ch.len_utf8()].chars().next(),
            Some(ch),
            "the recorded offset does not point at {ch:?}"
        );
    }
}
