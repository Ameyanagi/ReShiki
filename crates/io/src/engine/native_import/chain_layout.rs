//! Application policy for crowded molecules with a long, simple backbone.
//! Native reference depiction remains unchanged. Candidate coordinates are
//! detached, deterministic, and published only after geometry and ink checks.
use super::{State, depict, molecular};
use crate::{chemistry::stereo::Point3, document::Document};
use std::collections::{HashSet, VecDeque};
mod clearance;
use clearance::Quality;

pub(in crate::engine) const NOTICE: &str =
    "Layout still has overlaps or could not be fully checked · Review the drawing before export";

pub(in crate::engine) struct Layout {
    pub positions: Vec<Point3>,
    pub warnings: Vec<String>,
}

fn farthest(start: usize, adjacent: &[Vec<usize>]) -> Option<(usize, Vec<Option<usize>>)> {
    let mut previous = vec![None; adjacent.len()];
    let mut distances = vec![usize::MAX; adjacent.len()];
    *distances.get_mut(start)? = 0;
    let mut queue = VecDeque::from([start]);
    let mut far = start;
    while let Some(atom) = queue.pop_front() {
        let distance = *distances.get(atom)?;
        if distance > *distances.get(far)? || distance == *distances.get(far)? && atom < far {
            far = atom;
        }
        for &other in adjacent.get(atom)? {
            if *distances.get(other)? == usize::MAX {
                *distances.get_mut(other)? = distance + 1;
                *previous.get_mut(other)? = Some(atom);
                queue.push_back(other);
            }
        }
    }
    Some((far, previous))
}

