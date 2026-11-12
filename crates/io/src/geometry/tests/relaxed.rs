use super::*;

fn pair(left: &str, right: &str, order: u8) -> Document {
    let mut document = Document::default();
    let a = document.add_atom(left, Point::new(0., 0.));
    let b = document.add_atom(right, Point::new(42., 0.));
    document.add_bond(a, b, order, "plain");
    document
}

fn tetrahedral() -> Document {
    let mut document = Document::default();
    let center = document.add_atom("C", Point::new(100., 100.));
    for (element, x, y, display) in [
        ("F", 142., 100., "wedge"),
        ("Cl", 80., 64., "plain"),
        ("Br", 80., 136., "plain"),
        ("H", 100., 142., "plain"),
    ] {
        let ligand = document.add_atom(element, Point::new(x, y));
        document.add_bond(center, ligand, 1, display);
    }
    document
}

#[test]
fn strict_is_the_default_and_legacy_chemistry_rejections_keep_their_messages() {
    assert_eq!(Mode::default(), Mode::Strict);
    let prepared = Prepared::new(&ethanol(), &[]).unwrap();
    assert_eq!(prepared.mode(), Mode::Strict);
    assert!(prepared.skipped_checks().is_empty());
    assert!(prepared.mmff_available());
    assert!(
        !prepared
            .generation_request(ForceField::Uff, DEFAULT_SEED)
            .unwrap()
            .relaxed
    );
    let mut radical = pair("C", "C", 1);
    radical.atoms[0].radical_electrons = 1;
    for (document, expected) in [
        (radical, "3D geometry does not support radicals"),
        (
            pair("N", "Fe", 5),
            "3D geometry does not support hydrogen, coordination, partial or quadruple bonds",
        ),
        (
            pair("C", "C", 6),
            "3D geometry does not support hydrogen, coordination, partial or quadruple bonds",
        ),
        (
            pair("C", "C", 7),
            "3D geometry does not support hydrogen, coordination, partial or quadruple bonds",
        ),
        (
            pair("N", "Fe", 1),
            "3D geometry does not support elements outside H/B/C/N/O/F/Si/P/S/Cl/As/Se/Br/I",
        ),
    ] {
        assert_eq!(
            Prepared::new(&document, &[]).unwrap_err().to_string(),
            expected
        );
    }
}

#[test]
fn relaxed_bond_codes_preserve_dative_donor_direction_and_nonaromatic_partial_order() {
    for (left, right, drawing, native) in [
        ("N", "Fe", 5, 17),
        ("Fe", "N", 5, 17),
        ("C", "C", 6, 4),
        ("C", "C", 7, 7),
        ("C", "C", 4, 12),
    ] {
        let document = pair(left, right, drawing);
        let before = document.clone();
        let prepared = Prepared::relaxed(&document, &[]).unwrap();
        let request = prepared
            .generation_request(ForceField::Uff, DEFAULT_SEED)
            .unwrap();
        assert_eq!(prepared.mode(), Mode::Relaxed);
        assert!(request.relaxed);
        assert_eq!((request.bonds[0].a, request.bonds[0].b), (0, 1));
        assert_eq!(request.bonds[0].order, native);
        assert_eq!(request.bonds[0].aromatic, drawing == 4);
        assert_eq!(prepared.molecule().state.graph.bonds[0].order, drawing);
        let donor = crate::chemistry::ELEMENTS
            .iter()
            .position(|element| element.symbol == left)
            .unwrap();
        assert_eq!(
            request.atoms[request.bonds[0].a].atomic_number,
            donor as u32
        );
        assert_eq!(document, before);
    }
}

