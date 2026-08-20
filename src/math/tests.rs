// SPDX-License-Identifier: MIT
use super::render_inline;

fn rendered(src: &str) -> String {
    render_inline(src).unwrap_or_else(|err| panic!("{src:?} failed: {err}"))
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
    // `Ordinary('a')` then `BinaryOp('+')` with nothing after. The operator still gets
    // its usual left space (it is not the leading token), but `write_events`' final
    // `trim_end` removes whatever space `spaced` wrote on its right, so the line never
    // ends in whitespace regardless of what the formula ends in.
    assert_eq!(rendered("a+"), "a +");
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
    // A function name normally earns a trailing space (`\sin x` -> `sin x`), but a
    // script is not a separate operand -- it attaches directly to its base, so
    // `\sin^2 x` must read `sin²x`, not `sin ²x`.
    assert_eq!(rendered(r"\sin^2 x"), "sin²x");
}

#[test]
fn a_function_used_as_a_script_base_still_gets_its_own_leading_space() {
    // `take_group`'s single-event branch used to build the base in a fresh, isolated
    // buffer, so `spaced_word`'s "am I at the head of a run?" check always saw an
    // empty buffer and always answered yes -- even when the base plainly was not at
    // the head of the formula. That silently dropped the leading space whenever an
    // `Ordinary`/`Number` token (which writes no space of its own) preceded the
    // scripted function name. A preceding `BinaryOp`/`Relation` masked the bug because
    // those already write their own trailing space, so `2 + \sin^2 x` was never wrong
    // -- these four are the two working and the two broken shapes side by side.
    assert_eq!(rendered(r"2\sin x"), "2 sin x");
    assert_eq!(rendered(r"2\sin^2 x"), "2 sin²x");
    assert_eq!(rendered(r"x\sin y"), "x sin y");
    assert_eq!(rendered(r"x\sin^2 y"), "x sin²y");
}

#[test]
fn a_group_used_as_a_script_base_still_gets_its_own_leading_space() {
    // The same bug as above, in the sibling path: `take_group`'s group branch built a
    // `{…}` base by recursing into its own isolated buffer, so a group's first token
    // was just as blind to real context as a bare token was. `take_base` now writes a
    // group base straight into `out` via `write_into`, so its first token's leading
    // space is earned (or not) by what precedes the *group*, not by the group's own
    // emptiness -- and the base is then bracketed, so the script still applies to the
    // whole group rather than just the last atom written (`(sin x)²`, not `sin x²`,
    // which would read as `sin(x²)`).
    assert_eq!(rendered(r"{\sin x}^2"), "(sin x)²");
    assert_eq!(rendered(r"2{\sin x}^2"), "2 (sin x)²");
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
fn a_trailing_space_inside_a_bracketed_base_does_not_defeat_the_single_atom_exemption() {
    // Regression caught in re-review: bracketing ran *before* the unconditional
    // trailing-space trim at the bottom of `take_base`, so a trailing space
    // `write_into` leaves behind (a big operator's own spacing, or a function name's)
    // was sealed *inside* the new parentheses -- which made a genuinely one-character
    // base count as two, so `bracketed()`'s single-atom exemption never fired.
    // `{\sum}^2` drew `(∑ )²` instead of the correct `∑²`, a straight regression
    // against the version before the C1 fix. The body is now trimmed before it is
    // bracketed, not after.
    assert_eq!(rendered(r"{\sum}^2"), "∑²");
    assert_eq!(rendered(r"{\prod}^2"), "∏²");
    assert_eq!(rendered(r"{\int}^2"), "∫²");
    // Multi-character bases still bracket correctly with the trailing space gone.
    assert_eq!(rendered(r"{\sin}^2"), "(sin)²");
    assert_eq!(rendered(r"2{\log}_2"), "2 (log)₂");
    assert_eq!(rendered(r"{a+}^2"), "(a +)²");
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
fn a_fraction_or_radical_as_a_script_base_declines_by_name() {
    // Carried to stage 2 (final review, Task 5): a fraction or a radical drawn on this
    // row is a two-dimensional box, which a script cannot flatten onto -- correctly
    // declined, not approximated. Pinned here so stage 2's proper layout of this case
    // changes a test, not a silent behaviour drift. The message names the position
    // ("as a script base"), not the construct, because a fraction and a radical both
    // draw fine elsewhere on this very row (`a_fraction_is_written_with_a_slash`,
    // `a_root_takes_the_radical_sign`).
    assert_eq!(
        render_inline(r"\frac{a}{b}^2").unwrap_err(),
        crate::error::MathError::NotInline("a fraction as a script base")
    );
    assert_eq!(
        render_inline(r"\sqrt{x}^2").unwrap_err(),
        crate::error::MathError::NotInline("a radical as a script base")
    );
}

#[test]
fn a_leading_unary_minus_before_a_function_name_keeps_a_space() {
    // Carried to stage 2 (final review, Task 3): `spaced` suppresses both sides of a
    // leading operator, but `spaced_word` (a function name) suppresses only its own
    // leading side, and neither function knows about the other. `-x` reads `−x`
    // (`a_leading_operator_glues_to_what_follows_instead_of_floating`), but `-\sin x`
    // reads `− sin x`, not `−sin x` -- correct, merely loose. Pinned so stage 2's
    // rewrite of this walk changes a test rather than drifting silently.
    assert_eq!(rendered(r"-\sin x"), "− sin x");
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
