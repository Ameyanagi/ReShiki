use super::*;
use crate::{
    chemistry::{kekulize::Direction, ranking::Metadata, stereo::wedging},
    document::Point,
};

// RDKit's AtropManyChirals.cxsmi (#1056) with its N-vinyl axis N8-C9 drawn
// by an N8 wedge and C11 wedged away from axis end C9, at the given atom
// positions (world units).
fn molecule(positions: [(f32, f32); 20]) -> anyhow::Result<molecular::Molecule> {
    let mut doc = Document::default();
    for (element, (x, y)) in [
        "O", "C", "C", "C", "N", "C", "O", "N", "C", "C", "C", "C", "C", "Br", "C", "F", "C", "C",
        "Cl", "C",
    ]
    .into_iter()
    .zip(positions)
    {
        doc.add_atom(element, Point::new(x, y));
    }
    for (a, b, order, display) in [
        (1, 2, 2, "plain"),
        (2, 3, 1, "plain"),
        (3, 4, 2, "plain"),
        (4, 5, 1, "plain"),
        (5, 6, 1, "plain"),
        (6, 7, 2, "plain"),
        (6, 8, 1, "plain"),
        (8, 9, 1, "plain"),
        (9, 10, 2, "plain"),
        (11, 9, 1, "plain"),
        (11, 12, 1, "wedge"),
        (12, 13, 1, "hash"),
        (12, 14, 1, "plain"),
        (11, 15, 1, "plain"),
        (15, 16, 1, "plain"),
        (15, 17, 1, "wedge"),
        (11, 18, 1, "plain"),
        (18, 19, 1, "plain"),
        (18, 20, 1, "wedge"),
        (8, 2, 1, "wedge"),
    ] {
        doc.add_bond(a, b, order, display);
    }
    Ok(molecular::prepare(&doc)?)
}

const MACOS: [(f32, f32); 20] = [
    (-61.08, -75.62),
    (-69.01, -34.38),
    (-108.70, -20.63),
    (-116.63, 20.62),
    (-84.88, 48.11),
    (-45.20, 34.36),
    (-29.66, 50.39),
    (-37.26, -6.88),
    (1.07, -24.04),
    (1.72, -66.04),
    (37.12, -2.48),
    (58.68, -38.53),
    (38.24, -75.22),
    (100.67, -37.88),
    (73.16, 19.08),
    (109.86, -1.36),
    (72.51, 61.07),
    (18.78, 35.30),
    (-1.74, 44.07),
    (42.34, 70.07),
];

const LINUX: [(f32, f32); 20] = [
    (73.36, -69.67),
    (73.36, -27.67),
    (109.73, -6.67),
    (109.73, 35.33),
    (73.36, 56.33),
    (36.99, 35.33),
    (0.61, 56.33),
    (36.99, -6.67),
    (0.61, -27.67),
    (0.61, -69.67),
    (-35.76, -6.67),
    (-14.76, 29.70),
    (14.95, 37.11),
    (-35.76, 66.08),
    (-72.13, 14.33),
    (-72.13, 56.33),
    (-108.50, -6.67),
    (-56.76, -43.04),
    (-35.76, -79.42),
    (-98.76, -43.04),
];

fn centers(molecule: &molecular::Molecule) -> Vec<(u64, u8)> {
    let mut centers: Vec<_> = molecule
        .ids
        .iter()
        .zip(&molecule.state.metadata.atoms)
        .filter_map(|(&id, atom)| matches!(atom.chiral_tag, 1 | 2).then_some((id, atom.chiral_tag)))
        .collect();
    centers.sort_unstable();
    centers
}

fn read_wedges(document: &Document) -> anyhow::Result<molecular::Molecule> {
    let mut visible = document.clone();
    for atom in &mut visible.atoms {
        atom.stereo = None;
    }
    for bond in &mut visible.bonds {
        bond.stereo = None;
        bond.stereo_atoms.clear();
    }
    Ok(molecular::prepare(&visible)?)
}

