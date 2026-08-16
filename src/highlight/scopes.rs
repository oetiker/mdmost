// SPDX-License-Identifier: MIT
//! The scope → semantic style mapping.
//!
//! `syntect` describes every token by a *scope stack* such as
//! `source.rust meta.function.rust entity.name.function.rust`. Its own themes attach
//! colours to those scopes; `mdmost` deliberately does not use them, because a
//! foreign theme inside a framed code block clashes with the surrounding palette
//! (design spec §8).
//!
//! Instead this module owns a table of `TextMate`-style scope selectors, each pointing at one
//! *semantic slot* of [`CodeStyles`]. The slot is resolved against the active
//! [`Theme`](crate::theme::Theme), so highlighted code is drawn from the very same
//! palette as headings, tables and diagrams — in the dark theme and the light theme
//! alike.
//!
//! Selection follows the same rule as a `syntect` theme: every selector is matched
//! against the scope stack and the one with the highest [`MatchPower`] wins, so a
//! specific rule (`constant.numeric`) beats a general one (`constant`) without the
//! table having to be ordered.

use std::sync::LazyLock;

use syntect::highlighting::ScopeSelectors;
use syntect::parsing::{MatchPower, Scope};

use crate::theme::{CodeStyles, Style};

/// A semantic slot of [`CodeStyles`].
///
/// This indirection is what keeps the table below free of colours: it names *what a
/// token is*, and the theme decides what that looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// Language keywords and storage specifiers.
    Keyword,
    /// String and character literals.
    Str,
    /// Numeric literals.
    Number,
    /// Comments of every flavour.
    Comment,
    /// Function, method and macro names.
    Function,
    /// Type, class, trait and namespace names.
    TypeName,
    /// Plain identifiers.
    Variable,
    /// Named constants, language literals and escape sequences.
    Constant,
    /// Operators.
    Operator,
    /// Brackets, separators and terminators.
    Punctuation,
    /// Macro and preprocessor-macro names.
    MacroName,
    /// Namespace, module and package names.
    Namespace,
    /// Escape sequences inside string literals.
    Escape,
    /// Attributes, annotations, decorators, preprocessor directives, mapping keys.
    Attribute,
    /// Text the syntax flagged as illegal.
    Invalid,
}

impl Slot {
    /// Resolves the slot against a theme's code styles.
    fn style(self, code: &CodeStyles) -> Style {
        match self {
            Self::Keyword => code.keyword,
            Self::Str => code.string,
            Self::Number => code.number,
            Self::Comment => code.comment,
            Self::Function => code.function,
            Self::TypeName => code.type_name,
            Self::Variable => code.variable,
            Self::Constant => code.constant,
            Self::Operator => code.operator,
            Self::Punctuation => code.punctuation,
            Self::MacroName => code.macro_name,
            Self::Namespace => code.namespace,
            Self::Escape => code.escape,
            Self::Attribute => code.attribute,
            Self::Invalid => code.invalid,
        }
    }
}

