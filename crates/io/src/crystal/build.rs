//! From atoms and a lattice to a drawing: bonds from covalent radii across
//! periodic images, the boundary rule and the 3D model with its cell.
use super::{Atom, Error, Structure, V3, add, dot, elements, length, scale, sub};
use crate::{
    document::{Document, Point},
    models3d::{self, Cell, Crossing, Display, ForceField, Model3d, Placed, Site},
};
use std::collections::{HashMap, HashSet, VecDeque};

/// Bonded when closer than the two covalent radii plus this, in ångström.
const TOLERANCE: f64 = 0.45;
/// Closer atoms are the same site, or overlapping alternatives.
const SAME_SITE: f64 = 0.25;
/// Fractional coordinates within this of 1 wrap to 0, so atoms on a cell
/// face are kept once, on the origin's side.
const WRAP: f64 = 1e-4;
/// Drawing units per ångström, as MOL import scales 3D coordinates.
const UNITS: f32 = 28.;
/// The farthest cell a bond may reach, as a model stores bonds that leave it.
const MAX_REACH: i32 = 3;

/// A lattice: its vectors and the reciprocal vectors that give fractional
/// coordinates.
#[derive(Clone, Copy)]
pub(super) struct Lattice {
    pub vectors: [V3; 3],
    reciprocal: [V3; 3],
}

impl Lattice {
    pub(super) fn new(vectors: [V3; 3]) -> Result<Self, Error> {
        let [a, b, c] = vectors;
        let volume = dot(a, super::cross(b, c));
        if !vectors.iter().flatten().all(|v| v.is_finite()) || !volume.is_finite() {
            return Err(Error::new("The lattice vectors are not numbers"));
        }
        if volume.abs() < 1e-3 {
            return Err(Error::new("The unit cell has no volume"));
        }
        if volume < 0. {
            return Err(Error::new("The lattice vectors are left-handed"));
        }
        if vectors.iter().any(|v| length(*v) > 1_000.) {
            return Err(Error::new("The unit cell is longer than 1000 Å"));
        }
        let reciprocal = [
            scale(super::cross(b, c), 1. / volume),
            scale(super::cross(c, a), 1. / volume),
            scale(super::cross(a, b), 1. / volume),
        ];
        Ok(Self {
            vectors,
            reciprocal,
        })
    }

    /// The lattice of cell lengths (Å) and angles (degrees), a along x and b
    /// in the xy plane, as CIF and PDB define it.
    pub(super) fn from_parameters(lengths: V3, angles: V3) -> Result<Self, Error> {
        if !lengths.iter().all(|l| l.is_finite() && *l > 0.) {
            return Err(Error::new("The cell lengths must be positive"));
        }
        if !angles.iter().all(|a| a.is_finite() && *a > 0. && *a < 180.) {
            return Err(Error::new("The cell angles must be between 0° and 180°"));
        }
        let [a, b, c] = lengths;
        let [alpha, beta, gamma] = angles.map(f64::to_radians);
        let (cos_a, cos_b) = (alpha.cos(), beta.cos());
        let (sin_g, cos_g) = gamma.sin_cos();
        let cy = (cos_a - cos_b * cos_g) / sin_g;
        let cz2 = 1. - cos_b * cos_b - cy * cy;
        if cz2 <= 1e-9 {
            return Err(Error::new("These cell angles make no unit cell"));
        }
        Self::new([
            [a, 0., 0.],
            [b * cos_g, b * sin_g, 0.],
            [c * cos_b, c * cy, c * cz2.sqrt()],
        ])
    }

    pub(super) fn fractional(&self, p: V3) -> V3 {
        self.reciprocal.map(|r| dot(p, r))
    }

    pub(super) fn cartesian(&self, f: V3) -> V3 {
        let [a, b, c] = self.vectors;
        add(add(scale(a, f[0]), scale(b, f[1])), scale(c, f[2]))
    }

    /// The lengths of the reciprocal vectors a*, b* and c*.
    pub(super) fn reciprocal_lengths(&self) -> V3 {
        self.reciprocal.map(length)
    }

