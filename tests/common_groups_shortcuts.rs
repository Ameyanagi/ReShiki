use reshiki::{
    atom_text::{self, Mode},
    document::{Document, Point},
    editing,
    engine::{LocalEngine, Request},
    hotkeys,
};
#[tokio::test]
async fn additional_group_shortcuts_preserve_atoms_charges_counts_and_interchange()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::default();
    let mut source = Document::default();
    let a = source.add_atom("C", Point::new(-84., 0.));
    let b = source.add_atom("C", Point::new(-42., 0.));
    let end = source.add_atom("C", Point::default());
    source.add_bond(a, b, 1, "plain");
    source.add_bond(b, end, 1, "plain");
    for (key, label, formula) in [("M", "MgBr", "C2H5BrMg"), ("Z", "N3", "C2H5N3")] {
        let (doc, focus) = hotkeys::atom_edit(&source, end, key, 42.).ok_or("Unassigned key")??;
        assert_eq!(focus, end);
        doc.validate()?;
        assert_eq!(doc.abbreviation(end).ok_or("group")?.label, label);
        let response = engine
            .request(Request::molecule("analyze", doc.clone()))
            .await?;
        assert_eq!(response.analysis.ok_or("formula")?.formula, formula);
        assert_eq!(doc, atom_text::apply(&source, end, label, Mode::Auto)?);
        for format in ["cdxml", "cdx"] {
            let mut req = Request::molecule("export", doc.clone());
            req.format = Some(format.into());
            let output = engine.request(req).await?.output.ok_or("export")?;
            let back = engine.request(Request::import(format, &output)).await?;
            assert_eq!(back.analysis.ok_or("imported properties")?.formula, formula);
        }
        if label == "N3" {
            let positive = doc
                .atoms
                .iter()
                .find(|a| a.charge == 1)
                .ok_or("Central N+")?;
            let positions: Vec<_> = doc
                .bonds
                .iter()
                .filter_map(|b| {
                    if b.a == positive.id {
                        doc.atom(b.b)
                    } else if b.b == positive.id {
                        doc.atom(b.a)
                    } else {
                        None
                    }
                })
                .map(|a| a.position)
                .collect();
            let [left, right] = positions.as_slice() else {
                return Err("Azide connectivity".into());
            };
            assert!(((left.x + right.x) / 2. - positive.position.x).abs() < 0.001);
            assert!(((left.y + right.y) / 2. - positive.position.y).abs() < 0.001);
            assert_eq!(doc.atoms.iter().map(|a| a.charge).sum::<i32>(), 0);
        }
    }
    Ok(())
}
#[test]
fn pi_ligands_keep_metal_focus_real_ring_chemistry_and_multicenter_targets()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::default();
    let fe = doc.add_atom("Fe", Point::default());
    doc.atom_mut(fe).ok_or("iron")?.charge = 2;
    let original = doc.clone();
    for count in 1..=2 {
        let (candidate, focus) = hotkeys::atom_edit(&doc, fe, "j", 42.).ok_or("Cp shortcut")??;
        assert_eq!(focus, fe);
        assert_eq!(
            candidate.atom(fe).ok_or("iron")?,
            original.atom(fe).ok_or("iron")?
        );
        assert_eq!(
            candidate
                .atoms
                .iter()
                .filter(|a| a.attachment.is_some())
                .count(),
            count
        );
        for anchor in candidate.atoms.iter().filter(|a| a.attachment.is_some()) {
            assert_eq!(anchor.centroid.len(), 5);
            assert_eq!(
                anchor.attachment,
                Some(reshiki::attachments::Kind::MultiCenter)
            );
            let ring: Vec<_> = candidate
                .bonds
                .iter()
                .filter(|b| anchor.centroid.contains(&b.a) && anchor.centroid.contains(&b.b))
                .collect();
            assert_eq!(ring.len(), 5);
            assert!(ring.iter().all(|b| b.order == 4));
            for bond in ring {
                let a = candidate.atom(bond.a).ok_or("a")?;
                let b = candidate.atom(bond.b).ok_or("b")?;
                assert!(
                    (a.position.distance(b.position).hypot(a.depth - b.depth) - 42.).abs() < 0.001
                );
            }
        }
        doc = candidate;
    }
    assert_eq!(reshiki::attachments::composition(&doc)?.formula, "C10H10Fe");
    assert_eq!(doc.atoms.iter().map(|a| a.charge).sum::<i32>(), 0);
    let mut metal = Document::default();
    let ru = metal.add_atom("Ru", Point::default());
    let (arene, _) = hotkeys::atom_edit(&metal, ru, "J", 42.).ok_or("Arene shortcut")??;
    assert_eq!(arene.atoms.iter().filter(|a| a.element == "C").count(), 6);
    assert_eq!(
        arene
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .ok_or("attachment")?
            .centroid
            .len(),
        6
    );
    assert_eq!(reshiki::attachments::composition(&arene)?.formula, "C6H6Ru");
    let mut crowded = Document::default();
    let ids = editing::ring(&mut crowded, Point::default(), 6, false, 0.);
    assert!(
        hotkeys::atom_edit(&crowded, *ids.first().ok_or("ring")?, "j", 42.)
            .ok_or("shortcut")?
            .is_err()
    );
    Ok(())
}
#[test]
fn pi_shortcuts_retain_a_retiltable_3d_plane_and_keep_contacts_behind_the_ring()
-> Result<(), Box<dyn std::error::Error>> {
    for (key, element, size) in [("j", "Fe", 5), ("J", "Ru", 6)] {
        let mut source = Document::default();
        let metal = source.add_atom(element, Point::new(20., 30.));
        source.atom_mut(metal).ok_or("metal")?.depth = 17.;
        let (doc, _) = hotkeys::atom_edit(&source, metal, key, 42.).ok_or("key")??;
        let anchor = doc
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .ok_or("anchor")?;
        let ring = &anchor.centroid;
        assert_eq!(ring.len(), size);
        let edges: Vec<_> = doc
            .bonds
            .iter()
            .filter(|b| ring.contains(&b.a) && ring.contains(&b.b))
            .collect();
        assert!(edges.iter().all(|b| b.order == 4 && b.projection));
        assert_eq!(edges.iter().filter(|b| b.display == "bold").count(), 1);
        assert_eq!(edges.iter().filter(|b| b.display == "wedge").count(), 2);
        assert!(doc.atoms.iter().all(|a| a.stereo.is_none()));
        for atom in doc.atoms.iter().filter(|a| ring.contains(&a.id)) {
            if atom.charge != 0 {
                assert_eq!(atom.charge, -1);
                assert!(atom.display.hide_charge);
            }
        }
        assert_eq!(
            doc.atoms.iter().map(|a| a.charge).sum::<i32>(),
            if key == "j" { -1 } else { 0 }
        );
        let circles = reshiki::aromatic::circles(&doc);
        assert_eq!(circles.len(), 1);
        assert!(circles.first().ok_or("circle")?.projected_axes.is_some());
        let contact = doc
            .bonds
            .iter()
            .position(|b| b.a == metal || b.b == metal)
            .ok_or("contact")?;
        assert_eq!(doc.bonds.get(contact).ok_or("contact")?.z_order, -1);
        let gaps = reshiki::crossings::gaps(&doc);
        assert!(!gaps.get(contact).ok_or("contact gaps")?.is_empty());
        assert!(
            gaps.iter()
                .enumerate()
                .all(|(i, g)| i == contact || g.is_empty())
        );
        let restored: Document = serde_json::from_str(&serde_json::to_string(&doc)?)?;
        assert_eq!(restored, doc);
        let mut flat = restored.clone();
        editing::transform_about(&mut flat, ring, anchor.position, 1., 30.);
        reshiki::projection::tilt(&mut flat, ring, -60., false);
        for &id in ring {
            assert!((flat.atom(id).ok_or("flat atom")?.depth - 17.).abs() < 0.001);
        }
        for bond in &edges {
            assert!(
                (flat
                    .atom(bond.a)
                    .ok_or("a")?
                    .position
                    .distance(flat.atom(bond.b).ok_or("b")?.position)
                    - 42.)
                    .abs()
                    < 0.001
            );
        }
        let mut tilted = restored.clone();
        let mut selection = ring.clone();
        selection.push(anchor.id);
        for (degrees, x) in [(22., true), (-18., false), (18., false), (-22., true)] {
            reshiki::projection::tilt(&mut tilted, &selection, degrees, x);
        }
        for a in &doc.atoms {
            let b = tilted.atom(a.id).ok_or("retained atom")?;
            assert!(a.position.distance(b.position) < 0.001);
            assert!((a.depth - b.depth).abs() < 0.001);
        }
        assert_eq!(tilted.bonds, doc.bonds);
        assert_eq!(
            reshiki::attachments::composition(&tilted)?,
            reshiki::attachments::composition(&doc)?
        );
        assert_eq!(source.atoms.len(), 1);
    }
    Ok(())
}
#[test]
fn replacing_a_colored_collapsed_ring_prunes_its_old_fill() -> Result<(), Box<dyn std::error::Error>>
{
    let mut doc = Document::default();
    let end = doc.add_atom("C", Point::default());
    doc = atom_text::apply(&doc, end, "Ph", Mode::Auto)?;
    let members = doc.abbreviation(end).ok_or("Ph")?.members.clone();
    doc.expand_abbreviations(&members);
    reshiki::ring_fills::apply(&mut doc, &members, Some([201, 224, 248]));
    doc.contract(&members, "Ph", "")?;
    let result = atom_text::apply(&doc, end, "MgBr", Mode::Auto)?;
    result.validate()?;
    assert!(result.ring_fills.is_empty());
    Ok(())
}
