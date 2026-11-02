use super::{cdxml, cleanup, document, inchi::kernel, molfile, stereo::cip::label};
use crate::{
    cleanup::Options,
    document::{Document, Point},
    exchange::drawing,
};
use std::fmt::Write;

// BINOL: two naphthyl rings with a single wedge inside the first ring,
// originating at atom 0 on the 0--10 axis (IUPAC ST-1.7).
const POSITIONS: [(f32, f32); 22] = [
    (0., 0.),
    (-0.75, 1.299),
    (-2.25, 1.299),
    (-3., 0.),
    (-2.25, -1.299),
    (-0.75, -1.299),
    (-4.5, 0.),
    (-5.25, 1.299),
    (-4.5, 2.598),
    (-3., 2.598),
    (1.5, 0.),
    (2.25, -1.299),
    (3.75, -1.299),
    (4.5, 0.),
    (3.75, 1.299),
    (2.25, 1.299),
    (6., 0.),
    (6.75, -1.299),
    (6., -2.598),
    (4.5, -2.598),
    (0., 2.598),
    (1.5, -2.598),
];
const BONDS: [(usize, usize, u8); 25] = [
    (0, 1, 1),
    (1, 2, 2),
    (2, 3, 1),
    (3, 4, 2),
    (4, 5, 1),
    (5, 0, 2),
    (3, 6, 1),
    (6, 7, 2),
    (7, 8, 1),
    (8, 9, 2),
    (9, 2, 1),
    (10, 11, 1),
    (11, 12, 2),
    (12, 13, 1),
    (13, 14, 2),
    (14, 15, 1),
    (15, 10, 2),
    (13, 16, 1),
    (16, 17, 2),
    (17, 18, 1),
    (18, 19, 2),
    (19, 12, 1),
    (0, 10, 1),
    (1, 20, 1),
    (11, 21, 1),
];

fn binol(display: &str) -> Document {
    let mut doc = Document::default();
    let ids = POSITIONS
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            doc.add_atom(
                if i < 20 { "C" } else { "O" },
                Point::new(x * 28., -y * 28.),
            )
        })
        .collect::<Vec<_>>();
    for (i, &(a, b, order)) in BONDS.iter().enumerate() {
        doc.add_bond(
            ids[a],
            ids[b],
            order,
            if i == 0 { display } else { "plain" },
        );
    }
    doc
}

fn binol_mol(wedge: u8) -> String {
    let mut output =
        String::from("BINOL\n     RDKit          2D\n\n 22 25  0  0  0  0  0  0  0  0999 V2000\n");
    for (i, &(x, y)) in POSITIONS.iter().enumerate() {
        let symbol = if i < 20 { "C" } else { "O" };
        writeln!(
            output,
            "{x:10.4}{y:10.4}{:10.4} {symbol:<3} 0  0  0  0  0  0  0  0  0  0  0  0",
            0.
        )
        .unwrap();
    }
    for (i, &(a, b, order)) in BONDS.iter().enumerate() {
        writeln!(
            output,
            "{:3}{:3}{order:3}{:3}",
            a + 1,
            b + 1,
            if i == 0 { wedge } else { 0 }
        )
        .unwrap();
    }
    output.push_str("M  END\n");
    output
}

fn axial_code(molecule: &document::Molecule) -> anyhow::Result<String> {
    let labeled = label::assign(&molecule.state, &Default::default())?;
    let axes = labeled
        .state
        .metadata
        .bonds
        .iter()
        .zip(&labeled.state.properties.bond_codes)
        .filter(|(bond, _)| matches!(bond.stereo, 6 | 7))
        .collect::<Vec<_>>();
    assert_eq!(axes.len(), 1);
    let code = axes[0].1.as_deref().unwrap();
    assert!(matches!(code, "M" | "P"), "{code}");
    Ok(code.into())
}

fn assert_visible_axis(doc: &Document, expected: &str) -> anyhow::Result<()> {
    assert!(
        doc.bonds
            .iter()
            .any(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
    );
    let mut visible = doc.clone();
    for atom in &mut visible.atoms {
        atom.stereo = None;
    }
    for bond in &mut visible.bonds {
        bond.stereo = None;
        bond.stereo_atoms.clear();
    }
    assert_eq!(axial_code(&document::prepare(&visible)?)?, expected);
    Ok(())
}

#[test]
fn binol_mol_v2000_and_v3000_retain_axial_wedges_and_cip() -> anyhow::Result<()> {
    let mut codes = Vec::new();
    for (wedge, expected) in [(1, "P"), (6, "M")] {
        let imported = molfile::read(&binol_mol(wedge))?;
        assert_eq!(axial_code(&imported.molecule)?, expected);
        codes.push(expected);
        let drawn = document::prepare(&binol(if wedge == 1 { "wedge" } else { "hash" }))?;
        for molecule in [&imported.molecule, &drawn] {
            for force_v3000 in [false, true] {
                let output = molfile::write(molecule, molfile::Options { force_v3000 })?;
                if force_v3000 {
                    assert!(output.contains("CFG=1") || output.contains("CFG=3"));
                } else {
                    assert!(
                        output.lines().skip(26).take(25).any(|line| {
                            matches!(line.get(9..12).map(str::trim), Some("1" | "6"))
                        })
                    );
                }
                let reopened = molfile::read(&output)?;
                assert_eq!(axial_code(&reopened.molecule)?, expected);
            }
        }
    }
    assert_ne!(codes[0], codes[1]);
    Ok(())
}

#[test]
fn binol_cdxml_round_trip_keeps_visible_axial_configuration() -> anyhow::Result<()> {
    for (display, expected) in [("wedge", "P"), ("hash", "M")] {
        let source = binol(display);
        assert_eq!(axial_code(&document::prepare(&source)?)?, expected);
        let output = drawing::write(&source, Default::default())?;
        assert!(output.contains("WedgeBegin") || output.contains("WedgedHashBegin"));
        let reopened = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&output)?)?.into_document()?;
        assert_eq!(axial_code(&reopened.molecule)?, expected);
        assert_visible_axis(&reopened.document, expected)?;
        let output = drawing::write(&reopened.document, Default::default())?;
        let reopened = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&output)?)?.into_document()?;
        assert_eq!(axial_code(&reopened.molecule)?, expected);
    }
    Ok(())
}