    /// Whether some lattice copy of `q` lies within `radius` of `p`. Every
    /// copy that close is tried, however skewed the cell: a vector's
    /// fractional coordinate k is at most its length times |k*|.
    fn within(&self, p: V3, q: V3, radius: f64) -> bool {
        let d = self.fractional(sub(q, p));
        let r = self.reciprocal_lengths().map(|k| radius * k);
        let range = |k: usize| {
            let (d, r) = (
                d.get(k).copied().unwrap_or(0.),
                r.get(k).copied().unwrap_or(0.),
            );
            ((-r - d).ceil() as i64)..=((r - d).floor() as i64)
        };
        range(0).any(|i| {
            range(1).any(|j| {
                range(2).any(|k| {
                    let f = [d[0] + i as f64, d[1] + j as f64, d[2] + k as f64];
                    length(self.cartesian(f)) < radius
                })
            })
        })
    }

    /// The cells along each axis a sphere of `radius` reaches into.
    fn reach(&self, radius: f64) -> [i32; 3] {
        // The spacing of the lattice planes across axis k is 1 / |k*|.
        self.reciprocal_lengths()
            .map(|r| (radius * r).ceil().clamp(1., 1e3) as i32)
    }
}

/// Wrap a fractional coordinate into [0, 1), atoms on the far face to 0.
pub(super) fn wrap(f: f64) -> f64 {
    f - (f + WRAP).floor()
}

/// Whether `p` and some lattice copy of `q` are one site.
pub(super) fn same_site(lattice: &Lattice, p: V3, q: V3) -> bool {
    lattice.within(p, q, SAME_SITE)
}

/// Remove atoms within [`SAME_SITE`] of an earlier one, across the cell
/// boundary when there is a lattice; returns how many went.
pub(super) fn deduplicate(atoms: &mut Vec<Atom>, lattice: Option<&Lattice>) -> usize {
    let before = atoms.len();
    let mut kept: Vec<Atom> = Vec::with_capacity(atoms.len());
    for atom in atoms.drain(..) {
        let same = kept.iter().any(|k| {
            lattice.map_or_else(
                || length(sub(k.xyz, atom.xyz)) < SAME_SITE,
                |l| l.within(k.xyz, atom.xyz, SAME_SITE),
            )
        });
        if !same {
            kept.push(atom);
        }
    }
    *atoms = kept;
    before - atoms.len()
}

/// A bond from atom `a` to the copy of atom `b` `image` cells away.
#[derive(Clone, Copy, Debug)]
struct Link {
    a: usize,
    b: usize,
    image: [i32; 3],
}

/// Every bond by covalent radii, each once; across periodic images with a
/// lattice.
fn links(atoms: &[Atom], lattice: Option<&Lattice>) -> Result<Vec<Link>, Error> {
    let radii: Vec<f64> = atoms
        .iter()
        .map(|a| elements::covalent_radius(&a.element))
        .collect();
    let largest = radii.iter().copied().fold(0., f64::max);
    let reach = lattice.map_or([0; 3], |l| l.reach(2. * largest + TOLERANCE));
    // Bonds to copies more than three cells away would need a reduced cell.
    if reach.iter().any(|r| *r > MAX_REACH) {
        return Err(Error::new(
            "The unit cell is too thin or skewed to find its bonds; use a reduced (Niggli) cell",
        ));
    }
    let mut images = vec![];
    for i in -reach[0]..=reach[0] {
        for j in -reach[1]..=reach[1] {
            for k in -reach[2]..=reach[2] {
                images.push([i, j, k]);
            }
        }
    }
    let shift = |image: [i32; 3]| lattice.map_or([0.; 3], |l| l.cartesian(image.map(f64::from)));
    let shifts: Vec<_> = images.iter().map(|n| (*n, shift(*n))).collect();
    // In oxides, halides and other salts two metals are not bonded, however
    // close, as rutile's Ti–Ti at 2.96 Å; in metals and alloys they are.
    let salt = atoms.iter().any(|a| elements::anion(&a.element));
    let metal: Vec<bool> = atoms.iter().map(|a| elements::metal(&a.element)).collect();
    let mut links = vec![];
    for (a, (pa, ra)) in atoms.iter().zip(&radii).enumerate() {
        for (b, (pb, rb)) in atoms.iter().zip(&radii).enumerate().skip(a) {
            if salt && metal.get(a) == Some(&true) && metal.get(b) == Some(&true) {
                continue;
            }
            let cutoff = ra + rb + TOLERANCE;
            for (image, t) in &shifts {
                // An atom's bond to its own copy counts once, toward +image.
                if a == b && image.iter().find(|v| **v != 0).is_none_or(|v| *v < 0) {
                    continue;
                }
                let d = length(sub(add(pb.xyz, *t), pa.xyz));
                if d < cutoff && d > 0.4 {
                    links.push(Link {
                        a,
                        b,
                        image: *image,
                    });
                }
            }
        }
    }
    Ok(links)
}