fn backbone(state: &State) -> Option<Vec<usize>> {
    let n = state.graph.atoms.len();
    if !(32..=4096).contains(&n)
        || state.graph.bonds.len() > 6144
        || state.graph.atoms.iter().any(|a| a.atomic_number == 0)
    {
        return None;
    }
    let ring: HashSet<_> = state.rings.atoms.iter().flatten().copied().collect();
    let mut adjacent = vec![Vec::new(); n];
    let mut full = vec![Vec::new(); n];
    for bond in &state.graph.bonds {
        full.get_mut(bond.a)?.push(bond.b);
        full.get_mut(bond.b)?.push(bond.a);
        if bond.order == 1 && !ring.contains(&bond.a) && !ring.contains(&bond.b) {
            adjacent.get_mut(bond.a)?.push(bond.b);
            adjacent.get_mut(bond.b)?.push(bond.a);
        }
    }
    let mut seen = HashSet::new();
    let mut longest = Vec::new();
    for start in 0..n {
        if ring.contains(&start) || !seen.insert(start) {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let (mut vertices, mut edges) = (0, 0);
        while let Some(atom) = queue.pop_front() {
            vertices += 1;
            edges += adjacent.get(atom)?.len();
            for &other in adjacent.get(atom)? {
                if seen.insert(other) {
                    queue.push_back(other);
                }
            }
        }
        if edges / 2 != vertices - 1 {
            continue;
        }
        let (first, _) = farthest(start, &adjacent)?;
        let (mut last, previous) = farthest(first, &adjacent)?;
        let mut path = vec![last];
        while let Some(&Some(other)) = previous.get(last) {
            path.push(other);
            last = other;
        }
        if path.first() > path.last() {
            path.reverse();
        }
        if path.len() > longest.len() {
            longest = path;
        }
    }
    if longest.len() < 32 {
        return None;
    }
    let path: HashSet<_> = longest.iter().copied().collect();
    let mut visited = path.clone();
    for start in 0..n {
        if !visited.insert(start) {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let (mut size, mut attachments) = (0, 0);
        while let Some(atom) = queue.pop_front() {
            size += 1;
            if size > 16 {
                return None;
            }
            for &other in full.get(atom)? {
                if path.contains(&other) {
                    attachments += 1;
                } else if visited.insert(other) {
                    queue.push_back(other);
                }
            }
        }
        if attachments != 1 {
            return None;
        }
    }
    Some(longest)
}

fn supported_stereo(state: &State) -> bool {
    state.metadata.atoms.iter().all(|atom| atom.chiral_tag <= 2)
        && state.metadata.bonds.iter().all(|bond| bond.stereo <= 1)
}

fn seed(path: &[usize], wave: bool) -> depict::geometry::Coordinates {
    let mut coordinates = depict::geometry::Coordinates::new();
    let (mut x, mut y) = (0., 0.);
    let mut directions = if wave {
        vec![60_f64, 0., -60., 0.].into_iter().cycle()
    } else {
        vec![30_f64, -30.].into_iter().cycle()
    };
    for (index, &atom) in path.iter().enumerate() {
        if index > 0 {
            let angle = directions.next().unwrap_or_default().to_radians();
            x += 1.5 * angle.cos();
            y += 1.5 * angle.sin();
        }
        coordinates.insert(atom, depict::geometry::Point { x, y });
    }
    coordinates
}

fn label_document(state: &State, positions: &[Point3]) -> Option<Document> {
    let molecule = molecular::Molecule {
        rdkit_version: crate::chemistry::RDKIT_VERSION,
        ids: (1..=state.graph.atoms.len() as u64).collect(),
        positions: positions.to_vec(),
        state: state.clone(),
    };
    let drawing = molecular::for_import(&molecule, false, &vec![None; positions.len()]).ok()?;
    // Default import does not display CIP annotations. Defer their expensive
    // calculation to the normal publishing pipeline; H/isotope/charge ink and
    // graph-derived wedges are already present in this detached drawing.
    let labels = molecular::Labels {
        rdkit_version: crate::chemistry::RDKIT_VERSION.into(),
        atoms: vec![None; positions.len()],
        bonds: state
            .metadata
            .bonds
            .iter()
            .map(|bond| molecular::BondLabel {
                code: None,
                stereo: bond.stereo,
                stereo_atoms: bond.stereo_atoms.clone(),
            })
            .collect(),
    };
    drawing.finish(labels).ok()
}

pub(super) fn improve(
    state: &State,
    ranks: &[depict::ranks::AtomProperties],
    original: Vec<Point3>,
) -> Layout {
    improve_with_limit(state, ranks, original, 20_000_000)
}

pub(in crate::engine) fn for_cleanup(
    state: &State,
    original: Vec<Point3>,
    bond_length: f64,
    fixed: bool,
) -> Layout {
    if backbone(state).is_none() {
        return Layout {
            positions: original,
            warnings: vec![],
        };
    }
    // The fallback seeds and clearance thresholds use the native 1.5-unit
    // bond length. Cleanup restores the drawing's scale before its rigid
    // orientation and identity/visible-stereo checks run.
    let scale = bond_length / 1.5;
    let normalized = original
        .iter()
        .map(|p| Point3 {
            x: p.x / scale,
            y: p.y / scale,
            z: p.z,
        })
        .collect::<Vec<_>>();
    if clearance::geometry(state, &normalized) == Quality::Clear {
        return Layout {
            positions: original,
            warnings: vec![],
        };
    }
    if fixed {
        // A new backbone would move the caller's fixed atoms. Preserve the
        // solver's exact coordinates and report any remaining crowded chain.
        return Layout {
            positions: original,
            warnings: vec![NOTICE.into()],
        };
    }
    let ranks = vec![depict::ranks::AtomProperties::default(); state.graph.atoms.len()];
    let mut layout = improve(state, &ranks, normalized);
    if !layout.warnings.is_empty() {
        layout.positions = original;
        return layout;
    }
    for p in &mut layout.positions {
        p.x *= scale;
        p.y *= scale;
    }
    layout
}

fn improve_with_limit(
    state: &State,
    ranks: &[depict::ranks::AtomProperties],
    original: Vec<Point3>,
    work_limit: usize,
) -> Layout {
    let mut layout = Layout {
        positions: original,
        warnings: vec![],
    };
    // Leave unrelated imports, including their warnings, exactly as before.
    // Stereo exclusions apply after discovering the long-backbone topology so
    // a crowded long chain still receives an honest warning when excluded.
    let Some(path) = backbone(state) else {
        return layout;
    };
    if clearance::geometry(state, &layout.positions) == Quality::Clear {
        // Clear/native drawings keep their reference appearance. The label
        // pass is only needed for the long-backbone fallback's candidates.
        return layout;
    }
    if supported_stereo(state) {
        for wave in [false, true] {
            let coordinates = seed(&path, wave);
            if let Ok(candidate) = depict::compute_with_rank_properties(
                state,
                ranks,
                Some(&coordinates),
                depict::Options {
                    work_limit,
                    ..Default::default()
                },
            ) && clearance::geometry(state, &candidate.positions) == Quality::Clear
                && let Some(document) = label_document(state, &candidate.positions)
                && clearance::labels(state, &candidate.positions, &document) == Quality::Clear
            {
                layout.positions = candidate.positions;
                return layout;
            }
        }
    }
    layout.warnings.push(NOTICE.into());
    layout
}

#[cfg(test)]
mod tests;
