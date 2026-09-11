// SPDX-License-Identifier: MIT
use proptest::{prop_assert, prop_assert_eq};

use super::{render_display, render_inline, symbols};

fn rendered(src: &str) -> String {
    render_inline(src).unwrap_or_else(|err| panic!("{src:?} failed: {err}"))
}

#[test]
fn symbols_reports_the_documents_characters_and_not_the_crates_own() {
    // Design spec §13, and the half `tests/glyph_inventory.rs` cannot check on its own: it
    // *subtracts* this from what was drawn, so a `symbols` that under-reports only makes
    // the crate claim more glyphs than it should, and a fixture of ASCII formulas would
    // never notice. Both directions are asserted here instead.
    //
    // Reported, because the author named them: `\alpha` and `\times` are the document's
    // characters, asked for by name and resolved by `pulldown-latex`.
    assert_eq!(symbols(r"a \times \alpha").expect("parses"), "a×α");
    // Not reported, because this crate composed them: design spec §5.2's slash and radical
    // sign and §5.1's raised digit are mdmost's answer to a construct, not anything the
    // document asked for by name, so `glyph_inventory` must go on claiming them.
    //
    // The denominator's brackets arrived with the RULING of 2026-08-21 (owner, Task 6 fix
    // round): a piece that ends in a raised character is no longer one atom, so `b²` is
    // bracketed here where it used to set `(√a)/b²`. That is the price of `{x^2}^2` →
    // `(x²)²` and `\sqrt{x^2}` → `√(x²)`, pinned in `build.rs`'s
    // `a_one_piece_operand_that_draws_a_gap_is_parenthesised_all_the_same`. Nothing about
    // *this* test's subject moves with it: the brackets are the crate's own characters,
    // which is exactly what the line below claims.
    assert_eq!(rendered(r"\frac{\sqrt{a}}{b^2}"), "(√a)/(b²)");
    assert_eq!(symbols(r"\frac{\sqrt{a}}{b^2}").expect("parses"), "ab2");
    // A parse failure is an error here as it is in `render_inline`, not an empty answer:
    // an empty one would silently claim every glyph of a broken formula for this crate.
    assert!(symbols(r"\frac{1}").is_err());
}

#[test]
fn plain_arithmetic_passes_through() {
    assert_eq!(rendered("1 + 2"), "1 + 2");
    assert_eq!(rendered("a = b"), "a = b");
    assert_eq!(rendered("a < b"), "a < b");
    assert_eq!(rendered("a - b"), "a − b");
}

#[test]
fn a_leading_operator_glues_to_what_follows_instead_of_floating() {
    // `pulldown-latex` does not mark a leading `-` as unary; it is the same
    // `Content::BinaryOp` event as the one in `a - b`. The only signal this walk has
    // for "this is a prefix, not an infix" is that it is the first thing written, and
    // that signal has to suppress the space on *both* sides, not just the left one --
    // otherwise `-x` reads `− x` instead of `−x`.
    assert_eq!(rendered("-x"), "−x");
    assert_eq!(rendered("-1"), "−1");
}

#[test]
fn a_trailing_operator_never_leaves_a_stray_space() {
    // `a+` is a truncated formula, not a malformed one -- pulldown-latex parses it as
    // `Ordinary('a')` then `BinaryOp('+')` with nothing after.
    //
    // Stage 1 asserted `a +` here: it wrote the operator's usual pair of spaces and then
    // trimmed the trailing one off the finished string. The engine never writes it. An
    // operator with nothing on its right to bind to is a sign, not an operator, and takes
    // no gap on either side -- the second half of the `TeXbook`'s bin-to-ord rule, decided
    // once in `build.rs`'s unary pass instead of by a trim that could only ever reach the
    // end of the formula. So the left space goes too, which the old trim could not do.
    assert_eq!(rendered("a+"), "a+");
}

#[test]
fn named_symbols_become_their_characters() {
    assert_eq!(rendered(r"\alpha"), "α");
    assert_eq!(rendered(r"\Gamma"), "Γ");
    assert_eq!(rendered(r"\infty"), "∞");
    assert_eq!(rendered(r"a \times b"), "a × b");
    assert_eq!(rendered(r"x \le y"), "x ≤ y");
    assert_eq!(rendered(r"a \to b"), "a → b");
}

#[test]
fn text_mode_keeps_its_spaces() {
    assert_eq!(rendered(r"\text{if and only if}"), "if and only if");
}

#[test]
fn a_parse_failure_is_an_error_and_not_a_panic() {
    let err = render_inline(r"\frac{1}").unwrap_err();
    assert!(
        matches!(err, crate::error::MathError::Parse { .. }),
        "expected a parse error, got {err:?}"
    );
}

#[test]
fn a_parse_error_message_is_only_its_first_line() {
    // Pinned to the exact wording rather than just "contains no newline": if
    // `.lines().next()` were ever removed from `render_inline`, this message would grow
    // to include `ParserError`'s box-drawing context lines (`╭─►`, `│`, `╰─`), and this
    // assertion catches that even though it does not itself scan for those glyphs. The
    // wording comes from a single hard-coded `&'static str` arm in pulldown-latex's
    // `ErrorKind::Token`, which is as stable a string as a dependency offers; if a
    // future patch reword it, this test is meant to fail and be updated, not to pass
    // silently on a message that quietly grew a second line.
    let err = render_inline(r"\frac{1}").unwrap_err();
    let crate::error::MathError::Parse { message } = err else {
        panic!("expected a parse error, got {err:?}");
    };
    assert_eq!(message, "parsing error: expected a token");
    assert!(
        !message.contains('\n'),
        "message grew a second line: {message:?}"
    );
}

#[test]
fn a_function_name_is_written_as_its_word() {
    assert_eq!(rendered(r"\sin"), "sin");
}

#[test]
fn a_function_name_is_separated_from_its_operand() {
    // `sinx` reads as a product of three variables, not the function applied to `x` --
    // this is the defect a real document would hit on day one, not a tightness quibble.
    assert_eq!(rendered(r"\sin x"), "sin x");
    assert_eq!(rendered(r"\log n"), "log n");
}

