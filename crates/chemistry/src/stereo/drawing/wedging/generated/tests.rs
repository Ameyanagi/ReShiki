use super::*;
use crate::{depict, smiles};

type TestResult = anyhow::Result<()>;

fn prepared(text: &str) -> anyhow::Result<(perception::State, Conformer)> {
    let state = smiles::prepare(text)?.state;
    let ranks = state
        .properties
        .atoms
        .iter()
        .map(|a| a.cip_rank)
        .collect::<Vec<_>>();
    let conformer = depict::compute(&state, &ranks, None, Default::default())?;
    Ok((state, conformer))
}

fn properties(state: &perception::State) -> WedgeProperties {
    WedgeProperties {
        valences: state.valences.clone(),
        attachment_points: vec![false; state.graph.atoms.len()],
    }
}

fn wedge_state(state: &perception::State) -> WedgeState {
    WedgeState {
        graph: state.graph.clone(),
        metadata: state.metadata.clone(),
        directions: vec![Direction::None; state.graph.bonds.len()],
        rings: state.rings.clone(),
    }
}

// The reader removes ordinary graph H atoms so this compares the same hydrogen
// representation while retaining every tetrahedral configuration.
fn canonical(state: &perception::State) -> anyhow::Result<String> {
    let first = smiles::write::write(state, Default::default())?.text;
    let normalized = smiles::prepare(&first)?.state;
    Ok(smiles::write::write(&normalized, Default::default())?.text)
}

fn perceived(
    state: &perception::State,
    wedged: &WedgeState,
    conf: &Conformer,
) -> anyhow::Result<perception::State> {
    let seen = from_directions(
        &wedged.graph,
        &crate::ranking::Metadata::unspecified(&wedged.graph),
        &wedged.directions,
        Some(&conf.positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    let mut result = state.clone();
    result.graph = seen.graph;
    result.metadata = seen.metadata;
    result.valences = seen.valences;
    result.directions = wedged.directions.clone();
    result.properties = perception::Properties::unspecified(&result.graph);
    perception::perceive(
        &result,
        perception::Options {
            clean: false,
            force: true,
            flag_possible: true,
        },
    )
    .map_err(anyhow::Error::msg)
}

#[test]
fn norcamphor_and_substituted_norbornane_use_shortest_bridge_preserving_stereo() -> TestResult {
    for text in ["O=C1C[C@H]2CC[C@@H]1C2", "C[C@H]1C[C@H]2CC[C@@H]1C2"] {
        let (state, conf) = prepared(text)?;
        let props = properties(&state);
        let raw = super::super::wedge_molecule(&wedge_state(&state), &props, Some(&conf), false)
            .map_err(anyhow::Error::msg)?;
        let result = prefer_ring_wedges(&raw, &props, &conf).map_err(anyhow::Error::msg)?;
        let mut work = Work(1_000_000);
        let top = Topology::new(&state.graph, &mut work).map_err(anyhow::Error::msg)?;
        let mut checked = 0;
        for atom in 0..state.graph.atoms.len() {
            if !tetrahedral(state.metadata.atoms[atom].chiral_tag)
                || !matches!(
                    top.junction(atom, &mut work).map_err(anyhow::Error::msg)?,
                    Junction::Bridgehead
                )
                || top.adjacent[atom].len() != 3
            {
                continue;
            }
            let outgoing = top.adjacent[atom]
                .iter()
                .filter(|&&(_, bond)| {
                    result.graph.bonds[bond].a == atom && wedged(result.directions[bond])
                })
                .collect::<Vec<_>>();
            assert_eq!(outgoing.len(), 1, "{text}, atom {atom}");
            let shortest = top.ring_adjacent[atom]
                .iter()
                .map(|&(neighbor, _)| {
                    top.bridge_length(atom, neighbor, &mut work)
                        .map_err(anyhow::Error::msg)
                })
                .collect::<anyhow::Result<Vec<_>>>()?
                .into_iter()
                .min()
                .unwrap();
            assert_eq!(
                top.bridge_length(atom, outgoing[0].0, &mut work)
                    .map_err(anyhow::Error::msg)?,
                shortest,
                "{text}, atom {atom}"
            );
            assert_eq!(shortest, 2);
            checked += 1;
        }
        assert_eq!(checked, 2, "{text}");
        assert_eq!(
            canonical(&state)?,
            canonical(&perceived(&state, &result, &conf)?)?,
            "{text}"
        );
        assert_eq!(
            serde_json::to_value(&raw.metadata)?,
            serde_json::to_value(&result.metadata)?
        );
    }
    Ok(())
}

#[test]
fn bridgehead_with_acyclic_substituent_retains_raw_choice() -> TestResult {
    let (state, conf) = prepared("O=C1C[C@]2(C)CC[C@@H]1C2")?;
    let props = properties(&state);
    let raw = super::super::wedge_molecule(&wedge_state(&state), &props, Some(&conf), false)
        .map_err(anyhow::Error::msg)?;
    let result = prefer_ring_wedges(&raw, &props, &conf).map_err(anyhow::Error::msg)?;
    let center = state
        .graph
        .atoms
        .iter()
        .enumerate()
        .find_map(|(atom, _)| {
            let degree = state
                .graph
                .bonds
                .iter()
                .filter(|bond| bond.a == atom || bond.b == atom)
                .count();
            (tetrahedral(state.metadata.atoms[atom].chiral_tag) && degree == 4).then_some(atom)
        })
        .unwrap();
    for (id, bond) in raw
        .graph
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, bond)| bond.a == center || bond.b == center)
    {
        assert_eq!(result.directions[id], raw.directions[id]);
        assert_eq!(result.graph.bonds[id], *bond);
    }
    Ok(())
}

