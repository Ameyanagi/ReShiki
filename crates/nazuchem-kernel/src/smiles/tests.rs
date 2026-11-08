// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original nazuchem/ReShiki code; source history and separately attributed data/API context follow.
//   ReShiki, crates/chemistry/src/smiles/tests.rs @ be63a799aa03a83dc6ec74bb294f4d42b89298d3.

// Copied from crates/chemistry/src/smiles/tests.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3; changed: pass explicit default import/enumeration budgets; format affected calls
use super::Error;

const BINOL: &str = "c1(-c2c(O)c3c(cc2)cccc3)c(O)c2c(cc1)cccc2";
const BINOL_COORDINATES: &str = "0,0,;1.5,0,;2.25,-1.299,;1.5,-2.598,;3.75,-1.299,;4.5,0,;3.75,1.299,;2.25,1.299,;6,0,;6.75,-1.299,;6,-2.598,;4.5,-2.598,;-0.75,1.299,;0,2.598,;-2.25,1.299,;-3,0,;-2.25,-1.299,;-0.75,-1.299,;-4.5,0,;-5.25,1.299,;-4.5,2.598,;-3,2.598,";

fn binol(wedges: &str) -> String {
    format!("{BINOL} |({BINOL_COORDINATES}){wedges}|")
}

#[test]
fn protium_does_not_create_configuration() -> anyhow::Result<()> {
    for text in [
        "[1H][C@@H](C)O",
        "O[C@H](C[1H])C",
        "[1H]C/C(C)=C/F",
        "[1H]/C=C/F",
        r"[1H]/C=C\F",
        "[1H]/C([H])=C/F",
        "[1H]/C([1H])=C/F",
    ] {
        let state = super::read(text, &crate::budget::Budget::default())?
            .prepared
            .state;
        assert!(
            state.metadata.atoms.iter().all(|a| a.chiral_tag == 0),
            "{text}"
        );
        assert!(state.metadata.bonds.iter().all(|b| b.stereo == 0), "{text}");
        let assigned = crate::stereo::cip::label::assign(&state, &Default::default())?;
        assert!(assigned.labels.is_empty(), "{text}");
        assert!(
            assigned
                .state
                .properties
                .atoms
                .iter()
                .all(|a| a.cip_code.is_none()),
            "{text}"
        );
        assert!(
            assigned
                .state
                .properties
                .bond_codes
                .iter()
                .all(Option::is_none),
            "{text}"
        );
        let output = super::write::write(&state, Default::default())?.text;
        assert!(output.contains("[1H]"), "{text}: {output}");
        assert!(!output.contains(['@', '/', '\\']), "{text}: {output}");
    }
    Ok(())
}

#[test]
fn protium_preserves_genuine_configuration_on_round_trip() -> anyhow::Result<()> {
    for (text, expected) in [
        ("[1H][C@](F)(Cl)Br", "S"),
        ("[1H][C@@](F)(Cl)Br", "R"),
        ("[1H][C@]([2H])(F)Cl", "S"),
        ("[1H][C@@]([2H])(F)Cl", "R"),
        ("[1H]/C(F)=C(Cl)/Br", "Z"),
        (r"[1H]/C(F)=C(Cl)\Br", "E"),
        ("[1H]/C([2H])=C/F", "Z"),
        (r"[1H]/C([2H])=C\F", "E"),
    ] {
        let state = super::read(text, &crate::budget::Budget::default())?
            .prepared
            .state;
        let output = super::write::write(&state, Default::default())?.text;
        assert!(output.contains("[1H]"), "{text}: {output}");
        for state in [
            state,
            super::read(&output, &crate::budget::Budget::default())?
                .prepared
                .state,
        ] {
            let assigned = crate::stereo::cip::label::assign(&state, &Default::default())?;
            let codes = assigned
                .state
                .properties
                .atoms
                .iter()
                .filter_map(|a| a.cip_code.as_deref())
                .chain(
                    assigned
                        .state
                        .properties
                        .bond_codes
                        .iter()
                        .filter_map(|code| code.as_deref()),
                )
                .collect::<Vec<_>>();
            assert_eq!(codes, [expected], "{text}: {output}");
        }
    }
    Ok(())
}

#[test]
fn stereoisomers_preserve_absolute_and_expand_relative_lactic_acid() -> anyhow::Result<()> {
    let absolute = "C[C@H](O)C(=O)O";
    let options = super::write::Options::default();
    assert!(options.canonical);
    assert!(options.symbols.isomeric);
    assert_eq!(
        super::stereoisomers(absolute, &crate::budget::Budget::default())?,
        [super::write::write(
            &super::read(absolute, &crate::budget::Budget::default())?
                .prepared
                .state,
            options
        )?
        .text]
        .into()
    );
    let expected = [absolute.into(), "C[C@@H](O)C(=O)O".into()].into();
    for extension in ["o1:1", "&1:1", "r"] {
        assert_eq!(
            super::stereoisomers(
                &format!("{absolute} |{extension}|"),
                &crate::budget::Budget::default()
            )?,
            expected,
            "{extension}"
        );
    }
    assert_eq!(
        super::stereoisomers("CC(O)C(=O)O", &crate::budget::Budget::default())?.len(),
        1
    );
    Ok(())
}

