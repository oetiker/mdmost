//! `erDiagram` layout (design spec §6.4).
//!
//! An entity is a name over an attribute table, drawn with the shared compartment box
//! in [`record`](super::record); the crow's-foot cardinalities map onto the engine's
//! [`Terminator::CrowFoot`]. Layering, routing and fitting are the shared engine's job.
//!
//! ```text
//! ┌──────────────────────────┐
//! │         CUSTOMER         │
//! ├──────────────────────────┤
//! │ string  name        PK   │
//! │ string  email  "unique"  │
//! └──────────────────────────┘
//! ```
//!
//! The attribute table is column-aligned rather than a run of free text: type, name,
//! keys and comment each get their own column, sized to the widest entry in that
//! entity. A ragged attribute block is much harder to read, and design spec §6.4 calls
//! the block a table.

mod attribute;

use crate::canvas::Canvas;
use crate::error::MermaidError;
use crate::mermaid::ast::{Direction, Entity, ErCardinality, ErDiagram, ErRelationship, LineStyle};
use crate::text::wrap_plain;
use crate::theme::Theme;

use super::graph::{
    self, EdgeSpec, Fit, GraphSpec, GroupSpec, NodeArt, NodeIdx, Stroke, Terminator,
};
use super::record::{self, Row};

/// Widest a relationship label is allowed to get before it is wrapped.
const LABEL_WIDTH: usize = 18;

/// Draws a ER diagram into a canvas exactly `width` columns wide.
///
/// The engine may degrade the drawing as far as [`Fit::COMPACT`] allows; use
/// [`draw_with`] to say otherwise.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when the diagram cannot be made to fit.
pub fn draw(diagram: &ErDiagram, width: u16, theme: &Theme) -> Result<Canvas, MermaidError> {
    draw_with(diagram, width, theme, Fit::COMPACT)
}

/// Draws an ER diagram under the given fit policy.
///
/// # Errors
///
/// Returns [`MermaidError::TooNarrow`] when the diagram cannot be made to fit within
/// what `fit` allows.
pub fn draw_with(
    diagram: &ErDiagram,
    width: u16,
    theme: &Theme,
    fit: Fit,
) -> Result<Canvas, MermaidError> {
    let spec = build(diagram);
    graph::draw(&spec, &Art { diagram }, width, theme, fit)
}

/// Draws entity boxes for the engine.
struct Art<'a> {
    diagram: &'a ErDiagram,
}

impl NodeArt for Art<'_> {
    fn render(&self, node: NodeIdx, budget: u16, theme: &Theme) -> Canvas {
        match self.diagram.entities.get(node.0) {
            Some(entity) => entity_box(entity, budget, theme),
            None => Canvas::empty(0),
        }
    }
}

/// Draws one entity as a name compartment over its attribute table.
fn entity_box(entity: &Entity, budget: u16, theme: &Theme) -> Canvas {
    let styles = theme.diagram;
    let header = vec![Row::centred(display_name(entity), styles.node_text)];
    let attributes = attribute::table(&entity.attributes)
        .into_iter()
        .map(|text| Row::left(text, styles.node_text))
        .collect();
    record::draw(&[header, attributes], budget, theme)
}

/// The name shown in an entity box: the alias when the source gave one.
fn display_name(entity: &Entity) -> String {
    match entity.alias.as_deref().map(str::trim) {
        Some(alias) if !alias.is_empty() => alias.to_string(),
        _ => entity.name.clone(),
    }
}

/// Translates the ER AST into an engine specification.
///
/// ER diagrams have neither containers nor a direction statement, so every entity sits
/// in the root group and the diagram flows top to bottom.
fn build(diagram: &ErDiagram) -> GraphSpec {
    GraphSpec {
        direction: Direction::TopToBottom,
        node_count: diagram.entities.len(),
        edges: diagram.relationships.iter().map(relationship).collect(),
        root: GroupSpec {
            nodes: (0..diagram.entities.len()).map(NodeIdx).collect(),
            ..GroupSpec::default()
        },
    }
}

/// Translates one relationship, including its crow's feet and label.
fn relationship(relationship: &ErRelationship) -> EdgeSpec {
    EdgeSpec {
        from: NodeIdx(relationship.left.0),
        to: NodeIdx(relationship.right.0),
        stroke: match relationship.line {
            LineStyle::Solid => Stroke::Solid,
            LineStyle::Dashed => Stroke::Dotted,
        },
        tail: terminator(relationship.left_cardinality),
        head: terminator(relationship.right_cardinality),
        label: relationship
            .label
            .as_ref()
            .filter(|label| !label.is_empty())
            .map(|label| {
                label
                    .lines
                    .iter()
                    .flat_map(|line| wrap_plain(line, LABEL_WIDTH))
                    .collect()
            })
            .unwrap_or_default(),
        tail_label: None,
        head_label: None,
    }
}

