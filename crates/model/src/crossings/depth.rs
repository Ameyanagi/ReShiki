//! Resolve crossings at their actual XYZ intersection, with explicit layers
//! taking precedence over automatic depth. Positive Z faces the viewer.
use crate::document::{Atom, Bond, Document, Point};
use crate::scene::graph::Graph;

/// Atoms by ID: a document's own search, or a scene's index of it.
pub(super) trait Atoms {
    fn atom(&self, id: u64) -> Option<&Atom>;
}
impl Atoms for Document {
    fn atom(&self, id: u64) -> Option<&Atom> {
        Document::atom(self, id)
    }
}
impl Atoms for Graph<'_> {
    fn atom(&self, id: u64) -> Option<&Atom> {
        Graph::atom(self, id)
    }
}

pub(super) fn at(atoms: &impl Atoms, bond: &Bond, t: f32) -> Option<f32> {
    let a = atoms.atom(bond.a)?.depth;
    let b = atoms.atom(bond.b)?.depth;
    Some(a + (b - a) * t)
}

// Early projected pi shortcuts assigned -1 to every contact. Recognize that
// old default only against its own projected ligand; unrelated layers remain
// explicit. New shortcuts use layer 0, and Send backward uses -2 or lower.
pub(super) fn layer(atoms: &impl Atoms, bond: &Bond, ring: &[u64], projected: bool) -> i16 {
    if bond.z_order == -1
        && projected
        && [bond.a, bond.b].iter().any(|id| {
            atoms.atom(*id).is_some_and(|a| {
                a.attachment == Some(crate::attachments::Kind::MultiCenter)
                    && ring.iter().all(|id| a.centroid.contains(id))
            })
        })
    {
        0
    } else {
        bond.z_order
    }
}

pub fn bond_over(doc: &Document, i: usize, t: f32, j: usize, u: f32) -> bool {
    bond_over_with(doc, doc, i, t, j, u)
}

/// [`bond_over`] with `doc`'s atoms looked up in `atoms`.
pub(super) fn bond_over_with(
    doc: &Document,
    atoms: &impl Atoms,
    i: usize,
    t: f32,
    j: usize,
    u: f32,
) -> bool {
    let (Some(a), Some(b)) = (doc.bonds.get(i), doc.bonds.get(j)) else {
        return i > j;
    };
    let za = layer(atoms, a, &[b.a, b.b], b.projection);
    let zb = layer(atoms, b, &[a.a, a.b], a.projection);
    if za == zb
        && let Some(dz) = at(atoms, a, t).zip(at(atoms, b, u)).map(|(a, b)| a - b)
        && dz.abs() > 0.001
    {
        return dz > 0.;
    }
    (za, i) > (zb, j)
}

/// Z as an affine function of screen XY, fitted only to a planar, non-edge-on
/// ring. Degenerate/nonplanar drawings retain ordinary layer ordering.
pub(super) struct Plane {
    origin: Point,
    z: f32,
    dx: f32,
    dy: f32,
}
impl Plane {
    // Keep this separate from crate::scene::depth_field::DepthField::plane:
    // crossings use f32 throughout; depth uses f64 with weight subtraction
    // still in f32. Area starts at 0 with area.max(0.001), versus 1e-6 there,
    // and residual limits are 0.01 here versus 0.001 after f32 rounding there.
    // These fits must remain separate to preserve output.
    pub(super) fn fit(atoms: &[&Atom]) -> Option<Self> {
        let first = *atoms.first()?;
        let mut best = None;
        let mut area = 0_f32;
        for (i, a) in atoms.iter().enumerate().skip(1) {
            for b in atoms.iter().skip(i + 1) {
                let ax = a.position.x - first.position.x;
                let ay = a.position.y - first.position.y;
                let bx = b.position.x - first.position.x;
                let by = b.position.y - first.position.y;
                let det = ax * by - ay * bx;
                if det.abs() > area.max(0.001) {
                    area = det.abs();
                    let az = a.depth - first.depth;
                    let bz = b.depth - first.depth;
                    best = Some(Self {
                        origin: first.position,
                        z: first.depth,
                        dx: (az * by - ay * bz) / det,
                        dy: (ax * bz - az * bx) / det,
                    });
                }
            }
        }
        let plane = best?;
        atoms
            .iter()
            .all(|a| (plane.at(a.position) - a.depth).abs() < 0.01)
            .then_some(plane)
    }

    /// Least-squares plane through a slightly warped 3D ring, such as an
    /// optimized fullerene face whose atoms miss the exact fit by more than
    /// its tolerance. Without it, rear cage rings fall back to bond index
    /// order and can cut front bonds. A ring projected to a line keeps its
    /// mean depth; a ring without depth variation stays undecided.
    pub(super) fn least_squares(atoms: &[&Atom]) -> Option<Self> {
        let n = atoms.len() as f32;
        if atoms.len() < 3 {
            return None;
        }
        let origin = Point::new(
            atoms.iter().map(|a| a.position.x).sum::<f32>() / n,
            atoms.iter().map(|a| a.position.y).sum::<f32>() / n,
        );
        let z = atoms.iter().map(|a| a.depth).sum::<f32>() / n;
        if atoms.iter().all(|a| (a.depth - z).abs() < 0.001) {
            return None;
        }
        let (mut xx, mut xy, mut yy, mut xz, mut yz) = (0., 0., 0., 0., 0.);
        for a in atoms {
            let (x, y, w) = (
                a.position.x - origin.x,
                a.position.y - origin.y,
                a.depth - z,
            );
            xx += x * x;
            xy += x * y;
            yy += y * y;
            xz += x * w;
            yz += y * w;
        }
        let det = xx * yy - xy * xy;
        // Only a ring collapsed to a line has no depth gradient to fit.
        let (dx, dy) = if det > 1e-6 * (xx + yy).powi(2) {
            ((xz * yy - yz * xy) / det, (yz * xx - xz * xy) / det)
        } else {
            (0., 0.)
        };
        Some(Self { origin, z, dx, dy })
    }

    pub(super) fn at(&self, p: Point) -> f32 {
        self.z + (p.x - self.origin.x) * self.dx + (p.y - self.origin.y) * self.dy
    }
}
