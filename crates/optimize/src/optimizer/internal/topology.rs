//! Covalent topology and the primitive part of a molecular coordinate chart.
//!
//! Bond assignment and the 175-degree linear-bend criterion follow Sec. II A 1
//! of V. Bakken and T. Helgaker, J. Chem. Phys. 117, 9160–9174 (2002),
//! <https://doi.org/10.1063/1.1515483> \[Bakken2002Coordinates\]. Covalent radii for Z = 1–118 are from
//! P. Pyykkö and M. Atsumi, Chem. Eur. J. 15, 186–197 (2009), Fig. 2,
//! <https://doi.org/10.1002/chem.200800987> \[Pyykko2009Radii\]. Values were also checked against
//! Pyykkö's author table of the year-2009 single-, double- and triple-bond
//! radii (the single-bond column of Fig. 3 of the companion paper,
//! <https://doi.org/10.1002/chem.200901472> \[Pyykko2009Double\]).
//!
//! The ordinary stretch, bend, and torsion force constants follow Lindh et al.,
//! Chem. Phys. Lett. 241, 423–428 (1995),
//! <https://doi.org/10.1016/0009-2614(95)00646-L> \[Lindh1995Hessian\], using the parameters in
//! Bakken–Helgaker Table I and Eqs. (40)–(41). Periods beyond three use the
//! existing model Hessian's explicit period-three extrapolation.
//!
//! Torsions can span a run of linear degree-two atoms. Planar neighbor triples
//! at centers of any coordination receive symmetric out-of-plane coordinates,
//! and optional hydrogen bonds add weak distances. Each extension's stiffness
//! is documented below.

use std::collections::BTreeMap;

use crate::ANGSTROM_PER_BOHR;
use crate::model_hessian::{
    ALPHA, BEND, BOND_FACTOR, R_REF, STRETCH, TORSION, covalent_radius, period,
};

use super::geometry::{Vector, cross, dot, length, subtract};
use super::primitive::{Local, bend_axes};

mod chart_check;
use chart_check::ChartCheck;

const COS_FIVE_DEGREES: f64 = 0.996_194_698_091_745_5;
const SIN_FIVE_DEGREES: f64 = 0.087_155_742_747_658_17;
const MIN_DISTANCE: f64 = 1e-8;
// A compact covalent graph can produce O(n⁴) torsions. Limit local primitives
// during enumeration so ANC takes over before allocating dense Wilson rows
// and DLC weights. The chart adds at most six rigid coordinates per fragment.
// Dense inorganic clusters such as [Mo6Cl14]2- need about 84 per atom.
const MAX_LOCAL_PRIMITIVES_PER_ATOM: usize = 128;
// Weak initial curvature for an auxiliary H···acceptor distance (Eh/bohr²).
const HBOND_STIFFNESS: f64 = 1e-3;

#[derive(Clone, Copy)]
struct Neighbor {
    atom: usize,
    rho: f64,
}

pub(super) struct Topology {
    /// Connected components of the covalent graph.
    pub(super) fragments: Vec<Vec<usize>>,
    /// Local coordinates and diagonal force constants in atomic units.
    pub(super) locals: Vec<(Local, f64)>,
    covalent: Vec<(usize, usize)>,
    hydrogen_bonds: Vec<(usize, usize)>,
    // One oriented line per pair of linear bends, in generation order.
    linear_lines: Vec<Vector>,
    chart_check: Option<ChartCheck>,
}

