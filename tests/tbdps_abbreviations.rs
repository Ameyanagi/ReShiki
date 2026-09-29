use reshiki::{
    atom_text::{self, Mode},
    chemistry::{abbreviations, document as chemistry},
    document::{Document, History, Point},
    editing,
    engine::{LocalEngine, Request},
    scene,
};

fn propane() -> (Document, u64) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(36.373, -21.));
    let end = doc.add_atom("C", Point::new(72.746, 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, end, 1, "plain");
    (doc, end)
}

fn neighbors(doc: &Document, id: u64) -> Vec<u64> {
    doc.bonds
        .iter()
        .filter_map(|bond| {
            if bond.a == id {
                Some(bond.b)
            } else if bond.b == id {
                Some(bond.a)
            } else {
                None
            }
        })
        .collect()
}

// Check the protecting group topology independently of the catalog's SMILES.
fn assert_group(doc: &Document, anchor: u64, label: &str) {
    let oxygen = label == "OTBDPS";
    let group = doc.abbreviation(anchor).unwrap();
    assert_eq!(group.label, label);
    assert_eq!(group.members.len(), if oxygen { 18 } else { 17 });
    assert_eq!(
        doc.atom(anchor).unwrap().element,
        if oxygen { "O" } else { "Si" }
    );
    assert!(doc.atoms.iter().all(|atom| atom.element != "*"));
    let silicon = doc
        .atoms
        .iter()
        .find(|atom| atom.element == "Si")
        .unwrap()
        .id;
    let silicon_neighbors = neighbors(doc, silicon);
    assert_eq!(silicon_neighbors.len(), 4);
    if oxygen {
        assert!(silicon_neighbors.contains(&anchor));
        assert_eq!(neighbors(doc, anchor).len(), 2);
    } else {
        assert_eq!(silicon, anchor);
    }
    let phenyls: Vec<_> = silicon_neighbors
        .iter()
        .filter(|id| {
            group.members.contains(id)
                && doc.atom(**id).unwrap().element == "C"
                && neighbors(doc, **id).len() == 3
        })
        .collect();
    assert_eq!(phenyls.len(), 2);
    for root in phenyls {
        let mut ring = vec![*root];
        let mut index = 0;
        while index < ring.len() {
            for id in neighbors(doc, ring[index]) {
                if id != silicon && !ring.contains(&id) {
                    ring.push(id);
                }
            }
            index += 1;
        }
        assert_eq!(ring.len(), 6);
        assert!(ring.iter().all(|id| doc.atom(*id).unwrap().element == "C"));
        let bonds: Vec<_> = doc
            .bonds
            .iter()
            .filter(|bond| ring.contains(&bond.a) && ring.contains(&bond.b))
            .collect();
        assert_eq!(bonds.len(), 6);
        assert!(ring.iter().all(|id| {
            bonds
                .iter()
                .filter(|bond| bond.a == *id || bond.b == *id)
                .count()
                == 2
        }));
        // Native drawings may use Kekule or aromatic bond orders.
        assert!(
            bonds.iter().all(|bond| bond.order == 4)
                || (bonds.iter().filter(|bond| bond.order == 1).count() == 3
                    && bonds.iter().filter(|bond| bond.order == 2).count() == 3)
        );
    }
    let tert = silicon_neighbors
        .iter()
        .find(|id| {
            group.members.contains(id)
                && doc.atom(**id).unwrap().element == "C"
                && neighbors(doc, **id).len() == 4
        })
        .unwrap();
    let tert_neighbors = neighbors(doc, *tert);
    assert_eq!(tert_neighbors.len(), 4);
    assert_eq!(
        tert_neighbors
            .iter()
            .filter(|id| {
                **id != silicon
                    && doc.atom(**id).unwrap().element == "C"
                    && neighbors(doc, **id).len() == 1
            })
            .count(),
        3,
        "tert-butyl has three terminal methyl carbons"
    );
    let boundary: Vec<_> = doc
        .bonds
        .iter()
        .filter(|bond| group.members.contains(&bond.a) != group.members.contains(&bond.b))
        .collect();
    assert_eq!(boundary.len(), 1);
    assert!(boundary[0].a == anchor || boundary[0].b == anchor);
    assert_eq!(boundary[0].order, 1);
}