/// Atom positions after the boundary rule, the bonds inside the drawing and
/// the bonds that leave the cell.
struct Arranged {
    xyz: Vec<V3>,
    bonds: Vec<(usize, usize)>,
    crossing: Vec<Link>,
}

/// The boundary rule, molecule by molecule. A molecule (a finite connected
/// group) is unwrapped so it is whole and moved by whole cells so its centre
/// lies in the cell. An extended network, whose bonds connect an atom to its
/// own copy in another cell, keeps each atom once inside the cell; its bonds
/// that leave the cell are kept apart.
fn arrange(atoms: &[Atom], links: &[Link], lattice: Option<&Lattice>) -> Arranged {
    let n = atoms.len();
    let mut neighbours: Vec<Vec<(usize, [i32; 3])>> = vec![vec![]; n];
    for l in links {
        if let Some(list) = neighbours.get_mut(l.a) {
            list.push((l.b, l.image));
        }
        if let Some(list) = neighbours.get_mut(l.b) {
            list.push((l.a, l.image.map(|v| -v)));
        }
    }
    let mut offset: Vec<Option<[i32; 3]>> = vec![None; n];
    let mut xyz: Vec<V3> = atoms.iter().map(|a| a.xyz).collect();
    let mut network = vec![false; n];
    for start in 0..n {
        if offset.get(start).is_some_and(Option::is_some) {
            continue;
        }
        let mut members = vec![start];
        let mut periodic = false;
        if let Some(o) = offset.get_mut(start) {
            *o = Some([0; 3]);
        }
        let mut queue = VecDeque::from([start]);
        while let Some(i) = queue.pop_front() {
            let Some(ti) = offset.get(i).copied().flatten() else {
                continue;
            };
            for &(j, image) in neighbours.get(i).into_iter().flatten() {
                let want = [ti[0] + image[0], ti[1] + image[1], ti[2] + image[2]];
                match offset.get_mut(j) {
                    Some(slot @ None) => {
                        *slot = Some(want);
                        members.push(j);
                        queue.push_back(j);
                    }
                    Some(Some(tj)) => periodic |= *tj != want,
                    None => {}
                }
            }
        }
        let Some(lattice) = lattice else { continue };
        if periodic {
            for &m in &members {
                if let Some(flag) = network.get_mut(m) {
                    *flag = true;
                }
            }
            continue;
        }
        // Unwrap the molecule, then move it whole so its centre is inside.
        for &m in &members {
            if let (Some(p), Some(Some(t))) = (xyz.get_mut(m), offset.get(m)) {
                *p = add(*p, lattice.cartesian(t.map(f64::from)));
            }
        }
        let count = members.len() as f64;
        let center = members
            .iter()
            .filter_map(|m| xyz.get(*m))
            .fold([0.; 3], |c, p| add(c, scale(*p, 1. / count)));
        let f = lattice.fractional(center);
        let back = lattice.cartesian(f.map(|v| -(v + WRAP).floor()));
        for &m in &members {
            if let Some(p) = xyz.get_mut(m) {
                *p = add(*p, back);
            }
        }
    }
    let mut bonds = vec![];
    let mut crossing = vec![];
    for l in links {
        let inside = network.get(l.a) == Some(&true) || network.get(l.b) == Some(&true);
        if inside && l.image != [0; 3] {
            crossing.push(*l);
        } else {
            bonds.push((l.a, l.b));
        }
    }
    Arranged {
        xyz,
        bonds,
        crossing,
    }
}