#[test]
fn a_function_name_at_the_head_of_a_run_still_gets_its_trailing_space() {
    // Unlike a leading `BinaryOp`, a leading `Function` is never unary -- it is a word,
    // and `\sin x` opening a formula must still read `sin x`, not `sinx`.
    assert_eq!(rendered(r"\sin x"), "sin x");
    assert_eq!(rendered("1 + 2"), "1 + 2");
    assert_eq!(rendered("-x"), "−x");
}

#[test]
fn a_function_name_sits_tight_against_an_opening_delimiter() {
    // Chosen over `sin (x)`: real LaTeX sets `\sin(x)` tight too -- the gap after an
    // operator name separates it from its operand, and a delimited group `(x)` is
    // already visibly its own thing without one.
    assert_eq!(rendered(r"\sin(x)"), "sin(x)");
}

#[test]
fn a_two_character_relation_is_written_as_one_spaced_unit() {
    // `RelationContent` can hold two characters, and that is why `Content::Relation`
    // cannot share an or-pattern with the other `char`-only arms. The ones that do are
    // the sixteen `multirelation` calls at
    // `vendor/pulldown-latex/src/parser/primitives.rs:1171-1186`; `\coloneq` is `:` then
    // `−`, which is what this asserts.
    //
    // This comment named `\shortparallel` as an example. It is not one: it is
    // `RelationContent::single_char('∥')` at `primitives.rs:1080`, so the example
    // contradicted the input on the next line.
    assert_eq!(rendered(r"a \coloneq b"), "a :− b");
}

#[test]
fn a_horizontal_space_command_draws_one_column() {
    assert_eq!(rendered(r"a\kern1em b"), "a b");
}

#[test]
fn a_state_change_draws_nothing_of_its_own() {
    // `\mathbf` is `Event::StateChange(StateChange::Font(..))` wrapping its argument in a
    // group. The state change contributes no cell of its own — it moves the state the
    // *next* atom is drawn in — so one letter goes in and one letter comes out.
    //
    // *Which* letter is Task 15b's subject, pinned in the font tests at the foot of this
    // file. Until then this read `x`, because the walk dropped every state change and drew
    // the plain letter; the claim made here was always about the cell count.
    assert_eq!(rendered(r"\mathbf{x}"), "𝐱");
    // A style change draws nothing *and* changes nothing: a terminal has one size, so
    // `\displaystyle` is the state change with no state behind it.
    assert_eq!(rendered(r"\displaystyle x"), "x");
}

#[test]
fn a_script_whose_characters_all_exist_is_raised_or_lowered() {
    assert_eq!(rendered("x^2"), "x²");
    assert_eq!(rendered("E = mc^2"), "E = mc²");
    assert_eq!(rendered("x_i"), "xᵢ");
    assert_eq!(rendered("x^{n+1}"), "xⁿ⁺¹");
    assert_eq!(rendered("a_{ij}"), "aᵢⱼ");
}

#[test]
fn a_script_with_no_unicode_form_is_written_flat() {
    assert_eq!(rendered("x_b"), "x_b");
    assert_eq!(rendered("x^q"), "x^q");
    // Braces are kept where they group more than one character, because `a_bc` would
    // read as `(a_b)c`.
    assert_eq!(rendered("a_{bc}"), "a_{bc}");
}

#[test]
fn a_sub_and_superscript_pair_is_decided_independently() {
    // The subscript can be lowered and the superscript cannot, so one of each notation
    // appears in one expression. That is the honest answer: both halves are readable.
    assert_eq!(rendered("x_i^q"), "xᵢ^q");
}

#[test]
fn a_script_sits_flush_against_a_function_name_base() {
    // The script itself is still flush -- `sin²`, never `sin ²`: a script is not a
    // separate operand, and the engine appends it to the base's own cells with no gap
    // between them at all.
    //
    // What changed is the space after. Stage 1 asserted `sin²x`, because it suppressed a
    // function name's trailing space from inside the script writer, so a scripted `\sin`
    // stopped being a function name for spacing purposes. The engine keeps the base's
    // class -- a scripted `\sin` is still `Function` -- and asks the table once:
    // `gap(Function, Ordinary)` is 1, the same cell that makes `\sin x` read `sin x`.
    // `sin²x` was a carried stage-1 defect, not a decision.
    assert_eq!(rendered(r"\sin^2 x"), "sin² x");
}

#[test]
fn a_function_used_as_a_script_base_still_gets_its_own_leading_space() {
    // A scripted `\sin` is spaced by what its base *is*, not by what happened to it: the
    // engine returns the base's own class from `script_box`, so all four of these are one
    // lookup of `gap(Ordinary, Function)`, which is 1. Stage 1 had to reach this the hard
    // way -- through a head-of-run check that could only see an isolated buffer -- and got
    // it wrong twice before it got it right.
    //
    // The trailing halves (`sin² x`, `sin² y`) were `sin²x` and `sin²y` in stage 1; see
    // `a_script_sits_flush_against_a_function_name_base` for why that was a defect.
    assert_eq!(rendered(r"2\sin x"), "2 sin x");
    assert_eq!(rendered(r"2\sin^2 x"), "2 sin² x");
    assert_eq!(rendered(r"x\sin y"), "x sin y");
    assert_eq!(rendered(r"x\sin^2 y"), "x sin² y");
}

#[test]
fn a_group_used_as_a_script_base_is_spaced_as_the_ordinary_atom_it_is() {
    // Renamed: stage 1 asserted `2 (sin x)²` and called the space the group's "own leading
    // space", carried over from a bug where a `{…}` base could not see what preceded it.
    // The engine has no head-of-run notion to get wrong. A `Begin` is `Class::Ordinary`
    // whatever it contains -- a brace group is an Ord atom, which is what TeX calls it too
    // -- so `2{\sin x}^2` is `gap(Ordinary, Ordinary)`, which is 0. The function name
    // inside the braces is not what the `2` is set against; the group is.
    //
    // The grouping this test exists for is untouched: `(sin x)²`, not `sin x²`, which
    // would read as `sin(x²)`.
    assert_eq!(rendered(r"{\sin x}^2"), "(sin x)²");
    assert_eq!(rendered(r"2{\sin x}^2"), "2(sin x)²");
}

