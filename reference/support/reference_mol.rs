//! Deliberate MOL import presentation changes, shared by the live response guards.
use anyhow::Context;
use reshiki::engine::Response;
use serde_json::Value;

/// The metadata is computed by independent RDKit/source-record references.
/// Guard the complete frozen input before replacing only the documented fields.
pub fn adapt_response(
    mut expected: Value,
    display: Option<&Value>,
    generic: Option<&Value>,
) -> anyhow::Result<Value> {
    if let Some(display) = display {
        anyhow::ensure!(
            expected["document"] == display["legacy_document"]
                && expected["analysis"] == display["legacy_analysis"],
            "MOL hydrogen-folding legacy response changed"
        );
        let mut drawing = display["document"].clone();
        let mut legacy = expected["document"].clone();
        for key in ["atoms", "bonds"] {
            drawing
                .as_object_mut()
                .context("Missing folded drawing")?
                .remove(key);
            legacy
                .as_object_mut()
                .context("Missing legacy drawing")?
                .remove(key);
        }
        anyhow::ensure!(
            drawing == legacy,
            "Hydrogen folding changed unrelated drawing fields"
        );
        anyhow::ensure!(
            display["removed_atom_ids"]
                .as_array()
                .is_some_and(|ids| !ids.is_empty()),
            "Empty hydrogen-folding contract"
        );
        expected["document"] = display["document"].clone();
        expected["analysis"]["smiles"] = display["analysis_smiles"].clone();
    }
    if let Some(generic) = generic {
        anyhow::ensure!(
            expected["document"] == generic["legacy_document"],
            "Generic MOL legacy drawing changed"
        );
        let mut drawing = generic["document"].clone();
        let mut legacy = expected["document"].clone();
        for key in ["generic", "graphics"] {
            drawing
                .as_object_mut()
                .context("Missing generic drawing")?
                .remove(key);
            legacy
                .as_object_mut()
                .context("Missing legacy drawing")?
                .remove(key);
        }
        anyhow::ensure!(
            drawing == legacy,
            "Generic import changed unrelated drawing fields"
        );
        anyhow::ensure!(
            expected["analysis"].is_object()
                && expected["warnings"].as_array().is_some_and(Vec::is_empty),
            "Unexpected legacy generic analysis/warnings"
        );
        anyhow::ensure!(
            generic["analysis"].is_null()
                && generic["warnings"]
                    .as_array()
                    .is_some_and(|warnings| !warnings.is_empty()),
            "Missing generic chemistry restriction notice"
        );
        expected["document"] = generic["document"].clone();
        expected["analysis"] = generic["analysis"].clone();
        expected["warnings"] = generic["warnings"].clone();
    }
    Ok(expected)
}

// The unchanged worker flattens Z. Adapt exactly that deliberate display
// delta, retaining full equality for chemistry, source IDs, all other drawing
// fields and every unaffected 2D response. Explicit-H folding has separate
// independent actual-executable graph/stereo/XYZ controls.
pub fn mol_source_xyz(mut expected: Response, text: &str) -> anyhow::Result<Response> {
    let imported = reshiki::chemistry::molfile::read(text)?;
    if !imported.annotations.is_3d
        && !imported
            .molecule
            .positions
            .iter()
            .any(|position| position.z != 0.)
    {
        return Ok(expected);
    }
    let doc = expected
        .document
        .as_mut()
        .context("Missing source MOL drawing")?;
    let points: std::collections::HashMap<_, _> = imported
        .molecule
        .ids
        .iter()
        .copied()
        .zip(&imported.molecule.positions)
        .collect();
    for atom in &mut doc.atoms {
        anyhow::ensure!(atom.depth == 0., "Legacy MOL drawing no longer flattens Z");
        atom.depth = (points
            .get(&atom.id)
            .context("Missing original source atom")?
            .z
            * 28.) as f32;
    }
    let mut any = std::collections::HashSet::new();
    let mut unknown_singles = std::collections::HashSet::new();
    for (bond, metadata) in imported
        .molecule
        .state
        .graph
        .bonds
        .iter()
        .zip(&imported.molecule.state.metadata.bonds)
    {
        if bond.order == 2 && metadata.stereo == 1 {
            let a = imported.molecule.ids[bond.a];
            let b = imported.molecule.ids[bond.b];
            any.insert((a.min(b), a.max(b)));
        }
        if bond.order == 1 && metadata.unknown_stereo {
            let a = imported.molecule.ids[bond.a];
            let b = imported.molecule.ids[bond.b];
            unknown_singles.insert((a.min(b), a.max(b)));
        }
    }
    for bond in &mut doc.bonds {
        anyhow::ensure!(
            !bond.projection && !bond.stereo_authoritative,
            "Legacy MOL drawing already has source-XYZ controls"
        );
        // RDKit's 3D assignment consumes the source unknown single paint.
        // #296 restores that exact chemical unknown before projection controls.
        if bond.order == 1 && unknown_singles.contains(&(bond.a.min(bond.b), bond.a.max(bond.b))) {
            anyhow::ensure!(
                bond.display == "plain",
                "Unexpected legacy source-unknown single paint"
            );
            bond.display = "wavy".into();
        }
        if bond.order == 2 {
            bond.stereo_authoritative = true;
            if any.contains(&(bond.a.min(bond.b), bond.a.max(bond.b))) {
                bond.stereo = Some("any".into());
            }
        }
        if matches!(bond.order, 1 | 2 | 4) && !(bond.order == 1 && bond.display == "wavy") {
            bond.projection = true;
        }
    }
    Ok(expected)
}
