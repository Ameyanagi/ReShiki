use super::*;
use crate::{bonds::BondPreset, joining::Prepared};

pub(crate) fn en3() -> (Document, u64, Vec<u64>) {
    let mut doc = Document::default();
    let co = doc.add_atom("Co", Point::default());
    doc.atom_mut(co).unwrap().charge = 3;
    let mut donors = vec![];
    for angle in [0_f32, 120., 240.] {
        let point = |radius: f32, offset: f32| {
            let a = (angle + offset).to_radians();
            Point::new(radius * a.cos(), radius * a.sin())
        };
        let a = doc.add_atom("N", point(60., -25.));
        let b = doc.add_atom("C", point(96., -18.));
        let c = doc.add_atom("C", point(96., 18.));
        let d = doc.add_atom("N", point(60., 25.));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        doc.add_bond(c, d, 1, "plain");
        donors.extend([a, d]);
    }
    // Adding atoms invalidates cached H labels; set all six after the complete skeleton exists.
    for id in &donors {
        doc.atom_mut(*id).unwrap().label_h = 2;
    }
    (doc, co, donors)
}

#[test]
fn all_six_chelate_contacts_retain_positions_ids_hydrogens_charge_and_skeletons() {
    let (mut doc, co, donors) = en3();
    let before = doc.clone();
    assert_eq!(capacity(doc.atom(co).unwrap()), 0);
    for donor in &donors {
        let prepared = Prepared::with_mode(&doc, &[*donor], Connection::Coordinate).unwrap();
        let (next, selected) = prepared
            .place(
                Point::default(),
                Some(Point::new(900., 500.)),
                10.,
                Anchor::Atom(*donor),
                Connection::Coordinate,
            )
            .unwrap();
        assert_eq!(selected, vec![*donor, co]);
        assert_eq!(next.atoms, before.atoms);
        assert_eq!(&next.bonds[..9], &before.bonds[..]);
        assert_eq!(valence(&next, *donor), 2);
        assert_eq!(next.atom(*donor).unwrap().label_h, 2);
        doc = next;
    }
    assert_eq!(doc.atom(co).unwrap().charge, 3);
    assert_eq!(
        doc.bonds
            .iter()
            .filter(|b| b.order == 5 && b.b == co)
            .count(),
        6
    );
    assert_eq!(coordinate_atoms(&doc, donors[0], co).unwrap(), doc);
    doc.validate().unwrap();
    let chemical = crate::chemistry::document::prepare(&doc).unwrap();
    for donor in &donors {
        let index = chemical.ids.iter().position(|id| id == donor).unwrap();
        assert_eq!(chemical.state.valences[index].implicit_hydrogens, 2);
        assert_eq!(chemical.state.metadata.atoms[index].chiral_tag, 0);
    }
    assert_eq!(
        chemical
            .state
            .graph
            .bonds
            .iter()
            .filter(|b| b.order == 5)
            .count(),
        6
    );
    assert_eq!(
        Document::from_json(&doc.file_json().unwrap()).unwrap(),
        doc.current()
    );
    if let Some(dir) = std::env::var_os("RESHIKI_COORDINATION_FIXTURES") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("co-en3-before.rsk"), before.file_json().unwrap()).unwrap();
        for (i, bond) in doc.bonds.iter_mut().filter(|b| b.order == 5).enumerate() {
            match i {
                0 | 1 => BondPreset::Wedge.apply(bond),
                2 | 3 => BondPreset::HashedWedge.apply(bond),
                _ => {}
            }
            if matches!(i, 1 | 3) {
                assert!(bond.reverse_projection());
            }
        }
        std::fs::write(dir.join("co-en3-after.rsk"), doc.file_json().unwrap()).unwrap();
    }
}

