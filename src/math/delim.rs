// SPDX-License-Identifier: MIT
//! Delimiters sized to what they enclose (design spec §6.4).
//!
//! Box drawing, not the Unicode bracket-piece block (U+239B–U+23AD). The block is the one
//! designed for the job and it is thinner on the ground in terminal fonts than box drawing
//! is; these are the characters the frames, tables and quote bars already draw with, so
//! they inherit coverage that is already proven on the reader's terminal.
//!
//! A one-row body takes the plain character: `(x)`, never `╭x╮`.
//!
//! **A delimiter is never replaced by a different one** (owner's ruling, 2026-08-24).
//! Box art is a delimiter's own tall form — `(` grows into `╭ │ ╰`, `|` into `│` and `‖`
//! into `║`, the way a letter grows into a larger size — and a delimiter with no designed
//! tall form repeats itself rather than borrowing another's. Drawing `│` where the author
//! wrote `⌊` would put different mathematics on the reader's terminal without telling them.
//!
//! That ruling is why [`pieces`] returns `char` and not `&'static str`. A `&'static str`
//! cannot carry a character the table does not list, so it forces either a substituting
//! catch-all or a `String` per row; a `char` carries every delimiter at no cost, because
//! every piece here — box art and author's character alike — is exactly one `char` of
//! exactly one column. Measured, not assumed: all 40 characters `pulldown-latex` can
//! deliver as a delimiter are one column wide, and `every_reachable_delimiter_is_exactly_
//! one_column` below pins it.

/// The glyph for each row of `delimiter` at `height`, top to bottom.
///
/// A delimiter with no box-art form repeats itself. That is honest where inventing a
/// shape would not be: `⟨` drawn as a made-up stack of pieces is a character the reader
/// has never seen standing for one they have.
pub(crate) fn pieces(delimiter: char, height: u16) -> Vec<char> {
    // A zero-height box does not exist -- `MathBox::height` is `above + below + 1` -- so
    // this only guards a caller that computed a height rather than asking a box for one.
    let height = usize::from(height.max(1));
    if height == 1 {
        return vec![delimiter];
    }
    let Some((top, middle, bottom)) = art(delimiter) else {
        return vec![delimiter; height];
    };
    let mut out = Vec::with_capacity(height);
    out.push(top);
    // At height 2 there is no middle at all: a top and a bottom and nothing between.
    out.extend(std::iter::repeat_n(middle, height.saturating_sub(2)));
    out.push(bottom);
    out
}

