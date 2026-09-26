//! Regression cases from the PR #40 UI review. Native chemistry independently
//! checks the planner's graph; these tests do not merely recheck its capacities.
use reshiki::{
    chemistry::{document::prepare, smiles::write},
    document::{AtomStereo, Document, History, Point},
    editing,
    rings::{Drawing, Preset},
    templates::{self, Anchor, Connection},
};

fn tool(alternate: bool) -> Drawing {
    Drawing {
        preset: Preset::Benzene,
        length: 42.,
        alternate,
        connect: false,
    }
}
fn midpoint(d: &Document, a: u64, b: u64) -> Point {
    let a = d.atom(a).unwrap().position;
    let b = d.atom(b).unwrap().position;
    Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}
fn identity(d: &Document) -> String {
    let molecule = prepare(d).unwrap();
    write::write(&molecule.state, write::Options::default())
        .unwrap()
        .text
}
fn no_duplicates(d: &Document) {
    d.validate().unwrap();
    for (i, a) in d.atoms.iter().enumerate() {
        assert!(
            d.atoms
                .iter()
                .skip(i + 1)
                .all(|b| a.position.distance(b.position) > 0.01)
        );
    }
}
fn keeps_positions(before: &Document, after: &Document) {
    for a in &before.atoms {
        assert_eq!(after.atom(a.id).unwrap().position, a.position);
    }
}
fn naphthalene() -> Document {
    // Two hexagons sharing 2–3. The inward notch is between 1–2 and 2–10.
    let mut d = Document::default();
    let x = 21. * 3_f32.sqrt();
    for (x, y) in [
        (0., -42.),
        (x, -21.),
        (x, 21.),
        (0., 42.),
        (-x, 21.),
        (-x, -21.),
        (3. * x, -21.),
        (3. * x, 21.),
        (2. * x, 42.),
        (2. * x, -42.),
    ] {
        d.add_atom("C", Point::new(x, y));
    }
    for (a, b, o) in [
        (1, 2, 2),
        (2, 3, 1),
        (3, 4, 2),
        (4, 5, 1),
        (5, 6, 2),
        (6, 1, 1),
        (2, 10, 1),
        (10, 7, 2),
        (7, 8, 1),
        (8, 9, 2),
        (9, 3, 1),
    ] {
        d.add_bond(a, b, o, "plain");
    }
    d
}
fn crowded(neighbors: usize) -> Document {
    let mut d = Document::default();
    d.add_atom("C", Point::default());
    d.add_atom("C", Point::new(42., 0.));
    d.add_bond(1, 2, 1, "plain");
    let y = 42. * 3_f32.sqrt();
    let id = d.add_atom("C", Point::new(0., y));
    for (dx, dy) in [(-36.4, 21.), (36.4, 21.), (0., 42.)]
        .into_iter()
        .take(neighbors)
    {
        let other = d.add_atom("C", Point::new(dx, y + dy));
        d.add_bond(id, other, 1, "plain");
    }
    d
}
fn toward_vertex(d: &Document, alternate: bool) -> Result<(Document, Vec<u64>), &'static str> {
    tool(alternate).place(d, Point::new(21., 0.), Some(Point::new(21., 60.)), 5.)
}

#[test]
fn every_benzene_edge_and_phase_agrees_between_toolbar_and_templates() {
    for circular in [false, true] {
        let mut base = Preset::Benzene.document(42., false);
        if circular {
            for b in &mut base.bonds {
                b.order = 4;
            }
        }
        for bond in &base.bonds {
            for alternate in [false, true] {
                let point = midpoint(&base, bond.a, bond.b);
                let (drawn, _) = tool(alternate).place(&base, point, None, 5.).unwrap();
                assert_eq!((drawn.atoms.len(), drawn.bonds.len()), (10, 11));
                assert_eq!(identity(&drawn), "c1ccc2ccccc2c1");
                keeps_positions(&base, &drawn);
                no_duplicates(&drawn);
                let part = Preset::Benzene.document(42., alternate);
                let (templated, _) = templates::place_with_mode(
                    &base,
                    &part,
                    point,
                    None,
                    5.,
                    Anchor::Auto,
                    Connection::FuseBond,
                )
                .unwrap();
                assert_eq!(drawn, templated);
            }
        }
    }
}