/// The scope → slot table.
///
/// Rationale for the groupings, in the order a reader is likely to notice them:
///
/// * **Comments** are the only slot that is dimmed and italic, so prose inside code
///   recedes rather than competing with it. The comment *punctuation* (`//`, `/*`)
///   joins it so a comment reads as one object.
/// * **Strings** keep their quotes (`punctuation.definition.string`), but an escape
///   sequence inside a string gets its own `Escape` slot, which is the one place the
///   mapping deliberately breaks a run of colour — `\n` is code, not text.
/// * **Numbers** are separated from other constants so that literal-heavy code
///   (tables of data, colour values) gets visual rhythm.
/// * **Keywords vs. types**: the whole `storage` family is a keyword, and `type_name`
///   is reserved for *named* types (`entity.name.*`, `support.type`). This is not
///   arbitrary: `syntect` gives Rust's `let` and its `u64` the same
///   `storage.type.rust` scope, as it gives Go's `func`, `var` and `int` the same
///   `storage.type.go`, so no rule could separate them. Colouring the family as
///   keywords keeps declarations legible and leaves the type colour meaning "a type
///   this file names", which is the more informative distinction.
/// * **Functions** cover definitions (`entity.name.function`), calls
///   (`variable.function`) and library functions (`support.function`), so a call site
///   and its definition agree. The enclosing `meta.function-call` region is
///   deliberately *not* mapped: in shell syntaxes it spans the arguments too.
/// * **Attributes** collect the "meta" decorations that sit beside code rather than
///   in it: `#[derive]`, `@Override`, `#include`, HTML/JSX attribute names, and — the
///   one liberty taken — mapping keys in JSON/YAML/TOML, which are structurally the
///   same idea and would otherwise be indistinguishable from their string values.
/// * **Operators and punctuation** are separate but both quiet, punctuation quieter
///   still: an operator says what the code *does*, a bracket only says where it
///   starts, and neither should pull the eye away from identifiers.
/// * **Macros and namespaces** are split off from functions and types, because an
///   invocation of `println!` is a different event from a call to `write`, and a
///   module path names a container rather than a value's type.
/// * **Markup** scopes appear when a fence contains Markdown, LaTeX or a diff; they
///   are folded onto the nearest code slot rather than left unstyled.
const RULES: &[(&str, Slot)] = &[
    // Comments.
    ("comment", Slot::Comment),
    ("punctuation.definition.comment", Slot::Comment),
    // Strings.
    ("string", Slot::Str),
    ("punctuation.definition.string", Slot::Str),
    ("constant.character.escape", Slot::Escape),
    ("constant.other.placeholder", Slot::Escape),
    // Numbers and constants.
    ("constant", Slot::Constant),
    ("constant.numeric", Slot::Number),
    ("constant.language", Slot::Constant),
    ("support.constant", Slot::Constant),
    ("variable.language", Slot::Constant),
    // Keywords and storage.
    ("keyword", Slot::Keyword),
    ("storage", Slot::Keyword),
    // Functions and macros.
    ("entity.name.function", Slot::Function),
    ("entity.name.macro", Slot::MacroName),
    ("support.function", Slot::Function),
    ("support.macro", Slot::MacroName),
    ("variable.function", Slot::Function),
    // Types, classes and namespaces.
    ("entity.name.type", Slot::TypeName),
    ("entity.name.class", Slot::TypeName),
    ("entity.name.struct", Slot::TypeName),
    ("entity.name.enum", Slot::TypeName),
    ("entity.name.trait", Slot::TypeName),
    ("entity.name.interface", Slot::TypeName),
    ("entity.name.impl", Slot::TypeName),
    ("entity.name.union", Slot::TypeName),
    ("entity.name.namespace", Slot::Namespace),
    ("entity.name.section", Slot::Namespace),
    ("entity.name.module", Slot::Namespace),
    ("entity.other.inherited-class", Slot::TypeName),
    ("support.type", Slot::TypeName),
    ("support.class", Slot::TypeName),
    // Variables.
    ("variable", Slot::Variable),
    ("variable.parameter", Slot::Variable),
    ("variable.other.member", Slot::Variable),
    ("support.variable", Slot::Variable),
    // Attributes, annotations, directives and mapping keys.
    ("entity.other.attribute-name", Slot::Attribute),
    ("meta.annotation", Slot::Attribute),
    ("meta.attribute", Slot::Attribute),
    ("meta.preprocessor", Slot::Attribute),
    ("keyword.preprocessor", Slot::Attribute),
    ("variable.annotation", Slot::Attribute),
    ("punctuation.definition.annotation", Slot::Attribute),
    ("entity.name.tag", Slot::Keyword),
    ("meta.mapping.key string", Slot::Attribute),
    ("meta.structure.dictionary.key string", Slot::Attribute),
    // Operators and punctuation.
    ("keyword.operator", Slot::Operator),
    ("punctuation", Slot::Punctuation),
    ("punctuation.separator", Slot::Punctuation),
    ("punctuation.terminator", Slot::Punctuation),
    ("punctuation.section", Slot::Punctuation),
    ("punctuation.accessor", Slot::Punctuation),
    // Markup, for fences holding Markdown, LaTeX or a diff.
    ("markup.heading", Slot::Keyword),
    ("markup.bold", Slot::Keyword),
    ("markup.italic", Slot::TypeName),
    ("markup.underline.link", Slot::Function),
    ("markup.raw", Slot::Str),
    ("markup.list", Slot::Punctuation),
    ("markup.inserted", Slot::Str),
    ("markup.deleted", Slot::Invalid),
    ("markup.changed", Slot::Attribute),
    // Errors.
    ("invalid", Slot::Invalid),
    ("invalid.deprecated", Slot::Attribute),
];

