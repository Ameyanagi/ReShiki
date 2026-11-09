use super::*;
use crate::{
    graph::{Atom, Bond, Graph},
    stereo::{
        Point3,
        drawing::wedging,
        perception::{RingCache, RingKind},
    },
};

type TestResult = anyhow::Result<()>;

/// The four isotope-labelled carbons form a tetrahedrane. Each of atoms
/// 0..=2 bears N(CH3)-C(CH3)=CH2, whose N-vinyl bond is an atropisomer axis.
/// The core carbons are tetrahedral centres, not ends of those axes.
fn tetrahedrane_fixture() -> Result<(WedgeState, WedgeProperties, Conformer), String> {
    let mut atoms: Vec<_> = (12..=15)
        .map(|isotope| Atom {
            atomic_number: 6,
            isotope,
            no_implicit: true,
            ..Atom::default()
        })
        .collect();
    atoms[3].explicit_hydrogens = 1;
    let mut bonds: Vec<_> = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
        .into_iter()
        .map(|(a, b)| Bond {
            a,
            b,
            order: 1,
            aromatic: false,
        })
        .collect();
    for core in 0..3 {
        let n = atoms.len();
        for (atomic_number, explicit_hydrogens) in [(7, 0), (6, 0), (6, 2), (6, 3), (6, 3)] {
            atoms.push(Atom {
                atomic_number,
                explicit_hydrogens,
                no_implicit: true,
                ..Atom::default()
            });
        }
        for (a, b, order) in [
            (core, n, 1),
            (n, n + 1, 1),
            (n + 1, n + 2, 2),
            (n, n + 3, 1),
            (n + 1, n + 4, 1),
        ] {
            bonds.push(Bond {
                a,
                b,
                order,
                aromatic: false,
            });
        }
    }
    let graph = Graph { atoms, bonds };
    let properties = WedgeProperties {
        valences: graph.provisional_valences()?,
        attachment_points: vec![false; graph.atoms.len()],
    };
    let mut metadata = Metadata::unspecified(&graph);
    for atom in &mut metadata.atoms[..4] {
        atom.chiral_tag = 1;
    }
    for axis in [7, 12, 17] {
        metadata.bonds[axis].stereo = 6;
    }
    let state = WedgeState {
        directions: vec![Direction::None; graph.bonds.len()],
        graph,
        metadata,
        // All four triangles are equivalent. In particular, ring scores do
        // not break the preference for the lower-degree neighbour atom 3.
        rings: RingCache {
            kind: RingKind::Symmetric,
            atoms: vec![vec![0, 1, 2], vec![0, 1, 3], vec![0, 2, 3], vec![1, 2, 3]],
        },
    };
    let conformer = Conformer {
        is_3d: false,
        positions: [
            (-2.0, -1.0),
            (2.0, -1.0),
            (0.0, 2.0),
            (0.0, 0.0),
            (-6.0, -3.0),
            (-7.0, -6.0),
            (-9.0, -7.0),
            (-7.0, -1.0),
            (-6.0, -8.0),
            (6.0, -3.0),
            (9.0, -2.0),
            (11.0, -3.0),
            (5.0, -5.0),
            (10.0, 0.0),
            (0.0, 6.0),
            (-2.0, 8.0),
            (-2.0, 10.0),
            (2.0, 6.0),
            (-4.0, 8.0),
        ]
        .into_iter()
        .map(|(x, y)| Point3 { x, y, z: 0.0 })
        .collect(),
    };
    Ok((state, properties, conformer))
}