impl Topology {
    /// Generate a deterministic local chart from Cartesian positions in bohr.
    /// Covalent bonds use `r <= 1.3 (r_cov(A) + r_cov(B))`.
    pub(super) fn generate(numbers: &[u8], x: &[f64], hbonds: bool) -> Result<Self, String> {
        profile_phase!("topology");
        if numbers.is_empty() {
            return Err("internal topology needs at least one atom".into());
        }
        if numbers.len().checked_mul(3) != Some(x.len()) {
            return Err("internal topology needs three coordinates per atom".into());
        }
        if numbers.iter().any(|z| !(1..=118).contains(z)) {
            return Err("internal topology atomic numbers must be in 1..=118".into());
        }
        if x.iter().any(|v| !v.is_finite()) {
            return Err("internal topology coordinates must be finite".into());
        }

        let mut graph = vec![Vec::new(); numbers.len()];
        let mut covalent = Vec::new();
        let mut locals = Vec::new();
        let primitive_limit = numbers.len().saturating_mul(MAX_LOCAL_PRIMITIVES_PER_ATOM);
        let mut linear_lines = Vec::new();
        for a in 0..numbers.len() {
            for b in a + 1..numbers.len() {
                let r = length(subtract(point(x, a), point(x, b)));
                if !r.is_finite() {
                    return Err("internal topology interatomic distance overflow".into());
                }
                if r < MIN_DISTANCE {
                    return Err(format!("internal topology atoms {a} and {b} coincide"));
                }
                if r <= BOND_FACTOR * (covalent_radius(numbers[a]) + covalent_radius(numbers[b])) {
                    let rho = lindh_rho(numbers[a], numbers[b], r);
                    covalent.push((a, b));
                    graph[a].push(Neighbor { atom: b, rho });
                    graph[b].push(Neighbor { atom: a, rho });
                    locals.push((Local::Stretch(a, b), STRETCH * rho));
                    check_primitive_limit(locals.len(), primitive_limit)?;
                }
            }
        }
        // Pair enumeration above leaves every adjacency list in atom order.
        let fragments = components(&graph);
        for (b, adjacent) in graph.iter().enumerate() {
            for (index, left) in adjacent.iter().enumerate() {
                for right in &adjacent[index + 1..] {
                    let (a, c) = (left.atom, right.atom);
                    let force = BEND * left.rho * right.rho;
                    if linear(x, a, b, c) {
                        let axes = bend_axes(x, a, b, c)
                            .ok_or("cannot construct internal linear-bend axes")?;
                        linear_lines.push(cross(axes[0], axes[1]));
                        for axis in axes {
                            locals.push((Local::LinearBend(a, b, c, axis), force));
                        }
                    } else {
                        // A zero-degree bend is singular, whereas a 180-degree
                        // bend has the two regular coordinates just assigned.
                        let u = direction(x, b, a);
                        let v = direction(x, b, c);
                        if length(cross(u, v)) > 1e-10 {
                            locals.push((Local::Bend(a, b, c), force));
                        }
                    }
                    check_primitive_limit(locals.len(), primitive_limit)?;
                }
            }
        }
        add_torsions(&mut locals, &graph, x, primitive_limit)?;
        add_impropers(&mut locals, &graph, x, primitive_limit)?;

        let mut hydrogen_bonds = Vec::new();
        if hbonds {
            for (h, &number) in numbers.iter().enumerate() {
                if number != 1 {
                    continue;
                }
                for (a, &acceptor) in numbers.iter().enumerate() {
                    let Some(vdw) = hbond_acceptor_radius(acceptor) else {
                        continue;
                    };
                    if a == h || graph[h].iter().any(|edge| edge.atom == a) {
                        continue;
                    }
                    let r = length(subtract(point(x, h), point(x, a)));
                    if r <= covalent_radius(1) + covalent_radius(acceptor)
                        || r >= 0.9 * (1.10 + vdw) / ANGSTROM_PER_BOHR
                    {
                        continue;
                    }
                    // A covalent X–H donor points toward the acceptor when
                    // X–H···A exceeds 90° (Bakken–Helgaker, Sec. II A 1).
                    if graph[h].iter().any(|donor| {
                        hbond_acceptor_radius(numbers[donor.atom]).is_some()
                            && dot(direction(x, h, donor.atom), direction(x, h, a)) < 0.0
                    }) {
                        hydrogen_bonds.push((h, a));
                        locals.push((Local::Stretch(h.min(a), h.max(a)), HBOND_STIFFNESS));
                        check_primitive_limit(locals.len(), primitive_limit)?;
                    }
                }
            }
        }
        if locals.iter().any(|(_, k)| !k.is_finite() || *k <= 0.0) {
            return Err("internal topology produced an invalid force constant".into());
        }
        Ok(Self {
            fragments,
            locals,
            covalent,
            hydrogen_bonds,
            linear_lines,
            chart_check: ChartCheck::new(numbers, x, hbonds, &graph),
        })
    }

    /// Check ordinary organic charts without regenerating their local paths.
    /// Unusual angles, coordination and optional hydrogen bonds use the full
    /// generator, retaining the same chart-selection rules in every case.
    pub(super) fn same_chart_at(&self, numbers: &[u8], x: &[f64], hbonds: bool) -> bool {
        if let Some(result) = self
            .chart_check
            .as_ref()
            .and_then(|check| check.same_chart_at(numbers, x, hbonds, &self.covalent))
        {
            return result;
        }
        Self::generate(numbers, x, hbonds).is_ok_and(|new| self.same_chart(&new))
    }