#[test]
fn inner_naphthalene_notch_maps_both_shared_edges_before_valence() {
    let base = naphthalene();
    assert_eq!(identity(&base), "c1ccc2ccccc2c1");
    let mut products = std::collections::BTreeSet::new();
    for (a, b) in [(1, 2), (2, 10)] {
        for alternate in [false, true] {
            for scale in [0.5, 1., 2.] {
                for rotation in [0., 30., 90.] {
                    let mut original = base.clone();
                    let ids = original.all_ids();
                    editing::transform_about(
                        &mut original,
                        &ids,
                        Point::default(),
                        scale,
                        rotation,
                    );
                    let mut direction = Document::default();
                    direction.add_atom("C", Point::new(21. * 3_f32.sqrt(), -70.));
                    let ids = direction.all_ids();
                    editing::transform_about(
                        &mut direction,
                        &ids,
                        Point::default(),
                        scale,
                        rotation,
                    );
                    let point = midpoint(&original, a, b);
                    let direction = Some(direction.atoms[0].position);
                    let (placed, _) = tool(alternate)
                        .place(&original, point, direction, 5. * scale)
                        .unwrap();
                    assert_eq!((placed.atoms.len(), placed.bonds.len()), (13, 15));
                    assert_eq!(placed.bonds.iter().filter(|b| b.order == 2).count(), 6);
                    keeps_positions(&original, &placed);
                    no_duplicates(&placed);
                    products.insert(identity(&placed));
                    let part = Preset::Benzene.document(42., alternate);
                    let (library, _) = templates::place_with_mode(
                        &original,
                        &part,
                        point,
                        direction,
                        5. * scale,
                        Anchor::Auto,
                        Connection::FuseBond,
                    )
                    .unwrap();
                    assert_eq!(placed, library);
                }
            }
        }
    }
    assert_eq!(
        products.len(),
        1,
        "The side/phase must not select different products"
    );
}

#[test]
fn crowded_carbon_reserves_both_new_single_bonds() {
    for alternate in [false, true] {
        let original = crowded(2);
        let (placed, _) = toward_vertex(&original, alternate).unwrap();
        assert_eq!(
            placed
                .bonds
                .iter()
                .filter(|b| b.a == 3 || b.b == 3)
                .map(|b| u32::from(b.order))
                .sum::<u32>(),
            4
        );
        identity(&placed);
        keeps_positions(&original, &placed);
        no_duplicates(&placed);
        assert!(toward_vertex(&crowded(3), alternate).is_err());
    }
    let regular = Preset::Regular.document(42., false);
    assert!(
        tool(false)
            .place(&regular, regular.atoms[0].position, None, 5.)
            .is_err()
    );
}

#[test]
fn nearby_protected_or_incompatible_atoms_are_not_merged_or_evaded() {
    for case in 0..9 {
        for alternate in [false, true] {
            let mut d = crowded(0);
            let a = d.atom_mut(3).unwrap();
            match case {
                0 => a.explicit_h = 4,
                1 => a.element = "O".into(),
                2 => a.element = "F".into(),
                3 => {
                    a.element = "N".into();
                    a.explicit_h = 3;
                }
                4 => a.charge = 1,
                5 => a.isotope = 13,
                6 => a.map_num = 7,
                7 => a.no_implicit = true,
                _ => a.radical_electrons = 1,
            }
            let before = d.clone();
            assert!(toward_vertex(&d, alternate).is_err(), "case {case}");
            assert_eq!(d, before);
        }
    }
    let mut d = crowded(2);
    d.atom_mut(3).unwrap().stereo = Some(AtomStereo {
        winding: "cw".into(),
        neighbors: vec![4, 5],
    });
    assert!(toward_vertex(&d, false).is_err());
}

#[tokio::test]
async fn fused_history_native_and_cdxml_roundtrip_preserve_the_graph() {
    let original = naphthalene();
    let point = midpoint(&original, 1, 2);
    let (mut placed, _) = tool(false)
        .place(
            &original,
            point,
            Some(Point::new(21. * 3_f32.sqrt(), -70.)),
            5.,
        )
        .unwrap();
    let expected = placed.clone();
    let chemistry = identity(&placed);
    let mut history = History::default();
    assert!(history.commit(original.clone(), &placed));
    assert!(history.undo(&mut placed));
    assert_eq!(placed, original);
    assert!(!history.can_undo());
    assert!(history.redo(&mut placed));
    assert_eq!(placed, expected);
    let native: Document = serde_json::from_slice(&serde_json::to_vec(&placed).unwrap()).unwrap();
    assert_eq!(native, placed);
    let xml = reshiki::exchange::drawing::write(&placed, Default::default()).unwrap();
    let restored = reshiki::engine::LocalEngine::default()
        .request(reshiki::engine::Request::import("cdxml", &xml))
        .await
        .unwrap()
        .document
        .unwrap();
    assert_eq!(identity(&restored), chemistry);
    no_duplicates(&restored);
}