#[test]
fn explicit_coordination_does_not_relax_covalent_capacity_or_merge_atom_identity() {
    let (doc, co, donors) = en3();
    let prepared = Prepared::new(&doc, &[donors[0]]).unwrap();
    assert!(
        prepared
            .place(
                Point::default(),
                None,
                10.,
                Anchor::Atom(donors[0]),
                Connection::Connect
            )
            .is_err()
    );
    let mut unsupported = doc.clone();
    unsupported.atom_mut(donors[0]).unwrap().charge = 1;
    assert!(
        coordinate_atoms(&unsupported, donors[0], co)
            .unwrap_err()
            .contains("donor")
    );
    assert!(
        coordinate_atoms(&doc, co, donors[0])
            .unwrap_err()
            .contains("donor")
    );
    assert!(
        coordinate_atoms(&doc, donors[0], donors[1])
            .unwrap_err()
            .contains("transition-metal")
    );
    let mut fixed_h = doc.clone();
    fixed_h.atom_mut(donors[0]).unwrap().explicit_h = 2;
    fixed_h.atom_mut(donors[0]).unwrap().no_implicit = true;
    let connected = coordinate_atoms(&fixed_h, donors[0], co).unwrap();
    assert_eq!(connected.atoms, fixed_h.atoms);
    fixed_h.atom_mut(donors[0]).unwrap().explicit_h = 3;
    assert!(
        coordinate_atoms(&fixed_h, donors[0], co)
            .unwrap_err()
            .contains("lone pair")
    );
    assert_eq!(doc.bonds.len(), 9);
}

#[test]
fn dative_projection_styling_never_changes_order_direction_or_stereo() {
    let (doc, co, donors) = en3();
    let mut doc = coordinate_atoms(&doc, donors[0], co).unwrap();
    for preset in [
        BondPreset::Wedge,
        BondPreset::HashedWedge,
        BondPreset::HollowWedge,
        BondPreset::Bold,
        BondPreset::Hashed,
        BondPreset::Single,
        BondPreset::Dashed,
        BondPreset::Dative,
    ] {
        preset.place(&mut doc, co, donors[0]); // reverse drawing gesture
        let bond = doc.bonds.iter().find(|b| b.order == 5).unwrap();
        assert_eq!((bond.a, bond.b, bond.order), (donors[0], co, 5));
        assert!(bond.stereo.is_none());
        assert_eq!(
            bond.projection,
            !matches!(bond.display.as_str(), "plain" | "dashed")
        );
        assert!(preset.preserves_chemistry(bond));
        assert_eq!(
            BondPreset::of(bond),
            Some(if preset == BondPreset::Single {
                BondPreset::Dative
            } else {
                preset
            })
        );
        let before = doc.clone();
        let bond = doc.bonds.iter_mut().find(|b| b.order == 5).unwrap();
        if matches!(
            preset,
            BondPreset::Wedge | BondPreset::HashedWedge | BondPreset::HollowWedge
        ) {
            assert!(bond.reverse_projection());
            assert_eq!((bond.a, bond.b, bond.order), (donors[0], co, 5));
            assert_eq!(BondPreset::of(bond), Some(preset));
            assert!(bond.reverse_projection());
            assert_eq!(doc, before);
        }
        doc.validate().unwrap();
        assert!(!crate::scene::primitives(&doc).is_empty());
    }
}

#[test]
fn solid_coordination_projection_renders_a_taper_at_either_requested_end() {
    let mut original = Document::default();
    let donor = original.add_atom("N", Point::default());
    let metal = original.add_atom("Co", Point::new(120., 0.));
    original.atom_mut(metal).unwrap().charge = 3;
    let original = coordinate_atoms(&original, donor, metal).unwrap();
    let mut projected = original.clone();
    BondPreset::Wedge.apply(&mut projected.bonds[0]);
    for acceptor_narrowed in [false, true] {
        if acceptor_narrowed {
            assert!(projected.bonds[0].reverse_projection());
        }
        let shapes = crate::scene::primitives(&projected);
        let vertices: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                crate::scene::Primitive::Path {
                    commands,
                    filled: true,
                    ..
                } => Some(commands),
                _ => None,
            })
            .flatten()
            .flat_map(crate::graphics::PathCommand::iter_points)
            .collect();
        assert!(
            vertices.len() >= 4,
            "A solid dative projection must have filled ink, not a thin line"
        );
        let lo = vertices.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let hi = vertices
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let width = |x: f32| {
            let ys: Vec<_> = vertices
                .iter()
                .filter(|p| (p.x - x).abs() < 0.01)
                .map(|p| p.y)
                .collect();
            assert!(ys.len() >= 2, "Each wedge cap must have two corners");
            ys.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - ys.iter().copied().fold(f32::INFINITY, f32::min)
        };
        let donor_width = width(lo);
        let metal_width = width(hi);
        assert!(
            if acceptor_narrowed {
                metal_width * 2. < donor_width
            } else {
                donor_width * 2. < metal_width
            },
            "Requested narrow endpoint must determine the rendered taper"
        );
        assert_eq!(projected.atoms, original.atoms);
        let bond = &projected.bonds[0];
        assert_eq!((bond.a, bond.b, bond.order), (donor, metal, 5));
        assert!(bond.stereo.is_none());
    }
}