    /// Whether the same coordinates are present and their retained linear-bend
    /// chart is regular. Ignore force constants and transverse-axis reselection,
    /// but recenter once a line has rotated by more than 60 degrees. A retained
    /// transverse chart would lose a bending direction at a 90-degree rotation.
    pub(super) fn same_chart(&self, other: &Self) -> bool {
        self.covalent == other.covalent
            && self.hydrogen_bonds == other.hydrogen_bonds
            && self.fragments == other.fragments
            && self.linear_lines.len() == other.linear_lines.len()
            && self
                .linear_lines
                .iter()
                .zip(&other.linear_lines)
                .all(|(&a, &b)| dot(a, b) >= 0.5)
            && self.locals.len() == other.locals.len()
            && self
                .locals
                .iter()
                .zip(&other.locals)
                .all(|((a, _), (b, _))| local_key(a) == local_key(b))
    }
}

fn check_primitive_limit(count: usize, limit: usize) -> Result<(), String> {
    if count > limit {
        return Err(format!(
            "internal topology exceeds {MAX_LOCAL_PRIMITIVES_PER_ATOM} local primitives per atom"
        ));
    }
    Ok(())
}

fn lindh_rho(a: u8, b: u8, distance: f64) -> f64 {
    let (a, b) = (period(a), period(b));
    (ALPHA[a][b] * (R_REF[a][b].powi(2) - distance.powi(2))).exp()
}

fn components(graph: &[Vec<Neighbor>]) -> Vec<Vec<usize>> {
    let mut seen = vec![false; graph.len()];
    let mut fragments = Vec::new();
    for atom in 0..graph.len() {
        if seen[atom] {
            continue;
        }
        seen[atom] = true;
        let mut fragment = vec![atom];
        let mut cursor = 0;
        while cursor < fragment.len() {
            for neighbor in &graph[fragment[cursor]] {
                if !seen[neighbor.atom] {
                    seen[neighbor.atom] = true;
                    fragment.push(neighbor.atom);
                }
            }
            cursor += 1;
        }
        fragment.sort_unstable();
        fragments.push(fragment);
    }
    fragments
}

fn add_torsions(
    locals: &mut Vec<(Local, f64)>,
    graph: &[Vec<Neighbor>],
    x: &[f64],
    limit: usize,
) -> Result<(), String> {
    let mut torsions: BTreeMap<[usize; 4], f64> = BTreeMap::new();
    for b in 0..graph.len() {
        for first in &graph[b] {
            let mut path = vec![b, first.atom];
            let mut log_rho = first.rho.ln();
            loop {
                let c = path[path.len() - 1];
                // A path and its reversal define the same central axis.
                if b < c {
                    let axis = direction(x, b, c);
                    // Use the geometric mean of central-bond rho values for
                    // an extended axis. This is exactly Lindh for one bond,
                    // without an exponential dependence on linear-run length.
                    let central_rho = (log_rho / (path.len() - 1) as f64).exp();
                    for left in graph[b].iter().filter(|e| !path.contains(&e.atom)) {
                        if dot(direction(x, b, left.atom), axis).abs() >= COS_FIVE_DEGREES {
                            continue;
                        }
                        for right in graph[c].iter().filter(|e| !path.contains(&e.atom)) {
                            if left.atom == right.atom
                                || dot(direction(x, c, right.atom), axis).abs() >= COS_FIVE_DEGREES
                            {
                                continue;
                            }
                            let key = [left.atom, b, c, right.atom];
                            let force = TORSION * left.rho * central_rho * right.rho;
                            torsions
                                .entry(key)
                                .and_modify(|value| *value = value.max(force))
                                .or_insert(force);
                            check_primitive_limit(locals.len() + torsions.len(), limit)?;
                        }
                    }
                }
                // Skip only straight degree-two centers. Both terminal bond
                // planes stay well-defined, e.g. H–C=(C=)...C–H. This geometric
                // extension supplements the ordinary well-defined torsions of
                // Bakken–Helgaker.
                if graph[c].len() != 2 {
                    break;
                }
                let previous = path[path.len() - 2];
                let next = graph[c].iter().find(|edge| edge.atom != previous).unwrap();
                if path.contains(&next.atom) || !linear(x, previous, c, next.atom) {
                    break;
                }
                log_rho += next.rho.ln();
                path.push(next.atom);
            }
        }
    }
    locals.extend(
        torsions
            .into_iter()
            .map(|([a, b, c, d], k)| (Local::Torsion(a, b, c, d), k)),
    );
    Ok(())
}