#[test]
fn stereoisomers_invert_each_enhanced_group_independently() -> anyhow::Result<()> {
    let molecule = "C[C@H](O)[C@H](F)Cl";
    let expected = [
        "C[C@H](O)[C@H](F)Cl".into(),
        "C[C@H](O)[C@@H](F)Cl".into(),
        "C[C@@H](O)[C@H](F)Cl".into(),
        "C[C@@H](O)[C@@H](F)Cl".into(),
    ]
    .into();
    for extension in ["o1:1,&1:3", "o1:1,o2:3", "&1:1,&2:3", "r,o1:1", "r,&1:1"] {
        assert_eq!(
            super::stereoisomers(
                &format!("{molecule} |{extension}|"),
                &crate::budget::Budget::default()
            )?,
            expected,
            "{extension}"
        );
    }
    let paired = ["C[C@H](O)[C@H](F)Cl".into(), "C[C@@H](O)[C@@H](F)Cl".into()].into();
    for extension in ["o1:1,3", "&1:1,3", "r", "o1:1,3,r"] {
        assert_eq!(
            super::stereoisomers(
                &format!("{molecule} |{extension}|"),
                &crate::budget::Budget::default()
            )?,
            paired,
            "{extension}"
        );
    }
    for extension in ["a:1,&1:3", "r,a:1,&1:3", "r,a:1,o1:3", "r,a:1"] {
        assert_eq!(
            super::stereoisomers(
                &format!("{molecule} |{extension}|"),
                &crate::budget::Budget::default()
            )?,
            ["C[C@H](O)[C@H](F)Cl".into(), "C[C@H](O)[C@@H](F)Cl".into(),].into(),
            "{extension}"
        );
    }
    for extension in ["a:3,o1:1", "r,o1:1,a:3", "r,&1:1,a:3"] {
        assert_eq!(
            super::stereoisomers(
                &format!("{molecule} |{extension}|"),
                &crate::budget::Budget::default()
            )?,
            ["C[C@H](O)[C@H](F)Cl".into(), "C[C@@H](O)[C@H](F)Cl".into(),].into(),
            "{extension}"
        );
    }
    assert_eq!(
        super::stereoisomers(
            &format!("{molecule} |r,a:1,3|"),
            &crate::budget::Budget::default()
        )?,
        super::stereoisomers(molecule, &crate::budget::Budget::default())?
    );
    Ok(())
}

#[test]
fn stereoisomers_deduplicate_meso_and_refresh_ring_relationships() -> anyhow::Result<()> {
    // Atom maps are not identity: a mapped meso input needs no group either.
    let mapped = super::read(
        "[CH3:1][C@H:2]([OH:3])[C@H:4]([OH:5])[CH3:6] |r|",
        &crate::budget::Budget::default(),
    )?;
    assert!(mapped.prepared.state.metadata.groups.is_empty());
    for molecule in ["O[C@H]1CC[C@@H](O)CC1", "C[C@H](O)[C@H](O)C"] {
        let absolute = super::read(molecule, &crate::budget::Budget::default())?;
        let relative = super::read(
            &format!("{molecule} |r|"),
            &crate::budget::Budget::default(),
        )?;
        assert!(relative.relative);
        assert!(relative.prepared.state.metadata.groups.is_empty());
        assert_eq!(
            serde_json::to_value(relative)?,
            serde_json::to_value(absolute)?,
            "{molecule}"
        );
        assert_eq!(
            super::stereoisomers(
                &format!("{molecule} |r|"),
                &crate::budget::Budget::default()
            )?,
            super::stereoisomers(molecule, &crate::budget::Budget::default())?,
            "{molecule}"
        );
    }
    let molecule = "O[C@H]1CC[C@@H](O)CC1";
    let expected = super::stereoisomers(molecule, &crate::budget::Budget::default())?;
    assert_eq!(expected.len(), 1);
    let mut independent = expected;
    independent.extend(super::stereoisomers(
        "O[C@H]1CC[C@H](O)CC1",
        &crate::budget::Budget::default(),
    )?);
    assert_eq!(independent.len(), 2);
    assert_eq!(
        super::stereoisomers(
            &format!("{molecule} |o1:1,&1:4|"),
            &crate::budget::Budget::default()
        )?,
        independent
    );
    Ok(())
}

