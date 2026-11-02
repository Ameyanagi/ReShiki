use crate::{
    chemistry::{document::prepare, smiles},
    document::Document,
    templates::{Anchor, LIBRARY},
};

#[test]
fn bundled_frames_have_iupac_projection_geometry() {
    let square = [
        ("wedge", 30.),
        ("wedge", 150.),
        ("hash", -30.),
        ("hash", -150.),
    ];
    let trigonal = [
        ("plain", -90.),
        ("plain", 90.),
        ("plain", 180.),
        ("wedge", 30.),
        ("hash", -30.),
    ];
    let octahedral = [
        ("wedge", 30.),
        ("wedge", 150.),
        ("hash", -30.),
        ("hash", -150.),
        ("plain", -90.),
        ("plain", 90.),
    ];
    let frames: Vec<_> = LIBRARY
        .iter()
        .filter(|t| t.group == "Coordination geometry")
        .collect();
    assert_eq!(frames.len(), 3);
    for (name, element, pattern) in [
        ("Square planar ML₄ (SP-4)", "Pt", square.as_slice()),
        (
            "Trigonal bipyramidal ML₅ (TBPY-5)",
            "Fe",
            trigonal.as_slice(),
        ),
        ("Octahedral ML₆ (OC-6)", "Co", octahedral.as_slice()),
    ] {
        let template = frames.iter().find(|t| t.name == name).unwrap();
        let doc = &template.document;
        doc.validate().unwrap();
        let metal = doc.atoms.iter().find(|a| a.element == element).unwrap();
        assert_eq!(template.anchor, Anchor::Atom(metal.id));
        assert_eq!(doc.atoms.len(), pattern.len() + 1);
        assert_eq!(doc.bonds.len(), pattern.len());
        assert!(doc.atoms.iter().all(|a| a.stereo.is_none()));
        for (bond, &(display, degrees)) in doc.bonds.iter().zip(pattern) {
            assert_eq!(bond.a, metal.id, "narrow end at metal");
            assert_eq!(bond.order, 1);
            assert_eq!(bond.display, display);
            assert_eq!(bond.projection, display != "plain");
            assert!(bond.stereo.is_none());
            assert!(bond.stereo_atoms.is_empty());
            let ligand = doc.atom(bond.b).unwrap();
            assert_eq!(ligand.element, "C");
            let p = ligand.position;
            let origin = metal.position;
            let angle = (p.y - origin.y).atan2(p.x - origin.x).to_degrees();
            assert!((angle - degrees).abs() < 0.0001, "{name}: {angle}");
            assert!((p.distance(origin) - crate::style::DEFAULT.bond_length_world).abs() < 0.00001);
            if display == "plain" && degrees.abs() == 90. {
                assert_eq!(p.x, origin.x, "axial bonds are exactly vertical");
            }
        }
        let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reopened, doc.current());
    }
}

#[test]
fn frame_chemistry_and_smiles_have_no_tetrahedral_stereo() {
    let frames: Vec<_> = LIBRARY
        .iter()
        .filter(|t| t.group == "Coordination geometry")
        .collect();
    assert_eq!(frames.len(), 3);
    for template in frames {
        let molecule = prepare(&template.document).unwrap();
        assert!(
            molecule
                .state
                .metadata
                .atoms
                .iter()
                .all(|a| a.chiral_tag == 0)
        );
        assert!(molecule.state.metadata.bonds.iter().all(|b| b.stereo == 0));
        let output = smiles::write::write(&molecule.state, Default::default()).unwrap();
        let expected = smiles::prepare(&template.smiles).unwrap();
        let expected = smiles::write::write(&expected.state, Default::default()).unwrap();
        assert_eq!(output.text, expected.text, "{}", template.name);
        assert!(!output.text.contains('@'));
    }
}
