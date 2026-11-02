use super::{Molecule, State};
use crate::{
    kekulize::Direction,
    stereo::{self, allene, wedging},
};

/// The scalar adapter cannot write allene 0D parity, but its native kernel
/// accepts terminal wedge/hash bonds with real 2D coordinates. Prepare that
/// documented representation without changing the caller's molecular state.
pub(super) fn prepare(input: &Molecule) -> Result<Option<State>, String> {
    let unspecified = suppress_both_end_wedges(&input.state)?;
    let source = unspecified.as_ref().unwrap_or(&input.state);
    let centers = source
        .metadata
        .atoms
        .iter()
        .enumerate()
        .filter(|(_, atom)| atom.chiral_tag == 5)
        .map(|(center, atom)| (center, atom.chiral_permutation))
        .collect::<Vec<_>>();
    if centers.is_empty() {
        return Ok(unspecified);
    }
    let positions = input.positions.as_ref().ok_or(
        "InChI generation of allene stereochemistry requires nondegenerate 2D coordinates; cosmolkit-inchi 0.5.0 does not support allene 0D input. Use stereochemical SMILES or MOL export",
    )?;
    if positions.iter().any(|point| point.z != 0.) {
        return Err(
            "InChI generation of allene stereochemistry requires 2D coordinates (z = 0)".into(),
        );
    }
    for &(center, permutation) in &centers {
        if !allene::supported(&source.graph, center, 5, permutation) {
            return Err(format!(
                "Unsupported allene stereochemistry at atom {center}"
            ));
        }
    }
    let mut wedged = wedging::wedge_molecule(
        &wedging::WedgeState {
            graph: source.graph.clone(),
            metadata: source.metadata.clone(),
            directions: source.directions.clone(),
            rings: source.rings.clone(),
        },
        &wedging::WedgeProperties {
            valences: source.valences.clone(),
            attachment_points: vec![false; source.graph.atoms.len()],
        },
        Some(&wedging::Conformer {
            positions: positions.clone(),
            is_3d: false,
        }),
        false,
    )
    .map_err(|error| format!("Cannot depict allene stereochemistry for InChI: {error}"))?;
    for &(center, _) in &centers {
        let ends = allene::terminals(&wedged.graph, center).ok_or("Missing allene terminals")?;
        let Some((end, carrier, direction)) = wedged
            .graph
            .bonds
            .iter()
            .zip(&wedged.directions)
            .find_map(|(bond, &direction)| {
                (bond.order == 1
                    && ends.contains(&bond.a)
                    && matches!(direction, Direction::Wedge | Direction::Hash))
                .then_some((bond.a, bond.b, direction))
            })
        else {
            return Err("Missing allene stereobond".into());
        };
        // InChI needs the complementary stereobond when the chosen end
        // has two explicit carriers; a single wedge can be silently ignored.
        for (bond, other_direction) in wedged.graph.bonds.iter_mut().zip(&mut wedged.directions) {
            if bond.order != 1
                || ![bond.a, bond.b].contains(&end)
                || [bond.a, bond.b].contains(&carrier)
            {
                continue;
            }
            let complementary = if direction == Direction::Wedge {
                Direction::Hash
            } else {
                Direction::Wedge
            };
            if *other_direction != Direction::None {
                if bond.a != end || *other_direction != complementary {
                    return Err(format!(
                        "Ambiguous or conflicting allene stereochemistry at atom {center}; InChI requires complementary wedge/hash bonds at one end"
                    ));
                }
                continue;
            }
            if bond.b == end {
                std::mem::swap(&mut bond.a, &mut bond.b);
            }
            *other_direction = complementary;
        }
    }
    let inferred = stereo::from_directions(
        &wedged.graph,
        &wedged.metadata,
        &wedged.directions,
        Some(positions),
        true,
    )
    .map_err(|error| format!("Cannot validate allene stereochemistry for InChI: {error}"))?;
    for (center, permutation) in centers {
        let recovered = inferred
            .metadata
            .atoms
            .get(center)
            .ok_or("Missing inferred allene center")?;
        if recovered.chiral_tag != 5 || recovered.chiral_permutation != permutation {
            return Err(format!(
                "Ambiguous or conflicting allene stereochemistry at atom {center}; InChI requires a nondegenerate wedge/hash depiction at one end"
            ));
        }
    }
    let mut state = source.clone();
    state.graph = wedged.graph;
    state.directions = wedged.directions;
    Ok(Some(state))
}

/// Drawing perception leaves axes marked at both ends unspecified. The
/// native kernel otherwise resolves their raw wedges into a configuration,
/// so discard those directions only in the detached native-generation input.
fn suppress_both_end_wedges(input: &State) -> Result<Option<State>, String> {
    if !input
        .directions
        .iter()
        .any(|direction| matches!(direction, Direction::Wedge | Direction::Hash))
    {
        return Ok(None);
    }
    let mut directed = vec![false; input.graph.atoms.len()];
    for (bond, direction) in input.graph.bonds.iter().zip(&input.directions) {
        if bond.order == 1 && matches!(direction, Direction::Wedge | Direction::Hash) {
            *directed
                .get_mut(bond.a)
                .ok_or("Missing allene wedge endpoint")? = true;
        }
    }
    if directed.iter().filter(|&&marked| marked).take(2).count() < 2 {
        return Ok(None);
    }
    let mut candidates = vec![0usize; input.graph.atoms.len()];
    for bond in input.graph.bonds.iter().filter(|bond| bond.order == 2) {
        for (center, end) in [(bond.a, bond.b), (bond.b, bond.a)] {
            if *directed.get(end).ok_or("Missing allene wedge endpoint")? {
                let count = candidates.get_mut(center).ok_or("Missing allene center")?;
                *count += 1;
            }
        }
    }
    let mut ambiguous = vec![false; input.graph.atoms.len()];
    for (center, (&count, metadata)) in candidates.iter().zip(&input.metadata.atoms).enumerate() {
        if count != 2 || metadata.chiral_tag == 5 {
            continue;
        }
        if let Some(ends) = allene::terminals(&input.graph, center) {
            for end in ends {
                *ambiguous.get_mut(end).ok_or("Missing allene terminal")? = true;
            }
        }
    }
    if !ambiguous.iter().any(|&marked| marked) {
        return Ok(None);
    }
    let mut result = input.clone();
    for (bond, direction) in result.graph.bonds.iter().zip(&mut result.directions) {
        if bond.order == 1
            && *ambiguous.get(bond.a).ok_or("Missing allene terminal")?
            && matches!(direction, Direction::Wedge | Direction::Hash)
        {
            *direction = Direction::None;
        }
    }
    Ok(Some(result))
}