fn readback(state: &WedgeState, conformer: &Conformer) -> anyhow::Result<Metadata> {
    let drawn = from_directions(
        &state.graph,
        &Metadata::unspecified(&state.graph),
        &state.directions,
        Some(&conformer.positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    Ok(detect_atropisomers(
        &drawn.graph,
        &drawn.metadata,
        &state.directions,
        Some(conformer),
    )?)
}

#[test]
fn avoiding_axis_ends_must_not_leave_a_tetrahedrane_centre_unwedged() -> TestResult {
    let (state, properties, conformer) = tetrahedrane_fixture().map_err(anyhow::Error::msg)?;
    let raw = wedging::wedge_molecule(&state, &properties, Some(&conformer), false)
        .map_err(anyhow::Error::msg)?;
    assert!(
        !preserves_stereo(&raw, &conformer, &mut Work(50_000_000)).map_err(anyhow::Error::msg)?
    );

    // Greedily avoiding the nitrogen axis ends makes centres 0, 1 and 2
    // choose their bonds to atom 3. All of atom 3's bonds are then occupied.
    let mut greedy = state.clone();
    for (bond, atom, direction) in [
        (2, 0, Direction::Hash),
        (4, 1, Direction::Wedge),
        (5, 2, Direction::Hash),
        (6, 4, Direction::Wedge),
        (11, 9, Direction::Wedge),
        (16, 14, Direction::Wedge),
    ] {
        let edge = &mut greedy.graph.bonds[bond];
        if edge.a != atom {
            std::mem::swap(&mut edge.a, &mut edge.b);
        }
        greedy.directions[bond] = direction;
    }
    let lost = readback(&greedy, &conformer)?;
    assert_eq!(
        lost.atoms[..4]
            .iter()
            .map(|atom| atom.chiral_tag)
            .collect::<Vec<_>>(),
        [1, 1, 1, 0]
    );
    for axis in [7, 12, 17] {
        assert_eq!(lost.bonds[axis].stereo, 6);
    }
    assert!(
        !preserves_stereo(&greedy, &conformer, &mut Work(50_000_000))
            .map_err(anyhow::Error::msg)?
    );
    let stale = from_directions(
        &greedy.graph,
        &greedy.metadata,
        &greedy.directions,
        Some(&conformer.positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(stale.metadata.atoms[3].chiral_tag, 1);

    // A different assignment exists; this case must succeed, not be refused.
    let repaired =
        wedge_molecule(&state, &properties, &conformer, false).map_err(anyhow::Error::msg)?;
    assert!(
        preserves_stereo(&repaired, &conformer, &mut Work(50_000_000))
            .map_err(anyhow::Error::msg)?
    );
    let read = readback(&repaired, &conformer)?;
    for atom in 0..4 {
        assert_eq!(read.atoms[atom].chiral_tag, 1, "core atom {atom}");
    }
    for axis in [7, 12, 17] {
        assert_eq!(read.bonds[axis].stereo, 6, "axis bond {axis}");
    }
    let mut extra_axis = repaired.clone();
    extra_axis.metadata.bonds[7].stereo = 0;
    assert!(
        !preserves_stereo(&extra_axis, &conformer, &mut Work(50_000_000))
            .map_err(anyhow::Error::msg)?
    );
    let mut unwedged_centre = repaired.clone();
    for (bond, direction) in unwedged_centre
        .graph
        .bonds
        .iter()
        .zip(&mut unwedged_centre.directions)
    {
        if bond.a == 3 && wedged(*direction) {
            *direction = Direction::None;
        }
    }
    assert_eq!(unwedged_centre.metadata.atoms[3].chiral_tag, 1);
    assert!(
        !preserves_stereo(&unwedged_centre, &conformer, &mut Work(50_000_000))
            .map_err(anyhow::Error::msg)?
    );
    Ok(())
}

#[test]
fn atrop_search_stops_when_its_work_budget_is_exhausted() -> TestResult {
    let (state, properties, conformer) = tetrahedrane_fixture().map_err(anyhow::Error::msg)?;
    let mut setup = Work(50_000_000);
    let mut context =
        wedging::with_context(&state, &properties, Some(&conformer), false, &mut setup)
            .map_err(anyhow::Error::msg)?;
    context.state =
        prefer_ring_wedges(&context.state, &properties, &conformer).map_err(anyhow::Error::msg)?;
    let reference = context.state.clone();
    let mut work = Work(50_000_000);
    let mut readbacks = READBACK_LIMIT;
    assert!(
        search(&state, &mut context, false, &mut readbacks, &mut work)
            .map_err(anyhow::Error::msg)?
            .is_some()
    );
    assert!(
        readbacks < READBACK_LIMIT - 1,
        "must backtrack past a failed leaf"
    );
    // Leave enough work to reach and reject leaves, but not to read the
    // eventual solution. A zero budget would only test search setup.
    let mut exhausted = Work(50_000_000 - work.0 - 1);
    context.state = reference;
    readbacks = READBACK_LIMIT;
    let error = search(&state, &mut context, false, &mut readbacks, &mut exhausted)
        .err()
        .ok_or_else(|| anyhow::anyhow!("Search ignored its work budget"))?;
    assert_eq!(error, "Bond wedging work limit exceeded");
    assert!(readbacks < READBACK_LIMIT - 1);
    Ok(())
}

#[test]
fn atrop_search_caps_whole_molecule_readbacks() -> TestResult {
    let (state, properties, mut conformer) = tetrahedrane_fixture().map_err(anyhow::Error::msg)?;
    // Coincident ends cannot define the first axis, regardless of wedges.
    conformer.positions[5] = conformer.positions[4];
    let mut work = Work(50_000_000);
    let mut context =
        wedging::with_context(&state, &properties, Some(&conformer), false, &mut work)
            .map_err(anyhow::Error::msg)?;
    let reference =
        prefer_ring_wedges(&context.state, &properties, &conformer).map_err(anyhow::Error::msg)?;
    context.state = reference.clone();
    let mut readbacks = READBACK_LIMIT;
    let error = search(&state, &mut context, false, &mut readbacks, &mut work)
        .err()
        .ok_or_else(|| anyhow::anyhow!("Search did not exhaust its readback cap"))?;
    assert_eq!(error, "Atropisomer wedging readback limit exceeded");
    assert_eq!(readbacks, 0);
    assert!(work.0 > 0);
    let actual =
        wedge_molecule(&state, &properties, &conformer, false).map_err(anyhow::Error::msg)?;
    assert_eq!(
        serde_json::to_vec(&actual)?,
        serde_json::to_vec(&reference)?
    );
    Ok(())
}

#[test]
fn moved_centers_recover_a_valid_second_wedge() -> TestResult {
    let prepared = crate::smiles::prepare("[C@](N(C)C(=C)C)(F)(Cl)Br")?.state;
    let properties = WedgeProperties {
        attachment_points: vec![false; prepared.graph.atoms.len()],
        valences: prepared.valences,
    };
    let mut state = WedgeState {
        graph: prepared.graph,
        metadata: prepared.metadata,
        directions: prepared.directions,
        rings: prepared.rings,
    };
    state.metadata.bonds[2].stereo = 6; // N1-C3, beside the cardinal centre C0.
    let conformer = Conformer {
        is_3d: false,
        positions: [
            (0.0, 0.0),
            (1.0, 0.0),
            (1.0, 1.0),
            (2.0, -1.0),
            (3.0, -1.0),
            (2.0, -2.0),
            (0.0, 1.0),
            (-1.0, 0.0),
            (0.0, -1.0),
        ]
        .into_iter()
        .map(|(x, y)| Point3 { x, y, z: 0.0 })
        .collect(),
    };
    for (two_bonds, count) in [(false, 1), (true, 2)] {
        let mut work = Work(50_000_000);
        let mut context =
            wedging::with_context(&state, &properties, Some(&conformer), two_bonds, &mut work)
                .map_err(anyhow::Error::msg)?;
        context.state = prefer_ring_wedges(&context.state, &properties, &conformer)
            .map_err(anyhow::Error::msg)?;
        // Exercise retry restoration even if this reference already reads
        // correctly and the public entry point would keep it unchanged.
        let mut readbacks = READBACK_LIMIT;
        let repaired = search(&state, &mut context, two_bonds, &mut readbacks, &mut work)
            .map_err(anyhow::Error::msg)?;
        let repaired = repaired.ok_or_else(|| anyhow::anyhow!("No valid wedge assignment"))?;
        assert!(
            preserves_stereo(&repaired, &conformer, &mut Work(50_000_000))
                .map_err(anyhow::Error::msg)?
        );
        assert_eq!(
            repaired
                .graph
                .bonds
                .iter()
                .zip(&repaired.directions)
                .filter(|(bond, direction)| bond.a == 0 && wedged(**direction))
                .count(),
            count
        );
    }
    Ok(())
}

#[test]
fn the_stereo_predicate_preserves_allene_winding() -> TestResult {
    let mut prepared = crate::smiles::prepare("CC=C=CC")?.state;
    prepared.metadata.atoms[2].chiral_tag = 5;
    prepared.metadata.atoms[2].chiral_permutation = Some(1);
    let properties = WedgeProperties {
        attachment_points: vec![false; prepared.graph.atoms.len()],
        valences: prepared.valences,
    };
    let state = WedgeState {
        graph: prepared.graph,
        metadata: prepared.metadata,
        directions: prepared.directions,
        rings: prepared.rings,
    };
    let conformer = Conformer {
        is_3d: false,
        positions: [(-2.0, 1.0), (-1.0, 0.0), (0.0, 0.0), (1.0, 0.0), (2.0, 1.0)]
            .into_iter()
            .map(|(x, y)| Point3 { x, y, z: 0.0 })
            .collect(),
    };
    let raw = wedging::wedge_molecule(&state, &properties, Some(&conformer), false)
        .map_err(anyhow::Error::msg)?;
    assert!(preserves_stereo(&raw, &conformer, &mut Work(50_000_000)).map_err(anyhow::Error::msg)?);
    let read = readback(&raw, &conformer)?;
    assert_eq!(read.atoms[2].chiral_tag, 5);
    assert_eq!(read.atoms[2].chiral_permutation, Some(1));

    let mut wrong_permutation = raw.clone();
    wrong_permutation.metadata.atoms[2].chiral_permutation = Some(2);
    assert!(
        !preserves_stereo(&wrong_permutation, &conformer, &mut Work(50_000_000))
            .map_err(anyhow::Error::msg)?
    );

    // A wedge at the other terminal makes the drawing ambiguous even though
    // its stored allene tag and permutation remain unchanged.
    let mut both_ends = raw;
    both_ends.graph.bonds[3].a = 3;
    both_ends.graph.bonds[3].b = 4;
    both_ends.directions[3] = Direction::Wedge;
    let lost = readback(&both_ends, &conformer)?;
    assert_eq!(lost.atoms[2].chiral_tag, 0);
    assert_eq!(lost.atoms[2].chiral_permutation, None);
    assert!(
        !preserves_stereo(&both_ends, &conformer, &mut Work(50_000_000))
            .map_err(anyhow::Error::msg)?
    );
    Ok(())
}

#[test]
fn drawings_without_atropisomer_axes_keep_the_normal_generated_result() -> TestResult {
    let (mut state, properties, conformer) = tetrahedrane_fixture().map_err(anyhow::Error::msg)?;
    for axis in [7, 12, 17] {
        state.metadata.bonds[axis].stereo = 0;
    }
    let raw = wedging::wedge_molecule(&state, &properties, Some(&conformer), false)
        .map_err(anyhow::Error::msg)?;
    let expected = prefer_ring_wedges(&raw, &properties, &conformer).map_err(anyhow::Error::msg)?;
    let actual =
        wedge_molecule(&state, &properties, &conformer, false).map_err(anyhow::Error::msg)?;
    assert_eq!(serde_json::to_vec(&actual)?, serde_json::to_vec(&expected)?);
    Ok(())
}
