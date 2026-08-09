//! Property tests for the class, ER and state renderers (design spec §13.3).
//!
//! The shared engine has its own property tests in `mermaid_layout_property.rs`; these
//! exercise the three *translations* on top of it — the part each family owns — with
//! arbitrary ASTs at arbitrary widths. Three properties hold for all of them:
//!
//! 1. rendering never panics, and a diagram that cannot fit is reported as
//!    [`MermaidError::TooNarrow`] rather than drawn over the edge;
//! 2. every row of the returned canvas is exactly `width` display columns;
//! 3. rendering is deterministic.

use mdmost::error::MermaidError;
use mdmost::mermaid::ast::{
    Class, ClassArrow, ClassDiagram, ClassId, ClassRelation, Entity, EntityId, ErAttribute,
    ErCardinality, ErDiagram, ErKey, ErRelationship, Field, LineStyle, Member, StateDiagram,
    StateEndpoint, StateId, StateKind, StateNode, StateScope, Transition, Visibility,
};
use mdmost::mermaid::layout::{class, er, state};
use mdmost::text::display_width;
use mdmost::theme::Theme;
use proptest::prelude::*;

/// Asserts the canvas contract on a rendered diagram, tolerating a width refusal.
fn check(
    drawn: Result<mdmost::canvas::Canvas, MermaidError>,
    width: u16,
) -> Result<(), TestCaseError> {
    let canvas = match drawn {
        Ok(canvas) => canvas,
        Err(error) => {
            prop_assert!(
                matches!(error, MermaidError::TooNarrow { .. }),
                "only a width refusal is acceptable, got {error:?}"
            );
            return Ok(());
        }
    };
    prop_assert_eq!(canvas.width(), width);
    prop_assert!(canvas.check_invariants().is_ok());
    for row in 0..canvas.height() {
        prop_assert_eq!(
            display_width(&canvas.row_text(row)),
            usize::from(width),
            "row {} is not exactly {} columns",
            row,
            width
        );
    }
    Ok(())
}

/// Builds a class diagram from a raw description.
fn class_diagram(count: usize, members: usize, pairs: &[(usize, usize, usize)]) -> ClassDiagram {
    let count = count.max(1);
    let classes = (0..count)
        .map(|at| Class {
            name: format!("C{at}"),
            generic: None,
            annotation: None,
            members: (0..members)
                .map(|m| {
                    Member::Field(Field {
                        visibility: Some(Visibility::Public),
                        name: format!("f{m}"),
                        ty: Some("int".to_string()),
                        classifier: None,
                    })
                })
                .collect(),
        })
        .collect();
    let arrows = [
        ClassArrow::None,
        ClassArrow::Triangle,
        ClassArrow::FilledDiamond,
        ClassArrow::HollowDiamond,
        ClassArrow::Arrow,
    ];
    let relations = pairs
        .iter()
        .map(|&(from, to, kind)| ClassRelation {
            left: ClassId(from % count),
            right: ClassId(to % count),
            left_end: arrows[kind % arrows.len()],
            right_end: ClassArrow::None,
            line: if kind % 2 == 0 {
                LineStyle::Solid
            } else {
                LineStyle::Dashed
            },
            left_cardinality: (kind % 3 == 0).then(|| "1".to_string()),
            right_cardinality: (kind % 3 == 0).then(|| "0..*".to_string()),
            label: None,
        })
        .collect();
    ClassDiagram {
        direction: None,
        classes,
        relations,
    }
}

/// Builds an ER diagram from a raw description.
fn er_diagram(count: usize, attributes: usize, pairs: &[(usize, usize, usize)]) -> ErDiagram {
    let count = count.max(1);
    let entities = (0..count)
        .map(|at| Entity {
            name: format!("E{at}"),
            alias: None,
            attributes: (0..attributes)
                .map(|a| ErAttribute {
                    ty: "string".to_string(),
                    name: format!("a{a}"),
                    keys: if a == 0 {
                        vec![ErKey::Primary]
                    } else {
                        Vec::new()
                    },
                    comment: None,
                })
                .collect(),
        })
        .collect();
    let cardinalities = [
        ErCardinality::ZeroOrOne,
        ErCardinality::ExactlyOne,
        ErCardinality::ZeroOrMore,
        ErCardinality::OneOrMore,
    ];
    let relationships = pairs
        .iter()
        .map(|&(from, to, kind)| ErRelationship {
            left: EntityId(from % count),
            right: EntityId(to % count),
            left_cardinality: cardinalities[kind % cardinalities.len()],
            right_cardinality: cardinalities[(kind + 1) % cardinalities.len()],
            line: if kind % 2 == 0 {
                LineStyle::Solid
            } else {
                LineStyle::Dashed
            },
            label: None,
        })
        .collect();
    ErDiagram {
        entities,
        relationships,
    }
}

