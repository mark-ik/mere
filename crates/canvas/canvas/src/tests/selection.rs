use crate::Canvas;
use kernel::graph::{RelationKind, RelationSelector, SemanticSubKind};

#[test]
fn member_addressed_relation_editing_targets_one_relation_cell() {
    let mut canvas = Canvas::new();
    let a = canvas.open_member_as_new_node(None, "https://a.test");
    let b = canvas.open_member_as_new_node(None, "https://b.test");

    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::Cites));
    assert!(canvas.assert_relation_between_members(a, b, SemanticSubKind::Quotes));
    assert_eq!(
        canvas.graph().relations().count(),
        2,
        "the directed bundle carries both relation cells",
    );

    assert_eq!(
        canvas.retract_relation_between_members(
            a,
            b,
            RelationSelector::Semantic(SemanticSubKind::Cites),
        ),
        1,
        "only the selected relation cell is removed",
    );
    let remaining: Vec<_> = canvas.graph().relations().map(|r| r.kind).collect();
    assert_eq!(
        remaining,
        vec![RelationKind::Semantic(SemanticSubKind::Quotes)]
    );
}
