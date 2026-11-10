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

fn assert_axis_free(doc: &Document) -> anyhow::Result<()> {
    assert!(
        document::prepare(doc)?
            .state
            .metadata
            .bonds
            .iter()
            .all(|bond| !matches!(bond.stereo, 6 | 7))
    );
    Ok(())
}

fn axial_endpoints(molecule: &document::Molecule) -> Vec<(u64, u64)> {
    molecule
        .state
        .graph
        .bonds
        .iter()
        .zip(&molecule.state.metadata.bonds)
        .filter(|(_, metadata)| matches!(metadata.stereo, 6 | 7))
        .map(|(bond, _)| {
            (
                *molecule.ids.get(bond.a).unwrap(),
                *molecule.ids.get(bond.b).unwrap(),
            )
        })
        .collect()
}

fn assert_dropped_axis_wedge(doc: &Document, a: u64, b: u64) -> anyhow::Result<()> {
    let bond = doc
        .bonds
        .iter()
        .find(|bond| (bond.a, bond.b) == (a, b) || (bond.a, bond.b) == (b, a))
        .unwrap();
    assert_eq!(bond.display, "plain");
    assert!(!bond.projection);
    assert_axis_free(doc)
}

fn assert_no_added_axis_notice(notices: &[String]) {
    assert!(
        notices
            .iter()
            .all(|notice| !notice.starts_with("Removed ") || !notice.contains("atropisomer ax")),
        "{notices:?}"
    );
}

fn assert_no_allene_configuration(doc: &Document) -> anyhow::Result<()> {
    let molecule = document::prepare(doc)?;
    assert!(
        molecule
            .state
            .metadata
            .atoms
            .iter()
            .all(|atom| atom.chiral_tag != 5)
    );
    let labeled = label::assign(&molecule.state, &Default::default())?;
    assert!(
        labeled
            .state
            .properties
            .atoms
            .iter()
            .all(|atom| !matches!(atom.cip_code.as_deref(), Some("M" | "P")))
    );
    Ok(())
}

fn assert_allene_p_at(doc: &Document, id: u64) -> anyhow::Result<()> {
    let molecule = document::prepare(doc)?;
    let center = molecule.ids.iter().position(|&atom| atom == id).unwrap();
    let labeled = label::assign(&molecule.state, &Default::default())?;
    assert_eq!(labeled.state.metadata.atoms[center].chiral_tag, 5);
    assert_eq!(
        labeled.state.properties.atoms[center].cip_code.as_deref(),
        Some("P")
    );
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

fn rxn(dimension: &str, atoms: &str, bonds: &str) -> String {
    let water = "$MOL\n\n     RDKit          2D\n\n  1  0  0  0  0  0  0  0  0  0999 V2000
    0.0000    0.0000    0.0000 O   0  0  0  0  0  0  0  0  0  0  0  0\nM  END\n";
    let counts = format!("{} {}", atoms.lines().count(), bonds.lines().count());
    format!(
        "$RXN\n\n\n\n  1  1  1\n{water}{water}$MOL\n\n                    {dimension}\n
  0  0  0  0  0  0  0  0  0  0999 V3000
M  V30 BEGIN CTAB\nM  V30 COUNTS {counts} 0 0 0\nM  V30 BEGIN ATOM\n{atoms}M  V30 END ATOM
M  V30 BEGIN BOND\n{bonds}M  V30 END BOND\nM  V30 END CTAB\nM  END\n"
    )
}

// RDKit atropisomer geometry/4327 and 3583 (2D, crossed double bonds),
// geometry/824 (3D) and rings/c1ccccc1/C=C/c1ccccc1/False/34,
// each the agent of water to water (#1037, #1064).
#[tokio::test]
async fn rxn_imports_keep_axes_or_refuse_ones_the_drawing_changes() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    let engine = LocalEngine::in_process();
    let atoms = "M  V30 1 C 2.5 -1 0 0\nM  V30 2 C -1 1 0 0\nM  V30 3 C 0 0 0 0
M  V30 4 C 2.5 1 0 0\nM  V30 5 C -1 -1 0 0\nM  V30 6 C 1.5 0 0 0\n";
    let bonds = |crossed: u8| {
        format!(
            "M  V30 1 1 3 2 CFG=1\nM  V30 2 2 1 6 CFG=0\nM  V30 3 1 6 3 CFG=1
M  V30 4 2 3 5 CFG={crossed}\nM  V30 5 1 4 6 CFG=3\n"
        )
    };
    // The axis survives with and without the crossed double bond.
    let flat = rxn("2D", atoms, &bonds(0));
    let crossed = rxn("2D", atoms, &bonds(2));
    let both_crossed = rxn(
        "2D",
        "M  V30 1 C -1 1 0 0\nM  V30 2 C 1.5 0 0 0\nM  V30 3 C -1 -1 0 0
M  V30 4 C 0 0 0 0\nM  V30 5 C 2.5 1 0 0\nM  V30 6 C 2.5 -1 0 0\n",
        "M  V30 1 2 3 4 CFG=2\nM  V30 2 1 2 5 CFG=3\nM  V30 3 1 4 2 CFG=2
M  V30 4 2 6 2 CFG=2\nM  V30 5 1 1 4 CFG=1\n",
    );
    for (name, text) in [("flat", &flat), ("4327", &crossed), ("3583", &both_crossed)] {
        let read = super::reaction::read_rxn(text)?;
        assert_eq!(axial_code(&read.agents[0].molecule)?, "M");
        let drawing = read.drawing()?;
        let labels = drawing.labels()?;
        assert_visible_axis(&drawing.finish(labels)?, "M")?;
        let imported = engine
            .request(Request::import("rxn", text))
            .await
            .map_err(anyhow::Error::msg)?;
        let document = imported.document.unwrap();
        assert_visible_axis(&document, "M")?;
        let cleaned = engine
            .request(Request::molecule("clean", document))
            .await
            .map_err(|error| anyhow::anyhow!("Clean Up {name}: {error}"))?;
        assert_visible_axis(cleaned.document.as_ref().unwrap(), "M")?;
    }
    // The same 4327 participant imported as a 2D MOL must keep its axis on
    // the next preparation, not just in the reader's cached labels.
    let mol = crossed.rsplit_once("$MOL\n").unwrap().1;
    assert_eq!(axial_code(&molfile::read(mol)?.molecule)?, "M");
    let imported = engine
        .request(Request::import("mol", mol))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_visible_axis(imported.document.as_ref().unwrap(), "M")?;
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

#[tokio::test]
async fn rxn_github7669_keeps_its_biaryl_axis() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    // RDKit Code/GraphMol/ChemReactions/testData/github7669_1.rxn,
    // BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
    let text = include_str!("../../../../tests/fixtures/atropisomer/github7669_1.rxn");
    let read = super::reaction::read_rxn(text)?;
    assert_eq!(axial_code(&read.products[0].molecule)?, "M");
    let drawing = read.drawing()?;
    let labels = drawing.labels()?;
    assert_visible_axis(&drawing.finish(labels)?, "M")?;
    let engine = LocalEngine::in_process();
    let imported = engine
        .request(Request::import("rxn", text))
        .await
        .map_err(anyhow::Error::msg)?;
    let document = imported.document.unwrap();
    assert_visible_axis(&document, "M")?;
    let cleaned = engine
        .request(Request::molecule("clean", document))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_visible_axis(cleaned.document.as_ref().unwrap(), "M")?;
    Ok(())
}