#[test]
fn spiro_has_opposite_wedges_on_one_ring_and_other_ring_plain() -> TestResult {
    let (state, conf) = prepared("C1CO[C@]2(C1)CCNC2")?;
    let props = properties(&state);
    let raw = super::super::wedge_molecule(&wedge_state(&state), &props, Some(&conf), false)
        .map_err(anyhow::Error::msg)?;
    let result = prefer_ring_wedges(&raw, &props, &conf).map_err(anyhow::Error::msg)?;
    let center = state
        .metadata
        .atoms
        .iter()
        .position(|atom| tetrahedral(atom.chiral_tag))
        .unwrap();
    let mut work = Work(1_000_000);
    let top = Topology::new(&state.graph, &mut work).map_err(anyhow::Error::msg)?;
    let pairs = top
        .spiro_pairs(center, &mut work)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(pairs.len(), 2);
    let pair_directions = pairs
        .iter()
        .map(|pair| pair.map(|bond| result.directions[bond]))
        .collect::<Vec<_>>();
    assert!(pair_directions.contains(&[Direction::None, Direction::None]));
    assert!(
        pair_directions
            .iter()
            .any(|pair| pair.contains(&Direction::Wedge) && pair.contains(&Direction::Hash))
    );
    assert_eq!(
        canonical(&state)?,
        canonical(&perceived(&state, &result, &conf)?)?
    );
    Ok(())
}

