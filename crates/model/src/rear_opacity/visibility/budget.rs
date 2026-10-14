use super::{Bond, Document, PathCommand, Point, Primitive, flatten};
use std::collections::BTreeSet;

pub(super) const MAX_VERTICES: usize = 256;
pub(super) const MAX_FACE: usize = 24;
pub(super) const MAX_INK_EDGES: usize = 100_000;
pub(super) const MAX_TRIANGLES: usize = 768;
pub(super) const MAX_TRIANGLE_PAIRS: usize = 500_000;
pub(super) const MAX_LINE_QUERIES: usize = 1_000_000;
pub(super) const MAX_INK_WORK: usize = 100_000_000;
const MAX_CROSSING_WORK: usize = 200_000;

/// Conservative preflight weight for each generated source part.
pub(super) const QUERIES_PER_SOURCE_PART: usize = 4;
/// Conservative preflight weight for each managed bond.
const QUERIES_PER_BOND: usize = 4;
/// Conservative preflight weight for each managed ring fill.
const QUERIES_PER_RING_FILL: usize = 128;
/// Conservative preflight weight for each segment of an owned curve.
const OWNED_CURVE_QUERIES: usize = 2;

pub(super) fn rear_ids(doc: &Document) -> BTreeSet<u64> {
    let mut ids: BTreeSet<_> = doc
        .depth_appearance
        .iter()
        .flat_map(|s| {
            s.atoms
                .iter()
                .filter(|id| s.rear_opacity_for(**id) < 1.)
                .copied()
        })
        .collect();
    // Match Paint's ownership for nonchemical centroids, without charging
    // opaque RGB-owner members as visibility queries.
    for atom in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
        if atom.centroid.iter().any(|id| ids.contains(id)) {
            ids.insert(atom.id);
        } else {
            ids.remove(&atom.id);
        }
    }
    ids
}
pub(super) fn estimated_queries(doc: &Document, rear: &BTreeSet<u64>) -> usize {
    let active = |ids: &[u64]| ids.iter().any(|id| rear.contains(id));
    let bonds = doc.bonds.iter().filter(|b| active(&[b.a, b.b])).count();
    let fills = doc.ring_fills.iter().filter(|f| active(&f.atoms)).count();
    // Use original pre-knockout curves. Query ownership includes a managed
    // foreground bond crossing an otherwise opaque ring, not just ring IDs.
    // The uncut helper never constructs crossing Paint, avoiding recursion.
    let arcs = crate::ring_arcs::uncut(doc);
    let mut work = MAX_CROSSING_WORK;
    let mut curves = 0_usize;
    for (part, ids) in arcs.primitives.iter().zip(&arcs.owners) {
        let Some(cost) = ring_queries(doc, rear, part, ids, &mut work) else {
            return MAX_LINE_QUERIES + 1;
        };
        curves = curves.saturating_add(cost);
    }
    for circle in crate::aromatic::circles(doc)
        .into_iter()
        .filter(|c| !arcs.intersects(c))
    {
        for part in circle.graphic().parts() {
            let part = Primitive::Path {
                commands: part.commands,
                style: part.style,
                filled: part.filled,
            };
            let Some(cost) = ring_queries(doc, rear, &part, &circle.atoms, &mut work) else {
                return MAX_LINE_QUERIES + 1;
            };
            curves = curves.saturating_add(cost);
        }
    }
    rear.len()
        .saturating_add(bonds.saturating_mul(QUERIES_PER_BOND))
        .saturating_add(fills.saturating_mul(QUERIES_PER_RING_FILL))
        .saturating_add(curves)
}
fn ring_queries(
    doc: &Document,
    rear: &BTreeSet<u64>,
    part: &Primitive,
    ids: &[u64],
    work: &mut usize,
) -> Option<usize> {
    let count = curve_queries(part);
    let owned = ids.iter().any(|id| rear.contains(id));
    let Primitive::Path { commands, .. } = part else {
        return Some(MAX_LINE_QUERIES + 1);
    };
    let Some((lo, hi)) = commands.iter().flat_map(PathCommand::iter_points).fold(
        None::<(Point, Point)>,
        |bounds, p| {
            Some(match bounds {
                None => (p, p),
                Some((lo, hi)) => (
                    Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                ),
            })
        },
    ) else {
        return Some(0);
    };
    let mut crossings = 0_usize;
    for bond in &doc.bonds {
        if *work == 0 {
            return None;
        }
        *work -= 1;
        if !owned && !rear.contains(&bond.a) && !rear.contains(&bond.b) {
            continue;
        }
        let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
            continue;
        };
        let (a, b) = (a.position, b.position);
        if doc.bond_visible(bond.a, bond.b)
            && a.x.min(b.x) <= hi.x
            && a.x.max(b.x) >= lo.x
            && a.y.min(b.y) <= hi.y
            && a.y.max(b.y) >= lo.y
        {
            crossings += 1;
        }
    }
    let owned_queries = if owned { OWNED_CURVE_QUERIES } else { 0 };
    Some(count.saturating_mul(crossings.saturating_add(owned_queries)))
}
pub(super) fn curve_queries(part: &Primitive) -> usize {
    match part {
        Primitive::Path {
            commands,
            filled: false,
            ..
        } => flatten(commands)
            .iter()
            .map(|p| p.len().saturating_sub(1))
            .sum::<usize>(),
        _ => 1,
    }
}
pub(super) fn crossing_queries(doc: &Document, rear: &BTreeSet<u64>) -> usize {
    let mut segments: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| {
            doc.bond_visible(b.a, b.b).then_some((
                b,
                doc.atom(b.a)?.position,
                doc.atom(b.b)?.position,
            ))
        })
        .collect();
    segments.sort_by(|(_, a, b), (_, c, d)| a.x.min(b.x).total_cmp(&c.x.min(d.x)));
    let mut active: Vec<(&Bond, Point, Point)> = vec![];
    let mut work = MAX_CROSSING_WORK;
    let mut queries = 0_usize;
    for (first, a, b) in segments {
        active.retain(|(_, c, d)| c.x.max(d.x) + 0.001 >= a.x.min(b.x));
        for (second, c, d) in &active {
            if work == 0 {
                return MAX_LINE_QUERIES + 1;
            }
            work -= 1;
            if ![first.a, first.b, second.a, second.b]
                .iter()
                .any(|id| rear.contains(id))
                || [first.a, first.b]
                    .iter()
                    .any(|id| *id == second.a || *id == second.b)
                || a.y.min(b.y) > c.y.max(d.y) + 0.001
                || a.y.max(b.y) < c.y.min(d.y) - 0.001
            {
                continue;
            }
            let ab = [
                f64::from(b.x) - f64::from(a.x),
                f64::from(b.y) - f64::from(a.y),
            ];
            let cd = [
                f64::from(d.x) - f64::from(c.x),
                f64::from(d.y) - f64::from(c.y),
            ];
            let ac = [
                f64::from(c.x) - f64::from(a.x),
                f64::from(c.y) - f64::from(a.y),
            ];
            let cross = |[x, y]: [f64; 2], [u, v]: [f64; 2]| x * v - y * u;
            let det = cross(ab, cd);
            if det.abs() >= 0.0009
                && (-0.001..=1.001).contains(&(cross(ac, cd) / det))
                && (-0.001..=1.001).contains(&(cross(ac, ab) / det))
            {
                queries = queries.saturating_add(2);
            }
        }
        active.push((first, a, b));
    }
    queries
}
