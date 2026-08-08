//! Snapshot tests of class-diagram layout (design spec §6.3).
//!
//! Every case is a hand-built [`ClassDiagram`], so a change in `mermaid::parse` can
//! never silently rewrite what these snapshots check.

use mdless::mermaid::ast::{
    Class, ClassAnnotation, ClassArrow, ClassDiagram, ClassId, ClassRelation, Classifier,
    Direction, Field, Label, LineStyle, Member, Method, Param, Visibility,
};
use mdless::mermaid::layout::class;
use mdless::theme::Theme;

/// A class with a name and members.
fn class(name: &str, members: Vec<Member>) -> Class {
    Class {
        name: name.to_string(),
        generic: None,
        annotation: None,
        members,
    }
}

/// A public field `+name: ty`.
fn field(name: &str, ty: &str) -> Member {
    Member::Field(Field {
        visibility: Some(Visibility::Public),
        name: name.to_string(),
        ty: Some(ty.to_string()),
        classifier: None,
    })
}

/// A public method with no parameters.
fn method(name: &str, returns: &str) -> Member {
    Member::Method(Method {
        visibility: Some(Visibility::Public),
        name: name.to_string(),
        params: Vec::new(),
        returns: Some(returns.to_string()),
        classifier: None,
    })
}

/// A relation with the given ends and line style.
fn relation(
    left: usize,
    right: usize,
    left_end: ClassArrow,
    right_end: ClassArrow,
    line: LineStyle,
) -> ClassRelation {
    ClassRelation {
        left: ClassId(left),
        right: ClassId(right),
        left_end,
        right_end,
        line,
        left_cardinality: None,
        right_cardinality: None,
        label: None,
    }
}

/// Renders a diagram to plain text for snapshotting.
fn render(diagram: &ClassDiagram, width: u16) -> String {
    let theme = Theme::default_dark();
    let canvas = class::draw(diagram, width, &theme).expect("diagram fits");
    assert_eq!(canvas.width(), width, "canvas is exactly the width budget");
    canvas.check_invariants().expect("canvas contract holds");
    canvas.plain_text()
}

/// A diagram holding `classes` joined by `relations`.
fn diagram(classes: Vec<Class>, relations: Vec<ClassRelation>) -> ClassDiagram {
    ClassDiagram {
        direction: None,
        classes,
        relations,
    }
}