#[test]
fn trans_decalin_and_steroid_fusion_hydrogens_preserve_chemistry() -> TestResult {
    for (text, expected) in [
        ("C1CC[C@H]2CCCC[C@@H]2C1", 2),
        (
            "C[C@]12CC[C@H]3[C@@H]([C@@H]1CC[C@@H]2O)CCC4=CC(=O)CC[C@]34C",
            3,
        ),
    ] {
        let (state, conf) = prepared(text)?;
        let before = serde_json::to_value(&state)?;
        let result =
            add_fusion_hydrogens(&state, &conf.positions, 1.5).map_err(anyhow::Error::msg)?;
        assert_eq!(
            result.parents.len(),
            expected,
            "{text}, {:?}",
            result.parents
        );
        assert_eq!(canonical(&state)?, canonical(&result.state)?, "{text}");
        assert_eq!(result.positions[..state.graph.atoms.len()], conf.positions);
        for (index, &parent) in result.parents.iter().enumerate() {
            let hydrogen = state.graph.atoms.len() + index;
            assert_eq!(result.state.graph.atoms[hydrogen].atomic_number, 1);
            let vector = result.positions[hydrogen].sub(result.positions[parent]);
            assert!((vector.squared().sqrt() - 1.5).abs() < 1e-10);
            let mut work = Work(1_000_000);
            let top = Topology::new(&state.graph, &mut work).map_err(anyhow::Error::msg)?;
            let Junction::Fusion(shared) = top
                .junction(parent, &mut work)
                .map_err(anyhow::Error::msg)?
            else {
                panic!("not a fusion")
            };
            assert!(vector.dot(conf.positions[parent].sub(conf.positions[shared])) > 0.0);
            assert_eq!(result.state.graph.atoms[parent].explicit_hydrogens, 0);
            assert_eq!(result.state.valences[parent].implicit_hydrogens, 0);
        }
        let expanded_conf = Conformer {
            positions: result.positions,
            is_3d: false,
        };
        let props = properties(&result.state);
        let raw = super::super::wedge_molecule(
            &wedge_state(&result.state),
            &props,
            Some(&expanded_conf),
            false,
        )
        .map_err(anyhow::Error::msg)?;
        for (index, &parent) in result.parents.iter().enumerate() {
            let bond = state.graph.bonds.len() + index;
            assert!(wedged(raw.directions[bond]), "{text}, parent {parent}");
            assert_eq!(raw.graph.bonds[bond].a, parent);
        }
        assert_eq!(
            canonical(&state)?,
            canonical(&perceived(&result.state, &raw, &expanded_conf)?)?,
            "{text}"
        );
        assert_eq!(serde_json::to_value(&state)?, before);
    }
    Ok(())
}

#[test]
fn bridgeheads_and_existing_explicit_hydrogens_are_not_expanded() -> TestResult {
    let (state, conf) = prepared("O=C1C[C@H]2CC[C@@H]1C2")?;
    let result = add_fusion_hydrogens(&state, &conf.positions, 1.5).map_err(anyhow::Error::msg)?;
    assert!(result.parents.is_empty());
    // Cubane corners share a bond with each of three rings: cage, not fusion.
    let (state, conf) = prepared("F[C@]12[C@H]3[C@@H]4[C@@H]5[C@H]3[C@@]1(Cl)[C@]5(Br)[C@@H]42")?;
    let result = add_fusion_hydrogens(&state, &conf.positions, 1.5).map_err(anyhow::Error::msg)?;
    assert!(result.parents.is_empty());
    let (state, conf) = prepared("C1CC[C@H]2CCCC[C@@H]2C1")?;
    let result = add_fusion_hydrogens(&state, &conf.positions, 1.5).map_err(anyhow::Error::msg)?;
    let again =
        add_fusion_hydrogens(&result.state, &result.positions, 1.5).map_err(anyhow::Error::msg)?;
    assert!(again.parents.is_empty());
    assert_eq!(again.state.graph, result.state.graph);
    Ok(())
}

#[test]
fn invalid_input_is_rejected_without_mutation() -> TestResult {
    let (state, conf) = prepared("C1CC[C@H]2CCCC[C@@H]2C1")?;
    let before = serde_json::to_value(&state)?;
    for length in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(add_fusion_hydrogens(&state, &conf.positions, length).is_err());
    }
    assert!(add_fusion_hydrogens(&state, &[], 1.5).is_err());
    let mut bad = conf.clone();
    bad.positions[0].x = f64::NAN;
    assert!(prefer_ring_wedges(&wedge_state(&state), &properties(&state), &bad).is_err());
    assert_eq!(serde_json::to_value(&state)?, before);
    Ok(())
}