fn add_impropers(
    locals: &mut Vec<(Local, f64)>,
    graph: &[Vec<Neighbor>],
    x: &[f64],
    limit: usize,
) -> Result<(), String> {
    for (b, adjacent) in graph.iter().enumerate() {
        // At a planar center, stretches and ordinary bends have no first-order
        // response normal to the plane. Using every neighbor triple spans the
        // missing modes and treats equivalent ligands symmetrically.
        for (i, a) in adjacent.iter().enumerate() {
            for (j, c) in adjacent.iter().enumerate().skip(i + 1) {
                for d in &adjacent[j + 1..] {
                    add_improper_triple(locals, b, [*a, *c, *d], x);
                    check_primitive_limit(locals.len(), limit)?;
                }
            }
        }
    }
    Ok(())
}

fn add_improper_triple(
    locals: &mut Vec<(Local, f64)>,
    b: usize,
    [a, c, d]: [Neighbor; 3],
    x: &[f64],
) {
    let atoms = [a.atom, c.atom, d.atom];
    let vectors = atoms.map(|atom| direction(x, b, atom));
    let mask = improper_mask(vectors);
    // Lindh has no separate improper parameter. Use its small torsion
    // coefficient with this triple's three bonds to the center, and include
    // each peripheral atom to preserve symmetry in the redundant coordinates.
    let force = TORSION * a.rho * c.rho * d.rho;
    for index in 0..3 {
        if mask & (1 << index) != 0 {
            let a = atoms[index];
            let c = atoms[(index + 1) % 3];
            let d = atoms[(index + 2) % 3];
            // A lies out of the B–C–D plane, with B always second.
            locals.push((Local::OutOfPlane(a, b, c.min(d), c.max(d)), force));
        }
    }
}

/// Peripheral atoms whose out-of-plane coordinates the generator retains.
/// Shared with the bounded chart check so planarity has one definition.
fn improper_mask(vectors: [Vector; 3]) -> u8 {
    let normals = [
        cross(vectors[1], vectors[2]),
        cross(vectors[2], vectors[0]),
        cross(vectors[0], vectors[1]),
    ];
    let lengths = normals.map(length);
    let maximum = lengths.iter().copied().fold(0.0, f64::max);
    // The normalized scalar triple product measures departure from planarity
    // using the best-conditioned bond pair.
    if maximum < 1e-10 || dot(vectors[0], normals[0]).abs() > SIN_FIVE_DEGREES * maximum {
        return 0;
    }
    lengths
        .iter()
        .enumerate()
        .fold(0, |mask, (index, &length)| {
            mask | (u8::from(length > SIN_FIVE_DEGREES) << index)
        })
}

// Mantina et al., J. Phys. Chem. A 113, 5806–5812 (2009),
// doi:10.1021/jp8111556 [Mantina2009Radii], Table 6 (Bondi values, in Å). The H radius is
// 1.10 Å, the Rowland–Taylor value recommended in the same paper.
fn hbond_acceptor_radius(number: u8) -> Option<f64> {
    match number {
        7 => Some(1.55),
        8 => Some(1.52),
        9 => Some(1.47),
        16 => Some(1.80),
        17 => Some(1.75),
        _ => None,
    }
}

fn local_key(local: &Local) -> [usize; 5] {
    match *local {
        Local::Stretch(a, b) => [0, a, b, 0, 0],
        Local::Bend(a, b, c) => [1, a, b, c, 0],
        Local::LinearBend(a, b, c, _) => [2, a, b, c, 0],
        Local::Torsion(a, b, c, d) => [3, a, b, c, d],
        Local::OutOfPlane(a, b, c, d) => [4, a, b, c, d],
    }
}

fn point(x: &[f64], atom: usize) -> Vector {
    [x[3 * atom], x[3 * atom + 1], x[3 * atom + 2]]
}

fn direction(x: &[f64], from: usize, to: usize) -> Vector {
    let vector = subtract(point(x, to), point(x, from));
    let norm = length(vector);
    vector.map(|v| v / norm)
}

fn linear(x: &[f64], a: usize, b: usize, c: usize) -> bool {
    dot(direction(x, b, a), direction(x, b, c)) < -COS_FIVE_DEGREES
}
