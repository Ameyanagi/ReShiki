use super::*;
use crate::document::{AtomStereo, Point};

fn tetra(document: &mut Document, x: f32, display: &str) -> (u64, [u64; 3]) {
    let center = document.add_atom("C", Point::new(x, 0.));
    let neighbors = [
        document.add_atom("F", Point::new(x + 28., 0.)),
        document.add_atom("Cl", Point::new(x - 14., -24.)),
        document.add_atom("Br", Point::new(x - 14., 24.)),
    ];
    for (index, &other) in neighbors.iter().enumerate() {
        document.add_bond(center, other, 1, if index == 0 { display } else { "plain" });
    }
    (center, neighbors)
}

fn has(document: &Document, style: bool, kind: Kind) -> bool {
    check(document, style).iter().any(|note| note.kind == kind)
}

fn alkene(document: &mut Document, x: f32, authoritative: bool) -> (u64, u64) {
    let f = document.add_atom("F", Point::new(x - 28., -14.));
    let a = document.add_atom("C", Point::new(x, 0.));
    let b = document.add_atom("C", Point::new(x + 28., 0.));
    let cl = document.add_atom("Cl", Point::new(x + 56., 14.));
    document.add_bond(f, a, 1, "plain");
    document.add_bond(a, b, 2, "plain");
    let double = document.bonds.len() - 1;
    document.add_bond(b, cl, 1, "plain");
    document.bonds[double].stereo_authoritative = authoritative;
    (a, b)
}

#[test]
fn reversed_and_ineffective_bonds_use_chemical_centres() {
    let mut document = Document::default();
    let (center, neighbors) = tetra(&mut document, 0., "wedge");
    document.atom_mut(neighbors[0]).unwrap().element = "C".into();
    assert!(!has(&document, false, Kind::NoEffect));
    let bond = &mut document.bonds[0];
    std::mem::swap(&mut bond.a, &mut bond.b);
    let notes = check(&document, false);
    let reversed = notes.iter().find(|n| n.kind == Kind::Reversed).unwrap();
    assert_eq!(reversed.bonds, [(document.bonds[0].a, center)]);
    assert!(reversed.message.contains("C1"));
    assert!(!has(&document, false, Kind::Undefined));
    assert_eq!(undefined(&document).atoms, [center]);

    let mut ethane = Document::default();
    let a = ethane.add_atom("C", Point::default());
    let b = ethane.add_atom("C", Point::new(28., 0.));
    let c = ethane.add_atom("C", Point::new(-28., 0.));
    ethane.add_bond(a, b, 1, "wedge");
    ethane.add_bond(a, c, 1, "plain");
    assert!(has(&ethane, false, Kind::NoEffect));
    assert!(!has(&ethane, false, Kind::Undefined));
    assert!(!has(&ethane, false, Kind::Reversed));
}

#[test]
fn overlapping_and_conflicting_wedges_are_distinguished() {
    let mut document = Document::default();
    let (center, neighbors) = tetra(&mut document, 0., "wedge");
    document.atom_mut(neighbors[1]).unwrap().position = Point::default();
    let before = document.clone();
    let notes = check(&document, false);
    assert!(
        notes
            .iter()
            .any(|n| n.kind == Kind::Ambiguous && n.atoms == [center])
    );
    assert!(!notes.iter().any(|n| n.kind == Kind::NoEffect));
    assert!(!notes.iter().any(|n| n.kind == Kind::Undefined));
    assert_eq!(document, before);

    document.atom_mut(neighbors[1]).unwrap().position = Point::new(-14., -24.);
    document.bonds[1].display = "hash".into();
    assert!(has(&document, false, Kind::Conflicting));
    assert!(!has(&document, false, Kind::Undefined));
    assert_eq!(undefined(&document).atoms, [center]);
    document.bonds[1].display = "wedge".into();
    assert!(!has(&document, false, Kind::Conflicting));
}

