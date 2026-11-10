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
fn wavy_end_wedging_preserves_incoming_unknown_and_visible_configuration() -> TestResult {
    let (state, conf) = prepared("F[C@](Cl)(Br)I")?;
    let props = properties(&state);
    let mut input = wedge_state(&state);
    input.directions[0] = Direction::Unknown;
    assert_eq!(input.graph.bonds[0].b, 1);
    let before = serde_json::to_value(&input)?;
    let result = wedge_molecule(&input, &props, &conf, false).map_err(anyhow::Error::msg)?;
    assert_eq!(result.directions[0], Direction::Unknown);
    assert_eq!(result.graph.bonds[0], input.graph.bonds[0]);
    assert_eq!(
        result
            .graph
            .bonds
            .iter()
            .zip(&result.directions)
            .filter(|(bond, direction)| bond.a == 1 && wedged(**direction))
            .count(),
        1
    );
    assert_eq!(
        canonical(&state)?,
        canonical(&perceived(&state, &result, &conf)?)?
    );
    let repeated = super::super::wedge_molecule(&result, &props, Some(&conf), false)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(repeated.directions, result.directions);
    assert_eq!(repeated.graph.bonds, result.graph.bonds);
    assert_eq!(serde_json::to_value(&input)?, before);
    Ok(())
}

#[test]
fn remote_wavy_bond_keeps_many_independent_centers_within_work_budget() -> TestResult {
    let fragment = smiles::prepare("F[C@H](Cl)Br")?.state;
    let centers = 4_000;
    let mut graph = Graph {
        atoms: Vec::with_capacity(4 * centers + 2),
        bonds: Vec::with_capacity(3 * centers + 1),
    };
    let mut positions = Vec::with_capacity(4 * centers + 2);
    for index in 0..centers {
        let offset = graph.atoms.len();
        graph.atoms.extend(fragment.graph.atoms.iter().cloned());
        graph
            .bonds
            .extend(fragment.graph.bonds.iter().map(|bond| Bond {
                a: bond.a + offset,
                b: bond.b + offset,
                ..bond.clone()
            }));
        for (x, y) in [
            (-1.0, 0.0),
            (0.0, 0.0),
            (0.5, 3f64.sqrt() / 2.0),
            (0.5, -3f64.sqrt() / 2.0),
        ] {
            positions.push(Point3 {
                x: x + index as f64 * 4.0,
                y,
                z: 0.0,
            });
        }
    }
    let offset = graph.atoms.len();
    graph.atoms.resize(
        offset + 2,
        Atom {
            atomic_number: 6,
            ..Atom::default()
        },
    );
    graph.bonds.push(Bond {
        a: offset,
        b: offset + 1,
        order: 1,
        aromatic: false,
    });
    for x in [0.0, 1.0] {
        positions.push(Point3 {
            x: x + centers as f64 * 4.0,
            y: 0.0,
            z: 0.0,
        });
    }
    let mut metadata = crate::ranking::Metadata::unspecified(&graph);
    for index in 0..centers {
        metadata.atoms[4 * index + 1].chiral_tag = fragment.metadata.atoms[1].chiral_tag;
    }
    let props = WedgeProperties {
        valences: graph.provisional_valences().map_err(anyhow::Error::msg)?,
        attachment_points: vec![false; graph.atoms.len()],
    };
    let mut directions = vec![Direction::None; graph.bonds.len()];
    directions[3 * centers] = Direction::Unknown;
    let input = WedgeState {
        graph,
        metadata,
        directions,
        rings: perception::RingCache {
            kind: perception::RingKind::Basis,
            atoms: vec![],
        },
    };
    let conf = Conformer {
        positions,
        is_3d: false,
    };
    let result = wedge_molecule(&input, &props, &conf, false).map_err(anyhow::Error::msg)?;
    assert_eq!(result.directions[3 * centers], Direction::Unknown);
    assert_eq!(
        result.graph.bonds[3 * centers],
        input.graph.bonds[3 * centers]
    );
    let mut outgoing = vec![0; input.graph.atoms.len()];
    for (bond, &direction) in result.graph.bonds.iter().zip(&result.directions) {
        if wedged(direction) {
            outgoing[bond.a] += 1;
        }
    }
    assert_eq!(outgoing.iter().sum::<usize>(), centers);
    for index in 0..centers {
        assert_eq!(outgoing[4 * index + 1], 1, "center {index}");
    }
    Ok(())
}