#[tokio::test]
async fn rxn_drops_a_3d_allene_terminal_file_wedge_that_adds_an_axis() -> anyhow::Result<()> {
    use super::kekulize::Direction;
    use crate::engine::{LocalEngine, Request};

    // #1064's audit counterexample: all Z=1, so the file has no 3D axis.
    let text = rxn(
        "3D",
        "M  V30 1 C 0 0 1 0\nM  V30 2 C 1.5 0 1 0\nM  V30 3 C -1 1 1 0
M  V30 4 C -2 2 1 0\nM  V30 5 C -1 -1 1 0\nM  V30 6 C 2.5 1 1 0
M  V30 7 C 2.5 -1 1 0\nM  V30 8 C -3 1 1 0\n",
        "M  V30 1 1 1 2\nM  V30 2 2 1 3\nM  V30 3 2 3 4\nM  V30 4 1 1 5 CFG=1
M  V30 5 2 2 6 CFG=2\nM  V30 6 1 2 7\nM  V30 7 1 4 8\n",
    );
    let read = super::reaction::read_rxn(&text)?;
    let agent = &read.agents[0];
    assert!(agent.annotations.is_3d);
    assert!(
        agent
            .molecule
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    assert!(
        agent
            .molecule
            .state
            .metadata
            .atoms
            .iter()
            .all(|a| a.chiral_tag == 0)
    );
    // The allene-terminal exception retains the authored 1--5 wedge.
    // With no atom or bond configuration the chooser adds no other wedge.
    assert_eq!(agent.molecule.state.directions[3], Direction::Wedge);
    let drawing = document::for_import(&agent.molecule, true, &agent.annotations.dummy_labels)?;
    let labels = drawing.labels()?;
    let drawn = drawing.finish(labels)?;
    let wedges: Vec<_> = drawn
        .bonds
        .iter()
        .filter(|b| matches!(b.display.as_str(), "wedge" | "hash"))
        .map(|b| (b.a, b.b, b.display.as_str()))
        .collect();
    assert_eq!(wedges, [(1, 5, "wedge")]);
    let prepared = document::prepare(&drawn)?;
    assert_eq!(axial_code(&prepared)?, "P");
    assert!(matches!(prepared.state.metadata.bonds[0].stereo, 6 | 7));
    let drawing = read.drawing()?;
    // The agent follows the water reactant, so use its remapped canvas IDs.
    let (agent, _) = drawing.participants().nth(1).unwrap();
    let wedge = (agent.ids[0], agent.ids[4]);
    let labels = drawing.labels()?;
    let (drawn, notices) = drawing.finish_with_notices(labels)?;
    assert_eq!(notices, [super::ADDED_AXIS_WEDGE_NOTICE]);
    assert_dropped_axis_wedge(&drawn, wedge.0, wedge.1)?;
    assert_no_allene_configuration(&drawn)?;
    let engine = LocalEngine::in_process();
    let imported = engine
        .request(Request::import("rxn", &text))
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(
        imported
            .warnings
            .iter()
            .any(|notice| notice == super::ADDED_AXIS_WEDGE_NOTICE),
        "{:?}",
        imported.warnings
    );
    let doc = imported.document.unwrap();
    for doc in [drawn, doc] {
        assert_dropped_axis_wedge(&doc, wedge.0, wedge.1)?;
        assert_no_allene_configuration(&doc)?;
        let cleaned = engine
            .request(Request::molecule("clean", doc))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = cleaned.document.as_ref().unwrap();
        assert_dropped_axis_wedge(doc, wedge.0, wedge.1)?;
        assert_no_allene_configuration(doc)?;
    }
    // The same axis-free 3D agent without the file wedge still imports.
    let unwedged = text.replace("1 1 5 CFG=1", "1 1 5 CFG=0");
    let read = super::reaction::read_rxn(&unwedged)?;
    let drawing = read.drawing()?;
    let labels = drawing.labels()?;
    let (doc, notices) = drawing.finish_with_notices(labels)?;
    assert_no_added_axis_notice(&notices);
    let prepared = document::prepare(&doc)?;
    assert!(
        prepared
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    let imported = engine
        .request(Request::import("rxn", &unwedged))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_no_added_axis_notice(&imported.warnings);
    let prepared = document::prepare(imported.document.as_ref().unwrap())?;
    assert!(
        prepared
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    // Without a crossed double, the restored file wedge remains accepted.
    let plain = text.replace("2 2 6 CFG=2", "2 2 6 CFG=0");
    let read = super::reaction::read_rxn(&plain)?;
    assert!(
        read.agents[0]
            .molecule
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    let drawing = read.drawing()?;
    let labels = drawing.labels()?;
    let (doc, notices) = drawing.finish_with_notices(labels)?;
    assert_no_added_axis_notice(&notices);
    assert!(doc.bonds.iter().all(|b| b.display != "wavy"));
    assert_eq!(axial_code(&document::prepare(&doc)?)?, "P");
    let imported = engine
        .request(Request::import("rxn", &plain))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_no_added_axis_notice(&imported.warnings);
    assert_eq!(
        axial_code(&document::prepare(imported.document.as_ref().unwrap())?)?,
        "P"
    );
    Ok(())
}

#[tokio::test]
async fn rxn_axis_free_participants_do_not_require_drawing_preparation() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    for element in ["C", "R"] {
        let text = rxn("2D", &format!("M  V30 1 {element} 0 0 0 0\n"), "");
        let read = super::reaction::read_rxn(&text)?;
        assert!(read.agents[0].molecule.state.metadata.bonds.is_empty());
        let drawing = read.drawing()?;
        let labels = drawing.labels()?;
        let (document, notices) = drawing.finish_with_notices(labels)?;
        assert_no_added_axis_notice(&notices);
        assert!(document.bonds.is_empty());
        if element == "R" {
            assert!(document::prepare(&document).is_err());
        } else {
            assert!(
                document::prepare(&document)?
                    .state
                    .metadata
                    .bonds
                    .is_empty()
            );
            let imported = LocalEngine::in_process()
                .request(Request::import("rxn", &text))
                .await
                .map_err(anyhow::Error::msg)?;
            assert_no_added_axis_notice(&imported.warnings);
            assert!(imported.document.unwrap().bonds.is_empty());
        }
    }
    Ok(())
}

#[tokio::test]
async fn cdxml_drops_a_3d_allene_terminal_file_wedge_that_adds_an_axis() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    // #1064's review reproducer: p and xyz associate at twice the RXN scale,
    // with every Z equal. The file wedge selects an axis but supplies no depth.
    let mut xml = String::from("<CDXML BondLength='14.4'><page><fragment id='100'>");
    for (i, (x, y)) in [
        (0., 0.),
        (1.5, 0.),
        (-1., 1.),
        (-2., 2.),
        (-1., -1.),
        (2.5, 1.),
        (2.5, -1.),
        (-3., 1.),
    ]
    .into_iter()
    .enumerate()
    {
        write!(
            xml,
            "<n id='{}' p='{} {}' xyz='{} {} {}'/>",
            i + 1,
            19.2 * x,
            -19.2 * y,
            2. * x / 65536.,
            -2. * y / 65536.,
            2. / 65536.
        )?;
    }
    xml.push_str(
        "<b id='11' B='1' E='2'/><b id='12' B='1' E='3' Order='2'/>
<b id='13' B='3' E='4' Order='2'/><b id='14' B='1' E='5' Display='WedgeBegin'/>
<b id='15' B='2' E='6' Order='2' Display='Wavy'/><b id='16' B='2' E='7'/>
<b id='17' B='4' E='8'/></fragment></page></CDXML>",
    );
    let scene = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&xml)?)?;
    assert_eq!(scene.conformer_3d, Some(true));
    assert!(
        scene
            .molecule
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    let restored = scene.clone().into_unchecked_drawing()?;
    assert!(
        restored
            .bonds
            .iter()
            .any(|b| (b.a, b.b) == (1, 5) && b.display == "wedge" && !b.projection)
    );
    assert!(
        restored
            .bonds
            .iter()
            .any(|b| (b.a, b.b) == (2, 6) && b.display == "wavy" && !b.projection)
    );
    let prepared = document::prepare(&restored)?;
    assert_eq!(axial_code(&prepared)?, "P");
    assert!(matches!(prepared.state.metadata.bonds[0].stereo, 6 | 7));
    let imported = scene.into_document()?;
    assert_eq!(imported.notices, [super::ADDED_AXIS_WEDGE_NOTICE]);
    assert_dropped_axis_wedge(&imported.document, 1, 5)?;
    assert_no_allene_configuration(&imported.document)?;
    let engine = LocalEngine::in_process();
    let response = engine
        .request(Request::import("cdxml", &xml))
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(
        response
            .warnings
            .iter()
            .any(|notice| notice == super::ADDED_AXIS_WEDGE_NOTICE),
        "{:?}",
        response.warnings
    );
    let doc = response.document.unwrap();
    for doc in [imported.document, doc] {
        assert_dropped_axis_wedge(&doc, 1, 5)?;
        assert_no_allene_configuration(&doc)?;
        let cleaned = engine
            .request(Request::molecule("clean", doc))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = cleaned.document.as_ref().unwrap();
        assert_dropped_axis_wedge(doc, 1, 5)?;
        assert_no_allene_configuration(doc)?;
    }
    // With no restored wedge the same axis-free 3D file remains importable.
    let imported = cdxml::import_cdxml(&xml.replace(" Display='WedgeBegin'", ""))?;
    assert_no_added_axis_notice(&imported.notices);
    assert!(
        document::prepare(&imported.document)?
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    let unwedged = xml.replace(" Display='WedgeBegin'", "");
    let response = engine
        .request(Request::import("cdxml", &unwedged))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_no_added_axis_notice(&response.warnings);
    assert_axis_free(response.document.as_ref().unwrap())?;
    // Without the wavy double, the restored wedge remains accepted.
    let plain = xml.replace(" Display='Wavy'", "");
    let imported = cdxml::import_cdxml(&plain)?;
    assert_no_added_axis_notice(&imported.notices);
    assert!(
        imported
            .molecule
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    assert!(imported.document.bonds.iter().all(|b| b.display != "wavy"));
    assert_eq!(axial_code(&document::prepare(&imported.document)?)?, "P");
    let response = engine
        .request(Request::import("cdxml", &plain))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_no_added_axis_notice(&response.warnings);
    assert_eq!(
        axial_code(&document::prepare(response.document.as_ref().unwrap())?)?,
        "P"
    );
    Ok(())
}

#[tokio::test]
async fn cxsmiles_drops_wedges_that_add_axes_beside_wavy_doubles() -> anyhow::Result<()> {
    use super::kekulize::Direction;
    use crate::engine::{LocalEngine, Request};

    let engine = LocalEngine::in_process();
    for text in [
        "C(C(=C)C)(=C=CC)C |wU:0.6,w:1.1|",
        "C(C(=C)C)(=C=CC)C |(0,0,1;1.5,0,1;2.5,1,1;2.5,-1,1;-1,1,1;-2,2,1;-3,1,1;-1,-1,1),wU:0.6,w:1.1|",
    ] {
        let read = super::smiles::read(text)?;
        let state = &read.prepared.state;
        assert!(
            state
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7))
        );
        assert_eq!(state.directions[1], Direction::Unknown);
        assert_eq!(state.directions[6], Direction::Wedge);
        if let Some(conformer) = read.conformers.first() {
            assert!(state.metadata.atoms.iter().all(|a| a.chiral_tag == 0));
            let molecule = document::Molecule {
                rdkit_version: super::RDKIT_VERSION,
                ids: (1..=state.graph.atoms.len() as u64).collect(),
                positions: conformer.positions.clone(),
                state: state.clone(),
            };
            let drawing = document::for_generated(
                &molecule,
                &read.prepared.dummy_labels,
                Default::default(),
            )?;
            let labels = drawing.labels()?;
            let mut doc = drawing.finish(labels)?;
            assert_eq!(axial_code(&document::prepare(&doc)?)?, "P");
            assert_eq!(
                super::drop_added_axis_wedges(&mut doc, &molecule).as_deref(),
                Some(super::ADDED_AXIS_WEDGE_NOTICE)
            );
            assert_dropped_axis_wedge(&doc, 1, 8)?;
            assert_no_allene_configuration(&doc)?;
            let cleaned = engine
                .request(Request::molecule("clean", doc))
                .await
                .map_err(anyhow::Error::msg)?;
            let doc = cleaned.document.as_ref().unwrap();
            assert_dropped_axis_wedge(doc, 1, 8)?;
            assert_no_allene_configuration(doc)?;
        }
        let response = engine
            .request(Request::import("smiles", text))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = response.document.unwrap();
        assert_axis_free(&doc)?;
        // Coordinate-free import redraws wedges; the flat 3D conformer keeps the file wedge.
        if read.conformers.is_empty() {
            assert_no_added_axis_notice(&response.warnings);
        } else {
            assert!(
                response
                    .warnings
                    .iter()
                    .any(|notice| notice == super::ADDED_AXIS_WEDGE_NOTICE),
                "{:?}",
                response.warnings
            );
        }
        assert_dropped_axis_wedge(&doc, 1, 8)?;
        assert_no_allene_configuration(&doc)?;
        let cleaned = engine
            .request(Request::molecule("clean", doc))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = cleaned.document.as_ref().unwrap();
        assert_axis_free(doc)?;
        assert_dropped_axis_wedge(doc, 1, 8)?;
        assert_no_allene_configuration(doc)?;
        let unwedged = text.replace("wU:0.6,", "");
        let response = engine
            .request(Request::import("smiles", &unwedged))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_no_added_axis_notice(&response.warnings);
        assert!(
            document::prepare(response.document.as_ref().unwrap())?
                .state
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7))
        );
    }
    // A 2D conformer clears the input wedge before relayout, so it stays axis-free.
    let planar =
        "C(C(=C)C)(=C=CC)C |(0,0,;1.5,0,;2.5,1,;2.5,-1,;-1,1,;-2,2,;-3,1,;-1,-1,),wU:0.6,w:1.1|";
    let response = engine
        .request(Request::import("smiles", planar))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_no_added_axis_notice(&response.warnings);
    assert!(
        document::prepare(response.document.as_ref().unwrap())?
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    let response = engine
        .request(Request::import("smiles", "* |atomProp:0.dummyLabel.R1|"))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_no_added_axis_notice(&response.warnings);
    let doc = response.document.as_ref().unwrap();
    assert!(doc.bonds.is_empty());
    assert!(document::prepare(doc).is_err());
    Ok(())
}

