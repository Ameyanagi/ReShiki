//! Molecular ring drawings. Chair presets are 2D depictions, not 3D conformers
//! or stereochemical assignments. Haworth outlines use foreshortened edges.
use crate::{
    document::{Document, Point},
    editing, templates,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    #[default]
    Regular,
    Benzene,
    ChairUp,
    ChairDown,
    Cyclopentadiene,
    HaworthFive,
    HaworthSix,
}
impl Preset {
    pub const ALL: &'static [Self] = &[
        Self::Regular,
        Self::Benzene,
        Self::ChairUp,
        Self::ChairDown,
        Self::Cyclopentadiene,
        Self::HaworthFive,
        Self::HaworthSix,
    ];
    pub fn document(self, length: f32, alternate: bool) -> Document {
        match self {
            Self::HaworthFive => return crate::haworth::Ring::Five.document(length, false),
            Self::HaworthSix => return crate::haworth::Ring::Six.document(length, false),
            _ => {}
        }
        let mut doc = Document::default();
        let points = match self {
            Self::ChairUp | Self::ChairDown => {
                // Three unit vectors and their opposites close a centrosymmetric
                // chair. Its two shallow edges form the familiar inward corners.
                let mut points = vec![Point::default()];
                let mut current = Point::default();
                for angle in [60_f32, 195., 165., 240., 15.] {
                    let angle = angle.to_radians();
                    current = current.offset(length * angle.cos(), length * angle.sin());
                    points.push(current);
                }
                if self == Self::ChairDown {
                    for p in &mut points {
                        p.y = -p.y;
                    }
                }
                let center = Point::new(
                    points.iter().map(|p| p.x).sum::<f32>() / 6.,
                    points.iter().map(|p| p.y).sum::<f32>() / 6.,
                );
                points
                    .into_iter()
                    .map(|p| p.offset(-center.x, -center.y))
                    .collect::<Vec<_>>()
            }
            _ => {
                let n = if self == Self::Cyclopentadiene { 5 } else { 6 };
                let radius = length / (2. * (std::f32::consts::PI / n as f32).sin());
                (0..n)
                    .map(|i| {
                        let angle = -std::f32::consts::FRAC_PI_2
                            + i as f32 * std::f32::consts::TAU / n as f32;
                        Point::new(radius * angle.cos(), radius * angle.sin())
                    })
                    .collect()
            }
        };
        let ids: Vec<_> = points.into_iter().map(|p| doc.add_atom("C", p)).collect();
        for (i, (&a, &b)) in ids
            .iter()
            .zip(ids.iter().cycle().skip(1))
            .take(ids.len())
            .enumerate()
        {
            let phase = if alternate { 1 } else { 0 };
            let order = if (self == Self::Cyclopentadiene && [phase, phase + 2].contains(&i))
                || (self == Self::Benzene && i % 2 == phase)
            {
                2
            } else {
                1
            };
            doc.add_bond(a, b, order, "plain");
        }
        doc
    }
}
impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Regular => "Regular ring",
            Self::Benzene => "Benzene",
            Self::ChairUp => "Chair A",
            Self::ChairDown => "Chair B",
            Self::Cyclopentadiene => "Cyclopentadiene",
            Self::HaworthFive => "Haworth 5",
            Self::HaworthSix => "Haworth 6",
        })
    }
}

