//! Snapshot tests of ER-diagram layout (design spec §6.4).
//!
//! Every case is a hand-built [`ErDiagram`], so a change in `mermaid::parse` can never
//! silently rewrite what these snapshots check.

use mdless::mermaid::ast::{
    Entity, EntityId, ErAttribute, ErCardinality, ErDiagram, ErKey, ErRelationship, Label,
    LineStyle,
};
use mdless::mermaid::layout::er;
use mdless::theme::Theme;

/// An entity with a name and attributes.
fn entity(name: &str, attributes: Vec<ErAttribute>) -> Entity {
    Entity {
        name: name.to_string(),
        alias: None,
        attributes,
    }
}

/// An attribute with a type, a name and key markers.
fn attribute(ty: &str, name: &str, keys: Vec<ErKey>) -> ErAttribute {
    ErAttribute {
        ty: ty.to_string(),
        name: name.to_string(),
        keys,
        comment: None,
    }
}

/// A relationship with the given cardinalities and a label.
fn relationship(
    left: usize,
    right: usize,
    left_cardinality: ErCardinality,
    right_cardinality: ErCardinality,
    label: &str,
) -> ErRelationship {
    ErRelationship {
        left: EntityId(left),
        right: EntityId(right),
        left_cardinality,
        right_cardinality,
        line: LineStyle::Solid,
        label: Some(Label::line(label)),
    }
}

/// Renders a diagram to plain text for snapshotting.
fn render(diagram: &ErDiagram, width: u16) -> String {
    let theme = Theme::default_dark();
    let canvas = er::draw(diagram, width, &theme).expect("diagram fits");
    assert_eq!(canvas.width(), width, "canvas is exactly the width budget");
    canvas.check_invariants().expect("canvas contract holds");
    canvas.plain_text()
}

/// A diagram holding `entities` joined by `relationships`.
fn diagram(entities: Vec<Entity>, relationships: Vec<ErRelationship>) -> ErDiagram {
    ErDiagram {
        entities,
        relationships,
    }
}

#[test]
fn an_entity_with_a_full_attribute_table() {
    let customer = entity(
        "CUSTOMER",
        vec![
            attribute("string", "id", vec![ErKey::Primary]),
            attribute("string", "name", Vec::new()),
            ErAttribute {
                ty: "string".to_string(),
                name: "email".to_string(),
                keys: vec![ErKey::Unique],
                comment: Some("login".to_string()),
            },
            attribute("int", "regionId", vec![ErKey::Foreign]),
        ],
    );
    insta::assert_snapshot!(render(&diagram(vec![customer], Vec::new()), 70));
}

#[test]
fn the_classic_customer_places_orders() {
    let chart = diagram(
        vec![
            entity(
                "CUSTOMER",
                vec![attribute("string", "name", vec![ErKey::Primary])],
            ),
            entity(
                "ORDER",
                vec![attribute("int", "total", vec![ErKey::Primary])],
            ),
        ],
        vec![relationship(
            0,
            1,
            ErCardinality::ExactlyOne,
            ErCardinality::ZeroOrMore,
            "places",
        )],
    );
    insta::assert_snapshot!(render(&chart, 70));
}

#[test]
fn every_cardinality_combination() {
    let entities = vec![
        entity("HUB", Vec::new()),
        entity("ZERO_OR_ONE", Vec::new()),
        entity("EXACTLY_ONE", Vec::new()),
        entity("ZERO_OR_MORE", Vec::new()),
        entity("ONE_OR_MORE", Vec::new()),
    ];
    let relationships = vec![
        relationship(
            0,
            1,
            ErCardinality::ExactlyOne,
            ErCardinality::ZeroOrOne,
            "",
        ),
        relationship(
            0,
            2,
            ErCardinality::ZeroOrOne,
            ErCardinality::ExactlyOne,
            "",
        ),
        relationship(
            0,
            3,
            ErCardinality::OneOrMore,
            ErCardinality::ZeroOrMore,
            "",
        ),
        relationship(
            0,
            4,
            ErCardinality::ZeroOrMore,
            ErCardinality::OneOrMore,
            "",
        ),
    ];
    insta::assert_snapshot!(render(&diagram(entities, relationships), 100));
}

#[test]
fn a_non_identifying_relationship_is_drawn_dotted() {
    let mut link = relationship(
        0,
        1,
        ErCardinality::ExactlyOne,
        ErCardinality::ZeroOrMore,
        "may have",
    );
    link.line = LineStyle::Dashed;
    let chart = diagram(
        vec![entity("PARENT", Vec::new()), entity("CHILD", Vec::new())],
        vec![link],
    );
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn an_alias_is_shown_instead_of_the_entity_key() {
    let mut named = entity("CUSTOMER", vec![attribute("string", "name", Vec::new())]);
    named.alias = Some("Customer account".to_string());
    insta::assert_snapshot!(render(&diagram(vec![named], Vec::new()), 50));
}

#[test]
fn an_entity_with_no_attributes() {
    insta::assert_snapshot!(render(
        &diagram(vec![entity("PLAIN", Vec::new())], Vec::new()),
        30
    ));
}

#[test]
fn a_diagram_with_no_entities_at_all() {
    insta::assert_snapshot!(render(&diagram(Vec::new(), Vec::new()), 30));
}

#[test]
fn a_three_entity_chain() {
    let chart = diagram(
        vec![
            entity("CUSTOMER", vec![attribute("string", "name", Vec::new())]),
            entity("ORDER", vec![attribute("int", "id", vec![ErKey::Primary])]),
            entity("LINE_ITEM", vec![attribute("int", "qty", Vec::new())]),
        ],
        vec![
            relationship(
                0,
                1,
                ErCardinality::ExactlyOne,
                ErCardinality::ZeroOrMore,
                "places",
            ),
            relationship(
                1,
                2,
                ErCardinality::ExactlyOne,
                ErCardinality::OneOrMore,
                "contains",
            ),
        ],
    );
    insta::assert_snapshot!(render(&chart, 80));
}

#[test]
fn long_attribute_rows_are_elided_rather_than_overflowing() {
    let wide = entity(
        "WIDE",
        vec![attribute(
            "averyLongTypeNameIndeed",
            "anEvenLongerAttributeName",
            vec![ErKey::Primary, ErKey::Foreign],
        )],
    );
    insta::assert_snapshot!(render(&diagram(vec![wide], Vec::new()), 40));
}

#[test]
fn cjk_attributes_keep_the_table_aligned() {
    let chart = diagram(
        vec![entity(
            "顧客",
            vec![
                attribute("文字列", "名前", vec![ErKey::Primary]),
                attribute("int", "年齢", Vec::new()),
            ],
        )],
        Vec::new(),
    );
    insta::assert_snapshot!(render(&chart, 50));
}
