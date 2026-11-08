// Copied from crates/chemistry/src/stereo/drawing/allene.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3; changed: pass an explicit default preparation budget in the inline test
use super::*;
use crate::stereo::allene;

pub(super) fn perceive(
    result: &mut DrawnStereo,
    directions: &[Direction],
    positions: &[Point3],
    replace_existing: bool,
) -> Result<(), String> {
    let graph = &result.graph;
    if !graph.bonds.iter().zip(directions).any(|(b, d)| {
        b.order == 1 && matches!(d, Direction::Wedge | Direction::Hash | Direction::Unknown)
    }) {
        return Ok(());
    }
    let mut directed = vec![false; graph.atoms.len()];
    for (bond, direction) in graph.bonds.iter().zip(directions) {
        if bond.order == 1
            && matches!(
                direction,
                Direction::Wedge | Direction::Hash | Direction::Unknown
            )
        {
            *directed.get_mut(bond.a).ok_or("Missing allene endpoint")? = true;
        }
    }
    let mut doubles = vec![0; graph.atoms.len()];
    let mut candidates = vec![false; graph.atoms.len()];
    for bond in &graph.bonds {
        if bond.order == 2 {
            for (end, other) in [(bond.a, bond.b), (bond.b, bond.a)] {
                *doubles.get_mut(end).ok_or("Missing allene endpoint")? += 1;
                *candidates.get_mut(end).ok_or("Missing allene endpoint")? |=
                    *at(&directed, other)?;
            }
        }
    }
    for (center, (count, candidate)) in doubles.into_iter().zip(candidates).enumerate() {
        if count != 2
            || !candidate
            || !replace_existing && at(&result.metadata.atoms, center)?.chiral_tag != 0
        {
            continue;
        }
        let Some(ends) = allene::terminals(graph, center) else {
            continue;
        };
        let carriers = allene::carriers(graph, center).ok_or("Missing allene carriers")?;
        let axis = at(positions, ends[1])?.sub(*at(positions, ends[0])?);
        let mut vectors = [Point3::default(); 2];
        let mut wedges = [false; 2];
        let mut ambiguous = false;
        for ((&end, pair), (target, wedged)) in ends
            .iter()
            .zip(carriers.as_chunks::<2>().0)
            .zip(vectors.iter_mut().zip(&mut wedges))
        {
            let first = pair[0].ok_or("Missing allene substituent")?;
            let mut vector = at(positions, first)?.sub(*at(positions, end)?);
            vector.z = 0.;
            if let Some(second) = pair[1] {
                let other = at(positions, second)?.sub(*at(positions, end)?);
                // Explicit carriers must straddle the axis; otherwise bond
                // insertion order could determine the perceived configuration.
                let sides = axis.cross(vector).z * axis.cross(other).z;
                ambiguous |= !sides.is_finite() || sides >= -1e-7;
            }
            let mut winding = None;
            for (bond, direction) in graph.bonds.iter().zip(directions) {
                if bond.a != end || bond.order != 1 {
                    continue;
                }
                let sign = match direction {
                    Direction::Wedge => 1.,
                    Direction::Hash => -1.,
                    Direction::Unknown => {
                        ambiguous = true;
                        continue;
                    }
                    _ => continue,
                } * if bond.b == first { 1. } else { -1. };
                *wedged = true;
                if winding.is_some_and(|previous| previous != sign) {
                    ambiguous = true;
                }
                winding = Some(sign);
            }
            vector.z = winding.unwrap_or(0.);
            *target = vector;
        }
        if !wedges.iter().any(|&w| w) && !ambiguous {
            continue;
        }
        let meta = result
            .metadata
            .atoms
            .get_mut(center)
            .ok_or("Missing allene center")?;
        meta.chiral_tag = 0;
        meta.chiral_permutation = None;
        let volume = vectors[0].cross(vectors[1]).dot(axis);
        if ambiguous || wedges == [true, true] || !volume.is_finite() || volume.abs() < 1e-7 {
            continue;
        }
        // AL1 is anticlockwise in the ordered-carrier SMILES convention;
        // priority order [near high, near low, far high, far low] gives P.
        meta.chiral_tag = 5;
        meta.chiral_permutation = Some(if volume > 0. { 1 } else { 2 });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allene_existing_winding_and_nonfinite_import_coordinates() {
        let mut state = crate::smiles::prepare("CC=C=CC", &crate::budget::Budget::default())
            .unwrap()
            .state;
        let bond = &mut state.graph.bonds[0];
        std::mem::swap(&mut bond.a, &mut bond.b);
        state.metadata.atoms[2].chiral_tag = 5;
        state.metadata.atoms[2].chiral_permutation = Some(1);
        state.directions[0] = Direction::Wedge;
        let mut positions = [(-2., 1.), (-1., 0.), (0., 0.), (1., 0.), (2., 1.)]
            .map(|(x, y)| Point3 { x, y, z: 0. });
        for (replace, expected) in [(false, 1), (true, 2)] {
            let drawn = from_directions(
                &state.graph,
                &state.metadata,
                &state.directions,
                Some(&positions),
                replace,
            )
            .unwrap();
            assert_eq!(drawn.metadata.atoms[2].chiral_permutation, Some(expected));
        }
        positions[4].y = f64::NAN;
        let drawn = from_directions_with_bounds(
            &state.graph,
            &state.metadata,
            &state.directions,
            Some(&positions),
            true,
            CoordinateBounds::NativeImport,
        )
        .unwrap();
        assert_eq!(drawn.metadata.atoms[2].chiral_tag, 0);
        assert_eq!(drawn.metadata.atoms[2].chiral_permutation, None);
    }
}