#[test]
fn binol_inchi_generation_accepts_axial_stereo() -> anyhow::Result<()> {
    for wedge in [1, 6] {
        let molecule = molfile::read(&binol_mol(wedge))?.molecule;
        axial_code(&molecule)?;
        let input = kernel::Molecule::prepare(&molecule.state, None)?;
        let result = kernel::generate(&input).map_err(anyhow::Error::msg)?;
        assert!(result.inchi.starts_with("InChI=1S/C20H14O2/"), "{result:?}");
    }
    Ok(())
}

#[tokio::test]
async fn binol_app_analysis_and_smiles_export_keep_axial_drawing() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    let engine = LocalEngine::in_process();
    for (display, expected) in [("wedge", "P"), ("hash", "M")] {
        let imported = engine
            .request(Request::import(
                "mol",
                &binol_mol(if display == "wedge" { 1 } else { 6 }),
            ))
            .await
            .map_err(anyhow::Error::msg)?;
        let document = imported.document.unwrap();
        assert!(
            document
                .bonds
                .iter()
                .any(|bond| bond.cip_label.as_deref() == Some(expected))
        );
        assert_visible_axis(&document, expected)?;
        let source = binol(display);
        for operation in ["analyze", "export"] {
            // Identifier export needs no renderer-provided label metrics.
            let mut request = Request::molecule("analyze", source.clone());
            request.operation = operation.into();
            request.format = (operation == "export").then(|| "smiles".into());
            let response = engine.request(request).await.map_err(anyhow::Error::msg)?;
            let analysis = response.analysis.unwrap();
            assert!(analysis.inchi.starts_with("InChI=1S/C20H14O2/"));
            assert_eq!(analysis.formula, "C20H14O2");
            if operation == "export" {
                assert_eq!(response.output.unwrap().trim(), analysis.smiles);
            }
            assert_visible_axis(response.document.as_ref().unwrap(), expected)?;
        }
    }
    Ok(())
}

#[test]
fn mol_export_still_rejects_non_tetrahedral_atom_stereo() -> anyhow::Result<()> {
    let mut molecule = molfile::read(&binol_mol(0))?.molecule;
    molecule.state.metadata.atoms[0].chiral_tag = 3;
    assert!(matches!(
        molfile::write(&molecule, Default::default()),
        Err(molfile::Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn binol_geometry_reports_unsupported_axial_stereo() -> anyhow::Result<()> {
    let source = binol("hash");
    axial_code(&document::prepare(&source)?)?;
    assert!(matches!(
        crate::geometry::Prepared::new(&source, &[]),
        Err(crate::geometry::Error::Unsupported(_))
    ));
    Ok(())
}

#[test]
fn binol_cleanup_retains_visible_axial_configuration() -> anyhow::Result<()> {
    for (display, expected) in [("wedge", "P"), ("hash", "M")] {
        let source = binol(display);
        assert_eq!(axial_code(&document::prepare(&source)?)?, expected);
        for mirror in [false, true] {
            let cleaned = cleanup::prepare(&source, Options::default(), &[])?.finish_with(
                |request| -> Result<_, cleanup::Error> {
                    let mut points = super::depict::compute(
                        &request.molecule.state,
                        &request
                            .molecule
                            .state
                            .properties
                            .atoms
                            .iter()
                            .map(|p| p.cip_rank)
                            .collect::<Vec<_>>(),
                        Some(request.fixed),
                        super::depict::Options {
                            bond_length: request.bond_length,
                            canonical_orientation: request.canonical_orientation,
                            use_ring_templates: request.use_ring_templates,
                            ..Default::default()
                        },
                    )
                    .map_err(|e| cleanup::Error::Document(e.to_string()))?
                    .positions;
                    if mirror {
                        for point in &mut points {
                            point.x = -point.x;
                        }
                    }
                    Ok(points)
                },
            )?;
            assert_eq!(axial_code(cleaned.molecule.as_ref().unwrap())?, expected);
            assert_visible_axis(&cleaned.document, expected)?;
        }
    }
    Ok(())
}
