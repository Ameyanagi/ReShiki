use super::*;
use crate::models3d::{self, Display};
use reshiki_model::{attachments::Kind, science};

const CLUSTER: &str =
    include_str!("../../../../tests/fixtures/xtb/diruthenium_tetrahydride_gxtb.xyz");
const GFN2_CLUSTER: &str =
    include_str!("../../../../tests/fixtures/xtb/diruthenium_tetrahydride_gfn2.xyz");
const FERROCENE: &str = include_str!("../../../../tests/fixtures/xtb/ferrocene.xyz");

#[test]
fn diruthenium_xyz_retains_four_bridges_haptic_rings_and_the_atomic_frame() {
    let (mut doc, warnings) = read("xyz", CLUSTER).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let (frame, _) = crate::qcread::read_xyz_geometry(CLUSTER).unwrap();
    let original: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| a.attachment.is_none())
        .map(|a| a.id)
        .collect();
    assert_eq!(original.len(), 56);
    let anchors: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| a.attachment == Some(Kind::MultiCenter))
        .collect();
    assert_eq!(anchors.len(), 2);
    for (anchor, metal) in anchors.iter().zip(&original[..2]) {
        assert_eq!(anchor.centroid.len(), 5);
        assert!(
            anchor
                .centroid
                .iter()
                .all(|id| doc.atom(*id).unwrap().element == "C")
        );
        assert_eq!(
            doc.bonds
                .iter()
                .filter(|b| b.a == anchor.id || b.b == anchor.id)
                .count(),
            1
        );
        assert!(doc.bonds.iter().any(|b| b.a == anchor.id && b.b == *metal));
        assert!(
            !doc.bonds
                .iter()
                .any(|b| (b.a == *metal && anchor.centroid.contains(&b.b))
                    || (b.b == *metal && anchor.centroid.contains(&b.a)))
        );
    }
    assert!(
        !doc.bonds
            .iter()
            .any(|b| b.a == original[0] && b.b == original[1])
    );
    for hydrogen in &original[52..] {
        assert_eq!(doc.atom(*hydrogen).unwrap().element, "H");
        let contacts: Vec<_> = doc
            .bonds
            .iter()
            .filter(|b| b.a == *hydrogen || b.b == *hydrogen)
            .map(|b| if b.a == *hydrogen { b.b } else { b.a })
            .collect();
        assert_eq!(contacts.len(), 2);
        assert!(contacts.iter().all(|id| original[..2].contains(id)));
    }
    assert_eq!(doc.models[0].atoms.len(), 56);
    assert_eq!(
        doc.models[0].frame_sources(),
        original.iter().copied().map(Some).collect::<Vec<_>>()
    );
    assert_eq!(doc.models[0].linked_atoms.len(), 58);
    let launch = science::launch(&doc, original[0], 0, 1).unwrap();
    assert_eq!(launch.frame.numbers, frame.numbers);
    assert!(
        launch
            .frame
            .positions_angstrom
            .iter()
            .zip(&frame.positions_angstrom)
            .all(|(a, b)| a.map(|x| x as f32) == b.map(|x| x as f32))
    );
    let bonds = science::frame_bonds(&doc, &launch.atoms).unwrap();
    assert!(!bonds.iter().any(|(a, b, _)| *a < 2 && *b < 2));
    assert_eq!(bonds.iter().filter(|(a, b, _)| (*a < 2 && (52..56).contains(b))
        || (*b < 2 && (52..56).contains(a))).count(), 8);
    assert_eq!(
        bonds
            .iter()
            .filter(|(a, b, _)| (*a < 2 && (2..7).contains(b)) || (*a < 2 && (27..32).contains(b)))
            .count(),
        10
    );
    assert_eq!(
        reshiki_model::molecule_names::component(&doc, original[0]).len(),
        58
    );
    // This is the GUI's direct projection path for imported coordinates.
    let mut model = doc.models[0].clone();
    model.rotation = models3d::turn(model.rotation, 37., -24., false);
    models3d::lay_out_flat(&mut doc, &model);
    models3d::set_display(&mut doc, 0, Display::TwoD);
    for anchor in doc
        .atoms
        .iter()
        .filter(|a| a.attachment == Some(Kind::MultiCenter))
    {
        let members: Vec<_> = anchor
            .centroid
            .iter()
            .map(|id| doc.atom(*id).unwrap())
            .collect();
        let n = members.len() as f32;
        assert!(
            (anchor.position.x - members.iter().map(|a| a.position.x).sum::<f32>() / n).abs()
                < 1e-4
        );
        assert!((anchor.depth - members.iter().map(|a| a.depth).sum::<f32>() / n).abs() < 1e-4);
    }
    doc.validate().unwrap();
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.models, doc.models);
    assert_eq!(reopened.atoms, doc.atoms);
    assert_eq!(reopened.bonds, doc.bonds);
    let again = science::launch(&reopened, original[0], 0, 1).unwrap();
    assert_eq!(again.frame, launch.frame);
    assert!(
        crate::scene::primitives(&reopened)
            .iter()
            .all(|p| !matches!(p,
        crate::scene::Primitive::Text { text, .. } if text == "*"))
    );
}