#[test]
fn a_class_with_every_visibility_marker() {
    let members = vec![
        Member::Field(Field {
            visibility: Some(Visibility::Public),
            name: "id".to_string(),
            ty: Some("int".to_string()),
            classifier: None,
        }),
        Member::Field(Field {
            visibility: Some(Visibility::Private),
            name: "secret".to_string(),
            ty: Some("String".to_string()),
            classifier: None,
        }),
        Member::Field(Field {
            visibility: Some(Visibility::Protected),
            name: "kin".to_string(),
            ty: None,
            classifier: None,
        }),
        Member::Field(Field {
            visibility: Some(Visibility::PackageInternal),
            name: "shared".to_string(),
            ty: Some("bool".to_string()),
            classifier: Some(Classifier::Static),
        }),
        Member::Method(Method {
            visibility: Some(Visibility::Public),
            name: "rename".to_string(),
            params: vec![Param {
                name: "to".to_string(),
                ty: Some("String".to_string()),
            }],
            returns: Some("void".to_string()),
            classifier: Some(Classifier::Abstract),
        }),
    ];
    let chart = diagram(vec![class("Animal", members)], Vec::new());
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn inheritance_puts_the_parent_on_top() {
    let chart = diagram(
        vec![
            class("Animal", vec![method("isMammal", "bool")]),
            class("Duck", vec![field("beak", "Beak")]),
        ],
        vec![relation(
            0,
            1,
            ClassArrow::Triangle,
            ClassArrow::None,
            LineStyle::Solid,
        )],
    );
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn every_relation_kind() {
    let classes = vec![
        class("Root", Vec::new()),
        class("Inherit", Vec::new()),
        class("Compose", Vec::new()),
        class("Aggregate", Vec::new()),
        class("Associate", Vec::new()),
        class("Depend", Vec::new()),
    ];
    let relations = vec![
        relation(
            0,
            1,
            ClassArrow::Triangle,
            ClassArrow::None,
            LineStyle::Solid,
        ),
        relation(
            0,
            2,
            ClassArrow::FilledDiamond,
            ClassArrow::None,
            LineStyle::Solid,
        ),
        relation(
            0,
            3,
            ClassArrow::HollowDiamond,
            ClassArrow::None,
            LineStyle::Solid,
        ),
        relation(0, 4, ClassArrow::None, ClassArrow::Arrow, LineStyle::Solid),
        relation(0, 5, ClassArrow::None, ClassArrow::Arrow, LineStyle::Dashed),
    ];
    insta::assert_snapshot!(render(&diagram(classes, relations), 90));
}

#[test]
fn cardinalities_sit_at_their_own_end() {
    let mut link = relation(0, 1, ClassArrow::None, ClassArrow::Arrow, LineStyle::Solid);
    link.left_cardinality = Some("1".to_string());
    link.right_cardinality = Some("0..*".to_string());
    link.label = Some(Label::line("places"));
    let chart = diagram(
        vec![
            class("Customer", vec![field("name", "String")]),
            class("Order", vec![field("total", "Money")]),
        ],
        vec![link],
    );
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn an_interface_and_its_realization() {
    let mut shape = class("Shape", vec![method("area", "float")]);
    shape.annotation = Some(ClassAnnotation::Interface);
    let mut abstract_base = class("Base", Vec::new());
    abstract_base.annotation = Some(ClassAnnotation::Abstract);
    let chart = diagram(
        vec![shape, abstract_base, class("Circle", Vec::new())],
        vec![
            relation(
                0,
                2,
                ClassArrow::Triangle,
                ClassArrow::None,
                LineStyle::Dashed,
            ),
            relation(
                1,
                2,
                ClassArrow::Triangle,
                ClassArrow::None,
                LineStyle::Solid,
            ),
        ],
    );
    insta::assert_snapshot!(render(&chart, 70));
}

#[test]
fn a_generic_class() {
    let mut square = class("Square", vec![field("side", "int")]);
    square.generic = Some("Shape".to_string());
    insta::assert_snapshot!(render(&diagram(vec![square], Vec::new()), 40));
}

#[test]
fn a_single_class_with_nothing_in_it() {
    insta::assert_snapshot!(render(
        &diagram(vec![class("Empty", Vec::new())], Vec::new()),
        30
    ));
}

#[test]
fn a_diagram_with_no_classes_at_all() {
    insta::assert_snapshot!(render(&diagram(Vec::new(), Vec::new()), 30));
}

#[test]
fn a_left_to_right_diagram() {
    let mut chart = diagram(
        vec![
            class("A", vec![field("x", "int")]),
            class("B", vec![field("y", "int")]),
        ],
        vec![relation(
            0,
            1,
            ClassArrow::None,
            ClassArrow::Arrow,
            LineStyle::Solid,
        )],
    );
    chart.direction = Some(Direction::LeftToRight);
    insta::assert_snapshot!(render(&chart, 60));
}

#[test]
fn long_member_signatures_are_elided_rather_than_overflowing() {
    let members = vec![
        field(
            "aVeryLongFieldNameIndeed",
            "SomeExtremelyLongTypeNameThatGoesOn",
        ),
        method("anotherRatherLongMethodName", "AnotherLongReturnType"),
    ];
    insta::assert_snapshot!(render(
        &diagram(vec![class("Wide", members)], Vec::new()),
        40
    ));
}

#[test]
fn cjk_and_emoji_members_keep_the_box_square() {
    let members = vec![field("名前", "文字列"), method("走る🏃", "真偽")];
    insta::assert_snapshot!(render(
        &diagram(vec![class("動物", members)], Vec::new()),
        40
    ));
}

#[test]
fn a_cycle_of_relations_still_lays_out() {
    let chart = diagram(
        vec![
            class("A", Vec::new()),
            class("B", Vec::new()),
            class("C", Vec::new()),
        ],
        vec![
            relation(0, 1, ClassArrow::None, ClassArrow::Arrow, LineStyle::Solid),
            relation(1, 2, ClassArrow::None, ClassArrow::Arrow, LineStyle::Solid),
            relation(2, 0, ClassArrow::None, ClassArrow::Arrow, LineStyle::Solid),
        ],
    );
    insta::assert_snapshot!(render(&chart, 60));
}