#[test]
fn catalog_fusions_never_create_excess_ordinary_valence() {
    // Covers the Haworth failures in the review as well as every catalog bond.
    for template in templates::LIBRARY.iter() {
        for bond in &template.document.bonds {
            let point = midpoint(&template.document, bond.a, bond.b);
            if let Ok((result, _)) = tool(false).place(&template.document, point, None, 5.) {
                prepare(&result).unwrap_or_else(|e| {
                    panic!("{} edge {}–{}: {e}", template.name, bond.a, bond.b)
                });
                for a in &result.atoms {
                    let bonds: Vec<_> = result
                        .bonds
                        .iter()
                        .filter(|b| b.a == a.id || b.b == a.id)
                        .collect();
                    if bonds.iter().any(|b| !matches!(b.order, 1..=3)) {
                        continue;
                    }
                    let max = match (a.element.as_str(), a.charge) {
                        ("C", 0) => 4,
                        ("N", 0) => 3,
                        ("N", 1) => 4,
                        ("O", 0) => 2,
                        ("F" | "Cl" | "Br" | "I", 0) => 1,
                        _ => continue,
                    };
                    let used = a.explicit_h + bonds.iter().map(|b| u32::from(b.order)).sum::<u32>();
                    assert!(
                        used <= max,
                        "{} edge {}–{} produces atom {} valence {used}",
                        template.name,
                        bond.a,
                        bond.b,
                        a.id
                    );
                }
            }
        }
    }
}

fn hexagons(centers: &[(i32, i32)]) -> Document {
    // An independent hexagonal lattice fixture: coordinates, not the placement
    // code under test, decide which vertices and edges are shared.
    let mut d = Document::default();
    let mut vertices = std::collections::BTreeMap::new();
    for &(x, y) in centers {
        let ids: Vec<_> = [(0, -2), (1, -1), (1, 1), (0, 2), (-1, 1), (-1, -1)]
            .into_iter()
            .map(|(dx, dy)| {
                *vertices.entry((x + dx, y + dy)).or_insert_with(|| {
                    d.add_atom(
                        "C",
                        Point::new((x + dx) as f32 * 21. * 3_f32.sqrt(), (y + dy) as f32 * 21.),
                    )
                })
            })
            .collect();
        for (&a, &b) in ids.iter().zip(ids.iter().cycle().skip(1)).take(6) {
            d.add_bond(a, b, 4, "plain");
        }
    }
    d
}
#[test]
fn phenanthrene_bay_closes_to_pyrene_with_four_shared_vertices() {
    let circular = hexagons(&[(0, 0), (2, 0), (3, -3)]);
    let expected = hexagons(&[(0, 0), (2, 0), (3, -3), (1, -3)]);
    assert_eq!(circular.atoms.len(), 14);
    assert_eq!(expected.atoms.len(), 16);
    let mut kekule = circular.clone();
    let molecule = prepare(&circular).unwrap();
    let drawing = reshiki::chemistry::document::for_drawing(&molecule, &circular).unwrap();
    for b in &drawing.molecule().state.graph.bonds {
        let a = molecule.ids[b.a];
        let z = molecule.ids[b.b];
        kekule.add_bond(a, z, b.order, "plain");
    }
    assert!(kekule.bonds.iter().all(|b| matches!(b.order, 1 | 2)));
    for base in [circular, kekule] {
        for alternate in [false, true] {
            let point = midpoint(&base, 1, 2);
            let direction = Some(Point::new(21. * 3_f32.sqrt(), -63.));
            let (result, _) = tool(alternate).place(&base, point, direction, 5.).unwrap();
            assert_eq!((result.atoms.len(), result.bonds.len()), (16, 19));
            assert_eq!(identity(&result), identity(&expected));
            no_duplicates(&result);
            keeps_positions(&base, &result);
        }
    }
}

#[test]
fn heterocyclic_donor_hydrogen_and_nitrogen_identity_survive_fusion() {
    let benzene = Preset::Benzene.document(42., false);
    let point = midpoint(&benzene, 1, 2);
    for name in ["Pyridine", "Pyrrole", "Furan", "Thiophene"] {
        let part = &templates::LIBRARY
            .iter()
            .find(|t| t.name == name)
            .unwrap()
            .document;
        for source in &part.bonds {
            if [source.a, source.b]
                .iter()
                .any(|id| part.atom(*id).unwrap().element != "C")
            {
                continue;
            }
            let (result, ids) = templates::place_with_mode(
                &benzene,
                part,
                point,
                None,
                5.,
                Anchor::Bond(source.a, source.b),
                Connection::FuseBond,
            )
            .unwrap_or_else(|e| panic!("{name} edge {}–{}: {e}", source.a, source.b));
            let mapped: std::collections::HashMap<_, _> =
                part.all_ids().into_iter().zip(ids).collect();
            for a in part.atoms.iter().filter(|a| a.element != "C") {
                let after = result.atom(mapped[&a.id]).unwrap();
                assert_eq!(after.element, a.element);
                assert_eq!(after.explicit_h, a.explicit_h);
            }
            identity(&result);
            no_duplicates(&result);
        }
    }
}
