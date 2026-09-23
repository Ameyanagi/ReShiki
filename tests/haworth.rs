use anyhow::{Context, Result};
use reshiki::{
    document::{Document, Point},
    editing::{self, Transform},
    engine::{Analysis, ChemistryEngine, LocalEngine, Request},
    haworth::{Anomer, Ring, Sugar, sugar_document},
    rings::Preset,
    templates::{self, LIBRARY},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Reference {
    name: String,
    sugar: Sugar,
    anomer: Anomer,
    smiles: String,
    formula: String,
    inchikey: String,
}
fn references() -> Result<Vec<Reference>> {
    Ok(serde_json::from_str(include_str!(
        "../assets/haworth-sugars.json"
    ))?)
}

#[tokio::test]
async fn observed_chemdraw_clipboard_outputs_retain_all_ten_sugar_identities() -> Result<()> {
    #[derive(Deserialize)]
    struct Captured {
        name: String,
        chemdraw_smiles: String,
        inchikey: String,
    }
    #[derive(Deserialize)]
    struct Fixture {
        results: Vec<Captured>,
    }
    let fixture: Fixture = serde_json::from_str(include_str!("fixtures/haworth-chemdraw.json"))?;
    let references = references()?;
    assert_eq!(fixture.results.len(), references.len());
    for captured in fixture.results {
        let reference = references
            .iter()
            .find(|r| r.name == captured.name)
            .context("Unknown captured sugar")?;
        let response = LocalEngine::default()
            .execute(Request::import_smiles(&captured.chemdraw_smiles))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = response
            .analysis
            .context("Missing ChemDraw SMILES analysis")?;
        assert_eq!(analysis.inchikey, reference.inchikey, "{}", captured.name);
        assert_eq!(analysis.inchikey, captured.inchikey);
    }
    Ok(())
}
async fn analyze(doc: Document) -> Result<Analysis> {
    LocalEngine::default()
        .execute(Request::molecule("analyze", doc))
        .await
        .map_err(anyhow::Error::msg)?
        .analysis
        .context("Missing analysis")
}

#[tokio::test]
async fn all_ten_anomers_match_independent_pubchem_identities() -> Result<()> {
    let mut keys = std::collections::HashSet::new();
    for reference in references()? {
        let doc =
            sugar_document(reference.sugar, reference.anomer, 42.).map_err(anyhow::Error::msg)?;
        let analysis = analyze(doc.clone()).await?;
        let expected = LocalEngine::default()
            .execute(Request::import_smiles(&reference.smiles))
            .await
            .map_err(anyhow::Error::msg)?
            .analysis
            .context("Missing reference analysis")?;
        assert_eq!(analysis.formula, reference.formula, "{}", reference.name);
        assert_eq!(analysis.smiles, expected.smiles, "{}", reference.name);
        assert_eq!(analysis.inchikey, reference.inchikey, "{}", reference.name);
        let cleaned = LocalEngine::default()
            .execute(Request::molecule("clean", doc))
            .await
            .map_err(anyhow::Error::msg)?
            .analysis
            .context("Missing cleanup analysis")?;
        assert_eq!(
            cleaned.inchikey, reference.inchikey,
            "{} cleanup",
            reference.name
        );
        assert!(
            keys.insert(analysis.inchikey),
            "Anomers must remain distinct"
        );
    }
    assert_eq!(keys.len(), 10);
    Ok(())
}

#[test]
fn outlines_have_closed_front_junctions_and_no_invented_stereo() -> Result<()> {
    for (ring, size, preset) in [
        (Ring::Five, 5, Preset::HaworthFive),
        (Ring::Six, 6, Preset::HaworthSix),
    ] {
        for length in [24., 42., 80.] {
            let doc = ring.document(length, false);
            doc.validate().map_err(anyhow::Error::msg)?;
            assert_eq!(doc, preset.document(length, false));
            assert_eq!(doc.atoms.len(), size);
            assert_eq!(doc.bonds.len(), size);
            assert!(
                doc.atoms
                    .iter()
                    .all(|a| a.element == "C" && a.stereo.is_none())
            );
            assert_eq!(doc.bonds.iter().filter(|b| b.display == "bold").count(), 1);
            assert_eq!(doc.bonds.iter().filter(|b| b.display == "wedge").count(), 2);
            for atom in &doc.atoms {
                assert_eq!(
                    doc.bonds
                        .iter()
                        .filter(|b| b.a == atom.id || b.b == atom.id)
                        .count(),
                    2
                );
            }
            let molecule = reshiki::chemistry::document::prepare(&doc)?;
            assert!(
                molecule
                    .state
                    .metadata
                    .atoms
                    .iter()
                    .all(|a| a.chiral_tag == 0)
            );
            assert!(
                molecule
                    .state
                    .directions
                    .iter()
                    .all(|d| *d == reshiki::chemistry::kekulize::Direction::None)
            );
        }
    }
    for length in [f32::NAN, f32::INFINITY, -1., 0.] {
        assert!(sugar_document(Sugar::Glucose, Anomer::Alpha, length).is_err());
        assert!(Ring::Five.document(length, false).atoms.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn placement_rotation_copy_and_native_round_trip_preserve_sugar_identity() -> Result<()> {
    for reference in references()? {
        let template = LIBRARY
            .iter()
            .find(|t| t.name == format!("{} · Haworth", reference.name))
            .context("Missing carbohydrate template")?;
        let (mut doc, ids) = templates::place(
            &Document::default(),
            &template.document,
            Point::new(120., 90.),
            None,
            5.,
        )
        .map_err(anyhow::Error::msg)?;
        editing::transform(&mut doc, &ids, Transform::Rotate(73.));
        let mut copied = Document::default();
        editing::append(&mut copied, &doc, Point::new(200., -70.));
        let saved: Document = serde_json::from_str(&serde_json::to_string(&copied)?)?;
        assert_eq!(saved, copied);
        assert_eq!(
            analyze(saved).await?.inchikey,
            reference.inchikey,
            "{}",
            reference.name
        );
    }
    Ok(())
}

#[tokio::test]
async fn chemical_export_retains_anomers_and_figure_export_retains_perspective() -> Result<()> {
    for reference in references()? {
        let doc =
            sugar_document(reference.sugar, reference.anomer, 42.).map_err(anyhow::Error::msg)?;
        for format in ["mol", "smiles"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let output = LocalEngine::default()
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?
                .output
                .context("Missing chemical export")?;
            let restored = LocalEngine::default()
                .execute(Request::import(format, &output))
                .await
                .map_err(anyhow::Error::msg)?;
            if format == "mol" {
                assert_eq!(
                    output.lines().nth(3).and_then(|line| line.get(12..15)),
                    Some("  1"),
                    "Absolute configuration must survive ChemDraw import"
                );
            }
            assert_eq!(
                restored
                    .analysis
                    .context("Missing imported chemistry")?
                    .inchikey,
                reference.inchikey,
                "{} {format}",
                reference.name
            );
        }
        for format in ["svg", "png", "pdf"] {
            assert!(
                !reshiki::export::drawing(&doc, format)
                    .map_err(anyhow::Error::msg)?
                    .is_empty()
            );
        }
    }
    Ok(())
}

#[test]
fn absolute_mol_flag_is_present_only_for_defined_tetrahedral_stereo() -> Result<()> {
    use reshiki::chemistry::{document, molfile};
    for (doc, expected) in [
        (
            sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?,
            "1",
        ),
        (Ring::Six.document(42., true), "0"),
    ] {
        let molecule = document::prepare(&doc)?;
        for force_v3000 in [false, true] {
            let output = molfile::write_absolute(&molecule, molfile::Options { force_v3000 })?;
            let flag = if force_v3000 {
                output
                    .lines()
                    .find(|line| line.starts_with("M  V30 COUNTS "))
                    .and_then(|line| line.split_whitespace().last())
            } else {
                output
                    .lines()
                    .nth(3)
                    .and_then(|line| line.get(12..15))
                    .map(str::trim)
            };
            assert_eq!(flag, Some(expected));
        }
    }
    Ok(())
}

#[tokio::test]
async fn restyling_and_reversing_perspective_edges_preserves_all_stereocenters() -> Result<()> {
    use reshiki::bonds::BondPreset;
    for reference in references()? {
        let original =
            sugar_document(reference.sugar, reference.anomer, 42.).map_err(anyhow::Error::msg)?;
        let front = original
            .bonds
            .iter()
            .find(|b| b.display == "bold")
            .context("Missing front edge")?;
        for preset in [
            BondPreset::Single,
            BondPreset::Bold,
            BondPreset::Wedge,
            BondPreset::HashedWedge,
            BondPreset::HollowWedge,
        ] {
            let mut doc = original.clone();
            preset.place(&mut doc, front.b, front.a);
            let bond = doc
                .bonds
                .iter()
                .find(|b| b.a == front.b && b.b == front.a)
                .context("Reversed front edge")?;
            assert!(bond.projection);
            assert_eq!(
                analyze(doc).await?.inchikey,
                reference.inchikey,
                "{} {preset:?}",
                reference.name
            );
        }
    }
    Ok(())
}

#[test]
fn substituent_directions_and_condensed_groups_match_haworth_conventions() -> Result<()> {
    for reference in references()? {
        let doc =
            sugar_document(reference.sugar, reference.anomer, 42.).map_err(anyhow::Error::msg)?;
        let anomeric = doc.atoms.first().context("Missing anomeric carbon")?;
        let stereo = anomeric
            .stereo
            .as_ref()
            .context("Missing anomeric stereo")?;
        let oxygen = doc
            .atom(
                *stereo
                    .neighbors
                    .get(2)
                    .context("Missing hydroxyl neighbor")?,
            )
            .context("Missing hydroxyl")?;
        assert_eq!(oxygen.element, "O");
        assert_eq!(
            oxygen.position.y < anomeric.position.y,
            reference.anomer == Anomer::Beta
        );
        for group in &doc.abbreviations {
            assert_eq!(group.label, "CH2OH");
            assert_eq!(group.members.len(), 2);
            let mut elements = group
                .members
                .iter()
                .map(|id| doc.atom(*id).map(|a| a.element.as_str()))
                .collect::<Option<Vec<_>>>()
                .context("Missing abbreviated atoms")?;
            elements.sort_unstable();
            assert_eq!(elements, ["C", "O"]);
        }
        assert_eq!(
            doc.atoms.iter().filter(|a| a.element == "C").count(),
            if matches!(reference.sugar, Sugar::Ribose) {
                5
            } else {
                6
            }
        );
        reshiki::exchange::drawing::write(&doc, Default::default())?;
    }
    Ok(())
}