#[tokio::test]
async fn cdxml_import_removes_wedges_from_axes_exposed_by_another_removal() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    // MOL clears these non-allene file wedges before drawing. Flat 3D CDXML
    // retains them: equal Z coordinates leave the source axis-free while p/xyz
    // restore the supplied XY geometry and both authored wedges.
    let mut xml = String::from("<CDXML BondLength='14.4'><page><fragment id='100'>");
    for (i, (element, x, y)) in [
        (6, 0., 0.),
        (6, 0., 1.),
        (7, 1., 0.),
        (6, -1., 1.),
        (6, 1., 2.),
        (6, -1., 2.),
        (6, 2., -1.),
        (6, 2., 1.),
        (8, 3., 0.),
    ]
    .into_iter()
    .enumerate()
    {
        write!(
            xml,
            "<n id='{}' Element='{element}' p='{} {}' xyz='{} {} {}'/>",
            i + 1,
            19.2 * x,
            -19.2 * y,
            2. * x / 65536.,
            -2. * y / 65536.,
            2. / 65536.
        )?;
    }
    xml.push_str(
        "<b id='11' B='1' E='2'/><b id='12' B='1' E='3' Display='WedgeBegin'/>
<b id='13' B='1' E='4' Order='2'/><b id='14' B='2' E='5' Order='2' Display='Wavy'/>
<b id='15' B='2' E='6'/><b id='16' B='3' E='7'/>
<b id='17' B='3' E='8' Display='WedgeBegin'/><b id='18' B='7' E='9' Order='2'/>
</fragment></page></CDXML>",
    );
    let scene = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&xml)?)?;
    assert_eq!(scene.conformer_3d, Some(true));
    assert!(axial_endpoints(&scene.molecule).is_empty());
    let before_removal = scene.clone().into_unchecked_drawing()?;
    assert_eq!(
        axial_endpoints(&document::prepare(&before_removal)?),
        [(1, 2)]
    );
    let mut after_first_removal = before_removal;
    after_first_removal
        .bonds
        .iter_mut()
        .find(|bond| (bond.a, bond.b) == (1, 3))
        .unwrap()
        .display = "plain".into();
    // The wedge on 1--3 masked the neighbouring 3--7 candidate. Removing
    // only the first axis's wedge exposes a second, previously absent axis.
    assert_eq!(
        axial_endpoints(&document::prepare(&after_first_removal)?),
        [(3, 7)]
    );

    let expected_notice =
        "Removed 2 wedges that would have added atropisomer axes the file does not define";
    let imported = scene.into_document()?;
    assert!(axial_endpoints(&imported.molecule).is_empty());
    assert_eq!(imported.notices, [expected_notice]);
    let engine = LocalEngine::in_process();
    let response = engine
        .request(Request::import("cdxml", &xml))
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(
        response
            .warnings
            .iter()
            .any(|notice| notice == expected_notice),
        "{:?}",
        response.warnings
    );
    for doc in [imported.document, response.document.unwrap()] {
        assert_dropped_axis_wedge(&doc, 1, 3)?;
        assert_dropped_axis_wedge(&doc, 3, 8)?;
        let cleaned = engine
            .request(Request::molecule("clean", doc))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = cleaned.document.as_ref().unwrap();
        assert_dropped_axis_wedge(doc, 1, 3)?;
        assert_dropped_axis_wedge(doc, 3, 8)?;
    }
    Ok(())
}

