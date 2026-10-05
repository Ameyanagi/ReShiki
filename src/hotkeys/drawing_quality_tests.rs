use super::*;
use crate::engine::{LocalEngine, Request};

#[test]
fn triple_edits_reject_overvalence_and_leave_cyclic_geometry_intact() -> Result<(), String> {
    let ring = crate::rings::Preset::Regular.document(42., false);
    let edge = ring.bonds.first().ok_or("Missing ring edge")?;
    let edited = bond_edit(&ring, edge.a, edge.b, BondPreset::Triple)?;
    for atom in &ring.atoms {
        assert_eq!(
            edited.atom(atom.id).ok_or("Missing atom")?.position,
            atom.position
        );
    }
    let mut crowded = ring.clone();
    let branch = crowded.add_atom("C", Point::new(100., 100.));
    crowded.add_bond(edge.a, branch, 1, "plain");
    let before = crowded.clone();
    assert!(bond_edit(&crowded, edge.a, edge.b, BondPreset::Triple).is_err());
    assert_eq!(crowded, before);
    assert!(
        atom_edit(&ring, edge.a, "z", 42.)
            .ok_or("Missing alkyne")?
            .is_err()
    );
    Ok(())
}

#[test]
fn straightening_keeps_a_remote_group_and_wedge_together() -> Result<(), String> {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(63., 36.373));
    let d = doc.add_atom("C", Point::new(105., 36.373));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    BondPreset::Wedge.place(&mut doc, c, d);
    doc = atom_text::apply(&doc, d, "Boc", Mode::Group)?;
    let before = doc.clone();
    let result = bond_edit(&doc, a, b, BondPreset::Triple)?;
    assert_eq!(result.abbreviations, before.abbreviations);
    for original in &before.bonds {
        let changed = result
            .bonds
            .iter()
            .find(|e| e.a == original.a && e.b == original.b)
            .ok_or("Lost bond")?;
        if original.a != a || original.b != b {
            assert_eq!(changed, original);
        }
        let distance = |doc: &Document| -> Result<f32, String> {
            Ok(doc
                .atom(original.a)
                .ok_or("Missing atom")?
                .position
                .distance(doc.atom(original.b).ok_or("Missing atom")?.position))
        };
        assert!((distance(&result)? - distance(&before)?).abs() < 0.001);
    }
    result.validate()?;
    Ok(())
}

#[tokio::test]
async fn sulfonyl_replaces_terminal_and_internal_carbons_without_an_sh_label() -> Result<(), String>
{
    let engine = LocalEngine::default();
    for terminal in [true, false] {
        let mut doc = Document::default();
        let left = doc.add_atom("C", Point::new(-36.373, -21.));
        let center = doc.add_atom("C", Point::default());
        doc.add_bond(left, center, 1, "plain");
        if !terminal {
            let right = doc.add_atom("C", Point::new(36.373, -21.));
            doc.add_bond(center, right, 1, "plain");
        }
        let (result, _) = atom_edit(&doc, center, "k", 42.).ok_or("Missing sulfonyl")??;
        assert_eq!(result.atom(center).ok_or("Missing sulfur")?.element, "S");
        assert_eq!(result.atoms.len(), 5);
        let response = engine.request(Request::molecule("analyze", result)).await?;
        assert_eq!(
            response.analysis.ok_or("Missing analysis")?.formula,
            "C2H6O2S"
        );
        assert_eq!(
            response
                .document
                .ok_or("Missing drawing")?
                .atom(center)
                .ok_or("Missing sulfur")?
                .label_h,
            0
        );
    }
    Ok(())
}