#[test]
fn wavy_end_wedging_honors_begin_and_exhausted_candidates() -> TestResult {
    let (state, conf) = prepared("F[C@](Cl)(Br)I")?;
    let props = properties(&state);
    let mut input = wedge_state(&state);
    input.directions[0] = Direction::Unknown;
    let edge = &mut input.graph.bonds[0];
    std::mem::swap(&mut edge.a, &mut edge.b);
    let result = wedge_molecule(&input, &props, &conf, false).map_err(anyhow::Error::msg)?;
    assert_eq!(
        serde_json::to_value(&result)?,
        serde_json::to_value(&input)?
    );

    // No wavy bond starts at the tagged atom, but every possible wedge bond
    // already carries a chemical unknown that must remain visible.
    for (bond, direction) in input.graph.bonds.iter_mut().zip(&mut input.directions) {
        if bond.a == 1 {
            std::mem::swap(&mut bond.a, &mut bond.b);
        }
        *direction = Direction::Unknown;
    }
    let before = serde_json::to_value(&input)?;
    let result = wedge_molecule(&input, &props, &conf, false).map_err(anyhow::Error::msg)?;
    assert_eq!(serde_json::to_value(&result)?, before);
    assert_eq!(serde_json::to_value(&input)?, before);
    Ok(())
}

