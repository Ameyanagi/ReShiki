use super::{P, Point3, Quality, State};
use std::collections::HashSet;

struct Ring {
    atoms: HashSet<usize>,
    points: Vec<P>,
    center: P,
    lo: P,
    hi: P,
}
fn inside(point: P, polygon: &[P]) -> bool {
    let mut previous = polygon.last().copied().unwrap_or_default();
    let mut odd = false;
    for &next in polygon {
        if (previous.1 > point.1) != (next.1 > point.1)
            && point.0
                < (next.0 - previous.0) * (point.1 - previous.1) / (next.1 - previous.1)
                    + previous.0
        {
            odd = !odd;
        }
        previous = next;
    }
    odd
}

pub(super) fn quality(state: &State, positions: &[Point3]) -> Quality {
    if state.rings.atoms.len() > 256
        || state.rings.atoms.iter().map(Vec::len).sum::<usize>() > 16_384
    {
        return Quality::Unchecked;
    }
    let mut rings = Vec::new();
    for atoms in &state.rings.atoms {
        if atoms.len() < 3 {
            return Quality::Unchecked;
        }
        let mut points = Vec::new();
        let (mut x, mut y) = (0., 0.);
        let (mut lo, mut hi) = (
            (f64::INFINITY, f64::INFINITY),
            (f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for &atom in atoms {
            let Some(p) = positions.get(atom) else {
                return Quality::Unchecked;
            };
            points.push((p.x, p.y));
            x += p.x;
            y += p.y;
            lo.0 = lo.0.min(p.x);
            lo.1 = lo.1.min(p.y);
            hi.0 = hi.0.max(p.x);
            hi.1 = hi.1.max(p.y);
        }
        rings.push(Ring {
            atoms: atoms.iter().copied().collect(),
            points,
            center: (x / atoms.len() as f64, y / atoms.len() as f64),
            lo,
            hi,
        });
    }
    let mut remaining = 2_000_000usize;
    for (index, a) in rings.iter().enumerate() {
        for b in rings.iter().skip(index + 1) {
            let Some(next) = remaining.checked_sub(a.points.len() + b.points.len() + 1) else {
                return Quality::Unchecked;
            };
            remaining = next;
            // Fused and bridged rings intentionally share their boundary.
            if !a.atoms.is_disjoint(&b.atoms) {
                continue;
            }
            if (a.center.0 - b.center.0).hypot(a.center.1 - b.center.1) < 1.5 - 1e-8 {
                return Quality::Crowded;
            }
            if a.lo.0 > b.hi.0 || b.lo.0 > a.hi.0 || a.lo.1 > b.hi.1 || b.lo.1 > a.hi.1 {
                continue;
            }
            if a.points.first().is_some_and(|&p| inside(p, &b.points))
                || b.points.first().is_some_and(|&p| inside(p, &a.points))
            {
                return Quality::Crowded;
            }
        }
    }
    Quality::Clear
}