#[test]
fn both_optimized_hydride_cores_keep_their_contacts_without_inventing_a_direct_metal_edge() {
    for (source, bridges, terminal) in [(CLUSTER, 4, 0), (GFN2_CLUSTER, 2, 2)] {
        let (doc, _) = read("xyz", source).unwrap();
        let ids: Vec<_> = doc.models[0]
            .frame_sources()
            .into_iter()
            .flatten()
            .collect();
        let metals = &ids[..2];
        let hydride_contacts: Vec<_> = ids[52..]
            .iter()
            .map(|hydrogen| {
                assert_eq!(doc.atom(*hydrogen).unwrap().element, "H");
                doc.bonds
                    .iter()
                    .filter(|bond| {
                        (bond.a == *hydrogen && metals.contains(&bond.b))
                            || (bond.b == *hydrogen && metals.contains(&bond.a))
                    })
                    .count()
            })
            .collect();
        assert_eq!(
            hydride_contacts.iter().filter(|&&count| count == 2).count(),
            bridges
        );
        assert_eq!(
            hydride_contacts.iter().filter(|&&count| count == 1).count(),
            terminal
        );
        assert_eq!(
            hydride_contacts.iter().sum::<usize>(),
            2 * bridges + terminal
        );
        assert!(
            !doc.bonds
                .iter()
                .any(|bond| metals.contains(&bond.a) && metals.contains(&bond.b))
        );
        assert_eq!(
            doc.atoms
                .iter()
                .filter(|atom| atom.attachment == Some(Kind::MultiCenter))
                .count(),
            2
        );
        let (original, _) = crate::qcread::read_xyz_geometry(source).unwrap();
        let actual: Vec<_> = doc.models[0].atoms.iter().map(|atom| atom.xyz).collect();
        assert_eq!(
            actual,
            original
                .positions_angstrom
                .iter()
                .map(|point| point.map(|value| value as f32))
                .collect::<Vec<_>>()
        );
        let distance = |points: &[[f64; 3]]| {
            (0..3)
                .map(|axis| (points[0][axis] - points[1][axis]).powi(2))
                .sum::<f64>()
                .sqrt()
        };
        let actual: Vec<_> = actual.iter().map(|point| point.map(f64::from)).collect();
        assert!((distance(&actual) - distance(&original.positions_angstrom)).abs() < 1e-6);
    }
}

#[test]
fn haptic_metal_proximity_rule_is_preserved_below_two_shared_hydrides() {
    let lines: Vec<_> = CLUSTER.lines().skip(2).collect();
    for hydrides in 0..=4 {
        let text = format!(
            "{}\nbridge-count control\n{}\n",
            52 + hydrides,
            lines[..52 + hydrides].join("\n")
        );
        let (doc, _) = read("xyz", &text).unwrap();
        let metals: Vec<_> = doc
            .atoms
            .iter()
            .filter(|atom| atom.element == "Ru")
            .map(|atom| atom.id)
            .collect();
        assert_eq!(
            doc.atoms
                .iter()
                .filter(|atom| atom.attachment == Some(Kind::MultiCenter))
                .count(),
            2
        );
        assert_eq!(
            doc.bonds
                .iter()
                .filter(|bond| metals.contains(&bond.a) && metals.contains(&bond.b))
                .count(),
            usize::from(hydrides < 2)
        );
        let hydrogens: Vec<_> = doc.models[0]
            .frame_sources()
            .into_iter()
            .flatten()
            .skip(52)
            .collect();
        assert_eq!(hydrogens.len(), hydrides);
        assert!(hydrogens.iter().all(|hydrogen| {
            doc.bonds
                .iter()
                .filter(|bond| {
                    (bond.a == *hydrogen && metals.contains(&bond.b))
                        || (bond.b == *hydrogen && metals.contains(&bond.a))
                })
                .count()
                == 2
        }));
    }
}

