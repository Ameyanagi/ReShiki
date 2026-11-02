use super::*;
use serde_json::json;

fn atom(atomic_number: u8, isotope: u16, hydrogens: u8) -> AtomFacts {
    AtomFacts {
        atomic_number,
        isotope,
        charge: 0,
        hydrogens,
        radical_electrons: 0,
    }
}

#[test]
fn display_formula_distinguishes_nuclides_and_preserves_merged_formula() -> anyhow::Result<()> {
    for (atoms, formula, display) in [
        (
            vec![
                atom(6, 0, 2),
                atom(6, 0, 0),
                atom(8, 0, 1),
                atom(1, 2, 0),
                atom(1, 2, 0),
                atom(1, 2, 0),
            ],
            "C2H6O",
            "C₂H₃D₃O",
        ),
        (
            vec![
                atom(6, 14, 1),
                atom(6, 13, 0),
                atom(6, 0, 1),
                atom(6, 13, 0),
            ],
            "C4H2",
            "C¹³C₂¹⁴CH₂",
        ),
        (
            vec![atom(6, 13, 3), atom(1, 2, 0), atom(1, 3, 0)],
            "CH5",
            "¹³CH₃²H³H",
        ),
        (
            vec![atom(1, 3, 0), atom(1, 0, 0), atom(1, 2, 0)],
            "H3",
            "HDT",
        ),
        (
            vec![atom(1, 1, 0), atom(1, 2, 0), atom(1, 3, 0)],
            "H3",
            "¹H²H³H",
        ),
        (vec![atom(13, 0, 3), atom(1, 2, 0)], "H4Al", "H₃DAl"),
        (vec![atom(0, 99, 0), atom(1, 2, 0)], "H*", "D*"),
        (vec![atom(0, 99, 0)], "*", "*"),
        (vec![], "", ""),
    ] {
        let result = properties(&atoms).map_err(anyhow::Error::msg)?;
        assert_eq!(result.formula, formula);
        assert_eq!(serde_json::to_value(result)?["display_formula"], display);
    }
    Ok(())
}

#[test]
fn display_formula_formats_charges_and_charged_radicals() -> anyhow::Result<()> {
    for (atoms, formula, display) in [
        (
            vec![AtomFacts {
                charge: -3,
                ..atom(13, 0, 6)
            }],
            "H6Al-3",
            "H₆Al³⁻",
        ),
        (
            vec![AtomFacts {
                charge: 1,
                ..atom(7, 0, 4)
            }],
            "H4N+",
            "H₄N⁺",
        ),
        (
            vec![
                AtomFacts {
                    charge: 2,
                    radical_electrons: 1,
                    ..atom(6, 0, 3)
                },
                atom(6, 0, 3),
            ],
            "C2H6+2",
            "[C₂H₆]•²⁺",
        ),
        (
            vec![AtomFacts {
                charge: -1,
                radical_electrons: 1,
                ..atom(8, 0, 0)
            }],
            "O-",
            "[O]•⁻",
        ),
        (
            vec![AtomFacts {
                radical_electrons: 2,
                ..atom(6, 0, 2)
            }],
            "CH2",
            "CH₂",
        ),
    ] {
        let result = properties(&atoms).map_err(anyhow::Error::msg)?;
        assert_eq!(result.formula, formula);
        assert_eq!(serde_json::to_value(result)?["display_formula"], display);
    }
    Ok(())
}

#[test]
fn mass_percent_uses_isotope_masses_and_hill_element_order() -> anyhow::Result<()> {
    let ethanol =
        properties(&[atom(6, 0, 3), atom(6, 0, 2), atom(8, 0, 1)]).map_err(anyhow::Error::msg)?;
    let isotope =
        properties(&[atom(6, 13, 3), atom(8, 0, 1), atom(1, 2, 0)]).map_err(anyhow::Error::msg)?;
    let no_carbon = properties(&[atom(13, 0, 3)]).map_err(anyhow::Error::msg)?;
    for (result, expected) in [
        (ethanol, vec![("C", 24.022), ("H", 6.048), ("O", 15.999)]),
        (
            isotope,
            vec![("C", 13.00335484), ("H", 6.046101778), ("O", 15.999)],
        ),
        (no_carbon, vec![("Al", 26.982), ("H", 3.024)]),
    ] {
        let fractions: Vec<(String, f64)> =
            serde_json::from_value(serde_json::to_value(&result)?["mass_percent"].clone())?;
        assert_eq!(fractions.len(), expected.len());
        for ((element, fraction), (symbol, mass)) in fractions.iter().zip(expected) {
            assert_eq!(element, symbol);
            assert!((fraction - mass / result.mass * 100.).abs() < 1e-10);
        }
        assert!((fractions.iter().map(|(_, fraction)| fraction).sum::<f64>() - 100.).abs() < 1e-12);
    }
    for atoms in [vec![], vec![atom(0, 999, 0)]] {
        let result = properties(&atoms).map_err(anyhow::Error::msg)?;
        assert_eq!(serde_json::to_value(result)?["mass_percent"], json!([]));
    }
    Ok(())
}

#[test]
fn radioactive_nominal_averages_leave_exact_and_labelled_masses_unchanged() -> anyhow::Result<()> {
    for (number, average) in [
        (43, 97.),
        (105, 270.),
        (108, 270.),
        (113, 286.),
        (115, 289.),
        (117, 293.),
    ] {
        let result = properties(&[atom(number, 0, 0)]).map_err(anyhow::Error::msg)?;
        assert_eq!(result.mass, average);
        assert_eq!(result.exact_mass, ELEMENTS[usize::from(number)].exact);
        let explicit = properties(&[atom(number, 999, 0)]).map_err(anyhow::Error::msg)?;
        assert_eq!(explicit.mass, 999.);
        assert_eq!(explicit.exact_mass, 999.);
    }
    Ok(())
}