#[test]
fn relaxed_never_turns_a_drawn_metal_single_bond_into_a_dative_bond() {
    // Four N bonds make sanitization eligible to clean up the N--Fe single
    // bond as coordination. Geometry must still receive what was drawn.
    let mut document = pair("N", "Fe", 1);
    for x in [-42., 0., 42.] {
        let carbon = document.add_atom("C", Point::new(x, 42.));
        document.add_bond(1, carbon, 1, "plain");
    }
    let prepared = Prepared::relaxed(&document, &[]).unwrap();
    let request = prepared
        .generation_request(ForceField::Uff, DEFAULT_SEED)
        .unwrap();
    assert_eq!(request.bonds[0].order, 1);
    assert!(
        !prepared
            .skipped_checks()
            .iter()
            .any(|check| check.contains("Dative"))
    );
    assert_eq!(prepared.source(), &document);
}

#[test]
fn relaxed_flag_crosses_the_wire_for_generate_relax_and_evaluate() {
    let prepared = Prepared::relaxed(&ethanol(), &[]).unwrap();
    let conf = conformer(&prepared);
    for operation in [
        reshiki_geometry::Operation::Generate,
        reshiki_geometry::Operation::Relax,
        reshiki_geometry::Operation::Evaluate,
    ] {
        let request = prepared
            .native_request(
                ForceField::Uff,
                operation,
                (operation != reshiki_geometry::Operation::Generate).then_some(&conf),
                &[],
                20,
            )
            .unwrap();
        let frame = wire::encode(
            &wire::Request {
                heap_bytes: Limits::default().heap_bytes,
                capacity: None,
                operation: request,
            },
            wire::MAX_REQUEST_BYTES,
        )
        .unwrap();
        let decoded: wire::Request = wire::decode(&frame, wire::MAX_REQUEST_BYTES).unwrap();
        assert!(decoded.operation.relaxed);
        assert_eq!(decoded.operation.operation, operation);
        decoded.operation.validate().unwrap();
    }
}

#[test]
fn relaxed_radicals_and_hydrogen_policy_are_taken_exactly_as_drawn() {
    let mut document = Document::default();
    let id = document.add_atom("C", Point::new(0., 0.));
    let atom = document.atom_mut(id).unwrap();
    atom.radical_electrons = 1;
    atom.explicit_h = 3;
    atom.no_implicit = true;
    let prepared = Prepared::relaxed(&document, &[]).unwrap();
    let request = prepared
        .generation_request(ForceField::Uff, DEFAULT_SEED)
        .unwrap();
    assert_eq!(request.atoms[0].radical, 1);
    assert_eq!(request.atoms[0].explicit_h, 3);
    assert!(request.atoms[0].no_implicit);
    assert_eq!(prepared.skipped_checks(), ["Radicals taken as drawn"]);
    assert!(prepared.mmff_available());
}

#[test]
fn relaxed_successful_preparation_preserves_supported_stereo_without_a_skipped_stereo_note() {
    let document = tetrahedral();
    let expected = Prepared::new(&document, &[]).unwrap();
    let prepared = Prepared::relaxed(&document, &[]).unwrap();
    assert!(prepared.skipped_checks().is_empty());
    assert_eq!(
        prepared.molecule.state.metadata.atoms[0].chiral_tag,
        expected.molecule.state.metadata.atoms[0].chiral_tag
    );
    assert_ne!(prepared.molecule.state.metadata.atoms[0].chiral_tag, 0);
    let conf = conformer(&prepared);
    let mut frame = prepared.view_frame(&conf).unwrap();
    frame.rotate(90., 45.);
    let projected = prepared.document(&conf, &frame).unwrap();
    assert!(projected.atoms[0].stereo.is_some());
    assert_eq!(identity(&projected), identity(&document));
    assert_eq!(projected.bonds, document.bonds);
}