#[derive(Clone, Copy)]
pub struct Drawing {
    pub preset: Preset,
    pub length: f32,
    pub alternate: bool,
    pub connect: bool,
}
impl Drawing {
    pub fn place(
        self,
        doc: &Document,
        point: Point,
        direction: Option<Point>,
        radius: f32,
    ) -> Result<(Document, Vec<u64>), &'static str> {
        if !self.length.is_finite() || self.length <= 0. {
            return Err("Ring bond length must be positive and finite.");
        }
        if doc.validate().is_err() {
            return Err("The drawing is invalid.");
        }
        let part = self.preset.document(self.length, self.alternate);
        let attach = |part: &Document| {
            if self.preset == Preset::Benzene {
                self.place_benzene(doc, point, direction, radius)
            } else {
                templates::place(doc, part, point, direction, radius)
            }
        };
        if !self.connect {
            return attach(&part);
        }
        let Some(target_id) = doc.nearest(point, radius) else {
            if editing::nearest_bond(doc, point, radius).is_some() {
                return Err("Start on an atom to connect a ring with a new bond.");
            }
            return attach(&part);
        };
        let target = doc
            .atom(target_id)
            .ok_or("The attachment atom is no longer available")?;
        if target.stereo.is_some()
            || target.map_num != 0
            || target.explicit_h > 0
            || target.no_implicit
            || target.radical_electrons > 0
            || !target.marks.is_empty()
            || templates::valence(doc, target_id) + 2 > templates::capacity(target)
        {
            return Err("This atom cannot accept a new ring bond without changing its chemistry.");
        }
        let neighbors: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == target_id {
                    doc.atom(b.b)
                } else if b.b == target_id {
                    doc.atom(b.a)
                } else {
                    None
                }
            })
            .map(|a| a.position)
            .collect();
        let angle = direction
            .filter(|p| p.distance(target.position) > radius)
            .map(|p| (p.y - target.position.y).atan2(p.x - target.position.x))
            .unwrap_or_else(|| editing::open_angle(target.position, &neighbors));
        let length = if neighbors.is_empty() {
            self.length
        } else {
            neighbors
                .iter()
                .map(|p| p.distance(target.position))
                .sum::<f32>()
                / neighbors.len() as f32
        };
        let source = part
            .atoms
            .iter()
            .filter(|a| templates::valence(&part, a.id) + 2 <= templates::capacity(a))
            .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .ok_or("This ring has no available connection atom.")?;
        let source_id = source.id;
        let origin = source.position;
        let inward = (-origin.y).atan2(-origin.x);
        let mut positioned = part.clone();
        let all = positioned.all_ids();
        editing::transform_about(
            &mut positioned,
            &all,
            origin,
            length / self.length,
            (angle - inward).to_degrees(),
        );
        let dest = target
            .position
            .offset(length * angle.cos(), length * angle.sin());
        let mut result = doc.clone();
        let ids = editing::append(
            &mut result,
            &positioned,
            Point::new(dest.x - origin.x, dest.y - origin.y),
        );
        let source_index = part
            .all_ids()
            .iter()
            .position(|id| *id == source_id)
            .ok_or("The source ring atom is unavailable")?;
        let new_id = ids
            .get(source_index)
            .copied()
            .ok_or("The ring could not be inserted")?;
        result.add_bond(target_id, new_id, 1, "plain");
        result.reconcile_molecule_groups();
        let mut selected = ids;
        selected.push(target_id);
        Ok((result, selected))
    }

    fn place_benzene(
        self,
        doc: &Document,
        point: Point,
        direction: Option<Point>,
        radius: f32,
    ) -> Result<(Document, Vec<u64>), &'static str> {
        let hexagon = Preset::Regular.document(self.length, false);
        let (mut result, mut ring) = templates::place(doc, &hexagon, point, direction, radius)?;
        if ring.len() != 6 || ring.iter().any(|id| result.atom(*id).is_none()) {
            return Err("The ring could not be inserted.");
        }
        for k in 0..6 {
            let pair = edge(&ring, k);
            if find(doc, pair).is_none()
                && let Some(index) = result.bonds.iter().position(|b| joins(b, pair))
            {
                result.bonds.remove(index);
            }
        }
        let length = hexagon
            .atoms
            .first()
            .zip(hexagon.atoms.get(1))
            .map(|(a, b)| a.position.distance(b.position))
            .unwrap_or(self.length);
        let (first, second) = edge(&ring, 0);
        let scale = result
            .atom(first)
            .zip(result.atom(second))
            .map(|(a, b)| a.position.distance(b.position) / length)
            .filter(|s| s.is_finite() && *s > 0.)
            .unwrap_or(1.);
        let tolerance = FUSE_DISTANCE * self.length * scale;
        let mut merged = Vec::new();
        for k in 0..6 {
            let id = vertex(&ring, k);
            if doc.atom(id).is_some() {
                continue;
            }
            let Some(position) = result.atom(id).map(|a| a.position) else {
                continue;
            };
            let nearby = doc
                .atoms
                .iter()
                .filter(|a| !ring.contains(&a.id) && doc.atom_visible(a.id))
                .map(|a| (a.position.distance(position), a.id))
                .filter(|(d, _)| *d <= tolerance)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, existing)) = nearby
                && let Some(slot) = ring.get_mut(k)
            {
                result.delete(&[id]);
                *slot = existing;
                merged.push(k);
            }
        }
        for &k in &merged {
            let id = vertex(&ring, k);
            let new = [k + 5, k + 1]
                .into_iter()
                .filter(|j| find(doc, (id, vertex(&ring, *j))).is_none())
                .count();
            let room = doc.atom(id).is_none_or(|atom| {
                let capacity = templates::capacity(atom);
                capacity == 0 || templates::valence(doc, id) + 2 * new as u32 <= capacity
            });
            if !room {
                return Err(
                    "No compatible attachment: match elements and an eligible bond, with room for the new bonds.",
                );
            }
        }
        if let Some(first) = (0..6).find(|k| doc.atom(vertex(&ring, *k)).is_some()) {
            ring.rotate_left(first);
            if doc.atom(vertex(&ring, 1)).is_none()
                && doc.atom(vertex(&ring, 5)).is_some()
                && let Some(rest) = ring.get_mut(1..)
            {
                rest.reverse();
            }
        }
        let existing: Vec<_> = (0..6)
            .map(|k| find(&result, edge(&ring, k)).map(|b| b.order))
            .collect();
        // Shift asks for the other Kekule pattern, but only where it keeps every
        // double bond; around a Kekule fusion only one pattern fits the shared edge.
        let rotation = kekule_rotation(&existing);
        let plan = |rotation: usize| {
            let orders: Vec<_> = (0..6).map(|k| kekule(k + rotation)).collect();
            plan_orders(&result, &ring, &orders)
        };
        let doubles = |planned: &[u8]| planned.iter().filter(|o| **o == 2).count();
        let mut planned = plan(rotation);
        if self.alternate {
            let alternate = plan(rotation + 1);
            if doubles(&alternate) >= doubles(&planned) {
                planned = alternate;
            }
        }
        for (k, order) in planned.into_iter().enumerate() {
            let (a, b) = edge(&ring, k);
            if find(&result, (a, b)).is_none_or(|bond| bond.order != order) {
                result.add_bond(a, b, order, "plain");
            }
        }
        result.invalidate_chemistry(&ring);
        result.reconcile_molecule_groups();
        if crate::reactions::reconcile(&mut result).is_err() || result.validate().is_err() {
            return Err("The ring could not be inserted.");
        }
        Ok((result, ring))
    }
}

