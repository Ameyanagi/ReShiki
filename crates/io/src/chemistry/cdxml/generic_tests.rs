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
fn import(xml: &str) -> Document {
    import_cdxml(xml).unwrap().document
}
fn xml(d: &Document) -> String {
    let request = crate::engine::Request::molecule("export", d.clone());
    crate::exchange::drawing::write(d, (&request).into()).unwrap()
}
#[test]
fn anonymous_mixed_is_typed_and_interchange_expands_real_groups() {
    let d = import(include_str!(
        "../../../../../tests/fixtures/generic-structures/anonymous-mixed.cdxml"
    ));
    assert_eq!(d.generic.alternatives.len(), 1);
    assert_eq!(
        crate::generic::expand(&d, Default::default())
            .unwrap()
            .len(),
        2
    );
    let x = xml(&d);
    assert!(x.contains("AnonymousAlternativeGroup"));
    let copy = import(&x);
    assert_eq!(copy.generic.alternatives, d.generic.alternatives);
    let bytes = crate::exchange::to_cdx(&x).unwrap();
    let decoded = crate::exchange::from_cdx(&bytes).unwrap();
    assert_eq!(
        import(&decoded).generic.alternatives,
        d.generic.alternatives
    );
}

#[test]
fn native_chemdraw26_anonymous_products_match_concrete_rust_expansion() {
    let native = include_str!(
        "../../../../../tests/fixtures/generic-structures/chemdraw26-anonymous-ome-oph.cdxml"
    );
    let source = import(native);
    assert_eq!(source.atoms.len(), 2);
    assert_eq!(source.bonds.len(), 1);
    assert_eq!(source.generic.alternatives.len(), 1);
    let before = source.clone();
    let products = crate::generic::expand(&source, Default::default()).unwrap();
    assert_eq!(source, before);
    assert_eq!(
        products.iter().map(|d| d.atoms.len()).collect::<Vec<_>>(),
        vec![3, 8]
    );
    let identity = |d: &Document| {
        let molecule = crate::chemistry::document::prepare(d).unwrap();
        crate::chemistry::smiles::write::write(&molecule.state, Default::default())
            .unwrap()
            .text
    };
    let mut actual = products.iter().map(identity).collect::<Vec<_>>();
    actual.sort();
    let native_products = import(include_str!(
        "../../../../../tests/fixtures/generic-structures/chemdraw26-anonymous-ome-oph-expanded.cdxml"
    ));
    let reference = identity(&native_products);
    let mut reference = reference.split('.').map(str::to_owned).collect::<Vec<_>>();
    reference.sort();
    assert_eq!(actual, reference);
    // Semantic GenericNickname survives independently of decorative atom text.
    assert_eq!(
        import(&native.replace(">[OMe,OPh]</s>", ">choices</s>"))
            .generic
            .alternatives,
        source.generic.alternatives
    );
    let copy = import(
        &crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml(&source)).unwrap()).unwrap(),
    );
    assert_eq!(copy.generic.alternatives, source.generic.alternatives);
}
#[test]
fn element_lists_and_not_lists_use_signed_binary_counts() {
    for label in ["[Cl,Br,I]", "[NOT Cl,Br,I]"] {
        let d = crate::atom_text::apply(&chain(), 1, label, crate::atom_text::Mode::Auto).unwrap();
        let x = xml(&d);
        let bytes = crate::exchange::to_cdx(&x).unwrap();
        let decoded = crate::exchange::from_cdx(&bytes).unwrap();
        let read = import(&decoded);
        assert_eq!(
            read.generic.alternatives[0].choices,
            d.generic.alternatives[0].choices
        );
        assert_eq!(
            read.generic.alternatives[0].excluded,
            d.generic.alternatives[0].excluded
        );
    }
}
#[test]
fn link_chain_retains_bounds_in_cdxml_and_cdx() {
    let d = import(include_str!(
        "../../../../../tests/fixtures/generic-structures/link-chain.cdxml"
    ));
    assert_eq!(
        crate::generic::expand(&d, Default::default())
            .unwrap()
            .iter()
            .map(|d| d.atoms.len())
            .collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
    let x = xml(&d);
    let bytes = crate::exchange::to_cdx(&x).unwrap();
    let read = import(&crate::exchange::from_cdx(&bytes).unwrap());
    assert_eq!(read.generic.links, d.generic.links);
}
#[test]
fn polymer_members_and_crossings_round_trip_then_expand() {
    let mut d = chain();
    let bracket = d.next_id();
    d.graphics.push(crate::graphics::Graphic::dragged(
        bracket,
        crate::graphics::GraphicKind::Brackets,
        Point::new(20., -30.),
        Point::new(104., 30.),
        Default::default(),
        crate::graphics::BracketSides::Both,
        false,
    ));
    crate::generic::set_repeat_unit(
        &mut d,
        RepeatUnit {
            bracket: Some(bracket),
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
            count: RepeatCount::Finite { min: 1, max: 3 },
            label: "1-3".into(),
            flip: "Unspecified".into(),
        },
    )
    .unwrap();
    let x = xml(&d);
    assert!(x.contains("bracketedgroup"));
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&x).unwrap()).unwrap();
    let read = import(&decoded);
    assert_eq!(
        read.generic.repeats[0].members,
        d.generic.repeats[0].members
    );
    assert_eq!(
        read.generic.repeats[0].crossings,
        d.generic.repeats[0].crossings
    );
    assert_eq!(read.generic.repeats[0].count, d.generic.repeats[0].count);
    assert_eq!(
        crate::generic::expand(&read, Default::default())
            .unwrap()
            .iter()
            .map(|d| d.atoms.len())
            .collect::<Vec<_>>(),
        vec![4, 6, 8]
    );
}

