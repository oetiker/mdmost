// SPDX-License-Identifier: MIT
//! The Unicode superscript and subscript forms, and the rule for using them.
//!
//! Design spec §5.1. Inline math has one row, so a script is written by substituting
//! raised or lowered characters — but Unicode's coverage is incomplete, and the gaps
//! are not where anyone would guess: there is no superscript `q`, and subscripts exist
//! for only seventeen letters.
//!
//! **A script group is substituted only if every character in it has a form.**
//! Otherwise the caller writes `^` or `_` and the group is left as it was. Substituting
//! what fits and leaving the rest would produce `a_b c` for `a_{bc}` and `x²q` for
//! `x^{2q}`, which read as different expressions. A rendering that can be read wrongly
//! is worse than one that is plainly not typeset.
//!
//! This is not a font survey and must never become one. Whether the reader's font has a
//! glyph for `ᵢ` is not knowable from inside this process; what the test below asserts
//! is that the codepoint exists and that Unicode says it is one column wide, which is
//! what the layout depends on. The same rule, and the same reasoning, as
//! `render::glyphs`.

/// Superscript forms, as `(plain, raised)`.
///
/// Latin-1 supplies `¹²³`; U+2070..U+207F supplies the rest of the digits, the
/// operators and most lowercase letters. `q` is genuinely absent from Unicode and must
/// not be faked with a lookalike.
const SUPERSCRIPTS: &[(char, char)] = &[
    ('0', '⁰'),
    ('1', '¹'),
    ('2', '²'),
    ('3', '³'),
    ('4', '⁴'),
    ('5', '⁵'),
    ('6', '⁶'),
    ('7', '⁷'),
    ('8', '⁸'),
    ('9', '⁹'),
    ('+', '⁺'),
    ('-', '⁻'),
    // `pulldown-latex` resolves math-mode `-` to U+2212 MINUS SIGN, not ASCII `-`
    // (`Content::BinaryOp { content: '−' }`), so a script table keyed on ASCII alone
    // never matches a negative exponent -- `x^{-1}` needs this entry as much as the
    // one above, or it silently declines instead of raising to `x⁻¹`.
    ('\u{2212}', '⁻'),
    ('=', '⁼'),
    ('(', '⁽'),
    (')', '⁾'),
    ('a', 'ᵃ'),
    ('b', 'ᵇ'),
    ('c', 'ᶜ'),
    ('d', 'ᵈ'),
    ('e', 'ᵉ'),
    ('f', 'ᶠ'),
    ('g', 'ᵍ'),
    ('h', 'ʰ'),
    ('i', 'ⁱ'),
    ('j', 'ʲ'),
    ('k', 'ᵏ'),
    ('l', 'ˡ'),
    ('m', 'ᵐ'),
    ('n', 'ⁿ'),
    ('o', 'ᵒ'),
    ('p', 'ᵖ'),
    ('r', 'ʳ'),
    ('s', 'ˢ'),
    ('t', 'ᵗ'),
    ('u', 'ᵘ'),
    ('v', 'ᵛ'),
    ('w', 'ʷ'),
    ('x', 'ˣ'),
    ('y', 'ʸ'),
    ('z', 'ᶻ'),
];