#[test]
fn drawing_issues_only_suppress_covered_undefined_centres() {
    for (display, kind) in [("hash", Kind::Reversed), ("wedge", Kind::Conflicting)] {
        let mut document = Document::default();
        let (center, neighbors) = tetra(&mut document, 0., display);
        document.atom_mut(neighbors[0]).unwrap().element = "C".into();
        if kind == Kind::Reversed {
            let bond = &mut document.bonds[0];
            std::mem::swap(&mut bond.a, &mut bond.b);
        } else {
            document.bonds[1].display = "hash".into();
        }
        let (other, _) = tetra(&mut document, 140., "plain");
        document.add_bond(neighbors[0], other, 1, "plain");
        let double = alkene(&mut document, 280., true);
        let units = undefined(&document);
        assert_eq!(units.atoms, [center, other]);
        assert_eq!(units.bonds, [double]);

        let notes = check(&document, false);
        assert!(notes.iter().any(|note| note.kind == kind));
        let missing = notes.iter().find(|n| n.kind == Kind::Undefined).unwrap();
        assert_eq!(missing.atoms, [other]);
        assert_eq!(missing.bonds, [double]);
    }
}

#[test]
fn undefined_units_aggregate_names_and_exclude_explicit_unknown() {
    let mut document = Document::default();
    let (center, _) = tetra(&mut document, 0., "plain");
    document.atom_mut(center).unwrap().display.number = Some(crate::atom_labels::Number {
        text: "7a".into(),
        offset: None,
        style: crate::atom_labels::number_style(),
    });
    let double = alkene(&mut document, 140., true);
    let units = undefined(&document);
    assert_eq!(units.atoms, [center]);
    assert_eq!(units.bonds, [double]);
    let notes = check(&document, false);
    let note = notes.iter().find(|n| n.kind == Kind::Undefined).unwrap();
    assert_eq!(note.atoms, units.atoms);
    assert_eq!(note.bonds, units.bonds);
    assert!(note.message.contains("1 undefined stereocentre (7a)"));
    assert!(
        note.message
            .contains("1 undefined stereogenic double bond (C6=C7)")
    );
    document.bonds[0].display = "wavy".into();
    document.bonds[4].stereo = Some("any".into());
    assert_eq!(undefined(&document), Undefined::default());
}

#[test]
fn ordinary_alkene_geometry_is_specified_and_equivalent_ligands_are_not_stereogenic() {
    let mut document = Document::default();
    alkene(&mut document, 0., false);
    assert!(undefined(&document).bonds.is_empty());
    document.atoms[0].position = Point::new(-28., 0.);
    assert_eq!(undefined(&document).bonds.len(), 1);
    document.bonds[1].stereo_authoritative = true;
    assert_eq!(undefined(&document).bonds.len(), 1);
    let branch = document.add_atom("F", Point::new(0., 28.));
    let a = document.atoms[1].id;
    document.add_bond(a, branch, 1, "plain");
    assert!(undefined(&document).bonds.is_empty());

    let mut cumulene = Document::default();
    let atoms: Vec<_> = ["F", "C", "C", "C", "Cl"]
        .into_iter()
        .enumerate()
        .map(|(i, e)| cumulene.add_atom(e, Point::new(i as f32 * 28., 0.)))
        .collect();
    for (i, pair) in atoms.windows(2).enumerate() {
        cumulene.add_bond(
            pair[0],
            pair[1],
            if matches!(i, 1 | 2) { 2 } else { 1 },
            "plain",
        );
        cumulene.bonds.last_mut().unwrap().stereo_authoritative = matches!(i, 1 | 2);
    }
    assert!(undefined(&cumulene).bonds.is_empty());
}

#[test]
fn all_style_notes_are_opt_in_and_hashes_are_compared_across_molecules() {
    let mut document = Document::default();
    let (first, _) = tetra(&mut document, 0., "hash");
    tetra(&mut document, 140., "hashed");
    assert!(!has(&document, false, Kind::MixedHashes));
    assert!(!has(&document, false, Kind::NonWedgeOnly));
    assert!(has(&document, true, Kind::MixedHashes));
    assert!(has(&document, true, Kind::NonWedgeOnly));
    for display in ["bold", "hollow_wedge"] {
        document.bonds[0].display = display.into();
        assert!(
            check(&document, true)
                .iter()
                .any(|n| n.kind == Kind::NonWedgeOnly && n.atoms.contains(&first))
        );
    }
}

