// SPDX-License-Identifier: MIT
//! A class member's visibility marker, read from the source rather than built by hand.
//!
//! `mermaid_layout_class.rs` builds its members directly in the AST, which is right for a
//! layout snapshot and blind to the parser: `Visibility::PackageInternal` was drawn
//! correctly for as long as the parser was unable to produce one. The `~` that names it is
//! also Mermaid's generic delimiter, so reading the two in the wrong order lost the marker
//! *and* left every later generic in the same member inverted.
//!
//! So these tests assert the canvas, from Mermaid source, in both member forms — `class X
//! { … }` and `X : …`. A parse-only test here passed for years while the screen was wrong.

use mdmost::mermaid::ast::{Diagram, Field, Member, Visibility};
use mdmost::mermaid::parse::parse;
use mdmost::mermaid::render_mermaid;
use mdmost::theme::Theme;

/// The diagram `src` draws at width 60, as plain text.
#[track_caller]
fn drawn(src: &str) -> String {
    let theme = Theme::default_dark();
    match render_mermaid(src, 60, &theme) {
        Ok(canvas) => canvas.plain_text(),
        Err(error) => panic!("expected a drawn diagram, got: {error}"),
    }
}

/// Asserts that `art` shows `member` on some line.
#[track_caller]
fn shows(art: &str, member: &str) {
    assert!(art.contains(member), "expected `{member}` in:\n{art}");
}

/// Asserts that `art` shows nothing like `text`.
#[track_caller]
fn hides(art: &str, text: &str) {
    assert!(!art.contains(text), "did not expect `{text}` in:\n{art}");
}

/// The members of the first class in `src`.
#[track_caller]
fn members(src: &str) -> Vec<Member> {
    match parse(src) {
        Ok(Diagram::Class(diagram)) => diagram.classes[0].members.clone(),
        Ok(other) => panic!("expected a class diagram, got {other:?}"),
        Err(error) => panic!("expected a diagram, got: {error}"),
    }
}

/// All four markers, in both spellings of a member.
mod four_markers {
    use super::*;

    #[test]
    fn a_block_member_carries_every_marker() {
        let art = drawn(
            "classDiagram
    class A {
        +int pub
        -int priv
        #int prot
        ~int pkg
    }
",
        );
        shows(&art, "+pub: int");
        shows(&art, "-priv: int");
        shows(&art, "#prot: int");
        shows(&art, "~pkg: int");
    }

    #[test]
    fn a_colon_member_carries_every_marker() {
        // The `X : …` form reaches `parse_member` by a different route and was equally
        // broken; the original report named only the block form.
        let art = drawn(
            "classDiagram
    B : +int pub
    B : -int priv
    B : #int prot
    B : ~int pkg
",
        );
        shows(&art, "+pub: int");
        shows(&art, "-priv: int");
        shows(&art, "#prot: int");
        shows(&art, "~pkg: int");
    }

    #[test]
    fn a_package_internal_member_keeps_its_name_and_type_in_order() {
        // The symptom that started this: `~count int` drew `int: <count`, the marker
        // swallowed into the name and name and type swapped.
        let art = drawn("classDiagram\n    class A {\n        ~int count\n    }\n");
        shows(&art, "~count: int");
        hides(&art, "<count");
        assert_eq!(
            members("classDiagram\n    class A {\n        ~int count\n    }\n")[0],
            Member::Field(Field {
                visibility: Some(Visibility::PackageInternal),
                name: "count".to_string(),
                ty: Some("int".to_string()),
                classifier: None,
            })
        );
    }

    #[test]
    fn a_package_internal_method_keeps_its_marker() {
        let art = drawn("classDiagram\n    class A {\n        ~get() int\n    }\n");
        shows(&art, "~get(): int");
        let art = drawn("classDiagram\n    B : ~get() int\n");
        shows(&art, "~get(): int");
    }

    #[test]
    fn a_marker_still_composes_with_a_classifier() {
        let art = drawn("classDiagram\n    class A {\n        ~int shared$\n    }\n");
        shows(&art, "~shared: int$");
    }

    #[test]
    fn a_member_without_a_marker_is_unchanged() {
        let art = drawn("classDiagram\n    class A {\n        int count\n        run()\n    }\n");
        shows(&art, "count: int");
        shows(&art, "run()");
        hides(&art, "+count");
        hides(&art, "~count");
    }
}

/// Generics, which share the `~` the package-internal marker is spelled with.
mod generics {
    use super::*;

