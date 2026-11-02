use super::*;
use crate::{
    engine::LocalEngine,
    fischer::{self, Compound},
};

#[test]
fn wedge_conversion_reorients_condensed_terminal_labels() -> anyhow::Result<()> {
    for compound in [Compound::LAlanine, Compound::DGlucose] {
        let source = fischer::document(compound, 42.).map_err(anyhow::Error::msg)?;
        let mut document = wedge_request(&Request::molecule("fischer_wedge", source))?
            .document
            .unwrap();
        for group in document.abbreviations.clone() {
            let reverse = match group.label.as_str() {
                "CH3" => "H3C",
                "COOH" => "HOOC",
                "CHO" => "OHC",
                "CH2OH" => "HOH2C",
                _ => unreachable!(),
            };
            let neighbor = document
                .bonds
                .iter()
                .find_map(|bond| {
                    let other = if bond.a == group.anchor {
                        bond.b
                    } else if bond.b == group.anchor {
                        bond.a
                    } else {
                        return None;
                    };
                    (!group.members.contains(&other)).then_some(other)
                })
                .unwrap();
            let anchor = document.atom(group.anchor).unwrap().position;
            for (x, expected) in [(42., reverse), (-42., group.label.as_str())] {
                document.atom_mut(neighbor).unwrap().position = anchor.offset(x, 0.);
                assert_eq!(group.text(&document), expected);
                let (lo, hi) = crate::scene::atom_label_bounds(
                    document.atom(group.anchor).unwrap(),
                    &document,
                )
                .unwrap();
                // The attachment carbon stays at the end facing its bond.
                if x > 0. {
                    assert!(anchor.x - lo.x > hi.x - anchor.x);
                } else {
                    assert!(hi.x - anchor.x > anchor.x - lo.x);
                }
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn every_fischer_template_converts_to_ordinary_bond_angles_and_keeps_key()
-> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    for template in fischer::templates().map_err(anyhow::Error::msg)? {
        let centers = fischer::marked_centers(&template.document);
        let before = engine
            .request(Request::molecule("analyze", template.document.clone()))
            .await
            .map_err(anyhow::Error::msg)?;
        let converted = engine
            .request(Request::molecule("fischer_wedge", template.document))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            converted.analysis.unwrap().inchikey,
            before.analysis.unwrap().inchikey,
            "{}",
            template.name
        );
        let document = converted.document.unwrap();
        assert!(
            document.atoms.iter().all(|a| a.element != "H"),
            "{}",
            template.name
        );
        assert!(
            document.bonds.iter().all(|b| !b.projection),
            "{}",
            template.name
        );
        for center in centers {
            let atom = document.atom(center).unwrap();
            assert_eq!(
                atom.stereo.as_ref().unwrap().neighbors.len(),
                3,
                "{}",
                template.name
            );
            let vectors: Vec<_> = document
                .bonds
                .iter()
                .filter_map(|bond| {
                    if bond.a == center {
                        Some(bond.b)
                    } else if bond.b == center {
                        Some(bond.a)
                    } else {
                        None
                    }
                })
                .map(|id| {
                    let p = document.atom(id).unwrap().position;
                    (
                        f64::from(p.x - atom.position.x),
                        f64::from(p.y - atom.position.y),
                    )
                })
                .collect();
            assert_eq!(vectors.len(), 3, "{} centre {center}", template.name);
            for (index, &(ax, ay)) in vectors.iter().enumerate() {
                for &(bx, by) in &vectors[index + 1..] {
                    let cosine = (ax * bx + ay * by) / (ax.hypot(ay) * bx.hypot(by));
                    assert!(
                        cosine >= 165_f64.to_radians().cos(),
                        "{} centre {center} retains an angle of {} degrees",
                        template.name,
                        cosine.clamp(-1., 1.).acos().to_degrees()
                    );
                }
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn folding_one_center_keeps_a_neighboring_isotopic_hydrogen_and_key() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    let mut source = fischer::document(Compound::DGlucose, 42.).map_err(anyhow::Error::msg)?;
    let hydrogen = source.atoms.iter().find(|a| a.element == "H").unwrap().id;
    source.atom_mut(hydrogen).unwrap().isotope = 2;
    let before = engine
        .request(Request::molecule("analyze", source.clone()))
        .await
        .map_err(anyhow::Error::msg)?;
    let converted = engine
        .request(Request::molecule("fischer_wedge", source))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        converted.analysis.unwrap().inchikey,
        before.analysis.unwrap().inchikey
    );
    let document = converted.document.unwrap();
    assert_eq!(
        document.atoms.iter().filter(|a| a.element == "H").count(),
        1
    );
    assert_eq!(document.atom(hydrogen).unwrap().isotope, 2);
    assert!(document.bonds.iter().all(|b| !b.projection));
    Ok(())
}

#[tokio::test]
async fn fischer_to_wedge_keeps_key_and_emits_chemical_wedges() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    for compound in [Compound::LAlanine, Compound::DGlucose] {
        let source = fischer::document(compound, 42.).map_err(anyhow::Error::msg)?;
        let before = engine
            .request(Request::molecule("analyze", source.clone()))
            .await
            .map_err(anyhow::Error::msg)?;
        let converted = engine
            .request(Request::molecule("fischer_wedge", source.clone()))
            .await
            .map_err(anyhow::Error::msg)?;
        let result = converted.document.unwrap();
        assert_eq!(
            converted.analysis.unwrap().inchikey,
            before.analysis.unwrap().inchikey
        );
        assert!(result.bonds.iter().all(|bond| !bond.projection));
        assert!(
            result
                .bonds
                .iter()
                .any(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
        );
        assert!(fischer::recognize(&result).crosses.is_empty());
        for format in ["cdxml", "cdx"] {
            let mut export = Request::molecule("export", source.clone());
            export.format = Some(format.into());
            let error = engine.request(export).await.unwrap_err();
            assert!(error.contains("Convert to wedge drawing"), "{error}");
            let mut export = Request::molecule("export", result.clone());
            export.format = Some(format.into());
            engine.request(export).await.map_err(anyhow::Error::msg)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn off_axis_fischer_still_requires_and_supports_wedge_conversion() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    let mut source = fischer::document(Compound::LAlanine, 42.).map_err(anyhow::Error::msg)?;
    let original = source.clone();
    let ids = source.all_ids();
    crate::editing::transform(&mut source, &ids, crate::editing::Transform::Rotate(37.));
    let _ = crate::transaction::reconcile(&mut source, original)
        .map_err(|error| anyhow::anyhow!(error.message().to_owned()))?;
    assert!(fischer::recognize(&source).crosses.is_empty());
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", source.clone());
        request.format = Some(format.into());
        let error = engine.request(request).await.unwrap_err();
        assert!(error.contains("Convert to wedge drawing"), "{error}");
    }
    assert!(crate::exchange::drawing::write_clipboard(&source).is_err());
    let converted = engine
        .request(Request::molecule("fischer_wedge", source))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        converted.analysis.unwrap().inchikey,
        "QNAYBMKLOCPYGJ-REOHCLBHSA-N"
    );
    let converted = converted.document.unwrap();
    assert!(converted.bonds.iter().all(|bond| !bond.projection));
    assert!(
        converted
            .bonds
            .iter()
            .any(|bond| { matches!(bond.display.as_str(), "wedge" | "hash") })
    );
    Ok(())
}

#[tokio::test]
async fn reverse_fischer_roundtrip_preserves_alanine_and_glucose_keys() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    for input in ["C[C@H](N)C(=O)O", "O=C[C@H](O)[C@@H](O)[C@H](O)[C@H](O)CO"] {
        let source = engine
            .request(Request::import_smiles(input))
            .await
            .map_err(anyhow::Error::msg)?;
        let key = source.analysis.unwrap().inchikey;
        let source = source.document.unwrap();
        let mut request = Request::molecule("fischer", source.clone());
        request.selected_ids = Some(vec![source.atoms[0].id]);
        let fischer = engine.request(request).await.map_err(anyhow::Error::msg)?;
        assert_eq!(fischer.analysis.unwrap().inchikey, key);
        let document = fischer.document.unwrap();
        let crosses = crate::fischer::recognize(&document).crosses;
        assert!(!crosses.is_empty());
        let top = document.atom(crosses[0].up).unwrap();
        assert!(
            document.bonds.iter().any(|bond| {
                bond.order == 2
                    && (bond.a == top.id || bond.b == top.id)
                    && document
                        .atom(if bond.a == top.id { bond.b } else { bond.a })
                        .unwrap()
                        .element
                        == "O"
            }),
            "the carbonyl end must be C1 at the top"
        );
        assert!(document.atoms.iter().any(|a| a.element == "H"));
        let wedge = engine
            .request(Request::molecule("fischer_wedge", document))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(wedge.analysis.unwrap().inchikey, key);
    }
    Ok(())
}

#[tokio::test]
async fn unsupported_reverse_is_clear_and_never_changes_input() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    // A cyclic molecule, a centre off the longest chain, and an off-chain
    // alkene whose E/Z a rigid subtree move could turn over.
    for smiles in [
        "C1C[C@H](O)CCC1",
        "C[C@H](O)CO[C@H](F)Cl",
        "OCCC/C(CC[C@H](O)C)=C(/F)Cl",
    ] {
        let source = engine
            .request(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .unwrap();
        let snapshot = source.clone();
        let error = to_fischer(&source, &[source.atoms[0].id]).unwrap_err();
        assert!(error.to_string().contains("acyclic"));
        assert_eq!(source, snapshot);
    }
    Ok(())
}

#[test]
fn native_clipboard_keeps_fischer_while_external_copy_requires_conversion() -> anyhow::Result<()> {
    let source = fischer::document(Compound::LAlanine, 42.).map_err(anyhow::Error::msg)?;
    let encoded = source.file_json().map_err(anyhow::Error::msg)?;
    let decoded = Document::from_native_file(&encoded).map_err(anyhow::Error::msg)?;
    assert_eq!(decoded, source.current());
    let error = crate::exchange::drawing::write_clipboard(&source).unwrap_err();
    assert!(error.to_string().contains("Convert to wedge drawing"));
    Ok(())
}

#[tokio::test]
async fn every_fischer_template_matches_reference_and_mol_roundtrip_key() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    let references = [
        ("O=C[C@H](O)CO", "MNQZXJOMYWMBOU-VKHMYHEASA-N"),
        ("O=C[C@@H](O)CO", "MNQZXJOMYWMBOU-GSVOUGTGSA-N"),
        (
            "O=C[C@H](O)[C@@H](O)[C@H](O)[C@H](O)CO",
            "GZCGUPFRVQAUEE-SLPGGIOYSA-N",
        ),
        (
            "O=C[C@H](O)[C@H](O)[C@H](O)CO",
            "PYMYPHUHKUWMLA-LMVFSUKVSA-N",
        ),
        ("C[C@H](N)C(=O)O", "QNAYBMKLOCPYGJ-REOHCLBHSA-N"),
        ("N[C@@H](CO)C(=O)O", "MTCFGRXMJLQNBG-REOHCLBHSA-N"),
        ("OC[C@@H](O)COP(=O)(O)O", "AWUCVROLDVIAJX-GSVOUGTGSA-N"),
    ];
    for (template, (reference, expected)) in fischer::templates()
        .map_err(anyhow::Error::msg)?
        .into_iter()
        .zip(references)
    {
        let reference = engine
            .request(Request::import_smiles(reference))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            reference.analysis.unwrap().inchikey,
            expected,
            "{} independent reference",
            template.name
        );
        let native = engine
            .request(Request::molecule("analyze", template.document.clone()))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            native.analysis.unwrap().inchikey,
            expected,
            "{}",
            template.name
        );
        let mut export = Request::molecule("export", template.document.clone());
        export.format = Some("mol".into());
        let mol = engine
            .request(export)
            .await
            .map_err(anyhow::Error::msg)?
            .output
            .unwrap();
        let imported = engine
            .request(Request::import("mol", &mol))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            imported.analysis.unwrap().inchikey,
            expected,
            "{} MOL",
            template.name
        );
        assert!(
            imported
                .document
                .unwrap()
                .bonds
                .iter()
                .all(|bond| !bond.projection)
        );
    }
    Ok(())
}