#[test]
fn bonds_between_centres_and_wavy_alkene_links_get_style_advice() {
    let mut document = Document::default();
    let a = document.add_atom("C", Point::default());
    let b = document.add_atom("C", Point::new(28., 0.));
    document.add_bond(a, b, 1, "hash");
    for (center, x, first, second) in [(a, 0., "F", "Cl"), (b, 28., "Br", "I")] {
        for (element, y) in [(first, -28.), (second, 28.)] {
            let id = document.add_atom(element, Point::new(x, y));
            document.add_bond(center, id, 1, "plain");
        }
    }
    assert!(has(&document, true, Kind::BetweenCentres));
    assert!(!has(&document, false, Kind::BetweenCentres));
    assert_eq!(undefined(&document).atoms, [b]);
    assert_eq!(
        check(&document, true)
            .iter()
            .find(|note| note.kind == Kind::Undefined)
            .unwrap()
            .atoms,
        [b]
    );

    let mut wavy = Document::default();
    let c = wavy.add_atom("C", Point::default());
    let d = wavy.add_atom("C", Point::new(28., 0.));
    let e = wavy.add_atom("C", Point::new(56., 0.));
    wavy.add_bond(c, d, 1, "wavy");
    wavy.add_bond(d, e, 2, "plain");
    for (element, x, y) in [("F", -14., -24.), ("Cl", -14., 24.), ("Br", 70., 24.)] {
        let id = wavy.add_atom(element, Point::new(x, y));
        wavy.add_bond(if element == "Br" { e } else { c }, id, 1, "plain");
    }
    assert!(has(&wavy, true, Kind::WavyAlkeneCentre));
    assert!(!has(&wavy, false, Kind::WavyAlkeneCentre));
    assert!(!undefined(&wavy).atoms.contains(&c));
}

#[test]
fn tshape_and_reentrant_ring_advice_is_optional() {
    let mut document = Document::default();
    let (_, neighbors) = tetra(&mut document, 0., "wedge");
    document.atom_mut(neighbors[0]).unwrap().position = Point::new(0., -28.);
    document.atom_mut(neighbors[1]).unwrap().position = Point::new(-28., 0.);
    document.atom_mut(neighbors[2]).unwrap().position = Point::new(28., 0.);
    assert!(has(&document, true, Kind::TShape));
    assert!(!has(&document, false, Kind::TShape));

    let mut ring = Document::default();
    let atoms: Vec<_> = [(-28., -28.), (0., 0.), (28., -28.), (28., 28.), (-28., 28.)]
        .into_iter()
        .map(|(x, y)| ring.add_atom("C", Point::new(x, y)))
        .collect();
    for (&a, &b) in atoms.iter().zip(atoms.iter().cycle().skip(1)) {
        ring.add_bond(a, b, 1, "plain");
    }
    let o = ring.add_atom("O", Point::new(0., -28.));
    ring.add_bond(atoms[1], o, 1, "wedge");
    let f = ring.add_atom("F", Point::new(56., -28.));
    ring.add_bond(atoms[2], f, 1, "plain");
    assert!(has(&ring, true, Kind::ReEntrant));
    assert!(!has(&ring, false, Kind::ReEntrant));
}

