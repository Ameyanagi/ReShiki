use anyhow::{Context, Result};
use reshiki::{
    document::Document,
    editing::{self, Transform},
    engine::{ChemistryEngine, LocalEngine, Request},
    exchange::{self, drawing},
    haworth::{Anomer, Sugar},
};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Captured {
    id: String,
    inchikey: String,
    atoms: usize,
    bonds: usize,
}
#[derive(Deserialize)]
struct Manifest {
    structures: Vec<Captured>,
}
fn captures() -> Result<Vec<Captured>> {
    let manifest: Manifest =
        serde_json::from_str(include_str!("fixtures/haworth-interchange/manifest.json"))?;
    Ok(manifest.structures)
}
async fn check(xml: &str, expected: &Captured) -> Result<Document> {
    check_format("cdxml", xml, expected).await
}
async fn check_format(format: &str, text: &str, expected: &Captured) -> Result<Document> {
    let response = LocalEngine::default()
        .execute(Request::import(format, text))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        response.analysis.context("Missing analysis")?.inchikey,
        expected.inchikey,
        "{}",
        expected.id
    );
    let document = response.document.context("Missing drawing")?;
    assert_eq!(document.atoms.len(), expected.atoms);
    assert_eq!(document.bonds.len(), expected.bonds);
    assert_eq!(
        document
            .bonds
            .iter()
            .filter(|b| b.display == "bold")
            .count(),
        1
    );
    assert_eq!(
        document
            .bonds
            .iter()
            .filter(|b| b.display == "wedge")
            .count(),
        2
    );
    let n = if expected.id.contains("pyranose") || expected.id.contains("6-ring") {
        6
    } else {
        5
    };
    assert_eq!(
        document.bonds.iter().filter(|b| b.projection).count(),
        n,
        "{}",
        expected.id
    );
    Ok(document)
}

