//! ReShiki readability refinements after native depiction.
//!
//! Align and orient fresh layouts, arrange their components, and repair inward
//! terminal substituents without changing chemistry or reference depiction.
use super::{depict::geometry::Coordinates, stereo::Point3, stereo::perception::State};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    f64::consts::{PI, TAU},
};

/// Rotate a fresh layout by at most 15 degrees so most of its acyclic and
/// six-ring bonds lie on the 30-degree grid (#740). Candidate directions and
/// cluster midpoints keep the fit on bond directions, rather than stopping at
/// the edge of the 1.5-degree tolerance. The layout turns as one rigid body,
/// so packed components keep their spacing, about its centroid or about
/// `pivot` (cleanup's single fixed atom).
pub(crate) fn straighten(state: &State, points: &mut [Point3], pivot: Option<usize>) {
    let step = PI / 6.;
    let tolerance = 1.5_f64.to_radians();
    let epsilon = 1e-9;
    let sides = |ring: &Vec<usize>| {
        ring.iter()
            .zip(ring.iter().cycle().skip(1))
            .take(ring.len())
            .map(|(&a, &b)| (a.min(b), a.max(b)))
            .collect::<Vec<_>>()
    };
    let excluded: HashSet<_> = state
        .rings
        .atoms
        .iter()
        .filter(|ring| ring.len() != 6)
        .flat_map(sides)
        .collect();
    let direction = |(a, b): (usize, usize)| {
        points
            .get(a)
            .zip(points.get(b))
            .map(|(a, b)| (b.y - a.y).atan2(b.x - a.x))
    };
    let vertical: Vec<_> = state
        .rings
        .atoms
        .iter()
        .find(|ring| ring.len() == 6)
        .into_iter()
        .flat_map(sides)
        .filter_map(direction)
        .collect();
    let mut phases: Vec<_> = state
        .graph
        .bonds
        .iter()
        .map(|bond| (bond.a.min(bond.b), bond.a.max(bond.b)))
        .filter(|bond| !excluded.contains(bond))
        .filter_map(direction)
        .map(|angle| angle.rem_euclid(step))
        .collect();
    let count = phases.len();
    phases.sort_by(f64::total_cmp);
    phases.extend(phases.clone().into_iter().map(|angle| angle + step));
    let candidates =
        [0., -step / 2., step / 2.]
            .into_iter()
            .chain(phases.iter().take(count).flat_map(|&first| {
                let end =
                    phases.partition_point(|&angle| angle <= first + 2. * tolerance + epsilon);
                let last = phases.get(end - 1).copied().unwrap_or(first);
                [first, (first + last) / 2.]
                    .map(|angle| (step / 2. - angle).rem_euclid(step) - step / 2.)
            }));
    let (mut best_count, mut best_angle, mut best_vertical) = (0, 0_f64, 0_f64);
    for angle in candidates {
        let target = (-angle - tolerance).rem_euclid(step) + tolerance;
        let on_grid = phases.partition_point(|&p| p <= target + tolerance + epsilon)
            - phases.partition_point(|&p| p < target - tolerance - epsilon);
        // Six-ring sides at 90 degrees have cos(6 * direction) == -1;
        // horizontal-sided rings have +1. Only use this for equal rotations.
        let sides: f64 = vertical
            .iter()
            .map(|side| (6. * (side + angle)).cos())
            .sum();
        if on_grid > best_count
            || on_grid == best_count
                && (angle.abs() < best_angle.abs() - epsilon
                    || (angle.abs() - best_angle.abs()).abs() <= epsilon
                        && sides < best_vertical - epsilon)
        {
            (best_count, best_angle, best_vertical) = (on_grid, angle, sides);
        }
    }
    if best_count * 2 <= count || best_angle.abs() < epsilon {
        return;
    }
    let n = points.len() as f64;
    let center = pivot
        .and_then(|p| points.get(p).copied())
        .unwrap_or(Point3 {
            x: points.iter().map(|p| p.x).sum::<f64>() / n,
            y: points.iter().map(|p| p.y).sum::<f64>() / n,
            z: 0.,
        });
    let (sin, cos) = best_angle.sin_cos();
    for point in points {
        let (x, y) = (point.x - center.x, point.y - center.y);
        point.x = center.x + cos * x - sin * y;
        point.y = center.y + sin * x + cos * y;
    }
}

fn adjacency(state: &State) -> Option<Vec<Vec<usize>>> {
    let mut adjacent = vec![Vec::new(); state.graph.atoms.len()];
    for bond in &state.graph.bonds {
        adjacent.get_mut(bond.a)?.push(bond.b);
        adjacent.get_mut(bond.b)?.push(bond.a);
    }
    Some(adjacent)
}

fn components(adjacent: &[Vec<usize>]) -> Option<Vec<Vec<usize>>> {
    let mut seen = vec![false; adjacent.len()];
    let mut result = Vec::new();
    for start in 0..adjacent.len() {
        if *seen.get(start)? {
            continue;
        }
        *seen.get_mut(start)? = true;
        let mut pending = vec![start];
        let mut atoms = Vec::new();
        while let Some(atom) = pending.pop() {
            atoms.push(atom);
            for &other in adjacent.get(atom)? {
                if !*seen.get(other)? {
                    *seen.get_mut(other)? = true;
                    pending.push(other);
                }
            }
        }
        atoms.sort_unstable();
        result.push(atoms);
    }
    Some(result)
}

fn bond_length(state: &State, points: &[Point3]) -> f64 {
    if state.graph.bonds.is_empty() {
        return 1.5;
    }
    state
        .graph
        .bonds
        .iter()
        .filter_map(|bond| points.get(bond.a).zip(points.get(bond.b)))
        .map(|(a, b)| (b.x - a.x).hypot(b.y - a.y))
        .sum::<f64>()
        / state.graph.bonds.len() as f64
}

fn centroid(points: &[Point3], atoms: &[usize]) -> Point3 {
    Point3 {
        x: atoms
            .iter()
            .filter_map(|&id| points.get(id))
            .map(|p| p.x)
            .sum::<f64>()
            / atoms.len() as f64,
        y: atoms
            .iter()
            .filter_map(|&id| points.get(id))
            .map(|p| p.y)
            .sum::<f64>()
            / atoms.len() as f64,
        z: 0.,
    }
}