#[test]
fn reverse_requires_one_selected_component_and_preserves_other_geometry() -> anyhow::Result<()> {
    let source = fischer::document(Compound::LAlanine, 42.).map_err(anyhow::Error::msg)?;
    assert!(matches!(to_fischer(&source, &[]), Err(Error::SelectOne)));
    let mut source = source;
    let other = source.add_atom("C", Point::new(800., 100.));
    let snapshot = source.atom(other).unwrap().clone();
    let converted = to_fischer(&source, &[source.atoms[0].id])?;
    assert_eq!(converted.atom(other).unwrap(), &snapshot);
    assert!(matches!(
        to_fischer(&source, &[source.atoms[0].id, other]),
        Err(Error::SelectOne)
    ));
    Ok(())
}

#[tokio::test]
async fn reverse_moves_phosphate_subtree_rigidly_and_keeps_key() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    let source = engine
        .request(Request::import_smiles("OC[C@@H](O)COP(=O)(O)O"))
        .await
        .map_err(anyhow::Error::msg)?;
    let key = source.analysis.unwrap().inchikey;
    let source = source.document.unwrap();
    let phosphorus = source.atoms.iter().find(|a| a.element == "P").unwrap().id;
    let neighbors: Vec<_> = source
        .bonds
        .iter()
        .filter_map(|b| {
            if b.a == phosphorus {
                Some(b.b)
            } else if b.b == phosphorus {
                Some(b.a)
            } else {
                None
            }
        })
        .collect();
    let converted = to_fischer(&source, &[phosphorus])?;
    let shift = converted.atom(phosphorus).unwrap().position;
    let old = source.atom(phosphorus).unwrap().position;
    for id in neighbors {
        let before = source.atom(id).unwrap().position;
        let after = converted.atom(id).unwrap().position;
        assert!((after.x - shift.x - (before.x - old.x)).abs() < 1e-4);
        assert!((after.y - shift.y - (before.y - old.y)).abs() < 1e-4);
    }
    let converted = engine
        .request(Request::molecule("analyze", converted))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(converted.analysis.unwrap().inchikey, key);
    Ok(())
}

