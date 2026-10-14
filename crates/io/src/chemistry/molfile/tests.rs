use super::*;
use crate::{
    allocation_metrics,
    attachments::{Attachment, Kind},
};
use std::{collections::HashSet, hint::black_box, time::Instant};

const CPP7: &str = include_str!("../../../../../tests/fixtures/mol/cpp7.mol");
const CPP7_TRIMMED: &str = include_str!("../../../../../tests/fixtures/mol/cpp7-trimmed.mol");

fn assert_same_import(standard: &Imported, text: &str, case: &str) -> anyhow::Result<Imported> {
    let imported = read(text)?;
    assert_eq!(
        serde_json::to_value(&imported)?,
        serde_json::to_value(standard)?,
        "{case}"
    );
    assert_eq!(
        imported.annotations.chemdraw_directions, standard.annotations.chemdraw_directions,
        "{case}"
    );
    assert_eq!(
        serde_json::to_value(imported.clone().display_document()?)?,
        serde_json::to_value(standard.clone().display_document()?)?,
        "{case}"
    );
    Ok(imported)
}

fn v2000_atom(x: f64, y: f64, symbol: &str) -> String {
    format!(
        "{x:10.4}{y:10.4}{:10.4} {symbol:<3} 0  0  0  0  0  0  0  0  0  0  0  0",
        0.
    )
}

fn v2000_chain(atoms: usize) -> String {
    let bonds = atoms - 1;
    let mut lines = vec![
        String::new(),
        "     RDKit          2D".into(),
        String::new(),
        format!("{atoms:3}{bonds:3}  0  0  0  0  0  0  0  0999 V2000"),
    ];
    lines.extend((0..atoms).map(|i| v2000_atom(i as f64, 0., "C")));
    lines.extend((1..atoms).map(|i| format!("{i:3}{:3}  1  0", i + 1)));
    lines.push("M  END".into());
    lines.join("\n")
}

#[test]
fn pasted_cpp7_preserves_the_complete_import_and_formula() -> anyhow::Result<()> {
    let standard = read(CPP7)?;
    assert_eq!(standard.molecule.state.graph.atoms.len(), 42);
    assert_eq!(standard.molecule.state.graph.bonds.len(), 49);
    let facts = standard
        .molecule
        .state
        .graph
        .atom_facts()
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        crate::chemistry::properties(&facts)
            .map_err(anyhow::Error::msg)?
            .formula,
        "C42H28"
    );
    let counts_only = CPP7
        .lines()
        .enumerate()
        .map(|(i, line)| if i == 3 { line.trim_start() } else { line })
        .collect::<Vec<_>>()
        .join("\n");
    let bonds_only = CPP7
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if (46..95).contains(&i) {
                line.trim_start()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let all_trimmed = CPP7
        .lines()
        .map(|line| format!("{}   ", line.trim_start()))
        .collect::<Vec<_>>()
        .join("\n");
    for (case, text) in [
        ("owner paste", CPP7_TRIMMED),
        ("counts only", counts_only.as_str()),
        ("bonds only", bonds_only.as_str()),
        ("all lines with trailing spaces", all_trimmed.as_str()),
    ] {
        assert_same_import(&standard, text, case)?;
    }
    Ok(())
}

#[test]
fn trimmed_two_digit_wedge_records_preserve_stereochemistry() -> anyhow::Result<()> {
    for code in [1, 6] {
        let text = v2000_chain(12);
        let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
        lines[14] = v2000_atom(10., 1., "F");
        lines[15] = v2000_atom(10., -1., "Cl");
        lines[25] = format!(" 10 11  1{code:3}");
        lines[26] = " 10 12  1  0".into();
        let standard = read(&lines.join("\n"))?;
        assert_ne!(standard.molecule.state.metadata.atoms[9].chiral_tag, 0);
        lines[25] = lines[25].trim_start().into();
        assert_same_import(&standard, &lines.join("\n"), "two-digit wedge")?;
    }
    Ok(())
}

