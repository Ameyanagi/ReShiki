use super::*;
use serde_json::json;

#[test]
fn rule_protocol_checks_profile_input_and_bounded_name() {
    let reply = json!({"operation":"generate","protocol":worker::PROTOCOL,"profile":rules::PROFILE,"input":"CCO","result":{"Ok":"ethan-1-ol"}});
    assert_eq!(
        parse_response(&serde_json::to_vec(&reply).unwrap(), "CCO").unwrap(),
        "ethan-1-ol"
    );
    for (key, value) in [
        ("protocol", json!(3)),
        ("profile", json!("other")),
        ("input", json!("CCC")),
        ("result", json!({"Ok":""})),
        ("result", json!({"Ok":"x".repeat(2049)})),
    ] {
        let mut bad = reply.clone();
        bad[key] = value;
        assert!(parse_response(&serde_json::to_vec(&bad).unwrap(), "CCO").is_err());
    }
}

#[tokio::test]
async fn native_worker_then_local_opsin_verifies_supported_names() -> Result<(), String> {
    for &(smiles, expected_name) in rules::tests::CASES {
        let expected = super::super::canonical_smiles(smiles)?;
        let record = generate_name(
            &Identity {
                smiles: smiles.into(),
                warnings: vec![],
            },
            Cancel::default(),
        )
        .await
        .map_err(|e| format!("{smiles} -> {expected_name}: {e}"))?;
        assert_eq!(record.canonical_smiles, expected);
        assert_eq!(record.provenance, Provenance::LocalRules);
        assert_eq!(record.systematic_name.as_deref(), Some(expected_name));
    }
    Ok(())
}

#[tokio::test]
async fn independent_decoder_canary_catches_connectivity_stereo_isotope_charge_and_tautomer_loss()
-> Result<(), String> {
    for (expected, wrong_name) in [
        ("CCO", "methanol"),
        ("C[C@@H](O)C(=O)O", "(2S)-2-hydroxypropanoic acid"),
        ("C/C=C/C", "(2Z)-but-2-ene"),
        ("[13CH3]CO", "ethanol"),
        ("CC(=O)[O-]", "ethanoic acid"),
        ("C=CO", "ethanal"),
    ] {
        let parsed = super::super::resolve_name(wrong_name, Cancel::default()).await?;
        assert!(
            super::super::verify_identity(expected, &parsed.smiles).is_err(),
            "Lost semantics for {expected}"
        );
    }
    Ok(())
}