/// Builds a state diagram, optionally wrapping the tail of the states in a composite.
fn state_diagram(
    count: usize,
    pairs: &[(usize, usize)],
    markers: bool,
    composite: bool,
) -> StateDiagram {
    let count = count.max(1);
    let kinds = [
        StateKind::Simple,
        StateKind::Choice,
        StateKind::Fork,
        StateKind::Join,
    ];
    let mut states: Vec<StateNode> = (0..count)
        .map(|at| StateNode {
            key: format!("S{at}"),
            label: None,
            kind: kinds[at % kinds.len()].clone(),
        })
        .collect();

    let mut transitions: Vec<Transition> = pairs
        .iter()
        .map(|&(from, to)| Transition {
            from: StateEndpoint::State(StateId(from % count)),
            to: StateEndpoint::State(StateId(to % count)),
            label: None,
        })
        .collect();
    if markers {
        transitions.push(Transition {
            from: StateEndpoint::Initial,
            to: StateEndpoint::State(StateId(0)),
            label: None,
        });
        transitions.push(Transition {
            from: StateEndpoint::State(StateId(count - 1)),
            to: StateEndpoint::Final,
            label: None,
        });
    }

    let mut root = StateScope {
        states: (0..count).map(StateId).collect(),
        transitions,
        ..StateScope::default()
    };

    if composite && count > 1 {
        // Move the last state into a composite so nesting is exercised.
        let inner = StateScope {
            states: vec![StateId(count - 1)],
            ..StateScope::default()
        };
        root.states.pop();
        let wrapper = StateId(states.len());
        states.push(StateNode {
            key: "Group".to_string(),
            label: None,
            kind: StateKind::Composite(inner),
        });
        root.states.push(wrapper);
    }

    StateDiagram {
        direction: None,
        states,
        root,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// A class diagram always honours the canvas contract.
    #[test]
    fn class_diagrams_keep_the_canvas_contract(
        count in 1usize..7,
        members in 0usize..4,
        pairs in prop::collection::vec((0usize..6, 0usize..6, 0usize..5), 0..10),
        width in 20u16..120,
    ) {
        let theme = Theme::default_dark();
        let diagram = class_diagram(count, members, &pairs);
        check(class::draw(&diagram, width, &theme), width)?;
    }

    /// An ER diagram always honours the canvas contract.
    #[test]
    fn er_diagrams_keep_the_canvas_contract(
        count in 1usize..7,
        attributes in 0usize..4,
        pairs in prop::collection::vec((0usize..6, 0usize..6, 0usize..4), 0..10),
        width in 20u16..120,
    ) {
        let theme = Theme::default_dark();
        let diagram = er_diagram(count, attributes, &pairs);
        check(er::draw(&diagram, width, &theme), width)?;
    }

    /// A state diagram always honours the canvas contract, nesting included.
    #[test]
    fn state_diagrams_keep_the_canvas_contract(
        count in 1usize..7,
        pairs in prop::collection::vec((0usize..6, 0usize..6), 0..10),
        markers in any::<bool>(),
        composite in any::<bool>(),
        width in 20u16..120,
    ) {
        let theme = Theme::default_dark();
        let diagram = state_diagram(count, &pairs, markers, composite);
        check(state::draw(&diagram, width, &theme), width)?;
    }

    /// The same diagram always renders to exactly the same canvas.
    #[test]
    fn rendering_is_deterministic(
        count in 1usize..6,
        pairs in prop::collection::vec((0usize..5, 0usize..5, 0usize..4), 0..8),
        width in 30u16..100,
    ) {
        let theme = Theme::default_dark();
        let classes = class_diagram(count, 2, &pairs);
        let entities = er_diagram(count, 2, &pairs);
        let states = state_diagram(count, &pairs.iter().map(|&(a, b, _)| (a, b)).collect::<Vec<_>>(), true, true);

        prop_assert_eq!(
            class::draw(&classes, width, &theme).map(|c| c.plain_text()).ok(),
            class::draw(&classes, width, &theme).map(|c| c.plain_text()).ok()
        );
        prop_assert_eq!(
            er::draw(&entities, width, &theme).map(|c| c.plain_text()).ok(),
            er::draw(&entities, width, &theme).map(|c| c.plain_text()).ok()
        );
        prop_assert_eq!(
            state::draw(&states, width, &theme).map(|c| c.plain_text()).ok(),
            state::draw(&states, width, &theme).map(|c| c.plain_text()).ok()
        );
    }

    /// Every class name reaches the canvas when the width is generous.
    #[test]
    fn no_class_is_silently_dropped(
        count in 1usize..6,
        pairs in prop::collection::vec((0usize..5, 0usize..5, 0usize..5), 0..8),
    ) {
        let theme = Theme::default_dark();
        let diagram = class_diagram(count, 1, &pairs);
        if let Ok(canvas) = class::draw(&diagram, 120, &theme) {
            let text = canvas.plain_text();
            for class in &diagram.classes {
                prop_assert!(text.contains(&class.name), "{} missing from\n{}", class.name, text);
            }
        }
    }

    /// Every entity name reaches the canvas when the width is generous.
    #[test]
    fn no_entity_is_silently_dropped(
        count in 1usize..6,
        pairs in prop::collection::vec((0usize..5, 0usize..5, 0usize..4), 0..8),
    ) {
        let theme = Theme::default_dark();
        let diagram = er_diagram(count, 1, &pairs);
        if let Ok(canvas) = er::draw(&diagram, 120, &theme) {
            let text = canvas.plain_text();
            for entity in &diagram.entities {
                prop_assert!(text.contains(&entity.name), "{} missing from\n{}", entity.name, text);
            }
        }
    }
}