#[tokio::test]
async fn chemdraw_v2000_and_v3000_mol_haworths_retain_all_fourteen_identities() -> Result<()> {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/haworth-interchange");
    for reference in captures()? {
        for format in ["mol", "mol-v2000"] {
            let text =
                std::fs::read_to_string(directory.join(format!("{}.{format}.mol", reference.id)))?;
            let document = check_format("mol", &text, &reference).await?;
            let exported = reshiki::chemistry::molfile::write_document(&document)?;
            let response = LocalEngine::default()
                .execute(Request::import("mol", &exported))
                .await
                .map_err(anyhow::Error::msg)?;
            assert_eq!(
                response.analysis.context("MOL analysis")?.inchikey,
                reference.inchikey
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn all_fourteen_genuine_chemdraw_cdxml_and_cdx_files_preserve_identity_and_appearance()
-> Result<()> {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/haworth-interchange");
    let references = captures()?;
    assert_eq!(references.len(), 14);
    for reference in references {
        for format in ["cdxml", "cdx"] {
            let bytes = std::fs::read(directory.join(format!("{}.{format}", reference.id)))?;
            let xml = if format == "cdx" {
                exchange::from_cdx(&bytes).map_err(anyhow::Error::msg)?
            } else {
                String::from_utf8(bytes)?
            };
            let document = check(&xml, &reference).await?;
            let exported = drawing::write(&document, Default::default())?;
            check(&exported, &reference).await?;
            let binary = exchange::to_cdx(&exported).map_err(anyhow::Error::msg)?;
            check(
                &exchange::from_cdx(&binary).map_err(anyhow::Error::msg)?,
                &reference,
            )
            .await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn native_templates_export_with_stereo_after_rotation_translation_and_front_bond_reversal()
-> Result<()> {
    for reference in captures()? {
        let template = reshiki::templates::LIBRARY.iter().find(|t| {
            t.name
                .replace(" · Haworth", "")
                .replace('α', "alpha")
                .replace('β', "beta")
                == reference.id
        });
        let source = if let Some(t) = template {
            t.document.clone()
        } else {
            match reference.id.as_str() {
                "furanose-scaffold" => reshiki::haworth::Ring::Five.document(42., true),
                "pyranose-scaffold" => reshiki::haworth::Ring::Six.document(42., true),
                "carbon-5-ring" => reshiki::haworth::Ring::Five.document(42., false),
                "carbon-6-ring" => reshiki::haworth::Ring::Six.document(42., false),
                _ => anyhow::bail!("Missing template {}", reference.id),
            }
        };
        for angle in [0., 37., 180., 271.] {
            let mut document = source.clone();
            let ids = document.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
            editing::transform(&mut document, &ids, Transform::Rotate(angle));
            for atom in &mut document.atoms {
                atom.position = atom.position.offset(235., -179.);
            }
            let bold = document
                .bonds
                .iter_mut()
                .find(|b| b.display == "bold")
                .context("Missing front edge")?;
            std::mem::swap(&mut bold.a, &mut bold.b);
            let xml = drawing::write(&document, Default::default())?;
            check(&xml, &reference).await?;
        }
    }
    Ok(())
}

#[test]
fn ambiguous_or_conflicting_projection_is_rejected_without_panics_or_mutation() -> Result<()> {
    let mut flat = reshiki::haworth::Ring::Six.document(42., false);
    for atom in &mut flat.atoms {
        atom.position.y = 0.;
    }
    assert!(drawing::write(&flat, Default::default()).is_err());
    let original = reshiki::haworth::sugar_document(Sugar::Glucose, Anomer::Alpha, 42.)
        .map_err(anyhow::Error::msg)?;
    for mode in 0..8 {
        let mut document = original.clone();
        match mode {
            0 => {
                document.atoms.first_mut().context("atom")?.stereo = None;
            }
            1 => {
                document
                    .atoms
                    .first_mut()
                    .and_then(|a| a.stereo.as_mut())
                    .context("stereo")?
                    .winding = "ccw".into();
            }
            2 => {
                document.atoms.first_mut().context("atom")?.position.x += 12.;
            }
            3 => {
                document.atoms.get_mut(6).context("substituent")?.position.x += 20.;
            }
            4 => {
                document.bonds.first_mut().context("bond")?.display = "hash".into();
            }
            5 => {
                for atom in &mut document.atoms {
                    atom.position.y = 0.;
                }
            }
            6 => {
                let ids = document.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
                editing::transform(&mut document, &ids, Transform::FlipHorizontal);
            }
            _ => {
                for bond in &mut document.bonds {
                    bond.projection = false;
                }
                document.atoms.first_mut().context("atom")?.stereo = None;
            }
        }
        let before = document.clone();
        assert!(
            drawing::write(&document, Default::default()).is_err(),
            "mode {mode}"
        );
        assert_eq!(document, before);
    }
    Ok(())
}

#[tokio::test]
async fn cached_cip_letters_do_not_override_the_visible_haworth_convention() -> Result<()> {
    let xml = include_str!("fixtures/haworth-interchange/alpha-D-glucopyranose.cdxml");
    let altered = xml
        .replace("AS=\"R\"", "AS=\"u\"")
        .replace("AS=\"S\"", "AS=\"u\"");
    let reference = captures()?
        .into_iter()
        .find(|r| r.id == "alpha-D-glucopyranose")
        .context("reference")?;
    check(&altered, &reference).await?;
    Ok(())
}

#[test]
fn three_dimensional_scene_is_not_reinterpreted_as_a_2d_haworth() -> Result<()> {
    let original = include_str!("fixtures/haworth-interchange/alpha-D-glucopyranose.cdxml");
    let mut scene = reshiki::chemistry::cdxml::assemble_cdxml(
        &reshiki::chemistry::cdxml::prepare_cdxml(original)?,
    )?;
    scene.conformer_3d = Some(true);
    assert!(
        scene
            .into_document()?
            .document
            .bonds
            .iter()
            .all(|b| !b.projection)
    );
    Ok(())
}