#[test]
fn run_together_three_digit_bonds_keep_fixed_column_endpoints() -> anyhow::Result<()> {
    let text = v2000_chain(101);
    assert!(text.contains("\n100101  1  0\n"));
    let imported = read(&text)?;
    let graph = &imported.molecule.state.graph;
    assert_eq!(graph.atoms.len(), 101);
    assert_eq!(graph.bonds.len(), 100);
    assert_eq!((graph.bonds[99].a, graph.bonds[99].b), (99, 100));
    // ` 99100` that lost its leading space still means atoms 99 and 100.
    let trimmed = text.replace("\n 99100  1  0\n", "\n99100  1  0\n");
    assert_ne!(trimmed, text);
    assert_same_import(&imported, &trimmed, "trimmed mixed-width bond")?;
    Ok(())
}

#[test]
fn trimmed_mixed_width_counts_keep_fixed_column_counts() -> anyhow::Result<()> {
    let text = v2000_chain(99)
        .replace(" 99 98  0", " 99100  0")
        .replace("M  END", "  1 50  1  0\n  1 99  1  0\nM  END");
    let standard = read(&text)?;
    assert_eq!(standard.molecule.state.graph.atoms.len(), 99);
    assert_eq!(standard.molecule.state.graph.bonds.len(), 100);
    let trimmed = text.replace("\n 99100  0", "\n99100  0");
    assert_ne!(trimmed, text);
    assert_same_import(&standard, &trimmed, "trimmed mixed-width counts")?;
    Ok(())
}

#[test]
fn trimmed_atom_records_keep_blank_fields_in_their_columns() -> anyhow::Result<()> {
    let text = v2000_chain(3);
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    // Blank mass difference before the charge, blank fields before the
    // valence and before the atom map.
    lines[6] = format!(
        "{:10.4}{:10.4}{:10.4} N      3{:9}  4{:9}  7",
        2., 0., 0., "", ""
    );
    let standard = read(&lines.join("\n"))?;
    let atom = &standard.molecule.state.graph.atoms[2];
    assert_eq!((atom.atomic_number, atom.isotope, atom.charge), (7, 0, 1));
    assert!(atom.no_implicit);
    assert_eq!(atom.explicit_hydrogens, 3);
    assert_eq!(standard.molecule.state.metadata.atoms[2].map_number, 7);
    for lost in [1, 4] {
        let mut trimmed = lines.clone();
        trimmed[6] = lines[6][lost..].into();
        assert_same_import(&standard, &trimmed.join("\n"), &format!("lost {lost}"))?;
    }
    Ok(())
}

#[test]
fn odd_spaced_charge_properties_preserve_atom_three_charge() -> anyhow::Result<()> {
    let text = v2000_chain(3).replace("M  END", "M  CHG  1   3   1\nM  END");
    let standard = read(&text)?;
    assert_eq!(standard.molecule.state.graph.atoms[2].charge, 1);
    assert_same_import(
        &standard,
        &text.replace("M  CHG  1   3   1", "M  CHG  1  3  1"),
        "odd-spaced charge",
    )?;
    Ok(())
}

#[test]
fn trimmed_atom_fields_preserve_mass_charge_valence_and_map() -> anyhow::Result<()> {
    let text = v2000_chain(3);
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    lines[6] = format!(
        "{:10.4}{:10.4}{:10.4} N   1  3  0  0  0  4  0  0  0  7  0  0",
        2., 0., 0.
    );
    let standard = read(&lines.join("\n"))?;
    let atom = &standard.molecule.state.graph.atoms[2];
    assert_eq!((atom.atomic_number, atom.isotope, atom.charge), (7, 15, 1));
    assert!(atom.no_implicit);
    assert_eq!(atom.explicit_hydrogens, 3);
    assert_eq!(standard.molecule.state.metadata.atoms[2].map_number, 7);
    lines[6] = lines[6].trim_start().into();
    assert_same_import(&standard, &lines.join("\n"), "atom fields")?;
    for malformed in [
        "NaN 0 0 N 1 3 0 0 0 4 0 0 0 7 0 0",
        "1e101 0 0 N 1 3 0 0 0 4 0 0 0 7 0 0",
        "2 0 0 N invalid 3 0 0 0 4 0 0 0 7 0 0",
    ] {
        lines[6] = malformed.into();
        assert!(read(&lines.join("\n")).is_err(), "{malformed}");
    }
    Ok(())
}

