use super::*;
use crate::chemistry::{depict, stereo::perception::RingKind};

#[test]
fn projected_inchi_retains_captured_stereo_without_guessing_from_xy() -> anyhow::Result<()> {
    for (text, expected) in [
        ("FC=CF", "InChI=1S/C2H2F2/c3-1-2-4/h1-2H"),
        ("F/C=C/F", "InChI=1S/C2H2F2/c3-1-2-4/h1-2H/b2-1+"),
        ("F/C=C\\F", "InChI=1S/C2H2F2/c3-1-2-4/h1-2H/b2-1-"),
        ("FC(Cl)(Br)I", "InChI=1S/CBrClFI/c2-1(3,4)5"),
        ("F[C@](Cl)(Br)I", "InChI=1S/CBrClFI/c2-1(3,4)5/t1-/m1/s1"),
        ("F[C@@](Cl)(Br)I", "InChI=1S/CBrClFI/c2-1(3,4)5/t1-/m0/s1"),
    ] {
        let mut state = smiles::prepare(text)?.state;
        state.rings.kind = RingKind::Symmetric;
        let ranks = state
            .properties
            .atoms
            .iter()
            .map(|atom| atom.cip_rank)
            .collect::<Vec<_>>();
        let conformer = depict::compute(&state, &ranks, None, Default::default())?;
        let molecule = molecular::Molecule {
            rdkit_version: chemistry::RDKIT_VERSION,
            ids: (1..=state.graph.atoms.len() as u64).collect(),
            positions: conformer.positions,
            state,
        };
        let before = serde_json::to_value(&molecule)?;
        let (actual_smiles, input) = prepare_identifiers(&molecule, true)?;
        assert_eq!(
            actual_smiles,
            smiles::write::write(&molecule.state, Default::default())?.text
        );
        let input = input.ok_or_else(|| anyhow::anyhow!("Missing InChI input"))?;
        assert!(input.positions.is_none());
        assert_eq!(
            kernel::generate(&input).map_err(anyhow::Error::msg)?.inchi,
            expected,
            "{text}"
        );
        assert_eq!(serde_json::to_value(&molecule)?, before);
    }
    Ok(())
}

#[tokio::test]
async fn projected_engine_analyze_and_exports_preserve_the_drawing() -> anyhow::Result<()> {
    if std::env::var_os("RESHIKI_INCHI_HELPER").is_none() {
        eprintln!("Set RESHIKI_INCHI_HELPER to run projected real-engine response coverage");
        return Ok(());
    }
    for text in [
        "F/C=C/F",
        "F/C=C\\F",
        "FC=CF",
        "F[C@](Cl)(Br)I",
        "FC(Cl)(Br)I",
    ] {
        let imported = smiles::prepare(text)?;
        let mut state = imported.state;
        state.rings.kind = RingKind::Symmetric;
        let ranks = state
            .properties
            .atoms
            .iter()
            .map(|atom| atom.cip_rank)
            .collect::<Vec<_>>();
        let conformer = depict::compute(&state, &ranks, None, Default::default())?;
        let molecule = molecular::Molecule {
            rdkit_version: chemistry::RDKIT_VERSION,
            ids: (1..=state.graph.atoms.len() as u64).collect(),
            positions: conformer.positions,
            state,
        };
        let expected_smiles = smiles::write::write(&molecule.state, Default::default())?.text;
        let expected_inchi = kernel::generate(&kernel::Molecule::prepare(&molecule.state, None)?)
            .map_err(anyhow::Error::msg)?
            .inchi;
        let drawing = molecular::for_import(&molecule, false, &imported.dummy_labels)?;
        let mut document = drawing.clone().finish(drawing.labels()?)?;
        for (i, atom) in document.atoms.iter_mut().enumerate() {
            // Coincident XY is a legal projection of separated depths and must
            // never be passed to the ordinary wedge-construction path.
            atom.position = crate::document::Point::default();
            atom.depth = (i + 1) as f32 * 28.;
        }
        for bond in &mut document.bonds {
            bond.stereo_authoritative = bond.order == 2;
            bond.z_order = 7;
        }
        let before = serde_json::to_value(&document)?;
        for (operation, format) in [
            ("analyze", None),
            ("export", Some("smiles")),
            ("export", Some("inchi")),
            ("export", Some("mol")),
            ("export", Some("cdxml")),
        ] {
            let mut request = Request::molecule(operation, document.clone());
            request.format = format.map(str::to_owned);
            let response = execute(request).await?;
            let analysis = response
                .analysis
                .ok_or_else(|| anyhow::anyhow!("Missing projected analysis"))?;
            assert_eq!(analysis.smiles, expected_smiles, "{text}/{format:?}");
            assert_eq!(analysis.inchi, expected_inchi, "{text}/{format:?}");
            assert_eq!(
                serde_json::to_value(response.document)?,
                before,
                "Drawing changed: {text}/{operation}/{format:?}"
            );
            if format == Some("mol") {
                let output = response
                    .output
                    .ok_or_else(|| anyhow::anyhow!("Missing MOL"))?;
                let imported = molfile::read(&output)?;
                assert_eq!(
                    smiles::write::write(&imported.molecule.state, Default::default())?.text,
                    expected_smiles,
                    "{text} MOL identity"
                );
            }
        }
        assert_eq!(serde_json::to_value(&document)?, before);
        if let Some(index) = document
            .bonds
            .iter()
            .position(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
        {
            // Older depth-only wedge paint remains explicitly unsupported in
            // CDXML. New 3D geometry retains the original chemical wedge paint.
            let mut unsupported = document.clone();
            unsupported.bonds[index].projection = true;
            let unsupported_before = serde_json::to_value(&unsupported)?;
            let mut request = Request::molecule("export", unsupported.clone());
            request.format = Some("cdxml".into());
            let error = execute(request)
                .await
                .expect_err("CDXML accepted projected chemical wedge paint");
            assert!(error.to_string().contains("projected wedge styles"));
            assert_eq!(serde_json::to_value(&unsupported)?, unsupported_before);
        }
    }
    Ok(())
}
