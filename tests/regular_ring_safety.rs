use reshiki::{
    document::{AtomStereo, Document, Point},
    editing,
    rings::Preset,
};

fn midpoint(doc: &Document) -> Point {
    let bond = &doc.bonds[0];
    let a = doc.atom(bond.a).unwrap().position;
    let b = doc.atom(bond.b).unwrap().position;
    Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.)
}

fn reject_unchanged(doc: &Document, point: Point, size: u8, direction: Option<Point>) {
    let mut result = doc.clone();
    assert!(editing::ring_oriented(&mut result, point, size, false, 5., direction).is_err());
    assert_eq!(
        &result, doc,
        "Rejection must preserve the complete document"
    );
}

#[test]
fn regular_ring_rejects_full_valence_atoms_and_shared_edge_endpoints() {
    for degree in [3, 4] {
        let mut doc = Document::default();
        let center = doc.add_atom("C", Point::default());
        for i in 0..degree {
            let angle = i as f32 * std::f32::consts::TAU / degree as f32;
            let id = doc.add_atom("C", Point::new(42. * angle.cos(), 42. * angle.sin()));
            doc.add_bond(center, id, 1, "plain");
        }
        for size in 3..=8 {
            reject_unchanged(&doc, Point::default(), size, None);
            if degree == 4 {
                reject_unchanged(&doc, midpoint(&doc), size, None);
            }
        }
    }
}

#[test]
fn regular_ring_rejects_explicit_hydrogens_and_protected_atom_state() {
    for state in 0..9 {
        let mut doc = Document::default();
        let id = doc.add_atom("C", Point::default());
        let atom = doc.atom_mut(id).unwrap();
        match state {
            0 => atom.explicit_h = 4,
            1 => atom.explicit_h = 1,
            2 => atom.no_implicit = true,
            3 => atom.radical_electrons = 1,
            4 => {
                atom.stereo = Some(AtomStereo {
                    winding: "cw".into(),
                    neighbors: vec![],
                })
            }
            5 => atom.isotope = 13,
            6 => atom.map_num = 1,
            7 => atom.charge = -1,
            8 => {
                doc = reshiki::atom_text::apply(&doc, id, "Me", reshiki::atom_text::Mode::Group)
                    .unwrap();
            }
            _ => unreachable!(),
        }
        reject_unchanged(&doc, Point::default(), 6, None);
        // Sharing a protected endpoint through an otherwise eligible bond must
        // also reject, without stripping explicit H or atom metadata.
        let other = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(id, other, 1, "plain");
        if state == 4 {
            doc.atom_mut(id).unwrap().stereo = Some(AtomStereo {
                winding: "cw".into(),
                neighbors: vec![other],
            });
        }
        reject_unchanged(&doc, Point::new(21., 0.), 6, Some(Point::new(21., 80.)));
    }
}

#[test]
fn regular_ring_protects_drawing_defined_stereo_but_keeps_projection_bonds() {
    for display in ["wedge", "hash", "hashed", "hollow_wedge", "bold", "wavy"] {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(-42., 0.));
        doc.add_bond(a, b, 1, display);
        assert!(doc.bonds[0].stereo.is_none());
        assert!(doc.atom(a).unwrap().stereo.is_none());
        reject_unchanged(&doc, Point::default(), 6, None);
        doc.bonds[0].projection = true;
        let existing_bond = doc.bonds[0].clone();
        editing::ring_oriented(&mut doc, Point::default(), 6, false, 5., None).unwrap();
        assert_eq!(doc.bonds[0], existing_bond);
    }
}