/// A compiled rule: the selector plus the slot it selects.
struct Rule {
    selectors: ScopeSelectors,
    slot: Slot,
}

/// The compiled table.
///
/// Compilation is fallible in principle (a selector string could be malformed), and
/// this table lives outside test code, so a bad entry is dropped rather than
/// panicking. `compiled_rules_match_source_table` in the unit tests asserts that
/// nothing was in fact dropped, which turns a typo into a test failure.
static RULES_COMPILED: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    RULES
        .iter()
        .filter_map(|&(selector, slot)| {
            selector
                .parse::<ScopeSelectors>()
                .ok()
                .map(|selectors| Rule { selectors, slot })
        })
        .collect()
});

/// The style for a scope stack, or the theme's plain code style if nothing matches.
///
/// The rule with the highest [`MatchPower`] wins;
/// ties keep the earlier rule, which only ever happens between two spellings of the
/// same specificity and therefore never changes the visible result.
pub(super) fn style_for(stack: &[Scope], code: &CodeStyles) -> Style {
    let mut best: Option<(MatchPower, Slot)> = None;
    for rule in RULES_COMPILED.iter() {
        let Some(power) = rule.selectors.does_match(stack) else {
            continue;
        };
        if best.is_none_or(|(current, _)| power > current) {
            best = Some((power, rule.slot));
        }
    }
    match best {
        Some((_, slot)) => slot.style(code),
        None => code.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use syntect::parsing::ScopeStack;

    /// Every selector in [`RULES`] must compile; see [`RULES_COMPILED`].
    #[test]
    fn compiled_rules_match_source_table() {
        assert_eq!(RULES_COMPILED.len(), RULES.len());
    }

    fn style_of(scope: &str) -> Style {
        let theme = Theme::default_dark();
        let stack: ScopeStack = scope.parse().expect("test scope parses");
        style_for(stack.as_slice(), &theme.code)
    }

    #[test]
    fn specific_rules_beat_general_ones() {
        let theme = Theme::default_dark();
        assert_eq!(
            style_of("source.rust constant.numeric.rust"),
            theme.code.number
        );
        assert_eq!(
            style_of("source.rust constant.language.rust"),
            theme.code.constant
        );
        // `storage` is uniformly a keyword; see the table's documentation.
        assert_eq!(
            style_of("source.c storage.type.built-in.c"),
            theme.code.keyword
        );
        assert_eq!(style_of("source.c storage.modifier.c"), theme.code.keyword);
        assert_eq!(
            style_of("source.rust meta.struct.rust entity.name.struct.rust"),
            theme.code.type_name
        );
    }

    #[test]
    fn macros_and_namespaces_have_their_own_slots() {
        let theme = Theme::default_dark();
        assert_eq!(
            style_of("source.rust support.macro.rust"),
            theme.code.macro_name
        );
        assert_ne!(theme.code.macro_name, theme.code.function);
        assert_eq!(
            style_of("source.cs entity.name.namespace.cs"),
            theme.code.namespace
        );
        assert_ne!(theme.code.namespace, theme.code.type_name);
    }

    #[test]
    fn punctuation_is_quieter_than_operators() {
        let theme = Theme::default_dark();
        assert_eq!(
            style_of("source.rust punctuation.section.block.begin.rust"),
            theme.code.punctuation
        );
        assert_eq!(
            style_of("source.rust keyword.operator.rust"),
            theme.code.operator
        );
        assert_ne!(theme.code.punctuation, theme.code.operator);
    }

    #[test]
    fn escapes_stand_out_inside_strings() {
        let theme = Theme::default_dark();
        assert_eq!(
            style_of("source.rust string.quoted.double.rust"),
            theme.code.string
        );
        assert_eq!(
            style_of("source.rust string.quoted.double.rust constant.character.escape.rust"),
            theme.code.escape
        );
    }

    #[test]
    fn unmatched_scopes_fall_back_to_plain_code_text() {
        let theme = Theme::default_dark();
        assert_eq!(style_of("source.rust meta.block.rust"), theme.code.text);
        assert_eq!(style_of(""), theme.code.text);
    }
}
