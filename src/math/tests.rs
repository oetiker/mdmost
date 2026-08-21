// SPDX-License-Identifier: MIT
use super::{render_inline, symbols};

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
    assert_eq!(rendered(r"\frac{\sqrt{a}}{b^2}"), "(√a)/b²");
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
    // `pulldown-latex-0.8.0/src/parser/primitives.rs:1157-1172`; `\coloneq` is `:` then
    // `−`, which is what this asserts.
    //
    // This comment named `\shortparallel` as an example. It is not one: it is
    // `RelationContent::single_char('∥')` at `primitives.rs:1066`, so the example
    // contradicted the input on the next line.
    assert_eq!(rendered(r"a \coloneq b"), "a :− b");
}

#[test]
fn a_horizontal_space_command_draws_one_column() {
    assert_eq!(rendered(r"a\kern1em b"), "a b");
}

#[test]
fn a_state_change_draws_nothing_of_its_own() {
    // `\mathbf` is `Event::StateChange(StateChange::Font(..))` wrapping its argument in
    // a group; the walk ignores the state change and still writes the content inside.
    assert_eq!(rendered(r"\mathbf{x}"), "x");
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
    assert_eq!(err, crate::error::MathError::NotInline("a matrix"));
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
        crate::error::MathError::NotInline("a root index with no raised form")
    );
    // The boundary, one character either side of it: `q` has no superscript form and `p`
    // has, so these two differ only in whether the table has the letter.
    assert_eq!(
        render_inline(r"\sqrt[q]{x}").unwrap_err(),
        crate::error::MathError::NotInline("a root index with no raised form")
    );
    assert_eq!(rendered(r"\sqrt[p]{x}"), "ᵖ√x");
}

proptest::proptest! {
    /// Design spec §9: a wrecked formula must never take down a document.
    #[test]
    fn arbitrary_input_never_panics(src in ".{0,200}") {
        let _ = render_inline(&src);
    }

    /// The same, over strings made of the characters most likely to confuse a parser.
    #[test]
    fn latex_shaped_noise_never_panics(
        src in {
            use proptest::strategy::Strategy;
            proptest::collection::vec(
                proptest::sample::select(vec![
                    "\\frac", "\\sqrt", "\\sum", "{", "}", "^", "_", "&", "\\\\",
                    "\\begin{pmatrix}", "\\end{pmatrix}", "\\alpha", "$", "\\",
                    // Delimiter-aware vocabulary, added alongside the `\left`/`\right`
                    // fix: bare brackets, `\left`/`\right` themselves, an invisible
                    // delimiter, and plain letters/digits so a generated string is more
                    // often something the parser accepts far enough to reach the new
                    // code, rather than failing at the lexer on every case.
                    "\\left", "\\right", "(", ")", "[", "]", ".", "a", "1",
                ]),
                0..20,
            ).prop_map(|parts| parts.concat())
        }
    ) {
        let _ = render_inline(&src);
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
