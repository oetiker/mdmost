//! `classDiagram` layout (design spec §6.3).
//!
//! The three-compartment class box is a [`NodeArt`]; the relations map onto the
//! engine's [`Terminator`]s and cardinalities onto its end labels. Everything else —
//! layering, crossing reduction, routing — is the shared engine in
//! [`graph`](super::graph).
//!
//! ```text
//! ┌──────────────────┐
//! │  <<interface>>   │
//! │      Shape       │
//! ├──────────────────┤
//! │ +id: int         │
//! ├──────────────────┤
//! │ +area(): float   │
//! └──────────────────┘
//! ```

mod member;

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::{
    Class, ClassAnnotation, ClassArrow, ClassDiagram, ClassRelation, Direction, LineStyle, Member,
};
use crate::theme::Theme;

use super::graph::{
    self, DrawnLabel, EdgeSpec, Fit, GraphSpec, GroupSpec, NodeArt, NodeIdx, Stroke, Terminator,
};
use super::record::{self, Row};

/// Widest a relation label is allowed to get before it is wrapped.
const LABEL_WIDTH: usize = 18;

/// Draws a class diagram into a canvas exactly `width` columns wide.
///
/// The engine may degrade the drawing as far as [`Fit::COMPACT`] allows; use
/// [`draw_with`] to say otherwise.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when the diagram cannot be made to fit.
pub fn draw(diagram: &ClassDiagram, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    draw_with(diagram, width, theme, Fit::COMPACT)
}

/// Draws a class diagram under the given fit policy.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when the diagram cannot be made to fit within
/// what `fit` allows.
pub fn draw_with(
    diagram: &ClassDiagram,
    width: u16,
    theme: &Theme,
    fit: Fit,
) -> Result<Canvas, MermaidError> {
    let spec = build(diagram);
    graph::draw(&spec, &Art { diagram }, width, theme, fit)
}

/// Draws class boxes for the engine.
struct Art<'a> {
    diagram: &'a ClassDiagram,
}

impl NodeArt for Art<'_> {
    fn render(&self, node: NodeIdx, budget: u16, theme: &Theme) -> Canvas {
        match self.diagram.classes.get(node.0) {
            Some(class) => class_box(class, budget, theme),
            None => Canvas::empty(0),
        }
    }
}

/// Draws one class as a name compartment over fields over methods.
///
/// Empty compartments are dropped by [`record::draw`], so a class with no members is a
/// plain name box — which is exactly how Mermaid draws it.
fn class_box(class: &Class, budget: u16, theme: &Theme) -> Canvas {
    let styles = theme.diagram;
    let mut header = Vec::new();
    if let Some(annotation) = &class.annotation {
        header.push(Row::centred(stereotype(annotation), styles.stereotype));
    }
    header.push(Row::centred(display_name(class), styles.node_text).sourced(class.name.clone()));

    let mut fields = Vec::new();
    let mut methods = Vec::new();
    for member in &class.members {
        match member {
            Member::Field(field) => {
                fields.push(Row::left(member::field(field), styles.node_text));
            }
            Member::Method(method) => {
                methods.push(Row::left(member::method(method), styles.node_text));
            }
        }
    }

    record::draw(&[header, fields, methods], budget, theme)
}

/// The name shown in a class box, with any generic parameter restored.
///
/// A restored generic makes the drawn text `Square<Shape>`, which no stretch of the
/// source spells — the tildes are gone and the angle brackets were never there — so
/// `Label::spans_for` declines that row and it draws without provenance. A plain name
/// is a byte-for-byte copy and maps back.
fn display_name(class: &Class) -> String {
    match &class.generic {
        Some(generic) => format!("{}<{generic}>", class.name.text()),
        None => class.name.text(),
    }
}

/// The text of a `<<…>>` annotation, angle brackets included.
fn stereotype(annotation: &ClassAnnotation) -> String {
    let text = match annotation {
        ClassAnnotation::Interface => "interface",
        ClassAnnotation::Abstract => "abstract",
        ClassAnnotation::Enumeration => "enumeration",
        ClassAnnotation::Service => "service",
        ClassAnnotation::Other(other) => other.as_str(),
    };
    format!("<<{text}>>")
}

/// Translates the class AST into an engine specification.
///
/// Class diagrams have no containers, so every class sits in the root group.
fn build(diagram: &ClassDiagram) -> GraphSpec {
    GraphSpec {
        direction: diagram.direction.unwrap_or(Direction::TopToBottom),
        node_count: diagram.classes.len(),
        edges: diagram.relations.iter().map(relation).collect(),
        root: GroupSpec {
            nodes: (0..diagram.classes.len()).map(NodeIdx).collect(),
            ..GroupSpec::default()
        },
    }
}

/// Translates one relation, including its terminators, cardinalities and label.
///
/// The edge runs left-to-right as written, so `Animal <|-- Duck` layers `Animal` above
/// `Duck` — the parent on top, which is the conventional UML reading.
fn relation(relation: &ClassRelation) -> EdgeSpec {
    EdgeSpec {
        from: NodeIdx(relation.left.0),
        to: NodeIdx(relation.right.0),
        stroke: match relation.line {
            LineStyle::Solid => Stroke::Solid,
            LineStyle::Dashed => Stroke::Dotted,
        },
        tail: terminator(relation.left_end),
        head: terminator(relation.right_end),
        label: relation
            .label
            .as_ref()
            .filter(|label| !label.is_empty())
            .map(|label| DrawnLabel::wrapped(label, LABEL_WIDTH))
            .unwrap_or_default(),
        tail_label: cardinality(relation.left_cardinality.as_deref()),
        head_label: cardinality(relation.right_cardinality.as_deref()),
    }
}