#[test]
fn stereoisomers_use_group_indices_after_explicit_hydrogen_removal() -> anyhow::Result<()> {
    for (extension, kind) in [
        ("o1:1", 1),
        ("&1:1", 2),
        ("r", 2),
        ("r,o1:1", 1),
        ("r,&1:1", 2),
        ("r,a:1", 0),
    ] {
        let molecule = format!("[H][C@](C)(O)C(=O)O |{extension}|");
        let imported = super::read(&molecule, &crate::budget::Budget::default())?;
        assert_eq!(imported.prepared.state.graph.atoms.len(), 6);
        assert_eq!(imported.prepared.state.metadata.groups.len(), 1);
        assert_eq!(imported.prepared.state.metadata.groups[0].kind, kind);
        assert_eq!(imported.prepared.state.metadata.groups[0].atoms, [0]);
        assert!(matches!(
            imported.prepared.state.metadata.atoms[0].chiral_tag,
            1 | 2
        ));
        let expected = if kind == 0 {
            super::stereoisomers("[H][C@](C)(O)C(=O)O", &crate::budget::Budget::default())?
        } else {
            ["C[C@H](O)C(=O)O".into(), "C[C@@H](O)C(=O)O".into()].into()
        };
        assert_eq!(
            super::stereoisomers(&molecule, &crate::budget::Budget::default())?,
            expected,
            "{extension}"
        );
    }
    Ok(())
}

#[test]
fn stereoisomers_preserve_double_bond_stereo_across_group_inversions() -> anyhow::Result<()> {
    let molecule = "C[C@H](O)/C=C/F";
    let mut expected = super::stereoisomers(molecule, &crate::budget::Budget::default())?;
    expected.extend(super::stereoisomers(
        "C[C@@H](O)/C=C/F",
        &crate::budget::Budget::default(),
    )?);
    assert_eq!(expected.len(), 2);
    for extension in ["o1:1", "r"] {
        assert_eq!(
            super::stereoisomers(
                &format!("{molecule} |{extension}|"),
                &crate::budget::Budget::default()
            )?,
            expected
        );
    }
    for text in expected {
        assert!(text.contains('/') || text.contains('\\'));
        let state = super::read(&text, &crate::budget::Budget::default())?
            .prepared
            .state;
        let double = state.graph.bonds.iter().position(|b| b.order == 2).unwrap();
        assert_eq!(state.metadata.bonds[double].stereo, 3);
    }
    Ok(())
}

