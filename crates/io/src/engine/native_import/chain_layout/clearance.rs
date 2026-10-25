use super::State;
use crate::{chemistry::stereo::Point3, document::Document};
use std::collections::{HashMap, HashSet};
mod rings;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum Quality {
    Clear,
    Crowded,
    Unchecked,
}

type P = (f64, f64);
#[derive(Clone, Copy)]
enum Item {
    Atom(usize, P),
    Edge(usize, usize, P, P),
    Ink(usize, P, P),
}
fn bounds(item: Item) -> (P, P) {
    let (a, b, pad) = match item {
        Item::Atom(_, p) => (p, p, 0.375),
        Item::Edge(_, _, a, b) => (a, b, 0.375),
        Item::Ink(_, a, b) => (a, b, 0.02),
    };
    (
        (a.0.min(b.0) - pad, a.1.min(b.1) - pad),
        (a.0.max(b.0) + pad, a.1.max(b.1) + pad),
    )
}
fn cross(a: P, b: P, c: P) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}
fn distance(p: P, a: P, b: P) -> f64 {
    let (x, y) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * x + (p.1 - a.1) * y) / (x * x + y * y).max(1e-20)).clamp(0., 1.);
    (p.0 - a.0 - t * x).hypot(p.1 - a.1 - t * y)
}
fn intersects(a: P, b: P, c: P, d: P) -> bool {
    let eps = 1e-8;
    if a.0.min(b.0) > c.0.max(d.0) + eps
        || c.0.min(d.0) > a.0.max(b.0) + eps
        || a.1.min(b.1) > c.1.max(d.1) + eps
        || c.1.min(d.1) > a.1.max(b.1) + eps
    {
        return false;
    }
    cross(a, b, c) * cross(a, b, d) <= eps && cross(c, d, a) * cross(c, d, b) <= eps
}
fn inside(p: P, lo: P, hi: P) -> bool {
    p.0 >= lo.0 && p.0 <= hi.0 && p.1 >= lo.1 && p.1 <= hi.1
}
fn box_edge(lo: P, hi: P, a: P, b: P) -> bool {
    inside(a, lo, hi)
        || inside(b, lo, hi)
        || [
            (lo, (hi.0, lo.1)),
            ((hi.0, lo.1), hi),
            (hi, (lo.0, hi.1)),
            ((lo.0, hi.1), lo),
        ]
        .into_iter()
        .any(|(c, d)| intersects(a, b, c, d))
}
fn collision(a: Item, b: Item, bonds: &HashSet<(usize, usize)>) -> bool {
    match (a, b) {
        (Item::Atom(i, p), Item::Atom(j, q)) => {
            !bonds.contains(&(i.min(j), i.max(j))) && (p.0 - q.0).hypot(p.1 - q.1) < 0.75 - 1e-8
        }
        (Item::Atom(i, p), Item::Edge(j, k, a, b)) | (Item::Edge(j, k, a, b), Item::Atom(i, p)) => {
            i != j && i != k && distance(p, a, b) < 0.30 - 1e-8
        }
        (Item::Edge(i, j, a, b), Item::Edge(k, l, c, d)) => {
            if i == k || i == l || j == k || j == l {
                let (shared, p, q) = if i == k {
                    (a, b, d)
                } else if i == l {
                    (a, b, c)
                } else if j == k {
                    (b, a, d)
                } else {
                    (b, a, c)
                };
                cross(shared, p, q).abs() < 1e-8
                    && (p.0 - shared.0) * (q.0 - shared.0) + (p.1 - shared.1) * (q.1 - shared.1)
                        > 1e-8
            } else {
                intersects(a, b, c, d)
            }
        }
        (Item::Ink(i, lo, hi), Item::Ink(j, a, b)) => {
            i != j && lo.0 < b.0 && hi.0 > a.0 && lo.1 < b.1 && hi.1 > a.1
        }
        (Item::Ink(i, lo, hi), Item::Atom(j, p)) | (Item::Atom(j, p), Item::Ink(i, lo, hi)) => {
            i != j && inside(p, lo, hi)
        }
        (Item::Ink(i, lo, hi), Item::Edge(j, k, a, b))
        | (Item::Edge(j, k, a, b), Item::Ink(i, lo, hi)) => {
            i != j && i != k && box_edge(lo, hi, a, b)
        }
    }
}

