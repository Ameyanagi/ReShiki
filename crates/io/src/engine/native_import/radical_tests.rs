//! Atoms with three or four radical electrons, as RDKit reads `[C]`,
//! `[CH]`, `[N]` and `[Si]`, through import, export and analysis.
use super::*;

fn imported(format: &str, text: &str) -> anyhow::Result<Box<Prepared>> {
    let Preparation::Complete(prepared) = prepare(Request::import(format, text))? else {
        panic!("{format}: {text}: import deferred");
    };
    prepared.document.validate().map_err(anyhow::Error::msg)?;
    molecular::validate_molecule(&prepared.molecule)?;
    Ok(prepared)
}

#[test]
fn smiles_radicals_import_as_written_and_round_trip() -> anyhow::Result<()> {
    // RDKit's readings: a bracket atom carries only its written hydrogens, so
    // its open valence becomes radical electrons, up to four. CX ^ sets them
    // on an organic-subset atom, which then fills the rest with hydrogens;
    // on a bracket atom the valence deficit wins.
    for (text, radical, electrons, hydrogens, formula) in [
        ("[C]", 0, 4, vec![0], "C"),
        ("[CH]", 0, 3, vec![1], "CH"),
        ("[CH] |^4:0|", 0, 3, vec![1], "CH"),
        ("[C] |^1:0|", 0, 4, vec![0], "C"),
        ("[N]", 0, 3, vec![0], "N"),
        ("[B]", 0, 3, vec![0], "B"),
        ("[Si]", 0, 4, vec![0], "Si"),
        ("[Al]", 0, 3, vec![0], "Al"),
        ("[C+]", 0, 3, vec![0], "C+"),
        ("[C-]", 0, 3, vec![0], "C-"),
        ("[N+]", 0, 4, vec![0], "N+"),
        ("[C:0]-[H]", 0, 3, vec![1], "CH"),
        ("[0C]", 0, 4, vec![0], "C"),
        ("[C+0]", 0, 4, vec![0], "C"),
        ("[C:65536]", 0, 4, vec![0], "C"),
        ("C |^5:0|", 0, 3, vec![1], "CH"),
        ("CCO |^5:0|", 0, 3, vec![0, 2, 1], "C2H3O"),
        ("[C][C]", 0, 3, vec![0, 0], "C2"),
    ] {
        let prepared = imported("smiles", text)?;
        assert!(
            prepared.warnings.is_empty(),
            "{text}: {:?}",
            prepared.warnings
        );
        let atom = &prepared.document.atoms[radical];
        assert_eq!(atom.radical_electrons, electrons, "{text}");
        assert!(!atom.singlet, "{text}");
        assert_eq!(
            prepared.document.file_version(),
            crate::document::RADICAL_ELECTRONS
        );

        let assert_state =
            |name: &str, state: &State, atom_order: &[usize]| -> anyhow::Result<()> {
                let facts = state.graph.atom_facts().map_err(anyhow::Error::msg)?;
                assert_eq!(
                    facts.iter().map(|atom| atom.hydrogens).collect::<Vec<_>>(),
                    atom_order
                        .iter()
                        .map(|&atom| hydrogens[atom])
                        .collect::<Vec<_>>(),
                    "{text}: {name} hydrogens"
                );
                let expected = |atom: usize| {
                    if atom == radical {
                        electrons
                    } else if text == "[C][C]" {
                        3
                    } else {
                        0
                    }
                };
                assert_eq!(
                    facts
                        .iter()
                        .map(|atom| atom.radical_electrons)
                        .collect::<Vec<_>>(),
                    atom_order
                        .iter()
                        .map(|&atom| expected(atom))
                        .collect::<Vec<_>>(),
                    "{text}: {name} radicals"
                );
                assert_eq!(
                    chemistry::properties(&facts)
                        .map_err(anyhow::Error::msg)?
                        .formula,
                    formula,
                    "{text}: {name} formula"
                );
                Ok(())
            };
        let atom_order = (0..hydrogens.len()).collect::<Vec<_>>();
        assert_state("prepared", &prepared.molecule.state, &atom_order)?;
        let readback = molecular::prepare(&prepared.document)?;
        assert_state("reprepared", &readback.state, &atom_order)?;

        let reopened =
            Document::from_native_file(&prepared.document.file_json().map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?;
        assert_eq!(reopened.version, crate::document::RADICAL_ELECTRONS);
        assert_eq!(
            molecular::prepare(&reopened)?.state.graph,
            prepared.molecule.state.graph,
            "{text}: native document round trip"
        );
        let mol = molfile::write_document(&prepared.document)?;
        let from_mol = molfile::read(&mol)?.molecule.state;
        assert_state("MOL round trip", &from_mol, &atom_order)?;
        let smiles = smiles::write::write(&readback.state, Default::default())?;
        assert_state(
            "SMILES round trip",
            &smiles::read(&smiles.text)?.prepared.state,
            &smiles.atom_order,
        )?;
        // SMILES from the MOL reading matches, so SMILES, MOL and the native
        // file all carry the same chemistry. (V2000 truncates a five-digit
        // atom map, which is unrelated.)
        if !text.contains(':') {
            assert_eq!(
                smiles::write::write(&from_mol, Default::default())?.text,
                smiles.text,
                "{text}: SMILES of the MOL round trip"
            );
        }
    }
    for (text, electrons) in [("[CH]>>C |^4:0|", 3), ("[H][C]>>C", 3), ("[C]>>[CH4]", 4)] {
        let prepared = imported("rsmi", text)?;
        assert!(
            prepared.warnings.is_empty(),
            "{text}: {:?}",
            prepared.warnings
        );
        assert_eq!(
            prepared
                .document
                .atoms
                .iter()
                .map(|atom| atom.radical_electrons)
                .max(),
            Some(electrons),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn hypervalent_cx_radicals_match_rdkit_through_export() -> anyhow::Result<()> {
    // CX ^5 on S or P gives three radical electrons beside a higher valence.
    // MOL and SMILES state only the valence, so RDKit and ReShiki both read
    // their export back with one; the native file keeps three.
    for (text, radical, formula) in [
        ("S |^5:0|", 0, "HS"),
        ("PC |^5:0|", 0, "CH4P"),
        ("CS(C)C |^5:1|", 1, "C3H9S"),
    ] {
        let prepared = imported("smiles", text)?;
        assert!(prepared.warnings.is_empty(), "{text}");
        assert_eq!(
            prepared.document.atoms[radical].radical_electrons, 3,
            "{text}"
        );
        let reopened =
            Document::from_native_file(&prepared.document.file_json().map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?;
        assert_eq!(
            molecular::prepare(&reopened)?.state.graph,
            prepared.molecule.state.graph,
            "{text}: native file"
        );
        let mol = molfile::read(&molfile::write_document(&prepared.document)?)?
            .molecule
            .state;
        let smiles = smiles::write::write(&prepared.molecule.state, Default::default())?;
        let reread = smiles::read(&smiles.text)?.prepared.state;
        for (name, state) in [("MOL", &mol), ("SMILES", &reread)] {
            let facts = state.graph.atom_facts().map_err(anyhow::Error::msg)?;
            assert_eq!(
                facts.iter().map(|a| a.radical_electrons).sum::<u8>(),
                1,
                "{text}: {name}"
            );
            assert_eq!(
                chemistry::properties(&facts)
                    .map_err(anyhow::Error::msg)?
                    .formula,
                formula,
                "{text}: {name} formula"
            );
        }
    }
    Ok(())
}

#[test]
fn bracket_radicals_count_bonded_hydrogen_vertices() -> anyhow::Result<()> {
    // RDKit: [H][C] and [2H][C] are CH with three radical electrons, whether
    // the H is a vertex or a count.
    for (format, text) in [("rsmi", "[H][C]>>C"), ("smiles", "[2H][C]")] {
        let prepared = imported(format, text)?;
        assert!(prepared.warnings.is_empty(), "{text}");
        let graph = &prepared.molecule.state.graph;
        let (index, carbon) = graph
            .atoms
            .iter()
            .enumerate()
            .find(|(_, a)| a.radical_electrons != 0)
            .unwrap();
        assert_eq!(carbon.radical_electrons, 3, "{text}");
        let vertices = graph
            .bonds
            .iter()
            .filter(|b| b.a == index || b.b == index)
            .filter(|b| graph.atoms[if b.a == index { b.b } else { b.a }].atomic_number == 1)
            .count();
        assert_eq!(
            usize::from(carbon.explicit_hydrogens) + vertices,
            1,
            "{text}"
        );
    }
    Ok(())
}

/// RDKit 2026.03 MolToMolBlock and MolToV3KMolBlock of each SMILES: a bare
/// atom as valence 15 without RAD, otherwise RAD 2 (odd) or 3 (even) with its
/// valence.
const RDKIT_BLOCKS: &[(&str, &str, &str)] = &[
    (
        "[C]",
        "\n     RDKit          2D\n\n  1  0  0  0  0  0  0  0  0  0999 V2000\n    0.0000    0.0000    0.0000 C   0  0  0  0  0 15  0  0  0  0  0  0\nM  END\n",
        "\n     RDKit          2D\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\nM  V30 BEGIN CTAB\nM  V30 COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C 0.000000 0.000000 0.000000 0 VAL=-1\nM  V30 END ATOM\nM  V30 END CTAB\nM  END\n",
    ),
    (
        "[CH]",
        "\n     RDKit          2D\n\n  1  0  0  0  0  0  0  0  0  0999 V2000\n    0.0000    0.0000    0.0000 C   0  0  0  0  0  1  0  0  0  0  0  0\nM  RAD  1   1   2\nM  END\n",
        "\n     RDKit          2D\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\nM  V30 BEGIN CTAB\nM  V30 COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C 0.000000 0.000000 0.000000 0 RAD=2 VAL=1\nM  V30 END ATOM\nM  V30 END CTAB\nM  END\n",
    ),
    (
        "[Si]",
        "\n     RDKit          2D\n\n  1  0  0  0  0  0  0  0  0  0999 V2000\n    0.0000    0.0000    0.0000 Si  0  0  0  0  0 15  0  0  0  0  0  0\nM  END\n",
        "\n     RDKit          2D\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\nM  V30 BEGIN CTAB\nM  V30 COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 Si 0.000000 0.000000 0.000000 0 VAL=-1\nM  V30 END ATOM\nM  V30 END CTAB\nM  END\n",
    ),
    (
        "[N]",
        "\n     RDKit          2D\n\n  1  0  0  0  0  0  0  0  0  0999 V2000\n    0.0000    0.0000    0.0000 N   0  0  0  0  0 15  0  0  0  0  0  0\nM  END\n",
        "\n     RDKit          2D\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\nM  V30 BEGIN CTAB\nM  V30 COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 N 0.000000 0.000000 0.000000 0 VAL=-1\nM  V30 END ATOM\nM  V30 END CTAB\nM  END\n",
    ),
    (
        "C[C]",
        "\n     RDKit          2D\n\n  2  1  0  0  0  0  0  0  0  0999 V2000\n    0.0000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0\n    1.2990    0.7500    0.0000 C   0  0  0  0  0  1  0  0  0  0  0  0\n  1  2  1  0\nM  RAD  1   2   2\nM  END\n",
        "\n     RDKit          2D\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\nM  V30 BEGIN CTAB\nM  V30 COUNTS 2 1 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C 0.000000 0.000000 0.000000 0\nM  V30 2 C 1.299038 0.750000 0.000000 0 RAD=2 VAL=1\nM  V30 END ATOM\nM  V30 BEGIN BOND\nM  V30 1 1 1 2\nM  V30 END BOND\nM  V30 END CTAB\nM  END\n",
    ),
    (
        "[C]=O",
        "\n     RDKit          2D\n\n  2  1  0  0  0  0  0  0  0  0999 V2000\n    0.0000    0.0000    0.0000 C   0  0  0  0  0  2  0  0  0  0  0  0\n    1.2990    0.7500    0.0000 O   0  0  0  0  0  0  0  0  0  0  0  0\n  1  2  2  0\nM  RAD  1   1   3\nM  END\n",
        "\n     RDKit          2D\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\nM  V30 BEGIN CTAB\nM  V30 COUNTS 2 1 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C 0.000000 0.000000 0.000000 0 RAD=3 VAL=2\nM  V30 2 O 1.299038 0.750000 0.000000 0\nM  V30 END ATOM\nM  V30 BEGIN BOND\nM  V30 1 2 1 2\nM  V30 END BOND\nM  V30 END CTAB\nM  END\n",
    ),
];

/// The atom fields after the coordinates, and the RAD and CHG lines.
fn v2000_fields(block: &str) -> Vec<String> {
    block
        .lines()
        .skip(4)
        .filter_map(|line| {
            if line.starts_with("M  RAD") || line.starts_with("M  CHG") {
                Some(line.to_owned())
            } else {
                line.get(31..)
                    .filter(|_| line.len() == 69)
                    .map(str::to_owned)
            }
        })
        .collect()
}

#[test]
fn rdkit_mol_blocks_for_bracket_radicals_round_trip() -> anyhow::Result<()> {
    for &(smiles_text, v2000, v3000) in RDKIT_BLOCKS {
        let expected = smiles::read(smiles_text)?.prepared.state;
        for block in [v2000, v3000] {
            let prepared = imported("mol", block)?;
            assert!(
                prepared.warnings.is_empty(),
                "{smiles_text}: {:?}",
                prepared.warnings
            );
            assert_eq!(
                prepared.molecule.state.graph, expected.graph,
                "{smiles_text}: MOL reading"
            );
            let written = molfile::write_document(&prepared.document)?;
            assert_eq!(
                v2000_fields(&written),
                v2000_fields(v2000),
                "{smiles_text}: MOL writing"
            );
            assert_eq!(
                molfile::read(&written)?.molecule.state.graph,
                expected.graph,
                "{smiles_text}: MOL round trip"
            );
            assert_eq!(
                smiles::write::write(&prepared.molecule.state, Default::default())?.text,
                smiles::write::write(&expected, Default::default())?.text,
                "{smiles_text}: SMILES"
            );
        }
    }
    Ok(())
}

#[test]
fn cdxml_export_keeps_the_formula_of_three_or_four_radical_electrons() -> anyhow::Result<()> {
    // CDXML has no three- or four-electron radical: an odd count is written
    // as a doublet, an even one as a triplet, with the hydrogen count.
    for (text, radical, hydrogens, formula) in [
        ("[C]", "Triplet", "0", "C"),
        ("[CH]", "Doublet", "1", "CH"),
        ("[N]", "Doublet", "0", "N"),
        ("[Si]", "Triplet", "0", "Si"),
        ("[C+]", "Doublet", "0", "C+"),
        ("C |^5:0|", "Doublet", "1", "CH"),
    ] {
        let prepared = imported("smiles", text)?;
        let document = &prepared.document;
        let request = Request::molecule("export", document.clone());
        let exported = crate::exchange::drawing::write(document, (&request).into())?;
        let tree = roxmltree::Document::parse(&exported)?;
        let atom = tree
            .descendants()
            .find(|node| node.has_tag_name("n"))
            .unwrap();
        assert_eq!(atom.attribute("Radical"), Some(radical), "{text}");
        assert_eq!(atom.attribute("NumHydrogens"), Some(hydrogens), "{text}");
        let reopened = imported("cdxml", &exported)?;
        let facts = reopened
            .molecule
            .state
            .graph
            .atom_facts()
            .map_err(anyhow::Error::msg)?;
        assert_eq!(
            chemistry::properties(&facts)
                .map_err(anyhow::Error::msg)?
                .formula,
            formula,
            "{text}: CDXML round trip"
        );
    }
    Ok(())
}

#[tokio::test]
async fn analysis_of_bracket_radicals_matches_rdkit() -> anyhow::Result<()> {
    // RDKit 2026.03: formula, average mass, InChI and radical electrons.
    for (text, formula, mass, inchi, unpaired) in [
        ("[C]", "C", 12.011, "InChI=1S/C", 4),
        ("[CH]", "CH", 13.019, "InChI=1S/CH/h1H", 3),
        ("[N]", "N", 14.007, "InChI=1S/N", 3),
        ("[B]", "B", 10.812, "InChI=1S/B", 3),
        ("[Si]", "Si", 28.086, "InChI=1S/Si", 4),
        ("[Al]", "Al", 26.982, "InChI=1S/Al", 3),
        ("[C+]", "C+", 12.011, "InChI=1S/C/q+1", 3),
        ("[C][C]", "C2", 24.022, "InChI=1S/C2/c1-2", 6),
    ] {
        let Outcome::Complete(response) = execute(
            Request::import_smiles(text),
            Some(native_response::Config::in_process()),
        )
        .await?
        else {
            panic!("{text}: import deferred");
        };
        assert!(
            response.warnings.is_empty(),
            "{text}: {:?}",
            response.warnings
        );
        let analysis = response.analysis.as_ref().unwrap();
        assert_eq!(analysis.smiles, text, "{text}");
        assert_eq!(analysis.formula, formula, "{text}");
        assert!(
            (analysis.mass - mass).abs() < 5e-4,
            "{text}: {}",
            analysis.mass
        );
        assert_eq!(analysis.inchi, inchi, "{text}");
        assert_eq!(analysis.unpaired_electrons, unpaired, "{text}");
    }
    Ok(())
}

#[test]
fn more_than_four_radical_electrons_are_still_refused() -> anyhow::Result<()> {
    // RDKit reads [LiH3] with four radical electrons and [LiH2] with five.
    // Drawings keep 0-4, so the second is refused, as the frozen worker does.
    let four = imported("smiles", "[LiH3]")?;
    assert!(four.warnings.is_empty());
    assert_eq!(four.document.atoms[0].radical_electrons, 4);
    for format in ["smiles", "naming_smiles"] {
        assert_eq!(
            prepare(Request::import(format, "[LiH2]"))
                .err()
                .unwrap()
                .to_string(),
            "Invalid drawing: Unsupported drawing stereochemistry or radical count",
            "{format}"
        );
    }
    assert_eq!(
        prepare(Request::import("rsmi", "[LiH2]>>[Li]"))
            .err()
            .unwrap()
            .to_string(),
        "SMILES contains unsupported chemistry: reaction stereochemistry or radical count",
    );
    Ok(())
}
