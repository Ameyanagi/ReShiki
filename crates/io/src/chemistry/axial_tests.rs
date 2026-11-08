use super::{cdxml, cleanup, document, inchi::kernel, molfile, stereo::cip::label};
use crate::{
    cleanup::{Options, Orientation},
    document::{Document, Point},
    exchange::drawing,
};
use std::fmt::Write;

// [2,2'-Binaphthalene]-1,1'-diol: a single wedge inside the first ring,
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
        // IUPAC fused-ring orientation may also reflect the naphthalene.
        for (orientation, mirror) in [Orientation::Keep, Orientation::Free, Orientation::Iupac]
            .into_iter()
            .flat_map(|orientation| [(orientation, false), (orientation, true)])
        {
            let mut options = Options::default();
            options.set_orientation(orientation);
            let cleaned = cleanup::prepare(&source, options, &[])?.finish_with(
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

// RDKit atropisomer reference geometry/4327 (2D, with and without its crossed
// double bond), geometry/824 (3D) and rings/c1ccccc1/C=C/c1ccccc1/False/34,
// each the agent of water to water (#1037).
#[tokio::test]
async fn rxn_imports_keep_axes_or_refuse_ones_the_drawing_changes() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    let rxn = |dimension: &str, atoms: &str, bonds: &str| {
        let water = "$MOL\n\n     RDKit          2D\n\n  1  0  0  0  0  0  0  0  0  0999 V2000
    0.0000    0.0000    0.0000 O   0  0  0  0  0  0  0  0  0  0  0  0\nM  END\n";
        let counts = format!("{} {}", atoms.lines().count(), bonds.lines().count());
        format!(
            "$RXN\n\n\n\n  1  1  1\n{water}{water}$MOL\n\n                    {dimension}\n
  0  0  0  0  0  0  0  0  0  0999 V3000
M  V30 BEGIN CTAB\nM  V30 COUNTS {counts} 0 0 0\nM  V30 BEGIN ATOM\n{atoms}M  V30 END ATOM
M  V30 BEGIN BOND\n{bonds}M  V30 END BOND\nM  V30 END CTAB\nM  END\n"
        )
    };
    let engine = LocalEngine::in_process();
    let atoms = "M  V30 1 C 2.5 -1 0 0\nM  V30 2 C -1 1 0 0\nM  V30 3 C 0 0 0 0
M  V30 4 C 2.5 1 0 0\nM  V30 5 C -1 -1 0 0\nM  V30 6 C 1.5 0 0 0\n";
    let bonds = |crossed: u8| {
        format!(
            "M  V30 1 1 3 2 CFG=1\nM  V30 2 2 1 6 CFG=0\nM  V30 3 1 6 3 CFG=1
M  V30 4 2 3 5 CFG={crossed}\nM  V30 5 1 4 6 CFG=3\n"
        )
    };
    // Without the crossed double bond the drawn wedges keep the axis,
    // through Clean Up too.
    let flat = rxn("2D", atoms, &bonds(0));
    assert_eq!(
        axial_code(&super::reaction::read_rxn(&flat)?.agents[0].molecule)?,
        "M"
    );
    let imported = engine
        .request(Request::import("rxn", &flat))
        .await
        .map_err(anyhow::Error::msg)?;
    let document = imported.document.unwrap();
    assert_visible_axis(&document, "M")?;
    let cleaned = engine
        .request(Request::molecule("clean", document))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_visible_axis(cleaned.document.as_ref().unwrap(), "M")?;
    // The crossed double bond at the axis is drawn wavy, which hides the axis
    // when the drawing is prepared (#1053), so the reaction is refused.
    let crossed = rxn("2D", atoms, &bonds(2));
    let read = super::reaction::read_rxn(&crossed)?;
    assert_eq!(axial_code(&read.agents[0].molecule)?, "M");
    assert!(read.drawing().is_err());
    // A 3D participant whose XY projection still shows its axis imports.
    let lifted = rxn(
        "3D",
        "M  V30 1 C 2.5 -1 0 0\nM  V30 2 C -1 1 1 0\nM  V30 3 C 0 0 0 0
M  V30 4 C 2.5 1 0 0\nM  V30 5 C -1 -1 0 0\nM  V30 6 C 1.5 0 0 0\n",
        &bonds(0),
    );
    let read = super::reaction::read_rxn(&lifted)?;
    assert!(read.agents[0].annotations.is_3d);
    let code = axial_code(&read.agents[0].molecule)?;
    let imported = engine
        .request(Request::import("rxn", &lifted))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_visible_axis(imported.document.as_ref().unwrap(), &code)?;
    // Both ends' substituents project to one side of the axis in XY.
    let spatial = rxn(
        "3D",
        "M  V30 1 C 0.272056121724 2.5 -1 0\nM  V30 2 C 0.64164810781 -1 -1 0
M  V30 3 C 0.0162909961425 2.5 1 0\nM  V30 4 C 0.469819084052 1.5 0 0
M  V30 5 C 0.94431136875 -1 1 0\nM  V30 6 C 0.561261511328 0 0 0\n",
        "M  V30 1 2 4 1 CFG=2\nM  V30 2 1 6 4 CFG=0\nM  V30 3 1 4 3 CFG=3
M  V30 4 2 6 2 CFG=0\nM  V30 5 1 6 5 CFG=0\n",
    );
    let read = super::reaction::read_rxn(&spatial)?;
    axial_code(&read.agents[0].molecule)?;
    // The RXN reader's drawing step itself refuses, as live comparisons expect.
    assert!(read.drawing().is_err());
    let error = engine
        .request(Request::import("rxn", &spatial))
        .await
        .unwrap_err();
    assert!(
        error.contains("cannot keep this reaction's atropisomer axes"),
        "{error}"
    );
    // A file wedge on a ring double bond lands on a single bond once the ring
    // is drawn in Kekulé form, where it would add a second axis.
    let ring_wedge = rxn(
        "2D",
        "M  V30 1 C -2.45047391041 -1.14353732532 0 0
M  V30 2 C -4.90874998216 -0.302776506132 0 0
M  V30 3 C -1.96506042513 0.275749026448 0 0
M  V30 4 C 3.92231868892 1.43280009161 0 0
M  V30 5 C 2.45047391041 1.14353732532 0 0
M  V30 6 C 2.95149171836 -1.40577261192 0 0
M  V30 7 C -0.493215646616 0.565011792738 0 0
M  V30 8 C 4.42333649688 -1.11650984563 0 0
M  V30 9 C 1.96506042513 -0.275749026448 0 0
M  V30 10 C 4.90874998216 0.302776506132 0 0
M  V30 11 C -4.42333649688 1.11650984563 0 0
M  V30 12 C -2.95149171836 1.40577261192 0 0
M  V30 13 C 0.493215646616 -0.565011792738 0 0
M  V30 14 C -3.92231868892 -1.43280009161 0 0
",
        "M  V30 1 1 12 11 CFG=0
M  V30 2 1 14 2 CFG=0
M  V30 3 1 10 8 CFG=0
M  V30 4 1 13 9 CFG=3
M  V30 5 2 12 3 CFG=0
M  V30 6 1 4 5 CFG=0
M  V30 7 2 2 11 CFG=0
M  V30 8 1 3 1 CFG=3
M  V30 9 2 7 13 CFG=0
M  V30 10 2 1 14 CFG=0
M  V30 11 1 6 9 CFG=0
M  V30 12 2 6 8 CFG=0
M  V30 13 1 7 3 CFG=3
M  V30 14 2 9 5 CFG=3
M  V30 15 2 4 10 CFG=0
",
    );
    let read = super::reaction::read_rxn(&ring_wedge)?;
    assert_eq!(
        read.agents[0]
            .molecule
            .state
            .metadata
            .bonds
            .iter()
            .filter(|b| matches!(b.stereo, 6 | 7))
            .count(),
        1
    );
    assert!(read.drawing().is_err());
    let error = engine
        .request(Request::import("rxn", &ring_wedge))
        .await
        .unwrap_err();
    assert!(
        error.contains("cannot keep this reaction's atropisomer axes"),
        "{error}"
    );
    Ok(())
}

// RDKit atropisomer reference case geometry/1160 as a 3D file: the diene
// C=C(C)-C(C)=C with an M axis on its central bond.
const DIENE_3D: &str = "axial stereo reference
                    3D

  0  0  0  0  0  0  0  0  0  0999 V3000
M  V30 BEGIN CTAB
M  V30 COUNTS 6 5 0 0 0
M  V30 BEGIN ATOM
M  V30 1 C 0.117334601365 -2.5 1 0
M  V30 2 C 0.620467223097 1 -1 0
M  V30 3 C -0.802032661105 -1.5 0 0
M  V30 4 C -0.841446086872 -2.5 -1 0
M  V30 5 C -0.463377035522 1 1 0
M  V30 6 C -0.857724979162 0 0 0
M  V30 END ATOM
M  V30 BEGIN BOND
M  V30 1 2 4 3 CFG=0
M  V30 2 1 3 6 CFG=3
M  V30 3 2 2 6 CFG=3
M  V30 4 1 5 6 CFG=0
M  V30 5 1 3 1 CFG=3
M  V30 END BOND
M  V30 END CTAB
M  END
";

// A 3D drawing is a projection that keeps no wedges, so it cannot keep the
// axis (#1057). 0.14.0 opens the structure and drops the axis: after the
// app's label refresh, nothing shows an M or P.
#[tokio::test]
async fn mol_3d_axis_opens_the_structure_without_an_axis() -> anyhow::Result<()> {
    use crate::{
        atom_labels::refresh::Refresh,
        engine::{LocalEngine, Request},
    };

    assert_eq!(axial_code(&molfile::read(DIENE_3D)?.molecule)?, "M");
    let response = LocalEngine::in_process()
        .request(Request::import("mol", DIENE_3D))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(response.analysis.unwrap().smiles, "C=C(C)C(=C)C");
    let mut doc = response.document.unwrap();
    // The import may still show the reader's own label, never another one.
    assert!(
        doc.bonds
            .iter()
            .all(|bond| matches!(bond.cip_label.as_deref(), None | Some("M")))
    );
    let prepared = document::prepare(&doc)?;
    assert!(
        prepared
            .state
            .metadata
            .bonds
            .iter()
            .all(|bond| !matches!(bond.stereo, 6 | 7))
    );
    Refresh::calculate(&doc, &Default::default())
        .map_err(anyhow::Error::msg)?
        .apply(&mut doc);
    assert!(
        doc.bonds
            .iter()
            .all(|bond| !matches!(bond.cip_label.as_deref(), Some("M" | "P")))
    );
    Ok(())
}