fn check(state: &State, items: Vec<Item>) -> Quality {
    let bonds: HashSet<_> = state
        .graph
        .bonds
        .iter()
        .map(|b| (b.a.min(b.b), b.a.max(b.b)))
        .collect();
    let mut grid = HashMap::<(i64, i64), Vec<usize>>::new();
    let mut pairs = HashSet::new();
    let mut remaining = 2_000_000usize;
    for (index, &item) in items.iter().enumerate() {
        let (lo, hi) = bounds(item);
        if ![lo.0, lo.1, hi.0, hi.1]
            .into_iter()
            .all(|value| value.is_finite() && value.abs() <= 1e7)
        {
            return Quality::Unchecked;
        }
        let (x0, x1, y0, y1) = (
            (lo.0 / 1.5).floor() as i64,
            (hi.0 / 1.5).floor() as i64,
            (lo.1 / 1.5).floor() as i64,
            (hi.1 / 1.5).floor() as i64,
        );
        if x1 - x0 > 32 || y1 - y0 > 32 {
            return Quality::Unchecked;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                let cell = grid.entry((x, y)).or_default();
                for &other in cell.iter() {
                    let Some(next) = remaining.checked_sub(1) else {
                        return Quality::Unchecked;
                    };
                    remaining = next;
                    if pairs.insert((other, index)) {
                        let Some(&previous) = items.get(other) else {
                            return Quality::Unchecked;
                        };
                        if collision(previous, item, &bonds) {
                            return Quality::Crowded;
                        }
                    }
                }
                cell.push(index);
            }
        }
    }
    Quality::Clear
}

fn geometry_items(state: &State, positions: &[Point3]) -> Option<Vec<Item>> {
    if positions.len() != state.graph.atoms.len()
        || positions.len() > 4096
        || state.graph.bonds.len() > 6144
    {
        return None;
    }
    let mut items: Vec<_> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| Item::Atom(i, (p.x, p.y)))
        .collect();
    for bond in &state.graph.bonds {
        let a = positions.get(bond.a)?;
        let b = positions.get(bond.b)?;
        if !((0.9 - 1e-8)..=(2.4 + 1e-8)).contains(&(a.x - b.x).hypot(a.y - b.y)) {
            return None;
        }
        items.push(Item::Edge(bond.a, bond.b, (a.x, a.y), (b.x, b.y)));
    }
    Some(items)
}
pub(super) fn geometry(state: &State, positions: &[Point3]) -> Quality {
    let quality =
        geometry_items(state, positions).map_or(Quality::Unchecked, |items| check(state, items));
    if quality == Quality::Clear {
        rings::quality(state, positions)
    } else {
        quality
    }
}
pub(super) fn labels(state: &State, positions: &[Point3], doc: &Document) -> Quality {
    let Some(mut items) = geometry_items(state, positions) else {
        return Quality::Unchecked;
    };
    let ids: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    for atom in &doc.atoms {
        if let Some((lo, hi)) = crate::scene::atom_label_ink_bounds(atom, doc) {
            let Some(&id) = ids.get(&atom.id) else {
                return Quality::Unchecked;
            };
            let scale = crate::chemistry::document::WORLD_PER_DEPICT_UNIT;
            // Drawing Y points down; native conformer Y points up.
            items.push(Item::Ink(
                id,
                (f64::from(lo.x) / scale, -f64::from(hi.y) / scale),
                (f64::from(hi.x) / scale, -f64::from(lo.y) / scale),
            ));
        }
    }
    check(state, items)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crossings_collinear_overlaps_and_label_obstacles_are_distinct() {
        let empty = HashSet::new();
        assert!(collision(
            Item::Edge(0, 1, (0., 0.), (2., 2.)),
            Item::Edge(2, 3, (0., 2.), (2., 0.)),
            &empty
        ));
        assert!(collision(
            Item::Edge(0, 1, (0., 0.), (2., 0.)),
            Item::Edge(2, 3, (1., 0.), (3., 0.)),
            &empty
        ));
        assert!(!collision(
            Item::Edge(0, 1, (0., 0.), (1., 0.)),
            Item::Edge(1, 2, (1., 0.), (2., 0.)),
            &empty
        ));
        assert!(collision(
            Item::Ink(0, (0., 0.), (1., 1.)),
            Item::Edge(1, 2, (-1., 0.5), (2., 0.5)),
            &empty
        ));
        assert!(collision(
            Item::Atom(0, (0.5, 0.2)),
            Item::Edge(1, 2, (0., 0.), (1., 0.)),
            &empty
        ));
    }

    #[test]
    fn a_ring_inside_another_is_rejected_without_crossings_or_close_atoms() -> anyhow::Result<()> {
        let text = format!("C1{}1.c1ccccc1", "C".repeat(23));
        let state = crate::chemistry::smiles::read(&text)?.prepared.state;
        let mut positions = Vec::new();
        for (count, radius, x) in [
            (24usize, 1.5 / (2. * (std::f64::consts::PI / 24.).sin()), 0.),
            (6, 1.5, 2.),
        ] {
            for index in 0..count {
                let angle = index as f64 * 2. * std::f64::consts::PI / count as f64;
                positions.push(Point3 {
                    x: x + radius * angle.cos(),
                    y: radius * angle.sin(),
                    z: 0.,
                });
            }
        }
        assert_eq!(
            check(
                &state,
                geometry_items(&state, &positions).ok_or_else(|| anyhow::anyhow!("Geometry"))?
            ),
            Quality::Clear
        );
        assert_eq!(rings::quality(&state, &positions), Quality::Crowded);
        assert_eq!(geometry(&state, &positions), Quality::Crowded);
        Ok(())
    }
}