#[tokio::test]
async fn wedge_conversion_preserves_unselected_invalid_molecule_geometry() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    let mut source = fischer::document(Compound::LAlanine, 42.).map_err(anyhow::Error::msg)?;
    let selected = source.atoms[0].id;
    // The selected component is convertible even when another component cannot
    // be analyzed as a molecule.
    let other = source.add_atom("Xx", Point::new(800., 100.));
    let snapshot = source.atom(other).unwrap().clone();
    let mut request = Request::molecule("fischer_wedge", source);
    request.selected_ids = Some(vec![selected]);
    let converted = engine
        .request(request)
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .unwrap();
    assert_eq!(converted.atom(other).unwrap(), &snapshot);
    Ok(())
}

#[tokio::test]
async fn inverted_haworth_anomers_and_epimers_keep_editable_exchange_configuration()
-> anyhow::Result<()> {
    use crate::haworth::{Anomer, HaworthOptions, Sugar, sugar_document_with_options};
    let engine = LocalEngine::in_process();
    for ring_hydrogens in [false, true] {
        for (anomer, center_index, expected_sugar) in [
            (Anomer::Alpha, 0, Sugar::Glucose),
            (Anomer::Beta, 1, Sugar::Mannose),
        ] {
            let options = HaworthOptions { ring_hydrogens };
            let mut document = sugar_document_with_options(Sugar::Glucose, anomer, 42., options)
                .map_err(anyhow::Error::msg)?;
            let center = document.atoms[center_index].id;
            crate::stereochemistry::invert_configuration(&mut document, center)
                .map_err(anyhow::Error::msg)?;
            let expected = sugar_document_with_options(expected_sugar, Anomer::Beta, 42., options)
                .map_err(anyhow::Error::msg)?;
            let key = engine
                .request(Request::molecule("analyze", expected))
                .await
                .map_err(anyhow::Error::msg)?
                .analysis
                .unwrap()
                .inchikey;
            for format in ["cdxml", "cdx"] {
                let mut request = Request::molecule("export", document.clone());
                request.format = Some(format.into());
                let output = engine
                    .request(request)
                    .await
                    .map_err(anyhow::Error::msg)?
                    .output
                    .unwrap();
                let imported = engine
                    .request(Request::import(format, &output))
                    .await
                    .map_err(anyhow::Error::msg)?;
                assert_eq!(imported.analysis.unwrap().inchikey, key, "{format}");
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn cdxml_dl_stereo_tags_keep_measured_text_and_user_number_style() -> anyhow::Result<()> {
    let engine = LocalEngine::in_process();
    let mut document = engine
        .request(Request::import_smiles("C[C@H](N)C(=O)O"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .unwrap();
    document.atom_labels.stereo = true;
    document.atom_labels.dl_descriptors = true;
    let center = document
        .atoms
        .iter()
        .find(|atom| atom.stereo.is_some())
        .unwrap()
        .id;
    document.atom_mut(center).unwrap().display.number = Some(crate::atom_labels::Number {
        text: "L (S)".into(),
        style: crate::typography::TextStyle {
            italic: false,
            ..Default::default()
        },
        offset: None,
    });
    let mut request = Request::molecule("export", document);
    request.format = Some("cdxml".into());
    let xml = engine
        .request(request)
        .await
        .map_err(anyhow::Error::msg)?
        .output
        .unwrap();
    let parsed = roxmltree::Document::parse(&xml)?;
    let mut found = false;
    for tag in parsed.descendants().filter(|n| n.has_tag_name("objecttag")) {
        let text: String = tag
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect();
        match tag.attribute("Name") {
            Some("stereo") if text == "L (S)" => found = true,
            Some("number") => assert!(tag.descendants().filter(|n| n.has_tag_name("s")).all(|n| {
                n.attribute("face")
                    .is_none_or(|face| face.parse::<u32>().unwrap() & 2 == 0)
            })),
            _ => (),
        }
    }
    assert!(found, "{xml}");
    Ok(())
}
