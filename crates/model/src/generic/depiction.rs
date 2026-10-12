//! Coordinate-only depiction of generated link and repeat components.
use super::{
    Work,
    budget::charge_ink_work,
    certify::preserve,
    errors::retained,
    geometry::{at, at_mut, finite, include},
    graph::IndexedGraph,
    ink::{measured_bounds, move_box, object_bounds, reach, saved_and_refreshed_boxes},
    probe::{self, Stage},
    references::reject_changed_references,
};
use crate::{
    chemistry::{
        depict,
        document::{Molecule, WORLD_PER_DEPICT_UNIT},
    },
    document::{Document, Point},
};
use std::collections::HashSet;

const BOND_LABEL_GAP_RATIO: f32 = 0.4;
const MIN_VERTEX_TURN: f32 = 0.15;

fn label_clearance(margin: f32) -> f32 {
    margin * 2. + 1.
}

pub(super) struct Plan {
    graph: IndexedGraph,
    components: Vec<Vec<usize>>,
    affected: Vec<bool>,
}

impl Plan {
    pub(super) fn new(
        doc: &Document,
        anchors: &HashSet<u64>,
        work: &mut Work,
    ) -> Result<Self, String> {
        let n = doc.atoms.len();
        charge_ink_work(doc, work)?;
        let graph = IndexedGraph::from_edges(
            doc,
            doc.bonds.iter().map(|bond| (bond.a, bond.b)),
            "Missing generated bond endpoint",
        )?;
        let (_, all_components) = graph.components()?;
        let mut affected = vec![false; n];
        let mut components = Vec::new();
        for component in all_components {
            if component
                .iter()
                .any(|&i| doc.atoms.get(i).is_some_and(|a| anchors.contains(&a.id)))
            {
                for &i in &component {
                    *at_mut(&mut affected, i)? = true;
                }
                components.push(component);
            }
        }
        // An entire moved component is guarded: a remote stereocenter is not
        // safe merely because it is several bonds from the repeat definition.
        let affected_ids: HashSet<_> = doc
            .atoms
            .iter()
            .zip(&affected)
            .filter(|(_, flag)| **flag)
            .map(|(a, _)| a.id)
            .collect();
        if doc
            .atoms
            .iter()
            .any(|a| affected_ids.contains(&a.id) && a.stereo.is_some())
            || doc
                .bonds
                .iter()
                .any(|b| affected_ids.contains(&b.a) && b.specifies_stereo())
        {
            return Err(stereo_error());
        }
        reject_changed_references(doc, &affected_ids, |_| Ok(()))?;
        Ok(Self {
            graph,
            components,
            affected,
        })
    }