/// Translates a class relation terminator.
fn terminator(arrow: ClassArrow) -> Terminator {
    match arrow {
        ClassArrow::None => Terminator::None,
        ClassArrow::Triangle => Terminator::HollowTriangle,
        ClassArrow::FilledDiamond => Terminator::FilledDiamond,
        ClassArrow::HollowDiamond => Terminator::HollowDiamond,
        ClassArrow::Arrow => Terminator::Arrow,
    }
}

/// A cardinality label, dropped when the source gave an empty one.
fn cardinality(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::ast::{Field, Label, Method, Visibility};

    fn class(name: &str, members: Vec<Member>) -> Class {
        Class {
            name: Label::line(name),
            generic: None,
            annotation: None,
            members,
        }
    }

    fn field(name: &str, ty: &str) -> Member {
        Member::Field(Field {
            visibility: Some(Visibility::Public),
            name: name.to_string(),
            ty: Some(ty.to_string()),
            classifier: None,
        })
    }

    fn method(name: &str) -> Member {
        Member::Method(Method {
            visibility: Some(Visibility::Public),
            name: name.to_string(),
            params: Vec::new(),
            returns: Some("bool".to_string()),
            classifier: None,
        })
    }

    #[test]
    fn a_class_box_has_a_compartment_per_member_kind() {
        let theme = Theme::default_dark();
        let class = class("Animal", vec![field("age", "int"), method("isMammal")]);
        let text = class_box(&class, 40, &theme).plain_text();
        assert!(text.contains("Animal"), "{text}");
        assert!(text.contains("+age: int"), "{text}");
        assert!(text.contains("+isMammal(): bool"), "{text}");
        assert_eq!(text.matches('├').count(), 2, "two rules: {text}");
    }

    #[test]
    fn a_class_without_members_is_a_plain_name_box() {
        let theme = Theme::default_dark();
        let text = class_box(&class("Marker", Vec::new()), 40, &theme).plain_text();
        assert!(!text.contains('├'), "no rules expected: {text}");
    }

    #[test]
    fn a_stereotype_sits_above_the_name() {
        let theme = Theme::default_dark();
        let mut class = class("Shape", Vec::new());
        class.annotation = Some(ClassAnnotation::Interface);
        let canvas = class_box(&class, 40, &theme);
        assert!(canvas.row_text(1).contains("<<interface>>"));
        assert!(canvas.row_text(2).contains("Shape"));
    }

    #[test]
    fn a_generic_parameter_is_shown_in_the_name() {
        let mut class = class("Square", Vec::new());
        class.generic = Some("Shape".to_string());
        assert_eq!(display_name(&class), "Square<Shape>");
    }

    #[test]
    fn cardinalities_become_end_labels() {
        let spec = relation(&ClassRelation {
            left: crate::mermaid::ast::ClassId(0),
            right: crate::mermaid::ast::ClassId(1),
            left_end: ClassArrow::None,
            right_end: ClassArrow::Arrow,
            line: LineStyle::Solid,
            left_cardinality: Some("1".to_string()),
            right_cardinality: Some("0..*".to_string()),
            label: Some(Label::line("places")),
        });
        assert_eq!(spec.tail_label.as_deref(), Some("1"));
        assert_eq!(spec.head_label.as_deref(), Some("0..*"));
        assert_eq!(spec.label.lines(), vec!["places"]);
        assert_eq!(spec.head, Terminator::Arrow);
    }

    #[test]
    fn a_blank_cardinality_is_dropped() {
        assert_eq!(cardinality(Some("  ")), None);
        assert_eq!(cardinality(None), None);
        assert_eq!(cardinality(Some(" 1 ")).as_deref(), Some("1"));
    }

    #[test]
    fn a_dashed_relation_uses_the_dotted_stroke() {
        let mut base = ClassRelation {
            left: crate::mermaid::ast::ClassId(0),
            right: crate::mermaid::ast::ClassId(1),
            left_end: ClassArrow::Triangle,
            right_end: ClassArrow::None,
            line: LineStyle::Dashed,
            left_cardinality: None,
            right_cardinality: None,
            label: None,
        };
        assert_eq!(relation(&base).stroke, Stroke::Dotted);
        assert_eq!(relation(&base).tail, Terminator::HollowTriangle);
        base.line = LineStyle::Solid;
        assert_eq!(relation(&base).stroke, Stroke::Solid);
    }

    #[test]
    fn every_class_lands_in_the_root_group() {
        let diagram = ClassDiagram {
            direction: None,
            classes: vec![class("A", Vec::new()), class("B", Vec::new())],
            relations: Vec::new(),
        };
        let spec = build(&diagram);
        assert_eq!(spec.root.nodes, vec![NodeIdx(0), NodeIdx(1)]);
        assert_eq!(spec.direction, Direction::TopToBottom);
    }
}