#[test]
fn haptic_recognition_does_not_create_hydride_or_ring_contacts() {
    for (xyz, bonds) in [
        ("3\nterminal hydride\nRu 0 0 0\nH 1.7 0 0\nRu 5 0 0\n", 1),
        ("2\nordinary C-H\nC 0 0 0\nH 1.09 0 0\n", 1),
        ("2\nbare alloy control\nFe 0 0 0\nCu 2.5 0 0\n", 1),
    ] {
        let (doc, _) = read("xyz", xyz).unwrap();
        assert_eq!(doc.bonds.len(), bonds);
        assert!(doc.atoms.iter().all(|a| a.attachment.is_none()));
    }
    let (doc, _) = read("xyz", FERROCENE).unwrap();
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|a| a.attachment == Some(Kind::MultiCenter))
            .count(),
        2
    );
    // The same carbon ring becomes a saturated pentagon or an off-centre
    // sigma contact; neither coordinate set establishes eta5 coordination.
    for (radius, metal_x) in [(1.31, 0.), (1.216431, 1.4)] {
        let mut text = "6\nring control\n".to_owned();
        text.push_str(&format!("Fe {metal_x} 0 1.66\n"));
        for i in 0..5 {
            let angle = i as f64 * std::f64::consts::TAU / 5.;
            text.push_str(&format!(
                "C {} {} 0\n",
                radius * angle.cos(),
                radius * angle.sin()
            ));
        }
        let (doc, _) = read("xyz", &text).unwrap();
        assert!(doc.atoms.iter().all(|a| a.attachment.is_none()));
    }
}

#[test]
fn haptic_membership_changes_stale_calculation_baselines() {
    let (doc, _) = read("xyz", CLUSTER).unwrap();
    let ids: Vec<_> = doc.models[0]
        .frame_sources()
        .into_iter()
        .flatten()
        .collect();
    let baseline = science::baseline(&doc, &ids);
    let mut turned = doc.clone();
    turned.models[0].rotation = models3d::turn(turned.models[0].rotation, 42., -15., false);
    models3d::reconcile(&mut turned, &doc);
    assert_eq!(turned.models.len(), 1);
    let mut changed = doc.clone();
    let anchor = changed
        .atoms
        .iter_mut()
        .find(|a| a.attachment == Some(Kind::MultiCenter))
        .unwrap();
    anchor.centroid.pop();
    assert_ne!(science::baseline(&changed, &ids), baseline);
    models3d::reconcile(&mut changed, &doc);
    assert!(
        changed.models.is_empty(),
        "changed haptic membership invalidates imported topology"
    );
    assert_eq!(science::baseline(&doc, &ids), baseline);
}

#[tokio::test]
async fn haptic_cluster_survives_normal_cdxml_drawing_export_and_import() {
    use crate::engine::{LocalEngine, Request};
    let (mut doc, _) = read("xyz", CLUSTER).unwrap();
    models3d::set_display(&mut doc, 0, Display::TwoD);
    let engine = LocalEngine::default();
    let mut request = Request::molecule("export", doc);
    request.format = Some("cdxml".into());
    let output = engine.request(request).await.unwrap().output.unwrap();
    assert_eq!(output.matches("NodeType=\"MultiAttachment\"").count(), 2);
    let imported = engine
        .request(Request::import("cdxml", &output))
        .await
        .unwrap();
    let doc = imported.document.unwrap();
    assert_eq!(doc.atoms.iter().filter(|a| a.element != "*").count(), 56);
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|a| a.attachment == Some(Kind::MultiCenter))
            .count(),
        2
    );
    let metals: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| a.element == "Ru")
        .map(|a| a.id)
        .collect();
    assert_eq!(metals.len(), 2);
    assert!(
        !doc.bonds
            .iter()
            .any(|bond| metals.contains(&bond.a) && metals.contains(&bond.b))
    );
    let bridges = doc
        .atoms
        .iter()
        .filter(|a| {
            a.element == "H"
                && metals.iter().all(|metal| {
                    doc.bonds
                        .iter()
                        .any(|b| (b.a == a.id && b.b == *metal) || (b.b == a.id && b.a == *metal))
                })
        })
        .count();
    assert_eq!(bridges, 4);
    doc.validate().unwrap();
}

#[tokio::test]
async fn an_explicit_direct_metal_bond_survives_drawing_export_and_import() {
    use crate::engine::{LocalEngine, Request};
    let (mut doc, _) = read("xyz", CLUSTER).unwrap();
    let metals: Vec<_> = doc
        .atoms
        .iter()
        .filter(|atom| atom.element == "Ru")
        .map(|atom| atom.id)
        .collect();
    doc.add_bond(metals[0], metals[1], 1, "plain");
    models3d::set_display(&mut doc, 0, Display::TwoD);
    let engine = LocalEngine::default();
    let mut request = Request::molecule("export", doc);
    request.format = Some("cdxml".into());
    let output = engine.request(request).await.unwrap().output.unwrap();
    let doc = engine
        .request(Request::import("cdxml", &output))
        .await
        .unwrap()
        .document
        .unwrap();
    let metals: Vec<_> = doc
        .atoms
        .iter()
        .filter(|atom| atom.element == "Ru")
        .map(|atom| atom.id)
        .collect();
    assert_eq!(
        doc.bonds
            .iter()
            .filter(|bond| metals.contains(&bond.a) && metals.contains(&bond.b) && bond.order == 1)
            .count(),
        1
    );
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|atom| atom.attachment == Some(Kind::MultiCenter))
            .count(),
        2
    );
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|atom| atom.element == "H"
                && metals.iter().all(|metal| doc
                    .bonds
                    .iter()
                    .any(|bond| (bond.a == atom.id && bond.b == *metal)
                        || (bond.b == atom.id && bond.a == *metal))))
            .count(),
        4
    );
}