#[test]
fn old_properties_payloads_default_new_display_fields() -> anyhow::Result<()> {
    let old: Properties = serde_json::from_value(json!({
        "formula": "CH4", "mass": 16.043, "exact_mass": 16.031300128,
        "unpaired_electrons": 0,
    }))?;
    let value = serde_json::to_value(old)?;
    assert_eq!(value["display_formula"], "");
    assert_eq!(value["mass_percent"], json!([]));
    Ok(())
}

#[test]
fn worker_completion_is_atomic_and_requires_matching_data() -> Result<(), String> {
    let input = json!({"rdkit_version": RDKIT_VERSION, "graph": {"atoms": [{
            "atomic_number": 6, "isotope": 0, "charge": 0,
            "explicit_hydrogens": 0, "radical_electrons": 0,
            "no_implicit": false, "aromatic": false,
        }], "bonds": []}});
    let mut response = json!({"analysis": {
        "smiles": "C", "inchi": "InChI=1S/CH4/h1H4", "property_input": input,
    }});
    complete_analysis(&mut response)?;
    assert_eq!(response["analysis"]["formula"], "CH4");
    assert_eq!(response["analysis"]["mass"], 16.043);
    // Ring counts are calculated from the graph.
    assert_eq!(response["analysis"]["rings"], 0);
    assert_eq!(response["analysis"]["smiles"], "C");
    assert_eq!(
        response["analysis"]["inchikey"],
        "VNWKTOKETHGBQD-UHFFFAOYSA-N"
    );
    assert!(response["analysis"].get("property_input").is_none());
    for bad_input in [
        json!(null),
        json!({"rdkit_version": "different", "graph": {"atoms": [], "bonds": []}}),
        json!({"rdkit_version": RDKIT_VERSION, "graph": {"atoms": [{"atomic_number": 6}]}}),
        json!({"rdkit_version": RDKIT_VERSION, "graph": {"atoms": [{
                "atomic_number": 119, "isotope": 0, "charge": 0,
                "explicit_hydrogens": 0, "radical_electrons": 0,
                "no_implicit": false, "aromatic": false,
            }], "bonds": []}}),
        // Reject stale transport payloads and impossible graph valences.
        json!({"rdkit_version": RDKIT_VERSION, "atoms": []}),
        json!({"rdkit_version": RDKIT_VERSION, "graph": {"atoms": [{
                "atomic_number": 6, "explicit_hydrogens": 5, "isotope": 0, "charge": 0,
                "radical_electrons": 0, "no_implicit": false, "aromatic": false,
            }], "bonds": []}}),
    ] {
        let mut bad = json!({"analysis": {
            "smiles": "C", "inchi": "InChI=1S/CH4/h1H4", "property_input": bad_input,
        }});
        let original = bad.clone();
        assert!(complete_analysis(&mut bad).is_err());
        assert_eq!(bad, original);
    }
    assert!(complete_analysis(&mut response).is_err()); // Missing facts, no silent fallback.
    for mut no_analysis in [json!({}), json!({"analysis": null})] {
        complete_analysis(&mut no_analysis)?;
    }
    Ok(())
}

#[test]
fn worker_identifiers_are_checked_before_any_response_changes() -> anyhow::Result<()> {
    let original = json!({"analysis": {
        "smiles": "C", "inchi": "InChI=1S/CH4/h1H4",
        "property_input": {"rdkit_version": RDKIT_VERSION, "graph": {
            "atoms": [], "bonds": [],
        }},
    }});
    for malformed in [
        json!(null),
        json!(42),
        json!([]),
        json!("bad"),
        json!("InChI=1S/é"),
        json!("C".repeat(inchi::key::MAX_INPUT_BYTES + 1)),
    ] {
        let mut response = original.clone();
        response["analysis"]["inchi"] = malformed;
        let before = response.clone();
        assert!(complete_analysis(&mut response).is_err());
        assert_eq!(response, before);
    }
    let mut missing = original.clone();
    missing["analysis"]
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("No analysis"))?
        .remove("inchi");
    let before = missing.clone();
    assert!(complete_analysis(&mut missing).is_err());
    assert_eq!(missing, before);
    for unexpected in [json!(null), json!(""), json!("VNWKTOKETHGBQD-UHFFFAOYSA-N")] {
        let mut response = original.clone();
        response["analysis"]["inchikey"] = unexpected;
        let before = response.clone();
        assert_eq!(
            complete_analysis(&mut response).err().as_deref(),
            Some("Unexpected native InChIKey")
        );
        assert_eq!(response, before);
    }
    let mut empty = original;
    empty["analysis"]["inchi"] = "".into();
    complete_analysis(&mut empty).map_err(anyhow::Error::msg)?;
    assert_eq!(empty["analysis"]["inchikey"], "");
    Ok(())
}

#[test]
fn dense_ring_analysis_needs_no_reference_override() -> anyhow::Result<()> {
    let graph: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/ring-order-dependent.json"
    ))?;
    let input = json!({"rdkit_version": RDKIT_VERSION, "graph": graph});
    let mut response = json!({"analysis": {"inchi": "", "property_input": input}});
    complete_analysis(&mut response).map_err(anyhow::Error::msg)?;
    assert!(
        response["analysis"]["rings"]
            .as_u64()
            .is_some_and(|n| n > 1)
    );
    assert!(response["analysis"].get("property_input").is_none());
    let mut stale = input;
    stale["reference_rings"] = json!([[0, 8, 16]]);
    let mut response = json!({"analysis": {"property_input": stale}});
    let original = response.clone();
    assert!(complete_analysis(&mut response).is_err());
    assert_eq!(response, original);
    Ok(())
}