#[test]
fn trimmed_headers_keep_planar_3d_and_chemdraw_annotations() -> anyhow::Result<()> {
    for (info, is_3d) in [
        ("     RDKit          3D", true),
        ("  ChemDraw          2D    1.00000     0.00000", false),
        ("  ChemDraw10132610152D    1.00000     0.00000", false),
    ] {
        let mut lines = CPP7.lines().map(str::to_owned).collect::<Vec<_>>();
        lines[1] = info.into();
        let standard = read(&lines.join("\n"))?;
        assert_eq!(standard.annotations.is_3d, is_3d);
        if info.contains("ChemDraw") {
            assert_eq!(standard.annotations.chemdraw_directions.len(), 49);
        }
        lines[1] = format!("{}   ", info.trim_start());
        assert_same_import(&standard, &lines.join("\n"), info)?;
    }
    Ok(())
}

#[test]
fn unrecoverable_lines_keep_original_fixed_column_errors() {
    let mut lines = CPP7.lines().map(str::to_owned).collect::<Vec<_>>();
    lines[6].truncate(20);
    assert_eq!(
        read(&lines.join("\n")).unwrap_err().to_string(),
        "Invalid MOL input at line 7: Truncated or non-ASCII field"
    );
    lines[3] = "garbage counts".into();
    assert_eq!(
        read(&lines.join("\n")).unwrap_err().to_string(),
        "Invalid MOL input at line 4: Invalid integer \"gar\""
    );
}