#[test]
fn a_multi_atom_brace_group_used_as_a_script_base_keeps_its_grouping() {
    // A script applies to the whole `{…}` base, not to whichever atom happened to be
    // written last -- `2{ab}^2` must read `2(ab)²` (`2·(ab)²`), not `2ab²` (which reads
    // as `2·a·b²`). A single-atom base needs no visual grouping, the same exemption
    // `bracketed()` gives a fraction or radical operand: `{x}^2` stays `x²`.
    assert_eq!(rendered(r"2{ab}^2"), "2(ab)²");
    assert_eq!(rendered(r"2{ab}_2"), "2(ab)₂");
    assert_eq!(rendered(r"{x}^2"), "x²");
}

#[test]
fn a_group_used_as_a_script_base_brackets_by_the_pieces_it_holds() {
    // Renamed. Stage 1's name described a bug in a walk that no longer exists: it wrote
    // spaces into a buffer and then bracketed the buffer, so a big operator's own trailing
    // space could be sealed inside the parentheses and make a one-character base count as
    // two. The engine brackets a *box*, and a box has pieces rather than characters, so
    // there is no trailing space to seal and no ordering to get wrong.
    //
    // These three are unchanged; they are here because they are the case the piece count
    // and the character count agree on, and one that disagrees follows each way below.
    assert_eq!(rendered(r"{\sum}^2"), "∑²");
    assert_eq!(rendered(r"{\prod}^2"), "∏²");
    assert_eq!(rendered(r"{\int}^2"), "∫²");
    // `{\sin}` is one piece where stage 1 counted three characters and wrote `(sin)²`.
    // `sin²` is the conventional form and the piece count is the rule design spec §5.2
    // means by "a single atom": one thing set against another, not one column.
    assert_eq!(rendered(r"{\sin}^2"), "sin²");
    // RULED 2026-08-21, Task 6. `2\log_2` sets `2 log₂` and `2{\log}_2` sets `2log₂`, and
    // the braces are the whole difference: a group's class is `Ordinary` where a function
    // name's is `Function`, and `gap(Ordinary, Ordinary)` is 0. Kept as it stands, for
    // three reasons. It is what TeX does -- `{…}` is an Ord atom, and the author who wrote
    // the braces asked for exactly that. Making a group take its content's class instead
    // would stop a group being transparent to spacing, which `{a}+{b}` -> `a + b` depends
    // on, and would give `{\sum}x` a large operator's gap. And the author who wants
    // `2 log₂` writes `2\log_2`, which is the ordinary way to write it. The cost is that
    // `2log₂` can be read as one identifier; the braces are what asked for that reading.
    assert_eq!(rendered(r"2{\log}_2"), "2log₂");
    // The other direction: two pieces, so the brackets stay. The `+` loses its spaces
    // because a brace group bounds a run and its trailing `Bin` is demoted inside it --
    // the same rule as `a+` -> `a+`.
    assert_eq!(rendered(r"{a+}^2"), "(a+)²");
}

#[test]
fn an_empty_brace_group_used_as_a_script_base_brackets_an_empty_body() {
    // Degenerate LaTeX -- an empty group has nothing to be misread as a bigger
    // expression -- but it is worth pinning rather than guarding against: an empty
    // base is zero characters, not one, so `bracketed()`'s single-atom exemption
    // does not apply to it either way, and drawing `()` is an honest, consistent
    // answer for "a group, and it was empty" rather than a special case earning its
    // own branch.
    assert_eq!(rendered(r"{}^2"), "()²");
}

#[test]
fn a_nested_script_composes_without_panicking() {
    // The inner `y^2` is raised on its own (`y²`), but that result contains `²`, which
    // has no superscript form of its own -- so the outer raise declines and falls back
    // to flat notation, keeping the inner substitution rather than discarding it.
    assert_eq!(rendered("x^{y^2}"), "x^{y²}");
}

#[test]
fn an_empty_script_group_declines_and_writes_the_bare_marker() {
    // `superscript("")` and `subscript("")` both decline (Task 1), so the flat fallback
    // runs on empty text; a zero-character group never triggers the multi-character
    // brace rule, so the marker is written alone.
    assert_eq!(rendered("x^{}"), "x^");
    assert_eq!(rendered("x_{}"), "x_");
}

#[test]
fn a_plain_big_operator_with_no_script_still_gets_its_trailing_space() {
    assert_eq!(rendered(r"\sum x"), "∑ x");
}

#[test]
fn a_script_attached_to_a_big_operator_still_gets_its_trailing_space() {
    // A big operator takes one space after it and after its limits, so `\sum_{i=1}^{n} i`
    // reads `∑ᵢ₌₁ⁿ i` and not `∑ᵢ₌₁ⁿi`. The author wrote a space there and
    // `pulldown-latex` discards literal whitespace in math mode, so this walk is the
    // only thing that can put one back.
    assert_eq!(rendered(r"\sum_{i=1}^{n} i"), "∑ᵢ₌₁ⁿ i");
}

#[test]
fn a_fraction_is_written_with_a_slash() {
    assert_eq!(rendered(r"\frac{a}{b}"), "a/b");
    // Parenthesised when a part is more than one atom, because a + b/c is a different
    // expression from (a + b)/c. A fraction operand keeps its spaces: it is written at
    // full size and nothing is going to raise it.
    assert_eq!(rendered(r"\frac{a+b}{c}"), "(a + b)/c");
    assert_eq!(rendered(r"\frac{1}{2a}"), "1/(2a)");
}

#[test]
fn a_root_takes_the_radical_sign() {
    assert_eq!(rendered(r"\sqrt{x}"), "√x");
    assert_eq!(rendered(r"\sqrt{b^2-4ac}"), "√(b² − 4ac)");
    // The degree comes second in the event stream, not first. Getting the two operands
    // the wrong way round renders this `ˣ√3`, which no test above would notice.
    assert_eq!(rendered(r"\sqrt[3]{x}"), "³√x");
}

#[test]
fn a_big_operator_keeps_its_limits_as_scripts() {
    assert_eq!(rendered(r"\sum_{i=1}^{n} i"), "∑ᵢ₌₁ⁿ i");
    assert_eq!(rendered(r"\int_0^1 f"), "∫₀¹ f");
}

