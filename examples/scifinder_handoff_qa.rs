//! Prepare public manual-handoff fixtures locally. Does not contact CAS or use the clipboard.
use anyhow::{Context, ensure};
use reshiki::{
    document::Document,
    engine::{LocalEngine, Request},
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, future::Future, path::PathBuf, pin::Pin};

// Construct operation futures in a separate frame before heap-pinning them.
// An inline Box::pin(operation()) still leaves large temporary futures in the
// caller's poll frame in debug builds, overflowing Windows' main stack.
#[inline(never)]
fn heap_future<F: Future>(make: impl FnOnce() -> F) -> Pin<Box<F>> {
    Box::pin(make())
}

// InChI may materialize a stereochemical hydrogen as a separate atom. Compare
// the heavy graph here; formula and the full InChIKey still check total H,
// isotope and stereochemical identity in the round-trip loop below.
fn heavy_counts(document: &Document) -> (usize, usize) {
    let heavy: std::collections::HashSet<_> = document
        .atoms
        .iter()
        .filter(|atom| atom.element != "H")
        .map(|atom| atom.id)
        .collect();
    let bonds = document
        .bonds
        .iter()
        .filter(|bond| heavy.contains(&bond.a) && heavy.contains(&bond.b))
        .count();
    (heavy.len(), bonds)
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct ReactionOracle {
    reaction_smiles: String,
    // Per-participant coefficient, atom count and internal bond count, in role order.
    reactants: Vec<(u16, usize, usize)>,
    products: Vec<(u16, usize, usize)>,
    agents: Vec<(u16, usize, usize)>,
}

fn reaction_oracle(document: &Document) -> anyhow::Result<ReactionOracle> {
    document.validate().map_err(anyhow::Error::msg)?;
    let [reaction] = document.reactions.as_slice() else {
        anyhow::bail!("Expected exactly one explicitly defined reaction");
    };
    let participants = |values: &[reshiki::reactions::Participant]| {
        values
            .iter()
            .map(|participant| {
                let bonds = document
                    .bonds
                    .iter()
                    .filter(|bond| {
                        participant.atoms.contains(&bond.a) && participant.atoms.contains(&bond.b)
                    })
                    .count();
                (participant.coefficient, participant.atoms.len(), bonds)
            })
            .collect()
    };
    Ok(ReactionOracle {
        reaction_smiles: reshiki::chemistry::reaction::write_smiles(document, None)?,
        reactants: participants(&reaction.reactants),
        products: participants(&reaction.products),
        agents: participants(&reaction.agents),
    })
}

async fn reaction_fixture(
    engine: &LocalEngine,
    output: &std::path::Path,
) -> anyhow::Result<serde_json::Value> {
    let name = "ethanol-oxidation";
    let input = "CCO>>CC=O";
    let source = heap_future(|| engine.request(Request::import("rsmi", input)))
        .await
        .map_err(anyhow::Error::msg)?;
    let document = source.document.context("Missing reaction drawing")?;
    let oracle = reaction_oracle(&document)?;
    ensure!(
        oracle.reaction_smiles == input,
        "Source reaction connectivity or roles changed"
    );
    ensure!(
        oracle.reactants == [(1, 3, 2)]
            && oracle.products == [(1, 3, 2)]
            && oracle.agents.is_empty(),
        "Unexpected source reaction participants"
    );
    let native = serde_json::to_vec_pretty(&document)?;
    let restored = Document::from_json(&native).map_err(anyhow::Error::msg)?;
    ensure!(
        reaction_oracle(&restored)? == oracle,
        "Native reaction round trip changed"
    );
    fs::write(output.join(format!("{name}.rsk")), &native)?;
    let mut formats = Vec::new();
    for format in ["rxn", "rsmi"] {
        let mut request = Request::molecule("export", document.clone());
        request.format = Some(format.into());
        let exported = heap_future(|| engine.request(request))
            .await
            .map_err(anyhow::Error::msg)?;
        let text = exported.output.context("Missing reaction export")?;
        if format == "rxn" {
            ensure!(text.starts_with("$RXN V3000"), "Expected RXN V3000");
        }
        let restored = heap_future(|| engine.request(Request::import(format, &text)))
            .await
            .map_err(anyhow::Error::msg)?;
        let roundtrip = reaction_oracle(&restored.document.context("Missing imported reaction")?)?;
        ensure!(
            roundtrip == oracle,
            "{format}: reaction roles or connectivity changed"
        );
        let filename = format!("{name}.{format}");
        fs::write(output.join(&filename), &text)?;
        formats.push(json!({
            "file": filename,
            "sha256": format!("{:x}", Sha256::digest(text.as_bytes())),
            "local_round_trip": "passed",
            "oracle": roundtrip,
            "export_warnings": exported.warnings,
            "import_warnings": restored.warnings,
            "cas_editor_import": "not tested",
            "cas_search": "not tested"
        }));
    }
    // Use exactly the same preparation path as the Copy As command. This file
    // is the receiver-test payload, not a separately handcrafted JSON probe.
    let copied = heap_future(|| {
        reshiki::clipboard::prepare_as(
            engine.clone(),
            document.clone(),
            reshiki::clipboard::CopyFormat::ChemDoodleReaction,
        )
    })
    .await
    .map_err(anyhow::Error::msg)?;
    let text = copied.text().context("Missing ChemDoodle clipboard text")?;
    let json: serde_json::Value = serde_json::from_str(text)?;
    let molecules = json["m"]
        .as_array()
        .context("Missing ChemDoodle molecules")?;
    ensure!(molecules.len() == 2, "ChemDoodle participant count changed");
    for (side, orders, hydrogens) in [(0, [1, 1], [3, 2, 1]), (1, [1, 2], [3, 1, 0])] {
        let molecule = &molecules[side];
        let atoms = molecule["a"]
            .as_array()
            .context("Missing ChemDoodle atoms")?;
        let bonds = molecule["b"]
            .as_array()
            .context("Missing ChemDoodle bonds")?;
        ensure!(
            atoms.len() == 3 && bonds.len() == 2,
            "ChemDoodle graph count changed"
        );
        for (i, element) in ["C", "C", "O"].iter().enumerate() {
            ensure!(
                atoms[i]["l"] == *element && atoms[i]["c"] == 0 && atoms[i]["h"] == hydrogens[i],
                "ChemDoodle atom identity changed"
            );
        }
        for (i, order) in orders.iter().enumerate() {
            ensure!(
                bonds[i]["b"] == i && bonds[i]["e"] == i + 1 && bonds[i]["o"] == *order,
                "ChemDoodle connectivity changed"
            );
        }
        let role = if side == 0 { "rs" } else { "ps" };
        ensure!(
            json["s"][0][role] == json!([atoms[0]["i"]]),
            "ChemDoodle role changed"
        );
    }
    ensure!(
        json["s"][0]["a"] == "synthetic",
        "ChemDoodle reaction arrow changed"
    );
    let filename = format!("{name}.chemdoodle.json");
    fs::write(output.join(&filename), text)?;
    formats.push(json!({
        "file": filename,
        "sha256": format!("{:x}", Sha256::digest(text.as_bytes())),
        "source": "clipboard::prepare_as(CopyFormat::ChemDoodleReaction).text()",
        "local_validation": "Expected atom identities, H counts, bonds, arrow and role references passed",
        "local_round_trip": "Not applicable: ReShiki does not import ChemDoodle JSON",
        "export_warnings": copied.notices,
        "cas_editor_import": "not tested",
        "cas_search": "not tested"
    }));
    println!(
        "{name}: {input} · native/RXN/reaction-SMILES role and connectivity round trips passed"
    );
    Ok(json!({
        "name": name,
        "kind": "reaction",
        "input_reaction_smiles": input,
        "description": "Public ethanol to acetaldehyde connectivity and role control; no reagents or conditions supplied or inferred",
        "source_drawing": format!("{name}.rsk"),
        "source_sha256": format!("{:x}", Sha256::digest(&native)),
        "source_warnings": source.warnings,
        "native_round_trip": "passed",
        "oracle": oracle,
        "formats": formats
    }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("usage: scifinder_handoff_qa NEW_OUTPUT_DIRECTORY")?,
    );
    generate_fixtures(&output).await
}

async fn generate_fixtures(output: &std::path::Path) -> anyhow::Result<()> {
    fs::create_dir(output).context("Choose a new output directory with an existing parent")?;
    let engine = LocalEngine::default();
    let mut fixtures = Vec::new();
    for (name, input, abbreviate) in [
        ("ethanol", "CCO", false),
        ("lactic-acid-stereo", "C[C@H](O)C(=O)O", false),
        ("difluoroethene-stereo", "F/C=C/F", false),
        ("defined-abbreviations", "COc1ccc(NC(=O)OC(C)(C)C)cc1", true),
    ] {
        let response = heap_future(|| engine.request(Request::import_smiles(input)))
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(
            response.warnings.is_empty(),
            "{name}: {:?}",
            response.warnings
        );
        let original = response.analysis.context("Missing source identity")?;
        ensure!(!original.inchikey.is_empty(), "{name}: empty InChIKey");
        let mut document = response.document.context("Missing source drawing")?;
        if abbreviate {
            let mut request = Request::molecule("abbreviate", document.clone());
            request.selected_ids = Some(document.all_ids());
            let response = heap_future(|| engine.request(request))
                .await
                .map_err(anyhow::Error::msg)?;
            ensure!(
                response.warnings.is_empty(),
                "{name}: {:?}",
                response.warnings
            );
            document = response.document.context("Missing abbreviated drawing")?;
            ensure!(
                !document.abbreviations.is_empty(),
                "{name}: no abbreviations"
            );
        }
        document.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            reshiki::reactions::molecules(&document, &document.all_ids()).len() == 1,
            "{name}: fixture must contain exactly one connected molecule"
        );
        fs::write(
            output.join(format!("{name}.rsk")),
            serde_json::to_vec_pretty(&document)?,
        )?;
        let mut formats = Vec::new();
        for format in ["smiles", "mol", "inchi"] {
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(format.into());
            let response = heap_future(|| engine.request(request))
                .await
                .map_err(anyhow::Error::msg)?;
            ensure!(
                response.warnings.is_empty(),
                "{name} {format}: {:?}",
                response.warnings
            );
            let text = response.output.context("Missing exported text")?;
            ensure!(!text.trim().is_empty(), "{name} {format}: empty export");
            let restored = heap_future(|| engine.request(Request::import(format, &text)))
                .await
                .map_err(anyhow::Error::msg)?;
            ensure!(
                restored.warnings.is_empty(),
                "{name} {format}: {:?}",
                restored.warnings
            );
            let analysis = restored.analysis.context("Missing round-trip identity")?;
            let restored = restored.document.context("Missing round-trip drawing")?;
            ensure!(
                analysis.inchikey == original.inchikey,
                "{name} {format}: identity changed"
            );
            ensure!(
                analysis.formula == original.formula,
                "{name} {format}: formula changed"
            );
            ensure!(
                heavy_counts(&restored) == heavy_counts(&document),
                "{name} {format}: heavy-atom or heavy-bond count changed"
            );
            let filename = format!("{name}.{format}");
            fs::write(output.join(&filename), &text)?;
            formats.push(json!({
                "file": filename,
                "sha256": format!("{:x}", Sha256::digest(text.as_bytes())),
                "local_round_trip": "passed",
                "inchikey": analysis.inchikey,
                "formula": analysis.formula,
                "heavy_atoms_and_bonds": heavy_counts(&restored),
                "explicit_hydrogen_atoms": restored.atoms.iter().filter(|atom| atom.element == "H").count(),
                "warnings": [],
                "cas_editor_import": "not tested",
                "cas_search": "not tested"
            }));
        }
        println!(
            "{name}: {} · 3 local format round trips passed",
            original.inchikey
        );
        fixtures.push(json!({
            "name": name,
            "input_smiles": input,
            "source_drawing": format!("{name}.rsk"),
            "atoms": document.atoms.len(),
            "bonds": document.bonds.len(),
            "heavy_atoms_and_bonds": heavy_counts(&document),
            "count_policy": "Hydrogen atom materialization may vary; full InChIKey and formula must match, including isotope and stereochemical identity",
            "defined_abbreviations": document.abbreviations.len(),
            "analysis": original,
            "formats": formats
        }));
    }
    fixtures.push(reaction_fixture(&engine, output).await?);
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "reshiki_version": env!("CARGO_PKG_VERSION"),
            "purpose": "Local preparation for a manual CAS SciFinder handoff",
            "scope": "Four public single-molecule fixtures and one public reaction control; no network or clipboard access",
            "cas_validation": "Not tested. Requires an authorized CAS SciFinder session.",
            "fixtures": fixtures
        }))?,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "scifinder_handoff_qa/tests.rs"]
mod tests;
