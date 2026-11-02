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