#[test]
fn v3000_trimmed_headers_and_trailing_spaces_preserve_continuations() -> anyhow::Result<()> {
    let text = concat!(
        "\n     RDKit          2D\n\n",
        "  0  0  0  0  0  0  0  0  0  0999 V3000\n",
        "M  V30 BEGIN CTAB\nM  V30 COUNTS 2 1 0 0 0\nM  V30 BEGIN ATOM\n",
        "M  V30 1 C 0 0 -\nM  V30 0 0\nM  V30 2 O 1.5 0 0 0\n",
        "M  V30 END ATOM\nM  V30 BEGIN BOND\nM  V30 1 1 1 2\n",
        "M  V30 END BOND\nM  V30 END CTAB\nM  END\n",
    );
    let standard = read(text)?;
    let trimmed = text
        .lines()
        .map(|line| format!("{}   ", line.trim_start()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(trimmed.contains("-   \nM  V30 0 0"));
    assert_same_import(&standard, &trimmed, "V3000 continuation")?;
    Ok(())
}

fn molecule() -> Molecule {
    let mut state = crate::chemistry::smiles::prepare("*C.C.C.C").unwrap().state;
    state.rings.kind = RingKind::Symmetric;
    Molecule {
        rdkit_version: RDKIT_VERSION,
        ids: vec![9_007_199_254_740_993, 81, 12, 99, 73],
        positions: vec![crate::chemistry::stereo::Point3::default(); 5],
        state,
    }
}

#[test]
fn attachment_indices_keep_member_order_and_missing_target_errors() {
    let molecule = molecule();
    for (kind, mode) in [(Kind::MultiCenter, "ALL"), (Kind::Variable, "ANY")] {
        let attachment = Attachment {
            id: molecule.ids[0],
            kind,
            members: vec![73, 99, 12],
        };
        let block = write_part(
            &molecule,
            Options { force_v3000: true },
            true,
            std::slice::from_ref(&attachment),
            false,
            false,
        )
        .unwrap();
        assert!(
            block.contains(&format!("ENDPTS=(3 5 4 3) ATTACH={mode}")),
            "{block}"
        );
        let missing = Attachment {
            members: vec![99, 404],
            ..attachment
        };
        assert_eq!(
            write_part(
                &molecule,
                Options { force_v3000: true },
                true,
                &[missing],
                false,
                false
            )
            .unwrap_err()
            .to_string(),
            "Invalid MOL output: Missing MOL attachment target"
        );
    }
    let mut duplicate = molecule.clone();
    duplicate.ids[2] = duplicate.ids[1];
    assert_eq!(
        write(&duplicate, Options::default())
            .unwrap_err()
            .to_string(),
        "Invalid MOL output: Molecule dimensions or annotations changed"
    );
}

#[test]
fn projected_molecular_files_preserve_stereo_when_the_drawing_is_edge_on() -> anyhow::Result<()> {
    use crate::chemistry::{inchi::kernel, smiles, stereo::Point3};
    for text in [
        "F/C=C/F",
        "F/C=C\\F",
        "FC=CF",
        "F/C=C/C=C/Cl",
        "F[C@](Cl)(Br)I",
        "F[C@@](Cl)(Br)I",
        "FC(Cl)(Br)I",
        "F[C@H](Cl)[C@@H](Br)I",
        "C[C@H]1CCC[C@@H](C)C1",
    ] {
        let mut state = smiles::prepare(text)?.state;
        state.rings.kind = RingKind::Symmetric;
        let count = state.graph.atoms.len();
        let expected = smiles::write::write(&state, Default::default())?.text;
        let expected_inchi = kernel::generate(&kernel::Molecule::prepare(&state, None)?)
            .map_err(anyhow::Error::msg)?
            .inchi;
        for slope in [0., 1., -1.] {
            let molecule = Molecule {
                rdkit_version: RDKIT_VERSION,
                ids: (1..=count as u64).collect(),
                positions: (0..count)
                    .map(|i| Point3 {
                        x: i as f64,
                        y: slope * i as f64,
                        z: 0.,
                    })
                    .collect(),
                state: state.clone(),
            };
            let before = serde_json::to_value(&molecule)?;
            for force_v3000 in [false, true] {
                let block = write_projected_absolute(&molecule, Options { force_v3000 })?;
                let imported = read(&block)?;
                assert_eq!(
                    smiles::write::write(&imported.molecule.state, Default::default())?.text,
                    expected,
                    "{text}, slope={slope}, V3000={force_v3000}"
                );
                let actual_inchi =
                    kernel::generate(&kernel::Molecule::prepare(&imported.molecule.state, None)?)
                        .map_err(anyhow::Error::msg)?
                        .inchi;
                assert_eq!(actual_inchi, expected_inchi, "{text}");
                assert_eq!(serde_json::to_value(&molecule)?, before);
            }
        }
    }
    Ok(())
}

#[test]
fn projected_mol_preserves_explicit_unknown_double_bonds() -> anyhow::Result<()> {
    let mut molecule = molecule();
    molecule.state = crate::chemistry::smiles::prepare("FC=CF")?.state;
    molecule.state.rings.kind = RingKind::Symmetric;
    molecule.ids.truncate(4);
    molecule.positions.truncate(4);
    let metadata = molecule
        .state
        .metadata
        .bonds
        .get_mut(1)
        .ok_or_else(|| invalid("Missing test double bond"))?;
    metadata.stereo = 1;
    metadata.stereo_atoms.clear();
    for force_v3000 in [false, true] {
        let imported = read(&write_projected_absolute(
            &molecule,
            Options { force_v3000 },
        )?)?;
        assert_eq!(imported.molecule.state.metadata.bonds[1].stereo, 1);
    }
    Ok(())
}

#[test]
fn haworth_drawing_export_keeps_existing_2d_bytes_and_stereo() -> anyhow::Result<()> {
    use crate::haworth::{Anomer, Sugar, sugar_document};
    for sugar in [
        Sugar::Glucose,
        Sugar::Galactose,
        Sugar::Mannose,
        Sugar::Ribose,
        Sugar::Fructose,
    ] {
        for anomer in [Anomer::Alpha, Anomer::Beta] {
            let doc = sugar_document(sugar, anomer, 42.).map_err(anyhow::Error::msg)?;
            assert!(doc.atoms.iter().any(|atom| atom.depth.abs() > 0.028));
            let molecule = document::prepare(&doc)?;
            let expected_smiles =
                crate::chemistry::smiles::write::write(&molecule.state, Default::default())?.text;
            let before = doc.clone();
            for force_v3000 in [false, true] {
                let options = Options { force_v3000 };
                let expected = write_projected_absolute(&molecule, options)?;
                let (output, fallback) = write_drawing_absolute(&molecule, &doc, options)?;
                assert_eq!(output, expected, "{sugar:?}/{anomer:?}/{force_v3000}");
                assert!(!fallback, "Haworth perspective is expected 2D output");
                assert_eq!(output.lines().nth(1), Some("     RDKit          2D"));
                if force_v3000 {
                    for line in output
                        .lines()
                        .skip_while(|line| *line != "M  V30 BEGIN ATOM")
                        .skip(1)
                        .take(molecule.ids.len())
                    {
                        assert_eq!(line.split_whitespace().nth(6), Some("0.000000"));
                    }
                } else {
                    for line in output.lines().skip(4).take(molecule.ids.len()) {
                        assert_eq!(line.get(20..30), Some("    0.0000"));
                    }
                    assert_eq!(output, write_document(&doc)?);
                }
                let imported = read(&output)?;
                assert!(!imported.annotations.is_3d);
                assert!(imported.molecule.positions.iter().all(|p| p.z == 0.));
                assert_eq!(
                    crate::chemistry::smiles::write::write(
                        &imported.molecule.state,
                        Default::default()
                    )?
                    .text,
                    expected_smiles,
                    "{sugar:?}/{anomer:?}/{force_v3000}"
                );
                assert_eq!(doc, before);
            }
        }
    }
    Ok(())
}

#[test]
fn haworth_scaffold_export_keeps_existing_2d_bytes() -> anyhow::Result<()> {
    use crate::haworth::Ring;
    for ring in [Ring::Five, Ring::Six] {
        for oxygen in [false, true] {
            let doc = ring.document(42., oxygen);
            let molecule = document::prepare(&doc)?;
            for force_v3000 in [false, true] {
                let options = Options { force_v3000 };
                let (output, fallback) = write_drawing_absolute(&molecule, &doc, options)?;
                assert_eq!(output, write_projected_absolute(&molecule, options)?);
                assert_eq!(output.lines().nth(1), Some("     RDKit          2D"));
                assert!(read(&output)?.molecule.positions.iter().all(|p| p.z == 0.));
                assert!(!fallback);
            }
        }
    }
    Ok(())
}

#[test]
fn haworth_export_only_excludes_drawings_with_recognized_exported_atoms() -> anyhow::Result<()> {
    use crate::{
        document::Point,
        haworth::{Anomer, Sugar, sugar_document},
    };
    let mut doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
    // Even when only the unrelated component has depth, the exported Haworth
    // atoms require the same 2D depiction of the whole molecule as before.
    for atom in &mut doc.atoms {
        atom.depth = 0.;
    }
    let carbon = doc.add_atom("C", Point::new(300., 0.));
    let oxygen = doc.add_atom("O", Point::new(342., 28.));
    doc.add_bond(carbon, oxygen, 1, "plain");
    doc.atom_mut(oxygen).unwrap().depth = 28.;
    let molecule = document::prepare(&doc)?;
    for force_v3000 in [false, true] {
        let options = Options { force_v3000 };
        let (output, fallback) = write_drawing_absolute(&molecule, &doc, options)?;
        assert_eq!(output, write_projected_absolute(&molecule, options)?);
        assert!(!fallback);

        // A separate export that contains no recognized atoms keeps its 3D.
        let other = document::prepare(&crate::editing::selection(&doc, &[carbon, oxygen]))?;
        let (output, fallback) = write_drawing_absolute(&other, &doc, options)?;
        assert_eq!(output.lines().nth(1), Some("     RDKit          3D"));
        assert!(!fallback);
        assert_eq!(read(&output)?.molecule.positions[1].z, 1.);

        // Recognition also covers a direct substituent exported on its own.
        let substituent = doc.atoms[1].stereo.as_ref().unwrap().neighbors[2];
        doc.atom_mut(substituent).unwrap().depth = 28.;
        let branch = document::prepare(&crate::editing::selection(&doc, &[substituent]))?;
        let (output, fallback) = write_drawing_absolute(&branch, &doc, options)?;
        assert_eq!(output, write_projected_absolute(&branch, options)?);
        assert_eq!(output.lines().nth(1), Some("     RDKit          2D"));
        assert!(!fallback);
        doc.atom_mut(substituent).unwrap().depth = 0.;
    }
    Ok(())
}

#[test]
fn drawing_export_keeps_existing_2d_bytes_below_the_depth_threshold() -> anyhow::Result<()> {
    use crate::document::{Document, Point};
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 28.));
    doc.add_bond(a, b, 1, "plain");
    for projected in [false, true] {
        doc.bonds[0].projection = projected;
        for depth in [0., 0.0279, -0.0279] {
            doc.atom_mut(a).unwrap().depth = depth;
            let molecule = document::prepare(&doc)?;
            for force_v3000 in [false, true] {
                let options = Options { force_v3000 };
                let expected = if has_projected_coordinates(&doc) {
                    write_projected_absolute(&molecule, options)?
                } else {
                    write_absolute(&molecule, options)?
                };
                let before = doc.clone();
                let (output, fallback) = write_drawing_absolute(&molecule, &doc, options)?;
                assert_eq!(output, expected);
                assert!(!fallback);
                assert_eq!(output.lines().nth(1), Some("     RDKit          2D"));
                assert_eq!(doc, before);
            }
        }
    }
    for depth in [0.0281, -0.0281] {
        doc.atom_mut(a).unwrap().depth = depth;
        let molecule = document::prepare(&doc)?;
        for force_v3000 in [false, true] {
            let (output, fallback) =
                write_drawing_absolute(&molecule, &doc, Options { force_v3000 })?;
            assert!(!fallback);
            assert_eq!(output.lines().nth(1), Some("     RDKit          3D"));
        }
    }
    doc.atom_mut(a).unwrap().depth = 0.;
    let molecule = document::prepare(&doc)?;
    let outside = doc.add_atom("N", Point::new(200., 0.));
    doc.atom_mut(outside).unwrap().depth = 28.;
    let (output, fallback) = write_drawing_absolute(&molecule, &doc, Options::default())?;
    assert_eq!(
        output,
        write_projected_absolute(&molecule, Options::default())?
    );
    assert!(!fallback, "Only exported atoms qualify for 3D");
    Ok(())
}

