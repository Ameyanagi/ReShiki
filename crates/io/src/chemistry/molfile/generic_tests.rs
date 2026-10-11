use super::*;
use crate::{
    document::{Document, Point},
    generic::{Crossing, RepeatCount, RepeatPattern, RepeatUnit},
};
fn chain() -> Document {
    let mut d = Document::default();
    for (i, e) in ["O", "C", "C", "N"].iter().enumerate() {
        let id = d.add_atom(e, Point::new(i as f32 * 42., 0.));
        if id > 1 {
            d.add_bond(id - 1, id, 1, "plain");
        }
    }
    d
}
fn roundtrip(d: &Document) -> Document {
    let output = write_document(d).unwrap();
    assert!(output.contains("V3000"));
    let imported = read(&output).unwrap();
    let drawing = imported.drawing().unwrap();
    let labels = drawing.labels().unwrap();
    drawing.finish(labels).unwrap()
}
#[test]
fn v3000_atom_list_and_not_list_are_retained() {
    for label in ["[Cl,Br,I]", "[NOT Cl,Br,I]"] {
        let source =
            crate::atom_text::apply(&chain(), 1, label, crate::atom_text::Mode::Auto).unwrap();
        let read = roundtrip(&source);
        assert_eq!(read.generic.alternatives, source.generic.alternatives);
        assert!(crate::chemistry::document::prepare(&read).is_err());
    }
}
#[test]
fn v3000_link_node_round_trip_expands_lengths() {
    let source =
        crate::atom_text::apply(&chain(), 2, "[CH2]1-3", crate::atom_text::Mode::Auto).unwrap();
    let read = roundtrip(&source);
    assert_eq!(read.generic.links, source.generic.links);
    assert_eq!(
        crate::generic::expand(&read, Default::default())
            .unwrap()
            .iter()
            .map(|d| d.atoms.len())
            .collect::<Vec<_>>(),
        vec![4, 5, 6]
    );
}
#[test]
fn v3000_sru_preserves_members_crossings_and_indefinite_count() {
    for label in ["1-3", "n"] {
        let mut source = chain();
        crate::generic::set_repeat_unit(
            &mut source,
            RepeatUnit {
                bracket: None,
                bracket_objects: vec![],
                members: vec![2, 3],
                crossings: vec![
                    Crossing {
                        inside: 2,
                        outside: 1,
                    },
                    Crossing {
                        inside: 3,
                        outside: 4,
                    },
                ],
                usage: "SRU".into(),
                crossing_brackets: vec![],
                head_count: Some(1),
                pattern: RepeatPattern::HeadToTail,
                count: RepeatCount::parse(label).unwrap(),
                label: label.into(),
                flip: "Unspecified".into(),
            },
        )
        .unwrap();
        let read = roundtrip(&source);
        assert_eq!(
            read.generic.repeats[0].members,
            source.generic.repeats[0].members
        );
        assert_eq!(
            read.generic.repeats[0].crossings,
            source.generic.repeats[0].crossings
        );
        assert_eq!(
            read.generic.repeats[0].count,
            source.generic.repeats[0].count
        );
        if label == "n" {
            assert!(crate::generic::expand(&read, Default::default()).is_err());
        } else {
            assert_eq!(
                crate::generic::expand(&read, Default::default())
                    .unwrap()
                    .len(),
                3
            );
        }
    }
}

#[test]
fn malformed_linknode_and_nonrepresentable_fragments_are_explicit_errors() {
    let source =
        crate::atom_text::apply(&chain(), 2, "[CH2]1-3", crate::atom_text::Mode::Auto).unwrap();
    let output = write_document(&source).unwrap();
    assert!(read(&output.replace("LINKNODE 1 3 2 2 1 2 3", "LINKNODE 1 3 2 2 1 3 4")).is_err());
    let source =
        crate::atom_text::apply(&chain(), 1, "[OMe,OPh]", crate::atom_text::Mode::Auto).unwrap();
    assert!(
        write_document(&source)
            .unwrap_err()
            .to_string()
            .contains("Anonymous fragment alternatives")
    );
}

#[test]
fn v3000_preserves_absent_and_multi_head_assignments_and_multiple_group_counts() {
    let mut source = chain();
    let unit = RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
        members: vec![2, 3],
        crossings: vec![
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
        ],
        crossing_brackets: vec![],
        head_count: None,
        usage: "SRU".into(),
        pattern: RepeatPattern::EitherUnknown,
        count: RepeatCount::Indefinite { label: "n".into() },
        label: "n".into(),
        flip: "Unspecified".into(),
    };
    source.generic.repeats.push(unit);
    let output = write_document(&source).unwrap();
    assert!(output.contains("XBONDS=(2 1 3)"));
    assert!(!output.contains("XBHEAD="));
    assert_eq!(roundtrip(&source).generic.repeats[0].head_count, None);
    let imported = read(&output.replace(" CONNECT=EU", "")).unwrap();
    assert_eq!(
        imported.annotations.generic.repeats[0].pattern,
        RepeatPattern::EitherUnknown
    );
    source.generic.repeats[0].head_count = Some(2);
    assert!(write_document(&source).unwrap().contains("XBHEAD=(2 1 3)"));
    assert_eq!(roundtrip(&source).generic.repeats[0].head_count, Some(2));
    source.generic.repeats[0].usage = "MultipleGroup".into();
    source.generic.repeats[0].count = RepeatCount::Finite { min: 3, max: 3 };
    let output = write_document(&source).unwrap();
    assert!(output.contains("MULT=3"));
    assert_eq!(
        roundtrip(&source).generic.repeats[0].count,
        RepeatCount::Finite { min: 3, max: 3 }
    );
    assert!(crate::generic::expand(&source, Default::default()).is_err());
}

#[test]
fn unretained_polymer_relationships_and_count_label_mismatch_reject() {
    let mut source = chain();
    source.generic.repeats.push(RepeatUnit {
        bracket: None,
        bracket_objects: vec![],
        members: vec![2, 3],
        crossings: vec![
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
        ],
        crossing_brackets: vec![],
        head_count: Some(1),
        usage: "SRU".into(),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min: 1, max: 3 },
        label: "1-3".into(),
        flip: "Unspecified".into(),
    });
    let output = write_document(&source).unwrap();
    for property in ["XBCORR=(2 1 3)", "PARENT=2", "COMPNO=2", "PATOMS=(1 2)"] {
        assert!(
            read(&output.replace("LABEL=\"1-3\"", &format!("LABEL=\"1-3\" {property}"))).is_err()
        );
    }
    source.generic.repeats[0].label = "n".into();
    assert!(
        write_document(&source)
            .unwrap_err()
            .to_string()
            .contains("typed repeat count")
    );
}
