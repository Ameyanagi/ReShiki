use super::*;
use serde_json::json;

fn properties(cid: u64, smiles: &str) -> serde_json::Value {
    json!({"PropertyTable":{"Properties":[{"CID":cid,"SMILES":smiles,"IUPACName":"ethanol","Title":"Ethanol"}]}})
}

#[test]
fn graph_identity_preserves_stereo_isotopes_charges_and_tautomers() -> Result<(), String> {
    verify_identity("CCO", "OCC")?;
    verify_identity("[CH3:1][CH2:2][OH:3]", "CCO")?;
    for (expected, different) in [
        ("C[C@H](O)C(=O)O", "C[C@@H](O)C(=O)O"),
        ("C[C@H](O)C(=O)O", "CC(O)C(=O)O"),
        ("C/C=C/C", "C/C=C\\C"),
        ("C/C=C/C", "CC=CC"),
        ("[13CH3]CO", "CCO"),
        ("CC(=O)[O-]", "CC(=O)O"),
        ("CC=O", "C=CO"),
    ] {
        assert!(
            verify_identity(expected, different).is_err(),
            "{expected} became {different}"
        );
    }
    for text in [
        "*C",
        "[CH3]",
        "C~C",
        "CCO>>CC=O",
        "CCO |$;;OH$|",
        "FC=C(F)F |w:1|",
        "invalid",
    ] {
        assert!(
            canonical_smiles(text).is_err(),
            "Accepted unsupported input {text}"
        );
    }
    Ok(())
}

#[test]
fn source_responses_preserve_ambiguity_and_require_full_record_identity() -> Result<(), String> {
    assert_eq!(
        parse_cids(json!({"IdentifierList":{"CID":[702,887]}}))?,
        vec![702, 887]
    );
    for value in [
        json!({}),
        json!({"IdentifierList":{"CID":[]}}),
        json!({"IdentifierList":{"CID":[702,702]}}),
        json!({"IdentifierList":{"CID":[0]}}),
        json!({"IdentifierList":{"CID":(1..=17).collect::<Vec<_>>()}}),
    ] {
        assert!(parse_cids(value).is_err());
    }
    let records = parse_properties(properties(702, "OCC"), &[702])?;
    assert_eq!(records[0].canonical_smiles, "CCO");
    assert_eq!(records[0].provenance, Provenance::PubChem(702));
    assert!(parse_properties(properties(702, "CCO"), &[887]).is_err());
    assert!(parse_properties(properties(702, "CCO"), &[702, 887]).is_err());
    assert!(parse_properties(json!({"PropertyTable":{"Properties":[{"CID":702,"ConnectivitySMILES":"CCO","IUPACName":"ethanol","Title":"Ethanol"}]}}), &[702]).is_err(), "Connectivity-only SMILES cannot establish stereo identity");
    Ok(())
}

#[test]
fn opsin_failure_and_warning_are_never_silent_success() -> Result<(), String> {
    assert!(
        parse_opsin(
            json!({"status":"FAILURE","message":"Unsupported name"}),
            "nonsense"
        )
        .is_err()
    );
    assert!(parse_opsin(json!({"smiles":"CCO"}), "ethanol").is_err());
    assert!(parse_opsin(json!({"status":"SUCCESS"}), "ethanol").is_err());
    let record = parse_opsin(
        json!({"status":"WARNING","smiles":"CC(O)C(=O)O","message":"Optical rotation cannot specify configuration","warnings":["STEREOCHEMISTRY_IGNORED"]}),
        "(+)-lactic acid",
    )?;
    assert!(
        record
            .warnings
            .iter()
            .any(|s| s.contains("Optical rotation"))
    );
    assert!(
        record
            .warnings
            .iter()
            .any(|s| s.contains("STEREOCHEMISTRY_IGNORED"))
    );
    assert_eq!(record.systematic_name, None);
    assert!(!record.canonical_smiles.contains('@'));
    Ok(())
}

#[test]
fn synonyms_remain_tied_to_the_source_cid() -> Result<(), String> {
    let value = json!({"InformationList":{"Information":[{"CID":702,"Synonym":["Ethanol","ethyl alcohol","64-17-5"]}]}});
    assert_eq!(
        parse_synonyms(value.clone(), 702)?,
        vec!["Ethanol", "ethyl alcohol", "64-17-5"]
    );
    assert!(parse_synonyms(value, 887).is_err());
    Ok(())
}

#[test]
fn selections_cannot_silently_truncate_a_molecule() -> Result<(), String> {
    let mut doc = Document::default();
    let a = doc.add_atom("C", crate::document::Point::new(0., 0.));
    let b = doc.add_atom("C", crate::document::Point::new(28., 0.));
    let o = doc.add_atom("O", crate::document::Point::new(42., 24.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, o, 1, "plain");
    assert!(selected_identity(&doc, &[a, b]).is_err());
    assert_eq!(selected_identity(&doc, &[a, b, o])?.smiles, "CCO");
    doc.contract(&[a, b], "Et", "")?;
    let anchor = doc.abbreviations[0].anchor;
    assert_eq!(
        selected_identity(&doc, &[anchor, o])?.smiles,
        "CCO",
        "A selected abbreviation retains its complete hidden graph"
    );
    assert_eq!(doc.atoms.len(), 3);
    let water = doc.add_atom("O", crate::document::Point::new(100., 0.));
    assert!(selected_identity(&doc, &[a, b, o, water]).is_err());
    doc.bonds[0].display = "wavy".into();
    assert!(selected_identity(&doc, &[a, b, o]).is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit live-service check; sends only public chemical fixtures"]
async fn live_official_services_resolve_names_and_exact_stereoisomers() -> Result<(), String> {
    let service = Service::new()?;
    let ethanol = service.resolve_name("ethanol", NameSource::Opsin).await?;
    assert_eq!(ethanol[0].canonical_smiles, "CCO");
    let common = service.resolve_name("aspirin", NameSource::PubChem).await?;
    assert!(
        common
            .iter()
            .any(|r| r.canonical_smiles == canonical_smiles("CC(=O)Oc1ccccc1C(=O)O").unwrap())
    );
    let specified = service
        .resolve_name("(R)-lactic acid", NameSource::Opsin)
        .await?;
    assert!(specified[0].canonical_smiles.contains('@'));
    let result = service
        .lookup_structure(Identity {
            smiles: specified[0].canonical_smiles.clone(),
            warnings: vec![],
        })
        .await?;
    assert!(
        result
            .systematic_name
            .as_deref()
            .is_some_and(|s| s.contains("(2R)"))
    );
    assert!(!result.synonyms.is_empty());
    verify_identity(&specified[0].smiles, &result.smiles)?;
    let invalid = service
        .resolve_name("reshiki-no-such-chemical-name-50", NameSource::Opsin)
        .await;
    assert!(invalid.is_err());
    Ok(())
}