#[test]
fn typed_and_preset_groups_attach_through_the_correct_element() -> Result<(), String> {
    let (source, end) = propane();
    for label in ["TBDPS", "OTBDPS"] {
        assert!(reshiki::abbreviations::PRESETS.contains(&label));
        assert!(atom_text::description(label, Mode::Auto).starts_with("A real chemical group."));
        for mode in [Mode::Auto, Mode::Group] {
            let doc = atom_text::apply(&source, end, label, mode)?;
            assert_group(&doc, end, label);
            assert_eq!(
                doc.atom(end).unwrap().position,
                source.atom(end).unwrap().position
            );
            assert_eq!(atom_text::apply(&doc, end, label, mode)?, doc);
        }
        let doc = abbreviations::replace(&source, &[end], label).map_err(|e| e.to_string())?;
        assert_group(&doc, end, label);
        let explicit_text = atom_text::apply(&source, end, label, Mode::Text)?;
        assert_eq!(explicit_text.atom(end).unwrap().element, "*");
    }
    Ok(())
}

#[test]
fn orientation_native_save_copy_and_history_preserve_the_full_group() -> Result<(), String> {
    let (source, end) = propane();
    for label in ["TBDPS", "OTBDPS"] {
        let mut doc = atom_text::apply(&source, end, label, Mode::Auto)?;
        assert_eq!(doc.abbreviation(end).unwrap().text(&doc), label);
        let ids = doc.all_ids();
        editing::transform(&mut doc, &ids, editing::Transform::FlipHorizontal);
        let reversed = if label == "OTBDPS" { "TBDPSO" } else { label };
        assert_eq!(doc.abbreviation(end).unwrap().text(&doc), reversed);
        assert!(scene::svg(&doc).contains(reversed));
        let saved = serde_json::to_string(&doc).map_err(|e| e.to_string())?;
        let reopened: Document = serde_json::from_str(&saved).map_err(|e| e.to_string())?;
        assert_eq!(reopened, doc);
        reopened.validate()?;
        let copied = editing::selection(&doc, &[end]);
        assert_eq!(
            copied.atoms.len(),
            doc.abbreviation(end).unwrap().members.len()
        );
        let mut pasted = Document::default();
        editing::append(&mut pasted, &copied, Point::new(100., 0.));
        pasted.validate()?;
        assert_eq!(pasted.atoms.len(), copied.atoms.len());
        assert_eq!(pasted.bonds.len(), copied.bonds.len());
        assert_eq!(pasted.abbreviations[0].label, label);
        let collapsed = doc.clone();
        let mut history = History::default();
        history.commit(source.clone(), &doc);
        assert!(history.undo(&mut doc));
        assert_eq!(doc, source);
        assert!(history.redo(&mut doc));
        assert_eq!(doc, collapsed);
        assert_eq!(doc.expand_abbreviations(&[end]), 1);
        assert_eq!(doc.atoms, collapsed.atoms);
        assert_eq!(doc.bonds, collapsed.bonds);
        assert!(doc.atoms.iter().all(|atom| doc.atom_visible(atom.id)));
        let expanded = doc.clone();
        history.commit(collapsed.clone(), &doc);
        assert!(history.undo(&mut doc));
        assert_eq!(doc, collapsed);
        assert!(history.redo(&mut doc));
        assert_eq!(doc, expanded);
    }
    Ok(())
}