const FUSE_DISTANCE: f32 = 5. / 75.;

fn kekule(k: usize) -> u8 {
    if k.is_multiple_of(2) { 2 } else { 1 }
}
fn vertex(ring: &[u64], k: usize) -> u64 {
    ring.get(k % ring.len().max(1)).copied().unwrap_or_default()
}
fn edge(ring: &[u64], k: usize) -> (u64, u64) {
    (vertex(ring, k), vertex(ring, k + 1))
}
fn joins(bond: &crate::document::Bond, (a, b): (u64, u64)) -> bool {
    (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a)
}
fn find(doc: &Document, pair: (u64, u64)) -> Option<&crate::document::Bond> {
    doc.bonds.iter().find(|b| joins(b, pair))
}

fn kekule_rotation(existing: &[Option<u8>]) -> usize {
    let n = existing.len();
    let fused: Vec<(usize, u8)> = existing
        .iter()
        .enumerate()
        .filter_map(|(k, o)| o.map(|o| (k, o)))
        .collect();
    let mut best = (0, i32::MIN);
    match fused.as_slice() {
        [] => return 0,
        &[(k, order)] => {
            for rotation in 0..n {
                let mut score = 0;
                if matches!((order, kekule(k + rotation)), (1, 2) | (2, 1) | (2, 2)) {
                    score += 100;
                }
                for adjacent in [k + n - 1 + rotation, k + 1 + rotation] {
                    let adjacent = kekule(adjacent);
                    if (order == 1 && adjacent == 2) || (order == 2 && adjacent == 1) {
                        score += 50;
                    }
                }
                if score > best.1 {
                    best = (rotation, score);
                }
            }
        }
        _ => {
            for rotation in 0..n {
                let used: Vec<usize> = fused.iter().map(|(k, _)| (k + rotation) % n).collect();
                let mut score = 0;
                for &t in &used {
                    for adjacent in [(t + n - 1) % n, (t + 1) % n] {
                        if !used.contains(&adjacent) && kekule(adjacent) == 1 {
                            score += 5000;
                        }
                    }
                }
                for &(k, order) in &fused {
                    if kekule(k + rotation) == order {
                        score += if order == 2 { 1100 } else { 100 };
                    } else {
                        score -= 50;
                    }
                }
                if score > best.1 {
                    best = (rotation, score);
                }
            }
        }
    }
    best.0
}

