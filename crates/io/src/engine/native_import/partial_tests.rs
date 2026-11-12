use super::*;

const QUERY_NOTICE: &str = "The input's query constraints (such as CXSMILES u, rb or s) were ignored; ReShiki imported the plain structure";
const ANY_NOTICE: &str = "Bond C1–C2 has any order (~); bond order assumed single";
const ZERO_NOTICE: &str =
    "Bond C1–Fe2 has zero order (CXSMILES Z); ReShiki imported the atoms without that bond";

fn imported(format: &str, text: &str) -> anyhow::Result<Box<Prepared>> {
    let Preparation::Complete(prepared) = prepare(Request::import(format, text))? else {
        panic!("{format}: {text}: import deferred");
    };
    prepared.document.validate().map_err(anyhow::Error::msg)?;
    molecular::validate_molecule(&prepared.molecule)?;
    Ok(prepared)
}

fn assert_plain_import(
    format: &str,
    text: &str,
    plain: &str,
    warnings: &[&str],
) -> anyhow::Result<Box<Prepared>> {
    let prepared = imported(format, text)?;
    let expected = imported(format, plain)?;
    assert_eq!(prepared.warnings, warnings, "{format}: {text}");
    assert_eq!(
        prepared.molecule.state.graph, expected.molecule.state.graph,
        "{format}: {text}: retained chemistry"
    );
    assert_eq!(
        prepared.document.atoms.len(),
        expected.document.atoms.len(),
        "{format}: {text}: drawn atoms"
    );
    assert_eq!(
        prepared.document.bonds.len(),
        expected.document.bonds.len(),
        "{format}: {text}: drawn bonds"
    );
    let readback = molecular::prepare(&prepared.document)?;
    assert_eq!(
        readback.state.graph, prepared.molecule.state.graph,
        "{format}: {text}: editable chemistry"
    );
    Ok(prepared)
}