#[test]
fn relaxed_valence_fallback_keeps_original_stereo_and_discloses_only_applicable_skips() {
    let mut document = tetrahedral();
    let extra = document.add_atom("I", Point::new(145., 145.));
    document.add_bond(1, extra, 1, "plain");
    document.atoms[0].stereo = Some(AtomStereo {
        winding: "cw".into(),
        neighbors: document.bonds.iter().map(|bond| bond.b).collect(),
    });
    let original = document.clone();
    assert!(molecular::prepare(&document).is_err());
    let prepared = Prepared::relaxed(&document, &[]).unwrap();
    assert_eq!(
        prepared.skipped_checks(),
        ["Valence not checked", "Drawn stereochemistry not enforced"]
    );
    assert!(
        prepared
            .molecule
            .state
            .metadata
            .atoms
            .iter()
            .all(|atom| atom.chiral_tag == 0)
    );
    assert!(
        prepared
            .molecule
            .state
            .metadata
            .bonds
            .iter()
            .all(|bond| bond.stereo == 0)
    );
    let conf = conformer(&prepared);
    let mut frame = prepared.view_frame(&conf).unwrap();
    frame.rotate(37., 22.);
    let projected = prepared.document(&conf, &frame).unwrap();
    assert_eq!(projected.atoms[0].stereo, document.atoms[0].stereo);
    assert_eq!(projected.bonds, document.bonds);
    assert_eq!(document, original);

    // A failed sanitizer with no drawing stereo must not claim stereo was lost.
    let mut plain = document.clone();
    plain.atoms[0].stereo = None;
    plain.bonds[0].display = "plain".into();
    let prepared = Prepared::relaxed(&plain, &[]).unwrap();
    assert_eq!(prepared.skipped_checks(), ["Valence not checked"]);
}

#[test]
fn relaxed_non_tetrahedral_stereo_is_disclosed_and_drawing_wedges_are_retained() {
    let mut document = Document::default();
    for (element, x, y) in [
        ("C", -2., 1.),
        ("C", -1., 0.),
        ("C", 0., 0.),
        ("C", 1., 0.),
        ("C", 2., 1.),
        ("H", -2., -1.),
        ("H", 2., -1.),
    ] {
        document.add_atom(element, Point::new(x * 28., -y * 28.));
    }
    for (a, b, order, display) in [
        (2, 1, 1, "wedge"),
        (2, 3, 2, "plain"),
        (3, 4, 2, "plain"),
        (4, 5, 1, "plain"),
        (2, 6, 1, "plain"),
        (4, 7, 1, "plain"),
    ] {
        document.add_bond(a, b, order, display);
    }
    assert_eq!(
        molecular::prepare(&document).unwrap().state.metadata.atoms[2].chiral_tag,
        5
    );
    assert_eq!(
        Prepared::new(&document, &[]).unwrap_err().to_string(),
        "3D geometry does not support enhanced or non-tetrahedral stereochemistry"
    );
    let prepared = Prepared::relaxed(&document, &[]).unwrap();
    assert_eq!(
        prepared.skipped_checks(),
        ["Non-tetrahedral stereo ignored"]
    );
    assert_eq!(prepared.molecule.state.metadata.atoms[2].chiral_tag, 0);
    assert_eq!(prepared.materialized.bonds[0].display, "wedge");
    assert_eq!(document.bonds[0].display, "wedge");
    // When preparation fails, disclose the drawn stereo fallback without
    // inferring a non-tetrahedral tag from the drawing.
    document.atoms[0].explicit_h = 8;
    let fallback = Prepared::relaxed(&document, &[]).unwrap();
    assert_eq!(
        fallback.skipped_checks(),
        ["Valence not checked", "Drawn stereochemistry not enforced"]
    );
}

#[test]
fn relaxed_skips_have_stable_order() {
    let mut document = tetrahedral();
    document.atoms[0].radical_electrons = 1;
    document.atoms[0].explicit_h = 8;
    document.bonds[1].order = 5;
    let prepared = Prepared::relaxed(&document, &[]).unwrap();
    assert_eq!(
        prepared.skipped_checks(),
        [
            "Valence not checked",
            "Dative bonds treated as coordination bonds",
            "Radicals taken as drawn",
            "Drawn stereochemistry not enforced",
        ]
    );
}

