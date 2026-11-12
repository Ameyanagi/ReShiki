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

/// Full CIP codes distinguish configuration from neighbour/bond ordering.
fn tetrahedral_codes(
    molecule: &molecular::Molecule,
) -> anyhow::Result<std::collections::BTreeMap<u64, String>> {
    let mut state = molecule.state.clone();
    for atom in &mut state.properties.atoms {
        atom.cip_code = None;
    }
    state.properties.bond_codes.fill(None);
    let labeled = chemistry::stereo::cip::label::assign(&state, &Default::default())
        .map_err(molecular::Error::from)?;
    Ok(molecule
        .ids
        .iter()
        .zip(&state.metadata.atoms)
        .zip(labeled.state.properties.atoms)
        .filter(|((_, metadata), _)| matches!(metadata.chiral_tag, 1 | 2))
        .filter_map(|((&id, _), atom)| atom.cip_code.map(|code| (id, code)))
        .collect())
}

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
    // reference result so the import can warn about the missing axis.
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
fn an_axis_with_coincident_ends_imports_with_a_warning() -> anyhow::Result<()> {
    let mut fresh = molecule(MACOS)?;
    assert_eq!(axes(&fresh)?, [(8, 9, 7)]);
    fresh.state.directions.fill(Direction::None);
    // No wedge assignment can define an axis with no drawing direction.
    fresh.positions[8] = fresh.positions[7];
    let labels = vec![None; fresh.ids.len()];
    let prepared = finish_molecule(
        fresh.clone(),
        &labels,
        Default::default(),
        &ComponentRow::default(),
    )?;
    assert_eq!(
        prepared.warnings,
        [
            "The wedges in this layout cannot show the atropisomer axis N8–C9; ReShiki drew the structure without that axis"
        ]
    );
    let read = read_wedges(&prepared.document)?;
    assert!(axes(&read)?.is_empty());
    assert_eq!(centers(&read), centers(&fresh));
    assert_eq!(centers(&read).len(), 4);
    assert!(axes(&prepared.molecule)?.is_empty());
    assert_eq!(
        tetrahedral_codes(&prepared.molecule)?,
        tetrahedral_codes(&read)?
    );
    assert_stable_stereo(&prepared.document)?;
    Ok(())
}

#[test]
fn a_failed_repair_cannot_import_a_reversed_axis() -> anyhow::Result<()> {
    // The first component cannot be repaired. The shared-end component's
    // reference wedges reverse its second axis, so a failed search returns it.
    let text = format!(
        "{}.CN(C(=C)C)C(=C)C",
        ATROP_MANY_CHIRALS.split_whitespace().next().unwrap()
    );
    let mut source = super::molecule(smiles::prepare(&text)?.state)?;
    source.positions[..20].copy_from_slice(&molecule(MACOS)?.positions);
    source.positions[8] = source.positions[7];
    for (point, (x, y)) in source.positions[20..].iter_mut().zip([
        (-1.0, 0.0),
        (0.0, 0.0),
        (0.5, 1.0),
        (1.5, 1.0),
        (0.0, 2.0),
        (0.5, -1.0),
        (1.5, -1.0),
        (0.0, -2.0),
    ]) {
        *point = Point3 {
            x: x + 20.0,
            y,
            z: 0.0,
        };
    }
    for (bond, metadata) in source
        .state
        .graph
        .bonds
        .iter()
        .zip(&mut source.state.metadata.bonds)
    {
        let pair = (bond.a.min(bond.b), bond.a.max(bond.b));
        metadata.stereo = match pair {
            (7, 8) => 7,
            (21, 22) | (21, 25) => 6,
            _ => metadata.stereo,
        };
    }
    source.state.directions.fill(Direction::None);
    let labels = vec![None; source.ids.len()];
    let drawing = molecular::for_generated(&source, &labels, Default::default())?;
    let codes = drawing.labels()?;
    let before = read_wedges(&drawing.finish(codes)?)?;
    assert_eq!(axes(&before)?, [(22, 23, 6), (22, 26, 7)]);
    let prepared = finish_molecule(
        source.clone(),
        &labels,
        Default::default(),
        &ComponentRow::default(),
    )?;
    let after = read_wedges(&prepared.document)?;
    for axis in axes(&after)? {
        assert!(
            axes(&source)?.contains(&axis),
            "Reversed axis survived: {axis:?}"
        );
    }
    assert!(!axes(&after)?.contains(&(22, 26, 7)));
    assert_eq!(centers(&after), centers(&source));
    assert_eq!(tetrahedral_codes(&after)?, tetrahedral_codes(&source)?);
    assert!(prepared.warnings.contains(&"The wedges in this layout cannot show the atropisomer axis N22–C26; ReShiki drew the structure without that axis".into()));
    assert_stable_stereo(&prepared.document)?;
    Ok(())
}