/// Choose grid-preserving reflections about each component's centroid (#844).
/// Signed preferences score heteroatoms right (4), each exocyclic C=O/C=N up
/// (3), the largest ring system left/bottom (2/1), and fused-ring heteroatoms
/// right/bottom (1/1). Offsets below 0.1 bond length do not vote; identity wins
/// ties. Coordinates here are molfile-style y-up; drawing construction later
/// inverts y and assigns wedges from stereo state at the final positions.
pub(crate) fn conventional_orientation(state: &State, points: &mut [Point3]) {
    let _ = (|| -> Option<()> {
        let adjacent = adjacency(state)?;
        let components = components(&adjacent)?;
        let mut component_of = vec![0; points.len()];
        for (index, atoms) in components.iter().enumerate() {
            for &id in atoms {
                *component_of.get_mut(id)? = index;
            }
        }
        let tolerance = bond_length(state, points) * 0.1;
        let sign = |offset: f64| {
            if offset.abs() < tolerance {
                0
            } else if offset > 0. {
                1
            } else {
                -1
            }
        };
        let hetero = |id: usize| {
            state
                .graph
                .atoms
                .get(id)
                .is_some_and(|atom| !matches!(atom.atomic_number, 1 | 6))
        };
        let mut scores = vec![(0, 0); components.len()];
        let centers: Vec<_> = components
            .iter()
            .map(|atoms| centroid(points, atoms))
            .collect();
        for (index, atoms) in components.iter().enumerate() {
            let heteroatoms: Vec<_> = atoms.iter().copied().filter(|&id| hetero(id)).collect();
            if !heteroatoms.is_empty() {
                scores.get_mut(index)?.0 +=
                    4 * sign(centroid(points, &heteroatoms).x - centers.get(index)?.x);
            }
        }
        let ring_bonds: HashSet<_> = state
            .rings
            .atoms
            .iter()
            .flat_map(|ring| {
                ring.iter()
                    .zip(ring.iter().cycle().skip(1))
                    .take(ring.len())
                    .map(|(&a, &b)| (a.min(b), a.max(b)))
            })
            .collect();
        for bond in &state.graph.bonds {
            if bond.order != 2 || ring_bonds.contains(&(bond.a.min(bond.b), bond.a.max(bond.b))) {
                continue;
            }
            for (carbon, branch) in [(bond.a, bond.b), (bond.b, bond.a)] {
                if state.graph.atoms.get(carbon)?.atomic_number == 6
                    && (state.graph.atoms.get(branch)?.atomic_number == 7
                        || state.graph.atoms.get(branch)?.atomic_number == 8
                            && adjacent.get(branch)?.len() == 1)
                {
                    scores.get_mut(*component_of.get(carbon)?)?.1 +=
                        3 * sign(points.get(branch)?.y - points.get(carbon)?.y);
                }
            }
        }
        // Rings sharing any atom form one system, including spiro junctions.
        let mut memberships = vec![Vec::new(); points.len()];
        for (index, ring) in state.rings.atoms.iter().enumerate() {
            for &atom in ring {
                memberships.get_mut(atom)?.push(index);
            }
        }
        let mut seen_rings = vec![false; state.rings.atoms.len()];
        let mut seen_atoms = vec![false; points.len()];
        let mut largest = vec![0; components.len()];
        let mut ring_scores = vec![(0, 0); components.len()];
        for start in 0..state.rings.atoms.len() {
            if *seen_rings.get(start)? {
                continue;
            }
            *seen_rings.get_mut(start)? = true;
            let mut pending = vec![start];
            let mut atoms = Vec::new();
            let mut edges = HashSet::new();
            let mut fused = false;
            while let Some(ring) = pending.pop() {
                let ring = state.rings.atoms.get(ring)?;
                for (&a, &b) in ring
                    .iter()
                    .zip(ring.iter().cycle().skip(1))
                    .take(ring.len())
                {
                    fused |= !edges.insert((a.min(b), a.max(b)));
                }
                for &atom in ring {
                    if !*seen_atoms.get(atom)? {
                        *seen_atoms.get_mut(atom)? = true;
                        atoms.push(atom);
                        for &other in memberships.get(atom)? {
                            if !*seen_rings.get(other)? {
                                *seen_rings.get_mut(other)? = true;
                                pending.push(other);
                            }
                        }
                    }
                }
            }
            let index = *component_of.get(*atoms.first()?)?;
            let center = centroid(points, &atoms);
            if atoms.len() > *largest.get(index)? {
                *largest.get_mut(index)? = atoms.len();
                *ring_scores.get_mut(index)? = (
                    -2 * sign(center.x - centers.get(index)?.x),
                    -sign(center.y - centers.get(index)?.y),
                );
            }
            let heteroatoms: Vec<_> = atoms.into_iter().filter(|&id| hetero(id)).collect();
            if fused && !heteroatoms.is_empty() {
                let heterocenter = centroid(points, &heteroatoms);
                scores.get_mut(index)?.0 += sign(heterocenter.x - center.x);
                scores.get_mut(index)?.1 -= sign(heterocenter.y - center.y);
            }
        }
        for (index, atoms) in components.iter().enumerate() {
            let (x, y) = (
                scores.get(index)?.0 + ring_scores.get(index)?.0,
                scores.get(index)?.1 + ring_scores.get(index)?.1,
            );
            let (mut best, mut variant) = (x + y, (1, 1));
            for (sx, sy) in [(-1, 1), (1, -1), (-1, -1)] {
                let score = sx * x + sy * y;
                if score > best {
                    (best, variant) = (score, (sx, sy));
                }
            }
            if variant == (1, 1) {
                continue;
            }
            let center = centers.get(index)?;
            for &id in atoms {
                let point = points.get_mut(id)?;
                point.x = center.x + f64::from(variant.0) * (point.x - center.x);
                point.y = center.y + f64::from(variant.1) * (point.y - center.y);
            }
        }
        Some(())
    })();
}

#[derive(Clone, Copy)]
struct Fusion {
    a: usize,
    b: usize,
    bond: (usize, usize),
}

struct FusedSystem {
    rings: Vec<usize>,
    atoms: Vec<usize>,
    fusions: Vec<Fusion>,
}

fn largest_fused_system(state: &State) -> Option<FusedSystem> {
    let mut edges = BTreeMap::<_, Vec<usize>>::new();
    for (index, ring) in state.rings.atoms.iter().enumerate() {
        for (&a, &b) in ring
            .iter()
            .zip(ring.iter().cycle().skip(1))
            .take(ring.len())
        {
            edges.entry((a.min(b), a.max(b))).or_default().push(index);
        }
    }
    let mut shared = BTreeMap::<_, Vec<_>>::new();
    for (bond, rings) in edges {
        for (index, &a) in rings.iter().enumerate() {
            for &b in rings.iter().skip(index + 1) {
                shared.entry((a, b)).or_default().push(bond);
            }
        }
    }
    // Ortho- and peri-fused rings share exactly one bond; bridged rings share more.
    let mut adjacent = vec![Vec::new(); state.rings.atoms.len()];
    let mut fusions = Vec::new();
    for ((a, b), bonds) in shared {
        if let [bond] = bonds[..] {
            adjacent.get_mut(a)?.push(b);
            adjacent.get_mut(b)?.push(a);
            fusions.push(Fusion { a, b, bond });
        }
    }
    let mut best: Option<FusedSystem> = None;
    for rings in components(&adjacent)? {
        let mut atoms: Vec<_> = rings
            .iter()
            .filter_map(|&ring| state.rings.atoms.get(ring))
            .flatten()
            .copied()
            .collect();
        atoms.sort_unstable();
        atoms.dedup();
        if best
            .as_ref()
            .is_none_or(|best| (rings.len(), atoms.len()) > (best.rings.len(), best.atoms.len()))
        {
            best = Some(FusedSystem {
                rings,
                atoms,
                fusions: Vec::new(),
            });
        }
    }
    let mut best = best?;
    let mut indices = vec![None; state.rings.atoms.len()];
    for (index, &ring) in best.rings.iter().enumerate() {
        *indices.get_mut(ring)? = Some(index);
    }
    best.fusions = fusions
        .into_iter()
        .filter_map(|fusion| {
            Some(Fusion {
                a: indices.get(fusion.a).copied().flatten()?,
                b: indices.get(fusion.b).copied().flatten()?,
                bond: fusion.bond,
            })
        })
        .collect();
    Some(best)
}

