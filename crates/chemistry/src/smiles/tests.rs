use super::Error;

const BINOL: &str = "c1(-c2c(O)c3c(cc2)cccc3)c(O)c2c(cc1)cccc2";
const BINOL_COORDINATES: &str = "0,0,;1.5,0,;2.25,-1.299,;1.5,-2.598,;3.75,-1.299,;4.5,0,;3.75,1.299,;2.25,1.299,;6,0,;6.75,-1.299,;6,-2.598,;4.5,-2.598,;-0.75,1.299,;0,2.598,;-2.25,1.299,;-3,0,;-2.25,-1.299,;-0.75,-1.299,;-4.5,0,;-5.25,1.299,;-4.5,2.598,;-3,2.598,";

fn binol(wedges: &str) -> String {
    format!("{BINOL} |({BINOL_COORDINATES}){wedges}|")
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
        let imported = super::read(&binol(&format!(",{wedge}:0.13")))?;
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
        let imported = super::read(&binol(wedges))?;
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
        let imported = super::read(&binol(&format!(",{wedge}:0.13")))?;
        let state = &imported.prepared.state;
        let before = serde_json::to_value(state)?;
        let output = super::write::write(state, Default::default())?;
        let reopened = super::read(&output.text)?.prepared.state;
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
        let imported = super::read(&binol(&format!(",{wedge}:0.13")))?;
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
        let reopened = super::read(&cx)?.prepared.state;
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
        let imported = super::reaction_part(parsed, Vec::new())?;
        assert_eq!(imported.prepared.state.metadata.bonds[0].stereo, stereo);
    }
    for chiral_tag in 3..=8 {
        let mut parsed = super::parse("F[C](Cl)(Br)I")?;
        parsed.metadata.atoms[1].chiral_tag = chiral_tag;
        assert!(
            super::reaction_part(parsed, Vec::new()).is_err(),
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
        let expected =
            super::write::write(&super::read(texts[0])?.prepared.state, Default::default())?.text;
        for text in texts {
            let state = super::read(text)?.prepared.state;
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
                let reordered = super::read(&traversal.text)?.prepared.state;
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
        let state = super::read(text)?.prepared.state;
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
                let reopened = super::read(&output.text)?.prepared.state;
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
        let state = super::read(text)?.prepared.state;
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
    let state = super::read("CC=C=CC")?.prepared.state;
    assert!(state.metadata.atoms.iter().all(|a| a.chiral_tag == 0));
    assert_eq!(
        super::write::write(&state, Default::default())?.text,
        "CC=C=CC"
    );
    for text in ["N[C@H](O)C", "N[C@@H](O)C"] {
        let state = super::read(text)?.prepared.state;
        let original = crate::stereo::cip::label::assign(&state, &Default::default())?;
        let code = original.state.properties.atoms[1].cip_code.as_deref();
        assert!(matches!(code, Some("R" | "S")));
        let output = super::write::write(&state, Default::default())?;
        let reopened = super::read(&output.text)?.prepared.state;
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
        let state = super::read(text)?.prepared.state;
        let output = super::write::write(
            &state,
            super::write::Options {
                canonical: false,
                root: Some(0),
                ..Default::default()
            },
        )?;
        assert_eq!(output.text, text);
        let reaction = super::reaction_part(super::parse(text)?, Vec::new())?;
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
        assert!(super::read(text).is_err(), "{text}");
        assert!(
            super::reaction_part(super::parse(text)?, Vec::new()).is_err(),
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
