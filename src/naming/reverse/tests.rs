use super::*;
use serde_json::json;

#[test]
fn rule_protocol_checks_profile_input_and_bounded_name() {
    let reply = json!({"operation":"generate","protocol":worker::PROTOCOL,"profile":openclatura::PROFILE,"input":"CCO","result":{"Ok":{"text":"ethanol","status":{"status":"preferred"}}}});
    let generated = parse_response(&serde_json::to_vec(&reply).unwrap(), "CCO").unwrap();
    assert_eq!(generated.text, "ethanol");
    assert_eq!(generated.status, NameStatus::Preferred);
    for (key, value) in [
        ("protocol", json!(worker::PROTOCOL - 1)),
        ("profile", json!("other")),
        ("input", json!("CCC")),
        (
            "result",
            json!({"Ok":{"text":"","status":{"status":"preferred"}}}),
        ),
        (
            "result",
            json!({"Ok":{"text":"x".repeat(2049),"status":{"status":"preferred"}}}),
        ),
        ("result", json!({"Ok":{"text":"ethanol"}})),
        (
            "result",
            json!({"Ok":{"text":"ethanol","status":{"status":"unknown"}}}),
        ),
        (
            "result",
            json!({"Ok":{"text":"ethanol","status":{"status":"not_preferred"}}}),
        ),
        (
            "result",
            json!({"Ok":{"text":"ethanol","status":{"status":"preferred"},"unexpected":true}}),
        ),
        ("result", json!({"Ok":"ethanol"})),
    ] {
        let mut bad = reply.clone();
        bad[key] = value;
        assert!(matches!(
            parse_response(&serde_json::to_vec(&bad).unwrap(), "CCO"),
            Err(Error::Worker(_))
        ));
    }
    let mut rejected = reply;
    rejected["result"] = json!({"Err":"openclatura: isotopic naming not ported"});
    assert_eq!(
        parse_response(&serde_json::to_vec(&rejected).unwrap(), "CCO").unwrap_err(),
        Error::Rejected("openclatura: isotopic naming not ported".into())
    );
}

#[test]
fn worker_names_with_the_port_or_declines() {
    let generate = |smiles| {
        let reply = worker::respond(&generate_request(smiles).unwrap()).unwrap();
        parse_response(&reply, smiles)
    };
    let preferred = generate("CCO").unwrap();
    assert_eq!(preferred.text, "ethanol");
    assert_eq!(preferred.status, NameStatus::Preferred);
    let general = generate("CC(=NN)C").unwrap();
    assert_eq!(general.text, "propan-2-one hydrazone");
    assert_eq!(
        general.status,
        NameStatus::NotPreferred(super::super::NameReason::FunctionalClass)
    );
    let Err(Error::Rejected(reason)) = generate("[13CH3]CO") else {
        panic!("An isotope-labelled molecule received a name");
    };
    assert!(reason.starts_with("This structure can't be named locally yet: "));
}

#[tokio::test]
async fn native_worker_then_local_opsin_verifies_supported_names() -> Result<(), String> {
    // The removed rules namer's cases, which the harness checks in-process.
    let cases = include_str!("../../../tests/fixtures/s2n/rules_cases.tsv");
    for (smiles, expected_name) in cases.lines().skip(1).filter_map(|line| {
        let mut fields = line.split('\t');
        let smiles = fields.next()?;
        let name = fields.nth(1).filter(|name| !name.is_empty())?;
        Some((smiles, name))
    }) {
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
        assert_eq!(record.provenance, Provenance::Openclatura);
        assert_eq!(record.systematic_name.as_deref(), Some(expected_name));
        assert!(record.name_status.is_some());
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
