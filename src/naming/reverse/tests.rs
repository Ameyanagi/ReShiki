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
        assert!(matches!(
            parse_response(&serde_json::to_vec(&bad).unwrap(), "CCO"),
            Err(Error::Worker(_))
        ));
    }
    let mut rejected = reply;
    rejected["result"] = json!({"Err":"Unsupported fused-ring parent in local organic rules"});
    assert_eq!(
        parse_response(&serde_json::to_vec(&rejected).unwrap(), "CCO").unwrap_err(),
        Error::Rejected("Unsupported fused-ring parent in local organic rules".into())
    );
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
        // Source-labelled aliases supplement the generated name only after
        // exact graph reconstruction, including the two absolute enantiomers.
        let expected_aliases: Option<&[&str]> = match smiles {
            "CCO" => Some(&["ethanol", "ethyl alcohol"]),
            "C[C@H](O)C(=O)O" => Some(&["L-lactic acid", "(+)-lactic acid"]),
            "C[C@@H](O)C(=O)O" => Some(&["D-lactic acid"]),
            "CC(O)C(=O)O" => Some(&["lactic acid"]),
            "C" => Some(&[]),
            _ => None,
        };
        if let Some(expected) = expected_aliases {
            assert_eq!(
                record
                    .synonyms
                    .iter()
                    .map(|alias| alias.name.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "Wrong retained identity for {smiles}"
            );
        }
    }
    Ok(())
}

#[test]
fn in_process_opsin_reconstructs_every_generated_rule_case() -> Result<(), String> {
    for &(smiles, expected_name) in rules::tests::CASES {
        let name = rules::generate(smiles)?;
        assert_eq!(name, expected_name, "{smiles}");
        let request = super::super::local::parse_request(&name).map_err(|e| e.to_string())?;
        let reply = worker::respond(&request)?;
        let parsed =
            super::super::local::parse_result(&name, &reply).map_err(|e| format!("{name}: {e}"))?;
        let identity = Identity {
            smiles: smiles.into(),
            warnings: vec![],
        };
        generated_record(
            &identity,
            super::super::canonical_smiles(smiles)?,
            name,
            &parsed,
        )
        .map_err(|e| format!("{smiles}: {e}"))?;
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
        let parsed = super::super::resolve_name(wrong_name, Cancel::default())
            .await
            .map_err(|error| error.to_string())?;
        assert!(
            super::super::verify_identity(expected, &parsed.smiles).is_err(),
            "Lost semantics for {expected}"
        );
    }
    Ok(())
}
