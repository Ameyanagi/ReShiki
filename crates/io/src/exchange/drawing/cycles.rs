//! A catalytic cycle's ChemDraw steps and the data ReShiki reopens it from.
use super::{P, Result, Writer, steps::Step};
use crate::{
    cycles::{Materialized, Role},
    document::Point,
    exchange::cycles::{Payload, Saved},
};

impl Writer<'_> {
    /// Each step of each cycle as a ChemDraw reaction step, and the cycles
    /// themselves as file data.
    pub(super) fn cycles(&mut self, cycles: &[Materialized]) -> Result<Option<Payload>> {
        if cycles.is_empty() {
            return Ok(None);
        }
        let mut payload = Payload::default();
        for m in cycles {
            let mut g = m.graphic.clone();
            let Some(cycle) = g.cycle.as_deref_mut() else {
                continue;
            };
            let roles: Vec<(Role, Vec<u64>)> =
                cycle.parts().map(|(r, ids)| (r, ids.clone())).collect();
            let texts: Vec<bool> = roles
                .iter()
                .map(|(_, ids)| {
                    !ids.first()
                        .is_some_and(|id| self.atom_indices.contains_key(id))
                })
                .collect();
            let (ids, exact): (Vec<u64>, Vec<bool>) = roles
                .iter()
                .map(|(_, part)| self.part_id(part))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .unzip();
            for (part, id) in cycle.parts_mut().zip(&ids) {
                *part = vec![*id];
            }
            let mut drawn = vec![];
            for (id, key) in &self.objects {
                if m.drawn.contains(id) {
                    drawn.push(self.xml_id(*key)?);
                }
            }
            // The steps ChemDraw's reaction tools read.
            let n = cycle.intermediates.len();
            let mut steps = Vec::new();
            for s in 0..n {
                let mut step = Step::default();
                for ((role, _), (id, text)) in roles.iter().zip(ids.iter().zip(&texts)) {
                    // A lone intermediate starts and ends its step.
                    if *role == Role::Intermediate(s) {
                        step.reactants.push(*id);
                    }
                    let side = match *role {
                        Role::Intermediate(i) if i == (s + 1) % n => &mut step.products,
                        Role::Reagent(k) | Role::Product(k) if k == s && *text => &mut step.above,
                        Role::Reagent(k) if k == s => &mut step.reactants,
                        Role::Product(k) if k == s => &mut step.products,
                        _ => continue,
                    };
                    side.push(*id);
                }
                for (i, arrow) in &m.arcs {
                    if *i == s
                        && let Some((_, key)) = self.objects.iter().find(|(id, _)| id == arrow)
                    {
                        step.arrows.push(self.xml_id(*key)?);
                    }
                }
                steps.push(step);
            }
            if !steps.is_empty() {
                self.scheme(&steps)?;
            }
            // File coordinates, as the parts' own.
            let at = |p: Point| {
                let p = P::from(p);
                Point::new(
                    (p.x * self.scale + self.shift.x) as f32,
                    (p.y * self.scale + self.shift.y) as f32,
                )
            };
            let axis = |p: Point| {
                Point::new(
                    (f64::from(p.x) * self.scale) as f32,
                    (f64::from(p.y) * self.scale) as f32,
                )
            };
            g.origin = at(g.origin);
            g.axis_x = axis(g.axis_x);
            g.axis_y = axis(g.axis_y);
            // Only a cycle whose parts are each exactly a file object of its
            // own comes back.
            let mut seen = std::collections::HashSet::new();
            if ids.iter().all(|id| seen.insert(*id)) && exact.iter().all(|e| *e) {
                payload.cycles.push(Saved { graphic: g, drawn });
            }
        }
        Ok((!payload.cycles.is_empty()).then_some(payload))
    }
}
