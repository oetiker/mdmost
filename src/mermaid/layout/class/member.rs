// SPDX-License-Identifier: MIT
//! Formatting of class members into the one-line form drawn in a compartment.
//!
//! Mermaid accepts a member written either way round (`+int age` and `+age: int` are
//! the same field), so the AST keeps name and type apart and the choice of how to show
//! them is made here, once. UML order — `visibility name: type` — is used throughout,
//! so fields and methods line up down the compartment whichever way the source wrote
//! them (design spec §6.3).

use crate::mermaid::ast::{Classifier, Field, Method, Param, Visibility};

/// Formats a field as `+name: type`.
///
/// The type is omitted when the source gave none, and a trailing `$`/`*` classifier is
/// kept because it is the only place staticness and abstractness are shown.
pub(super) fn field(field: &Field) -> String {
    let mut out = String::new();
    out.push_str(marker(field.visibility));
    out.push_str(&field.name);
    if let Some(ty) = &field.ty {
        out.push_str(": ");
        out.push_str(ty);
    }
    out.push_str(classifier(field.classifier));
    out
}

/// Formats a method as `+name(params): returns`.
pub(super) fn method(method: &Method) -> String {
    let mut out = String::new();
    out.push_str(marker(method.visibility));
    out.push_str(&method.name);
    out.push('(');
    for (at, param) in method.params.iter().enumerate() {
        if at > 0 {
            out.push_str(", ");
        }
        out.push_str(&self::param(param));
    }
    out.push(')');
    if let Some(returns) = &method.returns {
        out.push_str(": ");
        out.push_str(returns);
    }
    out.push_str(classifier(method.classifier));
    out
}

/// Formats one parameter as `name: type`, or just the token when only one was written.
fn param(param: &Param) -> String {
    match &param.ty {
        Some(ty) => format!("{}: {ty}", param.name),
        None => param.name.clone(),
    }
}

/// The UML glyph for a visibility marker, or nothing when the source gave none.
fn marker(visibility: Option<Visibility>) -> &'static str {
    match visibility {
        Some(Visibility::Public) => "+",
        Some(Visibility::Private) => "-",
        Some(Visibility::Protected) => "#",
        Some(Visibility::PackageInternal) => "~",
        None => "",
    }
}

/// The trailing classifier glyph, or nothing.
fn classifier(classifier: Option<Classifier>) -> &'static str {
    match classifier {
        Some(Classifier::Static) => "$",
        Some(Classifier::Abstract) => "*",
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_field(name: &str, ty: Option<&str>) -> Field {
        Field {
            visibility: Some(Visibility::Private),
            name: name.to_string(),
            ty: ty.map(str::to_string),
            classifier: None,
        }
    }

    #[test]
    fn a_field_reads_name_then_type() {
        assert_eq!(field(&plain_field("age", Some("int"))), "-age: int");
        assert_eq!(field(&plain_field("age", None)), "-age");
    }

    #[test]
    fn every_visibility_marker_has_its_uml_glyph() {
        let cases = [
            (Visibility::Public, "+"),
            (Visibility::Private, "-"),
            (Visibility::Protected, "#"),
            (Visibility::PackageInternal, "~"),
        ];
        for (visibility, glyph) in cases {
            assert_eq!(marker(Some(visibility)), glyph);
        }
        assert_eq!(marker(None), "");
    }

    #[test]
    fn a_classifier_is_kept_at_the_end() {
        let mut with = plain_field("count", Some("int"));
        with.classifier = Some(Classifier::Static);
        assert_eq!(field(&with), "-count: int$");
        with.classifier = Some(Classifier::Abstract);
        assert_eq!(field(&with), "-count: int*");
    }

    #[test]
    fn a_method_shows_its_parameters_and_return_type() {
        let method_ = Method {
            visibility: Some(Visibility::Public),
            name: "move".to_string(),
            params: vec![
                Param {
                    name: "x".to_string(),
                    ty: Some("int".to_string()),
                },
                Param {
                    name: "flag".to_string(),
                    ty: None,
                },
            ],
            returns: Some("void".to_string()),
            classifier: None,
        };
        assert_eq!(method(&method_), "+move(x: int, flag): void");
    }

    #[test]
    fn a_method_without_parameters_or_return_type_still_has_parentheses() {
        let method_ = Method {
            visibility: None,
            name: "run".to_string(),
            params: Vec::new(),
            returns: None,
            classifier: None,
        };
        assert_eq!(method(&method_), "run()");
    }
}