#[test]
fn a_lost_centre_is_warned_and_has_no_stored_winding_or_label() -> anyhow::Result<()> {
    let mut fresh = molecule(MACOS)?;
    fresh.state.directions.fill(Direction::None);
    fresh.positions[8] = fresh.positions[7];
    // C12 has only one nonzero neighbour direction, so no wedge defines it.
    fresh.positions[12] = fresh.positions[11];
    fresh.positions[13] = fresh.positions[11];
    let labels = vec![None; fresh.ids.len()];
    let prepared = finish_molecule(fresh, &labels, Default::default(), &ComponentRow::default())?;
    assert_eq!(
        prepared.warnings,
        [
            "The wedges in this layout cannot show the atropisomer axis N8–C9; ReShiki drew the structure without that axis",
            "The wedges in this layout cannot show the stereocentre C12; ReShiki drew the structure without it",
        ]
    );
    let center = prepared
        .document
        .atoms
        .iter()
        .find(|atom| atom.id == 12)
        .unwrap();
    assert!(center.stereo.is_none());
    assert!(center.cip_label.is_none());
    assert!(!tetrahedral_codes(&prepared.molecule)?.contains_key(&12));
    assert_stable_stereo(&prepared.document)?;
    Ok(())
}

fn assert_stable_stereo(document: &Document) -> anyhow::Result<()> {
    let visible = read_wedges(document)?;
    let expected_axes = axes(&visible)?;
    let expected_centers = tetrahedral_codes(&visible)?;
    let saved = serde_json::to_string(document)?;
    let reopened: Document = serde_json::from_str(&saved)?;
    for document in [document, &reopened] {
        let prepared = molecular::prepare(document)?;
        assert_eq!(axes(&prepared)?, expected_axes);
        assert_eq!(tetrahedral_codes(&prepared)?, expected_centers);
        let drawing = molecular::for_drawing(&prepared, document)?;
        let labels = drawing.labels()?;
        let refreshed = drawing.finish_preserving(labels, document)?;
        assert_eq!(refreshed, *document, "Labels changed on re-preparation");
    }
    let exported = molfile::write_document(document)?;
    let reimported = molfile::read(&exported)?.molecule;
    assert_eq!(axes(&reimported)?, expected_axes);
    // The drawing reads an axis wedge at a ring amide N as a centre and MOL
    // does not, as on main. Every other centre must survive MOL.
    let mut carbon_centers = expected_centers;
    carbon_centers.retain(|id, _| {
        document
            .atoms
            .iter()
            .any(|atom| atom.id == *id && atom.element != "N")
    });
    assert_eq!(tetrahedral_codes(&reimported)?, carbon_centers);
    Ok(())
}

async fn import_smiles_response(text: &str) -> anyhow::Result<Response> {
    let config = if std::env::var_os("RESHIKI_INCHI_HELPER").is_some() {
        native_response::Config::new(helper::discover()?)
    } else {
        assert!(std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none());
        native_response::Config::in_process()
    };
    let outcome = execute(Request::import_smiles(text), Some(config)).await?;
    let Outcome::Complete(response) = outcome else {
        panic!("Import unexpectedly deferred");
    };
    Ok(*response)
}