fn fused_orientation_score(
    system: &FusedSystem,
    centers: &[Point3],
    bonds: &[(Point3, Point3)],
    tolerance: f64,
) -> Option<[i64; 4]> {
    let mut adjacent = vec![Vec::new(); centers.len()];
    let mut horizontal = HashMap::new();
    for (index, fusion) in system.fusions.iter().enumerate() {
        let (a, b) = bonds.get(index)?;
        let (left, right) = (centers.get(fusion.a)?, centers.get(fusion.b)?);
        if (b.x - a.x).abs() <= tolerance && (right.y - left.y).abs() <= tolerance {
            adjacent.get_mut(fusion.a)?.push(fusion.b);
            adjacent.get_mut(fusion.b)?.push(fusion.a);
            horizontal
                .entry((fusion.a.min(fusion.b), fusion.a.max(fusion.b)))
                .or_insert(index);
        }
    }
    let mut best = None;
    // Rings joined through vertical common bonds at one height form a row.
    for row in components(&adjacent)? {
        let mut row = row
            .into_iter()
            .map(|ring| Some((ring, *centers.get(ring)?)))
            .collect::<Option<Vec<_>>>()?;
        row.sort_by(|(a, p), (b, q)| p.x.total_cmp(&q.x).then(a.cmp(b)));
        let middle = row.len() / 2;
        let axis = if row.len() % 2 == 1 {
            row.get(middle)?.1
        } else {
            let (a, b) = (row.get(middle - 1)?.0, row.get(middle)?.0);
            let &(a, b) = bonds.get(*horizontal.get(&(a.min(b), a.max(b)))?)?;
            Point3 {
                x: (a.x + b.x) * 0.5,
                y: (a.y + b.y) * 0.5,
                z: 0.,
            }
        };
        let half = |offset: f64| {
            if offset.abs() <= tolerance {
                1
            } else if offset > 0. {
                2
            } else {
                0
            }
        };
        let (mut upper_right, mut lower_left, mut above) = (0, 0, 0);
        for center in centers {
            let (right, upper) = (half(center.x - axis.x), half(center.y - axis.y));
            upper_right += right * upper;
            lower_left += (2 - right) * (2 - upper);
            above += 2 * upper;
        }
        // Quarter-ring units retain the namer's half votes on either axis.
        let score = [row.len() as i64, upper_right, -lower_left, above];
        if best.is_none_or(|best| score > best) {
            best = Some(score);
        }
    }
    best
}

/// Orient the largest bond-fused ring system using IUPAC's horizontal-row and
/// quadrant preferences. Every common bond supplies the two rotations making
/// it exactly vertical, with or without reflection in the vertical axis.
/// All atoms move as one rigid body about their centroid, in y-up coordinates;
/// cleanup assigns wedges afterwards from the unchanged stereo state.
pub(crate) fn iupac_orientation(state: &State, points: &mut [Point3]) {
    let transform = (|| -> Option<_> {
        let system = largest_fused_system(state)?;
        if system.rings.len() < 2 || points.len() != state.graph.atoms.len() {
            return None;
        }
        let center = centroid(points, &(0..points.len()).collect::<Vec<_>>());
        let centers: Vec<_> = system
            .rings
            .iter()
            .map(|&ring| Some(centroid(points, state.rings.atoms.get(ring)?)))
            .collect::<Option<_>>()?;
        let bonds = system
            .fusions
            .iter()
            .map(|fusion| Some((*points.get(fusion.bond.0)?, *points.get(fusion.bond.1)?)))
            .collect::<Option<Vec<_>>>()?;
        let tolerance = bond_length(state, points) * 0.1;
        let mut best: Option<([i64; 4], bool, f64)> = None;
        for &(a, b) in &bonds {
            if (b.x - a.x).hypot(b.y - a.y) < 1e-12 {
                continue;
            }
            let vertical = PI * 0.5 - (b.y - a.y).atan2(b.x - a.x);
            for angle in [vertical, vertical + PI] {
                let angle = (angle + PI).rem_euclid(TAU) - PI;
                let angle = if angle.abs() < 1e-12 { 0. } else { angle };
                let (sin, cos) = angle.sin_cos();
                for reflect in [false, true] {
                    let apply = |point: Point3| {
                        let (x, y) = (point.x - center.x, point.y - center.y);
                        Point3 {
                            x: (cos * x - sin * y) * if reflect { -1. } else { 1. },
                            y: sin * x + cos * y,
                            z: point.z,
                        }
                    };
                    let centers = centers.iter().copied().map(apply).collect::<Vec<_>>();
                    let bonds = bonds
                        .iter()
                        .map(|&(a, b)| (apply(a), apply(b)))
                        .collect::<Vec<_>>();
                    let score = fused_orientation_score(&system, &centers, &bonds, tolerance)?;
                    if best.is_none_or(|(best_score, best_reflect, best_angle)| {
                        score > best_score
                            || score == best_score
                                && (!reflect && best_reflect
                                    || reflect == best_reflect
                                        && (angle.abs() < best_angle.abs() - 1e-12
                                            || (angle.abs() - best_angle.abs()).abs() <= 1e-12
                                                && angle < best_angle))
                    }) {
                        best = Some((score, reflect, angle));
                    }
                }
            }
        }
        best.map(|(_, reflect, angle)| (center, reflect, angle))
    })();
    let Some((center, reflect, angle)) = transform else {
        conventional_orientation(state, points);
        return;
    };
    if !reflect && angle == 0. {
        return;
    }
    let (sin, cos) = angle.sin_cos();
    for point in points {
        let (x, y) = (point.x - center.x, point.y - center.y);
        point.x = center.x + (cos * x - sin * y) * if reflect { -1. } else { 1. };
        point.y = center.y + sin * x + cos * y;
    }
}

/// Ordered rigid groups from a fresh layout, including their paired ions.
#[derive(Default)]
pub(crate) struct ComponentRow(Vec<Vec<usize>>);

