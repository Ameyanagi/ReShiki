//! Exact-source #538 presentation contract. The frozen worker stays unchanged;
//! expected template coordinates come from the pinned, independent RDKit API.
use anyhow::{Context, ensure};
use reshiki::{document::Point, engine::Response};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

const INPUT: &str = "CC1(C)[CH]2CC[CH](CC(O)=O)[CH]1C2";
const REACTION_INPUT: &str = "O>CC1(C)[CH]2CC[CH](CC(O)=O)[CH]1C2>O";

#[cfg(test)]
use crate::reference_presentation as differences;

fn oracle() -> anyhow::Result<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let output = Command::new(python)
        .arg(root.join("reference/support/nci_2808_template_layout.py"))
        .env("PYTHONUTF8", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()?;
    ensure!(
        output.status.success(),
        "Independent NCI/2808 layout oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("Invalid NCI/2808 layout oracle response")
}

/// Qualify by format, corpus identity and the complete literal source. Every
/// unrelated case, including collision-free bridged molecules, is unchanged.
/// Only expected geometry is replaced; the exact agent case also has frozen
/// assembly's dependent arrow/product placement. The caller still compares the
/// entire response, including all chemical, stereo, analysis and warning fields.
pub fn adapt_expected(
    format: &str,
    case_name: &str,
    input: &str,
    expected: Response,
) -> anyhow::Result<Response> {
    match (format, case_name) {
        ("smiles", "NCI/2808") => {
            ensure!(input == INPUT, "NCI/2808 source input changed");
            adapt_with_oracle(expected, &oracle()?)
        }
        ("rsmi", "molecule/NCI/2808/1") => {
            ensure!(
                input == REACTION_INPUT,
                "NCI/2808 agent source input changed"
            );
            adapt_reaction_with_oracle(expected, &oracle()?)
        }
        _ => Ok(expected),
    }
}

fn check_oracle(source: &Value, engine_version: &str) -> anyhow::Result<()> {
    ensure!(
        source["case_name"] == "NCI/2808"
            && source["input"] == INPUT
            && source["rdkit_version"] == reshiki::chemistry::RDKIT_VERSION
            && engine_version == reshiki::chemistry::RDKIT_VERSION,
        "NCI/2808 source/version contract changed"
    );
    let eligible = &source["eligibility"];
    ensure!(
        eligible["atom_count"] == 13
            && eligible["bond_count"] == 14
            && eligible["clearance"] == 0.75
            && eligible["shared_ring_atom_counts"]
                .as_array()
                .is_some_and(|counts| counts.iter().any(|n| n.as_u64().is_some_and(|n| n >= 3)))
            && eligible["default_collision_pairs"] == json!([[0, 5], [2, 5]])
            && eligible["template_collision_pairs"] == json!([])
            && eligible["default_proper_crossings"] == json!([])
            && eligible["template_proper_crossings"] == json!([]),
        "NCI/2808 independent eligibility/improvement contract changed"
    );
    ensure!(
        source["display_only_identity"]["inchikey"] == "GEVYGCFPYCDBEW-UHFFFAOYSA-N"
            && source["display_only_identity"]["assigned_centers"] == json!([]),
        "NCI/2808 independent chemical/stereo identity changed"
    );
    Ok(())
}

fn adapt_with_oracle(mut expected: Response, source: &Value) -> anyhow::Result<Response> {
    check_oracle(source, &expected.engine_version)?;
    let original: Response = serde_json::from_value(source["original_response"].clone())?;
    // Match the complete default drawing before adapting its presentation.
    // Analysis remains the fresh frozen-worker value, including platform-local
    // descriptor roundoff handled by the caller's existing strict comparison.
    ensure!(
        serde_json::to_value(&expected.document)? == serde_json::to_value(&original.document)?,
        "NCI/2808 original default drawing changed"
    );
    let positions: Vec<Point> = serde_json::from_value(source["template_positions"].clone())?;
    let document = expected
        .document
        .as_mut()
        .context("Missing NCI/2808 drawing")?;
    ensure!(
        document.atoms.len() == 13
            && positions.len() == document.atoms.len()
            && positions.iter().all(|point| point.is_finite()),
        "Invalid independent NCI/2808 positions"
    );
    for (atom, position) in document.atoms.iter_mut().zip(positions) {
        atom.position = position;
    }
    Ok(expected)
}

fn without_reaction_geometry(mut value: Value) -> anyhow::Result<Value> {
    for atom in value["document"]["atoms"]
        .as_array_mut()
        .context("Missing reaction atoms")?
    {
        atom.as_object_mut()
            .context("Invalid reaction atom")?
            .remove("position");
    }
    for arrow in value["document"]["arrows"]
        .as_array_mut()
        .context("Missing reaction arrows")?
    {
        for end in ["start", "end"] {
            arrow[end]
                .as_object_mut()
                .context("Missing reaction arrow endpoint")?
                .remove("x");
        }
    }
    Ok(value)
}

fn adapt_reaction_with_oracle(mut expected: Response, source: &Value) -> anyhow::Result<Response> {
    check_oracle(source, &expected.engine_version)?;
    let agent = &source["reaction_agent"];
    ensure!(
        agent["case_name"] == "molecule/NCI/2808/1"
            && agent["input"] == REACTION_INPUT
            && agent["template_depiction_calls"] == 1,
        "NCI/2808 exact agent contract changed"
    );
    let original: Response = serde_json::from_value(agent["original_response"].clone())?;
    let template: Response = serde_json::from_value(agent["template_response"].clone())?;
    ensure!(
        serde_json::to_value(&expected.document)? == serde_json::to_value(&original.document)?,
        "NCI/2808 original default reaction drawing changed"
    );
    ensure!(
        without_reaction_geometry(serde_json::to_value(original)?)?
            == without_reaction_geometry(serde_json::to_value(&template)?)?,
        "NCI/2808 template assembly changed nongeometry fields"
    );
    let template = template.document.context("Missing template reaction")?;
    let document = expected
        .document
        .as_mut()
        .context("Missing reaction drawing")?;
    ensure!(
        document.atoms.len() == 15
            && template.atoms.len() == document.atoms.len()
            && document.arrows.len() == 1
            && template.arrows.len() == 1
            && template.atoms.iter().all(|atom| atom.position.is_finite()),
        "Invalid NCI/2808 template reaction geometry"
    );
    for (atom, new) in document.atoms.iter_mut().zip(template.atoms) {
        atom.position = new.position;
    }
    // The unchanged frozen assembler sizes the arrow/product row from the
    // agent's template bounds. The separate reviewed ink projection follows.
    document.arrows[0].start.x = template.arrows[0].start.x;
    document.arrows[0].end.x = template.arrows[0].end.x;
    Ok(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original(source: &Value) -> anyhow::Result<Response> {
        Ok(serde_json::from_value(source["original_response"].clone())?)
    }

    fn without_positions(mut value: Value) -> Value {
        for atom in value["document"]["atoms"].as_array_mut().unwrap() {
            atom.as_object_mut().unwrap().remove("position");
        }
        value
    }

    #[test]
    fn independent_template_changes_only_expected_positions() -> anyhow::Result<()> {
        let source = oracle()?;
        let before = original(&source)?;
        let after = adapt_with_oracle(before.clone(), &source)?;
        let (before, after) = (serde_json::to_value(before)?, serde_json::to_value(after)?);
        assert_ne!(before["document"]["atoms"], after["document"]["atoms"]);
        assert_eq!(without_positions(before), without_positions(after));
        Ok(())
    }

    #[test]
    fn full_comparison_rejects_wrong_coordinate_chemistry_and_stereo() -> anyhow::Result<()> {
        let source = oracle()?;
        let expected = serde_json::to_value(adapt_with_oracle(original(&source)?, &source)?)?;
        for (path, mutation) in [
            ("/document/atoms/0/position/x", json!(123.0)),
            ("/document/atoms/0/charge", json!(1)),
            (
                "/document/atoms/3/stereo",
                json!({"winding": "cw", "neighbors": [2, 5, 13]}),
            ),
        ] {
            let mut actual = expected.clone();
            *actual.pointer_mut(path).context("Missing mutation field")? = mutation;
            let failures = differences::response_differences(&actual, &expected);
            assert!(
                failures.iter().any(|failure| failure.contains(path)),
                "{failures:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn changed_source_or_default_coordinates_are_rejected() -> anyhow::Result<()> {
        let source = oracle()?;
        assert!(adapt_expected("smiles", "NCI/2808", "CCO", original(&source)?).is_err());
        let mut changed = original(&source)?;
        changed.document.as_mut().unwrap().atoms[0].position.x += 1.0;
        assert!(adapt_with_oracle(changed, &source).is_err());
        let mut ineligible = source.clone();
        ineligible["eligibility"]["template_collision_pairs"] = json!([[0, 5]]);
        assert!(adapt_with_oracle(original(&source)?, &ineligible).is_err());
        Ok(())
    }

    #[test]
    fn collision_free_bridged_control_keeps_complete_default_response() -> anyhow::Result<()> {
        let source = oracle()?;
        let control = &source["collision_free_control"];
        assert_eq!(control["default_collision_pairs"], json!([]));
        let before: Response = serde_json::from_value(control["original_response"].clone())?;
        let after = adapt_expected(
            "smiles",
            control["name"].as_str().unwrap(),
            control["input"].as_str().unwrap(),
            before.clone(),
        )?;
        assert_eq!(serde_json::to_value(before)?, serde_json::to_value(after)?);
        Ok(())
    }

    #[test]
    fn exact_agent_uses_independent_frozen_assembly_without_chemical_changes() -> anyhow::Result<()>
    {
        let source = oracle()?;
        let before: Response =
            serde_json::from_value(source["reaction_agent"]["original_response"].clone())?;
        let after = adapt_reaction_with_oracle(before.clone(), &source)?;
        assert_eq!(
            without_reaction_geometry(serde_json::to_value(before)?)?,
            without_reaction_geometry(serde_json::to_value(&after)?)?
        );
        let document = after.document.unwrap();
        let agent = &document.reactions[0].agents[0].atoms;
        assert_eq!(agent, &(2..15).collect::<Vec<_>>());
        assert_eq!(
            document
                .atoms
                .iter()
                .filter(|a| agent.contains(&a.id))
                .map(|a| a.position.y)
                .max_by(f32::total_cmp),
            Some(-100.)
        );
        assert!(document.atoms.iter().any(|a| a.position.y < -200.));
        Ok(())
    }

    #[test]
    fn reaction_alias_scope_bad_y_and_role_changes_are_rejected() -> anyhow::Result<()> {
        let source = oracle()?;
        let original: Response =
            serde_json::from_value(source["reaction_agent"]["original_response"].clone())?;
        assert!(
            adapt_expected("rsmi", "molecule/NCI/2808/1", "O>CCO>O", original.clone()).is_err()
        );
        let unrelated = adapt_expected(
            "rsmi",
            "molecule/NCI/2808/0",
            REACTION_INPUT,
            original.clone(),
        )?;
        assert_eq!(
            serde_json::to_value(&original)?,
            serde_json::to_value(unrelated)?
        );
        let mut bad_roles = source.clone();
        bad_roles["reaction_agent"]["template_response"]["document"]["reactions"][0]["agents"][0]
            ["coefficient"] = json!(2);
        assert!(adapt_reaction_with_oracle(original.clone(), &bad_roles).is_err());
        let expected = serde_json::to_value(adapt_reaction_with_oracle(original, &source)?)?;
        let mut actual = expected.clone();
        actual["document"]["atoms"][1]["position"]["y"] = json!(-100.0);
        assert!(
            differences::response_differences(&actual, &expected)
                .iter()
                .any(|failure| failure.contains("/document/atoms/1/position/y"))
        );
        Ok(())
    }
}