#[tokio::test]
async fn cdxml_import_rechecks_axes_after_removing_an_invented_allene_wedge() -> anyhow::Result<()>
{
    use crate::engine::{LocalEngine, Request};

    // Flat 3D CDXML retains the authored wedges in the drawing while its
    // conformer defines neither an axis nor an allene configuration.
    // The allene is 1=3=4. Its 4->8 wedge and the same-display 8->9 wedge
    // initially mask the 8--10 axis through the three-coordinate amide N.
    let mut xml = String::from("<CDXML BondLength='14.4'><page><fragment id='100'>");
    for (i, (element, x, y)) in [
        (6, 0., 0.),
        (6, 1.5, 0.),
        (6, -1., 1.),
        (6, -2., 2.),
        (6, -1., -1.),
        (6, 2.5, 1.),
        (6, 2.5, -1.),
        (7, -3., 1.),
        (6, -4., -1.),
        (6, -3., 0.),
        (8, -2., -1.),
    ]
    .into_iter()
    .enumerate()
    {
        write!(
            xml,
            "<n id='{}' Element='{element}' p='{} {}' xyz='{} {} {}'/>",
            i + 1,
            19.2 * x,
            -19.2 * y,
            2. * x / 65536.,
            -2. * y / 65536.,
            2. / 65536.
        )?;
    }
    xml.push_str(
        "<b id='12' B='1' E='2'/><b id='13' B='1' E='3' Order='2'/>
<b id='14' B='3' E='4' Order='2'/><b id='15' B='1' E='5' Display='WedgeBegin'/>
<b id='16' B='2' E='6' Order='2' Display='Wavy'/><b id='17' B='2' E='7'/>
<b id='18' B='4' E='8' Display='WedgeBegin'/>
<b id='19' B='8' E='9' Display='WedgeBegin'/><b id='20' B='8' E='10'/>
<b id='21' B='10' E='11' Order='2'/></fragment></page></CDXML>",
    );
    let scene = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&xml)?)?;
    assert_eq!(scene.conformer_3d, Some(true));
    assert!(axial_endpoints(&scene.molecule).is_empty());
    let source = label::assign(&scene.molecule.state, &Default::default())?;
    assert!(
        source
            .state
            .metadata
            .atoms
            .iter()
            .all(|atom| atom.chiral_tag != 5)
    );
    assert!(
        source
            .state
            .properties
            .atoms
            .iter()
            .all(|atom| !matches!(atom.cip_code.as_deref(), Some("M" | "P")))
    );
    let before_removal = scene.clone().into_unchecked_drawing()?;
    assert_eq!(
        axial_endpoints(&document::prepare(&before_removal)?),
        [(1, 2)]
    );
    assert_no_allene_configuration(&before_removal)?;

    let mut after_axis_removal = before_removal;
    after_axis_removal
        .bonds
        .iter_mut()
        .find(|bond| (bond.a, bond.b) == (1, 5))
        .unwrap()
        .display = "plain".into();
    let molecule = document::prepare(&after_axis_removal)?;
    assert!(axial_endpoints(&molecule).is_empty());
    let allene_center = molecule.ids.iter().position(|&id| id == 3).unwrap();
    let labeled = label::assign(&molecule.state, &Default::default())?;
    assert_eq!(
        labeled
            .state
            .metadata
            .atoms
            .get(allene_center)
            .unwrap()
            .chiral_tag,
        5
    );
    assert!(matches!(
        labeled
            .state
            .properties
            .atoms
            .get(allene_center)
            .unwrap()
            .cip_code
            .as_deref(),
        Some("M" | "P")
    ));
    // The axis pass now appears finished, but removing the invented allene's
    // other wedge unmasks the neighbouring amide axis at three-coordinate N.
    after_axis_removal
        .bonds
        .iter_mut()
        .find(|bond| (bond.a, bond.b) == (4, 8))
        .unwrap()
        .display = "plain".into();
    assert_eq!(
        axial_endpoints(&document::prepare(&after_axis_removal)?),
        [(8, 10)]
    );
    assert_no_allene_configuration(&after_axis_removal)?;

    let expected_notice = "Removed 2 wedges that would have added atropisomer axes the file does not define; the allene's other wedge was removed too, so it stays unspecified as in the file";
    let imported = scene.into_document()?;
    assert!(axial_endpoints(&imported.molecule).is_empty());
    assert_eq!(imported.notices, [expected_notice]);
    let engine = LocalEngine::in_process();
    let response = engine
        .request(Request::import("cdxml", &xml))
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(
        response
            .warnings
            .iter()
            .any(|notice| notice == expected_notice),
        "{:?}",
        response.warnings
    );
    for doc in [imported.document, response.document.unwrap()] {
        assert_dropped_axis_wedge(&doc, 1, 5)?;
        assert_dropped_axis_wedge(&doc, 4, 8)?;
        assert_dropped_axis_wedge(&doc, 8, 9)?;
        assert_no_allene_configuration(&doc)?;
        let cleaned = engine
            .request(Request::molecule("clean", doc))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = cleaned.document.as_ref().unwrap();
        assert_dropped_axis_wedge(doc, 1, 5)?;
        assert_dropped_axis_wedge(doc, 4, 8)?;
        assert_dropped_axis_wedge(doc, 8, 9)?;
        assert_no_allene_configuration(doc)?;
    }
    Ok(())
}