#[test]
fn redraw_preserves_every_property_except_stereobond_appearance_and_orientation()
-> anyhow::Result<()> {
    let mut document = Document::default();
    let (center, neighbors) = tetra(&mut document, 0., "hollow_wedge");
    let tag = document::prepare(&document)?.state.metadata.atoms[0].chiral_tag;
    document.atom_mut(center).unwrap().stereo = Some(AtomStereo {
        winding: if tag == 1 { "cw" } else { "ccw" }.into(),
        neighbors: neighbors.to_vec(),
    });
    alkene(&mut document, 140., false);
    tetra(&mut document, 280., "bold");
    let wavy_a = document.add_atom("C", Point::new(420., 0.));
    let wavy_b = document.add_atom("C", Point::new(448., 0.));
    document.add_bond(wavy_a, wavy_b, 1, "wavy");
    let project_a = document.add_atom("C", Point::new(560., 0.));
    let project_b = document.add_atom("C", Point::new(588., 0.));
    document.add_bond(project_a, project_b, 1, "wedge");
    document.bonds.last_mut().unwrap().projection = true;
    document.bonds[0].z_order = 7;
    document.bonds[0].color = crate::palette::Color::Custom([180, 50, 55]);
    let before = document.clone();
    let original = document::prepare(&document)?;
    let original_labels = document::for_drawing(&original, &document)?.labels()?;
    let (after, count) = redraw(&document, &[center])?;
    assert_eq!(count, 1);
    assert_eq!(after.atoms, before.atoms);
    assert_eq!(after.bonds[3..], before.bonds[3..]);
    assert_eq!(after.bonds[0].z_order, 7);
    assert_eq!(
        after.bonds[0].color,
        crate::palette::Color::Custom([180, 50, 55])
    );
    assert!(
        after
            .bonds
            .iter()
            .take(3)
            .any(|b| matches!(b.display.as_str(), "wedge" | "hash") && b.a == center)
    );
    let perceived = document::prepare(&after)?;
    let labels = document::for_drawing(&perceived, &after)?.labels()?;
    assert_eq!(labels.atoms, original_labels.atoms);
    assert_eq!(
        serde_json::to_value(labels.bonds)?,
        serde_json::to_value(original_labels.bonds)?
    );
    assert_eq!(
        serde_json::to_value(&perceived.state.metadata.bonds)?,
        serde_json::to_value(&original.state.metadata.bonds)?
    );
    assert_eq!(
        original
            .state
            .metadata
            .atoms
            .iter()
            .map(|a| a.chiral_tag)
            .collect::<Vec<_>>(),
        perceived
            .state
            .metadata
            .atoms
            .iter()
            .map(|a| a.chiral_tag)
            .collect::<Vec<_>>()
    );
    assert_eq!(document, before);
    let (all_drawing, all) = redraw(&document, &[])?;
    assert_eq!(all, 2);
    assert_eq!(
        all_drawing.bond_between(wavy_a, wavy_b).unwrap().1,
        document.bond_between(wavy_a, wavy_b).unwrap().1
    );
    assert_eq!(
        all_drawing.bond_between(project_a, project_b).unwrap().1,
        document.bond_between(project_a, project_b).unwrap().1
    );
    Ok(())
}

#[test]
fn redraw_moves_trans_methylcyclohexanol_wedges_to_substituents() -> anyhow::Result<()> {
    let mut document = Document::default();
    let ring: Vec<_> = [
        (-14., -24.),
        (14., -24.),
        (28., 0.),
        (14., 24.),
        (-14., 24.),
        (-28., 0.),
    ]
    .into_iter()
    .map(|(x, y)| document.add_atom("C", Point::new(x, y)))
    .collect();
    for (index, (&a, &b)) in ring.iter().zip(ring.iter().cycle().skip(1)).enumerate() {
        document.add_bond(
            a,
            b,
            1,
            match index {
                0 => "wedge",
                1 => "hash",
                _ => "plain",
            },
        );
    }
    let o = document.add_atom("O", Point::new(-28., -48.));
    let methyl = document.add_atom("C", Point::new(28., -48.));
    document.add_bond(ring[0], o, 1, "plain");
    document.add_bond(ring[1], methyl, 1, "plain");
    let before = document::prepare(&document)?;
    let smiles = super::super::smiles::write::write(&before.state, Default::default())?.text;
    let (after, count) = redraw(&document, &[])?;
    assert_eq!(count, 2);
    assert!(after.bonds[..6].iter().all(|b| b.display == "plain"));
    assert!(
        after.bonds[6..]
            .iter()
            .all(|b| matches!(b.display.as_str(), "wedge" | "hash"))
    );
    assert_ne!(after.bonds[6].display, after.bonds[7].display);
    assert_eq!(after.atoms, document.atoms);
    let perceived = document::prepare(&after)?;
    assert_eq!(
        super::super::smiles::write::write(&perceived.state, Default::default())?.text,
        smiles
    );
    Ok(())
}