#[test]
fn attachment_document_export_keeps_3d_coordinates() -> anyhow::Result<()> {
    use crate::document::Point;
    const ALKANE: &str =
        include_str!("../../../../../tests/fixtures/mol-import-3d/alkane-3d-v3000.mol");
    let mut doc = read(ALKANE)?.display_document()?.0;
    let members = doc.atoms.iter().map(|atom| atom.id).collect::<Vec<_>>();
    let point = crate::attachments::add(&mut doc, &members, Kind::MultiCenter)
        .map_err(anyhow::Error::msg)?;
    let metal = doc.add_atom("Cu", Point::new(14., 56.));
    doc.atom_mut(metal).unwrap().charge = 2;
    doc.atom_mut(metal).unwrap().depth = 21.;
    doc.add_bond(point, metal, 5, "plain");
    let before = doc.clone();
    let (output, fallback) = write_document_with_fallback(&doc)?;
    assert!(!fallback);
    assert_eq!(output, write_document(&doc)?);
    assert_eq!(output.lines().nth(1), Some("     RDKit          3D"));
    assert!(output.contains("ATTACH=ALL"));
    let imported = read(&output)?;
    let proxy = crate::attachments::interchange_graph(&doc).map_err(anyhow::Error::msg)?;
    let prepared = document::prepare(&proxy)?;
    assert_eq!(imported.molecule.positions.len(), prepared.ids.len());
    for (&id, position) in prepared.ids.iter().zip(&imported.molecule.positions) {
        let atom = doc.atom(id).unwrap();
        assert!((position.x - f64::from(atom.position.x) / 28.).abs() < 1e-4);
        assert!((position.y + f64::from(atom.position.y) / 28.).abs() < 1e-4);
        assert!((position.z - f64::from(atom.depth) / 28.).abs() < 1e-4);
    }
    assert_eq!(doc, before);
    Ok(())
}