fn plan_orders(doc: &Document, ring: &[u64], orders: &[u8]) -> Vec<u8> {
    use std::collections::HashMap;
    let edges: Vec<_> = (0..ring.len()).map(|k| edge(ring, k)).collect();
    let mut load: HashMap<u64, i64> = ring
        .iter()
        .map(|id| (*id, i64::from(templates::valence(doc, *id))))
        .collect();
    let fits = |load: &HashMap<u64, i64>, id: u64, extra: i64| {
        extra <= 0
            || doc.atom(id).is_none_or(|atom| {
                let capacity = i64::from(templates::capacity(atom));
                capacity == 0 || load.get(&id).copied().unwrap_or(0) + 2 * extra <= capacity
            })
    };
    let mut incoming: HashMap<u64, i64> = HashMap::new();
    for &(a, b) in edges.iter().filter(|pair| find(doc, **pair).is_none()) {
        *incoming.entry(a).or_default() += 1;
        *incoming.entry(b).or_default() += 1;
    }
    let mut planned: Vec<Option<u8>> = Vec::with_capacity(edges.len());
    for (&(a, b), &order) in edges.iter().zip(orders) {
        let Some(existing) = find(doc, (a, b)) else {
            planned.push(None);
            continue;
        };
        let extra = i64::from(order) - i64::from(existing.order);
        let other_double = |id: u64| {
            doc.bonds
                .iter()
                .any(|x| (x.a == id || x.b == id) && x.order == 2 && !std::ptr::eq(x, existing))
        };
        let overwrite = existing.order == 1
            && existing.display == "plain"
            && existing.stereo.is_none()
            && !other_double(a)
            && !other_double(b)
            && fits(&load, a, extra + incoming.get(&a).copied().unwrap_or(0))
            && fits(&load, b, extra + incoming.get(&b).copied().unwrap_or(0));
        if overwrite {
            *load.entry(a).or_default() += 2 * extra;
            *load.entry(b).or_default() += 2 * extra;
            planned.push(Some(order));
        } else {
            planned.push(Some(existing.order));
        }
    }
    for ((&(a, b), &order), slot) in edges.iter().zip(orders).zip(&mut planned) {
        if slot.is_some() {
            continue;
        }
        let mut order = order;
        while order > 1 && !(fits(&load, a, i64::from(order)) && fits(&load, b, i64::from(order))) {
            order -= 1;
        }
        *load.entry(a).or_default() += 2 * i64::from(order);
        *load.entry(b).or_default() += 2 * i64::from(order);
        *slot = Some(order);
    }
    planned.into_iter().map(|o| o.unwrap_or(1)).collect()
}