/// Arrange fresh import components by decreasing heavy-atom count, then charge
/// sign (cation, neutral, anion) on exact size ties, retaining input order for
/// other ties (#843). Align bounding-box centers with a one-bond gap, then pair
/// monoatomic ions with available opposite charges along their free bisectors.
pub(crate) fn arrange_components(state: &State, points: &mut [Point3]) -> ComponentRow {
    (|| -> Option<ComponentRow> {
        let adjacent = adjacency(state)?;
        let mut components = components(&adjacent)?;
        if components.len() < 2 {
            return Some(ComponentRow::default());
        }
        let length = bond_length(state, points);
        components.sort_by_cached_key(|atoms| {
            let heavy = atoms
                .iter()
                .filter_map(|&id| state.graph.atoms.get(id))
                .filter(|atom| atom.atomic_number > 1)
                .count();
            let charge: i32 = atoms
                .iter()
                .filter_map(|&id| state.graph.atoms.get(id))
                .map(|atom| i32::from(atom.charge))
                .sum();
            (std::cmp::Reverse(heavy), std::cmp::Reverse(charge.signum()))
        });
        let mut left = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let bottom = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let top = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        let baseline = (bottom + top) * 0.5;
        for atoms in &components {
            let min_x = atoms
                .iter()
                .filter_map(|&id| points.get(id))
                .map(|p| p.x)
                .fold(f64::INFINITY, f64::min);
            let max_x = atoms
                .iter()
                .filter_map(|&id| points.get(id))
                .map(|p| p.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = atoms
                .iter()
                .filter_map(|&id| points.get(id))
                .map(|p| p.y)
                .fold(f64::INFINITY, f64::min);
            let max_y = atoms
                .iter()
                .filter_map(|&id| points.get(id))
                .map(|p| p.y)
                .fold(f64::NEG_INFINITY, f64::max);
            for &id in atoms {
                let point = points.get_mut(id)?;
                point.x += left - min_x;
                point.y += baseline - (min_y + max_y) * 0.5;
            }
            left += max_x - min_x + length;
        }
        let sites: Vec<_> = components
            .iter()
            .filter(|part| part.len() > 1)
            .flatten()
            .copied()
            .filter(|&id| {
                state
                    .graph
                    .atoms
                    .get(id)
                    .is_some_and(|atom| atom.charge != 0)
            })
            .collect();
        let mut paired = HashSet::new();
        let mut row = components.clone();
        for atoms in &components {
            let &[ion] = atoms.as_slice() else {
                continue;
            };
            let charge = state.graph.atoms.get(ion)?.charge;
            if charge == 0 {
                continue;
            }
            for &atom in &sites {
                if paired.contains(&atom)
                    || state.graph.atoms.get(atom)?.charge.signum() != -charge.signum()
                {
                    continue;
                }
                let origin = *points.get(atom)?;
                let (mut x, mut y) = (0., 0.);
                for &neighbor in adjacent.get(atom)? {
                    let neighbor = points.get(neighbor)?;
                    let (dx, dy) = (neighbor.x - origin.x, neighbor.y - origin.y);
                    let distance = dx.hypot(dy);
                    x -= dx / distance;
                    y -= dy / distance;
                }
                let distance = x.hypot(y);
                if distance < 1e-9 {
                    continue;
                }
                let target = Point3 {
                    x: origin.x + length * x / distance,
                    y: origin.y + length * y / distance,
                    z: points.get(ion)?.z,
                };
                // Keep the ion clear of atoms and of bonds passing between them.
                let clear_of_atoms = points.iter().enumerate().all(|(id, p)| {
                    id == ion || (p.x - target.x).hypot(p.y - target.y) >= length * 0.5
                });
                let clear_of_bonds = state.graph.bonds.iter().all(|bond| {
                    let (Some(a), Some(b)) = (points.get(bond.a), points.get(bond.b)) else {
                        return true;
                    };
                    let (dx, dy) = (b.x - a.x, b.y - a.y);
                    let t = (((target.x - a.x) * dx + (target.y - a.y) * dy) / (dx * dx + dy * dy))
                        .clamp(0., 1.);
                    (a.x + t * dx - target.x).hypot(a.y + t * dy - target.y) >= length * 0.5
                });
                if clear_of_atoms && clear_of_bonds {
                    *points.get_mut(ion)? = target;
                    paired.insert(atom);
                    row.retain(|part| part != atoms);
                    row.iter_mut().find(|part| part.contains(&atom))?.push(ion);
                    break;
                }
            }
        }
        Some(ComponentRow(row))
    })()
    .unwrap_or_default()
}

/// Widen only the arranged row's gaps for the finished labels (#1033).
pub(crate) fn space_component_labels(
    row: &ComponentRow,
    molecule: &mut super::document::Molecule,
    doc: &mut crate::document::Document,
) {
    if row.0.len() < 2 {
        return;
    }
    let gap = (bond_length(&molecule.state, &molecule.positions)
        * super::document::WORLD_PER_DEPICT_UNIT) as f32;
    let mut right = f32::NEG_INFINITY;
    for group in &row.0 {
        let ids: Vec<_> = group
            .iter()
            .filter_map(|&index| molecule.ids.get(index).copied())
            .collect();
        let ids = crate::reactions::molecules(doc, &ids).concat();
        // Measure each group alone so crowded neighbours cannot flip implicit H.
        let part = crate::document::Document {
            atoms: doc
                .atoms
                .iter()
                .filter(|a| ids.contains(&a.id))
                .cloned()
                .collect(),
            bonds: doc
                .bonds
                .iter()
                .filter(|b| ids.contains(&b.a))
                .cloned()
                .collect(),
            drawing_style: doc.drawing_style.clone(),
            atom_labels: doc.atom_labels.clone(),
            ..Default::default()
        };
        let (left, edge) = part
            .atoms
            .iter()
            .map(|a| (a.position, a.position))
            .chain(crate::scene::participant_label_ink_boxes(&part, &ids))
            .fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(left, right), (lo, hi)| (left.min(lo.x), right.max(hi.x)),
            );
        let dx = (right + gap - left).max(0.);
        right = edge + dx;
        for atom in &mut doc.atoms {
            if ids.contains(&atom.id) {
                atom.position.x += dx;
            }
        }
        for (id, point) in molecule.ids.iter().zip(&mut molecule.positions) {
            if ids.contains(id) {
                point.x += f64::from(dx) / super::document::WORLD_PER_DEPICT_UNIT;
            }
        }
    }
    if let Some(conformer) = &mut molecule.state.conformer {
        conformer.positions.clone_from(&molecule.positions);
    }
}