/// Translates a crow's-foot cardinality.
///
/// The two bits Mermaid encodes are independent: the inner marker says whether the end
/// is optional (`o`) or mandatory (`|`), and the outer one whether it is many (`{`/`}`)
/// or one (`|`).
fn terminator(cardinality: ErCardinality) -> Terminator {
    match cardinality {
        ErCardinality::ZeroOrOne => Terminator::CrowFoot {
            many: false,
            optional: true,
        },
        ErCardinality::ExactlyOne => Terminator::CrowFoot {
            many: false,
            optional: false,
        },
        ErCardinality::ZeroOrMore => Terminator::CrowFoot {
            many: true,
            optional: true,
        },
        ErCardinality::OneOrMore => Terminator::CrowFoot {
            many: true,
            optional: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::ast::{EntityId, ErAttribute, ErKey, Label};

    fn entity(name: &str, attributes: Vec<ErAttribute>) -> Entity {
        Entity {
            name: name.to_string(),
            alias: None,
            attributes,
        }
    }

    fn attribute(ty: &str, name: &str, keys: Vec<ErKey>) -> ErAttribute {
        ErAttribute {
            ty: ty.to_string(),
            name: name.to_string(),
            keys,
            comment: None,
        }
    }

    #[test]
    fn an_entity_box_holds_its_attribute_table() {
        let theme = Theme::default_dark();
        let entity = entity(
            "CUSTOMER",
            vec![
                attribute("string", "name", vec![ErKey::Primary]),
                attribute("int", "age", Vec::new()),
            ],
        );
        let text = entity_box(&entity, 60, &theme).plain_text();
        assert!(text.contains("CUSTOMER"), "{text}");
        assert!(text.contains("PK"), "{text}");
        assert!(text.contains("string"), "{text}");
        assert_eq!(text.matches('├').count(), 1, "one rule: {text}");
    }

    #[test]
    fn an_entity_without_attributes_is_a_plain_name_box() {
        let theme = Theme::default_dark();
        let text = entity_box(&entity("PLAIN", Vec::new()), 40, &theme).plain_text();
        assert!(!text.contains('├'), "no rule expected: {text}");
    }

    #[test]
    fn an_alias_is_shown_instead_of_the_key() {
        let mut named = entity("CUSTOMER", Vec::new());
        named.alias = Some("Customer account".to_string());
        assert_eq!(display_name(&named), "Customer account");
        named.alias = Some("  ".to_string());
        assert_eq!(display_name(&named), "CUSTOMER");
    }

    #[test]
    fn every_cardinality_maps_to_its_crows_foot() {
        let cases = [
            (ErCardinality::ZeroOrOne, false, true),
            (ErCardinality::ExactlyOne, false, false),
            (ErCardinality::ZeroOrMore, true, true),
            (ErCardinality::OneOrMore, true, false),
        ];
        for (cardinality, many, optional) in cases {
            assert_eq!(
                terminator(cardinality),
                Terminator::CrowFoot { many, optional },
                "{cardinality:?}"
            );
        }
    }

    #[test]
    fn a_non_identifying_relationship_is_dotted() {
        let relation = ErRelationship {
            left: EntityId(0),
            right: EntityId(1),
            left_cardinality: ErCardinality::ExactlyOne,
            right_cardinality: ErCardinality::ZeroOrMore,
            line: LineStyle::Dashed,
            label: Some(Label::line("places")),
        };
        let spec = relationship(&relation);
        assert_eq!(spec.stroke, Stroke::Dotted);
        assert_eq!(spec.label, vec!["places".to_string()]);
        assert_eq!(
            spec.head,
            Terminator::CrowFoot {
                many: true,
                optional: true
            }
        );
    }

    #[test]
    fn every_entity_lands_in_the_root_group() {
        let diagram = ErDiagram {
            entities: vec![entity("A", Vec::new()), entity("B", Vec::new())],
            relationships: Vec::new(),
        };
        let spec = build(&diagram);
        assert_eq!(spec.root.nodes, vec![NodeIdx(0), NodeIdx(1)]);
    }
}