#[test]
fn a_matrix_declines_rather_than_being_flattened() {
    // No alignment mark and no row break, so what declines is the *matrix* — the thing
    // spec §5.2 says is not representable in one row — and not the `&` that a wider
    // fixture would have tripped over first.
    let err = render_inline(r"\begin{pmatrix} 1 \end{pmatrix}").unwrap_err();
    assert_eq!(err, crate::error::MathError::NotDrawable("a matrix"));
}

#[test]
fn a_radical_or_fraction_may_be_a_script_base() {
    // Renamed and inverted, which is the change this stage exists to make. Stage 1's
    // one-row walk had no notion of a box, so a fraction or a radical reaching a script
    // base was a shape it could not carry and it declined by name. In one engine a base is
    // just a box: the fraction has already rewritten itself to `a/b` on this same row, and
    // a script goes on it like any other.
    //
    // Design spec §5.2 brackets the base, and it has to: `√x` is two atoms, and `√x²`
    // would read as `√(x²)`, a different number.
    assert_eq!(rendered(r"\frac{a}{b}^2"), "(a/b)²");
    assert_eq!(rendered(r"\sqrt{x}^2"), "(√x)²");
}

#[test]
fn a_leading_unary_minus_binds_tight_to_a_function_name() {
    // Renamed, because the space it was named for is gone. Stage 1 wrote `− sin x`:
    // `spaced` suppressed both sides of an operator at the head of a run and `spaced_word`
    // only the leading one, and neither knew about the other, so a sign in front of a
    // function name kept a space that `-x` -> `−x` did not. Correct, merely loose, and
    // pinned then so that this rewrite would change a test rather than drift.
    //
    // The spacing table has no head-of-run rule to disagree with. The `−` is demoted to
    // `Class::Unary` by the bin-to-ord pass because it has nothing on its left, and then
    // it is one lookup: `gap(Unary, Function)` is 0.
    assert_eq!(rendered(r"-\sin x"), "−sin x");
}

#[test]
fn a_formula_may_begin_or_end_with_a_column_and_keeps_it() {
    // RULED 2026-08-21, Task 6. `render_inline` returns the row as the engine built it and
    // trims nothing. Stage 1 trimmed the trailing end, and could not have done the leading
    // one -- it had no leading space to trim, because its head-of-run rule suppressed the
    // space instead of writing it.
    //
    // Both of these are faithful to TeX and neither is a defect to be tidied away. `\,` is
    // a thin space the author asked for, and it is the first thing in the run, so the run
    // starts with a column. `{}-x` is the classic idiom for keeping a minus binary: an
    // empty group is a zero-width `Ordinary` piece, so the `−` has a left operand, is not
    // demoted to a sign, and keeps the spaces an operator gets.
    //
    // Trimming would put a spacing decision outside `spacing.rs`, which is the one place
    // this crate decides spacing, and it would make `render_inline` disagree with
    // `draw::to_row` over the same box -- one engine, split in two again by the back door.
    // A caller that cannot take a leading column is the caller that should trim.
    assert_eq!(rendered(r"\,x"), " x");
    assert_eq!(rendered(r"{}-x"), " − x");
    assert_eq!(rendered(r"a\,"), "a ");
    // And the boundary: without the empty group the `−` is a sign and takes no space at
    // all, which is the difference the idiom exists to make.
    assert_eq!(rendered(r"-x"), "−x");
}

#[test]
fn a_root_index_with_no_raised_form_declines_rather_than_writing_a_caret() {
    // `\sqrt[3]{x}` draws (`a_root_takes_the_radical_sign`). An index Unicode cannot raise
    // does not, and stage 1's answer here was wrong rather than merely worse: it reached
    // for the same `^` fallback a script uses and wrote `^α√x`, which is not a root with
    // an index written plainly, it is nonsense. There is no caret notation for a root
    // index. So the root declines and design spec §9 shows the source instead.
    //
    // The caption names the index, not the root, because the root is fine -- it is the
    // index that has no form, and a reader who is told "a root with an index" would go
    // looking for the wrong thing after seeing `\sqrt[3]{x}` draw on the line above.
    assert_eq!(
        render_inline(r"\sqrt[\alpha]{x}").unwrap_err(),
        crate::error::MathError::NotDrawable("a root index with no raised form")
    );
    // The boundary, one character either side of it: `q` has no superscript form and `p`
    // has, so these two differ only in whether the table has the letter.
    assert_eq!(
        render_inline(r"\sqrt[q]{x}").unwrap_err(),
        crate::error::MathError::NotDrawable("a root index with no raised form")
    );
    assert_eq!(rendered(r"\sqrt[p]{x}"), "ᵖ√x");
}

/// Strings made of the characters most likely to confuse a LaTeX parser.
///
/// One definition, used by both proptests. It was written inline inside
/// `latex_shaped_noise_never_panics` when it had one caller; the display proptest wants the
/// same vocabulary, and two copies of a word list drift the moment either grows.
fn latex_shaped() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::strategy::Strategy;
    proptest::collection::vec(
        proptest::sample::select(vec![
            "\\frac",
            "\\sqrt",
            "\\sum",
            "{",
            "}",
            "^",
            "_",
            "&",
            "\\\\",
            "\\begin{pmatrix}",
            "\\end{pmatrix}",
            "\\alpha",
            "$",
            "\\",
            // Delimiter-aware vocabulary, added alongside the `\left`/`\right`
            // fix: bare brackets, `\left`/`\right` themselves, an invisible
            // delimiter, and plain letters/digits so a generated string is more
            // often something the parser accepts far enough to reach the new
            // code, rather than failing at the lexer on every case.
            "\\left",
            "\\right",
            "(",
            ")",
            "[",
            "]",
            ".",
            "a",
            "1",
        ]),
        0..20,
    )
    .prop_map(|parts| parts.concat())
}

