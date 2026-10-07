//! Conservative admission of closed sp2 carbon cages and their geometry.
//! This near-convex envelope is a supported calculation domain, not a proof
//! that every fullerene geometry outside it is chemically invalid.
use cosmolkit_core::{Hybridization, Molecule, symmetrize_sssr};

pub(super) struct Cage {
    faces: Vec<Vec<usize>>,
}

impl Cage {
    pub(super) fn from_molecule(mol: &Molecule) -> Option<Self> {
        if mol.num_atoms() < 20
            || mol.atoms().iter().any(|atom| {
                atom.atomic_number() != 6
                    || atom.formal_charge() != 0
                    || atom.hybridization() != Hybridization::Sp2
            })
        {
            return None;
        }
        let mut degree = vec![0; mol.num_atoms()];
        for bond in mol.bonds() {
            degree[bond.begin().index()] += 1;
            degree[bond.end().index()] += 1;
        }
        if degree.iter().any(|&count| count != 3) {
            return None;
        }
        let rings = symmetrize_sssr(mol).ok()?;
        if rings.atom_rings().len() != mol.num_bonds() - mol.num_atoms() + 2
            || rings
                .atom_rings()
                .iter()
                .any(|ring| !matches!(ring.len(), 5 | 6))
            || rings
                .atom_rings()
                .iter()
                .filter(|ring| ring.len() == 5)
                .count()
                != 12
        {
            return None;
        }
        let mut incidence = vec![0; mol.num_bonds()];
        for ring in rings.bond_rings() {
            for bond in ring {
                incidence[bond.index()] += 1;
            }
        }
        if incidence.iter().any(|&count| count != 2) {
            return None;
        }
        Some(Self {
            faces: rings
                .atom_rings()
                .iter()
                .map(|ring| ring.iter().map(|atom| atom.index()).collect())
                .collect(),
        })
    }

    pub(super) fn validate(&self, mol: &Molecule, coordinates: &[[f64; 3]]) -> Result<(), String> {
        if coordinates.len() != mol.num_atoms()
            || coordinates.iter().flatten().any(|value| !value.is_finite())
        {
            return Err(unsupported("invalid calculation coordinates"));
        }
        let center = centroid(coordinates.iter().copied(), coordinates.len());
        let mut lengths: Vec<_> = mol
            .bonds()
            .iter()
            .map(|bond| {
                norm(sub(
                    coordinates[bond.begin().index()],
                    coordinates[bond.end().index()],
                ))
            })
            .collect();
        lengths.sort_by(f64::total_cmp);
        let middle = lengths.len() / 2;
        let length = (lengths[(lengths.len() - 1) / 2] + lengths[middle]) / 2.;
        if length <= 0. {
            return Err(unsupported("collapsed cage bonds"));
        }
        for face in &self.faces {
            let face_center = centroid(face.iter().map(|&index| coordinates[index]), face.len());
            let mut newell = [0.; 3];
            for (i, &index) in face.iter().enumerate() {
                let next = face[(i + 1) % face.len()];
                let contribution = cross(
                    sub(coordinates[index], face_center),
                    sub(coordinates[next], face_center),
                );
                for axis in 0..3 {
                    newell[axis] += contribution[axis];
                }
            }
            let magnitude = norm(newell);
            if magnitude * 0.5 <= 0.25 * length * length {
                return Err(unsupported("a cage face has insufficient area"));
            }
            // The winding normal handles either order returned by ring
            // perception; use its outward orientation for supporting planes.
            let winding_normal = newell.map(|value| value / magnitude);
            let projection = dot(sub(face_center, center), winding_normal);
            if projection.abs() <= 0.25 * length {
                return Err(unsupported("a cage face is folded toward the cage center"));
            }
            let outward = winding_normal.map(|value| value * projection.signum());
            for &index in face {
                if dot(sub(coordinates[index], face_center), outward).abs() > 0.20 * length {
                    return Err(unsupported(
                        "a cage face exceeds supported planarity tolerance",
                    ));
                }
            }
            for (index, &point) in coordinates.iter().enumerate() {
                if !face.contains(&index) && dot(sub(point, face_center), outward) > 0.20 * length {
                    return Err(unsupported(
                        "a cage face is not a near-convex supporting face",
                    ));
                }
            }
            if !projected_face_is_convex(face, coordinates, winding_normal, length) {
                return Err(unsupported("a projected cage face is folded or nonconvex"));
            }
        }
        for (i, bond) in mol.bonds().iter().enumerate() {
            let (a, b) = (bond.begin().index(), bond.end().index());
            for other in &mol.bonds()[i + 1..] {
                let (c, d) = (other.begin().index(), other.end().index());
                if a != c
                    && a != d
                    && b != c
                    && b != d
                    && segment_distance_squared(
                        coordinates[a],
                        coordinates[b],
                        coordinates[c],
                        coordinates[d],
                    ) < 0.5 * 0.5
                {
                    return Err(unsupported(
                        "nonincident cage bonds approach within 0.5 Angstrom",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn unsupported(reason: &str) -> String {
    format!("cage geometry is outside supported near-convex envelope: {reason}")
}

fn centroid(points: impl Iterator<Item = [f64; 3]>, count: usize) -> [f64; 3] {
    let mut center = [0.; 3];
    for point in points {
        for axis in 0..3 {
            center[axis] += point[axis] / count as f64;
        }
    }
    center
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(vector: [f64; 3]) -> f64 {
    dot(vector, vector).sqrt()
}

fn projected_face_is_convex(
    face: &[usize],
    coordinates: &[[f64; 3]],
    winding_normal: [f64; 3],
    length: f64,
) -> bool {
    // Every other vertex must lie strictly on the interior side of every
    // directed edge. Consecutive positive turns alone also admit pentagrams.
    for (i, &start) in face.iter().enumerate() {
        let end = face[(i + 1) % face.len()];
        let edge = sub(coordinates[end], coordinates[start]);
        for &other in face {
            if other != start
                && other != end
                && dot(
                    cross(edge, sub(coordinates[other], coordinates[start])),
                    winding_normal,
                ) <= 1e-6 * length * length
            {
                return false;
            }
        }
    }
    true
}

fn segment_distance_squared(p: [f64; 3], q: [f64; 3], r: [f64; 3], s: [f64; 3]) -> f64 {
    let u = sub(q, p);
    let v = sub(s, r);
    let w = sub(p, r);
    let (a, b, c, d, e) = (dot(u, u), dot(u, v), dot(v, v), dot(u, w), dot(v, w));
    let denominator = a * c - b * b;
    let mut along_u = if denominator > 1e-12 * a * c {
        ((b * e - c * d) / denominator).clamp(0., 1.)
    } else {
        0.
    };
    let mut along_v = (b * along_u + e) / c;
    if along_v < 0. {
        along_v = 0.;
        along_u = (-d / a).clamp(0., 1.);
    } else if along_v > 1. {
        along_v = 1.;
        along_u = ((b - d) / a).clamp(0., 1.);
    }
    let separation = std::array::from_fn(|axis| w[axis] + along_u * u[axis] - along_v * v[axis]);
    dot(separation, separation)
}

#[cfg(test)]
mod tests;
