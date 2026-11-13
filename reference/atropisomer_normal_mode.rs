//! #1117: RDKit-parity runs lose the atropisomer axis of RDKit's
//! AtropManyChirals inputs, which `engine_migration` counts. This binary never
//! enables parity mode, so it checks the import users get: the axis RDKit
//! perceives in each input survives on the same bond with the same winding and
//! M label, and no other axis appears.
#[path = "support/axial_extension.rs"]
mod axial_extension;
#[path = "support/parity_atropisomers.rs"]
mod parity_atropisomers;

use anyhow::Context;
use reshiki::engine::{ChemistryEngine, LocalEngine, Request};

#[tokio::test]
async fn atropisomer_axes_survive_the_normal_import() -> anyhow::Result<()> {
    // With the pinned source, the inputs must still be the source's own.
    let data = std::env::var_os("RESHIKI_RDKIT_SOURCE")
        .map(|source| std::path::Path::new(&source).join("Code/GraphMol/SmilesParse/test_data"));
    let local = LocalEngine::default();
    for (file, text) in parity_atropisomers::CASES {
        if let Some(data) = &data {
            let input = std::fs::read_to_string(data.join(file))?;
            anyhow::ensure!(
                input.lines().next().map(str::trim) == Some(text),
                "{file} changed in the RDKit source"
            );
        }
        let response = local
            .execute(Request::import("smiles", text))
            .await
            .map_err(anyhow::Error::msg)?;
        let document = response.document.context("Missing document")?;
        anyhow::ensure!(
            axial_extension::accepted(
                file,
                Some(parity_atropisomers::FAILURE),
                "smiles",
                text,
                axial_extension::Checked::Document(&document),
            ),
            "{file}: the normal import lost or changed RDKit's axis"
        );
        let labels: Vec<_> = document
            .bonds
            .iter()
            .filter_map(|bond| bond.cip_label.as_deref())
            .collect();
        anyhow::ensure!(labels == ["M"], "{file}: bond labels {labels:?}");
    }
    Ok(())
}