#[test]
fn relaxed_metals_disable_mmff_and_offer_flat_drawing_coordinates_as_a_seed() {
    for metal in ["Fe", "Cu", "K", "Mg", "La", "U", "Al"] {
        let document = pair("N", metal, 5);
        let prepared = Prepared::relaxed(&document, &[]).unwrap();
        assert!(!prepared.mmff_available(), "{metal}");
        let request = prepared
            .clone()
            .without_retained_depth()
            .generation_request(ForceField::Uff, DEFAULT_SEED)
            .unwrap();
        assert_eq!(request.coordinates.len(), document.atoms.len(), "{metal}");
        assert!(request.coordinates.iter().all(|point| point[2] == 0.));
    }
    for nonmetal in ["C", "Ge", "As", "Se", "Sb", "Br", "Xe"] {
        let mut document = Document::default();
        document.add_atom(nonmetal, Point::new(0., 0.));
        let prepared = Prepared::relaxed(&document, &[]).unwrap();
        assert!(prepared.mmff_available(), "{nonmetal}");
        assert!(
            prepared
                .generation_request(ForceField::Uff, DEFAULT_SEED)
                .unwrap()
                .coordinates
                .is_empty()
        );
    }
}

#[test]
fn relaxed_retains_bounds_structural_guards_and_machine_capacity() {
    for change in [
        |document: &mut Document| document.atoms[0].element = "Qq".into(),
        |document: &mut Document| document.atoms[0].element = "*".into(),
        |document: &mut Document| document.atoms[0].charge = 9,
        |document: &mut Document| document.atoms[0].charge = -9,
        |document: &mut Document| document.atoms[0].isotope = 501,
        |document: &mut Document| document.atoms[0].explicit_h = 9,
        |document: &mut Document| document.atoms[0].radical_electrons = 3,
        |document: &mut Document| document.atoms[0].position.x = f32::NAN,
        |document: &mut Document| document.bonds[0].order = 0,
        |document: &mut Document| document.bonds[0].order = 8,
        |document: &mut Document| document.bonds[0].b = document.bonds[0].a,
        |document: &mut Document| document.bonds.push(document.bonds[0].clone()),
        |document: &mut Document| document.atoms[0].centroid = vec![document.atoms[1].id],
    ] {
        let mut document = ethanol();
        change(&mut document);
        assert!(Prepared::relaxed(&document, &[]).is_err(), "{document:?}");
    }
    let limits = Limits {
        heap_bytes: 64 * 1024 * 1024,
        ..Limits::default()
    };
    assert!(matches!(
        Prepared::with_mode(
            &carbon_chain(limits.capacity().atoms + 1),
            &[],
            limits,
            Mode::Relaxed
        ),
        Err(Error::Capacity {
            kind: "original atoms",
            ..
        })
    ));
}

#[test]
fn relaxed_keeps_defined_unknown_and_unspecified_alkene_metadata() {
    for mode in 0..3 {
        let mut document = Document::default();
        let ids = [
            document.add_atom("F", Point::new(0., 0.)),
            document.add_atom("C", Point::new(42., 42.)),
            document.add_atom("C", Point::new(84., 42.)),
            document.add_atom("F", Point::new(126., 0.)),
        ];
        document.add_bond(ids[0], ids[1], 1, "plain");
        document.add_bond(ids[1], ids[2], 2, if mode == 1 { "wavy" } else { "plain" });
        document.add_bond(ids[2], ids[3], 1, "plain");
        if mode == 0 {
            document.bonds[1].stereo_authoritative = true;
        }
        let strict = Prepared::new(&document, &[]).unwrap();
        let relaxed = Prepared::relaxed(&document, &[]).unwrap();
        assert!(relaxed.skipped_checks().is_empty());
        let expected = &strict.molecule.state.metadata.bonds[1];
        let actual = &relaxed.molecule.state.metadata.bonds[1];
        assert_eq!(actual.stereo, expected.stereo);
        assert_eq!(actual.stereo_atoms, expected.stereo_atoms);
        let request = relaxed
            .generation_request(ForceField::Uff, DEFAULT_SEED)
            .unwrap();
        assert_eq!(request.bonds[1].stereo, u32::from(expected.stereo));
        assert_eq!(relaxed.materialized.bonds[1], strict.materialized.bonds[1]);
    }
}

