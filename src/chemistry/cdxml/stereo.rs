use super::{Error, Fragment, Result};
use crate::chemistry::{ELEMENTS, kekulize::Direction, rings, stereo};
use std::collections::HashSet;
use stereo::{SpatialAnnotations, SpatialOptions, wedging::Conformer};

pub(super) fn finish(part: &mut Fragment) -> Result<()> {
    part.non_explicit_3d_chirality = vec![None; part.graph.atoms.len()];
    part.atom_cip_ranks = vec![None; part.graph.atoms.len()];
    if part.graph.atoms.is_empty() {
        return Ok(());
    }
    // Only wedge origins receive native property caches in the 2D pass. An
    // earlier wavy bond blocks later wedges from that atom. Keep availability
    // separate from provisional values used privately by Rust geometry passes.
    let mut cached = vec![part.is_3d; part.graph.atoms.len()];
    let mut blocked = vec![false; part.graph.atoms.len()];
    for (bond, direction) in part.graph.bonds.iter().zip(&part.directions) {
        let block = blocked
            .get_mut(bond.a)
            .ok_or_else(|| Error::Invalid("Missing stereo atom".into()))?;
        match direction {
            Direction::Unknown => *block = true,
            Direction::Wedge | Direction::Hash if !*block => {
                *cached
                    .get_mut(bond.a)
                    .ok_or_else(|| Error::Invalid("Missing stereo cache".into()))? = true;
            }
            _ => (),
        }
    }
    let conformer = Conformer {
        positions: part.positions.clone(),
        is_3d: part.is_3d,
    };
    if part.is_3d {
        let result = stereo::from_3d(
            &part.graph,
            &part.metadata,
            &part.directions,
            Some(&conformer),
            &SpatialAnnotations {
                non_explicit: part.non_explicit_3d_chirality.clone(),
                done: None,
            },
            SpatialOptions::default(),
        )
        .map_err(|e| Error::Stereo(e.to_string()))?;
        part.metadata = result.metadata;
        part.non_explicit_3d_chirality = result.annotations.non_explicit;
    } else if part
        .directions
        .iter()
        .any(|d| matches!(d, Direction::Wedge | Direction::Hash | Direction::Unknown))
    {
        let result = stereo::from_directions(
            &part.graph,
            &part.metadata,
            &part.directions,
            Some(&part.positions),
            true,
        )
        .map_err(Error::Stereo)?;
        part.graph = result.graph;
        part.metadata = result.metadata;
    }
    part.metadata = stereo::detect_atropisomers(
        &part.graph,
        &part.metadata,
        &part.directions,
        Some(&conformer),
    )
    .map_err(|e| Error::Stereo(e.to_string()))?;
    for ((bond, meta), direction) in part
        .graph
        .bonds
        .iter()
        .zip(&mut part.metadata.bonds)
        .zip(&mut part.directions)
    {
        if bond.order == 1 {
            if *direction == Direction::Unknown {
                meta.unknown_stereo = true;
            }
            *direction = Direction::None;
        }
    }
    let rings = rings::perceive(&part.graph, rings::Options::default())
        .map_err(|e| Error::Stereo(e.to_string()))?;
    let geometry = stereo::detect_bond_stereo(
        &part.graph,
        &part.metadata,
        &part.directions,
        Some(&part.positions),
        &rings.atoms,
    )
    .map_err(Error::Stereo)?;
    part.metadata = geometry.metadata;
    part.directions = geometry.directions;
    bond_labels(part, &cached)
}