#[test]
fn relative_cx_marker_is_exact_and_normalized_into_import_output() -> anyhow::Result<()> {
    let molecule = "C[C@H](O)C(=O)O";
    let absolute = super::read(molecule, &crate::budget::Budget::default())?;
    let relative = super::read(
        &format!("{molecule} |r|"),
        &crate::budget::Budget::default(),
    )?;
    assert!(!absolute.relative);
    assert!(relative.relative);
    assert_ne!(
        serde_json::to_value(&absolute)?,
        serde_json::to_value(&relative)?
    );
    assert!(absolute.prepared.state.metadata.groups.is_empty());
    assert_eq!(relative.prepared.state.metadata.groups.len(), 1);
    assert_eq!(relative.prepared.state.metadata.groups[0].kind, 2);
    assert_eq!(relative.prepared.state.metadata.groups[0].atoms, [1]);
    for extension in ["rb:", "rr", "r1", "r:", "xr"] {
        let imported = super::read(
            &format!("{molecule} |{extension}|"),
            &crate::budget::Budget::default(),
        )?;
        assert!(!imported.relative, "{extension}");
        assert_eq!(
            serde_json::to_value(&absolute)?,
            serde_json::to_value(imported)?
        );
    }
    assert!(
        super::read(
            &format!("{molecule} |rb:0:0|"),
            &crate::budget::Budget::default()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn relative_cx_groups_keep_configurations_and_explicit_groups() -> anyhow::Result<()> {
    for (molecule, inverse, atoms) in [
        ("C[C@H](O)C(=O)O", "C[C@@H](O)C(=O)O", vec![1]),
        ("C[C@@H](O)C(=O)O", "C[C@H](O)C(=O)O", vec![1]),
        ("C[C@H](O)[C@@H](C)Cl", "C[C@@H](O)[C@H](C)Cl", vec![1, 3]),
        ("C[C@H](O)[C@@H](O)C", "C[C@@H](O)[C@H](O)C", vec![1, 3]),
    ] {
        let absolute = super::read(molecule, &crate::budget::Budget::default())?;
        let relative = super::read(
            &format!("{molecule} |r|"),
            &crate::budget::Budget::default(),
        )?;
        let state = &relative.prepared.state;
        assert_eq!(state.metadata.groups.len(), 1, "{molecule}");
        assert_eq!(state.metadata.groups[0].kind, 2, "{molecule}");
        assert_eq!(state.metadata.groups[0].atoms, atoms, "{molecule}");
        assert_eq!(
            serde_json::to_value(&state.metadata.atoms)?,
            serde_json::to_value(&absolute.prepared.state.metadata.atoms)?,
            "{molecule}"
        );
        let mut expected = super::stereoisomers(molecule, &crate::budget::Budget::default())?;
        expected.extend(super::stereoisomers(
            inverse,
            &crate::budget::Budget::default(),
        )?);
        assert_eq!(expected.len(), 2);
        assert_eq!(
            super::stereoisomers(
                &format!("{molecule} |r|"),
                &crate::budget::Budget::default()
            )?,
            expected,
            "{molecule}"
        );
    }
    let molecule = "C[C@H](O)[C@H](F)Cl";
    for extension in ["a:1,3", "o1:1,3", "&1:1,3", "o1:1,a:3", "&1:1,a:3"] {
        let explicit = super::read(
            &format!("{molecule} |{extension}|"),
            &crate::budget::Budget::default(),
        )?;
        let relative = super::read(
            &format!("{molecule} |r,{extension}|"),
            &crate::budget::Budget::default(),
        )?;
        assert_eq!(
            serde_json::to_value(&relative.prepared.state)?,
            serde_json::to_value(&explicit.prepared.state)?,
            "{extension}"
        );
    }
    let molecule = "C[C@H](O)C(=O)O.O[C@H]1CC[C@@H](O)CC1";
    let explicit = super::read(
        &format!("{molecule} |a:1|"),
        &crate::budget::Budget::default(),
    )?;
    let relative = super::read(
        &format!("{molecule} |r,a:1|"),
        &crate::budget::Budget::default(),
    )?;
    assert_eq!(relative.prepared.state.metadata.groups.len(), 1);
    assert_eq!(relative.prepared.state.metadata.groups[0].kind, 0);
    assert_eq!(
        serde_json::to_value(relative)?,
        serde_json::to_value(explicit)?
    );
    Ok(())
}

#[test]
fn relative_cx_adds_one_group_without_inverting_explicit_absolute_centres() -> anyhow::Result<()> {
    let molecule = "C[C@H](O)[C@H](F)[C@H](Br)Cl";
    let imported = super::read(
        &format!("{molecule} |r,o1:1,a:3|"),
        &crate::budget::Budget::default(),
    )?;
    let groups = &imported.prepared.state.metadata.groups;
    assert_eq!(groups.len(), 3);
    assert_eq!((groups[0].kind, groups[0].atoms.as_slice()), (1, &[1][..]));
    assert_eq!((groups[1].kind, groups[1].atoms.as_slice()), (0, &[3][..]));
    assert_eq!((groups[2].kind, groups[2].atoms.as_slice()), (2, &[5][..]));
    assert_eq!(
        super::stereoisomers(
            &format!("{molecule} |r,o1:1,a:3|"),
            &crate::budget::Budget::default()
        )?,
        [
            "C[C@H](O)[C@H](F)[C@H](Cl)Br".into(),
            "C[C@@H](O)[C@H](F)[C@H](Cl)Br".into(),
            "C[C@H](O)[C@H](F)[C@@H](Cl)Br".into(),
            "C[C@@H](O)[C@H](F)[C@@H](Cl)Br".into(),
        ]
        .into()
    );
    Ok(())
}

#[test]
fn relative_cx_keeps_inversion_independent_of_explicit_groups() -> anyhow::Result<()> {
    let molecule = "C[C@H](O)[C@H](F)Cl.C[C@H](O)[C@@H](F)Cl";
    for (extension, explicit) in [
        ("r,o1:1,a:7", "o1:1,a:7,&1:3,9"),
        ("r,&1:1,a:7", "&1:1,a:7,&2:3,9"),
    ] {
        let relative = format!("{molecule} |{extension}|");
        let expected = super::stereoisomers(
            &format!("{molecule} |{explicit}|"),
            &crate::budget::Budget::default(),
        )?;
        assert_eq!(expected.len(), 3);
        assert_eq!(
            super::stereoisomers(&relative, &crate::budget::Budget::default())?,
            expected,
            "{extension}"
        );
        let imported = super::read(&relative, &crate::budget::Budget::default())?;
        let groups = &imported.prepared.state.metadata.groups;
        assert_eq!(groups.len(), 3);
        assert_eq!(
            (groups[2].kind, groups[2].atoms.as_slice()),
            (2, &[3, 9][..])
        );
    }
    Ok(())
}

#[test]
fn relative_cx_without_configured_tetrahedral_centres_is_unchanged() -> anyhow::Result<()> {
    for molecule in ["CC(O)C(=O)O", "CCO", "C/C=C/C", "C[C@H](C)O"] {
        let absolute = super::read(molecule, &crate::budget::Budget::default())?;
        let relative = super::read(
            &format!("{molecule} |r|"),
            &crate::budget::Budget::default(),
        )?;
        assert!(relative.relative);
        assert!(relative.prepared.state.metadata.groups.is_empty());
        assert_eq!(
            serde_json::to_value(relative)?,
            serde_json::to_value(absolute)?,
            "{molecule}"
        );
        assert_eq!(
            super::stereoisomers(
                &format!("{molecule} |r|"),
                &crate::budget::Budget::default()
            )?,
            super::stereoisomers(molecule, &crate::budget::Budget::default())?,
            "{molecule}"
        );
    }
    Ok(())
}

#[test]
fn reaction_relative_cx_refuses_configured_centres_and_keeps_controls() -> anyhow::Result<()> {
    for molecule in [
        "C[C@H](O)C(=O)O",
        "C[C@@H](O)C(=O)O",
        "C[C@H](O)[C@@H](C)Cl",
        "C[C@H](O)[C@@H](O)C",
        "[H][C@](C)(O)C(=O)O",
    ] {
        let parsed = super::parse(molecule)?;
        assert!(
            matches!(
                super::reaction_part(
                    parsed,
                    vec![crate::cx::Event::RelativeStereo],
                    &crate::budget::Budget::default()
                ),
                Err(Error::Unsupported(
                    "reaction stereochemistry or radical count"
                ))
            ),
            "{molecule}"
        );
    }
    for molecule in [
        "CC(O)C(=O)O",
        "CCO",
        "C/C=C/C",
        "[H]C(O)(C)C(=O)O",
        "O[C@H]1CC[C@@H](O)CC1",
        "C[C@H](O)[C@H](O)C",
    ] {
        let absolute = super::reaction_part(
            super::parse(molecule)?,
            Vec::new(),
            &crate::budget::Budget::default(),
        )?;
        let relative = super::reaction_part(
            super::parse(molecule)?,
            vec![crate::cx::Event::RelativeStereo],
            &crate::budget::Budget::default(),
        )?;
        assert!(relative.relative);
        assert!(relative.prepared.state.metadata.groups.is_empty());
        assert_eq!(
            serde_json::to_value(relative)?,
            serde_json::to_value(absolute)?,
            "{molecule}"
        );
    }
    Ok(())
}

#[test]
fn stereochemistry_work_limit_keeps_its_actionable_message() {
    let message = crate::stereo::CIP_WORK_LIMIT_MESSAGE;
    assert_eq!(Error::Stereo(message.into()).to_string(), message);
}

#[test]
fn other_stereochemistry_errors_keep_the_smiles_context() {
    for message in [
        "Invalid CIP ranking index",
        "CIP ranking storage limit exceeded",
        "Atropisomer cleanup work limit exceeded",
    ] {
        assert_eq!(
            Error::Stereo(message.into()).to_string(),
            format!("SMILES stereochemistry: {message}")
        );
    }
}

#[test]
fn cx_binol_wedges_match_independent_axial_cip_labels() -> anyhow::Result<()> {
    // RDKit 2026.03.6 MOL/CX readers and AssignCIPLabels independently give
    // P for this ring wedge and M for its hash (chemistry coordinates: Y up).
    for (wedge, stereo, code) in [("wU", 6, "P"), ("wD", 7, "M")] {
        let imported = super::read(
            &binol(&format!(",{wedge}:0.13")),
            &crate::budget::Budget::default(),
        )?;
        let state = &imported.prepared.state;
        let axis = state
            .graph
            .bonds
            .iter()
            .position(|b| b.a == 0 && b.b == 1)
            .unwrap();
        assert_eq!(state.metadata.bonds[axis].stereo, stereo);
        let assigned = crate::stereo::cip::label::assign(state, &Default::default())?;
        assert_eq!(
            assigned.state.properties.bond_codes[axis].as_deref(),
            Some(code)
        );
        assert_eq!(imported.conformers.len(), 1);
        assert!(!imported.conformers[0].is_3d);
    }
    Ok(())
}

#[test]
fn cx_binol_needs_a_wedge_directly_connected_to_the_axis() -> anyhow::Result<()> {
    // An OH wedge distant from the axis, a wedge on the axis itself, and a
    // plain biaryl must not invent an axial configuration (IUPAC ST-1.7).
    for wedges in ["", ",wU:12.14", ",wD:12.14", ",wU:0.0"] {
        let imported = super::read(&binol(wedges), &crate::budget::Budget::default())?;
        assert!(
            imported
                .prepared
                .state
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7)),
            "{wedges}"
        );
    }
    Ok(())
}

#[test]
fn plain_smiles_can_serialize_an_axis_without_changing_the_input() -> anyhow::Result<()> {
    for wedge in ["wU", "wD"] {
        let imported = super::read(
            &binol(&format!(",{wedge}:0.13")),
            &crate::budget::Budget::default(),
        )?;
        let state = &imported.prepared.state;
        let before = serde_json::to_value(state)?;
        let output = super::write::write(state, Default::default())?;
        let reopened = super::read(&output.text, &crate::budget::Budget::default())?
            .prepared
            .state;
        assert_eq!(reopened.graph.atoms.len(), state.graph.atoms.len());
        assert_eq!(reopened.graph.bonds.len(), state.graph.bonds.len());
        assert!(
            reopened
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7))
        );
        assert_eq!(serde_json::to_value(state)?, before);
    }
    Ok(())
}