#[test]
fn wavy_end_wedging_after_fusion_hydrogen_expansion() -> TestResult {
    let (mut state, conf) = prepared("F[C@](Cl)(Br)I.C1CC[C@H]2CCCC[C@@H]2C1")?;
    state.directions[0] = Direction::Unknown;
    let expanded =
        add_fusion_hydrogens(&state, &conf.positions, 1.5).map_err(anyhow::Error::msg)?;
    assert_eq!(expanded.parents.len(), 2);
    let state = expanded.state;
    let conf = Conformer {
        positions: expanded.positions,
        is_3d: false,
    };
    let props = properties(&state);
    let mut input = wedge_state(&state);
    input.directions = state.directions.clone();
    let result = wedge_molecule(&input, &props, &conf, false).map_err(anyhow::Error::msg)?;
    assert_eq!(result.directions[0], Direction::Unknown);
    assert_eq!(result.graph.bonds[0], input.graph.bonds[0]);
    assert_eq!(result.directions.iter().filter(|&&d| wedged(d)).count(), 3);
    for &parent in &expanded.parents {
        assert!(
            result
                .graph
                .bonds
                .iter()
                .zip(&result.directions)
                .any(|(bond, &direction)| {
                    bond.a == parent
                        && state.graph.atoms[bond.b].atomic_number == 1
                        && wedged(direction)
                })
        );
    }
    assert_eq!(
        canonical(&state)?,
        canonical(&perceived(&state, &result, &conf)?)?
    );
    Ok(())
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
fn bridgehead_preference_preserves_wavy_shortest_bridge() -> TestResult {
    let (state, conf) = prepared("O=C1C[C@H]2CC[C@@H]1C2")?;
    let props = properties(&state);
    let mut input = super::super::wedge_molecule(&wedge_state(&state), &props, Some(&conf), false)
        .map_err(anyhow::Error::msg)?;
    let preferred = prefer_ring_wedges(&input, &props, &conf).map_err(anyhow::Error::msg)?;
    let mut work = Work(1_000_000);
    let top = Topology::new(&state.graph, &mut work).map_err(anyhow::Error::msg)?;
    let marker = preferred
        .graph
        .bonds
        .iter()
        .enumerate()
        .find_map(|(bond, edge)| {
            (wedged(preferred.directions[bond])
                && matches!(top.junction(edge.a, &mut work), Ok(Junction::Bridgehead)))
            .then_some(bond)
        })
        .unwrap();
    // Point the unknown away from its untagged neighbor, toward the center
    // that would otherwise select this shortest bridge.
    input.graph.bonds[marker].a = preferred.graph.bonds[marker].b;
    input.graph.bonds[marker].b = preferred.graph.bonds[marker].a;
    input.directions[marker] = Direction::Unknown;
    let before = serde_json::to_value(&input)?;
    let result = prefer_ring_wedges(&input, &props, &conf).map_err(anyhow::Error::msg)?;
    assert_eq!(result.directions[marker], Direction::Unknown);
    assert_eq!(result.graph.bonds[marker], input.graph.bonds[marker]);
    assert_eq!(
        canonical(&state)?,
        canonical(&perceived(&state, &result, &conf)?)?
    );
    assert_eq!(serde_json::to_value(&input)?, before);
    Ok(())
}

#[test]
fn bridgehead_preference_without_waves_retains_raw_choice_when_shortest_is_tagged() -> TestResult {
    let state = smiles::prepare("O=C1CC[C@H]2CC[C@@H]1[C@H]2F")?.state;
    let conf = Conformer {
        positions: [
            (2.5, -1.5),
            (1.5, -1.),
            (0., -2.),
            (-1.5, -1.),
            (-1., 0.),
            (-0.5, 1.5),
            (0.5, 1.5),
            (1., 0.),
            (0., 0.8),
            (0., 1.8),
        ]
        .into_iter()
        .map(|(x, y)| Point3 { x, y, z: 0. })
        .collect(),
        is_3d: false,
    };
    let props = properties(&state);
    let raw = super::super::wedge_molecule(&wedge_state(&state), &props, Some(&conf), false)
        .map_err(anyhow::Error::msg)?;
    let generated =
        wedge_molecule(&wedge_state(&state), &props, &conf, false).map_err(anyhow::Error::msg)?;
    assert_eq!(generated.graph, raw.graph);
    assert_eq!(generated.directions, raw.directions);
    let preferred = prefer_ring_wedges(&raw, &props, &conf).map_err(anyhow::Error::msg)?;
    for (stage, result) in [("ordinary chooser", &raw), ("ring preference", &preferred)] {
        assert_eq!(
            result
                .graph
                .bonds
                .iter()
                .zip(&result.directions)
                .filter(|(bond, direction)| bond.a == 4 && wedged(**direction))
                .map(|(bond, &direction)| (bond.b, direction))
                .collect::<Vec<_>>(),
            [(3, Direction::Wedge)],
            "{stage}: keep the length-four bridge when the shortest is tagged"
        );
        let visible = perceived(&state, result, &conf)?;
        assert_eq!(
            visible
                .metadata
                .atoms
                .iter()
                .filter(|atom| tetrahedral(atom.chiral_tag))
                .count(),
            3,
            "{stage}"
        );
        assert_eq!(canonical(&state)?, canonical(&visible)?, "{stage}");
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
fn spiro_preference_preserves_wavy_bond_at_either_pair_position() -> TestResult {
    let (state, conf) = prepared("C1CO[C@]2(C1)CCNC2")?;
    let props = properties(&state);
    let plain = wedge_state(&state);
    let preferred = prefer_ring_wedges(&plain, &props, &conf).map_err(anyhow::Error::msg)?;
    let center = state
        .metadata
        .atoms
        .iter()
        .position(|atom| tetrahedral(atom.chiral_tag))
        .unwrap();
    let mut work = Work(1_000_000);
    let top = Topology::new(&state.graph, &mut work).map_err(anyhow::Error::msg)?;
    let selected = top
        .spiro_pairs(center, &mut work)
        .map_err(anyhow::Error::msg)?
        .into_iter()
        .find(|pair| pair.iter().all(|&bond| wedged(preferred.directions[bond])))
        .unwrap();
    for marker in selected {
        let mut input = plain.clone();
        let edge = &mut input.graph.bonds[marker];
        if edge.a == center {
            std::mem::swap(&mut edge.a, &mut edge.b);
        }
        input.directions[marker] = Direction::Unknown;
        let result = prefer_ring_wedges(&input, &props, &conf).map_err(anyhow::Error::msg)?;
        assert_eq!(result.directions[marker], Direction::Unknown);
        assert_eq!(result.graph.bonds[marker], input.graph.bonds[marker]);
        assert_eq!(
            canonical(&state)?,
            canonical(&perceived(&state, &result, &conf)?)?
        );
    }
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