proptest::proptest! {
    /// Design spec §9: a wrecked formula must never take down a document.
    #[test]
    fn arbitrary_input_never_panics(src in ".{0,200}") {
        let _ = render_inline(&src);
    }

    /// The same, over strings made of the characters most likely to confuse a parser.
    #[test]
    fn latex_shaped_noise_never_panics(src in latex_shaped()) {
        let _ = render_inline(&src);
    }

    /// Design spec §14. The display path does far more arithmetic than the inline one —
    /// baselines, saturating widths, negative row offsets — and every one of those is a
    /// place a formula could take the pager down. It may fail; it may not panic.
    ///
    /// A width of 1 is in the list deliberately: it is the value that exercises
    /// `draw::centre`'s `saturating_sub` and `draw::place`'s `usize::try_from` guards,
    /// because every construct wider than one column is then drawn against a floor it
    /// overruns.
    #[test]
    fn render_display_never_panics_and_always_holds_the_canvas_contract(src in latex_shaped()) {
        let theme = crate::theme::Theme::default();
        for width in [1u16, 8, 40, 200] {
            match render_display(&src, width, &theme) {
                Ok(canvas) => {
                    prop_assert!(canvas.check_invariants().is_ok());
                    // Equality, not the floor `draw::to_canvas` promises. `TooWide` is
                    // returned before the draw, so a box wider than `width` never reaches
                    // `to_canvas` through here and its widening never fires: every canvas
                    // this function returns — the empty one included — is exactly `width`.
                    prop_assert_eq!(canvas.width(), width);
                }
                // The error is not merely an error: `needed` is the answer, so it has to
                // be a width this call could not show.
                Err(crate::error::MathError::TooWide { needed }) => {
                    prop_assert!(needed > width);
                }
                Err(_) => {}
            }
        }
    }
}

#[test]
fn left_right_draws_its_own_delimiter_characters() {
    // `Grouping::LeftRight` carries its delimiter characters in its own two fields
    // (`event.rs:316`), not as separate `Content` events either side, so ignoring the
    // `Begin`/`End` the way `Grouping::Normal` is ignored silently drops them.
    assert_eq!(rendered(r"\left(x\right)"), "(x)");
    assert_eq!(rendered(r"\left(a+b\right)"), "(a + b)");
    assert_eq!(rendered(r"\left[x\right]"), "[x]");
    // Not just a missing character: without the parentheses this is a different
    // expression. `a/b²` parses as `a/(b²)`; `(a/b)²` is what the source asked for.
    assert_eq!(rendered(r"\left(\frac{a}{b}\right)^2"), "(a/b)²");
    // `\left.` is a deliberately invisible delimiter, not a missing one.
    assert_eq!(rendered(r"\left.x\right)"), "x)");
    assert_eq!(rendered(r"\left(x\right."), "(x");
    // Delimiters sit tight against their content -- no space is added on either side
    // beyond what the surrounding run already carries, matching `\sin(x)` -> `sin(x)`.
    assert_eq!(rendered(r"a + \left(b\right)"), "a + (b)");
}

#[test]
fn left_right_still_draws_its_delimiters_as_a_script_base() {
    // `take_base`'s group branch used to strip every group boundary unconditionally,
    // the same bug as `write_one`'s, in a second place: a `\left...\right` used as a
    // script's own base would lose its delimiters just the same.
    assert_eq!(rendered(r"\left(x\right)_0"), "(x)₀");
}

/// A display formula drawn on a canvas at its own width, one string per row, trailing
/// blanks trimmed.
///
/// Deliberately not through [`render_display`], which takes the *caller's* width and would
/// pad every row out to it — these tests assert the art, and a formula's own width is the
/// only width at which the art is exactly the formula. Reaching `build` and `draw` directly
/// is what lets `b.width` be passed as the floor.
fn display(src: &str) -> Vec<String> {
    let storage = pulldown_latex::Storage::new();
    let events = super::build::parse(src, &storage).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    let b = super::build::build(&events, super::build::Mode::Display)
        .unwrap_or_else(|e| panic!("{src:?}: {e}"));
    let theme = crate::theme::Theme::default();
    let canvas = super::draw::to_canvas(&b, b.width, &theme);
    canvas.check_invariants().expect("width holds on every row");
    (0..canvas.height())
        .map(|r| canvas.row_text(r).trim_end().to_string())
        .collect()
}

/// A root index that is a fence draws, instead of leaving the rows it reserved empty.
///
/// Carried from the Task 7 review. Task 7 grew a radical's `above` to hold an index of any
/// size, so a `Fenced` index reserved its rows straight away — but nothing drew into them
/// while `place`'s `Fenced` arm was empty, and both of these came out with two blank rows
/// on top:
///
/// ```text
/// ["", "", "      ─", "    √ x"]
/// ```
///
/// This is the check that the reserve and the drawing agree about height, not merely that
/// something appears: the index fills exactly the rows Task 7 added and no others, so a
/// fence one row taller or shorter than its reserve would show up here as a blank row or
/// a clipped one rather than as a silently wrong-looking root.
#[test]
fn a_fenced_root_index_fills_the_rows_the_radical_reserved_for_it() {
    assert_eq!(
        display(r"\sqrt[\left(\frac{a}{b}\right)]{x}"),
        vec!["╭ a ╮", "│ ─ │", "╰ b ╯ ─", "    √ x"]
    );
    // `\binom` reaches the same arm by a different route -- it is a `\left(…\right)` the
    // author never typed -- so it is asserted rather than assumed to follow.
    //
    // The middle row is blank and the rest of the picture is identical to the `\frac`
    // above it, which is the point: a ruleless fraction measures exactly like a ruled one
    // (RULING, owner, 2026-09-10), so the radical's reserve, the fence's three rows and
    // the column the overline starts in are all unmoved by the rule going away.
    assert_eq!(
        display(r"\sqrt[\binom{n}{k}]{x}"),
        vec!["╭ n ╮", "│   │", "╰ k ╯ ─", "    √ x"]
    );
}

/// A binomial coefficient is not a division: no rule, in display mode.
///
/// RULING, owner, 2026-09-10 (Task 14b). `\binom{n}{k}` shipped from stage 1 drawing the
/// rule of a fraction, because `build.rs` discarded the thickness the parser hands it and
/// every `Visual::Fraction` became one construct. A rule between `n` and `k` says "n
/// divided by k", which is a different number from the one the author wrote.
///
/// The blank row is the baseline, not an absence: it is the row the rule would have been
/// on, and the fenced cases show it at full width rather than trimmed away.
#[test]
fn a_binomial_coefficient_draws_no_rule_between_its_parts() {
    for src in [r"\binom{n}{k}", r"\dbinom{n}{k}", r"\tbinom{n}{k}"] {
        assert_eq!(
            display(src),
            vec!["╭ n ╮", "│   │", "╰ k ╯"],
            "{src} must not draw a division"
        );
    }
    // Unmoved: the fraction the reader has always had.
    assert_eq!(display(r"\frac{a}{b}"), vec!["a", "─", "b"]);
    assert_eq!(display(r"\dfrac{a}{b}"), vec!["a", "─", "b"]);
    // `\genfrac` asks for either from the same command, and the thickness is what decides
    // -- `0pt` is `\atop`, the identical construct without the parentheses. The middle row
    // comes back empty rather than as a space only because `display` trims each row's
    // tail; the fenced cases above are where its full width is visible.
    assert_eq!(display(r"\genfrac{}{}{0pt}{}{a}{b}"), vec!["a", "", "b"]);
    assert_eq!(display(r"\genfrac{}{}{2pt}{}{a}{b}"), vec!["a", "─", "b"]);
}