/// The three pieces of a growable delimiter, or `None` for one with no box-art form.
///
/// There is no "which side" parameter, and none is possible to want: the *character* says
/// which end it is, which is why `{` and `}` are separate arms rather than one arm and a
/// side. Measured over the closed set of 40 before the parameter was dropped — not one of
/// them is side-dependent.
const fn art(delimiter: char) -> Option<(char, char, char)> {
    match delimiter {
        '(' => Some(('╭', '│', '╰')),
        ')' => Some(('╮', '│', '╯')),
        '[' => Some(('┌', '│', '└')),
        ']' => Some(('┐', '│', '┘')),
        // The middle piece is the one design spec §6.4's `cases` illustration draws.
        '{' => Some(('╭', '┤', '╰')),
        '}' => Some(('╮', '├', '╯')),
        '|' => Some(('│', '│', '│')),
        // Owner's ruling, 2026-08-24, and exactly the parallel of `|` growing into `│`:
        // three stacked `‖` read as three norms where one joined double rule reads as one.
        '‖' => Some(('║', '║', '║')),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::pieces;

    /// Every character `pulldown-latex` can deliver as a `Grouping::LeftRight` delimiter.
    ///
    /// The union of its two delimiter tables, `parser/tables.rs:110`
    /// (`char_delimiter_map`) and `parser/tables.rs:126`
    /// (`control_sequence_delimiter_map`). `parser/lex.rs:145`'s `delimiter` is the only
    /// way a `\left`/`\right` delimiter is read and it goes through both, so this list is
    /// closed: a delimiter outside it is a parse error, not a surprise character.
    const REACHABLE: &str = "()[]{}|/\\‖↑⇑↓⇓↕⇕⌈⌉⌊⌋⎰⎱┌┐└┘⟦⟧⟨⟩⟪⟫⟮⟯⦃⦄⦇⦈⦉⦊";

    #[test]
    fn a_one_row_delimiter_is_the_plain_character() {
        assert_eq!(pieces('(', 1), vec!['(']);
        assert_eq!(pieces(')', 1), vec![')']);
        // A delimiter with no box art takes the same path, and must not be confused with
        // the repeat-itself path below: at height 1 there is nothing to repeat.
        assert_eq!(pieces('⌊', 1), vec!['⌊']);
    }

    #[test]
    fn a_round_bracket_grows_light_arcs() {
        assert_eq!(pieces('(', 3), vec!['╭', '│', '╰']);
        assert_eq!(pieces(')', 3), vec!['╮', '│', '╯']);
    }

    #[test]
    fn a_square_bracket_grows_square_corners() {
        assert_eq!(pieces('[', 3), vec!['┌', '│', '└']);
        assert_eq!(pieces(']', 3), vec!['┐', '│', '┘']);
    }

    #[test]
    fn a_brace_grows_the_middle_of_the_cases_illustration() {
        assert_eq!(pieces('{', 3), vec!['╭', '┤', '╰']);
        assert_eq!(pieces('}', 3), vec!['╮', '├', '╯']);
    }

    #[test]
    fn a_vertical_bar_is_the_same_glyph_all_the_way_down() {
        assert_eq!(pieces('|', 4), vec!['│', '│', '│', '│']);
    }

    #[test]
    fn a_tall_stack_repeats_the_middle() {
        assert_eq!(pieces('(', 5), vec!['╭', '│', '│', '│', '╰']);
    }

    #[test]
    fn a_two_row_delimiter_is_a_top_and_a_bottom_with_no_middle() {
        // The boundary of `height - 2`. Every other growth assert here has at least one
        // middle row, so only this one tells a saturating subtraction from a wrapping
        // one: at height 2 the count of middles is zero, not `usize::MAX`.
        assert_eq!(pieces('(', 2), vec!['╭', '╰']);
        assert_eq!(pieces('{', 2), vec!['╭', '╰']);
    }

    #[test]
    fn a_delimiter_with_no_box_art_repeats_itself() {
        // The owner's ruling of 2026-08-24, and the reason this returns `char`. Each of
        // these reaches the `None` arm of `art`, and each must come back as ITSELF -- a
        // `│` here would be a floor bracket silently rendered as a bar.
        assert_eq!(pieces('⟨', 3), vec!['⟨', '⟨', '⟨']);
        assert_eq!(pieces('⌊', 3), vec!['⌊', '⌊', '⌊']);
        assert_eq!(pieces('⌉', 3), vec!['⌉', '⌉', '⌉']);
        assert_eq!(pieces('⎰', 2), vec!['⎰', '⎰']);
        assert_eq!(pieces('⇕', 4), vec!['⇕', '⇕', '⇕', '⇕']);
    }

    #[test]
    fn no_reachable_delimiter_is_replaced_by_a_different_character() {
        // The ruling stated over the whole reachable set rather than over the handful of
        // examples above: every row of every delimiter is either that delimiter itself or
        // a piece of ITS OWN box art. Nothing else may appear.
        for delimiter in REACHABLE.chars() {
            let art = super::art(delimiter);
            for piece in pieces(delimiter, 5) {
                let allowed = art.is_some_and(|(t, m, b)| piece == t || piece == m || piece == b);
                assert!(
                    piece == delimiter || allowed,
                    "{delimiter:?} drew {piece:?}, which is neither itself nor its own box art"
                );
            }
        }
    }

    #[test]
    fn a_double_bar_grows_into_the_double_box_rule() {
        // Owner's ruling, 2026-08-24. `‖` is the one delimiter that used to reach the
        // repeat-itself path and no longer does, so BOTH cases are pinned here: one row
        // is still the author's plain `‖`, and only a taller body joins it into `║`.
        // Asserting the tall case alone would leave `╭`-style box art on a one-row fence
        // free to creep in, which is the thing `\left\|x\right\|` must never become.
        assert_eq!(pieces('‖', 1), vec!['‖']);
        assert_eq!(pieces('‖', 2), vec!['║', '║']);
        assert_eq!(pieces('‖', 4), vec!['║', '║', '║', '║']);
        // And the parallel it was ruled from: `|` behaves the same way, one row plain and
        // taller joined. Treating the two by different rules is the difference a reader
        // cannot account for.
        assert_eq!(pieces('|', 1), vec!['|']);
        assert_eq!(pieces('|', 2), vec!['│', '│']);
    }

    #[test]
    fn a_height_below_one_still_yields_one_row() {
        // `pieces` takes a height rather than a box, so a caller can hand it a computed
        // zero. One row is the floor: an empty `Vec` would draw no delimiter at all.
        assert_eq!(pieces('(', 0), vec!['(']);
        assert_eq!(pieces('⌊', 0), vec!['⌊']);
    }

    #[test]
    fn every_piece_is_exactly_one_column() {
        for c in ['(', ')', '[', ']', '{', '}', '|', '⟨', '‖'] {
            for piece in pieces(c, 4) {
                assert_eq!(
                    crate::text::display_width(piece.encode_utf8(&mut [0u8; 4])),
                    1,
                    "{piece:?} is not one column, which would break the box arithmetic"
                );
            }
        }
    }

    #[test]
    fn every_reachable_delimiter_is_exactly_one_column() {
        // `boxes::fenced` charges one column per side (`per_side` is 1 or 2, the second
        // column being the padding), so a two-column delimiter would overrun the width
        // the box reserved and `Canvas::check_invariants` would fail on the drawn row.
        // The claim is over the whole reachable set, not over the nine the plan listed.
        assert_eq!(
            REACHABLE.chars().count(),
            40,
            "the closed set is 40 characters"
        );
        for delimiter in REACHABLE.chars() {
            // Height 4 so the box-art path is taken where there is one: `║` is reachable
            // only through a tall `‖`, and this is what measures it.
            for piece in pieces(delimiter, 4) {
                assert_eq!(
                    crate::text::display_width(piece.encode_utf8(&mut [0u8; 4])),
                    1,
                    "{piece:?}, drawn for {delimiter:?}, is not one column"
                );
            }
        }
    }

    #[test]
    fn a_delimiter_draws_exactly_as_many_rows_as_it_was_asked_for() {
        // The box art path and the repeat path build the vector differently -- one pushes
        // a top, a run and a bottom, the other fills -- so the length is asserted over
        // both rather than over whichever one a single example happened to take.
        for delimiter in REACHABLE.chars() {
            for height in 1..8u16 {
                assert_eq!(
                    pieces(delimiter, height).len(),
                    usize::from(height),
                    "{delimiter:?} at height {height}"
                );
            }
        }
    }
}