fn finish_fresh(mut molecule: molecular::Molecule) -> anyhow::Result<Document> {
    assert_eq!(centers(&molecule).len(), 4);
    assert_eq!(axes(&molecule)?.len(), 1);
    // Fresh imports have tags but no wedges at their new coordinates.
    molecule.state.directions.fill(Direction::None);
    let labels = vec![None; molecule.ids.len()];
    let prepared = finish_molecule(
        molecule.clone(),
        &labels,
        Default::default(),
        &ComponentRow::default(),
    )?;
    // Stored tags must not conceal a centre lost by a new wedge choice.
    let read = read_wedges(&prepared.document)?;
    assert_eq!(centers(&read), centers(&molecule));
    assert_eq!(axes(&read)?, axes(&molecule)?);
    Ok(prepared.document)
}

fn wedge_input(
    molecule: &molecular::Molecule,
) -> (
    wedging::WedgeState,
    wedging::WedgeProperties,
    wedging::Conformer,
) {
    (
        wedging::WedgeState {
            graph: molecule.state.graph.clone(),
            metadata: molecule.state.metadata.clone(),
            directions: vec![Direction::None; molecule.state.graph.bonds.len()],
            rings: molecule.state.rings.clone(),
        },
        wedging::WedgeProperties {
            valences: molecule.state.valences.clone(),
            attachment_points: vec![false; molecule.ids.len()],
        },
        wedging::Conformer {
            positions: molecule.positions.clone(),
            is_3d: false,
        },
    )
}

fn reference_wedges(
    input: &wedging::WedgeState,
    properties: &wedging::WedgeProperties,
    conformer: &wedging::Conformer,
) -> anyhow::Result<wedging::WedgeState> {
    let reference = wedging::wedge_molecule(input, properties, Some(conformer), false)
        .map_err(anyhow::Error::msg)?;
    wedging::generated::prefer_ring_wedges(&reference, properties, conformer)
        .map_err(anyhow::Error::msg)
}

fn append_fixture(
    input: &mut wedging::WedgeState,
    properties: &mut wedging::WedgeProperties,
    conformer: &mut wedging::Conformer,
    state: &State,
    positions: &[Point3],
) {
    let offset = input.graph.atoms.len();
    input.graph.atoms.extend_from_slice(&state.graph.atoms);
    input
        .graph
        .bonds
        .extend(state.graph.bonds.iter().cloned().map(|mut bond| {
            bond.a += offset;
            bond.b += offset;
            bond
        }));
    input
        .metadata
        .atoms
        .extend_from_slice(&state.metadata.atoms);
    input
        .metadata
        .bonds
        .extend(state.metadata.bonds.iter().cloned().map(|mut bond| {
            for atom in &mut bond.stereo_atoms {
                *atom += offset;
            }
            bond
        }));
    input
        .directions
        .resize(input.graph.bonds.len(), Direction::None);
    input.rings.atoms.extend(
        state
            .rings
            .atoms
            .iter()
            .map(|ring| ring.iter().map(|atom| atom + offset).collect()),
    );
    properties.valences.extend_from_slice(&state.valences);
    properties
        .attachment_points
        .resize(input.graph.atoms.len(), false);
    conformer.positions.extend_from_slice(positions);
}

fn append_cardinal(
    input: &mut wedging::WedgeState,
    properties: &mut wedging::WedgeProperties,
    conformer: &mut wedging::Conformer,
) -> anyhow::Result<()> {
    let state = smiles::prepare("[C@](F)(Cl)(Br)I")?.state;
    let positions = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)]
        .map(|(x, y)| Point3 { x, y, z: 0.0 });
    append_fixture(input, properties, conformer, &state, &positions);
    Ok(())
}

