//! Resolve crossings at their actual XYZ intersection, with explicit layers
//! taking precedence over automatic depth. Positive Z faces the viewer.
use crate::document::{Bond, Document, Point};

pub(super) fn at(doc: &Document, bond: &Bond, t: f32) -> Option<f32> {
    let a = doc.atom(bond.a)?.depth;
    let b = doc.atom(bond.b)?.depth;
    Some(a + (b - a) * t)
}

// Early projected pi shortcuts assigned -1 to every contact. Recognize that
// old default only against its own projected ligand; unrelated layers remain
// explicit. New shortcuts use layer 0, and Send backward uses -2 or lower.
pub(super) fn layer(doc: &Document, bond: &Bond, ring: &[u64], projected: bool) -> i16 {
    if bond.z_order == -1
        && projected
        && [bond.a, bond.b].iter().any(|id| {
            doc.atom(*id).is_some_and(|a| {
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

pub(super) fn bond_over(doc: &Document, i: usize, t: f32, j: usize, u: f32) -> bool {
    let (Some(a), Some(b)) = (doc.bonds.get(i), doc.bonds.get(j)) else {
        return i > j;
    };
    let za = layer(doc, a, &[b.a, b.b], b.projection);
    let zb = layer(doc, b, &[a.a, a.b], a.projection);
    if za == zb
        && let Some(dz) = at(doc, a, t).zip(at(doc, b, u)).map(|(a, b)| a - b)
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
    pub(super) fn fit(doc: &Document, ids: &[u64]) -> Option<Self> {
        let atoms: Option<Vec<_>> = ids.iter().map(|id| doc.atom(*id)).collect();
        let atoms = atoms?;
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

    pub(super) fn at(&self, p: Point) -> f32 {
        self.z + (p.x - self.origin.x) * self.dx + (p.y - self.origin.y) * self.dy
    }
}