#[test]
fn unsupported_and_failed_molecules_do_not_mask_independent_notices() {
    let mut document = Document::default();
    let (center, _) = tetra(&mut document, 0., "plain");
    let (blocked, _) = tetra(&mut document, 140., "plain");
    document.bonds[3].distance_derived = true;
    document.version = crate::document::DISTANCE_BONDS;
    let (invalid, _) = tetra(&mut document, 280., "plain");
    document.atom_mut(invalid).unwrap().explicit_h = 8;
    let (generic, _) = tetra(&mut document, 420., "plain");
    document =
        crate::atom_text::apply(&document, generic, "[C,N]", crate::atom_text::Mode::Auto).unwrap();
    let (crystal, neighbors) = tetra(&mut document, 560., "plain");
    let atoms = std::iter::once(crystal)
        .chain(neighbors)
        .enumerate()
        .map(|(i, id)| crate::models3d::Placed {
            id,
            xyz: [i as f32, 0., 0.],
        })
        .collect();
    let model = crate::models3d::Model3d::new(
        &document,
        atoms,
        vec![],
        crate::models3d::ForceField::Mmff94s,
    )
    .with_cell(
        &document,
        Some(crate::models3d::Cell {
            vectors: [[10., 0., 0.], [0., 10., 0.], [0., 0., 10.]],
            crossing: vec![],
        }),
        vec![],
    );
    document.models.push(model);
    document.version = crate::document::MODEL_POLISH;
    assert_eq!(undefined(&document).atoms, [center]);
    assert!(!undefined(&document).atoms.contains(&blocked));
}

#[test]
fn redraw_rejects_degenerate_geometry_without_changing_input() {
    let mut document = Document::default();
    let (center, neighbors) = tetra(&mut document, 0., "plain");
    document.atom_mut(center).unwrap().stereo = Some(AtomStereo {
        winding: "cw".into(),
        neighbors: neighbors.to_vec(),
    });
    document.atom_mut(neighbors[1]).unwrap().position = Point::default();
    let before = document.clone();
    assert!(redraw(&document, &[]).is_err());
    assert_eq!(document, before);
}

#[test]
fn degenerate_assigned_tetrahedron_does_not_hide_a_connected_undefined_alkene() -> anyhow::Result<()>
{
    let mut document = Document::default();
    let (center, neighbors) = tetra(&mut document, 0., "plain");
    document.atom_mut(center).unwrap().stereo = Some(AtomStereo {
        winding: "cw".into(),
        neighbors: neighbors.to_vec(),
    });
    document.atom_mut(neighbors[0]).unwrap().element = "C".into();
    document.atom_mut(neighbors[1]).unwrap().position = Point::default();
    let double = alkene(&mut document, 140., false);
    let linker = document.atoms[4].id;
    document.atom_mut(linker).unwrap().element = "C".into();
    document.atom_mut(linker).unwrap().position = Point::new(112., 0.);
    document.add_bond(neighbors[0], linker, 1, "plain");
    document::prepare(&document)?;
    assert_eq!(undefined(&document).bonds, [double]);
    Ok(())
}

#[test]
fn atom_maps_do_not_make_equivalent_ligands_stereogenic() {
    let mut document = Document::default();
    let center = document.add_atom("C", Point::default());
    for (element, position) in [
        ("F", Point::new(28., 0.)),
        ("C", Point::new(-14., -24.)),
        ("C", Point::new(-14., 24.)),
    ] {
        let atom = document.add_atom(element, position);
        document.add_bond(center, atom, 1, "plain");
    }
    document.atoms[2].map_num = 1;
    document.atoms[3].map_num = 2;
    assert!(undefined(&document).atoms.is_empty());
    document.bonds[0].display = "wedge".into();
    assert!(has(&document, false, Kind::NoEffect));
}
