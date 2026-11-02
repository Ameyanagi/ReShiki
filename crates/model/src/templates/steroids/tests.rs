use super::*;

fn references() -> Vec<Facts> {
    serde_json::from_str(include_str!("../../../../../assets/steroids.json")).unwrap()
}

#[test]
fn all_steroids_have_regular_fused_rings_in_orientation_2a() {
    let rings: &[&[u64]] = &[
        &[1, 2, 3, 4, 5, 10],
        &[5, 6, 7, 8, 9, 10],
        &[8, 9, 11, 12, 13, 14],
        &[13, 14, 15, 16, 17],
    ];
    for facts in references() {
        for length in [24., 42., 80.] {
            for implicit in [false, true] {
                let doc = geometry(&facts, length, implicit).unwrap();
                for ring in rings {
                    for i in 0..ring.len() {
                        let p = doc.atom(ring[i]).unwrap().position;
                        let q = doc.atom(ring[(i + 1) % ring.len()]).unwrap().position;
                        let r = doc.atom(ring[(i + 2) % ring.len()]).unwrap().position;
                        assert!(
                            (p.distance(q) - length).abs() < length * 0.000001,
                            "{} {ring:?}: unequal edge",
                            facts.name
                        );
                        let cosine = ((p.x - q.x) * (r.x - q.x) + (p.y - q.y) * (r.y - q.y))
                            / (p.distance(q) * q.distance(r));
                        let expected = -(std::f32::consts::TAU / ring.len() as f32).cos();
                        assert!(
                            (cosine - expected).abs() < 0.000002,
                            "{} {ring:?}: irregular angle {cosine}",
                            facts.name
                        );
                    }
                }
                let center = |ring: &[u64]| {
                    let n = ring.len() as f32;
                    ring.iter().fold(Point::default(), |sum, id| {
                        let p = doc.atom(*id).unwrap().position;
                        sum.offset(p.x / n, p.y / n)
                    })
                };
                let a = center(rings[0]);
                let d = center(rings[3]);
                assert!(
                    d.x > a.x && d.y < a.y,
                    "{}: D must be upper right of A",
                    facts.name
                );
                for bond in &doc.bonds {
                    assert!(
                        (doc.atom(bond.a)
                            .unwrap()
                            .position
                            .distance(doc.atom(bond.b).unwrap().position)
                            - length)
                            .abs()
                            < length * 0.000002,
                        "{} {bond:?}",
                        facts.name
                    );
                }
            }
        }
    }
}

#[test]
fn implicit_variant_omits_only_8_9_14_h_and_never_leaves_a_bare_stereo_bond() {
    for facts in references() {
        let full = geometry(&facts, 42., false).unwrap();
        let implicit = geometry(&facts, 42., true).unwrap();
        assert_eq!(full.atoms.len(), implicit.atoms.len() + 3);
        assert_eq!(full.bonds.len(), implicit.bonds.len() + 3);
        for center in [8, 9, 14] {
            assert_eq!(full.atom(center + 100).unwrap().element, "H");
            assert!(implicit.atom(center + 100).is_none());
            assert!(implicit.atom(center).unwrap().stereo.is_some());
        }
        for doc in [&full, &implicit] {
            for bond in &doc.bonds {
                if bond.a <= 17 && bond.b <= 17 {
                    assert_eq!(
                        bond.display, "plain",
                        "No unlabelled junction stereo: {}",
                        facts.name
                    );
                }
                if bond.b >= 100 {
                    assert_eq!(doc.atom(bond.b).unwrap().element, "H");
                    assert!(crate::atom_labels::visible(doc.atom(bond.b).unwrap(), doc));
                    assert_eq!(
                        bond.display,
                        match bond.a {
                            8 | 10 | 13 => "bold",
                            _ => "hash",
                        }
                    );
                }
            }
            for (junction, methyl) in [(10, 19), (13, 18)] {
                if doc.atom(methyl).is_some() {
                    assert_eq!(
                        doc.bond_between(junction, methyl).unwrap().1.display,
                        "bold"
                    );
                }
            }
            for (center, oxygen) in [(3, 28), (17, 29)] {
                if let Some((_, bond)) = doc.bond_between(center, oxygen)
                    && bond.order == 1
                    && doc.atom(center).unwrap().stereo.is_some()
                {
                    assert_eq!(bond.display, "wedge", "β hydroxyl at C{center}");
                }
            }
            for stereo in &facts.stereo {
                let actual = doc.atom(stereo.center).unwrap().stereo.as_ref().unwrap();
                assert_eq!(actual.winding, stereo.winding);
                assert_eq!(
                    actual.neighbors.len(),
                    stereo.neighbors.len() + usize::from(doc.atom(stereo.center + 100).is_some())
                );
            }
            let native = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
            assert_eq!(native, doc.current());
        }
        if matches!(facts.name.as_str(), "Gonane" | "Estrane") {
            assert!(full.atom(5).unwrap().stereo.is_none());
            assert!(full.atom(105).is_none());
        }
    }
}

#[test]
fn explicit_bold_and_hashed_drawing_agrees_with_frozen_chemical_parity() {
    use crate::chemistry::{document, smiles::write};
    for facts in references() {
        let mut doc = geometry(&facts, 42., false).unwrap();
        let mut parity_only = doc.clone();
        for bond in &mut parity_only.bonds {
            bond.display = "plain".into();
        }
        let tagged = document::prepare(&parity_only).unwrap();
        let tagged = write::write(&tagged.state, write::Options::default())
            .unwrap()
            .text;
        for atom in &mut doc.atoms {
            atom.stereo = None;
        }
        let drawn = document::prepare(&doc).unwrap();
        let drawn = write::write(&drawn.state, write::Options::default())
            .unwrap()
            .text;
        assert_eq!(
            tagged, drawn,
            "{} must be readable from its drawing",
            facts.name
        );
    }
}

#[test]
fn steroid_catalog_has_eight_parents_and_sixteen_variants_and_rejects_invalid_scale() {
    let facts = references();
    assert_eq!(facts.len(), 8);
    assert_eq!(templates().unwrap().len(), 16);
    for fact in &facts {
        for length in [f32::NAN, f32::INFINITY, -1., 0., 0.5, 10_001.] {
            assert!(geometry(fact, length, false).is_err());
        }
    }
}