#[test]
fn regular_ring_expands_existing_reaction_participants_before_validation() {
    use reshiki::{
        document::Arrow,
        reactions::{self, Role},
    };
    let mut doc = Document::default();
    let anchor = doc.add_atom("C", Point::default());
    let neighbor = doc.add_atom("C", Point::new(-42., 0.));
    doc.add_bond(anchor, neighbor, 1, "plain");
    let product = doc.add_atom("C", Point::new(300., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(120., 0.),
        Point::new(240., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut doc, arrow, &[anchor], Role::Reactant).unwrap();
    reactions::assign(&mut doc, arrow, &[product], Role::Product).unwrap();
    let selected = editing::ring_oriented(&mut doc, Point::default(), 6, false, 5., None).unwrap();
    assert_eq!(doc.reactions[0].reactants[0].atoms.len(), 7);
    assert!(
        selected
            .iter()
            .all(|id| doc.reactions[0].reactants[0].atoms.contains(id))
    );
    assert_eq!(doc.reactions[0].products[0].atoms, [product]);
    doc.validate().unwrap();
}

#[test]
fn regular_ring_rejects_entire_overlays_for_every_supported_size() {
    for size in 3..=8 {
        let mut doc = Document::default();
        editing::ring_oriented(&mut doc, Point::default(), size, false, 0., None).unwrap();
        reject_unchanged(&doc, midpoint(&doc), size, Some(Point::default()));
    }
}

#[test]
fn regular_ring_rejects_one_colliding_vertex_including_explicit_methane() {
    let mut host = Document::default();
    let a = host.add_atom("C", Point::default());
    let b = host.add_atom("C", Point::new(42., 0.));
    host.add_bond(a, b, 1, "plain");
    let point = Point::new(21., 0.);
    let direction = Some(Point::new(21., 80.));
    let mut trial = host.clone();
    let ring = editing::ring_oriented(&mut trial, point, 6, false, 5., direction).unwrap();
    let collision = trial.atom(ring[3]).unwrap().position;
    for offset in [0., 0.005, 1.] {
        let mut doc = host.clone();
        let carbon = doc.add_atom("C", collision.offset(offset, 0.));
        doc.atom_mut(carbon).unwrap().explicit_h = 4;
        reject_unchanged(&doc, point, 6, direction);
    }
    // Empty-space placement must reject collisions too, even with snapping off.
    let mut doc = Document::default();
    doc.add_atom("C", Point::new(42., 0.));
    let before = doc.clone();
    assert!(editing::ring_oriented(&mut doc, Point::default(), 6, false, 0., None).is_err());
    assert_eq!(doc, before);
}

#[test]
fn regular_ring_retains_valid_attachment_fusion_and_shared_bond_orders() {
    for size in 3..=8 {
        // One open-valence carbon can become a spiro junction.
        let mut doc = Preset::Regular.document(42., false);
        let anchor = doc.atoms[0].position;
        let original = doc.clone();
        let selected = editing::ring_oriented(&mut doc, anchor, size, false, 5., None).unwrap();
        assert_eq!(selected.len(), size as usize);
        assert_eq!(doc.atoms.len(), 6 + size as usize - 1);
        assert_eq!(doc.bonds.len(), 6 + size as usize);
        assert_eq!(
            &doc.atoms[..original.atoms.len()],
            original.atoms.as_slice()
        );
        reshiki::chemistry::document::prepare(&doc).unwrap();
        let restored: Document =
            serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(restored, doc);
        // Fusion adds one bond per endpoint; preserve an existing double edge.
        for order in [1, 2] {
            let mut doc = Document::default();
            let a = doc.add_atom("C", Point::default());
            let b = doc.add_atom("C", Point::new(60., 0.));
            doc.add_bond(a, b, order, "plain");
            let shared = doc.bonds[0].clone();
            let selected = editing::ring_oriented(
                &mut doc,
                Point::new(30., 0.),
                size,
                false,
                5.,
                Some(Point::new(30., 80.)),
            )
            .unwrap();
            assert_eq!(&selected[..2], &[a, b]);
            assert_eq!(doc.bonds[0], shared);
            assert_eq!(
                (doc.atoms.len(), doc.bonds.len()),
                (size as usize, size as usize)
            );
            reshiki::chemistry::document::prepare(&doc).unwrap();
        }
        // A normal outward fused ring remains available after inward rejection.
        let mut doc = Preset::Regular.document(42., false);
        let p = midpoint(&doc);
        let before = doc.clone();
        reject_unchanged(&doc, p, 6, Some(Point::default()));
        editing::ring_oriented(
            &mut doc,
            p,
            size,
            false,
            5.,
            Some(Point::new(2. * p.x, 2. * p.y)),
        )
        .unwrap();
        assert_eq!(doc.atoms.len(), before.atoms.len() + size as usize - 2);
        reshiki::chemistry::document::prepare(&doc).unwrap();
    }
}

#[test]
fn regular_ring_rejects_nonfinite_or_degenerate_geometry_atomically() {
    let source = Document::default();
    for (point, radius, direction) in [
        (Point::new(f32::NAN, 0.), 5., None),
        (Point::default(), -1., None),
        (Point::default(), f32::INFINITY, None),
        (Point::default(), 5., Some(Point::new(0., f32::NAN))),
    ] {
        let mut doc = source.clone();
        assert!(editing::ring_oriented(&mut doc, point, 6, false, radius, direction).is_err());
        assert_eq!(doc, source);
    }
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::default());
    doc.add_bond(a, b, 1, "plain");
    reject_unchanged(&doc, Point::default(), 6, None);
}