#[test]
fn partial_import_drops_each_unsupported_coordination_configuration() -> anyhow::Result<()> {
    for (format, text, plain, element, warning) in [
        (
            "smiles",
            "F[Pt@SP1](Cl)(Br)I",
            "F[Pt](Cl)(Br)I",
            "Pt",
            "ReShiki cannot draw the square-planar configuration (@SP) at Pt2; it imported that atom without a configuration",
        ),
        (
            "smiles",
            "F[P@TB1](Cl)(Br)(I)N",
            "F[P](Cl)(Br)(I)N",
            "P",
            "ReShiki cannot draw the trigonal-bipyramidal configuration (@TB) at P2; it imported that atom without a configuration",
        ),
        (
            "smiles",
            "F[Co@OH1](Cl)(Br)(I)(N)O",
            "F[Co](Cl)(Br)(I)(N)O",
            "Co",
            "ReShiki cannot draw the octahedral configuration (@OH) at Co2; it imported that atom without a configuration",
        ),
        (
            "rsmi",
            "F[Pt@SP1](Cl)(Br)I>>FI",
            "F[Pt](Cl)(Br)I>>FI",
            "Pt",
            "ReShiki cannot draw the square-planar configuration (@SP) at Pt2; it imported that atom without a configuration",
        ),
    ] {
        let prepared = assert_plain_import(format, text, plain, &[warning])?;
        let center = prepared
            .document
            .atoms
            .iter()
            .find(|atom| atom.element == element)
            .unwrap();
        assert!(center.stereo.is_none(), "{text}");
        assert!(center.cip_label.is_none(), "{text}");
        assert!(
            prepared
                .molecule
                .state
                .metadata
                .atoms
                .iter()
                .all(|atom| atom.chiral_tag == 0 && atom.chiral_permutation.unwrap_or(0) == 0),
            "{text}"
        );
        assert!(
            prepared
                .document
                .bonds
                .iter()
                .all(|bond| bond.display == "plain"),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn partial_import_replaces_excess_radicals_with_hydrogens_and_round_trips() -> anyhow::Result<()> {
    for (format, text, count, before, formula, hydrogens) in [
        ("smiles", "[CH] |^4:0|", 3, 1, "CH2", vec![2]),
        ("smiles", "[CH]", 3, 1, "CH2", vec![2]),
        ("smiles", "[C]", 4, 0, "CH2", vec![2]),
        ("smiles", "[N]", 3, 0, "HN", vec![1]),
        ("smiles", "C |^5:0|", 3, 1, "CH2", vec![2]),
        ("smiles", "CCO |^5:0|", 3, 0, "C2H4O", vec![1, 2, 1]),
        ("smiles", "S |^5:0|", 3, 1, "S", vec![0]),
        ("smiles", "PC |^5:0|", 3, 1, "CH3P", vec![0, 3]),
        ("rsmi", "[CH]>>C |^4:0|", 3, 1, "C2H6", vec![2, 4]),
    ] {
        if format == "smiles" {
            let source = smiles::read(text)?.prepared.state;
            let source_facts = source.graph.atom_facts().map_err(anyhow::Error::msg)?;
            assert_eq!(source.graph.atoms[0].radical_electrons, count, "{text}");
            assert_eq!(
                source_facts[0].hydrogens, before,
                "{text}: source hydrogens"
            );
        }
        let prepared = imported(format, text)?;
        let element = &prepared.document.atoms[0].element;
        assert_eq!(
            prepared.warnings,
            [format!(
                "Atom {element}1 had {count} radical electrons; ReShiki keeps at most 2 and drew it as a triplet; its hydrogen count changed from {before} to {}",
                hydrogens[0]
            )],
            "{text}"
        );
        assert_eq!(prepared.document.atoms.len(), hydrogens.len(), "{text}");
        assert_eq!(prepared.document.atoms[0].radical_electrons, 2, "{text}");
        assert_eq!(
            prepared.document.atoms[0].explicit_h,
            u32::from(hydrogens[0]),
            "{text}"
        );
        assert!(!prepared.document.atoms[0].singlet, "{text}");

        let assert_state =
            |name: &str, state: &State, atom_order: &[usize]| -> anyhow::Result<()> {
                let facts = state.graph.atom_facts().map_err(anyhow::Error::msg)?;
                assert_eq!(facts.len(), hydrogens.len(), "{text}: {name} atoms");
                assert_eq!(
                    facts.iter().map(|atom| atom.hydrogens).collect::<Vec<_>>(),
                    atom_order
                        .iter()
                        .map(|&atom| hydrogens[atom])
                        .collect::<Vec<_>>(),
                    "{text}: {name} hydrogens"
                );
                assert_eq!(
                    facts
                        .iter()
                        .map(|atom| atom.radical_electrons)
                        .collect::<Vec<_>>(),
                    atom_order
                        .iter()
                        .map(|&atom| if atom == 0 { 2 } else { 0 })
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

        let reopened: Document = serde_json::from_str(&serde_json::to_string(&prepared.document)?)?;
        reopened.validate().map_err(anyhow::Error::msg)?;
        assert_eq!(
            molecular::prepare(&reopened)?.state.graph,
            prepared.molecule.state.graph,
            "{text}: native document round trip"
        );
        let mol = molfile::write_document(&prepared.document)?;
        assert_state(
            "MOL round trip",
            &molfile::read(&mol)?.molecule.state,
            &atom_order,
        )?;
        let smiles = smiles::write::write(&readback.state, Default::default())?;
        assert_state(
            "SMILES round trip",
            &smiles::read(&smiles.text)?.prepared.state,
            &smiles.atom_order,
        )?;
    }
    Ok(())
}

#[test]
fn partial_import_keeps_radicals_without_a_valence_derived_triplet_refused() -> anyhow::Result<()> {
    let text = "CS(C)C |^5:1|";
    let source = smiles::read(text)?.prepared.state;
    assert_eq!(source.graph.atoms[1].radical_electrons, 3);
    let mut bare = source.graph.clone();
    bare.atoms[1].no_implicit = true;
    bare.atoms[1].explicit_hydrogens = 0;
    assert_eq!(
        chemistry::sanitize::sanitize(&bare, &source.metadata, &source.directions)?
            .graph
            .atoms[1]
            .radical_electrons,
        1
    );
    assert_eq!(
        smiles::read_partial(text)?.prepared.state.graph,
        source.graph
    );
    assert!(prepare_smiles(text, false, Default::default(), true).is_err());
    assert_eq!(
        prepare(Request::import("smiles", text))
            .err()
            .unwrap()
            .to_string(),
        prepare_smiles(text, false, Default::default(), false)
            .err()
            .unwrap()
            .to_string()
    );
    Ok(())
}

#[test]
fn partial_import_clears_invalid_allene_tags() -> anyhow::Result<()> {
    for (format, text, plain, atom) in [
        ("smiles", "C[C@AL1](F)Cl", "C[C](F)Cl", "C2"),
        ("rsmi", "C[C@AL1](F)Cl>>CC", "C[C](F)Cl>>CC", "C2"),
        ("smiles", "[Pt@AL1](F)(Cl)(Br)", "[Pt](F)(Cl)(Br)", "Pt1"),
        (
            "smiles",
            "C[Pt@ALH](F)(Cl)(Br)(I)(N)(O)",
            "C[PtH](F)(Cl)(Br)(I)(N)(O)",
            "Pt2",
        ),
        ("smiles", "[S@AL2H1+0]", "[SH1+0]", "S1"),
        ("smiles", "[H][C@AL1](F)Cl", "[H][C](F)Cl", "C2"),
    ] {
        if format == "smiles" {
            assert_eq!(
                smiles::read(text).unwrap_err().to_string(),
                "SMILES contains unsupported chemistry: invalid allene stereochemistry",
                "{text}"
            );
        }
        let warning = format!(
            "The allene stereo tag (@AL) at {atom} is invalid; ReShiki imported that atom without a configuration"
        );
        let prepared = assert_plain_import(format, text, plain, &[&warning])?;
        assert!(
            prepared
                .document
                .atoms
                .iter()
                .all(|atom| atom.stereo.is_none()),
            "{text}"
        );
        assert!(
            prepared
                .molecule
                .state
                .metadata
                .atoms
                .iter()
                .all(|atom| atom.chiral_tag == 0 && atom.chiral_permutation.unwrap_or(0) == 0),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn partial_import_keeps_bare_cx_stars_without_query_notices() -> anyhow::Result<()> {
    for (format, text, plain) in [
        ("smiles", "* |(0,0,)|", "*"),
        ("smiles", "* |wD:1.0|", "*"),
        ("smiles", "* |atomProp:0._chiralPermutation.-1|", "*"),
        ("smiles", "C* |atomProp:0._UnknownStereo.bad|", "C*"),
        ("rsmi", "*C>>*C |(0,0,;1,0,;0,0,;1,0,)|", "*C>>*C"),
    ] {
        assert_plain_import(format, text, plain, &[])?;
    }
    Ok(())
}

#[test]
fn partial_import_ignores_each_query_constraint_with_one_notice() -> anyhow::Result<()> {
    for (format, text, plain) in [
        ("smiles", "CCO |u:0|", "CCO"),
        ("smiles", "CC1CC1 |rb:1:2|", "CC1CC1"),
        ("smiles", "CCO |s:0:2|", "CCO"),
        ("smiles", "* |u:0|", "*"),
        ("smiles", "*C |s:0:2|", "*C"),
        ("smiles", "CC* |rb:2:2|", "CC*"),
        ("smiles", "*CC |u:1|", "*CC"),
        ("smiles", "* |$star_e$|", "*"),
        ("smiles", "* |$Q_e$|", "*"),
        ("smiles", "* |$QH_p$|", "*"),
        ("smiles", "* |$AH_p$|", "*"),
        ("smiles", "* |$X_p$|", "*"),
        ("smiles", "* |$XH_p$|", "*"),
        ("smiles", "* |$M_p$|", "*"),
        ("smiles", "* |$MH_p$|", "*"),
        ("rsmi", "CCO>>CC=O |u:0|", "CCO>>CC=O"),
        ("rsmi", "*C>>*C |u:0,2|", "*C>>*C"),
        ("rsmi", "*C>>*C |$star_e;;Q_e$|", "*C>>*C"),
        ("smiles", "CCO |u:0,1,s:0:2|", "CCO"),
        ("rsmi", "CCO.CCO>CCO>CCO |u:0,3,6,9|", "CCO.CCO>CCO>CCO"),
    ] {
        assert_plain_import(format, text, plain, &[QUERY_NOTICE])?;
    }
    Ok(())
}

#[test]
fn partial_import_drops_zero_order_bonds_without_changing_hydrogens() -> anyhow::Result<()> {
    for (format, text, plain, bare, hydrogens) in [
        ("smiles", "C[Fe] |Z:0|", "C.[Fe]", "C[Fe]", vec![4, 0]),
        (
            "rsmi",
            "C[Fe]>>C |Z:0|",
            "C.[Fe]>>C",
            "C[Fe].C",
            vec![4, 0, 4],
        ),
    ] {
        // The strict reader refuses Z. Sanitize the bare graph with its zero
        // contact still present to establish the original hydrogen counts.
        let mut source = smiles::parse(bare)?;
        source.graph.bonds[0].order = 0;
        let source =
            chemistry::sanitize::sanitize(&source.graph, &source.metadata, &source.directions)?;
        let source_facts = source.graph.atom_facts().map_err(anyhow::Error::msg)?;
        assert_eq!(
            source_facts
                .iter()
                .map(|atom| atom.hydrogens)
                .collect::<Vec<_>>(),
            hydrogens,
            "{text}: source hydrogens"
        );
        let prepared = assert_plain_import(format, text, plain, &[ZERO_NOTICE])?;
        assert_eq!(prepared.document.atoms.len(), hydrogens.len(), "{text}");
        assert_eq!(
            prepared.molecule.state.graph.atoms.len(),
            hydrogens.len(),
            "{text}"
        );
        assert!(prepared.document.bonds.is_empty(), "{text}");
        assert!(prepared.molecule.state.graph.bonds.is_empty(), "{text}");
        assert_eq!(
            prepared
                .molecule
                .state
                .graph
                .atom_facts()
                .map_err(anyhow::Error::msg)?
                .iter()
                .map(|atom| (atom.hydrogens, atom.radical_electrons))
                .collect::<Vec<_>>(),
            source_facts
                .iter()
                .map(|atom| (atom.hydrogens, atom.radical_electrons))
                .collect::<Vec<_>>(),
            "{text}: retained chemistry"
        );
        let smiles = smiles::write::write(&prepared.molecule.state, Default::default())?;
        assert_eq!(
            smiles.text.split('.').count(),
            hydrogens.len(),
            "{text}: fragments"
        );
        let reopened: Document = serde_json::from_str(&serde_json::to_string(&prepared.document)?)?;
        assert_eq!(
            molecular::prepare(&reopened)?.state.graph,
            prepared.molecule.state.graph,
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn partial_import_zero_bond_removal_preserves_surviving_bonds_and_input_positions()
-> anyhow::Result<()> {
    let text = "C[Fe].C1CC1.C~C.F/C=C/F |Z:0,u:4|";
    let plain = "C.[Fe].C1CC1.CC.F/C=C/F";
    let prepared = assert_plain_import(
        "smiles",
        text,
        plain,
        &[
            QUERY_NOTICE,
            ZERO_NOTICE,
            "Bond C6–C7 has any order (~); bond order assumed single",
        ],
    )?;
    assert_eq!(prepared.document.atoms.len(), 11);
    assert_eq!(prepared.document.bonds.len(), 7);
    assert_eq!(prepared.molecule.state.rings.atoms.len(), 1);
    assert_eq!(
        smiles::write::write(&prepared.molecule.state, Default::default())?.text,
        smiles::write::write(
            &imported("smiles", plain)?.molecule.state,
            Default::default()
        )?
        .text,
        "{text}: surviving ring and alkene stereochemistry"
    );

    // Ring closure stores its endpoints in the reverse of input atom order.
    for (format, text, plain) in [
        ("smiles", "C1CC[Fe]1 |Z:3|", "CCC[Fe]"),
        ("rsmi", "C1CC[Fe]1>>C |Z:3|", "CCC[Fe]>>C"),
    ] {
        assert_plain_import(
            format,
            text,
            plain,
            &[
                "Bond C1–Fe4 has zero order (CXSMILES Z); ReShiki imported the atoms without that bond",
            ],
        )?;
    }
    Ok(())
}

#[test]
fn partial_import_assumes_any_order_bonds_are_single() -> anyhow::Result<()> {
    for (format, text, plain) in [("smiles", "C~C", "CC"), ("rsmi", "C~C>>CC", "CC>>CC")] {
        let prepared = assert_plain_import(format, text, plain, &[ANY_NOTICE])?;
        assert!(
            prepared.document.bonds.iter().all(|bond| bond.order == 1),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn partial_import_warning_positions_follow_input_atoms_before_h_removal_and_reaction_layout()
-> anyhow::Result<()> {
    let prepared = assert_plain_import(
        "smiles",
        "[H]C.F[Pt@SP1](Cl)(Br)I",
        "[H]C.F[Pt](Cl)(Br)I",
        &[
            "ReShiki cannot draw the square-planar configuration (@SP) at Pt4; it imported that atom without a configuration",
        ],
    )?;
    assert_eq!(prepared.document.atoms.len(), 6);
    assert!(
        prepared
            .document
            .atoms
            .iter()
            .all(|atom| atom.element != "H")
    );
    assert_plain_import(
        "rsmi",
        "CC.C~C>N~N>O~O",
        "CC.CC>NN>OO",
        &[
            "Bond C3–C4 has any order (~); bond order assumed single",
            "Bond N5–N6 has any order (~); bond order assumed single",
            "Bond O7–O8 has any order (~); bond order assumed single",
        ],
    )?;
    assert_plain_import(
        "rsmi",
        "C>C1.N~N.CC1>C",
        "C>C1.NN.CC1>C",
        &["Bond N3–N4 has any order (~); bond order assumed single"],
    )?;
    Ok(())
}

#[test]
fn partial_import_preserves_supported_tetrahedral_and_diradical_input() -> anyhow::Result<()> {
    let tetrahedral = imported("smiles", "F[C@H](Cl)Br")?;
    assert!(tetrahedral.warnings.is_empty());
    assert!(
        tetrahedral
            .document
            .atoms
            .iter()
            .any(|atom| atom.stereo.is_some())
    );
    let diradical = imported("smiles", "[CH2]")?;
    assert!(diradical.warnings.is_empty());
    assert_eq!(diradical.document.atoms[0].radical_electrons, 2);
    assert_eq!(diradical.document.atoms[0].explicit_h, 2);
    assert!(!diradical.document.atoms[0].singlet);
    Ok(())
}

#[test]
fn partial_import_keeps_hydrogen_bond_refusals_unchanged() -> anyhow::Result<()> {
    for text in ["C[Fe] |H:0.0|", "N[H]O |H:1.1|"] {
        let strict = smiles::read(text)?;
        assert!(strict.dropped.is_empty(), "{text}");
        assert!(
            strict
                .prepared
                .state
                .graph
                .bonds
                .iter()
                .any(|bond| bond.order == 0),
            "{text}: strict reader retains the hydrogen bond"
        );
        for format in ["smiles", "naming_smiles"] {
            assert_eq!(
                prepare(Request::import(format, text))
                    .err()
                    .unwrap()
                    .to_string(),
                "Invalid drawing: Unsupported bond appearance for this order",
                "{text}: {format}"
            );
        }
    }
    Ok(())
}

#[test]
fn partial_import_does_not_relax_strict_readers_or_private_naming_import() -> anyhow::Result<()> {
    for (text, error) in [
        ("C~C", "SMILES contains unsupported chemistry: query bond"),
        (
            "CCO |u:0|",
            "SMILES contains unsupported chemistry: query atom",
        ),
        (
            "CC1CC1 |rb:1:2|",
            "SMILES contains unsupported chemistry: query atom",
        ),
        (
            "CCO |s:0:2|",
            "SMILES contains unsupported chemistry: query atom",
        ),
        (
            "C[Fe] |Z:0|",
            "SMILES contains unsupported chemistry: bond type",
        ),
        (
            "C[C@AL1](F)Cl",
            "SMILES contains unsupported chemistry: invalid allene stereochemistry",
        ),
    ] {
        assert_eq!(
            smiles::read(text).unwrap_err().to_string(),
            error,
            "{text}: read"
        );
        assert_eq!(
            smiles::read_dropping_flagged_ez(text)
                .unwrap_err()
                .to_string(),
            error,
            "{text}: read_dropping_flagged_ez"
        );
        assert_eq!(
            prepare(Request::import("naming_smiles", text))
                .err()
                .unwrap()
                .to_string(),
            error,
            "{text}: naming_smiles"
        );
    }
    assert_eq!(
        smiles::parse("C~C").unwrap_err().to_string(),
        "SMILES contains unsupported chemistry: query bond"
    );
    for text in [
        "F[Pt@SP1](Cl)(Br)I",
        "F[P@TB1](Cl)(Br)(I)N",
        "F[Co@OH1](Cl)(Br)(I)(N)O",
        "[CH] |^4:0|",
        "[CH]",
        "[C]",
        "[N]",
        "C |^5:0|",
        "CCO |^5:0|",
    ] {
        let strict = smiles::read(text)?.prepared.state;
        let dropping = smiles::read_dropping_flagged_ez(text)?.prepared.state;
        assert_eq!(
            dropping.graph, strict.graph,
            "{text}: strict reader chemistry"
        );
        assert_eq!(
            dropping
                .metadata
                .atoms
                .iter()
                .map(|atom| atom.chiral_tag)
                .collect::<Vec<_>>(),
            strict
                .metadata
                .atoms
                .iter()
                .map(|atom| atom.chiral_tag)
                .collect::<Vec<_>>(),
            "{text}: strict reader configuration"
        );
        assert_eq!(
            molecular::validate_molecule(&molecule(strict)?)
                .unwrap_err()
                .to_string(),
            "Invalid drawing: Unsupported drawing stereochemistry or radical count",
            "{text}: strict drawing"
        );
        assert_eq!(
            prepare(Request::import("naming_smiles", text))
                .err()
                .unwrap()
                .to_string(),
            "Invalid drawing: Unsupported drawing stereochemistry or radical count",
            "{text}: naming_smiles"
        );
    }
    for (text, error) in [
        (
            "F[Pt@SP1](Cl)(Br)I>>FI",
            "SMILES contains unsupported chemistry: reaction stereochemistry or radical count",
        ),
        (
            "[CH]>>C |^4:0|",
            "SMILES contains unsupported chemistry: reaction stereochemistry or radical count",
        ),
        (
            "CCO>>CC=O |u:0|",
            "SMILES contains unsupported chemistry: reaction query or bond type",
        ),
        (
            "C[Fe]>>C |Z:0|",
            "SMILES contains unsupported chemistry: reaction query or bond type",
        ),
        (
            "C~C>>CC",
            "SMILES contains unsupported chemistry: query bond",
        ),
        (
            "C[C@AL1](F)Cl>>CC",
            "SMILES contains unsupported chemistry: invalid allene stereochemistry",
        ),
    ] {
        assert_eq!(
            reaction::read_smiles(text).unwrap_err().to_string(),
            error,
            "{text}"
        );
    }
    let grouped = "F[Pt@SP1](Cl)(Br)I.C[C@H](O)Cl |&1:6|";
    assert_eq!(
        prepare(Request::import_smiles(grouped))
            .err()
            .unwrap()
            .to_string(),
        "Invalid drawing: Unsupported drawing stereochemistry or radical count",
        "normalizing SP must not discard the independent enhanced stereo group"
    );
    assert_eq!(
        prepare(Request::import(
            "rsmi",
            "F[Pt@SP1](Cl)(Br)I.C[C@H](O)Cl>>FI |&1:6|"
        ))
        .err()
        .unwrap()
        .to_string(),
        "SMILES contains unsupported chemistry: reaction stereochemistry or radical count",
    );
    Ok(())
}

#[tokio::test]
async fn partial_import_exact_notices_reach_the_native_response() -> anyhow::Result<()> {
    for (format, text, warning, formula) in [
        ("smiles", "C~C", ANY_NOTICE, "C2H6"),
        (
            "smiles",
            "[CH]",
            "Atom C1 had 3 radical electrons; ReShiki keeps at most 2 and drew it as a triplet; its hydrogen count changed from 1 to 2",
            "CH2",
        ),
    ] {
        let Outcome::Complete(response) = execute(
            Request::import(format, text),
            Some(native_response::Config::in_process()),
        )
        .await?
        else {
            panic!("{text}: import deferred");
        };
        assert_eq!(response.warnings, [warning], "{text}");
        assert_eq!(
            response.analysis.as_ref().unwrap().formula,
            formula,
            "{text}"
        );
        let document = response.document.as_ref().unwrap();
        document.validate().map_err(anyhow::Error::msg)?;
        molecular::prepare(document)?;
    }
    Ok(())
}

#[test]
fn partial_import_any_bonds_keep_explicit_cx_bond_kinds() -> anyhow::Result<()> {
    let any = imported("smiles", "C~C")?;
    assert_eq!(any.warnings, [ANY_NOTICE]);
    assert_eq!(any.molecule.state.graph.bonds[0].order, 1);
    let prepared = imported("smiles", "C~[Fe] |C:0.0|")?;
    assert!(prepared.warnings.is_empty());
    assert_eq!(prepared.molecule.state.graph.bonds[0].order, 5);
    assert_plain_import("smiles", "C~[Fe] |Z:0|", "C.[Fe]", &[ZERO_NOTICE])?;
    assert_plain_import("smiles", "C~1CCCCC1 |u:0|", "C~1CCCCC1", &[QUERY_NOTICE])?;
    let expected = prepare_smiles("C[Fe] |H:0.0|", false, Default::default(), false)
        .err()
        .unwrap()
        .to_string();
    assert_eq!(
        prepare_smiles("C~[Fe] |H:0.0|", false, Default::default(), true)
            .err()
            .unwrap()
            .to_string(),
        expected
    );
    assert_eq!(
        prepare(Request::import("smiles", "C~[Fe] |H:0.0|"))
            .err()
            .unwrap()
            .to_string(),
        "SMILES contains unsupported chemistry: query bond"
    );
    Ok(())
}

#[test]
fn partial_import_does_not_drop_enhanced_groups_on_unsupported_centres() {
    for (format, text, error) in [
        (
            "smiles",
            "F[Pt@SP1](Cl)(Br)I |&1:1|",
            "Invalid drawing: Unsupported drawing stereochemistry or radical count",
        ),
        (
            "smiles",
            "F[Pt@SP1](Cl)(Br)I |o1:1|",
            "Invalid drawing: Unsupported drawing stereochemistry or radical count",
        ),
        (
            "rsmi",
            "F[Pt@SP1](Cl)(Br)I>>FI |&1:1|",
            "SMILES contains unsupported chemistry: reaction stereochemistry or radical count",
        ),
        (
            "smiles",
            "C[C@AL1](F)Cl |&1:1|",
            "SMILES contains unsupported chemistry: invalid allene stereochemistry",
        ),
        (
            "rsmi",
            "C[C@AL1](F)Cl>>CC |&1:1|",
            "SMILES contains unsupported chemistry: invalid allene stereochemistry",
        ),
    ] {
        assert_eq!(
            prepare(Request::import(format, text))
                .err()
                .unwrap()
                .to_string(),
            error,
            "{text}"
        );
        if format == "smiles" {
            assert!(
                prepare_smiles(text, false, Default::default(), true).is_err(),
                "{text}"
            );
        } else {
            assert!(prepare_reaction_smiles(text, true).is_err(), "{text}");
        }
    }
}

#[test]
fn partial_import_radical_notices_count_bonded_hydrogen_vertices() -> anyhow::Result<()> {
    for (format, text) in [("rsmi", "[H][C]>>C"), ("smiles", "[2H][C]")] {
        let prepared = imported(format, text)?;
        assert_eq!(
            prepared.warnings,
            [
                "Atom C2 had 3 radical electrons; ReShiki keeps at most 2 and drew it as a triplet; its hydrogen count changed from 1 to 2"
            ],
            "{text}"
        );
        let carbon = prepared
            .molecule
            .state
            .graph
            .atoms
            .iter()
            .find(|a| a.radical_electrons == 2)
            .unwrap();
        assert_eq!(carbon.explicit_hydrogens, 1, "{text}");
    }
    Ok(())
}

#[test]
fn partial_import_warning_classifier_excludes_other_notices() -> anyhow::Result<()> {
    for text in [
        "F[Pt@SP1](Cl)(Br)I",
        "[CH]",
        "C[C@AL1](F)Cl",
        "CCO |u:0|",
        "C~C",
        "C[Fe] |Z:0|",
    ] {
        let prepared = imported("smiles", text)?;
        assert!(!prepared.warnings.is_empty(), "{text}");
        assert!(
            prepared
                .warnings
                .iter()
                .all(|warning| is_partial_import_warning(warning)),
            "{text}"
        );
    }
    for warning in [
        "",
        CUMULENE_EZ_NOTICE,
        RELATIVE_STEREO_NOTICE,
        "Bond C1–C2",
        "Atom C1 had 3 radical electrons",
        "ReShiki cannot draw the atom",
    ] {
        assert!(!is_partial_import_warning(warning), "{warning}");
    }
    let prepared = imported(
        "rsmi",
        "F[Pt@SP1](Cl)(Br)I>N>CC |atomProp:1._chiralPermutation.bad|",
    )?;
    assert_eq!(
        prepared.warnings,
        [
            "ReShiki cannot draw the square-planar configuration (@SP) at Pt2; it imported that atom without a configuration"
        ]
    );
    Ok(())
}

#[test]
fn partial_import_keeps_strict_successes_and_errors_unchanged() -> anyhow::Result<()> {
    for text in [
        "C~1CC:1",
        "C~%99CC~%99",
        "C/1CC~1",
        "C~1CCCCC1",
        "[H]~C",
        "C~[H]",
        "[H]~C~[H]",
        "C~[H+]",
        "[H]~[C@](F)(Cl)Br",
        "[13CH3]~[H]",
        "[H]~C |H:0.0,Z:0|",
        "CC=[C@AL1]=CC",
        "CC=[C@AL2]=CC",
        "CC=[C@]=CC",
    ] {
        let strict = smiles::read(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        let expected = prepare_smiles(text, false, Default::default(), false)
            .unwrap_or_else(|error| panic!("{text}: {error}"));
        let actual = imported("smiles", text)?;
        assert!(actual.warnings.is_empty(), "{text}: {:?}", actual.warnings);
        assert_eq!(
            serde_json::to_vec(&actual.document)?,
            serde_json::to_vec(&expected.document)?,
            "{text}"
        );
        assert_eq!(
            actual.molecule.state.graph, strict.prepared.state.graph,
            "{text}: analysis graph"
        );
    }
    let text = "O>C~0CC-0>O";
    let expected = prepare_reaction_smiles(text, false)?;
    let actual = imported("rsmi", text)?;
    assert!(actual.warnings.is_empty());
    assert_eq!(
        serde_json::to_vec(&actual.document)?,
        serde_json::to_vec(&expected.document)?
    );
    assert_eq!(actual.molecule.state.graph, expected.molecule.state.graph);
    for (format, text, strict) in [
        (
            "smiles",
            "C~[Fe] |H:0.0|",
            prepare_smiles("C~[Fe] |H:0.0|", false, Default::default(), false),
        ),
        (
            "rsmi",
            "FC(Cl)=C(F)Cl>>C |Z:0,t:2|",
            prepare_reaction_smiles("FC(Cl)=C(F)Cl>>C |Z:0,t:2|", false),
        ),
    ] {
        assert_eq!(
            prepare(Request::import(format, text))
                .err()
                .unwrap()
                .to_string(),
            strict.err().unwrap().to_string(),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn partial_import_reference_coordination_examples_warn_without_configuration() -> anyhow::Result<()>
{
    for (text, atom, class) in [
        (
            "[Pt@SP1](Cl)(F)(Br)I",
            "Pt1",
            "square-planar configuration (@SP)",
        ),
        (
            "[P@TB5](F)(Cl)(Br)(I)N",
            "P1",
            "trigonal-bipyramidal configuration (@TB)",
        ),
        (
            "[Co@OH23](F)(Cl)(Br)(I)(N)O",
            "Co1",
            "octahedral configuration (@OH)",
        ),
    ] {
        let prepared = imported("smiles", text)?;
        assert_eq!(
            prepared.warnings,
            [format!(
                "ReShiki cannot draw the {class} at {atom}; it imported that atom without a configuration"
            )],
            "{text}"
        );
        assert_eq!(
            prepared.molecule.state.metadata.atoms[0].chiral_tag, 0,
            "{text}"
        );
        assert!(prepared.document.atoms[0].stereo.is_none(), "{text}");
    }
    Ok(())
}
