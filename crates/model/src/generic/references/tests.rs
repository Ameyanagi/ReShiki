use super::*;
use crate::generic::drawing_test_support::union_source;
use crate::generic::probe;
use crate::{
    atom_text::{self, Mode},
    document::Point,
    generic::{self, Limits},
};

#[test]
fn generated_union_rejects_changed_component_dependencies_atomically() {
    let mut cases = Vec::new();
    let mut named = union_source();
    let caption = named.next_id();
    named.annotations.push(crate::document::Annotation {
        id: caption,
        position: Point::new(0., 100.),
        text: "propane".into(),
        format: Default::default(),
    });
    named
        .molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation: caption,
            atoms: vec![1, 2, 3],
            smiles: "CCC".into(),
            text: "propane".into(),
        });
    cases.push((named, "generated name captions"));
    let mut rear = union_source();
    rear.depth_appearance[0].atoms.push(1);
    rear.depth_appearance[0].weights.insert(1, 0.5);
    rear.depth_appearance[0].rear_weights.insert(1, 1.);
    cases.push((rear, "depth appearance scopes"));
    for target in [
        crate::arrow_anchors::Target::Atom { atom: 1 },
        crate::arrow_anchors::Target::Bond {
            a: 1,
            b: 2,
            fraction: 0.3,
        },
        crate::arrow_anchors::Target::LonePair { atom: 1, mark: 7 },
    ] {
        let mut linked = union_source();
        linked
            .atom_mut(1)
            .unwrap()
            .marks
            .push(crate::scientific::AtomMark {
                id: Some(7),
                kind: crate::scientific::MarkKind::LonePair,
                offset: Point::new(0., -20.),
                angle: 0.,
                size_pt: None,
            });
        linked.arrows[0].start_anchor.as_mut().unwrap().target = target;
        cases.push((linked, "attached arrows"));
    }
    // Alternatives change the whole component even when there is no depiction.
    let mut alternative = Document::default();
    let a = alternative.add_atom("C", Point::default());
    let b = alternative.add_atom("*", Point::new(42., 0.));
    alternative.add_bond(a, b, 1, "plain");
    alternative = atom_text::apply(&alternative, b, "[C,N]", Mode::Auto).unwrap();
    let caption = alternative.next_id();
    alternative.annotations.push(crate::document::Annotation {
        id: caption,
        position: Point::new(0., 70.),
        text: "inherited claim".into(),
        format: Default::default(),
    });
    alternative
        .molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation: caption,
            atoms: vec![a, b],
            smiles: "CC".into(),
            text: "inherited claim".into(),
        });
    cases.push((alternative, "generated name captions"));
    for (source, expected) in cases {
        let before = source.clone();
        source.validate().unwrap();
        probe::reset();
        assert!(
            generic::prepare_expansion(&source, Limits::default())
                .unwrap_err()
                .contains(expected)
        );
        assert!(probe::counts().iter().take(5).all(|count| *count == 0));
        assert_eq!(source, before);
    }
}
