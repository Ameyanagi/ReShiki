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
                assert!(
                    (candidate
                        .atom(bond.a)
                        .ok_or("a")?
                        .position
                        .distance(candidate.atom(bond.b).ok_or("b")?.position)
                        - 42.)
                        .abs()
                        < 0.001
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