#[test]
fn branch_pairs_are_symmetric_and_tert_butyl_arms_have_even_angles() -> Result<(), String> {
    for rotation in [0_f32, 17.3, 90., 211.] {
        let mut doc = Document::default();
        let center = doc.add_atom("C", Point::default());
        for angle in [rotation + 210., rotation + 330.] {
            let theta = angle.to_radians();
            let other = doc.add_atom("C", Point::new(42. * theta.cos(), 42. * theta.sin()));
            doc.add_bond(center, other, 1, "plain");
        }
        let (gem, _) = atom_edit(&doc, center, "9", 42.).ok_or("Missing dimethyl")??;
        let added: Vec<_> = gem
            .atoms
            .iter()
            .filter(|a| doc.atom(a.id).is_none())
            .collect();
        assert_eq!(added.len(), 2);
        let expected = (rotation + 90.).to_radians();
        let sum = added.iter().fold(Point::default(), |p, a| {
            p.offset(a.position.x, a.position.y)
        });
        assert!((sum.x / sum.distance(Point::default()) - expected.cos()).abs() < 0.0001);
        assert!((sum.y / sum.distance(Point::default()) - expected.sin()).abs() < 0.0001);
        if let [a, b] = added.as_slice() {
            let cosine = (a.position.x * b.position.x + a.position.y * b.position.y) / (42. * 42.);
            assert!((cosine - 0.5).abs() < 0.0001);
        }
        let mut ethane = Document::default();
        let c = ethane.add_atom("C", Point::default());
        let a = (rotation + 180.).to_radians();
        let other = ethane.add_atom("C", Point::new(42. * a.cos(), 42. * a.sin()));
        ethane.add_bond(c, other, 1, "plain");
        let (tbu, _) = atom_edit(&ethane, c, "K", 42.).ok_or("Missing tert-butyl")??;
        let mut angles: Vec<_> = tbu
            .atoms
            .iter()
            .filter(|a| a.id != c)
            .map(|a| {
                a.position
                    .y
                    .atan2(a.position.x)
                    .rem_euclid(std::f32::consts::TAU)
            })
            .collect();
        angles.sort_by(f32::total_cmp);
        for (&a, &b) in angles
            .iter()
            .zip(angles.iter().cycle().skip(1))
            .take(angles.len())
        {
            assert!(
                ((b - a).rem_euclid(std::f32::consts::TAU) - std::f32::consts::FRAC_PI_2).abs()
                    < 0.0001
            );
        }
    }
    Ok(())
}

#[test]
fn isolated_ring_hotkeys_keep_the_requested_length() -> Result<(), String> {
    for element in ["C", "N"] {
        for length in [24., 60., 85.] {
            let mut doc = Document::default();
            let id = doc.add_atom(element, Point::default());
            for key in ["3", "6", "7", "v", "u"] {
                let (result, _) =
                    ring_edit(&doc, Some(id), None, key, length).ok_or("Missing ring")??;
                for b in &result.bonds {
                    let a = result.atom(b.a).ok_or("Missing endpoint")?.position;
                    let z = result.atom(b.b).ok_or("Missing endpoint")?.position;
                    assert!(
                        (a.distance(z) - length).abs() < 0.001,
                        "{key} on {element}: {} vs {length}",
                        a.distance(z)
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn triple_bond_edit_straightens_both_branches_preserving_lengths_and_remote_content()
-> Result<(), String> {
    for rotation in [0_f32, 17.3, 83., 213.] {
        let mut doc = Document::default();
        let mut point = Point::default();
        let mut ids = Vec::new();
        for i in 0..6 {
            ids.push(doc.add_atom("C", point));
            let theta = (rotation + if i % 2 == 0 { -30. } else { 30. }).to_radians();
            point = point.offset(42. * theta.cos(), 42. * theta.sin());
        }
        for pair in ids.windows(2) {
            if let [a, b] = pair {
                doc.add_bond(*a, *b, 1, "plain");
            }
        }
        let a = *ids.get(2).ok_or("Missing atom")?;
        let b = *ids.get(3).ok_or("Missing atom")?;
        let remote = doc.add_atom("O", Point::new(600., -400.));
        let original = doc.clone();
        let result = bond_edit(&doc, a, b, BondPreset::Triple)?;
        for id in [a, b, remote] {
            assert_eq!(
                result.atom(id).ok_or("Missing atom")?.position,
                doc.atom(id).ok_or("Missing atom")?.position
            );
        }
        for bond in &result.bonds {
            assert!(
                (result
                    .atom(bond.a)
                    .ok_or("Missing atom")?
                    .position
                    .distance(result.atom(bond.b).ok_or("Missing atom")?.position)
                    - 42.)
                    .abs()
                    < 0.001
            );
        }
        for (index, other) in [(2, b), (3, a)] {
            let id = *ids.get(index).ok_or("Missing atom")?;
            let neighbor = *ids
                .get(if index == 2 { 1 } else { 4 })
                .ok_or("Missing neighbor")?;
            let p = result.atom(id).ok_or("Missing atom")?.position;
            let q = result.atom(other).ok_or("Missing atom")?.position;
            let r = result.atom(neighbor).ok_or("Missing atom")?.position;
            let cosine = ((q.x - p.x) * (r.x - p.x) + (q.y - p.y) * (r.y - p.y))
                / (p.distance(q) * p.distance(r));
            assert!((cosine + 1.).abs() < 0.0001);
        }
        assert_eq!(doc, original);
    }
    Ok(())
}