pub(crate) fn bridged(state: &State) -> bool {
    let mut remaining = 1_000_000usize;
    let mut shared = HashMap::<(usize, usize), usize>::new();
    let mut memberships = HashMap::<usize, Vec<usize>>::new();
    for (ring, atoms) in state.rings.atoms.iter().enumerate() {
        for &atom in atoms {
            let previous = memberships.entry(atom).or_default();
            for &other in previous.iter() {
                let Some(next) = remaining.checked_sub(1) else {
                    return false;
                };
                remaining = next;
                let count = shared.entry((other, ring)).or_default();
                *count += 1;
                if *count >= 3 {
                    return true;
                }
            }
            previous.push(ring);
        }
    }
    false
}

fn cross(a: Point3, b: Point3, c: Point3) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn crosses_other_bond(
    state: &State,
    points: &[Point3],
    leaf: usize,
    center: usize,
    end: Point3,
) -> bool {
    let Some(&start) = points.get(center) else {
        return true;
    };
    state.graph.bonds.iter().any(|bond| {
        if [bond.a, bond.b]
            .iter()
            .any(|&id| id == leaf || id == center)
        {
            return false;
        }
        let Some((&a, &b)) = points.get(bond.a).zip(points.get(bond.b)) else {
            return true;
        };
        let (ab_a, ab_b, cd_a, cd_b) = (
            cross(start, end, a),
            cross(start, end, b),
            cross(a, b, start),
            cross(a, b, end),
        );
        let on = |point: Point3, a: Point3, b: Point3| {
            point.x >= a.x.min(b.x) - 1e-9
                && point.x <= a.x.max(b.x) + 1e-9
                && point.y >= a.y.min(b.y) - 1e-9
                && point.y <= a.y.max(b.y) + 1e-9
        };
        ab_a * ab_b < -1e-12 && cd_a * cd_b < -1e-12
            || ab_a.abs() < 1e-12 && on(a, start, end)
            || ab_b.abs() < 1e-12 && on(b, start, end)
            || cd_a.abs() < 1e-12 && on(start, a, b)
            || cd_b.abs() < 1e-12 && on(end, a, b)
    })
}

fn adds_collision(
    points: &[Point3],
    leaf: usize,
    center: usize,
    old: Point3,
    new: Point3,
    clearance: f64,
) -> bool {
    points.iter().enumerate().any(|(id, point)| {
        id != leaf
            && id != center
            && (point.x - new.x).hypot(point.y - new.y) < clearance
            && (point.x - old.x).hypot(point.y - old.y) >= clearance
    })
}

fn bridged_junction(state: &State, rings: &[usize], remaining: &mut usize) -> bool {
    for (i, &first) in rings.iter().enumerate() {
        for &second in rings.iter().skip(i + 1) {
            let Some((a, b)) = state
                .rings
                .atoms
                .get(first)
                .zip(state.rings.atoms.get(second))
            else {
                return false;
            };
            let Some(next) = remaining.checked_sub(a.len().saturating_mul(b.len())) else {
                return false;
            };
            *remaining = next;
            if a.iter().filter(|id| b.contains(id)).take(3).count() == 3 {
                return true;
            }
        }
    }
    false
}