/// Inline, a ruleless fraction refuses; it does not invent a flat notation.
///
/// RULING, owner, 2026-09-10 (Task 14b), point 2. `(n/k)` is the wrong answer twice over:
/// the slash reads as division, and the parentheses make it look deliberate. There is no
/// conventional one-row notation for a binomial coefficient, and design spec §9's "no
/// symbol table of our own" covers notation as much as it covers glyphs -- so the engine
/// declines and the reader gets the verbatim source with its dollars, which is at least
/// unambiguous.
///
/// The message is the same for `\genfrac{}{}{0pt}{}` because it is the same construct.
#[test]
fn an_inline_binomial_coefficient_refuses_rather_than_setting_a_slash() {
    for src in [
        r"\binom{n}{k}",
        r"\dbinom{n}{k}",
        r"\tbinom{n}{k}",
        r"\genfrac{}{}{0pt}{}{a}{b}",
    ] {
        assert_eq!(
            render_inline(src).expect_err("no one-row form exists"),
            crate::error::MathError::NotDrawable("a binomial coefficient"),
            "{src} has no honest one row"
        );
    }
    // Unmoved: a ruled fraction still sets flat, and it is `a/b` and not `(a/b)` --
    // measured at c9c3e13, not assumed. The brackets appear only where a part needs them.
    assert_eq!(render_inline(r"\frac{a}{b}").expect("renders"), "a/b");
    assert_eq!(
        render_inline(r"\genfrac{}{}{2pt}{}{a}{b}").expect("renders"),
        "a/b"
    );
    // A binomial anywhere inside an inline formula refuses the whole formula, the way a
    // matrix does: there is no partial answer to give.
    assert_eq!(
        render_inline(r"1 + \binom{n}{k}").expect_err("no one-row form exists"),
        crate::error::MathError::NotDrawable("a binomial coefficient")
    );
}

/// The four presence combinations, end to end from LaTeX rather than from a hand-built box.
///
/// `\left.` is the case a builder test cannot reach by hand: it is the source construct
/// that produces `None`, and the whole reason `boxes::fenced` charges per side.
#[test]
fn a_display_fence_draws_only_the_delimiters_the_source_asked_for() {
    assert_eq!(
        display(r"\left(\frac{a}{b}\right)"),
        vec!["╭ a ╮", "│ ─ │", "╰ b ╯"]
    );
    assert_eq!(
        display(r"\left[\frac{a}{b}\right."),
        vec!["┌ a", "│ ─", "└ b"]
    );
    assert_eq!(
        display(r"\left.\frac{a}{b}\right]"),
        vec!["a ┐", "─ │", "b ┘"]
    );
    assert_eq!(display(r"\left.\frac{a}{b}\right."), vec!["a", "─", "b"]);
    // A delimiter with no box-art form, reached from the source that names it. This is
    // the no-substitution ruling stated where the reader meets it: `\lfloor` must not
    // arrive on the page as a bar.
    assert_eq!(
        display(r"\left\lfloor\frac{a}{b}\right\rfloor"),
        vec!["⌊ a ⌋", "⌊ ─ ⌋", "⌊ b ⌋"]
    );
    // `\|` is the one delimiter whose tall form is a DIFFERENT character from its plain
    // one, so both cases are rendered from source rather than one being inferred.
    assert_eq!(
        display(r"\left\|\frac{a}{b}\right\|"),
        vec!["║ a ║", "║ ─ ║", "║ b ║"]
    );
    assert_eq!(display(r"\left\|x\right\|"), vec!["‖x‖"]);
}

#[test]
fn a_display_formula_draws_as_a_canvas() {
    let theme = crate::theme::Theme::default();
    let canvas = render_display(r"\frac{a}{b}", 40, &theme).expect("draws");
    assert_eq!(canvas.height(), 3);
    assert_eq!(canvas.width(), 40);
    canvas.check_invariants().expect("width holds");
}

#[test]
fn a_display_formula_wider_than_the_width_says_what_it_needs() {
    let theme = crate::theme::Theme::default();
    let err = render_display(r"\frac{a+b+c+d+e+f}{2}", 8, &theme).expect_err("too wide");
    let crate::error::MathError::TooWide { needed } = err else {
        panic!("expected TooWide, got {err:?}")
    };
    assert!(needed > 8, "and the number is the answer, not a hint");
}

#[test]
fn a_formula_that_exactly_fits_draws_and_one_column_narrower_does_not() {
    // The boundary, from both sides, because `TooWide` is one comparison and a `>=` there
    // is as easy to write as a `>`. `E = mc` is six columns
    // (`display_and_inline_agree_about_a_flat_formula` renders it), so six fits exactly and
    // five is the first width that cannot show it.
    let theme = crate::theme::Theme::default();
    let canvas = render_display("E = mc", 6, &theme).expect("exactly the width is not too wide");
    assert_eq!(canvas.width(), 6);
    assert_eq!(canvas.row_text(0), "E = mc");
    assert_eq!(
        render_display("E = mc", 5, &theme).expect_err("one column short"),
        crate::error::MathError::TooWide { needed: 6 },
        "and `needed` is the formula's own width, not the width that was asked for"
    );
}