/// The drawing of a structure: its atoms and bonds laid out as the opening
/// 3D view shows them, with a model shown in 3D that holds them all and the
/// cell, if there is one.
pub(super) fn document(structure: Structure) -> Result<Document, Error> {
    let Structure { atoms, lattice, .. } = structure;
    let links = links(&atoms, lattice.as_ref())?;
    let arranged = arrange(&atoms, &links, lattice.as_ref());
    let mut doc = Document::default();
    let ids: Vec<u64> = atoms
        .iter()
        .map(|atom| {
            let id = doc.add_atom(&atom.element, Point::default());
            if let Some(a) = doc.atom_mut(id) {
                a.charge = atom.charge;
                // Every atom of a crystal file is explicit.
                a.no_implicit = true;
            }
            id
        })
        .collect();
    let id = |i: usize| ids.get(i).copied();
    let mut bonded = HashSet::new();
    for &(a, b) in &arranged.bonds {
        let (Some(a), Some(b)) = (id(a), id(b)) else {
            continue;
        };
        if a != b && bonded.insert((a.min(b), a.max(b))) {
            doc.add_bond(a, b, 1, "plain");
        }
    }
    let narrow = |v: i32| i8::try_from(v).unwrap_or(0);
    let mut crossing: Vec<Crossing> = arranged
        .crossing
        .iter()
        .filter_map(|l| {
            let (a, b) = (id(l.a)?, id(l.b)?);
            Some(if a <= b {
                Crossing {
                    a,
                    b,
                    image: l.image.map(narrow),
                }
            } else {
                Crossing {
                    a: b,
                    b: a,
                    image: l.image.map(|v| narrow(-v)),
                }
            })
        })
        .collect();
    let mut seen = HashSet::new();
    crossing.retain(|c| seen.insert(*c));
    let narrow_xyz = |p: V3| p.map(|v| v as f32);
    let placed: Vec<Placed> = ids
        .iter()
        .zip(&arranged.xyz)
        .map(|(id, p)| Placed {
            id: *id,
            xyz: narrow_xyz(*p),
        })
        .collect();
    let sites: Vec<Site> = ids
        .iter()
        .zip(&atoms)
        .filter(|(_, a)| !a.label.is_empty() || a.adp.is_some())
        .map(|(id, a)| Site {
            id: *id,
            label: a.label.chars().take(64).collect(),
            adp: a.adp,
        })
        .collect();
    let cell = lattice.map(|l| Cell {
        vectors: l.vectors.map(narrow_xyz),
        crossing,
    });
    let model =
        Model3d::new(&doc, placed, vec![], ForceField::default()).with_cell(&doc, cell, sites);
    lay_out(&mut doc, &model);
    doc.models.push(model);
    models3d::require_version(&mut doc);
    models3d::set_display(&mut doc, 0, Display::ThreeD);
    doc.validate().map_err(|e| Error::new(&e))?;
    Ok(doc)
}

/// Place each 2D atom where the opening view shows it, at the drawing's
/// scale, keeping its depth, as MOL import keeps 3D coordinates.
fn lay_out(doc: &mut Document, model: &Model3d) {
    let positions: HashMap<u64, [f32; 3]> = models3d::viewed(doc, model).into_iter().collect();
    for atom in &mut doc.atoms {
        if let Some([x, y, z]) = positions.get(&atom.id) {
            atom.position = Point::new(x * UNITS, -y * UNITS);
            atom.depth = z * UNITS;
        }
    }
}
