use super::{
    RDKIT_VERSION, cdxml, document,
    kekulize::Direction,
    molfile, smiles,
    stereo::{Point3, cip::label},
};
use crate::{
    document::{Document, Point},
    exchange::drawing,
};
use std::fmt::Write;

// Penta-2,3-diene with both terminal hydrogens explicit. In chemical
// coordinates the left methyl points toward the viewer and the right methyl
// points upward. Looking along the axis left to right, that is M.
const POSITIONS: [(f32, f32); 7] = [
    (-2., 1.),
    (-1., 0.),
    (0., 0.),
    (1., 0.),
    (2., 1.),
    (-2., -1.),
    (2., -1.),
];
const BONDS: [(usize, usize, u8); 6] = [
    (1, 0, 1),
    (1, 2, 2),
    (2, 3, 2),
    (3, 4, 1),
    (1, 5, 1),
    (3, 6, 1),
];

fn allene(display: &str) -> Document {
    let mut doc = Document::default();
    let ids = POSITIONS
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            doc.add_atom(if i < 5 { "C" } else { "H" }, Point::new(x * 28., -y * 28.))
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

fn allene_mol(wedge: u8) -> String {
    let mut output = String::from(
        "Penta-2,3-diene\n     RDKit          2D\n\n  7  6  0  0  0  0  0  0  0  0999 V2000\n",
    );
    for (i, &(x, y)) in POSITIONS.iter().enumerate() {
        let symbol = if i < 5 { "C" } else { "H" };
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

fn allene_code(molecule: &document::Molecule) -> anyhow::Result<String> {
    let labeled = label::assign(&molecule.state, &Default::default())?;
    let centers = labeled
        .state
        .metadata
        .atoms
        .iter()
        .zip(&labeled.state.properties.atoms)
        .filter(|(atom, _)| atom.chiral_tag == 5)
        .collect::<Vec<_>>();
    assert_eq!(centers.len(), 1);
    let code = centers[0].1.cip_code.as_deref().unwrap();
    assert!(matches!(code, "M" | "P"), "{code}");
    Ok(code.into())
}

fn assert_visible_allene(doc: &Document, expected: &str) -> anyhow::Result<()> {
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
    assert_eq!(allene_code(&document::prepare(&visible)?)?, expected);
    Ok(())
}

#[test]
fn allene_mol_v2000_and_v3000_keep_one_ended_wedges_and_cip() -> anyhow::Result<()> {
    for (wedge, expected) in [(1, "M"), (6, "P")] {
        let imported = molfile::read(&allene_mol(wedge))?;
        assert_eq!(allene_code(&imported.molecule)?, expected);
        let drawn = document::prepare(&allene(if wedge == 1 { "wedge" } else { "hash" }))?;
        let mut metadata_only = imported.molecule.clone();
        metadata_only.state.directions.fill(Direction::None);
        for molecule in [&imported.molecule, &drawn, &metadata_only] {
            let canonical = smiles::write::write(&molecule.state, Default::default())?.text;
            for force_v3000 in [false, true] {
                let output = molfile::write(molecule, molfile::Options { force_v3000 })?;
                let reopened = molfile::read(&output)?;
                assert_eq!(allene_code(&reopened.molecule)?, expected);
                assert_eq!(
                    smiles::write::write(&reopened.molecule.state, Default::default())?.text,
                    canonical
                );
                let (drawing, _) = reopened.display_document()?;
                assert_visible_allene(&drawing, expected)?;
                let wedged_ends = drawing
                    .bonds
                    .iter()
                    .filter(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
                    .map(|bond| bond.a)
                    .collect::<std::collections::HashSet<_>>();
                assert_eq!(wedged_ends.len(), 1);
            }
        }
    }
    Ok(())
}

#[test]
fn allene_mol_both_ended_wedges_survive_import_and_warn() -> anyhow::Result<()> {
    let mol = allene_mol(1).replace("  4  5  1  0", "  4  5  1  6");
    let imported = molfile::read(&mol)?;
    assert_eq!(imported.molecule.state.metadata.atoms[2].chiral_tag, 0);
    for bond in [0, 3] {
        assert!(
            matches!(
                imported.molecule.state.directions[bond],
                Direction::Wedge | Direction::Hash
            ),
            "bond={bond}: {:?}",
            imported.molecule.state.directions
        );
    }
    let (drawing, _) = imported.display_document()?;
    assert_eq!(
        document::prepare(&drawing)?.state.metadata.atoms[2].chiral_tag,
        0
    );
    assert!(
        super::stereo_lint::check(&drawing, false)
            .iter()
            .any(|note| note.kind == super::stereo_lint::Kind::AlleneBothEnds)
    );
    Ok(())
}

#[test]
fn allene_cdxml_round_trip_keeps_visible_configuration() -> anyhow::Result<()> {
    for (display, expected) in [("wedge", "M"), ("hash", "P")] {
        let source = allene(display);
        assert_eq!(allene_code(&document::prepare(&source)?)?, expected);
        let output = drawing::write(&source, Default::default())?;
        assert!(output.contains("WedgeBegin") || output.contains("WedgedHashBegin"));
        let reopened = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&output)?)?.into_document()?;
        assert_eq!(allene_code(&reopened.molecule)?, expected);
        assert_visible_allene(&reopened.document, expected)?;
        let output = drawing::write(&reopened.document, Default::default())?;
        let reopened = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&output)?)?.into_document()?;
        assert_eq!(allene_code(&reopened.molecule)?, expected);
    }
    Ok(())
}

#[test]
fn mol_export_rejects_invalid_allene_and_other_atom_stereo() -> anyhow::Result<()> {
    let plain = molfile::read(&allene_mol(0))?.molecule;
    for (center, tag, permutation) in [
        (2, 3, None),
        (2, 4, Some(1)),
        (2, 5, None),
        (2, 5, Some(0)),
        (2, 5, Some(3)),
        (1, 5, Some(1)),
        (2, 6, Some(1)),
        (2, 7, Some(1)),
        (2, 8, Some(1)),
    ] {
        let mut molecule = plain.clone();
        let stereo = &mut molecule.state.metadata.atoms[center];
        stereo.chiral_tag = tag;
        stereo.chiral_permutation = permutation;
        assert!(
            matches!(
                molfile::write(&molecule, Default::default()),
                Err(molfile::Error::Invalid(_))
            ),
            "center={center} tag={tag} permutation={permutation:?}"
        );
    }
    Ok(())
}

#[test]
fn mol_export_still_rejects_longer_cumulene_atom_stereo() -> anyhow::Result<()> {
    let imported = smiles::read("CC=C=C=C=CC")?;
    let mut molecule = document::Molecule {
        rdkit_version: RDKIT_VERSION,
        ids: (1..=7).collect(),
        positions: (0..7)
            .map(|i| Point3 {
                x: i as f64,
                y: 0.,
                z: 0.,
            })
            .collect(),
        state: imported.prepared.state,
    };
    molecule.state.metadata.atoms[3].chiral_tag = 5;
    molecule.state.metadata.atoms[3].chiral_permutation = Some(1);
    assert!(matches!(
        molfile::write(&molecule, Default::default()),
        Err(molfile::Error::Invalid(_))
    ));
    Ok(())
}

#[test]
fn allene_geometry_reports_unsupported_stereo() -> anyhow::Result<()> {
    let source = allene("wedge");
    assert_eq!(allene_code(&document::prepare(&source)?)?, "M");
    assert!(matches!(
        crate::geometry::Prepared::new(&source, &[]),
        Err(crate::geometry::Error::Unsupported(_))
    ));
    Ok(())
}