    pub(super) fn finish(
        self,
        doc: &mut Document,
        molecule: &Molecule,
        work: &mut Work,
    ) -> Result<(Point, Point), String> {
        let n = doc.atoms.len();
        let state = &molecule.state;
        if molecule.ids.len() != n
            || molecule.positions.len() != n
            || state.graph.atoms.len() != n
            || state.valences.len() != n
            || state.metadata.atoms.len() != n
            || state.graph.bonds.len() != doc.bonds.len()
            || state.metadata.bonds.len() != doc.bonds.len()
            || molecule
                .ids
                .iter()
                .zip(&doc.atoms)
                .any(|(&id, a)| id != a.id)
        {
            return Err("Generated layout received mismatched chemical state".into());
        }
        if state
            .metadata
            .atoms
            .iter()
            .zip(&self.affected)
            .any(|(m, flag)| *flag && m.chiral_tag != 0)
        {
            return Err(stereo_error());
        }
        for (b, m) in state.graph.bonds.iter().zip(&state.metadata.bonds) {
            if *at(&self.affected, b.a)?
                && (m.stereo != 0 || m.unknown_stereo || !m.stereo_atoms.is_empty())
            {
                return Err(stereo_error());
            }
        }
        let before = doc.clone();
        if !self.components.is_empty() {
            work.charge(n.saturating_add(doc.bonds.len()).saturating_mul(8))?;
            let fixed: depict::geometry::Coordinates = doc
                .atoms
                .iter()
                .zip(&self.affected)
                .enumerate()
                .filter(|(_, (_, flag))| !**flag)
                .map(|(i, (a, _))| {
                    (
                        i,
                        depict::geometry::Point {
                            x: f64::from(a.position.x) / WORLD_PER_DEPICT_UNIT,
                            y: -f64::from(a.position.y) / WORLD_PER_DEPICT_UNIT,
                        },
                    )
                })
                .collect();
            let ranks = vec![None; n];
            probe::enter(Stage::Solver);
            let conformer = depict::compute_with_work(
                state,
                &ranks,
                (!fixed.is_empty()).then_some(&fixed),
                depict::Options {
                    bond_length: f64::from(doc.drawing_style.bond_length_world)
                        / WORLD_PER_DEPICT_UNIT,
                    canonical_orientation: true,
                    use_ring_templates: true,
                    work_limit: work.remaining,
                },
                &mut work.remaining,
            )
            .map_err(|e| format!("Generated structure layout failed: {e}"))?;
            if conformer.positions.len() != n {
                return Err("Generated layout lost atom coordinates".into());
            }
            for (i, p) in conformer.positions.iter().enumerate() {
                if *at(&self.affected, i)? {
                    let position = Point::new(
                        (p.x * WORLD_PER_DEPICT_UNIT) as f32,
                        (-p.y * WORLD_PER_DEPICT_UNIT) as f32,
                    );
                    finite(position)?;
                    at_mut(&mut doc.atoms, i)?.position = position;
                }
            }
        }
        // Hydrogen refresh is measurement-only. Saved chemical fields and
        // display/cache preferences remain exactly those of the generated clone.
        let mut preview = doc.clone();
        for (i, a) in preview.atoms.iter_mut().enumerate() {
            a.label_h = u32::from(at(&state.graph.atoms, i)?.explicit_hydrogens)
                .checked_add(at(&state.valences, i)?.implicit_hydrogens)
                .ok_or("Generated hydrogen label overflow")?;
        }
        probe::enter(Stage::Measure);
        let mut boxes = saved_and_refreshed_boxes(doc, &preview)?;
        let margin = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
        let bond_length = doc.drawing_style.bond_length_world;
        let gap = (bond_length * BOND_LABEL_GAP_RATIO).max(label_clearance(margin));
        let edges: HashSet<_> = doc
            .bonds
            .iter()
            .map(|b| {
                Ok(super::pair(
                    *self
                        .graph
                        .indices
                        .get(&b.a)
                        .ok_or("Missing generated endpoint")? as u64,
                    *self
                        .graph
                        .indices
                        .get(&b.b)
                        .ok_or("Missing generated endpoint")? as u64,
                ))
            })
            .collect::<Result<_, String>>()?;
        for component in &self.components {
            let mut scale = 1_f32;
            for (k, &i) in component.iter().enumerate() {
                for &j in component.iter().skip(k + 1) {
                    let p = at(&doc.atoms, i)?.position;
                    let q = at(&doc.atoms, j)?.position;
                    let distance = p.distance(q);
                    if !distance.is_finite() || distance < 0.01 {
                        return Err("Generated layout has coincident atoms".into());
                    }
                    scale = scale.max(
                        required_clearance(doc, &boxes, &edges, (i, j), distance, gap, margin)?
                            / distance,
                    );
                }
            }
            if !scale.is_finite() {
                return Err("Generated label clearance is nonfinite".into());
            }
            // One uniform enlargement keeps every chain angle/ring closure and
            // every forced or automatic H direction. No retry/cleanup loop.
            let center = at(
                &doc.atoms,
                *component.first().ok_or("Empty generated component")?,
            )?
            .position;
            for &i in component {
                let old = at(&doc.atoms, i)?.position;
                let new = Point::new(
                    center.x + (old.x - center.x) * scale,
                    center.y + (old.y - center.y) * scale,
                );
                finite(new)?;
                move_box(at_mut(&mut boxes, i)?, old, new);
                at_mut(&mut doc.atoms, i)?.position = new;
            }
        }
        // Fixed disconnected molecules/annotations stay put. Place each moved
        // component after their actual label/object extents, then after the
        // previous moved component; coordinate-only translation is bounded.
        let mut occupied = object_bounds(doc)?;
        for (i, a) in doc.atoms.iter().enumerate() {
            if !*at(&self.affected, i)? {
                include(&mut occupied, a.position, a.position);
                if let Some((lo, hi)) = *at(&boxes, i)? {
                    include(&mut occupied, lo, hi);
                }
            }
        }
        for component in &self.components {
            let mut bounds = None;
            for &i in component {
                let p = at(&doc.atoms, i)?.position;
                include(&mut bounds, p, p);
                if let Some((lo, hi)) = *at(&boxes, i)? {
                    include(&mut bounds, lo, hi);
                }
            }
            let (lo, hi) = bounds.ok_or("Generated layout has an empty component")?;
            let shift = occupied.map_or(Point::default(), |(_, end)| {
                Point::new(end.x + bond_length - lo.x, 0.)
            });
            for &i in component {
                let old = at(&doc.atoms, i)?.position;
                let new = old.offset(shift.x, shift.y);
                finite(new)?;
                move_box(at_mut(&mut boxes, i)?, old, new);
                at_mut(&mut doc.atoms, i)?.position = new;
            }
            include(
                &mut occupied,
                lo.offset(shift.x, shift.y),
                hi.offset(shift.x, shift.y),
            );
        }
        for component in &self.components {
            for (k, &i) in component.iter().enumerate() {
                for &j in component.iter().skip(k + 1) {
                    let p = at(&doc.atoms, i)?.position;
                    let q = at(&doc.atoms, j)?.position;
                    let distance = p.distance(q);
                    let required =
                        required_clearance(doc, &boxes, &edges, (i, j), distance, gap, margin)?;
                    if distance + 0.01 < required {
                        return Err(
                            "Generated layout cannot clear the preserved atom labels".into()
                        );
                    }
                }
            }
        }
        // Ordinary hidden saturated carbons need a visible vertex. A solver
        // failure may not turn a multi-carbon repeat back into one long line.
        for (i, a) in doc.atoms.iter().enumerate() {
            let neighbors = at(&self.graph.adjacent, i)?;
            if !*at(&self.affected, i)?
                || a.element != "C"
                || neighbors.len() != 2
                || crate::atom_labels::visible_with_degree(a, doc, || neighbors.len())
                || doc
                    .bonds
                    .iter()
                    .any(|b| (b.a == a.id || b.b == a.id) && b.order != 1)
            {
                continue;
            }
            let p = at(&doc.atoms, *at(neighbors, 0)?)?.position;
            let q = at(&doc.atoms, *at(neighbors, 1)?)?.position;
            let ux = p.x - a.position.x;
            let uy = p.y - a.position.y;
            let vx = q.x - a.position.x;
            let vy = q.y - a.position.y;
            let turn = (ux * vy - uy * vx).abs() / (ux.hypot(uy) * vx.hypot(vy));
            if !turn.is_finite() || turn < MIN_VERTEX_TURN {
                return Err("Generated layout hides a saturated carbon vertex".into());
            }
        }
        preserve(&before, doc, &self.affected)?;
        measured_bounds(doc, &boxes)
    }
}

fn required_clearance(
    doc: &Document,
    boxes: &[Option<(Point, Point)>],
    edges: &HashSet<(u64, u64)>,
    (i, j): (usize, usize),
    distance: f32,
    gap: f32,
    margin: f32,
) -> Result<f32, String> {
    let p = at(&doc.atoms, i)?.position;
    let q = at(&doc.atoms, j)?.position;
    let ux = (q.x - p.x) / distance;
    let uy = (q.y - p.y) / distance;
    Ok(reach(p, *at(boxes, i)?, ux, uy)
        + reach(q, *at(boxes, j)?, -ux, -uy)
        + if edges.contains(&super::pair(i as u64, j as u64)) {
            gap
        } else {
            label_clearance(margin)
        })
}

fn stereo_error() -> String {
    retained(
        "Generated link/repeat layout involving specified or perceived component stereochemistry is unsupported",
    )
}

#[cfg(test)]
mod tests;