#[test]
fn a_block_that_draws_nothing_draws_nothing() {
    // Design spec §16.3. The rule is stated over the *result*, so all six of these are the
    // same case and none of them is a listed command: a `\newcommand` with and without a
    // parameter count, a `\def` with and without a parameter, a comment and whitespace all
    // lay out to a box with no cells.
    let theme = crate::theme::Theme::default();
    for src in [
        r"\newcommand{\R}{\mathbb{R}}",
        r"\newcommand{\R}[0]{\mathbb{R}}",
        r"\def\R{\mathbb{R}}",
        r"\def\R#1{\mathbb{R}}",
        "% nothing but a comment",
        "   ",
    ] {
        let canvas = render_display(src, 40, &theme).unwrap_or_else(|e| panic!("{src:?}: {e}"));
        assert_eq!(
            canvas.height(),
            0,
            "{src:?}: no frame, no caption, no blank line"
        );
    }
    // And the other side of the rule, which is what stops it being "return an empty canvas":
    // a formula that draws something must not take the empty path. `\R` is the same macro
    // put to use, so the pair also says the definition really was read and not merely
    // skipped over.
    let drawn = render_display(r"\def\R{\mathbb{R}}\R", 40, &theme).expect("draws");
    assert_eq!(drawn.height(), 1, "a formula with cells keeps its row");
}

#[test]
fn a_display_formula_that_does_not_parse_is_an_error_not_a_panic() {
    let theme = crate::theme::Theme::default();
    assert!(matches!(
        render_display(r"\frac{", 40, &theme),
        Err(crate::error::MathError::Parse { .. })
    ));
}

#[test]
fn a_construct_this_engine_cannot_build_reaches_the_display_caller_as_an_error() {
    // `render_display` must not swallow `NotDrawable`. Two routes reach it in display mode and
    // neither is an inline constraint: a grid, which no mode builds yet (design spec §6.5),
    // and `build::parse`'s source caps, which run before the mode is consulted at all.
    //
    // Both matter to the caller for the same reason: design spec §9's framed source is
    // reached by returning the error, so an arm that turned either into an empty canvas
    // would make the formula vanish off the page instead.
    let theme = crate::theme::Theme::default();
    assert_eq!(
        render_display(r"\begin{pmatrix} 1 \end{pmatrix}", 40, &theme).expect_err("no grids yet"),
        crate::error::MathError::NotDrawable("a matrix")
    );
    assert_eq!(
        render_display(&r"\alpha".repeat(64), 40, &theme).expect_err("past the command-run cap"),
        crate::error::MathError::NotDrawable("a formula with more than 32 chained commands")
    );
}

#[test]
fn display_and_inline_agree_about_a_flat_formula() {
    // One engine: a formula that needs no second row is the same row either way. The claim
    // is narrow on purpose — this pins that the two entry points agree, not that the two
    // walks do; `draw.rs`'s `the_flat_walk_and_the_canvas_walk_render_the_same_cells` is
    // where that is pinned, and it asserts `is_inline` on every case, so neither test can
    // catch a substitution that only shows up on a tall box.
    let theme = crate::theme::Theme::default();
    let canvas = render_display("E = mc", 40, &theme).expect("draws");
    assert_eq!(canvas.height(), 1);
    assert_eq!(canvas.row_text(0).trim_end(), "E = mc");
    assert_eq!(render_inline("E = mc").expect("renders"), "E = mc");
}

// --- Font commands (Task 15b) ----------------------------------------------------------
//
// `\mathbb{R}` and its friends are drawn with the *parser's own* table,
// `pulldown_latex::event::Font::map_char` (made `pub` by vendor patch 5). Every code point
// asserted below was read off that table rather than guessed, which matters: `\mathcal{L}`
// is U+2112 ℒ from Letterlike Symbols, **not** U+1D4DB 𝓛 — that one is `\mathbfcal`, the
// bold script. Eight of the sixteen `Font` variants land in Letterlike Symbols for at least
// one letter, because Unicode unified the "already existing" ones there and left holes in
// the Mathematical Alphanumeric Symbols block for them.

#[test]
fn a_font_command_draws_the_styled_letter_and_not_the_plain_one() {
    assert_eq!(rendered(r"\mathbb{R}"), "ℝ");
    assert_eq!(rendered(r"\mathbb{C}"), "ℂ");
    // A hole in U+1D400's double-struck range and a letter that is not a hole, so the
    // pair says the table is consulted rather than an offset applied.
    assert_eq!(rendered(r"\mathbb{E}"), "𝔼");
    assert_eq!(rendered(r"\mathcal{L}"), "ℒ");
    assert_eq!(rendered(r"\mathcal{A}"), "𝒜");
    assert_eq!(rendered(r"\mathfrak{a}"), "𝔞");
}

#[test]
fn every_font_command_the_parser_knows_reaches_its_own_block() {
    // `vendor/pulldown-latex/src/parser/primitives.rs:437-456` is the whole list of font
    // commands, and this is one case per `Font` variant that `map_char` maps. Two variants
    // map nothing and are listed with the others in
    // `a_font_variant_with_no_mapping_draws_the_plain_letter`.
    for (src, want) in [
        (r"\mathbf{x}", "𝐱"),
        (r"\mathit{x}", "𝑥"),
        (r"\mathsf{x}", "𝗑"),
        (r"\mathtt{x}", "𝚡"),
        (r"\mathbb{x}", "𝕩"),
        (r"\mathfrak{x}", "𝔵"),
        (r"\mathcal{X}", "𝒳"),
        (r"\mathbfcal{X}", "𝓧"),
        (r"\mathbfit{x}", "𝒙"),
        (r"\mathbffrak{x}", "𝖝"),
        (r"\mathsfit{x}", "𝘹"),
        (r"\mathbfsfup{x}", "𝘅"),
        (r"\mathbfsfit{x}", "𝙭"),
        (r"\mathbbit{d}", "ⅆ"),
    ] {
        assert_eq!(rendered(src), want, "{src}");
    }
}

#[test]
fn a_font_variant_with_no_mapping_draws_the_plain_letter() {
    // `Font::UpRight` maps every character to itself on purpose — `\mathrm{x}` *is* `x` on
    // a terminal, where there is one face. `Font::BoldSymbol` maps nothing because
    // `map_char` has no arm for it: the vendor's own renderer resolves `\boldsymbol` to
    // `Bold` or `BoldItalic` before the lookup, in `mathml.rs`, using a config-dependent
    // uprightness rule rather than the table.
    //
    // A *letter* therefore still draws plain, and that is what this pins. The digit half
    // of the same gap is closed — see
    // `boldsymbol_draws_bold_digits_and_leaves_its_letters_plain` — because the collapse
    // for digits is unconditional and needs none of that rule.
    assert_eq!(rendered(r"\mathrm{x}"), "x");
    assert_eq!(rendered(r"\boldsymbol{x}"), "x");
}