#[test]
fn relaxed_rejects_link_nodes_and_repeating_units_before_removing_generic_metadata() {
    use crate::generic::{Crossing, LinkNode, RepeatCount, RepeatPattern, RepeatUnit};
    let mut document = carbon_chain(3);
    document.generic.links.push(LinkNode {
        atom: 2,
        min: 1,
        max: 3,
    });
    document.atom_mut(2).unwrap().display.variable = Some("(CH2)1-3".into());
    assert!(matches!(
        Prepared::relaxed(&document, &[]),
        Err(Error::Unsupported("link nodes or repeating units"))
    ));
    document.generic.links.clear();
    document.atom_mut(2).unwrap().display.variable = None;
    document.generic.repeats.push(RepeatUnit {
        bracket: None,
        bracket_objects: Vec::new(),
        members: vec![2],
        crossings: vec![
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 2,
                outside: 3,
            },
        ],
        crossing_brackets: Vec::new(),
        head_count: Some(1),
        usage: "SRU".into(),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min: 1, max: 3 },
        label: "1-3".into(),
        flip: "Unspecified".into(),
        label_offset: None,
    });
    document.validate().unwrap();
    assert!(matches!(
        Prepared::relaxed(&document, &[]),
        Err(Error::Unsupported("link nodes or repeating units"))
    ));
}

#[test]
fn reported_crown_and_cisplatin_drawings_keep_legacy_errors_and_exact_relaxed_notes() {
    let mut crown = Document::default();
    for i in 0..18 {
        let angle = std::f32::consts::TAU * i as f32 / 18.;
        crown.add_atom(
            if i % 3 == 0 { "O" } else { "C" },
            Point::new(98. * angle.cos(), 98. * angle.sin()),
        );
    }
    let potassium = crown.add_atom("K", Point::new(0., 0.));
    crown.atom_mut(potassium).unwrap().charge = 1;
    for i in 0..18 {
        crown.add_bond(i + 1, (i + 1) % 18 + 1, 1, "plain");
        if i % 3 == 0 {
            crown.add_bond(i + 1, potassium, 1, "plain");
        }
    }
    // Current strict preparation cleans the O--K contacts to dative before
    // the geometry element gate rejects K. Preserve that existing error.
    assert_eq!(
        Prepared::new(&crown, &[]).unwrap_err().to_string(),
        "3D geometry does not support elements outside H/B/C/N/O/F/Si/P/S/Cl/As/Se/Br/I"
    );
    let prepared = Prepared::relaxed(&crown, &[]).unwrap();
    assert!(!prepared.mmff_available());
    assert_eq!(prepared.skipped_checks(), ["Valence not checked"]);
    assert_eq!(prepared.ids().len(), 19);
    assert!(
        prepared
            .generation_request(ForceField::Uff, DEFAULT_SEED)
            .unwrap()
            .bonds
            .iter()
            .all(|bond| bond.order == 1)
    );

    let mut platinum = Document::default();
    let pt = platinum.add_atom("Pt", Point::new(0., 0.));
    platinum.atom_mut(pt).unwrap().charge = 2;
    for (element, x, y, dative) in [
        ("N", 42., 0., true),
        ("N", 0., 42., true),
        ("Cl", -42., 0., false),
        ("Cl", 0., -42., false),
    ] {
        let ligand = platinum.add_atom(element, Point::new(x, y));
        if dative {
            let atom = platinum.atom_mut(ligand).unwrap();
            atom.explicit_h = 3;
            atom.no_implicit = true;
            platinum.add_bond(ligand, pt, 5, "plain");
        } else {
            platinum.add_bond(pt, ligand, 1, "plain");
        }
    }
    assert_eq!(
        Prepared::new(&platinum, &[]).unwrap_err().to_string(),
        "3D geometry does not support hydrogen, coordination, partial or quadruple bonds"
    );
    let prepared = Prepared::relaxed(&platinum, &[]).unwrap();
    assert!(!prepared.mmff_available());
    assert_eq!(
        prepared.skipped_checks(),
        ["Dative bonds treated as coordination bonds"]
    );
}