#[test]
fn hashed_coordination_projection_tapers_at_either_requested_end() {
    let mut original = Document::default();
    let donor = original.add_atom("N", Point::default());
    let metal = original.add_atom("Co", Point::new(120., 0.));
    original.atom_mut(metal).unwrap().charge = 3;
    let original = coordinate_atoms(&original, donor, metal).unwrap();
    let mut projected = original.clone();
    BondPreset::HashedWedge.apply(&mut projected.bonds[0]);
    for acceptor_narrowed in [false, true] {
        if acceptor_narrowed {
            assert!(projected.bonds[0].reverse_projection());
        }
        let shapes = crate::scene::primitives(&projected);
        let mut strokes: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                crate::scene::Primitive::Line(a, b, _) if (a.x - b.x).abs() < 0.01 => {
                    Some((a.x, (a.y - b.y).abs()))
                }
                _ => None,
            })
            .collect();
        strokes.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(strokes.len() > 3, "A hashed wedge needs distinct crossbars");
        let donor_width = strokes.first().unwrap().1;
        let metal_width = strokes.last().unwrap().1;
        assert!(
            if acceptor_narrowed {
                metal_width * 2. < donor_width
            } else {
                donor_width * 2. < metal_width
            },
            "Crossbar lengths must taper toward the requested endpoint"
        );
        assert_eq!(projected.atoms, original.atoms);
        assert_eq!(
            (
                projected.bonds[0].a,
                projected.bonds[0].b,
                projected.bonds[0].order
            ),
            (donor, metal, 5)
        );
    }
}

#[test]
fn projected_dative_crossing_clearance_is_independent_of_narrow_endpoint() {
    let mut doc = Document::default();
    let donor = doc.add_atom("N", Point::default());
    let metal = doc.add_atom("Co", Point::new(120., 0.));
    doc.atom_mut(metal).unwrap().charge = 3;
    let mut doc = coordinate_atoms(&doc, donor, metal).unwrap();
    BondPreset::Wedge.apply(&mut doc.bonds[0]);
    doc.bonds[0].z_order = 1;
    let lo = Point::new(60., -45.);
    let hi = Point::new(60., 45.);
    let a = doc.add_atom("C", lo);
    let b = doc.add_atom("C", hi);
    doc.add_bond(a, b, 1, "plain");
    let clearance = |doc: &Document| {
        let gaps = crate::crossings::gaps(doc);
        let cut = crate::crossings::cut(vec![crate::scene::Primitive::Line(lo, hi, 1.)], &gaps[1]);
        cut.into_iter()
            .map(|shape| match shape {
                crate::scene::Primitive::Line(a, b, _) => (a, b),
                _ => panic!("Expected clipped line pieces"),
            })
            .collect::<Vec<_>>()
    };
    let donor_narrowed = clearance(&doc);
    assert_eq!(donor_narrowed.len(), 2);
    assert!(doc.bonds[0].reverse_projection());
    assert_eq!(clearance(&doc), donor_narrowed);
    assert_eq!((doc.bonds[0].a, doc.bonds[0].b), (donor, metal));
}