async fn import_smiles_document(text: &str) -> anyhow::Result<Document> {
    import_smiles_response(text)
        .await?
        .document
        .ok_or_else(|| anyhow::anyhow!("Missing imported drawing"))
}

#[tokio::test]
async fn fresh_cxsmiles_import_preserves_axes_sharing_an_end() -> anyhow::Result<()> {
    let document = import_smiles_document(
        "CN(C(=C)C)C(=C)C |(-1,0,;0,0,;0.5,1,;1.5,1,;0,2,;0.5,-1,;1.5,-1,;0,-2,),wD:2.3,wU:5.6|",
    )
    .await?;
    assert_eq!(axes(&read_wedges(&document)?)?, [(2, 3, 6), (2, 6, 6)]);
    Ok(())
}

#[tokio::test]
async fn fresh_cxsmiles_import_preserves_disconnected_shared_end_axes() -> anyhow::Result<()> {
    let document = import_smiles_document(
        "CN(C(=C)C)C(=C)C.CN(C(=C)C)C(=C)C |(-1,0,;0,0,;0.5,1,;1.5,1,;0,2,;0.5,-1,;1.5,-1,;0,-2,;5,0,;6,0,;6.5,1,;7.5,1,;6,2,;6.5,-1,;7.5,-1,;6,-2,),wD:2.3,10.10,wU:5.6,13.13|",
    )
    .await?;
    assert_eq!(
        axes(&read_wedges(&document)?)?,
        [(2, 3, 6), (2, 6, 6), (10, 11, 6), (10, 14, 6)]
    );
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

const ATROP_MANY_CHIRALS: &str = r"O=C1C=CNC(=O)N1C(=C)[C@]([C@H](C)Br)([C@@H](F)C)[C@H](Cl)C |(1.6038,0.5917,;1.2842,-0.2899,;1.9435,-0.5613,;2.0181,-1.3829,;1.344,-1.8584,;0.5951,-1.5123,;0.0595,-2.2018,;0.5204,-0.6907,;-0.3117,-0.3431,;-0.861,-1.0085,;-0.5013,0.5405,;-0.4918,1.4387,;-0.0783,2.2018,;-1.0961,2.0398,;0.3347,0.736,;0.6313,-0.0436,;1.118,1.0786,;-1.3746,0.4021,;-2.0181,0.9732,;-1.9327,-0.2608,),wU:7.7,11.13,10.14,17.18wD:14.15|";

const COUPLED_AXES: &str = r"C=C([C@H](C)C(C(=C)n1c(=O)[nH]cc(C)c1=O)(C(=C)N1C(=O)CCNC1=O)C(=C)N1C(=O)CCNC1=O)n1c(=O)[nH]cc(C)c1=O |(2.03213,-3.51413,;1.549,-2.09406,;0.0776293,-1.80242,;-0.584758,-3.14824,;-0.756696,-0.555863,;-2.00325,-1.39019,;-1.90398,-2.8869,;-3.50292,-1.35888,;-3.66457,-2.85015,;-2.45391,-3.73577,;-5.03686,-3.4558,;-6.24751,-2.57018,;-6.08587,-1.07891,;-7.29653,-0.193295,;-4.71358,-0.473265,;-5.07192,0.983302,;0.325974,0.482322,;0.425206,1.57131,;1.82267,0.581891,;2.70853,1.79237,;2.12726,3.17517,;4.19976,1.63043,;4.80513,0.25801,;3.91927,-0.952465,;2.42804,-0.790525,;1.65109,-2.07363,;-1.36179,0.816675,;-2.30672,1.367,;-1.33049,2.31635,;-0.0840978,3.15092,;1.40196,3.35493,;-0.183667,4.64762,;-1.52963,5.30973,;-2.77601,4.47516,;-2.67644,2.97846,;-4.09642,2.49506,;2.71826,-1.15446,;2.75017,0.345201,;1.79048,1.49803,;4.06487,1.0674,;5.34766,0.289934,;5.31575,-1.20973,;6.59854,-1.98719,;4.00105,-1.93192,;3.60721,-3.3793,),wD:2.2,wU:18.25,28.30|";

#[tokio::test]
async fn atrop_many_chirals_import_keeps_its_axis_and_four_centres_without_warning()
-> anyhow::Result<()> {
    let source = super::molecule(smiles::read(ATROP_MANY_CHIRALS)?.prepared.state)?;
    let response = import_smiles_response(ATROP_MANY_CHIRALS).await?;
    assert!(response.warnings.is_empty(), "{:?}", response.warnings);
    let document = response.document.unwrap();
    let read = read_wedges(&document)?;
    assert_eq!(axes(&read)?, axes(&source)?);
    assert_eq!(centers(&read), centers(&source));
    assert_eq!(centers(&read).len(), 4);
    assert_stable_stereo(&document)?;
    Ok(())
}

#[tokio::test]
async fn coupled_axes_import_warns_exactly_for_missing_stereo() -> anyhow::Result<()> {
    let source = super::molecule(smiles::read(COUPLED_AXES)?.prepared.state)?;
    let expected_axes = axes(&source)?;
    let source_centers = centers(&source);
    let response = import_smiles_response(COUPLED_AXES).await?;
    let document = response.document.unwrap();
    let read = read_wedges(&document)?;
    let actual_axes = axes(&read)?;
    let actual_centers = centers(&read);
    for axis in &actual_axes {
        assert!(
            expected_axes.contains(axis),
            "Invented or reversed axis {axis:?}"
        );
    }
    for (id, tag) in &source_centers {
        assert!(
            actual_centers
                .iter()
                .all(|(actual, actual_tag)| actual != id || actual_tag == tag),
            "Reversed centre {id}"
        );
    }
    let mut expected_warnings = Vec::new();
    for &(a, b, stereo) in &expected_axes {
        if !actual_axes.contains(&(a, b, stereo)) {
            let a = chemistry::stereo_lint::atom_name(&document, a);
            let b = chemistry::stereo_lint::atom_name(&document, b);
            expected_warnings.push(format!("The wedges in this layout cannot show the atropisomer axis {a}–{b}; ReShiki drew the structure without that axis"));
        }
    }
    for (id, _) in &source_centers {
        if !actual_centers.iter().any(|(actual, _)| actual == id) {
            let atom = chemistry::stereo_lint::atom_name(&document, *id);
            expected_warnings.push(format!("The wedges in this layout cannot show the stereocentre {atom}; ReShiki drew the structure without it"));
        }
    }
    assert_eq!(response.warnings, expected_warnings);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert!(!response.warnings.is_empty());
    assert_stable_stereo(&document)?;
    // Initial analysis, subsequent analysis and export use the same stereo.
    let analysis = response.analysis.unwrap();
    let engine = if std::env::var_os("RESHIKI_INCHI_HELPER").is_some() {
        crate::engine::LocalEngine::default()
    } else {
        crate::engine::LocalEngine::in_process()
    };
    for operation in ["analyze", "export"] {
        let mut request = Request::molecule(operation, document.clone());
        if operation == "export" {
            request.format = Some("mol".into());
        }
        let subsequent = engine.request(request).await.map_err(anyhow::Error::msg)?;
        let actual = subsequent.analysis.unwrap();
        assert_eq!(actual.smiles, analysis.smiles);
        assert_eq!(actual.smiles_notice, analysis.smiles_notice);
        assert_eq!(actual.inchi, analysis.inchi);
        assert_eq!(actual.inchikey, analysis.inchikey);
    }
    Ok(())
}
