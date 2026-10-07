use super::*;
use serde_json::json;

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