#[test]
fn boldsymbol_draws_bold_digits_and_leaves_its_letters_plain() {
    // `\boldsymbol` is bold *italic*, and Unicode encodes no italic digit, so the parser's
    // own renderer collapses `BoldSymbol` to `Bold` for a `Content::Number` and for
    // nothing else (`vendor/pulldown-latex/src/mathml.rs:628`). `build::atom` mirrors that
    // one rule. Without it `map_char` has no `BoldSymbol` arm at all and these draw as a
    // plain `123`.
    assert_eq!(rendered(r"\boldsymbol{123}"), "𝟏𝟐𝟑");
    // The same three characters `Bold` gives, which is the whole content of the collapse.
    assert_eq!(rendered(r"\mathbf{123}"), "𝟏𝟐𝟑");
    // The letters half stays open: choosing between `Bold` and `BoldItalic` needs the
    // parser's config-dependent `should_be_upright`, which this crate does not consult.
    assert_eq!(rendered(r"\boldsymbol{x}"), "x");
    // The collapse is scoped to the font, not to digits in general: an unstyled digit is
    // untouched, and a digit under another font still takes that font.
    assert_eq!(rendered(r"123"), "123");
    assert_eq!(rendered(r"\mathbb{1}"), "𝟙");
}

#[test]
fn a_font_command_does_not_leak_past_its_group() {
    // `\mathbb{R}` is `Begin(Normal)`, `StateChange(Font(DoubleStruck))`, the content,
    // `End` — the font is scoped by the group the parser wrapped the argument in. The `x`
    // is outside it and must stay plain; a leak would draw `𝕩`.
    assert_eq!(rendered(r"\mathbb{R}x"), "ℝx");
    assert_eq!(rendered(r"\mathbb{Q} \cup \mathbb{Z}"), "ℚ ∪ ℤ");
    assert_eq!(rendered(r"\mathbb{R} \times \mathbb{R} = x"), "ℝ × ℝ = x");
}

#[test]
fn a_bare_font_switch_runs_to_the_end_of_its_group() {
    // `\bf` takes no argument: it is a lone `StateChange` with no `Begin` of its own, so
    // it applies from where it stands to the end of the group that encloses it. The brace
    // is what stops it, and the same rule scopes both forms.
    assert_eq!(rendered(r"x \bf y"), "x𝐲");
    assert_eq!(rendered(r"{x \bf y} z"), "x𝐲z");
}

#[test]
fn a_font_command_keeps_its_scripts() {
    assert_eq!(rendered(r"\mathbb{R}^n"), "ℝⁿ");
    assert_eq!(rendered(r"\mathbb{R}^2"), "ℝ²");
    // `\mathbb{R^n}` puts the exponent *inside* the group, so LaTeX styles it too — and
    // Unicode has no raised 𝕟, so design spec §5.1's substitution declines and the caret
    // stays. That is the same answer this engine already gives `x^\alpha`, which sets
    // `x^α` for the same reason, so the font commands added no new fallback. Written out
    // here because `ℝⁿ` is the plausible wrong expectation: it would mean the `n` lost its
    // style on the way up.
    assert_eq!(rendered(r"\mathbb{R^n}"), "ℝ^𝕟");
    assert_eq!(rendered(r"x^\alpha"), "x^α");
}

#[test]
fn a_font_command_styles_digits_and_leaves_operators_alone() {
    // The parser applies the font to `Text`, `Number` and non-stretchy `Ordinary` and to
    // nothing else (`mathml.rs:602`, `:630`, `:683`), so this mirrors it: `1` is a
    // `Content::Number` and takes the style, `+` is a `Content::BinaryOp` and does not.
    // `map_char` would leave `+` alone anyway; asserting it here says the *arm* is right
    // and not merely the table.
    assert_eq!(rendered(r"\mathbb{1}"), "𝟙");
    assert_eq!(rendered(r"\mathbb{R+1}"), "ℝ + 𝟙");
}

#[test]
fn a_styled_letter_is_one_column_wide() {
    // Both blocks these come from are narrow, so a formula containing one measures like
    // the plain letter. If a terminal font sets them wide the *document* is what changes,
    // not this measurement — `crate::text` is the single home of width logic.
    let theme = crate::theme::Theme::default();
    let canvas = render_display(r"\mathbb{R} \cup \mathbb{Z}", 40, &theme).expect("draws");
    assert_eq!(canvas.height(), 1);
    assert_eq!(canvas.row_text(0).trim_end(), "ℝ ∪ ℤ");
    // One column each, so the formula measures exactly as `R ∪ Z` does. A styled letter
    // that measured two would push everything after it and the display width with it.
    assert_eq!(
        canvas.width(),
        render_display(r"R \cup Z", 40, &theme)
            .expect("draws")
            .width(),
        "a styled letter must measure like the plain one it replaced"
    );
}

#[test]
fn symbols_reports_the_styled_letter_because_the_author_asked_for_it() {
    // Design spec §13: an author who writes `\mathbb{R}` asked for `ℝ` as surely as one
    // who typed it, so `symbols` must report it and `tests/glyph_inventory.rs` must not
    // claim it for this crate. The plain `R` is *not* what was drawn and must not be
    // reported either — reporting it would let a real `R` elsewhere go unclaimed.
    assert_eq!(symbols(r"\mathbb{R}").expect("parses"), "ℝ");
    assert_eq!(
        symbols(r"\mathbb{Q} \cup \mathbb{Z}").expect("parses"),
        "ℚ∪ℤ"
    );
    assert_eq!(
        symbols(r"\mathcal{L}(x) = \mathbb{E}[x]").expect("parses"),
        "ℒ(x)=𝔼[x]"
    );
    // The same scoping `render_inline` gets: the walk `symbols` does is flat, so this is
    // the one assertion that pins its font stack rather than the recursion's.
    assert_eq!(symbols(r"\mathbb{R}x").expect("parses"), "ℝx");
}
