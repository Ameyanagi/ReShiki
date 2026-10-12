//! View visibility for conservative closed cages and real projected ink.
//! Geometry is recomputed from retained XYZ; RGB/frozen paint is not a camera.
use super::{Field, PathCommand, Point, Primitive, flatten, ink};
use crate::document::{Bond, Document};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;

mod budget;
mod cage;
mod occluder_ink;
mod rims;
mod triangle;
mod vector;

use budget::{
    MAX_INK_EDGES, MAX_INK_WORK, MAX_LINE_QUERIES, crossing_queries, estimated_queries, rear_ids,
};
use cage::cages;
use occluder_ink::Ink;
use rims::{on_segment, rims};
use triangle::Triangle;
use vector::xy;

#[cfg(test)]
thread_local! {
    static CONSTRUCTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(super) fn construction_count() -> usize {
    CONSTRUCTIONS.with(std::cell::Cell::get)
}

#[derive(Default)]
pub(super) struct Visibility {
    triangles: Vec<Triangle>,
    ink: Vec<Ink>,
    epsilon: f64,
    rim_atoms: BTreeMap<u64, (u64, Point)>,
    rim_bonds: BTreeMap<(u64, u64), (u64, Point, Point)>,
    atom_shells: BTreeMap<u64, u64>,
}
impl Visibility {
    pub(super) fn new(doc: &Document) -> Self {
        #[cfg(test)]
        CONSTRUCTIONS.with(|count| count.set(count.get() + 1));
        // Flat 2D drawings retain their exact paint order and opaque ink.
        let lo = doc
            .atoms
            .iter()
            .map(|a| a.depth)
            .fold(f32::INFINITY, f32::min);
        let hi = doc
            .atoms
            .iter()
            .map(|a| a.depth)
            .fold(f32::NEG_INFINITY, f32::max);
        if hi - lo < 1e-6 {
            return Self::default();
        }
        let epsilon = f64::from(doc.drawing_style.bond_length_world.max(1.)) * 0.0001;
        let rear = rear_ids(doc);
        let mut ink = vec![];
        let mut budget = MAX_INK_EDGES;
        let sources = crate::scene::occlusion_ink(doc);
        // Conservative preflight estimates, not a formal operation counter.
        // Include actual generated dash/rail pieces: one long dashed bond is
        // not merely one query. Crossing cuts can proliferate runtime pieces.
        let source_queries = sources
            .iter()
            .filter(|(owners, _, _, _)| owners.iter().any(|id| rear.contains(id)))
            .map(|(_, _, _, parts)| parts.len())
            .sum::<usize>()
            .saturating_mul(4)
            .saturating_add(rear.len());
        let queries = estimated_queries(doc, &rear)
            .max(source_queries)
            .saturating_add(crossing_queries(doc, &rear));
        let triangles = cages(doc, queries);
        let (rim_atoms, rim_bonds, atom_shells) = rims(doc, &triangles, epsilon);
        for (owners, layer, field, parts) in sources {
            let mut contours = vec![];
            for part in parts {
                let part = match part {
                    Primitive::Line(a, b, w) => Primitive::Polygon(super::capsule(a, b, w)),
                    p => ink::single_paint(p),
                };
                let paths = match part {
                    Primitive::Polygon(p) => vec![p],
                    Primitive::Path {
                        commands,
                        filled: true,
                        ..
                    } => flatten(&commands),
                    _ => vec![],
                };
                for path in paths {
                    if path.len() > budget {
                        return Self::default();
                    }
                    budget -= path.len();
                    contours.push(path);
                }
            }
            if !contours.is_empty() {
                ink.push(Ink {
                    owners,
                    layer,
                    field,
                    contours,
                });
            }
        }
        let work = ink
            .iter()
            .map(|i| {
                let edges = i.contours.iter().map(Vec::len).sum::<usize>();
                edges.saturating_mul((usize::BITS - edges.max(1).leading_zeros()) as usize + 3)
            })
            .sum::<usize>();
        if triangles.len().saturating_mul(queries) > MAX_LINE_QUERIES
            || work.saturating_mul(queries) > MAX_INK_WORK
        {
            return Self::default();
        }
        Self {
            triangles,
            ink,
            epsilon,
            rim_atoms,
            rim_bonds,
            atom_shells,
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.triangles.is_empty() && self.ink.is_empty()
    }
    pub(super) fn rim_anchor(&self, id: u64) -> Option<Point> {
        self.rim_atoms.get(&id).map(|(_, p)| *p)
    }
    pub(super) fn hidden(
        &self,
        a: Point,
        b: Point,
        field: Field,
        owners: &[u64],
        layer: i16,
    ) -> Vec<(f32, f32)> {
        self.hidden_with_support(a, b, field, owners, layer, None)
    }
    pub(super) fn hidden_with_support(
        &self,
        a: Point,
        b: Point,
        field: Field,
        owners: &[u64],
        layer: i16,
        support: Option<&[u64]>,
    ) -> Vec<(f32, f32)> {
        self.hidden_with_policy(a, b, field, owners, layer, (support, None))
    }
    /// An exposed anchor owns its original shared corner, but unrelated
    /// foreground ink must still occlude that corner's restored opaque paint.
    pub(super) fn external_cap_hidden(
        &self,
        a: Point,
        b: Point,
        field: Field,
        bond: &Bond,
        anchor: u64,
    ) -> Vec<(f32, f32)> {
        let shell = self.rim_atoms.get(&anchor).map(|(shell, _)| *shell);
        self.hidden_with_policy(a, b, field, &[bond.a, bond.b], bond.z_order, (None, shell))
    }
    /// Face layer rule: a face below the query layer never hides; an
    /// equal-layer face hides only where it is in front; a higher-layer face
    /// depth-tests owners that all lie in its shell and hides everything else it
    /// covers in projection. Ink uses its own layer rule in Ink::intervals, and
    /// masks() uses a different one. Pinned by
    /// view_visibility_negative_cage_layers_characterize_masks_and_hidden_ranges.
    fn hidden_with_policy(
        &self,
        a: Point,
        b: Point,
        field: Field,
        owners: &[u64],
        layer: i16,
        (support, exempt_shell): (Option<&[u64]>, Option<u64>),
    ) -> Vec<(f32, f32)> {
        let za = f64::from(field.at(a));
        let zb = f64::from(field.at(b));
        let mut ranges = vec![];
        let support = support.map(face_key);
        let rim_shell = self.rim_exemption(a, b, owners, support.is_some(), exempt_shell);
        for triangle in &self.triangles {
            if rim_shell == Some(triangle.shell) {
                continue;
            }
            if support.as_ref().is_some_and(|key| *key == triangle.face) {
                continue;
            }
            let same_shell = self.all_in_shell(owners, triangle.shell);
            if triangle.layer < layer {
                continue;
            }
            let depth = (same_shell || triangle.layer == layer).then_some((za, zb));
            if let Some(r) = triangle.interval(a, b, depth, self.epsilon) {
                ranges.push(r);
            }
        }
        for ink in &self.ink {
            if rim_shell.is_some_and(|shell| self.all_in_shell(&ink.owners, shell)) {
                continue;
            }
            if !ink.owners.iter().any(|id| owners.contains(id)) {
                ranges.extend(ink.intervals(a, b, za, zb, layer, self.epsilon));
            }
        }
        merge(ranges)
    }
    fn rim_exemption(
        &self,
        a: Point,
        b: Point,
        owners: &[u64],
        has_support: bool,
        exempt_shell: Option<u64>,
    ) -> Option<u64> {
        // A genuine geometric rim is exposed to its own shell. Its graphical
        // stroke thickness has no separate XYZ and must stay solid in full.
        if exempt_shell.is_some() {
            exempt_shell
        } else if let [first, last] = owners
            && !has_support
        {
            self.rim_bonds
                .get(&((*first).min(*last), (*first).max(*last)))
                .filter(|(_, x, y)| {
                    on_segment(a, *x, *y, self.epsilon) && on_segment(b, *x, *y, self.epsilon)
                })
                .map(|(shell, _, _)| *shell)
        } else if let [id] = owners
            && a == b
        {
            self.rim_atoms
                .get(id)
                .filter(|(_, p)| p.distance(a) <= self.epsilon as f32)
                .map(|(shell, _)| *shell)
        } else {
            None
        }
    }
    fn all_in_shell(&self, ids: &[u64], shell: u64) -> bool {
        ids.iter()
            .all(|id| self.atom_shells.get(id) == Some(&shell))
    }
    pub(super) fn occluded(&self, p: Point, z: f32, owners: &[u64], layer: i16) -> bool {
        !self
            .hidden(p, p, Field::Constant(z), owners, layer)
            .is_empty()
    }
    /// Unlike hidden_with_policy, masks ignore the query layer: faces with a
    /// nonnegative layer depth-test every owner group, and faces with a negative
    /// layer only mask owners that all lie in their shell. Pinned by
    /// view_visibility_negative_cage_layers_characterize_masks_and_hidden_ranges.
    pub(super) fn masks(&self, field: Field, owners: &[u64]) -> Vec<Vec<Point>> {
        let support = face_key(owners);
        self.triangles
            .iter()
            .filter(|triangle| {
                triangle.face != support
                    && (triangle.layer >= 0 || self.all_in_shell(owners, triangle.shell))
            })
            .filter_map(|triangle| {
                let p: Vec<_> = triangle.vertices.iter().copied().map(xy).collect();
                let clipped = super::clip_values(&p, |p| {
                    triangle.depth(p).unwrap_or(f64::NEG_INFINITY)
                        - f64::from(field.at(p))
                        - self.epsilon
                });
                (clipped.len() >= 3).then_some(clipped)
            })
            .collect()
    }
}

fn face_key(ids: &[u64]) -> Vec<u64> {
    let reverse: Vec<_> = ids.iter().rev().copied().collect();
    [ids, reverse.as_slice()]
        .into_iter()
        .flat_map(|p| {
            (0..p.len()).map(move |i| {
                p.iter()
                    .cycle()
                    .skip(i)
                    .take(p.len())
                    .copied()
                    .collect::<Vec<_>>()
            })
        })
        .min()
        .unwrap_or_default()
}
pub(super) fn merge(mut ranges: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    ranges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut out: Vec<(f32, f32)> = vec![];
    for (a, b) in ranges {
        let a = a.clamp(0., 1.);
        let b = b.clamp(0., 1.);
        if b - a <= 1e-7 {
            continue;
        }
        if let Some(last) = out.last_mut()
            && a <= last.1 + 1e-6
        {
            last.1 = last.1.max(b);
        } else {
            out.push((a, b));
        }
    }
    out
}

pub(crate) fn bond_field(doc: &Document, bond: &Bond) -> Field {
    let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
        return Field::Constant(0.);
    };
    let linear = Field::Bond {
        a: a.position,
        b: b.position,
        wa: a.depth,
        wb: b.depth,
    };
    if matches!(bond.order, 2 | 7) && bond.double_position == crate::bonds::DoublePosition::Auto {
        crate::scene::projected_bonds::face_atoms(doc, bond)
            .and_then(|ids| {
                let vertices: Vec<_> = ids.iter().filter_map(|id| doc.atom(*id)).collect();
                let n = vertices.len().max(1) as f32;
                let center = vertices.iter().fold(Point::default(), |p, a| {
                    p.offset(a.position.x / n, a.position.y / n)
                });
                let depth = vertices.iter().map(|a| a.depth / n).sum();
                Field::plane(&[
                    (a.position, a.depth),
                    (b.position, b.depth),
                    (center, depth),
                ])
            })
            .unwrap_or(linear)
    } else {
        linear
    }
}

#[cfg(test)]
impl Triangle {
    fn hidden(&self, a: Point, b: Point, za: f64, zb: f64, epsilon: f64) -> Option<(f32, f32)> {
        self.interval(a, b, Some((za, zb)), epsilon)
    }
}

#[cfg(test)]
impl Visibility {
    pub(super) fn support_ranges(
        &self,
        a: Point,
        b: Point,
        field: Field,
        support: &[u64],
    ) -> Vec<(f32, f32)> {
        let key = face_key(support);
        merge(
            self.triangles
                .iter()
                .filter(|t| t.face == key)
                .filter_map(|t| {
                    t.hidden(a, b, field.at(a).into(), field.at(b).into(), self.epsilon)
                })
                .collect(),
        )
    }
    pub(super) fn face_count(&self) -> usize {
        self.triangles
            .iter()
            .map(|t| (&t.face, t.shell))
            .collect::<BTreeSet<_>>()
            .len()
    }
    pub(super) fn raw_atom_hits(
        &self,
        p: Point,
        z: f32,
        owners: &[u64],
    ) -> (Vec<Vec<u64>>, Vec<Vec<u64>>) {
        let triangles = self
            .triangles
            .iter()
            .filter(|t| t.hidden(p, p, z.into(), z.into(), self.epsilon).is_some())
            .map(|t| t.face.clone())
            .collect();
        let ink = self
            .ink
            .iter()
            .filter(|i| {
                !i.owners.iter().any(|id| owners.contains(id))
                    && !i
                        .intervals(p, p, z.into(), z.into(), 0, self.epsilon)
                        .is_empty()
            })
            .map(|i| i.owners.clone())
            .collect();
        (triangles, ink)
    }
}

#[cfg(test)]
mod tests;