/// Leave fixed atoms, ring geometry, ordinary layouts and already exterior
/// substituents untouched. Every accepted move keeps its bond length and adds
/// neither a severe nonbonded collision nor a proper bond crossing.
pub(crate) fn exterior_terminals(state: &State, points: &mut [Point3], fixed: &Coordinates) {
    if points.len() != state.graph.atoms.len() || !bridged(state) {
        return;
    }
    let mut adjacent = vec![Vec::new(); points.len()];
    for bond in &state.graph.bonds {
        let Some(a) = adjacent.get_mut(bond.a) else {
            return;
        };
        a.push((bond.b, bond.order));
        let Some(b) = adjacent.get_mut(bond.b) else {
            return;
        };
        b.push((bond.a, bond.order));
    }
    let mut rings = vec![Vec::new(); points.len()];
    for (index, ring) in state.rings.atoms.iter().enumerate() {
        for &id in ring {
            let Some(count) = rings.get_mut(id) else {
                return;
            };
            count.push(index);
        }
    }
    let mut remaining = 1_000_000usize;
    for (leaf, atom) in state.graph.atoms.iter().enumerate() {
        if atom.atomic_number != 6
            || atom.charge != 0
            || atom.radical_electrons != 0
            || atom.aromatic
            || fixed.contains_key(&leaf)
        {
            continue;
        }
        let Some(&[(center, 1)]) = adjacent.get(leaf).map(Vec::as_slice) else {
            continue;
        };
        let Some(neighbors) = adjacent.get(center) else {
            continue;
        };
        let Some(center_rings) = rings.get(center) else {
            return;
        };
        if neighbors.len() != 4 || center_rings.len() < 2 {
            continue;
        }
        let Some(&origin) = points.get(center) else {
            return;
        };
        let Some(&old) = points.get(leaf) else {
            return;
        };
        let mut angles = Vec::with_capacity(3);
        for &(id, order) in neighbors {
            if id == leaf {
                continue;
            }
            if order != 1 || rings.get(id).is_none_or(Vec::is_empty) {
                break;
            }
            let Some(point) = points.get(id) else {
                return;
            };
            angles.push(
                (point.y - origin.y)
                    .atan2(point.x - origin.x)
                    .rem_euclid(TAU),
            );
        }
        if angles.len() != 3 {
            continue;
        }
        angles.sort_by(f64::total_cmp);
        let Some((&first, &last)) = angles.first().zip(angles.last()) else {
            continue;
        };
        let mut gap = (last, first + TAU - last);
        for pair in angles.windows(2) {
            if let [a, b] = pair
                && b - a > gap.1
            {
                gap = (*a, b - a);
            }
        }
        let old_angle = (old.y - origin.y).atan2(old.x - origin.x).rem_euclid(TAU);
        if gap.1 <= PI + 1e-6 || (old_angle - gap.0).rem_euclid(TAU) < gap.1 {
            continue;
        }
        if !bridged_junction(state, center_rings, &mut remaining) {
            continue;
        }
        let length = (old.x - origin.x).hypot(old.y - origin.y);
        if !length.is_finite() || length < 1e-6 {
            continue;
        }
        let cost = points
            .len()
            .saturating_add(state.graph.bonds.len())
            .saturating_mul(2);
        let Some(next) = remaining.checked_sub(cost) else {
            break;
        };
        remaining = next;
        let angle = gap.0 + gap.1 * 0.5;
        let new = Point3 {
            x: origin.x + length * angle.cos(),
            y: origin.y + length * angle.sin(),
            z: old.z,
        };
        if !adds_collision(points, leaf, center, old, new, length * 0.5)
            && !crosses_other_bond(state, points, leaf, center, new)
            && let Some(point) = points.get_mut(leaf)
        {
            *point = new;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chemistry::{depict, document, smiles};
    const ARTEMISININ: &str = "C[C@@H]1CC[C@H]2[C@H](C(=O)O[C@H]3[C@@]24[C@H]1CC[C@](O3)(OO4)C)C";

    fn layout(text: &str) -> anyhow::Result<(State, Vec<Point3>)> {
        let state = smiles::read(text)?.prepared.state;
        let positions = depict::compute(
            &state,
            &vec![None; state.graph.atoms.len()],
            None,
            depict::Options {
                use_ring_templates: true,
                ..Default::default()
            },
        )?
        .positions;
        Ok((state, positions))
    }

    fn iupac_score(state: &State, points: &[Point3]) -> [i64; 4] {
        let system = largest_fused_system(state).unwrap();
        let centers = system
            .rings
            .iter()
            .map(|&ring| centroid(points, &state.rings.atoms[ring]))
            .collect::<Vec<_>>();
        let bonds = system
            .fusions
            .iter()
            .map(|fusion| (points[fusion.bond.0], points[fusion.bond.1]))
            .collect::<Vec<_>>();
        fused_orientation_score(&system, &centers, &bonds, bond_length(state, points) * 0.1)
            .unwrap()
    }

    #[test]
    fn iupac_phenanthrene_and_anthracene_choose_horizontal_rows_rigidly() -> anyhow::Result<()> {
        for (text, expected) in [
            ("c1ccc2c(c1)ccc1ccccc12", [2, 6, -2, 8]),
            ("c1ccc2cc3ccccc3cc2c1", [3, 3, -3, 6]),
        ] {
            let (state, original) = layout(text)?;
            let center = centroid(&original, &(0..original.len()).collect::<Vec<_>>());
            for angle in [0_f64, 0.37, 1.9, -2.6] {
                for reflection in [1., -1.] {
                    let (sin, cos) = angle.sin_cos();
                    let mut points = original
                        .iter()
                        .map(|p| {
                            let (x, y) = (p.x - center.x, p.y - center.y);
                            Point3 {
                                x: center.x + reflection * (cos * x - sin * y),
                                y: center.y + sin * x + cos * y,
                                z: p.z,
                            }
                        })
                        .collect::<Vec<_>>();
                    iupac_orientation(&state, &mut points);
                    assert_eq!(iupac_score(&state, &points), expected, "{text}");
                    let system = largest_fused_system(&state).unwrap();
                    let centers = system
                        .rings
                        .iter()
                        .map(|&ring| centroid(&points, &state.rings.atoms[ring]))
                        .collect::<Vec<_>>();
                    let row = system
                        .fusions
                        .iter()
                        .filter(|fusion| (centers[fusion.a].y - centers[fusion.b].y).abs() < 1e-8)
                        .collect::<Vec<_>>();
                    for fusion in &row {
                        assert!((points[fusion.bond.0].x - points[fusion.bond.1].x).abs() < 1e-8);
                    }
                    if expected[0] == 2 {
                        assert_eq!(row.len(), 1);
                        let row = row[0];
                        let third = (0..centers.len())
                            .find(|&id| id != row.a && id != row.b)
                            .unwrap();
                        let axis_x = (points[row.bond.0].x + points[row.bond.1].x) * 0.5;
                        assert!(centers[third].x > axis_x);
                        assert!(
                            centers[third].y > centers[row.a].y,
                            "The third ring must be above the row in y-up coordinates"
                        );
                    } else {
                        assert_eq!(row.len(), 2);
                        assert!(
                            centers
                                .iter()
                                .all(|point| (point.y - centers[0].y).abs() < 1e-8)
                        );
                    }
                    let after = centroid(&points, &(0..points.len()).collect::<Vec<_>>());
                    assert!((after.x - center.x).hypot(after.y - center.y) < 1e-12);
                    for (a, point) in points.iter().enumerate() {
                        for (b, other) in points.iter().enumerate() {
                            let before = (original[a].x - original[b].x)
                                .hypot(original[a].y - original[b].y);
                            let after = (point.x - other.x).hypot(point.y - other.y);
                            assert!((before - after).abs() < 1e-12);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn iupac_peri_pyrene_is_deterministic_and_idempotent() -> anyhow::Result<()> {
        let (mut state, original) = layout("c1cc2ccc3cccc4ccc(c1)c2c34")?;
        assert_eq!(state.rings.atoms.len(), 4);
        let mut preferred = original.clone();
        iupac_orientation(&state, &mut preferred);
        assert_eq!(iupac_score(&state, &preferred)[0], 2);
        for _ in 0..3 {
            let mut repeated = original.clone();
            iupac_orientation(&state, &mut repeated);
            assert_eq!(repeated, preferred);
            iupac_orientation(&state, &mut repeated);
            assert_eq!(repeated, preferred);
        }
        state.rings.atoms.reverse();
        let mut reordered = original;
        iupac_orientation(&state, &mut reordered);
        assert_eq!(reordered, preferred);
        Ok(())
    }

    #[test]
    fn iupac_unfused_layouts_use_conventional_orientation() -> anyhow::Result<()> {
        for text in ["CC(=O)O", "Oc1ccccc1", "C1CCC2(CC1)CCNC2", "C1CC2CCC1C2"] {
            let (state, mut actual) = layout(text)?;
            let mut expected = actual.clone();
            conventional_orientation(&state, &mut expected);
            iupac_orientation(&state, &mut actual);
            assert_eq!(actual, expected, "{text}");
        }
        Ok(())
    }

    #[test]
    fn iupac_largest_system_prefers_ring_count_then_atom_count() -> anyhow::Result<()> {
        for text in ["c1ccc2ccccc2c1C1CCCCCCCCCC1", "C1CCC2CCCC2C1c1ccc2ccccc2c1"] {
            let state = smiles::read(text)?.prepared.state;
            let system = largest_fused_system(&state).unwrap();
            assert_eq!((system.rings.len(), system.atoms.len()), (2, 10));
        }
        Ok(())
    }

    #[test]
    fn iupac_rows_require_common_vertical_bonds() {
        let system = FusedSystem {
            rings: vec![0, 1, 2, 3],
            atoms: vec![],
            fusions: (0..3)
                .map(|a| Fusion {
                    a,
                    b: a + 1,
                    bond: (0, 1),
                })
                .collect(),
        };
        let centers = (0..4)
            .map(|x| Point3 {
                x: x as f64,
                y: 0.,
                z: 0.,
            })
            .collect::<Vec<_>>();
        let mut bonds = (0..3)
            .map(|x| {
                (
                    Point3 {
                        x: x as f64 + 0.5,
                        y: -0.5,
                        z: 0.,
                    },
                    Point3 {
                        x: x as f64 + 0.5,
                        y: 0.5,
                        z: 0.,
                    },
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            fused_orientation_score(&system, &centers, &bonds, 0.1).unwrap()[0],
            4
        );
        bonds[1].1.x += 0.2;
        assert_eq!(
            fused_orientation_score(&system, &centers, &bonds, 0.1).unwrap()[0],
            2
        );
    }

    #[test]
    fn conventional_variants_converge_without_changing_component_geometry() -> anyhow::Result<()> {
        let state = smiles::read("CC(=O)O")?.prepared.state;
        let preferred =
            [(0., 0.), (1., 0.), (1., 1.), (2., 0.)].map(|(x, y)| Point3 { x, y, z: 0. });
        let center = centroid(&preferred, &[0, 1, 2, 3]);
        for (sx, sy) in [(1., 1.), (-1., 1.), (1., -1.), (-1., -1.)] {
            let mut points = preferred.map(|p| Point3 {
                x: center.x + sx * (p.x - center.x),
                y: center.y + sy * (p.y - center.y),
                z: p.z,
            });
            conventional_orientation(&state, &mut points);
            assert_eq!(points, preferred);
        }
        let mut symmetric = preferred;
        let state = smiles::read("CCCC")?.prepared.state;
        conventional_orientation(&state, &mut symmetric);
        assert_eq!(symmetric, preferred, "Identity must win when no rule votes");
        let state = smiles::read("CCO")?.prepared.state;
        let mut noise = [
            Point3 {
                x: -1.5,
                y: 0.,
                z: 0.,
            },
            Point3 {
                x: 1.5,
                y: 0.,
                z: 0.,
            },
            Point3 {
                x: -0.01,
                y: 0.,
                z: 0.,
            },
        ];
        let before = noise;
        conventional_orientation(&state, &mut noise);
        assert_eq!(
            noise, before,
            "Small centroid offsets must not flip a layout"
        );
        Ok(())
    }

    #[test]
    fn conventional_ring_preferences_join_spiro_rings_and_lower_fused_heteroatoms()
    -> anyhow::Result<()> {
        let state = smiles::read("C1CCC2(CC1)CCCC2CCc1ccccc1")?.prepared.state;
        let (first, second) = (&state.rings.atoms[0], &state.rings.atoms[1]);
        assert!(first.iter().any(|id| second.contains(id)));
        let system: HashSet<_> = first.iter().chain(second).copied().collect();
        assert!(system.len() > state.rings.atoms[2].len());
        let mut points: Vec<_> = (0..state.graph.atoms.len())
            .map(|id| {
                let offset = if system.contains(&id) { 3. } else { -3. };
                Point3 {
                    x: offset + id as f64 * 0.01,
                    y: offset,
                    z: 0.,
                }
            })
            .collect();
        conventional_orientation(&state, &mut points);
        let whole = centroid(&points, &(0..points.len()).collect::<Vec<_>>());
        let system = centroid(&points, &system.into_iter().collect::<Vec<_>>());
        assert!(system.x < whole.x && system.y < whole.y);

        let state = smiles::read("c1ccc2ncccc2c1")?.prepared.state;
        assert_eq!(state.rings.atoms.len(), 2);
        let nitrogen = state
            .graph
            .atoms
            .iter()
            .position(|a| a.atomic_number == 7)
            .unwrap();
        let mut points = vec![Point3::default(); state.graph.atoms.len()];
        points[nitrogen].y = 1.5;
        conventional_orientation(&state, &mut points);
        assert!(
            points[nitrogen].y < 0.,
            "Fused-ring heteroatoms prefer the bottom"
        );

        let state = smiles::read("C1CCC2(CC1)CCNC2")?.prepared.state;
        assert_eq!(state.rings.atoms.len(), 2);
        let nitrogen = state
            .graph
            .atoms
            .iter()
            .position(|a| a.atomic_number == 7)
            .unwrap();
        let mut points = vec![Point3::default(); state.graph.atoms.len()];
        points[nitrogen].y = 1.5;
        let before = points.clone();
        conventional_orientation(&state, &mut points);
        assert_eq!(points, before, "Spiro rings share no fused edge");
        Ok(())
    }

    #[test]
    fn conventional_double_branches_include_substituted_imines_but_exclude_ring_bonds()
    -> anyhow::Result<()> {
        let state = smiles::read("CC=NC")?.prepared.state;
        let mut points =
            [(0., 0.), (1.5, 0.), (1.5, -1.5), (3., -1.5)].map(|(x, y)| Point3 { x, y, z: 0. });
        conventional_orientation(&state, &mut points);
        assert!(
            points[2].y > points[1].y,
            "Nonterminal imine N must prefer up"
        );
        for text in ["C1=NCCC1", "C=[O+]C"] {
            let state = smiles::read(text)?.prepared.state;
            let mut points = vec![Point3::default(); state.graph.atoms.len()];
            points[1].y = -1.5;
            let before = points.clone();
            conventional_orientation(&state, &mut points);
            assert_eq!(points, before, "{text}: no exocyclic terminal branch");
        }
        Ok(())
    }

    #[test]
    fn component_row_uses_real_bond_lengths_and_stable_charge_size_order() -> anyhow::Result<()> {
        let state = smiles::read("Cl.CN.[Na+].[Cl-].O")?.prepared.state;
        let mut points = vec![Point3::default(); state.graph.atoms.len()];
        points[2].x = 2.4;
        points[2].y = 1.8;
        arrange_components(&state, &mut points);
        // CN has two heavy atoms. The one-heavy-atom ties are cation, neutral
        // Cl then O in input order, and anion; implicit H on HCl is not heavy.
        let order = [1, 3, 0, 5, 4];
        for pair in order.windows(2) {
            let right = if pair[0] == 1 {
                points[2].x
            } else {
                points[pair[0]].x
            };
            assert!((points[pair[1]].x - right - 3.).abs() < 1e-12);
        }
        let baseline = (points[1].y + points[2].y) * 0.5;
        for &id in &order[1..] {
            assert!((points[id].y - baseline).abs() < 1e-12);
        }
        Ok(())
    }

    #[test]
    fn label_spacing_preserves_an_unlabeled_row() -> anyhow::Result<()> {
        let (state, mut positions) = layout("CCC.CC")?;
        let row = arrange_components(&state, &mut positions);
        let mut molecule = document::Molecule {
            rdkit_version: crate::chemistry::RDKIT_VERSION,
            ids: (1..=positions.len() as u64).collect(),
            state,
            positions,
        };
        let drawing = document::for_generated(
            &molecule,
            &vec![None; molecule.ids.len()],
            Default::default(),
        )?;
        let labels = drawing.labels()?;
        let mut doc = drawing.finish(labels)?;
        let before = doc.clone();
        let original = molecule.positions.clone();
        space_component_labels(&row, &mut molecule, &mut doc);
        assert_eq!(doc, before);
        assert_eq!(molecule.positions, original);
        Ok(())
    }

    #[test]
    fn counterions_require_clear_bisectors_and_pair_each_charge_once() -> anyhow::Result<()> {
        for blocked in [false, true] {
            let state = smiles::read("[Na+].[Na+].[O-]CC")?.prepared.state;
            let mut points = vec![Point3::default(); 5];
            points[3].x = 1.5;
            points[4].x = if blocked { -1.5 } else { 3. };
            let length = bond_length(&state, &points);
            arrange_components(&state, &mut points);
            let distances =
                [0, 1].map(|id| (points[id].x - points[2].x).hypot(points[id].y - points[2].y));
            if blocked {
                assert!(distances.iter().all(|&d| d > length));
            } else {
                assert!((distances[0] - length).abs() < 1e-12);
                assert!(distances[1] > length);
            }
        }
        Ok(())
    }

    #[test]
    fn straightening_fits_noisy_bond_clusters_and_requires_a_strict_majority() -> anyhow::Result<()>
    {
        for (angles, should_rotate) in [
            (vec![5_f64, 7., 20.], true),
            (vec![5_f64, 5., 20., 20.], false),
        ] {
            let state = smiles::read(&"C".repeat(angles.len() + 1))?.prepared.state;
            let mut points = vec![Point3::default()];
            for angle in angles {
                let previous = *points.last().unwrap();
                let (sin, cos) = angle.to_radians().sin_cos();
                points.push(Point3 {
                    x: previous.x + cos,
                    y: previous.y + sin,
                    z: 0.,
                });
            }
            let original = points.clone();
            straighten(&state, &mut points, None);
            if should_rotate {
                assert_ne!(points, original);
                let angles: Vec<_> = points
                    .windows(2)
                    .map(|pair| {
                        (pair[1].y - pair[0].y)
                            .atan2(pair[1].x - pair[0].x)
                            .to_degrees()
                    })
                    .collect();
                assert!((angles[0] + 1.).abs() < 1e-7);
                assert!((angles[1] - 1.).abs() < 1e-7);
            } else {
                assert_eq!(points, original);
            }
        }
        Ok(())
    }

    #[test]
    fn straightening_ignores_non_six_ring_bonds() -> anyhow::Result<()> {
        for text in ["C1CC1", "C1CCC1", "C1CCCC1", "C1CCCCCC1"] {
            let (state, mut points) = layout(text)?;
            let original = points.clone();
            straighten(&state, &mut points, None);
            assert_eq!(points, original, "{text}");
        }
        Ok(())
    }

    #[test]
    fn exterior_repair_changes_only_the_leaf_and_respects_fixed_atoms() -> anyhow::Result<()> {
        let (state, old) = layout(ARTEMISININ)?;
        let mut repaired = old.clone();
        exterior_terminals(&state, &mut repaired, &Coordinates::new());
        assert_ne!(repaired[18], old[18]);
        for (id, (a, b)) in old.iter().zip(&repaired).enumerate() {
            if id != 18 {
                assert_eq!(a, b);
            }
        }
        let length =
            |points: &[Point3]| (points[18].x - points[14].x).hypot(points[18].y - points[14].y);
        assert!((length(&old) - length(&repaired)).abs() < 1e-12);
        let mut fixed = Coordinates::new();
        fixed.insert(
            18,
            depict::geometry::Point {
                x: old[18].x,
                y: old[18].y,
            },
        );
        let mut held = old.clone();
        exterior_terminals(&state, &mut held, &fixed);
        assert_eq!(held, old);
        exterior_terminals(&state, &mut repaired, &Coordinates::new());
        let twice = repaired.clone();
        exterior_terminals(&state, &mut repaired, &Coordinates::new());
        assert_eq!(repaired, twice);
        Ok(())
    }

    #[test]
    fn occupied_exterior_or_new_crossing_does_not_publish_a_move() -> anyhow::Result<()> {
        for mode in ["collision", "crossing", "touching", "collinear"] {
            let (state, mut points) = layout(&format!(
                "{ARTEMISININ}.{}",
                if mode == "collision" { "C" } else { "CC" }
            ))?;
            let mut target = points.clone();
            exterior_terminals(&state, &mut target, &Coordinates::new());
            assert_ne!(target[18], points[18]);
            if mode != "collision" {
                let origin = points[14];
                let end = target[18];
                let (dx, dy) = (end.x - origin.x, end.y - origin.y);
                let middle = Point3 {
                    x: (origin.x + end.x) * 0.5,
                    y: (origin.y + end.y) * 0.5,
                    z: 0.,
                };
                points[20] = Point3 {
                    x: middle.x - dy,
                    y: middle.y + dx,
                    z: 0.,
                };
                points[21] = Point3 {
                    x: middle.x + dy,
                    y: middle.y - dx,
                    z: 0.,
                };
                if mode == "touching" {
                    points[20] = Point3 {
                        x: origin.x + dx * 0.25,
                        y: origin.y + dy * 0.25,
                        z: 0.,
                    };
                    points[21] = Point3 {
                        x: points[20].x - dy,
                        y: points[20].y + dx,
                        z: 0.,
                    };
                } else if mode == "collinear" {
                    points[20] = Point3 {
                        x: origin.x - dx,
                        y: origin.y - dy,
                        z: 0.,
                    };
                    points[21] = Point3 {
                        x: origin.x + 2. * dx,
                        y: origin.y + 2. * dy,
                        z: 0.,
                    };
                }
                assert!(!adds_collision(
                    &points, 18, 14, points[18], target[18], 0.75
                ));
                assert!(crosses_other_bond(&state, &points, 18, 14, target[18]));
            } else {
                points[20] = target[18];
            }
            let original = points.clone();
            exterior_terminals(&state, &mut points, &Coordinates::new());
            assert_eq!(points, original, "Unsafe exterior: {mode}");
        }
        Ok(())
    }

    #[test]
    fn a_disconnected_fused_junction_is_not_reclassified_as_bridged() -> anyhow::Result<()> {
        let (state, mut points) = layout(&format!("{ARTEMISININ}.CC12CCCCC1CCCC2"))?;
        assert!(bridged(&state));
        let junction = 21;
        let rings = state
            .rings
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(id, ring)| ring.contains(&junction).then_some(id))
            .collect::<Vec<_>>();
        assert!(rings.len() >= 2);
        assert!(!bridged_junction(&state, &rings, &mut 1_000_000));
        let origin = Point3 {
            x: 30.,
            y: 30.,
            z: 0.,
        };
        points[junction] = origin;
        let neighbors = state
            .graph
            .bonds
            .iter()
            .filter_map(|bond| {
                if bond.a == junction {
                    Some(bond.b)
                } else if bond.b == junction {
                    Some(bond.a)
                } else {
                    None
                }
            })
            .filter(|&id| id != 20)
            .collect::<Vec<_>>();
        assert_eq!(neighbors.len(), 3);
        for (index, neighbor) in neighbors.iter().enumerate() {
            let angle = index as f64 * PI / 4.;
            points[*neighbor] = Point3 {
                x: origin.x + 1.5 * angle.cos(),
                y: origin.y + 1.5 * angle.sin(),
                z: 0.,
            };
        }
        points[20] = Point3 {
            x: 31.,
            y: 31.,
            z: 0.,
        };
        let old = points[20];
        exterior_terminals(&state, &mut points, &Coordinates::new());
        assert_eq!(points[20], old);
        Ok(())
    }
}