    #[test]
    fn a_generic_after_a_package_internal_marker_is_not_inverted() {
        // The serious symptom. The marker was consumed as an opening `~`, so the next
        // pair in the same member came out backwards and an ordinary Mermaid diagram
        // silently drew `Map>K,V<` at the reader.
        let art = drawn(
            "classDiagram
    class C {
        +Map~K,V~ pairs
        ~get() Map~K,V~
    }
",
        );
        shows(&art, "+pairs: Map<K,V>");
        shows(&art, "~get(): Map<K,V>");
        hides(&art, "Map>K,V<");
    }

    #[test]
    fn a_generic_after_a_package_internal_marker_survives_the_colon_form_too() {
        let art = drawn("classDiagram\n    B : ~get(List~int~ xs) Map~K,V~\n");
        shows(&art, "~get(xs: List<int>): Map<K,V>");
        hides(&art, "List>int<");
        hides(&art, "Map>K,V<");
    }

    #[test]
    fn a_generic_without_a_marker_still_normalises() {
        let art = drawn(
            "classDiagram
    class D {
        List~int~ position
        setPoints(List~int~ points)
        getPoints() List~int~
    }
",
        );
        shows(&art, "position: List<int>");
        shows(&art, "setPoints(points: List<int>)");
        shows(&art, "getPoints(): List<int>");
        hides(&art, "List>int<");
    }

    #[test]
    fn a_generic_after_each_of_the_other_markers_is_still_upright() {
        let art = drawn(
            "classDiagram
    class E {
        +get() List~int~
        -get() List~int~
        #get() List~int~
    }
",
        );
        assert_eq!(
            art.matches("get(): List<int>").count(),
            3,
            "all three draw upright:\n{art}"
        );
    }
}

/// The `#` that is both a visibility marker and Mermaid's entity sigil.
mod hash_sigil {
    use super::*;

    #[test]
    fn a_protected_member_may_hold_an_entity() {
        // `#` opens an escape as well as naming protected visibility. The marker is read
        // at the first character and the entity decodes later, on the leaf, so the two
        // never contend.
        let art = drawn("classDiagram\n    class F {\n        #int issue#35;7\n    }\n");
        shows(&art, "#issue#7: int");
        let art = drawn("classDiagram\n    B : #int issue#35;7\n");
        shows(&art, "#issue#7: int");
    }

    #[test]
    fn a_hash_that_names_nothing_is_still_a_marker() {
        let art = drawn("classDiagram\n    class F {\n        #int count\n    }\n");
        shows(&art, "#count: int");
    }

    #[test]
    fn a_leading_sigil_is_read_as_the_marker_it_looks_like() {
        // The one place the two readings collide: a `#` in the marker position that also
        // opens a valid escape. Syntax is read from the source before anything decodes —
        // that is what keeps a decoded character from becoming syntax — so the marker
        // wins, and `#35;count` is a protected member named `35;count`. Recorded because
        // it is a real boundary, not because an author is likely to write it.
        let art = drawn("classDiagram\n    class F {\n        #35;count\n    }\n");
        shows(&art, "#35;count");
    }
}

/// A decoded character is text. It must not become a marker or a generic delimiter.
mod decoded_tilde {
    use super::*;

    #[test]
    fn a_decoded_tilde_is_not_a_visibility_marker() {
        // `&#126;count` is a name an author asked for, not a package-internal `count`.
        // The two draw the same glyph in the same column — the marker's glyph *is* `~` —
        // so the canvas cannot tell them apart and the AST is the honest witness here.
        // The drawn line is asserted as well, so a fix that loses the character shows up.
        let src = "classDiagram\n    class G {\n        &#126;count: int\n    }\n";
        assert_eq!(
            members(src)[0],
            Member::Field(Field {
                visibility: None,
                name: "~count".to_string(),
                ty: Some("int".to_string()),
                classifier: None,
            })
        );
        shows(&drawn(src), "~count: int");
    }

    #[test]
    fn a_decoded_tilde_is_not_a_generic_delimiter_either() {
        // Here the canvas can tell: a member whose text was decoded before its generics
        // were normalised has one extra `~` in the run, so the pair that follows comes
        // out backwards. This is the same inversion as the marker bug, from the other
        // side, and it is why decoding stays on the leaves.
        let art = drawn("classDiagram\n    class G {\n        a&#126;b: Map~K,V~\n    }\n");
        shows(&art, "a~b: Map<K,V>");
        hides(&art, "Map>K,V<");
    }
}
