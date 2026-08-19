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
    // `RelationContent` can hold two characters (`\shortparallel` and its relatives);
    // `\coloneq` is one, and its own doc comment names it as the reason
    // `Content::Relation` cannot share an or-pattern with the other `char`-only arms.
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