#[test]
fn reordered_haptic_molecules_preserve_frame_order_and_separated_metals_stay_unbonded() {
    let signature = |doc: &Document, order: &[usize]| {
        let ids: std::collections::HashMap<_, _> = doc
            .atoms
            .iter()
            .filter(|a| a.attachment.is_none())
            .zip(order)
            .map(|(a, i)| (a.id, *i))
            .collect();
        let mut contacts: Vec<_> = doc
            .atoms
            .iter()
            .filter(|a| a.attachment == Some(Kind::MultiCenter))
            .map(|a| {
                let metal = doc
                    .bonds
                    .iter()
                    .find_map(|b| {
                        if b.a == a.id {
                            Some(b.b)
                        } else if b.b == a.id {
                            Some(b.a)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let mut ring: Vec<_> = a.centroid.iter().map(|id| ids[id]).collect();
                ring.sort_unstable();
                (ids[&metal], ring)
            })
            .collect();
        contacts.sort_unstable();
        contacts
    };
    let baseline = read("xyz", CLUSTER).unwrap().0;
    let expected = signature(&baseline, &(0..56).collect::<Vec<_>>());
    let lines: Vec<_> = CLUSTER.lines().skip(2).collect();
    let reversed = format!(
        "{}\nreordered\n{}\n",
        lines.len(),
        lines.iter().rev().copied().collect::<Vec<_>>().join("\n")
    );
    let (doc, _) = read("xyz", &reversed).unwrap();
    assert_eq!(
        signature(&doc, &(0..56).rev().collect::<Vec<_>>()),
        expected
    );
    let (frame, _) = crate::qcread::read_xyz_geometry(&reversed).unwrap();
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|a| a.attachment == Some(Kind::MultiCenter))
            .count(),
        2
    );
    let metal = doc.atoms.iter().find(|a| a.element == "Ru").unwrap().id;
    let launch = science::launch(&doc, metal, 0, 1).unwrap();
    assert_eq!(launch.frame.numbers, frame.numbers);
    assert!(
        launch
            .frame
            .positions_angstrom
            .iter()
            .zip(&frame.positions_angstrom)
            .all(|(a, b)| a.map(|x| x as f32) == b.map(|x| x as f32))
    );
    let rotation = crate::geometry::Rotation::from_quaternion([0.91, 0.2, -0.3, 0.1]);
    for reverse in [false, true] {
        let (frame, _) = crate::qcread::read_xyz_geometry(CLUSTER).unwrap();
        let order: Vec<_> = if reverse {
            (0..56).rev().collect()
        } else {
            (0..56).collect()
        };
        let mut text = "56\nrotated optimized cluster\n".to_owned();
        for &i in &order {
            let [x, y, z] = frame.positions_angstrom[i];
            let p = rotation.apply(crate::geometry::Point3 { x, y, z });
            text.push_str(&format!("{} {} {} {}\n", frame.numbers[i], p.x, p.y, p.z));
        }
        let (doc, _) = read("xyz", &text).unwrap();
        assert_eq!(signature(&doc, &order), expected, "reverse={reverse}");
    }
    let (mut frame, _) = crate::qcread::read_xyz_geometry(CLUSTER).unwrap();
    for index in std::iter::once(1).chain(27..52) {
        frame.positions_angstrom[index][0] += 20.;
    }
    let mut xyz = format!("{}\nseparate complexes\n", frame.numbers.len());
    for (number, [x, y, z]) in frame.numbers.iter().zip(&frame.positions_angstrom) {
        xyz.push_str(&format!("{number} {x} {y} {z}\n"));
    }
    let (doc, _) = read("xyz", &xyz).unwrap();
    assert_eq!(
        doc.atoms
            .iter()
            .filter(|a| a.attachment == Some(Kind::MultiCenter))
            .count(),
        2
    );
    let metals: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| a.element == "Ru")
        .map(|a| a.id)
        .collect();
    assert!(
        !doc.bonds
            .iter()
            .any(|b| metals.contains(&b.a) && metals.contains(&b.b))
    );
}