#[test]
fn malformed_list_link_and_crossing_references_publish_no_document() {
    let list =
        include_str!("../../../../../tests/fixtures/generic-structures/anonymous-mixed.cdxml");
    for xml in [
        list.replace("[OMe,OPh]", "[OMe,,OPh]"),
        list.replace("AnonymousAlternativeGroup", "ElementList")
            .replace("Element=\"0\"", "Element=\"0\" ElementList=\"999\""),
        include_str!("../../../../../tests/fixtures/generic-structures/link-chain.cdxml")
            .replace("LinkCountLow=\"1\"", "LinkCountLow=\"0\""),
    ] {
        assert!(import_cdxml(&xml).is_err());
    }
}

#[test]
fn ordinary_bonds_without_ids_keep_the_existing_import_contract() {
    let x = r#"<CDXML BondLength="14.4"><page><fragment id="1"><n id="2" p="0 0"/><n id="3" p="42 0"/><b B="2" E="3"/></fragment></page></CDXML>"#;
    let d = import(x);
    assert_eq!(d.atoms.len(), 2);
    assert_eq!(d.bonds.len(), 1);
    assert!(d.generic.is_empty());
    let x = r#"<CDXML BondLength="14.4"><page><fragment id="1"><n id="2" p="0 0"/></fragment><fragment id="3"><n id="2" p="42 0"/></fragment></page></CDXML>"#;
    assert_eq!(import(x).atoms.len(), 2);
}

#[test]
fn explicit_anonymous_fragments_are_verified_and_richer_queries_reject() {
    let list =
        include_str!("../../../../../tests/fixtures/generic-structures/anonymous-mixed.cdxml");
    let native_style = list.replace(
        "NodeType=\"ExternalConnectionPoint\" Element=\"0\"",
        "NodeType=\"ExternalConnectionPoint\"",
    );
    assert_eq!(
        import(&native_style).generic.alternatives,
        import(list).generic.alternatives
    );
    assert!(
        import_cdxml(&list.replace(
            "id=\"9\" p=\"28.8 0\" Element=\"6\"",
            "id=\"9\" p=\"28.8 0\" Element=\"7\""
        ))
        .is_err()
    );
    let plain = r#"<CDXML><page><fragment id="1"><n id="2" p="0 0" NodeType="ElementList" ElementList="6 7" GenericList="R"/></fragment></page></CDXML>"#;
    assert!(import_cdxml(plain).is_err());
    let link = include_str!("../../../../../tests/fixtures/generic-structures/link-chain.cdxml");
    assert!(
        import_cdxml(&link.replace(
            "NodeType=\"LinkNode\"",
            "NodeType=\"LinkNode\" GenericNickname=\"Q\""
        ))
        .is_err()
    );
}

#[test]
fn bracket_crossing_associations_survive_cdxml_and_cdx() {
    let mut d = chain();
    for id in [5, 6] {
        d.graphics.push(crate::graphics::Graphic::dragged(
            id,
            crate::graphics::GraphicKind::Brackets,
            Point::new(20. + (id - 5) as f32 * 4., -30.),
            Point::new(104. + (id - 5) as f32 * 4., 30.),
            Default::default(),
            crate::graphics::BracketSides::Both,
            false,
        ));
    }
    d.generic.repeats.push(RepeatUnit {
        bracket: Some(5),
        bracket_objects: vec![6],
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
        crossing_brackets: vec![5, 6],
        head_count: Some(1),
        usage: "SRU".into(),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min: 1, max: 3 },
        label: "1-3".into(),
        flip: "Unspecified".into(),
    });
    let x = xml(&d);
    let copy = import(&crate::exchange::from_cdx(&crate::exchange::to_cdx(&x).unwrap()).unwrap());
    let unit = &copy.generic.repeats[0];
    assert_eq!(
        unit.crossing_brackets,
        vec![unit.bracket.unwrap(), unit.bracket_objects[0]]
    );
    assert_eq!(unit.crossings, d.generic.repeats[0].crossings);
    assert!(
        import_cdxml(&x.replace(
            "BracketUsage=\"SRU\"",
            "BracketUsage=\"SRU\" ComponentOrder=\"2\""
        ))
        .is_err()
    );
    d.generic.repeats[0].label = "n".into();
    let request = crate::engine::Request::molecule("export", d.clone());
    assert!(
        crate::exchange::drawing::write(&d, (&request).into())
            .unwrap_err()
            .to_string()
            .contains("typed count")
    );
}

#[test]
fn alternative_export_uses_typed_choices_and_rejects_unrepresentable_isotopes() {
    let mut d =
        crate::atom_text::apply(&chain(), 1, "[OMe,OPh]", crate::atom_text::Mode::Auto).unwrap();
    d.atom_mut(1).unwrap().display.variable = Some("[Cl,Br]".into());
    assert_eq!(
        import(&xml(&d)).generic.alternatives,
        d.generic.alternatives
    );
    d.generic.alternatives[0].choices = vec![
        crate::generic::Alternative::Element {
            symbol: "C".into(),
            isotope: 13,
        },
        crate::generic::Alternative::Element {
            symbol: "N".into(),
            isotope: 0,
        },
    ];
    let request = crate::engine::Request::molecule("export", d.clone());
    assert!(
        crate::exchange::drawing::write(&d, (&request).into())
            .unwrap_err()
            .to_string()
            .contains("Isotopic alternatives")
    );
    d.generic.alternatives[0].choices = vec![
        crate::generic::Alternative::Element {
            symbol: "H".into(),
            isotope: 2,
        },
        crate::generic::Alternative::Element {
            symbol: "H".into(),
            isotope: 3,
        },
    ];
    assert_eq!(
        import(&xml(&d)).generic.alternatives,
        d.generic.alternatives
    );
}
