use super::*;

fn state(text: &str) -> anyhow::Result<State> {
    Ok(crate::smiles::read(text)?.prepared.state)
}

#[test]
fn mancude_integer_averages_and_ring_only_relaxation() -> anyhow::Result<()> {
    for (text, atom, expected) in [
        ("F[C@H](c1[o+]cccc1)C(O)N", 1, "S"),
        ("[N+]1(c2ccccc2)=C([C@H](F)C(=N)C)C=C(c2ccccc2)CC1", 8, "R"),
    ] {
        let result = assign(&state(text)?, &Options::default())?;
        assert_eq!(
            result.state.properties.atoms[atom].cip_code.as_deref(),
            Some(expected),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn unknown_isotope_mass_uses_its_mass_number() -> anyhow::Result<()> {
    let result = assign(&state("[999CH3][C@H]([14CH3])F")?, &Options::default())?;
    assert_eq!(
        result.state.properties.atoms[1].cip_code.as_deref(),
        Some("S")
    );
    Ok(())
}

#[test]
fn ring_visits_beyond_255_keep_their_distance() -> anyhow::Result<()> {
    let text = format!("F[C@H]({}C1CC1){}C1CCC1", "C".repeat(254), "C".repeat(254));
    let result = assign(
        &state(&text)?,
        &Options {
            max_iterations: 100_000,
            ..Default::default()
        },
    )?;
    assert_eq!(
        result.state.properties.atoms[1].cip_code.as_deref(),
        Some("R")
    );
    Ok(())
}

#[test]
fn partial_labeling_uses_unselected_auxiliary_configurations() -> anyhow::Result<()> {
    let mut input = state(r"C\C=C/[C@@H](\C=C\O)[C@H](C)[C@H](\C=C/C)\C=C\O")?;
    for atom in &mut input.properties.atoms {
        atom.cip_code = None;
    }
    input.properties.bond_codes.fill(None);
    let result = assign(
        &input,
        &Options {
            atoms: Some(vec![7]),
            bonds: Some(vec![]),
            ..Default::default()
        },
    )?;
    assert_eq!(result.labels.len(), 1);
    assert_eq!(result.labels[0].target, Target::Atom(7));
    for (atom, properties) in result.state.properties.atoms.iter().enumerate() {
        assert_eq!(properties.cip_code.as_deref(), (atom == 7).then_some("r"));
    }
    assert!(
        result
            .state
            .properties
            .bond_codes
            .iter()
            .all(Option::is_none)
    );
    Ok(())
}

#[test]
fn preliminary_comparisons_share_the_exact_caller_budget() -> anyhow::Result<()> {
    let input = state("C[C@H](F)Cl")?;
    let before = serde_json::to_value(&input)?;
    for max_iterations in 1..=3 {
        assert!(matches!(
            assign(
                &input,
                &Options {
                    max_iterations,
                    ..Default::default()
                }
            ),
            Err(Error::Iterations)
        ));
        assert_eq!(serde_json::to_value(&input)?, before);
    }
    for max_iterations in [4, 0] {
        let result = assign(
            &input,
            &Options {
                max_iterations,
                ..Default::default()
            },
        )?;
        assert_eq!(
            result.state.properties.atoms[1].cip_code.as_deref(),
            Some("R")
        );
    }
    Ok(())
}

#[test]
fn preliminary_budget_is_shared_between_configurations() -> anyhow::Result<()> {
    let input = state("C[C@H](F)Cl.C[C@H](F)Cl")?;
    let before = serde_json::to_value(&input)?;
    for max_iterations in [4, 7] {
        assert!(matches!(
            assign(
                &input,
                &Options {
                    max_iterations,
                    ..Default::default()
                }
            ),
            Err(Error::Iterations)
        ));
        assert_eq!(serde_json::to_value(&input)?, before);
    }
    let result = assign(
        &input,
        &Options {
            max_iterations: 8,
            ..Default::default()
        },
    )?;
    assert_eq!(result.labels.len(), 2);
    for atom in [1, 5] {
        assert_eq!(
            result.state.properties.atoms[atom].cip_code.as_deref(),
            Some("R")
        );
    }
    Ok(())
}

#[test]
fn auxiliary_ties_preserve_configuration_order_and_the_later_label() -> anyhow::Result<()> {
    let input = state("C[C@H](F)[C@H](Cl)Br")?;
    let opposite = state("C[C@H](F)[C@@H](Cl)Br")?;
    for (reverse, expected) in [(false, Descriptor::S), (true, Descriptor::R)] {
        let mut molecule = Molecule::new(&input)?;
        let opposite = Molecule::new(&opposite)?;
        let center = Configuration::tetrahedral(&molecule, 1)?;
        let first = Configuration::tetrahedral(&molecule, 3)?;
        let last = Configuration::tetrahedral(&opposite, 3)?;
        let mut configurations = if reverse {
            vec![center, last, first]
        } else {
            vec![center, first, last]
        };
        let mut graph = configurations[0].make_digraph(&molecule)?;
        graph.edges(&mut molecule, graph.original_root())?;
        auxiliary(
            &mut configurations,
            0,
            &mut molecule,
            &mut graph,
            &Rules::full(),
            &mut Iterations::new(0),
        )?;
        for node in graph.nodes_for_atom(&mut molecule, 3)? {
            if !graph.node(node)?.is_duplicate() {
                assert_eq!(graph.node(node)?.aux, expected);
            }
        }
    }
    Ok(())
}

#[test]
fn atrop_lowercase_requires_exactly_one_pseudo_end() -> anyhow::Result<()> {
    for (text, expected) in [
        (
            "F[C@H](Cl)C([C@@H](F)Cl)C([C@H](F)Cl)[C@@H](F)Cl",
            ["P", "M"],
        ),
        ("F[C@H](Cl)C([C@@H](F)Cl)C(F)Cl", ["m", "p"]),
    ] {
        for (stereo, code) in [6, 7].into_iter().zip(expected) {
            let mut input = state(text)?;
            let axis = input
                .graph
                .bonds
                .iter()
                .position(|b| (b.a, b.b) == (3, 7))
                .ok_or_else(|| anyhow::anyhow!("Missing atrop axis"))?;
            input.metadata.bonds[axis].stereo = stereo;
            let result = assign(&input, &Options::default())?;
            assert_eq!(
                result.state.properties.bond_codes[axis].as_deref(),
                Some(code)
            );
        }
    }
    Ok(())
}

#[test]
fn removed_stereo_and_tied_centers_clear_old_labels() -> anyhow::Result<()> {
    let labeled = assign(&state("C[C@H](F)Cl")?, &Options::default())?.state;
    assert_eq!(labeled.properties.atoms[1].cip_code.as_deref(), Some("R"));
    for remove_tag in [true, false] {
        let mut input = labeled.clone();
        if remove_tag {
            input.metadata.atoms[1].chiral_tag = 0;
        } else {
            input.graph.atoms[2].atomic_number = 17;
        }
        let result = assign(&input, &Options::default())?;
        assert!(result.state.properties.atoms[1].cip_code.is_none());
        assert!(result.labels.is_empty());
    }
    let mut input = assign(&state("F/C=C/Cl")?, &Options::default())?.state;
    assert_eq!(input.properties.bond_codes[1].as_deref(), Some("E"));
    input.metadata.bonds[1].stereo = 0;
    let result = assign(&input, &Options::default())?;
    assert!(result.state.properties.bond_codes[1].is_none());
    assert!(result.labels.is_empty());
    Ok(())
}

#[test]
fn partial_labeling_clears_selected_nonstereo_outputs_only() -> anyhow::Result<()> {
    let mut input = state("C[C@H](F)Cl")?;
    input.properties.atoms[0].cip_code = Some("stale".into());
    input.properties.atoms[1].cip_code = Some("keep".into());
    let result = assign(
        &input,
        &Options {
            atoms: Some(vec![0]),
            ..Default::default()
        },
    )?;
    assert!(result.state.properties.atoms[0].cip_code.is_none());
    assert_eq!(
        result.state.properties.atoms[1].cip_code.as_deref(),
        Some("keep")
    );
    assert!(result.labels.is_empty());
    Ok(())
}

#[test]
fn empty_selections_keep_boost_python_full_selection_semantics() -> anyhow::Result<()> {
    let result = assign(
        &state("C[C@H](F)Cl")?,
        &Options {
            atoms: Some(vec![]),
            bonds: Some(vec![]),
            ..Default::default()
        },
    )?;
    assert_eq!(
        result.state.properties.atoms[1].cip_code.as_deref(),
        Some("R")
    );
    Ok(())
}

#[test]
fn malformed_tetrahedral_and_bond_markers_are_skipped() -> anyhow::Result<()> {
    let mut input = state("CF")?;
    input.metadata.atoms[0].chiral_tag = 1;
    input.properties.atoms[0].cip_code = Some("stale".into());
    let result = assign(&input, &Options::default())?;
    assert!(result.state.properties.atoms[0].cip_code.is_none());
    assert!(result.labels.is_empty());
    for (text, index, stereo, carriers) in [
        ("CCCC", 1, 4, vec![0, 3]),
        ("CC", 0, 6, vec![]),
        ("FC=CCl", 1, 6, vec![]),
    ] {
        let mut input = state(text)?;
        input.metadata.bonds[index].stereo = stereo;
        input.metadata.bonds[index].stereo_atoms = carriers;
        input.properties.bond_codes[index] = Some("stale".into());
        let result = assign(&input, &Options::default())?;
        assert!(
            result.state.properties.bond_codes[index].is_none(),
            "{text}"
        );
        assert!(result.labels.is_empty());
    }
    Ok(())
}

#[test]
fn malformed_sp2_carriers_are_skipped_without_changing_metadata() -> anyhow::Result<()> {
    for (text, index, carriers, expected) in [
        ("FC=CCl", 1, [0, 3], None),
        ("c1ccccc1C=CCl", 6, [5, 8], None),
        ("F[C@H](Cl)c1ccccc1C=CCl", 9, [8, 11], Some("R")),
    ] {
        let original = state(text)?;
        for invalid in [
            vec![original.graph.atoms.len() + 1, carriers[1]],
            vec![carriers[0] + 2, carriers[1]],
            vec![carriers[1], carriers[0]],
            vec![carriers[0], carriers[0]],
        ] {
            let mut input = original.clone();
            input.metadata.bonds[index].stereo = 4;
            input.metadata.bonds[index].stereo_atoms = invalid;
            input.properties.bond_codes[index] = Some("stale".into());
            let before = serde_json::to_value(&input)?;
            let result = assign(&input, &Options::default())?;
            assert!(
                result.state.properties.bond_codes[index].is_none(),
                "{text}"
            );
            assert_eq!(result.labels.len(), usize::from(expected.is_some()));
            assert_eq!(
                result.state.properties.atoms[1].cip_code.as_deref(),
                expected
            );
            assert_eq!(serde_json::to_value(&input)?, before);
            assert_eq!(
                serde_json::to_value(&result.state.metadata)?,
                serde_json::to_value(&input.metadata)?
            );
        }
    }
    Ok(())
}
