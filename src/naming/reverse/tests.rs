use super::*;
use crate::naming::NameStatus;
use serde_json::json;

#[test]
fn generated_record_keeps_reconstruction_smiles_order_for_locants() {
    let name = "ethanol";
    let reply = worker::respond(&super::super::local::parse_request(name).unwrap()).unwrap();
    let reconstructed = super::super::local::parse_result(name, &reply).unwrap();
    let expected = "CCO".to_owned();
    let identity = Identity {
        smiles: expected.clone(),
        naming_smiles: None,
        write_unknown_stereo: false,
        warnings: vec![],
    };
    let candidate = openclatura::Name {
        text: name.into(),
        status: NameStatus::Preferred,
        alternatives: vec![],
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    };
    let generated = generated_record(&identity, expected, candidate, &reconstructed).unwrap();
    assert!(generated.locants.is_some());
    assert_eq!(generated.locants, reconstructed.locants);
    assert_eq!(generated.smiles, "CCO");
}

#[test]
fn rule_protocol_checks_profile_input_and_bounded_name() {
    let reply = json!({"operation":"generate","protocol":worker::PROTOCOL,"profile":openclatura::PROFILE,"input":"CCO","result":{"Ok":{"text":"ethanol","status":{"status":"preferred"},"alternatives":[],"omitted_stereo":[],"omitted_units":0}}});
    let generated = parse_response(&serde_json::to_vec(&reply).unwrap(), "CCO").unwrap();
    assert_eq!(generated.text, "ethanol");
    assert_eq!(generated.status, NameStatus::Preferred);
    assert!(generated.alternatives.is_empty());
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
        (
            "result",
            json!({"Ok":{"text":"ethanol","status":{"status":"preferred"},
                "alternatives":[{"name":"","status":{"status":"preferred"}}],"omitted_stereo":[],"omitted_units":0}}),
        ),
        ("result", json!({"Ok":"ethanol"})),
        (
            "result",
            json!({"Ok":{"text":"ethanol","status":{"status":"preferred"},
                "alternatives":[{"name":"x".repeat(2049),"status":{"status":"preferred"}}],"omitted_stereo":[],"omitted_units":0}}),
        ),
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

#[tokio::test]
async fn phane_names_are_verified_alternatives_or_the_only_name() -> Result<(), String> {
    use crate::naming::{AlternativeName, NameReason};
    for smiles in ["C1Cc2ccc(cc2)CCc2ccc1cc2", "C1Cc2cccc(c2)CCc2cccc1c2"] {
        let record = generate_name(
            &Identity {
                smiles: smiles.into(),
                naming_smiles: None,
                write_unknown_stereo: false,
                warnings: vec![],
            },
            Cancel::default(),
        )
        .await
        .map_err(|e| e.to_string())?;
        if smiles == "C1Cc2cccc(c2)CCc2cccc1c2" {
            assert_eq!(
                record.name_status,
                Some(NameStatus::NotPreferred(NameReason::Phane))
            );
            assert_eq!(
                record.alternatives,
                [AlternativeName {
                    name: "1,4(1,3)-dibenzenacyclohexaphane".into(),
                    status: NameStatus::Unverified(NameReason::Phane),
                }]
            );
        } else {
            assert_eq!(
                record.name_status,
                Some(NameStatus::Unverified(NameReason::Phane))
            );
            assert_eq!(record.title, "1,4(1,4)-dibenzenacyclohexaphane");
            assert!(record.alternatives.is_empty());
        }
    }
    Ok(())
}

#[test]
fn neutral_parent_verification_cannot_be_substituted_for_another_identity() {
    let mut reply = json!({"operation":"generate","protocol":worker::PROTOCOL,"profile":openclatura::PROFILE,
        "input":"[CH2+][CH2]","result":{"Ok":{"text":"ethene radical cation",
        "status":{"status":"not_preferred","reason":"radical_ion"},
        "alternatives":[],"omitted_stereo":[],"omitted_units":0,
        "verification":{"name":"ethene","smiles":"C=C"}}}});
    assert!(parse_response(&serde_json::to_vec(&reply).unwrap(), "[CH2+][CH2]").is_ok());
    for (field, wrong) in [("smiles", "CC"), ("name", "ethane")] {
        let mut bad = reply.clone();
        bad["result"]["Ok"]["verification"][field] = json!(wrong);
        assert!(matches!(
            parse_response(&serde_json::to_vec(&bad).unwrap(), "[CH2+][CH2]"),
            Err(Error::Worker(_))
        ));
    }
    reply["input"] = json!("CC");
    assert!(matches!(
        parse_response(&serde_json::to_vec(&reply).unwrap(), "CC"),
        Err(Error::Worker(_))
    ));
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
    let isotope = generate("[13CH3]CO").unwrap();
    assert!(isotope.text.contains("13C"));
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
                naming_smiles: None,
                write_unknown_stereo: false,
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

#[tokio::test]
async fn native_species_generation_verifies_the_complete_identity_and_status() {
    use crate::naming::NameReason;
    for (smiles, name, status) in [
        ("[Na+]", "sodium(1+)", NameStatus::Preferred),
        ("[Ca+2]", "calcium(2+)", NameStatus::Preferred),
        ("[Fe+2]", "iron(2+)", NameStatus::Preferred),
        ("CC(=O)[O-].[Na+]", "sodium acetate", NameStatus::Preferred),
        (
            "C[NH3+].[Cl-]",
            "methanaminium chloride",
            NameStatus::Preferred,
        ),
        (
            "[Ca+2].[Cl-].[Cl-]",
            "calcium dichloride",
            NameStatus::Preferred,
        ),
        (
            "[Na+].[Na+].O=S(=O)([O-])[O-]",
            "disodium sulfate",
            NameStatus::Preferred,
        ),
        ("[CH3]", "methyl", NameStatus::Preferred),
        ("C[CH]C", "propan-2-yl", NameStatus::Preferred),
        ("[CH2]", "methylidene", NameStatus::Preferred),
        ("[CH2][CH2]", "ethane-1,2-diyl", NameStatus::Preferred),
        ("[OH]", "hydroxyl", NameStatus::Preferred),
        ("C[O]", "methoxyl", NameStatus::Preferred),
        ("C[NH]", "methanaminyl", NameStatus::Preferred),
        (
            "[CH+]1[CH]C=CC=C1",
            "benzene radical cation",
            NameStatus::NotPreferred(NameReason::RadicalIon),
        ),
        (
            "[CH-]1[CH]C=CC2=CC=CC=C12",
            "naphthalene radical anion",
            NameStatus::NotPreferred(NameReason::RadicalIon),
        ),
        (
            "[CH2+][CH2]",
            "ethene radical cation",
            NameStatus::NotPreferred(NameReason::RadicalIon),
        ),
        (
            "C[B-](C)C",
            "trimethylborane radical anion",
            NameStatus::NotPreferred(NameReason::RadicalIon),
        ),
    ] {
        let record = generate_name(
            &Identity {
                smiles: smiles.into(),
                naming_smiles: None,
                write_unknown_stereo: false,
                warnings: vec![],
            },
            Cancel::default(),
        )
        .await
        .unwrap_or_else(|e| panic!("{smiles}: {e}"));
        assert_eq!(record.title, name, "{smiles}");
        assert_eq!(record.name_status, Some(status), "{smiles}");
        assert_eq!(
            record.canonical_smiles,
            crate::naming::canonical_smiles(smiles).unwrap()
        );
    }
    // OPSIN treats the transition-metal salt as a bonded coordination compound,
    // so its readable binary name cannot identify these disconnected ions.
    assert!(
        generate_name(
            &Identity {
                smiles: "[Fe+2].[Cl-].[Cl-]".into(),
                naming_smiles: None,
                write_unknown_stereo: false,
                warnings: vec![],
            },
            Cancel::default()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn native_isotope_styles_preserve_graph_and_preference_status() {
    use crate::naming::NameReason;
    for (smiles, style, expected) in [
        ("[13CH4]", IsotopeStyle::Iupac, "(13C)methane"),
        ("[13CH3]CCl", IsotopeStyle::Iupac, "1-chloro(2-13C)ethane"),
        (
            "ClC([2H])([2H])Cl",
            IsotopeStyle::Iupac,
            "dichloro(2H2)methane",
        ),
        (
            "[13CH3][C@@H](F)Cl",
            IsotopeStyle::Iupac,
            "(1S)-1-chloro-1-fluoro(2-13C)ethane",
        ),
        ("[2H]C([2H])([2H])O", IsotopeStyle::Iupac, "(2H3)methanol"),
        ("CO[2H]", IsotopeStyle::Iupac, "(O-2H1)methanol"),
        (
            "[2H]C([2H])([2H])O[2H]",
            IsotopeStyle::Boughton,
            "methanol-d4",
        ),
        (
            "[2H]C([2H])(C)O",
            IsotopeStyle::Boughton,
            "ethan-1-ol-1,1-d2",
        ),
    ] {
        let record = generate_name_with_options(
            &Identity {
                smiles: smiles.into(),
                naming_smiles: None,
                write_unknown_stereo: false,
                warnings: vec![],
            },
            style,
            Cancel::default(),
        )
        .await
        .unwrap_or_else(|e| panic!("{smiles}: {e}"));
        assert_eq!(record.title, expected);
        assert_eq!(
            record.name_status,
            Some(match style {
                IsotopeStyle::Iupac => NameStatus::Unverified(NameReason::IsotopePlacement),
                IsotopeStyle::Boughton => NameStatus::NotPreferred(NameReason::BoughtonIsotope),
            })
        );
        crate::naming::verify_identity(smiles, &record.canonical_smiles).unwrap();
    }
}

#[tokio::test]
async fn alternatives_require_independent_reconstruction() {
    use crate::naming::{AlternativeName, NameReason};
    let identity = Identity {
        smiles: "CCO".into(),
        naming_smiles: None,
        write_unknown_stereo: false,
        warnings: vec![],
    };
    let candidate = openclatura::Name {
        text: "ethanol".into(),
        status: NameStatus::Preferred,
        alternatives: [
            "ethyl alcohol",
            "methanol",
            "(2R)-propan-2-ol",
            "not a chemical name",
        ]
        .into_iter()
        .map(|name| AlternativeName {
            name: name.into(),
            status: NameStatus::NotPreferred(NameReason::FunctionalClass),
        })
        .collect(),
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    };
    let record = reconstruct(
        &identity,
        super::super::canonical_smiles("CCO").unwrap(),
        candidate,
        &Cancel::default(),
    )
    .await
    .unwrap();
    assert_eq!(record.alternatives.len(), 1);
    assert_eq!(record.alternatives[0].name, "ethyl alcohol");
    let request = super::super::local::parse_request(&record.alternatives[0].name).unwrap();
    let reply = worker::respond(&request).unwrap();
    let parsed = super::super::local::parse_result(&record.alternatives[0].name, &reply).unwrap();
    assert_eq!(parsed.canonical_smiles, record.canonical_smiles);
}

#[tokio::test]
async fn only_endo_exo_alternatives_accept_the_complete_mirror_image() {
    use crate::naming::{AlternativeName, NameReason};
    let smiles = "O[C@H]1C[C@@H]2CC[C@H]1C2";
    let primary = "(1S,2S,4R)-bicyclo[2.2.1]heptan-2-ol";
    let mirror = "(1R,2R,4S)-bicyclo[2.2.1]heptan-2-ol";
    let diastereomer = "(1S,2R,4R)-bicyclo[2.2.1]heptan-2-ol";
    let relative = NameStatus::NotPreferred(NameReason::EndoExoStereo);
    let identity = Identity {
        smiles: smiles.into(),
        naming_smiles: None,
        write_unknown_stereo: false,
        warnings: vec![],
    };
    let candidate = openclatura::Name {
        text: primary.into(),
        status: NameStatus::Preferred,
        alternatives: [
            (mirror, relative),
            (
                mirror,
                NameStatus::NotPreferred(NameReason::RelativeRingStereo),
            ),
            (mirror, NameStatus::Unverified(NameReason::EndoExoStereo)),
            (mirror, NameStatus::Preferred),
            (diastereomer, relative),
            ("bicyclo[2.2.1]heptan-2-ol", relative),
            ("ethanol", relative),
        ]
        .into_iter()
        .map(|(name, status)| AlternativeName {
            name: name.into(),
            status,
        })
        .collect(),
        omitted_stereo: vec![],
        omitted_units: 0,
        verification: None,
    };
    let record = reconstruct(
        &identity,
        super::super::canonical_smiles(smiles).unwrap(),
        candidate,
        &Cancel::default(),
    )
    .await
    .unwrap();
    assert_eq!(record.title, primary);
    assert_eq!(record.name_status, Some(NameStatus::Preferred));
    assert_eq!(
        record.alternatives,
        [AlternativeName {
            name: mirror.into(),
            status: relative,
        }]
    );
}

#[tokio::test]
async fn unknown_configuration_preference_selects_the_verified_primary() {
    use crate::naming::NameReason;
    let input = "C[CH:900001](O)[C@H](Cl)C";
    for explicit in [false, true] {
        let reply = worker::respond(&generate_request(input).unwrap()).unwrap();
        let candidate = parse_response(&reply, input).unwrap();
        let expected = super::super::canonical_smiles(input).unwrap();
        let identity = Identity {
            smiles: expected.clone(),
            naming_smiles: Some(input.into()),
            write_unknown_stereo: explicit,
            warnings: vec![],
        };
        let record = reconstruct(&identity, expected, candidate, &Cancel::default())
            .await
            .unwrap();
        if explicit {
            assert_eq!(record.title, "(2Ξ,3R)-3-chlorobutan-2-ol");
            assert_eq!(record.name_status, Some(NameStatus::Preferred));
            assert!(record.omitted_stereo.is_empty());
            assert_eq!(record.omitted_stereo_note(), None);
            assert_eq!(record.alternatives[0].name, "(3R)-3-chlorobutan-2-ol");
        } else {
            assert_eq!(record.title, "(3R)-3-chlorobutan-2-ol");
            assert_eq!(
                record.name_status,
                Some(NameStatus::NotPreferred(NameReason::UnknownConfiguration))
            );
            assert_eq!(record.omitted_stereo, ["2"]);
            assert_eq!(
                record.omitted_stereo_note().as_deref(),
                Some("Configuration at position 2 not specified in name")
            );
            assert_eq!(record.alternatives[0].name, "(2Ξ,3R)-3-chlorobutan-2-ol");
            let mut unlocated = record.clone();
            unlocated.omitted_stereo.clear();
            unlocated.omitted_units = 2;
            assert_eq!(
                unlocated.omitted_stereo_note().as_deref(),
                Some("Configuration of 2 units drawn as unknown not specified in name")
            );
        }
    }
}