#[tokio::test]
async fn properties_detection_and_editable_exchange_keep_independent_identity() -> Result<(), String>
{
    let engine = LocalEngine::default();
    let (source, end) = propane();
    // Independently evaluated with RDKit 2026.03.6 from
    // CC(C)(C)[Si](CC)(c1ccccc1)c1ccccc1 and its Si(OCC) counterpart.
    for (label, formula, key) in [
        ("TBDPS", "C18H24Si", "VRAHOEXKDMDTPA-UHFFFAOYSA-N"),
        ("OTBDPS", "C18H24OSi", "PJSWCLCQUIRIQP-UHFFFAOYSA-N"),
    ] {
        let mut collapsed = atom_text::apply(&source, end, label, Mode::Auto)?;
        // The reversed spelling must normalize back to OTBDPS during exchange.
        let ids = collapsed.all_ids();
        editing::transform(&mut collapsed, &ids, editing::Transform::FlipHorizontal);
        for operation in ["analyze", "clean"] {
            let checked = engine
                .request(Request::molecule(operation, collapsed.clone()))
                .await?;
            let properties = checked.analysis.ok_or("Missing analysis")?;
            assert_eq!(properties.formula, formula);
            assert_eq!(properties.inchikey, key);
            assert_eq!(properties.rings, 2);
            assert_group(&checked.document.ok_or("Missing drawing")?, end, label);
        }
        let mut expanded = collapsed.clone();
        expanded.expand_abbreviations(&[end]);
        let prepared = chemistry::prepare(&expanded).map_err(|e| e.to_string())?;
        let detected = abbreviations::find(&expanded, &prepared, &[], Some(label))
            .map_err(|e| e.to_string())?;
        assert_eq!(detected.atoms, expanded.atoms);
        assert_eq!(detected.bonds, expanded.bonds);
        assert_eq!(
            detected.abbreviations.len(),
            1,
            "Contract the chosen protecting group"
        );
        assert_group(&detected, end, label);
        for format in ["cdxml", "cdx", "mol", "smiles"] {
            let mut request = Request::molecule("export", collapsed.clone());
            request.format = Some(format.into());
            let output = engine
                .request(request)
                .await?
                .output
                .ok_or("Missing export")?;
            let back = engine.request(Request::import(format, &output)).await?;
            let properties = back.analysis.ok_or("Missing imported analysis")?;
            assert_eq!(properties.formula, formula, "{label}/{format}");
            assert_eq!(properties.inchikey, key, "{label}/{format}");
            let restored = back.document.ok_or("Missing imported drawing")?;
            if matches!(format, "cdxml" | "cdx") {
                let group = restored
                    .abbreviations
                    .first()
                    .ok_or("Lost editable group")?;
                assert_group(&restored, group.anchor, label);
                assert_eq!(
                    group.reverse_label,
                    if label == "OTBDPS" { "TBDPSO" } else { "" }
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn automatic_detection_prefers_complete_groups_on_a_ring_backbone() -> Result<(), String> {
    let engine = LocalEngine::default();
    for (label, smiles) in [
        ("TBDPS", "N1CCC([Si](c2ccccc2)(c2ccccc2)C(C)(C)C)CC1"),
        ("OTBDPS", "N1CCC(O[Si](c2ccccc2)(c2ccccc2)C(C)(C)C)CC1"),
    ] {
        let imported = engine.request(Request::import_smiles(smiles)).await?;
        let source = imported.document.ok_or("Missing ring backbone")?;
        let prepared = chemistry::prepare(&source).map_err(|e| e.to_string())?;
        let detected =
            abbreviations::find(&source, &prepared, &[], None).map_err(|e| e.to_string())?;
        assert_eq!(detected.atoms, source.atoms);
        assert_eq!(detected.bonds, source.bonds);
        assert_eq!(
            detected.abbreviations.len(),
            1,
            "Prefer the complete protecting group over Ph/tBu"
        );
        assert_group(&detected, detected.abbreviations[0].anchor, label);
    }
    Ok(())
}

#[tokio::test]
async fn selected_phenyl_contracts_without_an_unselected_protecting_group_suppressing_it()
-> Result<(), String> {
    let engine = LocalEngine::default();
    for (label, smiles) in [
        ("TBDPS", "N1CCC([Si](c2ccccc2)(c2ccccc2)C(C)(C)C)CC1"),
        ("OTBDPS", "N1CCC(O[Si](c2ccccc2)(c2ccccc2)C(C)(C)C)CC1"),
    ] {
        let source = engine
            .request(Request::import_smiles(smiles))
            .await?
            .document
            .ok_or("Missing protecting group")?;
        let before = source.clone();
        let prepared = chemistry::prepare(&source).map_err(|e| e.to_string())?;
        let phenyls =
            abbreviations::find(&source, &prepared, &[], Some("Ph")).map_err(|e| e.to_string())?;
        assert_eq!(phenyls.abbreviations.len(), 2);
        for phenyl in &phenyls.abbreviations {
            assert_eq!(phenyl.members.len(), 6);
            let mut detected = abbreviations::find(&source, &prepared, &phenyl.members, None)
                .map_err(|e| e.to_string())?;
            assert_eq!(detected.abbreviations, vec![phenyl.clone()]);
            assert_eq!(detected.atoms, source.atoms);
            assert_eq!(detected.bonds, source.bonds);
            let explicit = abbreviations::find(&source, &prepared, &phenyl.members, Some("Ph"))
                .map_err(|e| e.to_string())?;
            assert_eq!(detected, explicit);
            assert!(
                abbreviations::find(&source, &prepared, &phenyl.members, Some(label))
                    .map_err(|e| e.to_string())?
                    .abbreviations
                    .is_empty()
            );
            assert_eq!(detected.expand_abbreviations(&phenyl.members), 1);
            assert_eq!(detected.atoms, source.atoms);
            assert_eq!(detected.bonds, source.bonds);
            detected.validate()?;
            let partial: Vec<_> = phenyl.members.iter().skip(1).copied().collect();
            assert!(
                abbreviations::find(&source, &prepared, &partial, Some("Ph"))
                    .map_err(|e| e.to_string())?
                    .abbreviations
                    .is_empty()
            );
        }
        let full = abbreviations::find(&source, &prepared, &[], None).map_err(|e| e.to_string())?;
        assert_eq!(full.abbreviations.len(), 1);
        assert_eq!(full.abbreviations[0].label, label);
        let selected_full =
            abbreviations::find(&source, &prepared, &full.abbreviations[0].members, None)
                .map_err(|e| e.to_string())?;
        assert_eq!(selected_full, full);
        assert_eq!(source, before);
    }
    Ok(())
}