fn bond_labels(part: &mut Fragment, cached: &[bool]) -> Result<()> {
    if part.bond_cip.iter().all(Option::is_none) {
        return Ok(());
    }
    let mut neighbors = vec![Vec::new(); part.graph.atoms.len()];
    for b in &part.graph.bonds {
        neighbors
            .get_mut(b.a)
            .ok_or_else(|| Error::Invalid("Missing bond endpoint".into()))?
            .push(b.b);
        neighbors
            .get_mut(b.b)
            .ok_or_else(|| Error::Invalid("Missing bond endpoint".into()))?
            .push(b.a);
    }
    let mut remaining = 2_000_000usize;
    let mut ranks = None;
    for (index, (bond, label)) in part.graph.bonds.iter().zip(&part.bond_cip).enumerate() {
        let Some(label) = label else {
            continue;
        };
        let current = part
            .metadata
            .bonds
            .get(index)
            .ok_or_else(|| Error::Invalid("Missing stereo bond".into()))?;
        if current.stereo != 0 {
            continue;
        }
        let stereo = if *label == 2 { 3 } else { 2 };
        let left = neighbors
            .get(bond.a)
            .ok_or_else(|| Error::Invalid("Missing neighbors".into()))?
            .iter()
            .copied()
            .filter(|a| *a != bond.b)
            .collect::<Vec<_>>();
        let right = neighbors
            .get(bond.b)
            .ok_or_else(|| Error::Invalid("Missing neighbors".into()))?
            .iter()
            .copied()
            .filter(|a| *a != bond.a)
            .collect::<Vec<_>>();
        let mut controls = match (left.as_slice(), right.as_slice()) {
            ([a], [b]) => Some([*a, *b]),
            _ => None,
        };
        if controls.is_none() && !part.is_3d {
            let work = left.len().checked_mul(right.len()).ok_or(Error::Limit)?;
            remaining = remaining.checked_sub(work).ok_or(Error::Limit)?;
            let p = part
                .positions
                .get(bond.a)
                .ok_or_else(|| Error::Invalid("Missing position".into()))?;
            let q = part
                .positions
                .get(bond.b)
                .ok_or_else(|| Error::Invalid("Missing position".into()))?;
            let (dx, dy) = (q.x - p.x, q.y - p.y);
            let side = |a: usize| -> Result<f64> {
                let r = part
                    .positions
                    .get(a)
                    .ok_or_else(|| Error::Invalid("Missing position".into()))?;
                Ok(dx * (r.y - p.y) - dy * (r.x - p.x))
            };
            let mut best = -1.;
            for &a in &left {
                let begin = side(a)?;
                if begin.abs() < 1e-6 {
                    continue;
                }
                for &b in &right {
                    let end = side(b)?;
                    if end.abs() < 1e-6 || (begin * end > 0.) != (stereo == 2) {
                        continue;
                    }
                    let score = begin.abs() + end.abs();
                    if score > best {
                        best = score;
                        controls = Some([a, b]);
                    }
                }
            }
        }
        if controls.is_none() {
            if ranks.is_none() {
                let values = fallback_ranks(part, cached)?;
                part.atom_cip_ranks = values.iter().copied().map(Some).collect();
                ranks = Some(values);
            }
            let ranks = ranks
                .as_ref()
                .ok_or_else(|| Error::Invalid("Missing CIP priorities".into()))?;
            let highest = |items: &[usize]| -> Option<usize> {
                let (mut best, mut result) = (0, None);
                for &atom in items {
                    let rank = *ranks.get(atom)?;
                    if result.is_none() || rank > best {
                        best = rank;
                        result = Some(atom);
                    } else if rank == best {
                        result = None;
                    }
                }
                result
            };
            controls = highest(&left).zip(highest(&right)).map(|(a, b)| [a, b]);
        }
        if let Some(controls) = controls {
            let meta = part
                .metadata
                .bonds
                .get_mut(index)
                .ok_or_else(|| Error::Invalid("Missing stereo bond".into()))?;
            meta.stereo = stereo;
            meta.stereo_atoms = controls.to_vec();
        }
    }
    Ok(())
}

fn fallback_ranks(part: &Fragment, cached: &[bool]) -> Result<Vec<u32>> {
    // Native refinement reads H counts only when its initial invariants tie.
    // Explicit no-implicit atoms need no H cache even in that case. CDXML has
    // no atom maps here: generic R query atoms are rejected at this boundary.
    let mut unique = HashSet::new();
    let mut tied = false;
    for atom in &part.graph.atoms {
        let mut mass = 0;
        if atom.isotope != 0 {
            let element = ELEMENTS
                .get(usize::from(atom.atomic_number))
                .ok_or_else(|| Error::Invalid("Unknown element".into()))?;
            mass = i32::from(atom.isotope) - i32::from(element.common_isotope);
            if mass >= 0 {
                mass += 1;
            }
        }
        mass = (mass + 512).max(0) % 1024;
        tied |= !unique.insert((atom.atomic_number, mass));
    }
    if tied
        && part
            .graph
            .atoms
            .iter()
            .zip(cached)
            .any(|(atom, cache)| !cache && !atom.no_implicit)
    {
        return Err(Error::Stereo(
            "Bond CIP refinement reads an uninitialized hydrogen cache".into(),
        ));
    }
    stereo::atom_priorities(&part.graph, &part.metadata).map_err(Error::Stereo)
}