/// A selected simple cycle, excluding exocyclic bonds and nonchemical anchors.
pub fn selected_cycle(doc: &Document, selected: &[u64]) -> Option<Vec<u64>> {
    let mut ids: Vec<_> = selected
        .iter()
        .copied()
        .filter(|id| {
            doc.atom(*id)
                .is_some_and(|a| a.centroid.is_empty() && a.element != "*")
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    if !(3..=8).contains(&ids.len()) {
        return None;
    }
    let edges: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
        .collect();
    if edges.len() != ids.len()
        || edges.iter().any(|b| !matches!(b.order, 1 | 2 | 4))
        || ids
            .iter()
            .any(|id| edges.iter().filter(|b| b.a == *id || b.b == *id).count() != 2)
    {
        return None;
    }
    let mut reached = vec![*ids.first()?];
    let mut index = 0;
    while let Some(id) = reached.get(index).copied() {
        for b in &edges {
            if b.a == id || b.b == id {
                let next = if b.a == id { b.b } else { b.a };
                if !reached.contains(&next) {
                    reached.push(next);
                }
            }
        }
        index += 1;
    }
    (reached.len() == ids.len()).then_some(ids)
}
pub fn toggle_selected_aromatic(doc: &mut Document, selected: &[u64]) -> Result<bool, String> {
    let ids = selected_cycle(doc, selected).ok_or("Select one complete ring (3–8 atoms)")?;
    let aromatic = !doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
        .all(|b| b.order == 4);
    doc.invalidate_chemistry(&ids);
    for a in &mut doc.atoms {
        if ids.contains(&a.id) {
            a.aromatic = aromatic;
        }
    }
    for b in &mut doc.bonds {
        if ids.contains(&b.a) && ids.contains(&b.b) {
            b.order = if aromatic { 4 } else { 1 };
            b.display = "plain".into();
            b.projection = false;
            b.secondary_display = None;
            b.double_position = Default::default();
            b.stereo = None;
            b.stereo_atoms.clear();
        }
    }
    Ok(aromatic)
}
#[cfg(test)]
mod benzene_rotation_tests {
    use super::*;
    fn benzene() -> Drawing {
        Drawing {
            preset: Preset::Benzene,
            length: 42.,
            alternate: false,
            connect: false,
        }
    }
    fn fuse(doc: &Document, order: u8) -> (Document, Vec<u64>) {
        let bond = doc.bonds.iter().find(|b| b.order == order).unwrap();
        let (a, b) = (doc.atom(bond.a).unwrap(), doc.atom(bond.b).unwrap());
        let mid = Point::new(
            (a.position.x + b.position.x) / 2.,
            (a.position.y + b.position.y) / 2.,
        );
        benzene().place(doc, mid, None, 5.).unwrap()
    }
    fn doubles(doc: &Document, id: u64) -> usize {
        doc.bonds
            .iter()
            .filter(|b| (b.a == id || b.b == id) && b.order == 2)
            .count()
    }
    fn ring_orders(doc: &Document, ring: &[u64]) -> Vec<u8> {
        (0..6)
            .map(|k| {
                let (a, b) = (ring[k], ring[(k + 1) % 6]);
                doc.bonds
                    .iter()
                    .find(|x| (x.a == a && x.b == b) || (x.a == b && x.b == a))
                    .unwrap()
                    .order
            })
            .collect()
    }

    #[test]
    fn free_benzene_is_unchanged() {
        let (doc, ring) = benzene()
            .place(&Document::default(), Point::default(), None, 5.)
            .unwrap();
        assert_eq!(ring.len(), 6);
        assert_eq!(doc.bonds.iter().filter(|b| b.order == 2).count(), 3);
        assert!(ring.iter().all(|id| doubles(&doc, *id) == 1));
    }

    #[test]
    fn kekule_single_bond_fuses_into_naphthalene() {
        let base = Preset::Benzene.document(42., false);
        let (doc, ring) = fuse(&base, 1);
        assert_eq!(doc.atoms.len(), 10);
        assert_eq!(doc.bonds.len(), 11);
        assert!(doc.atoms.iter().all(|a| doubles(&doc, a.id) == 1));
        assert_eq!(ring_orders(&doc, &ring), vec![1, 1, 2, 1, 2, 1]);
    }

    #[test]
    fn kekule_double_bond_fuses_into_naphthalene() {
        let base = Preset::Benzene.document(42., false);
        let (doc, ring) = fuse(&base, 2);
        assert_eq!(doc.atoms.len(), 10);
        assert!(doc.atoms.iter().all(|a| doubles(&doc, a.id) == 1));
        assert_eq!(ring_orders(&doc, &ring), vec![2, 1, 2, 1, 2, 1]);
    }

    #[test]
    fn saturated_single_bond_is_upgraded() {
        let base = Preset::Regular.document(42., false);
        let (doc, ring) = fuse(&base, 1);
        assert_eq!(ring_orders(&doc, &ring), vec![2, 1, 2, 1, 2, 1]);
        assert_eq!(doc.bonds.iter().filter(|b| b.order == 2).count(), 3);
    }

    #[test]
    fn crowded_single_bond_lowers_new_doubles() {
        let mut base = Document::default();
        let a = base.add_atom("C", Point::new(0., 0.));
        let b = base.add_atom("C", Point::new(42., 0.));
        let c = base.add_atom("C", Point::new(-21., -36.4));
        let d = base.add_atom("C", Point::new(63., -36.4));
        base.add_bond(a, b, 1, "plain");
        base.add_bond(a, c, 2, "plain");
        base.add_bond(b, d, 2, "plain");
        let (doc, ring) = fuse(&base, 1);
        assert_eq!(ring_orders(&doc, &ring)[0], 1);
        assert!(doc.atoms.iter().all(|x| doubles(&doc, x.id) <= 1));
    }

    fn shifted() -> Drawing {
        Drawing {
            alternate: true,
            ..benzene()
        }
    }
    fn fuse_with(drawing: Drawing, doc: &Document, order: u8) -> (Document, Vec<u64>) {
        let bond = doc.bonds.iter().find(|b| b.order == order).unwrap();
        let (a, b) = (doc.atom(bond.a).unwrap(), doc.atom(bond.b).unwrap());
        let mid = Point::new(
            (a.position.x + b.position.x) / 2.,
            (a.position.y + b.position.y) / 2.,
        );
        drawing.place(doc, mid, None, 5.).unwrap()
    }

    #[test]
    fn shift_keeps_kekule_fusion_intact() {
        let base = Preset::Benzene.document(42., false);
        for order in [1, 2] {
            let (doc, _) = fuse_with(shifted(), &base, order);
            assert_eq!(doc.bonds.iter().filter(|b| b.order == 2).count(), 5);
            assert!(doc.atoms.iter().all(|a| doubles(&doc, a.id) == 1));
        }
    }

    #[test]
    fn shift_still_alternates_where_both_patterns_fit() {
        let base = Preset::Regular.document(42., false);
        let (doc, ring) = fuse_with(shifted(), &base, 1);
        assert_eq!(ring_orders(&doc, &ring), vec![1, 2, 1, 2, 1, 2]);
        let (doc, ring) = shifted()
            .place(&Document::default(), Point::default(), None, 5.)
            .unwrap();
        let (plain, plain_ring) = benzene()
            .place(&Document::default(), Point::default(), None, 5.)
            .unwrap();
        assert_ne!(ring_orders(&doc, &ring), ring_orders(&plain, &plain_ring));
    }

    /// A single bond a-b plus a carbon sitting on a vertex of the ring fused onto it,
    /// carrying `substituents` single bonds that point away from the ring.
    fn crowded_vertex(substituents: usize) -> Document {
        let mut base = Document::default();
        let a = base.add_atom("C", Point::new(0., 0.));
        let b = base.add_atom("C", Point::new(42., 0.));
        base.add_bond(a, b, 1, "plain");
        let x = base.add_atom("C", Point::new(0., 72.746));
        for (dx, dy) in [(-36.4, 21.), (36.4, 21.), (0., 42.)]
            .into_iter()
            .take(substituents)
        {
            let s = base.add_atom("C", Point::new(dx, 72.746 + dy));
            base.add_bond(x, s, 1, "plain");
        }
        base
    }
    /// Drags from the a-b bond toward the crowded carbon, forcing the ring onto its side.
    fn fuse_toward_vertex(base: &Document) -> Result<(Document, Vec<u64>), &'static str> {
        benzene().place(base, Point::new(21., 0.), Some(Point::new(21., 60.)), 5.)
    }

    #[test]
    fn nearby_atom_with_room_is_merged() {
        let base = crowded_vertex(1);
        let (doc, ring) = fuse_toward_vertex(&base).unwrap();
        assert_eq!(doc.atoms.len(), base.atoms.len() + 3);
        assert!(ring.contains(&3));
    }

    #[test]
    fn nearby_saturated_atom_is_rejected() {
        assert!(fuse_toward_vertex(&crowded_vertex(3)).is_err());
    }

    #[test]
    fn rotation_scoring() {
        assert_eq!(kekule_rotation(&[None; 6]), 0);
        assert_eq!(kekule_rotation(&[Some(1), None, None, None, None, None]), 0);
        assert_eq!(kekule_rotation(&[Some(2), None, None, None, None, None]), 0);
        assert_eq!(kekule_rotation(&[None, Some(2), None, None, None, None]), 1);
        assert_eq!(
            kekule_rotation(&[Some(1), Some(2), None, None, None, None]),
            1
        );
    }
}

#[cfg(test)]
mod aromatic_toggle_tests {
    use super::*;
    #[test]
    fn toggle_preserves_member_count_coordinates_substituents_and_charges() {
        for n in 3..=8 {
            let mut doc = Document::default();
            let ids = editing::ring(&mut doc, Point::default(), n, false, 5.);
            let atom = ids[0];
            let extra = doc.add_atom("O", Point::new(100., 0.));
            doc.add_bond(atom, extra, 1, "plain");
            doc.atom_mut(atom).unwrap().charge = -1;
            let positions: Vec<_> = doc
                .atoms
                .iter()
                .map(|a| (a.id, a.position, a.charge))
                .collect();
            let edges: Vec<_> = doc.bonds.iter().map(|b| (b.a, b.b)).collect();
            assert!(toggle_selected_aromatic(&mut doc, &ids).unwrap());
            assert_eq!(
                doc.bonds.iter().filter(|b| b.order == 4).count(),
                n as usize
            );
            assert!(!toggle_selected_aromatic(&mut doc, &ids).unwrap());
            assert!(doc.bonds.iter().all(|b| b.order == 1));
            assert_eq!(
                edges,
                doc.bonds.iter().map(|b| (b.a, b.b)).collect::<Vec<_>>()
            );
            assert_eq!(
                positions,
                doc.atoms
                    .iter()
                    .map(|a| (a.id, a.position, a.charge))
                    .collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn partial_fused_and_disconnected_selections_are_not_rewritten() {
        let mut doc = Document::default();
        let ids = editing::ring(&mut doc, Point::default(), 6, false, 5.);
        let before = doc.clone();
        assert!(toggle_selected_aromatic(&mut doc, &ids[..5]).is_err());
        assert_eq!(doc, before);
        doc.add_bond(ids[0], ids[3], 1, "plain");
        let before = doc.clone();
        assert!(toggle_selected_aromatic(&mut doc, &ids).is_err());
        assert_eq!(doc, before);
    }
}