fn assert_stereo_readback(
    input: &wedging::WedgeState,
    result: &wedging::WedgeState,
    conformer: &wedging::Conformer,
) -> anyhow::Result<()> {
    let drawn = chemistry::stereo::from_directions(
        &result.graph,
        &Metadata::unspecified(&result.graph),
        &result.directions,
        Some(&conformer.positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    for (atom, (expected, actual)) in input
        .metadata
        .atoms
        .iter()
        .zip(&drawn.metadata.atoms)
        .enumerate()
    {
        // A wedged axis-end amine reads as a provisional centre; only the
        // source's centres must survive.
        if expected.chiral_tag != 0 {
            assert_eq!(expected.chiral_tag, actual.chiral_tag, "atom {atom}");
        }
    }
    let read = chemistry::stereo::detect_atropisomers(
        &drawn.graph,
        &drawn.metadata,
        &result.directions,
        Some(conformer),
    )?;
    let read = chemistry::stereo::atropisomers(
        &drawn.graph,
        &read,
        &vec![chemistry::electronic::Hybridization::Unspecified; drawn.graph.atoms.len()],
        &input.rings.atoms,
    )
    .map_err(anyhow::Error::msg)?;
    let axes = |metadata: &Metadata| {
        metadata
            .bonds
            .iter()
            .enumerate()
            .filter_map(|(bond, meta)| matches!(meta.stereo, 6 | 7).then_some((bond, meta.stereo)))
            .collect::<Vec<_>>()
    };
    assert_eq!(axes(&read), axes(&input.metadata));
    Ok(())
}

#[test]
fn fresh_wedges_keep_every_center_and_axis_on_macos_and_linux() -> anyhow::Result<()> {
    let original = molecule(MACOS)?;
    assert_eq!(axes(&original)?, [(8, 9, 7)]);
    let document = finish_fresh(original)?;
    // C11 towards axis end C9 hides the axis; C11 towards C12 keeps both.
    // These are stable document IDs, not zero-based chemistry indices.
    assert!(document.bonds.iter().any(|bond| {
        bond.a == 11 && bond.b == 12 && matches!(bond.display.as_str(), "wedge" | "hash")
    }));
    // Preserve the already representable Linux layout as well.
    let mut linux = molecule(MACOS)?;
    linux.positions = molecule(LINUX)?.positions;
    finish_fresh(linux)?;
    Ok(())
}

#[test]
fn acceptable_linux_wedges_keep_the_reference_assignment() -> anyhow::Result<()> {
    let mut linux = molecule(MACOS)?;
    linux.positions = molecule(LINUX)?.positions;
    let (input, properties, conformer) = wedge_input(&linux);
    let reference = reference_wedges(&input, &properties, &conformer)?;
    let generated = wedging::generated::wedge_molecule(&input, &properties, &conformer, false)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        serde_json::to_vec(&generated)?,
        serde_json::to_vec(&reference)?
    );
    Ok(())
}

#[test]
fn an_input_wedge_towards_an_axis_end_is_never_moved() -> anyhow::Result<()> {
    let (mut input, properties, conformer) = wedge_input(&molecule(MACOS)?);
    let raw = wedging::wedge_molecule(&input, &properties, Some(&conformer), false)
        .map_err(anyhow::Error::msg)?;
    let bond = raw
        .graph
        .bonds
        .iter()
        .position(|bond| bond.a == 10 && bond.b == 8)
        .ok_or_else(|| anyhow::anyhow!("Missing reference C11->C9 wedge"))?;
    assert!(matches!(
        raw.directions[bond],
        Direction::Wedge | Direction::Hash
    ));
    // Supply only the reference's C11->C9 centre wedge as fixed input.
    input.graph.bonds[bond] = raw.graph.bonds[bond].clone();
    input.directions[bond] = raw.directions[bond];
    let reference = reference_wedges(&input, &properties, &conformer)?;
    let generated = wedging::generated::wedge_molecule(&input, &properties, &conformer, false)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        (
            generated.graph.bonds[bond].a,
            generated.graph.bonds[bond].b,
            generated.directions[bond]
        ),
        (10, 8, raw.directions[bond])
    );
    // This fixed wedge leaves no acceptable drawing. Preserve the complete
    // reference result so the import's readback can refuse it.
    let drawn = chemistry::stereo::from_directions(
        &reference.graph,
        &Metadata::unspecified(&reference.graph),
        &reference.directions,
        Some(&conformer.positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    let read = chemistry::stereo::detect_atropisomers(
        &drawn.graph,
        &drawn.metadata,
        &reference.directions,
        Some(&conformer),
    )?;
    assert!(
        input
            .metadata
            .bonds
            .iter()
            .zip(&read.bonds)
            .any(|(expected, actual)| {
                matches!(expected.stereo, 6 | 7) && expected.stereo != actual.stereo
            })
    );
    assert_eq!(
        serde_json::to_vec(&generated)?,
        serde_json::to_vec(&reference)?
    );
    Ok(())
}

fn inverted_molecule() -> anyhow::Result<molecular::Molecule> {
    let mut inverted = molecule(MACOS)?;
    let tag = &mut inverted.state.metadata.atoms[10].chiral_tag;
    assert!(matches!(*tag, 1 | 2));
    *tag = 3 - *tag; // C11's [C@@] configuration.
    // The historical chemistry fixture uses the world-scale numbers with Y
    // up. Restore that uniform scale before moving these zero-based atoms.
    for point in &mut inverted.positions {
        point.x *= molecular::WORLD_PER_DEPICT_UNIT;
        point.y *= molecular::WORLD_PER_DEPICT_UNIT;
    }
    for (atom, (x, y)) in [
        (14, (-10.36, 30.24)),
        (15, (26.34, 50.68)),
        (16, (-11.01, -11.75)),
    ] {
        inverted.positions[atom] = Point3 { x, y, z: 0.0 };
    }
    Ok(inverted)
}

#[test]
fn inverted_center_tries_a_wedge_that_defines_it() -> anyhow::Result<()> {
    let inverted = inverted_molecule()?;
    let (input, properties, conformer) = wedge_input(&inverted);
    let bond = input
        .graph
        .bonds
        .iter()
        .position(|bond| bond.a == 10 && bond.b == 11)
        .ok_or_else(|| anyhow::anyhow!("Missing C11->C12 bond"))?;
    let avoiding = wedging::wedge_bond(&input, &properties, &conformer, bond, 10)
        .map_err(anyhow::Error::msg)?;
    let read = chemistry::stereo::from_directions(
        &avoiding.graph,
        &Metadata::unspecified(&avoiding.graph),
        &avoiding.directions,
        Some(&conformer.positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(read.metadata.atoms[10].chiral_tag, 0);
    let document = finish_fresh(inverted)?;
    // C11->C12 avoids the axis but does not define this centre. C11->C15
    // preserves its inverted winding and the other centres and axis.
    assert!(document.bonds.iter().any(|bond| {
        bond.a == 11 && bond.b == 15 && matches!(bond.display.as_str(), "wedge" | "hash")
    }));
    Ok(())
}

#[test]
fn independent_centers_do_not_exhaust_axis_repair() -> anyhow::Result<()> {
    let (mut input, mut properties, mut conformer) = wedge_input(&inverted_molecule()?);
    for _ in 0..10 {
        append_cardinal(&mut input, &mut properties, &mut conformer)?;
    }
    assert_eq!((input.graph.atoms.len(), input.graph.bonds.len()), (70, 60));
    let generated = wedging::generated::wedge_molecule(&input, &properties, &conformer, false)
        .map_err(anyhow::Error::msg)?;
    assert_stereo_readback(&input, &generated, &conformer)?;
    assert!(
        generated
            .graph
            .bonds
            .iter()
            .zip(&generated.directions)
            .any(|(bond, direction)| {
                bond.a == 10
                    && bond.b == 14
                    && matches!(direction, Direction::Wedge | Direction::Hash)
            })
    );
    Ok(())
}

#[test]
fn independent_axes_do_not_exhaust_inverted_center_repair() -> anyhow::Result<()> {
    let (mut input, mut properties, mut conformer) = wedge_input(&inverted_molecule()?);
    let first_bond = input.graph.bonds.len();
    let mut independent = smiles::prepare("CN(CC)C(=C)C")?.state;
    let axis = independent
        .graph
        .bonds
        .iter()
        .position(|bond| (bond.a == 1 && bond.b == 4) || (bond.a == 4 && bond.b == 1))
        .ok_or_else(|| anyhow::anyhow!("Missing independent N1-C4 axis"))?;
    independent
        .metadata
        .bonds
        .get_mut(axis)
        .ok_or_else(|| anyhow::anyhow!("Missing independent axis metadata"))?
        .stereo = 6;
    independent.directions.fill(Direction::None);
    let positions = [
        (-1.0, 1.0),
        (0.0, 0.0),
        (-1.0, -1.0),
        (-2.0, -1.0),
        (1.0, 0.0),
        (2.0, 1.0),
        (2.0, -1.0),
    ]
    .map(|(x, y)| Point3 { x, y, z: 0.0 });
    for _ in 0..3 {
        append_fixture(
            &mut input,
            &mut properties,
            &mut conformer,
            &independent,
            &positions,
        );
    }
    assert_eq!((input.graph.atoms.len(), input.graph.bonds.len()), (41, 38));
    assert_eq!(
        input
            .metadata
            .bonds
            .iter()
            .filter(|bond| matches!(bond.stereo, 6 | 7))
            .count(),
        4
    );
    let reference = reference_wedges(&input, &properties, &conformer)?;
    assert_eq!(
        reference
            .directions
            .iter()
            .skip(first_bond)
            .filter(|direction| matches!(direction, Direction::Wedge | Direction::Hash))
            .count(),
        3
    );
    // C11->C12 leaves the inverted centre undefined. Clearing the three
    // valid independent axes used to add 7^3 = 343 failed leaves before
    // C11->C15, exhausting the fixed 256-readback limit.
    let generated = wedging::generated::wedge_molecule(&input, &properties, &conformer, false)
        .map_err(anyhow::Error::msg)?;
    assert_stereo_readback(&input, &generated, &conformer)?;
    assert!(
        generated
            .graph
            .bonds
            .iter()
            .zip(&generated.directions)
            .any(|(bond, direction)| {
                bond.a == 10
                    && bond.b == 14
                    && matches!(direction, Direction::Wedge | Direction::Hash)
            })
    );
    // Retain every independent bond's reference orientation and direction.
    assert_eq!(
        generated
            .graph
            .bonds
            .iter()
            .zip(&generated.directions)
            .skip(first_bond)
            .collect::<Vec<_>>(),
        reference
            .graph
            .bonds
            .iter()
            .zip(&reference.directions)
            .skip(first_bond)
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn axis_repair_preserves_an_independent_centers_second_wedge() -> anyhow::Result<()> {
    let (mut input, mut properties, mut conformer) = wedge_input(&molecule(MACOS)?);
    let first_bond = input.graph.bonds.len();
    append_cardinal(&mut input, &mut properties, &mut conformer)?;
    let raw = wedging::wedge_molecule(&input, &properties, Some(&conformer), true)
        .map_err(anyhow::Error::msg)?;
    let reference = wedging::generated::prefer_ring_wedges(&raw, &properties, &conformer)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        reference.directions[first_bond..]
            .iter()
            .filter(|direction| matches!(direction, Direction::Wedge | Direction::Hash))
            .count(),
        2
    );
    let generated = wedging::generated::wedge_molecule(&input, &properties, &conformer, true)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        generated.graph.bonds[first_bond..],
        reference.graph.bonds[first_bond..]
    );
    assert_eq!(
        generated.directions[first_bond..],
        reference.directions[first_bond..]
    );
    // A second C11 wedge towards C9 would hide the axis again; reject that
    // addition while keeping the independent cardinal centre's two wedges.
    assert_eq!(
        generated
            .graph
            .bonds
            .iter()
            .zip(&generated.directions)
            .filter(|(bond, direction)| bond.a == 10
                && matches!(direction, Direction::Wedge | Direction::Hash))
            .count(),
        1
    );
    assert_stereo_readback(&input, &generated, &conformer)
}

#[test]
fn axis_repair_preserves_an_independent_spiro_ring_convention() -> anyhow::Result<()> {
    let (mut input, mut properties, mut conformer) = wedge_input(&molecule(MACOS)?);
    let first_bond = input.graph.bonds.len();
    let state = smiles::prepare("C1CO[C@]2(C1)CCNC2")?.state;
    let ranks = state
        .properties
        .atoms
        .iter()
        .map(|atom| atom.cip_rank)
        .collect::<Vec<_>>();
    let spiro = depict::compute(&state, &ranks, None, Default::default())?;
    append_fixture(
        &mut input,
        &mut properties,
        &mut conformer,
        &state,
        &spiro.positions,
    );
    let reference = reference_wedges(&input, &properties, &conformer)?;
    let directions = &reference.directions[first_bond..];
    assert_eq!(
        directions
            .iter()
            .filter(|direction| matches!(direction, Direction::Wedge | Direction::Hash))
            .count(),
        2
    );
    assert!(directions.contains(&Direction::Wedge));
    assert!(directions.contains(&Direction::Hash));
    let generated = wedging::generated::wedge_molecule(&input, &properties, &conformer, false)
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        generated.graph.bonds[first_bond..],
        reference.graph.bonds[first_bond..]
    );
    assert_eq!(
        generated.directions[first_bond..],
        reference.directions[first_bond..]
    );
    assert_stereo_readback(&input, &generated, &conformer)
}

#[test]
fn an_axis_with_coincident_ends_is_still_refused() -> anyhow::Result<()> {
    let mut fresh = molecule(MACOS)?;
    assert_eq!(axes(&fresh)?.len(), 1);
    fresh.state.directions.fill(Direction::None);
    // No wedge assignment can define an axis with no drawing direction.
    fresh.positions[8] = fresh.positions[7];
    let labels = vec![None; fresh.ids.len()];
    let error = finish_molecule(fresh, &labels, Default::default(), &ComponentRow::default())
        .err()
        .ok_or_else(|| anyhow::anyhow!("Undrawable axis accepted"))?;
    assert!(
        matches!(
            &error,
            Error::Molecular(molecular::Error::WedgeAssignment(_))
        ),
        "{error}"
    );
    assert!(error.to_string().contains("atropisomer axis"), "{error}");
    Ok(())
}

#[tokio::test]
async fn fresh_cxsmiles_import_preserves_axes_sharing_an_end() -> anyhow::Result<()> {
    let config = if std::env::var_os("RESHIKI_INCHI_HELPER").is_some() {
        native_response::Config::new(helper::discover()?)
    } else {
        assert!(std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none());
        native_response::Config::in_process()
    };
    let outcome = execute(
        Request::import_smiles(
            "CN(C(=C)C)C(=C)C |(-1,0,;0,0,;0.5,1,;1.5,1,;0,2,;0.5,-1,;1.5,-1,;0,-2,),wD:2.3,wU:5.6|",
        ),
        Some(config),
    )
    .await?;
    let Outcome::Complete(response) = outcome else {
        panic!("Import unexpectedly deferred");
    };
    let document = response
        .document
        .ok_or_else(|| anyhow::anyhow!("Missing imported drawing"))?;
    assert_eq!(axes(&read_wedges(&document)?)?, [(2, 3, 6), (2, 6, 6)]);
    Ok(())
}

#[tokio::test]
async fn an_axis_beside_a_named_dummy_atom_still_imports() -> anyhow::Result<()> {
    // The finished drawing stores R1 as an element and cannot be prepared
    // again, so the axis check leaves this import to main's behaviour.
    let config = if std::env::var_os("RESHIKI_INCHI_HELPER").is_some() {
        native_response::Config::new(helper::discover()?)
    } else {
        assert!(std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none());
        native_response::Config::in_process()
    };
    let outcome = execute(
        Request::import_smiles(
            "c1(-c2c(O)c3c(cc2)cccc3)c(O)c2c(cc1)cccc2.* |wU:0.13,atomProp:22.dummyLabel.R1|",
        ),
        Some(config),
    )
    .await?;
    let Outcome::Complete(response) = outcome else {
        panic!("Import unexpectedly deferred");
    };
    assert!(response.document.is_some());
    Ok(())
}