#[test]
fn smiles_output_order_keeps_axial_cx_coordinates_and_wedges_aligned() -> anyhow::Result<()> {
    use std::fmt::Write;
    for (wedge, code) in [("wU", "P"), ("wD", "M")] {
        let imported = super::read(
            &binol(&format!(",{wedge}:0.13")),
            &crate::budget::Budget::default(),
        )?;
        let state = &imported.prepared.state;
        let wedge_bond = state
            .graph
            .bonds
            .iter()
            .position(|b| b.a == 0 && b.b == 12)
            .unwrap();
        let output = super::write::write(state, Default::default())?;
        let mut cx = format!("{} |(", output.text);
        for (i, &atom) in output.atom_order.iter().enumerate() {
            if i != 0 {
                cx.push(';');
            }
            let p = imported.conformers[0].positions[atom];
            write!(cx, "{},{},", p.x, p.y)?;
        }
        let begin = output.atom_order.iter().position(|&a| a == 0).unwrap();
        let bond = output
            .bond_order
            .iter()
            .position(|&b| b == wedge_bond)
            .unwrap();
        write!(cx, "),{wedge}:{begin}.{bond}|")?;
        let reopened = super::read(&cx, &crate::budget::Budget::default())?
            .prepared
            .state;
        let labeled = crate::stereo::cip::label::assign(&reopened, &Default::default())?;
        let codes = labeled
            .state
            .properties
            .bond_codes
            .iter()
            .flatten()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(codes, [code], "{cx}");
    }
    Ok(())
}