#[test]
fn reaction_export_keeps_3d_participant_depth_and_2d_bytes() -> anyhow::Result<()> {
    use crate::{
        chemistry::reaction,
        document::{Arrow, Point},
        reactions::{self, Role},
    };
    const CHIRAL: &str =
        include_str!("../../../../../tests/fixtures/mol-import-3d/chiral-3d-v3000.mol");
    let mut doc = read(CHIRAL)?.display_document()?.0;
    let reactant = document::prepare(&doc)?;
    let product = doc.add_atom("C", Point::new(300., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(100., 0.),
        Point::new(200., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut doc, arrow, &reactant.ids, Role::Reactant)
        .map_err(anyhow::Error::msg)?;
    reactions::assign(&mut doc, arrow, &[product], Role::Product).map_err(anyhow::Error::msg)?;
    let (output, fallback) = reaction::write_rxn_with_fallback(&doc, None)?;
    assert!(!fallback);
    let imported = reaction::read_rxn(&output)?;
    for (&id, position) in reactant
        .ids
        .iter()
        .zip(&imported.reactants[0].molecule.positions)
    {
        assert!((position.z - f64::from(doc.atom(id).unwrap().depth) / 28.).abs() < 1e-4);
    }
    assert_eq!(
        crate::chemistry::smiles::write::write(
            &imported.reactants[0].molecule.state,
            Default::default()
        )?
        .text,
        crate::chemistry::smiles::write::write(&reactant.state, Default::default())?.text
    );
    let stereo = doc
        .atoms
        .iter_mut()
        .find_map(|atom| atom.stereo.as_mut())
        .unwrap();
    stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
    let (_, fallback) = reaction::write_rxn_with_fallback(&doc, None)?;
    assert!(fallback);
    for atom in &mut doc.atoms {
        atom.depth = 0.;
    }
    let (output, fallback) = reaction::write_rxn_with_fallback(&doc, None)?;
    assert!(!fallback);
    let projected = interchange_depiction(&document::prepare(&crate::editing::selection(
        &doc,
        &reactant.ids,
    ))?)?;
    let ctab = write_part(
        &projected,
        Options { force_v3000: true },
        true,
        &[],
        false,
        false,
    )?;
    let product = interchange_depiction(&document::prepare(&crate::editing::selection(
        &doc,
        &[product],
    ))?)?;
    let product = write_part(
        &product,
        Options { force_v3000: true },
        true,
        &[],
        false,
        false,
    )?;
    let mut expected = String::from(concat!(
        "$RXN V3000\n\n      RDKit\n\nM  V30 COUNTS 1 1 0\n",
        "M  V30 BEGIN REACTANT\n",
    ));
    expected.push_str(&ctab);
    expected.push_str("M  V30 END REACTANT\nM  V30 BEGIN PRODUCT\n");
    expected.push_str(&product);
    expected.push_str(concat!(
        "M  V30 END PRODUCT\nM  V30 BEGIN AGENT\n",
        "M  V30 END AGENT\nM  END\n",
    ));
    assert_eq!(output, expected);
    assert_eq!(output, reaction::write_rxn(&doc, None)?);
    Ok(())
}

#[test]
#[ignore = "isolated requested-Rust-allocation and lookup timing measurement"]
fn measure_attachment_index_tradeoff() {
    for (atoms, members) in [(32, 0), (4096, 0), (4096, 300), (4096, 30_000)] {
        let ids = (0..atoms)
            .map(|i| 9_007_199_254_740_993 + (i * 7) as u64)
            .collect::<Vec<_>>();
        let targets = (0..members)
            .map(|i| ids[atoms - 1 - i % atoms])
            .collect::<Vec<_>>();
        for indexed in [false, true] {
            let baseline = allocation_metrics::reset();
            let start = Instant::now();
            let indices = if indexed {
                let mut map = HashMap::new();
                map.extend(ids.iter().enumerate().map(|(i, &id)| (id, i)));
                assert_eq!(map.len(), atoms);
                Some(map)
            } else {
                assert_eq!(ids.iter().collect::<HashSet<_>>().len(), atoms);
                None
            };
            // Model the writer's output buffer overlap with the validation map.
            let output = vec![0u8; atoms * 80];
            let mut checksum = 0;
            for id in &targets {
                checksum += if let Some(indices) = &indices {
                    *indices.get(black_box(id)).unwrap()
                } else {
                    ids.iter().position(|n| n == black_box(id)).unwrap()
                };
            }
            black_box((&output, checksum));
            let elapsed = start.elapsed();
            let snapshot = allocation_metrics::snapshot();
            drop(indices);
            drop(output);
            let after_drop = allocation_metrics::snapshot();
            println!(
                "indexed={indexed} atoms={atoms} members={members} elapsed={elapsed:?} allocations={} allocated={} peak_extra={} live_during_serialization={} live_after_drop={} checksum={checksum}",
                snapshot.allocation_count,
                snapshot.allocated_bytes,
                snapshot.peak_bytes.saturating_sub(baseline),
                snapshot.live_bytes.saturating_sub(baseline),
                after_drop.live_bytes.saturating_sub(baseline)
            );
        }
    }
}
