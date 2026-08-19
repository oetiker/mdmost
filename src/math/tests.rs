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