#[test]
fn reaction_smiles_keeps_axial_bonds_and_rejects_other_atom_stereo_classes() -> anyhow::Result<()> {
    for stereo in [6, 7] {
        let mut parsed = super::parse(BINOL)?;
        parsed.metadata.bonds[0].stereo = stereo;
        let imported = super::reaction_part(parsed, Vec::new(), &crate::budget::Budget::default())?;
        assert_eq!(imported.prepared.state.metadata.bonds[0].stereo, stereo);
    }
    for chiral_tag in 3..=8 {
        let mut parsed = super::parse("F[C](Cl)(Br)I")?;
        parsed.metadata.atoms[1].chiral_tag = chiral_tag;
        assert!(
            super::reaction_part(parsed, Vec::new(), &crate::budget::Budget::default()).is_err(),
            "{chiral_tag}"
        );
    }
    Ok(())
}

#[test]
fn allene_smiles_classes_and_shorthand_keep_their_permutation() -> anyhow::Result<()> {
    for (text, permutation) in [
        ("FC(Cl)=[C@AL1]=C(Br)I", 1),
        ("FC(Cl)=[C@AL2]=C(Br)I", 2),
        ("FC(Cl)=[C@]=C(Br)I", 1),
        ("FC(Cl)=[C@@]=C(Br)I", 2),
        // The implicit H follows its terminal atom in the text: at the
        // second end it precedes F, but storage keeps absent H last.
        ("OC=[C@AL1]=CF", 2),
        ("OC=[C@AL2]=CF", 1),
    ] {
        let parsed = super::parse(text)?;
        let center = parsed
            .graph
            .atoms
            .iter()
            .enumerate()
            .find_map(|(a, _)| (parsed.metadata.atoms[a].chiral_tag != 0).then_some(a))
            .unwrap();
        assert_eq!(parsed.metadata.atoms[center].chiral_tag, 5, "{text}");
        assert_eq!(
            parsed.metadata.atoms[center].chiral_permutation,
            Some(permutation),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn allene_smiles_canonical_order_accounts_for_axial_permutations() -> anyhow::Result<()> {
    for texts in [
        &["CC=[C@AL1]=CC=[C@AL2]=CC", "CC=[C@AL2]=CC=[C@AL1]=CC"][..],
        &["FC(Cl)=[C@AL1]=C(Br)I", "ClC(F)=[C@AL2]=C(Br)I"][..],
        &["FC(Cl)=[C@AL1]=C(Br)C(Br)=[C@AL2]=C(F)Cl"][..],
        &["CCCC(CC)=[C@AL1]=C(CCCCC)CCCCCCC(CCCCC)=[C@AL2]=C(CC)CCC"][..],
    ] {
        let expected = super::write::write(
            &super::read(texts[0], &crate::budget::Budget::default())?
                .prepared
                .state,
            Default::default(),
        )?
        .text;
        for text in texts {
            let state = super::read(text, &crate::budget::Budget::default())?
                .prepared
                .state;
            assert_eq!(
                super::write::write(&state, Default::default())?.text,
                expected,
                "{text}"
            );
            for root in 0..state.graph.atoms.len() {
                let traversal = super::write::write(
                    &state,
                    super::write::Options {
                        canonical: false,
                        root: Some(root),
                        ..Default::default()
                    },
                )?;
                let reordered = super::read(&traversal.text, &crate::budget::Budget::default())?
                    .prepared
                    .state;
                assert_eq!(
                    super::write::write(&reordered, Default::default())?.text,
                    expected,
                    "{text}, root {root}: {}",
                    traversal.text
                );
            }
        }
    }
    Ok(())
}

#[test]
fn allene_smiles_writer_preserves_axial_cip_across_traversals() -> anyhow::Result<()> {
    // With each end ordered high/low, AL1 is P and AL2 is M. Each
    // high/low reversal changes that descriptor independently at either end.
    for (text, expected) in [
        ("FC(Cl)=[C@AL1]=C(Br)I", "P"),
        ("FC(Cl)=[C@AL2]=C(Br)I", "M"),
        ("ClC(F)=[C@AL2]=C(Br)I", "P"),
        ("C(=[C@AL1]=C(Br)I)(F)Cl", "P"),
        ("OC=[C@AL1]=CF", "M"),
        ("O[CH]=[C@AL1]=[CH]F", "M"),
        ("OC([H])=[C@AL1]=C([H])F", "M"),
        ("[H]C(O)=[C@AL2]=C(F)[H]", "P"),
        ("[2H]C(O)=[C@AL2]=C(F)[H]", "P"),
        ("FC1=[C@AL1]=C(Br)CCCC1", "P"),
    ] {
        let state = super::read(text, &crate::budget::Budget::default())?
            .prepared
            .state;
        let assigned = crate::stereo::cip::label::assign(&state, &Default::default())?;
        let code = assigned
            .state
            .properties
            .atoms
            .iter()
            .find_map(|a| a.cip_code.as_deref())
            .expect("specified allene has an axial CIP descriptor");
        assert_eq!(code, expected, "{text}");
        for root in 0..state.graph.atoms.len() {
            for canonical in [false, true] {
                let output = super::write::write(
                    &state,
                    super::write::Options {
                        canonical,
                        root: Some(root),
                        ..Default::default()
                    },
                )?;
                assert!(output.text.contains("@AL"), "{text}: {}", output.text);
                let reopened = super::read(&output.text, &crate::budget::Budget::default())?
                    .prepared
                    .state;
                let labeled = crate::stereo::cip::label::assign(&reopened, &Default::default())?;
                let codes = labeled
                    .state
                    .properties
                    .atoms
                    .iter()
                    .filter_map(|a| a.cip_code.as_deref())
                    .collect::<Vec<_>>();
                assert_eq!(codes, [code], "{text}, root {root}: {}", output.text);
            }
        }
    }
    Ok(())
}

#[test]
fn allene_smiles_explicit_hydrogens_and_bracket_hydrogens_agree() -> anyhow::Result<()> {
    let mut expected = None;
    for text in [
        "OC=[C@AL1]=CF",
        "O[CH]=[C@AL1]=[CH]F",
        "OC([H])=[C@AL1]=C([H])F",
        "[H]C(O)=[C@AL2]=C([H])F",
    ] {
        let state = super::read(text, &crate::budget::Budget::default())?
            .prepared
            .state;
        assert_eq!(state.graph.atoms.len(), 5, "{text}");
        let center = state
            .metadata
            .atoms
            .iter()
            .position(|a| a.chiral_tag == 5)
            .unwrap();
        assert_eq!(
            state.metadata.atoms[center].chiral_permutation,
            Some(2),
            "{text}"
        );
        let assigned = crate::stereo::cip::label::assign(&state, &Default::default())?;
        let code = assigned.state.properties.atoms[center].cip_code.clone();
        assert!(code.is_some(), "{text}");
        if let Some(expected) = &expected {
            assert_eq!(&code, expected, "{text}");
        } else {
            expected = Some(code);
        }
    }
    Ok(())
}

#[test]
fn allene_smiles_without_stereo_and_tetrahedral_stereo_are_unchanged() -> anyhow::Result<()> {
    let state = super::read("CC=C=CC", &crate::budget::Budget::default())?
        .prepared
        .state;
    assert!(state.metadata.atoms.iter().all(|a| a.chiral_tag == 0));
    assert_eq!(
        super::write::write(&state, Default::default())?.text,
        "CC=C=CC"
    );
    for text in ["N[C@H](O)C", "N[C@@H](O)C"] {
        let state = super::read(text, &crate::budget::Budget::default())?
            .prepared
            .state;
        let original = crate::stereo::cip::label::assign(&state, &Default::default())?;
        let code = original.state.properties.atoms[1].cip_code.as_deref();
        assert!(matches!(code, Some("R" | "S")));
        let output = super::write::write(&state, Default::default())?;
        let reopened = super::read(&output.text, &crate::budget::Budget::default())?
            .prepared
            .state;
        let assigned = crate::stereo::cip::label::assign(&reopened, &Default::default())?;
        assert!(
            assigned
                .state
                .properties
                .atoms
                .iter()
                .any(|a| a.cip_code.as_deref() == code)
        );
    }
    Ok(())
}

#[test]
fn allene_smiles_writes_exact_classes_and_rejects_unsupported_topologies() -> anyhow::Result<()> {
    for text in [
        "FC(Cl)=[C@AL1]=C(Br)I",
        "FC(Cl)=[C@AL2]=C(Br)I",
        "OC=[C@AL1]=CF",
        "OC=[C@AL2]=CF",
    ] {
        let state = super::read(text, &crate::budget::Budget::default())?
            .prepared
            .state;
        let output = super::write::write(
            &state,
            super::write::Options {
                canonical: false,
                root: Some(0),
                ..Default::default()
            },
        )?;
        assert_eq!(output.text, text);
        let reaction = super::reaction_part(
            super::parse(text)?,
            Vec::new(),
            &crate::budget::Budget::default(),
        )?;
        assert!(
            reaction
                .prepared
                .state
                .metadata
                .atoms
                .iter()
                .any(|a| a.chiral_tag == 5)
        );
    }
    for text in [
        "F[C@AL1](Cl)(Br)I",
        "CC=[C@AL]=CC",
        "FC=[C@AL1]=O",
        "C=[C@AL1]=CC",
        "CC=[C@AL1]=C=CC",
    ] {
        assert!(
            super::read(text, &crate::budget::Budget::default()).is_err(),
            "{text}"
        );
        assert!(
            super::reaction_part(
                super::parse(text)?,
                Vec::new(),
                &crate::budget::Budget::default()
            )
            .is_err(),
            "{text}"
        );
    }
    // An explicitly tetrahedral class retains its original raw class even at
    // a degree-two atom; only the shorthand selects AL from the topology.
    assert_eq!(
        super::parse("FC(Cl)=[C@TH1]=C(Br)I")?.metadata.atoms[3].chiral_tag,
        2
    );
    Ok(())
}

#[test]
fn raw_allene_tag_off_an_allene_centre_writes_no_winding() -> anyhow::Result<()> {
    // Raw states can carry tag 5 anywhere; RDKit writes no winding for it
    // outside ring stereo.
    for (text, atom) in [("C", 0), ("CC(C)(F)Cl", 1), ("CC=C", 1)] {
        let mut state = super::read(text, &crate::budget::Budget::default())?
            .prepared
            .state;
        state.metadata.atoms[atom].chiral_tag = 5;
        state.metadata.atoms[atom].chiral_permutation = Some(1);
        for root in 0..state.graph.atoms.len() {
            let options = super::write::Options {
                canonical: false,
                root: Some(root),
                ..Default::default()
            };
            assert!(
                !super::write::write(&state, options)?.text.contains('@'),
                "{text}, root {root}"
            );
        }
    }
    // Ring stereo rewrites any tag on its centres, tag 5 included, as RDKit's
    // canonicalizeFragment does: RDKit 2026.03.6 writes the same SMILES here.
    let mut ring = super::read("O[C@H]1CC[C@@H](O)CC1", &crate::budget::Budget::default())?
        .prepared
        .state;
    for atom in [1, 4] {
        ring.metadata.atoms[atom].chiral_tag = 5;
    }
    assert_eq!(
        super::write::write(&ring, Default::default())?.text,
        "O[C@H]1CC[C@@H](O)CC1"
    );
    Ok(())
}

#[test]
fn bridging_hydride_keeps_configuration() -> anyhow::Result<()> {
    // A bridging H⁻ continues to B, so it is not the ligand an implicit H is.
    for (first, second) in [
        ("B[H-]/C=C/F", "B[H-]/C=C\\F"),
        ("C[C@H](F)[1H-]B", "C[C@@H](F)[1H-]B"),
        ("C[C@H](F)[H-]B", "C[C@@H](F)[H-]B"),
        ("C[C@H](F)<-[1HH]", "C[C@@H](F)<-[1HH]"),
    ] {
        assert_ne!(
            super::stereoisomers(first, &crate::budget::Budget::default())?,
            super::stereoisomers(second, &crate::budget::Budget::default())?,
            "{first}"
        );
    }
    Ok(())
}

#[test]
fn terminal_hydride_is_no_stereo_ligand() -> anyhow::Result<()> {
    // Charge is no sequence-rule criterion: H⁻ and H tie (P-92.3).
    for (first, second) in [
        ("C[C@H](F)[H-]", "C[C@@H](F)[H-]"),
        ("C[C@H](F)<-[H-]", "C[C@@H](F)<-[H-]"),
        ("C[C@H](F)[H-] |H:1.2|", "C[C@@H](F)[H-] |H:1.2|"),
    ] {
        assert_eq!(
            super::stereoisomers(first, &crate::budget::Budget::default())?,
            super::stereoisomers(second, &crate::budget::Budget::default())?,
            "{first}"
        );
        for smiles in [first, second] {
            let state = super::read(smiles, &crate::budget::Budget::default())?
                .prepared
                .state;
            assert_eq!(state.metadata.atoms[1].chiral_tag, 0, "{smiles}");
            assert!(state.properties.atoms[1].cip_code.is_none(), "{smiles}");
        }
    }
    Ok(())
}