#[tokio::test]
async fn mol_import_keeps_both_ended_allene_unspecified_after_removing_an_axis()
-> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    let engine = LocalEngine::in_process();
    for other_wedge in [1, 6] {
        let mut text = String::from(
            "Both-ended allene\n     RDKit          2D\n\n  8  7  0  0  0  0  0  0  0  0999 V2000\n",
        );
        for (x, y) in [
            (0., 0.),
            (1.5, 0.),
            (-1., 1.),
            (-2., 2.),
            (-1., -1.),
            (2.5, 1.),
            (2.5, -1.),
            (-3., 1.),
        ] {
            writeln!(
                text,
                "{x:10.4}{y:10.4}{:10.4} C   0  0  0  0  0  0  0  0  0  0  0  0",
                0.
            )?;
        }
        write!(
            text,
            "  1  2  1  0\n  1  3  2  0\n  3  4  2  0\n  1  5  1  1\n  2  6  2  4\n  2  7  1  0\n  4  8  1  {other_wedge}\nM  END\n"
        )?;
        let read = molfile::read(&text)?;
        assert!(axial_endpoints(&read.molecule).is_empty());
        assert!(
            read.molecule
                .state
                .metadata
                .atoms
                .iter()
                .all(|atom| atom.chiral_tag != 5)
        );
        let drawing = read.drawing()?;
        let labels = drawing.labels()?;
        let before_removal = drawing.finish(labels)?;
        assert_eq!(
            axial_endpoints(&document::prepare(&before_removal)?),
            [(1, 2)]
        );
        assert_no_allene_configuration(&before_removal)?;
        let expected_notice = "Removed a wedge that would have added an atropisomer axis the file does not define; the allene's other wedge was removed too, so it stays unspecified as in the file";
        let (drawing, molecule, notices) = read.display_document()?;
        assert!(axial_endpoints(&molecule).is_empty());
        assert!(
            molecule
                .state
                .metadata
                .atoms
                .iter()
                .all(|atom| atom.chiral_tag != 5)
        );
        assert_eq!(notices, [expected_notice]);
        let response = engine
            .request(Request::import("mol", &text))
            .await
            .map_err(anyhow::Error::msg)?;
        assert!(
            response
                .warnings
                .iter()
                .any(|notice| notice == expected_notice),
            "{:?}",
            response.warnings
        );
        for doc in [drawing, response.document.unwrap()] {
            assert_dropped_axis_wedge(&doc, 1, 5)?;
            assert_dropped_axis_wedge(&doc, 4, 8)?;
            assert_no_allene_configuration(&doc)?;
            let cleaned = engine
                .request(Request::molecule("clean", doc))
                .await
                .map_err(anyhow::Error::msg)?;
            let doc = cleaned.document.as_ref().unwrap();
            assert_dropped_axis_wedge(doc, 1, 5)?;
            assert_dropped_axis_wedge(doc, 4, 8)?;
            assert_no_allene_configuration(doc)?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn mol_and_sdf_drop_added_axis_wedges_only_for_non_projection_file_wedges()
-> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request, text_format};

    let engine = LocalEngine::in_process();
    for (dimension, z, crossed) in [("2D", 0., 4), ("2D", 0., 3), ("3D", 1., 4), ("3D", 1., 3)] {
        let mut text = format!(
            "\n     RDKit          {dimension}\n\n  8  7  0  0  0  0  0  0  0  0999 V2000\n"
        );
        for (x, y) in [
            (0., 0.),
            (1.5, 0.),
            (-1., 1.),
            (-2., 2.),
            (-1., -1.),
            (2.5, 1.),
            (2.5, -1.),
            (-3., 1.),
        ] {
            writeln!(
                text,
                "{x:10.4}{y:10.4}{z:10.4} C   0  0  0  0  0  0  0  0  0  0  0  0"
            )?;
        }
        write!(
            text,
            "  1  2  1  0\n  1  3  2  0\n  3  4  2  0\n  1  5  1  1\n  2  6  2  {crossed}\n  2  7  1  0\n  4  8  1  0\nM  END\n"
        )?;
        for suffix in ["", ">  <Name>\naxis audit\n\n$$$$\n"] {
            let source = format!("{text}{suffix}");
            let read = molfile::read(&source)?;
            if dimension == "2D" && crossed == 3 {
                assert_eq!(axial_code(&read.molecule)?, "P");
            } else {
                assert!(
                    read.molecule
                        .state
                        .metadata
                        .bonds
                        .iter()
                        .all(|b| !matches!(b.stereo, 6 | 7))
                );
            }
            let format = text_format(&source);
            assert_eq!(format, "mol");
            if dimension == "2D" && crossed == 4 {
                // The reader accepts code 4 on a double as Unknown. Flattened
                // file wedges would then create an atropisomer axis beside it.
                let labeled = label::assign(&read.molecule.state, &Default::default())?;
                let center = read.molecule.ids.iter().position(|&id| id == 3).unwrap();
                assert_eq!(labeled.state.metadata.atoms[center].chiral_tag, 5);
                assert_eq!(
                    labeled.state.properties.atoms[center].cip_code.as_deref(),
                    Some("P")
                );
                let drawing = read.drawing()?;
                let labels = drawing.labels()?;
                let mut unrecoverable = drawing.finish(labels)?;
                assert_eq!(axial_code(&document::prepare(&unrecoverable)?)?, "P");
                // A styled bond at the other terminal leaves no plain bond
                // for rewedging. Dropping the configuration also clears its label.
                unrecoverable
                    .bonds
                    .iter_mut()
                    .find(|bond| (bond.a, bond.b) == (4, 8))
                    .unwrap()
                    .display = "wavy".into();
                unrecoverable.atom_mut(3).unwrap().cip_label = Some("P".into());
                assert_eq!(
                    super::drop_added_axis_wedges(&mut unrecoverable, &read.molecule),
                    Some(format!(
                        "{}; the allene's (P) configuration could not be kept",
                        super::ADDED_AXIS_WEDGE_NOTICE
                    ))
                );
                assert_dropped_axis_wedge(&unrecoverable, 1, 5)?;
                assert_no_allene_configuration(&unrecoverable)?;
                assert_eq!(unrecoverable.atom(3).unwrap().cip_label, None);
                let (drawing, _, notices) = read.display_document()?;
                let expected_notice = format!(
                    "{}; the allene's (P) configuration is now wedged at its other end",
                    super::ADDED_AXIS_WEDGE_NOTICE
                );
                assert_eq!(notices.as_slice(), std::slice::from_ref(&expected_notice));
                let response = engine
                    .request(Request::import(format, &source))
                    .await
                    .map_err(anyhow::Error::msg)?;
                assert!(
                    response.warnings.contains(&expected_notice),
                    "{:?}",
                    response.warnings
                );
                let imported = response.document.unwrap();
                for doc in [&drawing, &imported] {
                    assert_dropped_axis_wedge(doc, 1, 5)?;
                    assert_allene_p_at(doc, 3)?;
                    assert!(doc.bonds.iter().any(|bond| {
                        (bond.a, bond.b) == (4, 8)
                            && matches!(bond.display.as_str(), "wedge" | "hash")
                            && !bond.projection
                    }));
                    let cleaned = engine
                        .request(Request::molecule("clean", doc.clone()))
                        .await
                        .map_err(anyhow::Error::msg)?;
                    let doc = cleaned.document.as_ref().unwrap();
                    assert_dropped_axis_wedge(doc, 1, 5)?;
                    assert_allene_p_at(doc, 3)?;
                }
            } else {
                let (drawing, _, notices) = read.display_document()?;
                assert_no_added_axis_notice(&notices);
                let response = engine
                    .request(Request::import(format, &source))
                    .await
                    .map_err(anyhow::Error::msg)?;
                assert_no_added_axis_notice(&response.warnings);
                for doc in [&drawing, response.document.as_ref().unwrap()] {
                    let prepared = document::prepare(doc)?;
                    if dimension == "2D" {
                        assert_eq!(axial_code(&prepared)?, "P");
                    } else {
                        assert!(doc.bonds.iter().all(|b| b.projection));
                        assert!(
                            prepared
                                .state
                                .metadata
                                .bonds
                                .iter()
                                .all(|b| !matches!(b.stereo, 6 | 7))
                        );
                        assert_no_allene_configuration(doc)?;
                        let cleaned = engine
                            .request(Request::molecule("clean", doc.clone()))
                            .await
                            .map_err(anyhow::Error::msg)?;
                        let doc = cleaned.document.as_ref().unwrap();
                        assert_axis_free(doc)?;
                        assert_no_allene_configuration(doc)?;
                    }
                }
            }
        }
    }
    // A named dummy with no axis retains main's unpreparable drawing.
    let text = rxn("2D", "M  V30 1 R 0 0 0 0\n", "");
    let mol = text.rsplit("$MOL\n").next().unwrap();
    for suffix in ["", ">  <Name>\naxis-free dummy\n\n$$$$\n"] {
        let source = format!("{mol}{suffix}");
        let (doc, molecule, notices) = molfile::read(&source)?.display_document()?;
        assert_no_added_axis_notice(&notices);
        assert!(molecule.state.metadata.bonds.is_empty());
        assert!(doc.bonds.is_empty());
        assert!(document::prepare(&doc).is_err());
        let response = engine
            .request(Request::import("mol", &source))
            .await
            .map_err(anyhow::Error::msg)?;
        assert_no_added_axis_notice(&response.warnings);
        let doc = response.document.as_ref().unwrap();
        assert!(doc.bonds.is_empty());
        assert!(document::prepare(doc).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn cdxml_axis_free_crossed_double_keeps_its_2d_drawing() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    let xml = "<CDXML BondLength='30'><page><fragment id='100'>
<n id='1' p='0 0'/><n id='2' p='30 0'/><n id='3' p='60 -15'/><n id='4' p='90 -15'/>
<b id='11' B='1' E='2'/><b id='12' B='2' E='3' Order='2' Display='Wavy'/>
<b id='13' B='3' E='4'/></fragment></page></CDXML>";
    let scene = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(xml)?)?;
    assert_eq!(scene.conformer_3d, Some(false));
    assert!(
        scene
            .molecule
            .state
            .metadata
            .bonds
            .iter()
            .all(|b| !matches!(b.stereo, 6 | 7))
    );
    let imported = scene.into_document()?;
    let response = LocalEngine::in_process()
        .request(Request::import("cdxml", xml))
        .await
        .map_err(anyhow::Error::msg)?;
    for doc in [&imported.document, response.document.as_ref().unwrap()] {
        assert_eq!(
            doc.atoms
                .iter()
                .map(|a| (a.id, a.position.x, a.position.y))
                .collect::<Vec<_>>(),
            [
                (1, 0., 0.),
                (2, 87.5, 0.),
                (3, 175., -43.75),
                (4, 262.5, -43.75)
            ]
        );
        assert_eq!(
            doc.bonds
                .iter()
                .map(|b| (b.a, b.b, b.order, b.display.as_str(), b.projection))
                .collect::<Vec<_>>(),
            [
                (1, 2, 1, "plain", false),
                (2, 3, 2, "wavy", false),
                (3, 4, 1, "plain", false)
            ]
        );
        assert!(
            document::prepare(doc)?
                .state
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7))
        );
    }
    // Generic atoms still import when preparation fails, with or without a wavy double.
    for source in [xml.to_owned(), xml.replace(" Display='Wavy'", "")] {
        let ordinary = cdxml::import_cdxml(&source)?;
        let generic = cdxml::import_cdxml(&source.replace(
            "id='1' p='0 0'",
            "id='1' p='0 0' NodeType='ElementList' ElementList='6 7'",
        ))?;
        assert!(
            generic
                .molecule
                .state
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7))
        );
        assert!(document::prepare(&generic.document).is_err());
        // The element list changes CIP priorities, so compare the drawn bonds only.
        let drawn = |doc: &crate::document::Document| {
            doc.bonds
                .iter()
                .map(|b| (b.a, b.b, b.order, b.display.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(drawn(&generic.document), drawn(&ordinary.document));
    }
    Ok(())
}

#[tokio::test]
async fn crossed_double_2d_imports_without_wedges_remain_axis_free() -> anyhow::Result<()> {
    use crate::engine::{LocalEngine, Request};

    let text = rxn(
        "2D",
        "M  V30 1 C 0 0 0 0\nM  V30 2 C 1.5 0 0 0\nM  V30 3 C 3 1 0 0\nM  V30 4 C 4.5 1 0 0\n",
        "M  V30 1 1 1 2\nM  V30 2 2 2 3 CFG=2\nM  V30 3 1 3 4\n",
    );
    let mol = text.rsplit("$MOL\n").next().unwrap();
    let check = |doc: &Document| -> anyhow::Result<()> {
        assert!(
            doc.bonds
                .iter()
                .any(|b| b.order == 2 && b.display == "wavy" && !b.projection)
        );
        assert!(
            doc.bonds
                .iter()
                .all(|b| !matches!(b.display.as_str(), "wedge" | "hash"))
        );
        assert!(
            document::prepare(doc)?
                .state
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7))
        );
        Ok(())
    };
    let drawing = super::reaction::read_rxn(&text)?.drawing()?;
    let labels = drawing.labels()?;
    check(&drawing.finish(labels)?)?;
    for suffix in ["", ">  <Name>\nunwedged alkene\n\n$$$$\n"] {
        check(
            &molfile::read(&format!("{mol}{suffix}"))?
                .display_document()?
                .0,
        )?;
    }
    let sdf = format!("{mol}>  <Name>\nunwedged alkene\n\n$$$$\n");
    let engine = LocalEngine::in_process();
    for (format, source) in [
        ("rxn", text.as_str()),
        ("mol", mol),
        ("mol", sdf.as_str()),
        ("smiles", "CC=CC |(0,0,;1.5,0,;3,1,;4.5,1,),w:1.1|"),
    ] {
        let response = engine
            .request(Request::import(format, source))
            .await
            .map_err(anyhow::Error::msg)?;
        check(response.document.as_ref().unwrap())?;
    }
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
