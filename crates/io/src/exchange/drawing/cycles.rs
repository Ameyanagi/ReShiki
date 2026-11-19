//! A catalytic cycle's ChemDraw steps and the data ReShiki reopens it from.
use super::{P, Result, Writer, invalid};
use crate::{
    cycles::{Materialized, Role},
    document::Point,
    exchange::cycles::{Payload, Saved},
};

impl Writer<'_> {
    /// The file object ID of a cycle part: the outermost fragment holding a
    /// structure's first atom, or a text's own text object. Also whether
    /// that object is the part exactly: a fragment can hold more, as when a
    /// bond deleted in a part left two pieces in two fragments.
    fn part_id(&self, part: &[u64]) -> Result<(u64, bool)> {
        let first = *part.first().ok_or_else(|| invalid("Empty cycle part"))?;
        let mut exact = true;
        let key = if self.atom_indices.contains_key(&first) {
            let mut key = self.atom_node(first)?;
            let mut fragment = None;
            while let Some(parent) = self.tree.node(key)?.parent {
                if self.tree.node(parent)?.tag == "fragment" {
                    fragment = Some(parent);
                }
                key = parent;
            }
            let fragment = fragment.ok_or_else(|| invalid("Cycle structure outside a fragment"))?;
            let mut held = std::collections::HashSet::new();
            for key in self.tree.descendants(fragment)? {
                if self.tree.node(key)?.tag == "n"
                    && let Some(atom) = self
                        .tree
                        .get(key, "id")?
                        .and_then(|id| id.parse::<usize>().ok())
                        .and_then(|id| self.doc.atoms.get(id.checked_sub(3)?))
                {
                    held.insert(atom.id);
                }
            }
            exact = held.len() == part.len() && part.iter().all(|id| held.contains(id));
            fragment
        } else {
            self.objects
                .iter()
                .find(|(id, _)| *id == first)
                .map(|(_, key)| *key)
                .ok_or_else(|| invalid("Missing cycle part"))?
        };
        Ok((self.xml_id(key)?, exact))
    }

    fn xml_id(&self, key: usize) -> Result<u64> {
        self.tree
            .value(key, "id")?
            .parse()
            .map_err(|_| invalid("Invalid object ID"))
    }

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
            if n > 0 {
                let scheme_id = self.id()?;
                let scheme = self
                    .tree
                    .add(Some(self.page), "scheme", [("id", scheme_id)])?;
                for s in 0..n {
                    let mut reactants = vec![];
                    let mut products = vec![];
                    let mut above = vec![];
                    for ((role, _), (id, text)) in roles.iter().zip(ids.iter().zip(&texts)) {
                        // A lone intermediate starts and ends its step.
                        if *role == Role::Intermediate(s) {
                            reactants.push(*id);
                        }
                        let side = match *role {
                            Role::Intermediate(i) if i == (s + 1) % n => &mut products,
                            Role::Reagent(k) | Role::Product(k) if k == s && *text => &mut above,
                            Role::Reagent(k) if k == s => &mut reactants,
                            Role::Product(k) if k == s => &mut products,
                            _ => continue,
                        };
                        side.push(*id);
                    }
                    let mut arrows = vec![];
                    for (step, arrow) in &m.arcs {
                        if *step == s
                            && let Some((_, key)) = self.objects.iter().find(|(id, _)| id == arrow)
                        {
                            arrows.push(self.xml_id(*key)?);
                        }
                    }
                    let list =
                        |ids: &[u64]| ids.iter().map(u64::to_string).collect::<Vec<_>>().join(" ");
                    let mut attrs = vec![("id", self.id()?)];
                    for (name, ids) in [
                        ("ReactionStepReactants", &reactants),
                        ("ReactionStepProducts", &products),
                        ("ReactionStepArrows", &arrows),
                        ("ReactionStepObjectsAboveArrow", &above),
                    ] {
                        if !ids.is_empty() {
                            attrs.push((name, list(ids)));
                        }
                    }
                    self.tree.add(Some(scheme), "step", attrs)?;
                }
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