/// Subscript forms, as `(plain, lowered)`.
///
/// U+2080..U+208E and a few scattered elsewhere. Seventeen letters only — there is no
/// subscript `b`, `c`, `d`, `f`, `g`, `q`, `w`, `y` or `z`, which is why the
/// all-or-nothing rule earns its keep far more often here than above.
const SUBSCRIPTS: &[(char, char)] = &[
    ('0', '₀'),
    ('1', '₁'),
    ('2', '₂'),
    ('3', '₃'),
    ('4', '₄'),
    ('5', '₅'),
    ('6', '₆'),
    ('7', '₇'),
    ('8', '₈'),
    ('9', '₉'),
    ('+', '₊'),
    ('-', '₋'),
    // See the matching comment in `SUPERSCRIPTS`: `pulldown-latex` resolves math-mode
    // `-` to U+2212, not ASCII `-`, so `x_{-1}` needs this entry to reach `x₋₁`.
    ('\u{2212}', '₋'),
    ('=', '₌'),
    ('(', '₍'),
    (')', '₎'),
    ('a', 'ₐ'),
    ('e', 'ₑ'),
    ('h', 'ₕ'),
    ('i', 'ᵢ'),
    ('j', 'ⱼ'),
    ('k', 'ₖ'),
    ('l', 'ₗ'),
    ('m', 'ₘ'),
    ('n', 'ₙ'),
    ('o', 'ₒ'),
    ('p', 'ₚ'),
    ('r', 'ᵣ'),
    ('s', 'ₛ'),
    ('t', 'ₜ'),
    ('u', 'ᵤ'),
    ('v', 'ᵥ'),
    ('x', 'ₓ'),
];

/// `text` raised, or `None` if any character has no superscript form.
pub(crate) fn superscript(text: &str) -> Option<String> {
    substitute(text, SUPERSCRIPTS)
}

/// `text` lowered, or `None` if any character has no subscript form.
pub(crate) fn subscript(text: &str) -> Option<String> {
    substitute(text, SUBSCRIPTS)
}

/// All of `text` through `table`, or nothing.
///
/// An empty group declines, so that `x^{}` writes nothing raised rather than nothing at
/// all: the caller falls back and the braces stay visible, which is the honest answer
/// for a formula that asked for an empty script.
fn substitute(text: &str, table: &[(char, char)]) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    text.chars()
        .map(|ch| {
            table
                .iter()
                .find(|(plain, _)| *plain == ch)
                .map(|(_, sub)| *sub)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_whose_characters_all_exist_is_raised() {
        assert_eq!(superscript("2").as_deref(), Some("²"));
        assert_eq!(superscript("n+1").as_deref(), Some("ⁿ⁺¹"));
        assert_eq!(subscript("i").as_deref(), Some("ᵢ"));
        assert_eq!(subscript("ij").as_deref(), Some("ᵢⱼ"));
    }

    #[test]
    fn a_group_with_one_missing_character_declines_whole() {
        // No subscript b or c exists, so `bc` must not become `_b c` or `ᵦc`.
        assert_eq!(subscript("bc"), None);
        assert_eq!(subscript("b"), None);
        // No superscript q exists.
        assert_eq!(superscript("q"), None);
        assert_eq!(superscript("2q"), None);
    }

    #[test]
    fn an_empty_group_declines() {
        assert_eq!(superscript(""), None);
        assert_eq!(subscript(""), None);
    }

    #[test]
    fn every_substitute_is_one_column_and_distinct_from_its_plain_form() {
        // `crate::text::display_width` is this crate's one home for column arithmetic
        // (see its module doc); calling `unicode_width` directly here would be the only
        // exception in the tree.
        for (name, table) in [("superscript", SUPERSCRIPTS), ("subscript", SUBSCRIPTS)] {
            for (plain, sub) in table {
                assert_eq!(
                    crate::text::display_width(&sub.to_string()),
                    1,
                    "{name} for {plain:?} is U+{:04X}, which is not one column wide; \
                     the layout counts columns and would drift",
                    *sub as u32
                );
                assert_ne!(plain, sub, "{name} for {plain:?} substitutes itself");
            }
        }
    }

    #[test]
    fn no_plain_character_has_two_forms_in_one_table() {
        for (name, table) in [("superscript", SUPERSCRIPTS), ("subscript", SUBSCRIPTS)] {
            let mut seen: Vec<char> = table.iter().map(|(plain, _)| *plain).collect();
            seen.sort_unstable();
            let before = seen.len();
            seen.dedup();
            assert_eq!(before, seen.len(), "{name} table has a duplicate entry");
        }
    }
}
