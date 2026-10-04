//! Explicit reaction membership, independent of where a scheme is drawn.
use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[cfg(test)]
mod copy_tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub atoms: Vec<u64>,
    #[serde(default = "one")]
    pub coefficient: u16,
}
fn one() -> u16 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reaction {
    pub arrow: u64,
    #[serde(default)]
    pub reactants: Vec<Participant>,
    #[serde(default)]
    pub products: Vec<Participant>,
    #[serde(default)]
    pub agents: Vec<Participant>,
    /// Captions and conditions selected with the reaction, not chemical participants.
    #[serde(default)]
    pub annotations: Vec<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Reactant,
    Product,
    Agent,
}
impl Role {
    pub const ALL: [Self; 3] = [Self::Reactant, Self::Product, Self::Agent];
    pub fn label(self) -> &'static str {
        match self {
            Self::Reactant => "Reactants",
            Self::Product => "Products",
            Self::Agent => "Reagents / catalysts",
        }
    }
}
impl Reaction {
    pub fn new(arrow: u64) -> Self {
        Self {
            arrow,
            reactants: vec![],
            products: vec![],
            agents: vec![],
            annotations: vec![],
        }
    }
    pub fn participants(&self, role: Role) -> &[Participant] {
        match role {
            Role::Reactant => &self.reactants,
            Role::Product => &self.products,
            Role::Agent => &self.agents,
        }
    }
    pub fn participants_mut(&mut self, role: Role) -> &mut Vec<Participant> {
        match role {
            Role::Reactant => &mut self.reactants,
            Role::Product => &mut self.products,
            Role::Agent => &mut self.agents,
        }
    }
    pub fn ids(&self) -> Vec<u64> {
        std::iter::once(self.arrow)
            .chain(self.annotations.iter().copied())
            .chain(Role::ALL.into_iter().flat_map(|role| {
                self.participants(role)
                    .iter()
                    .flat_map(|p| p.atoms.iter().copied())
            }))
            .collect()
    }
    pub fn ready(&self) -> bool {
        !self.reactants.is_empty() && !self.products.is_empty()
    }
    pub fn remap(&mut self, mapping: &HashMap<u64, u64>) -> Option<()> {
        self.arrow = *mapping.get(&self.arrow)?;
        for id in &mut self.annotations {
            *id = *mapping.get(id)?;
        }
        for role in Role::ALL {
            for participant in self.participants_mut(role) {
                for id in &mut participant.atoms {
                    *id = *mapping.get(id)?;
                }
            }
        }
        Some(())
    }
}

/// Includes complete connected molecules, including atoms hidden by abbreviations.
pub fn molecules(doc: &Document, selected: &[u64]) -> Vec<Vec<u64>> {
    if selected.is_empty() {
        return vec![];
    }
    let atoms: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    let mut remaining: HashSet<_> = selected
        .iter()
        .copied()
        .filter(|id| atoms.contains(id))
        .collect();
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for (a, b) in doc
        .bonds
        .iter()
        .filter(|b| b.order != 0)
        .map(|b| (b.a, b.b))
        .chain(
            doc.atoms
                .iter()
                .filter(|a| a.attachment.is_some())
                .flat_map(|a| a.centroid.iter().map(move |id| (a.id, *id))),
        )
    {
        adjacent.entry(a).or_default().push(b);
        adjacent.entry(b).or_default().push(a);
    }
    let mut result = Vec::new();
    for atom in &doc.atoms {
        if !remaining.contains(&atom.id) {
            continue;
        }
        let mut component = HashSet::from([atom.id]);
        let mut pending = vec![atom.id];
        while let Some(id) = pending.pop() {
            for other in adjacent.get(&id).into_iter().flatten() {
                if component.insert(*other) {
                    pending.push(*other);
                }
            }
        }
        remaining.retain(|id| !component.contains(id));
        let mut ids: Vec<_> = component.into_iter().collect();
        ids.sort_unstable();
        result.push(ids);
    }
    result
}

/// Reaction membership for chemical copy only; neither input is changed.
/// Original explicit roles take precedence, even when selection pruned them.
pub fn copy_reaction(
    source: &Document,
    snapshot: &Document,
) -> Result<Option<Reaction>, &'static str> {
    if snapshot.arrows.is_empty() {
        let selected: HashSet<_> = snapshot.atoms.iter().map(|a| a.id).collect();
        let mut participants = source
            .reactions
            .iter()
            .flat_map(|reaction| {
                Role::ALL
                    .into_iter()
                    .flat_map(move |role| reaction.participants(role))
            })
            .filter(|participant| participant.atoms.iter().any(|id| selected.contains(id)));
        if let Some(participant) = participants.next()
            && (participants.next().is_some()
                || participant.atoms.len() != selected.len()
                || participant.atoms.iter().any(|id| !selected.contains(id)))
        {
            return Err(
                "Select one complete reaction participant, or the complete reaction with its arrow.",
            );
        }
        return Ok(None);
    }
    let [arrow] = snapshot.arrows.as_slice() else {
        return Err("Select one reaction arrow before copying.");
    };
    source.validate().map_err(|_| "Invalid source drawing.")?;
    snapshot
        .validate()
        .map_err(|_| "Invalid copied selection.")?;
    if !source.arrows.iter().any(|a| a.id == arrow.id) {
        return Err("The copied arrow is missing from the source drawing.");
    }
    let atoms: HashMap<_, _> = snapshot.atoms.iter().map(|a| (a.id, a)).collect();
    let selected: Vec<_> = snapshot.atoms.iter().map(|a| a.id).collect();
    let parts = molecules(source, &selected);
    if parts.iter().map(Vec::len).sum::<usize>() != atoms.len()
        || parts.iter().flatten().any(|id| !atoms.contains_key(id))
    {
        return Err("Select complete molecules and the reaction arrow before copying.");
    }
    if let Some(reaction) = source.reactions.iter().find(|r| r.arrow == arrow.id) {
        let selected: HashSet<_> = snapshot.all_ids().into_iter().collect();
        if !reaction.ready() || reaction.ids().iter().any(|id| !selected.contains(id)) {
            return Err("Select the complete defined reaction before copying.");
        }
        if Role::ALL
            .into_iter()
            .flat_map(|role| reaction.participants(role))
            .map(|participant| participant.atoms.len())
            .sum::<usize>()
            != atoms.len()
        {
            return Err("Select only the defined reaction, including all its participants.");
        }
        return Ok(Some(reaction.clone()));
    }
    let style = arrow.appearance();
    if arrow.kind != "forward"
        || arrow.control.is_some()
        || style.head != crate::arrows::Head::Full
        || style.tail != crate::arrows::Head::None
        || style.no_go != crate::arrows::NoGo::None
        || style.dipole
    {
        return Err("Automatic reaction copy needs one straight forward arrow.");
    }
    let (dx, dy) = (
        f64::from(arrow.end.x) - f64::from(arrow.start.x),
        f64::from(arrow.end.y) - f64::from(arrow.start.y),
    );
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0. {
        return Err("The reaction arrow needs distinct start and end points.");
    }
    if source.reactions.iter().any(|reaction| {
        Role::ALL
            .into_iter()
            .flat_map(|role| reaction.participants(role))
            .any(|participant| participant.atoms.iter().any(|id| atoms.contains_key(id)))
    }) {
        return Err(
            "Copied molecules have roles on another arrow; select that complete reaction or unassigned molecules.",
        );
    }
    let (mx, my) = (
        (f64::from(arrow.start.x) + f64::from(arrow.end.x)) * 0.5,
        (f64::from(arrow.start.y) + f64::from(arrow.end.y)) * 0.5,
    );
    // Scale-relative tolerance for ties, independent of orientation and zoom.
    let tolerance = length_squared * 1e-6;
    let mut reaction = Reaction::new(arrow.id);
    for members in parts {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for id in &members {
            let atom = atoms
                .get(id)
                .ok_or("Select complete molecules and the reaction arrow before copying.")?;
            if snapshot.atom_visible(*id) {
                let side =
                    (f64::from(atom.position.x) - mx) * dx + (f64::from(atom.position.y) - my) * dy;
                lo = lo.min(side);
                hi = hi.max(side);
            }
        }
        let role = if !lo.is_finite() {
            return Err("A molecule has no visible position; assign reaction roles explicitly.");
        } else if hi < -tolerance {
            Role::Reactant
        } else if lo > tolerance {
            Role::Product
        } else {
            return Err(
                "A molecule overlaps the arrow midpoint; assign reaction roles explicitly.",
            );
        };
        reaction.participants_mut(role).push(Participant {
            atoms: members,
            coefficient: 1,
        });
    }
    if !reaction.ready() {
        return Err("Select starting materials at the arrow start and products at its end.");
    }
    reaction.annotations = snapshot.annotations.iter().map(|a| a.id).collect();
    Ok(Some(reaction))
}

pub fn assign(doc: &mut Document, arrow: u64, selected: &[u64], role: Role) -> Result<(), String> {
    if !doc.arrows.iter().any(|a| a.id == arrow) {
        return Err("Select a reaction arrow first".into());
    }
    let parts = molecules(doc, selected);
    if parts.is_empty() {
        return Err("Select a molecule on the canvas first".into());
    }
    let assigned: HashSet<_> = parts.iter().flatten().copied().collect();
    let mut reaction = doc
        .reactions
        .iter()
        .find(|r| r.arrow == arrow)
        .cloned()
        .unwrap_or_else(|| Reaction::new(arrow));
    // Moving a participant between roles keeps its coefficient and salt components.
    let mut moved = Vec::new();
    for old_role in Role::ALL {
        for participant in reaction.participants(old_role) {
            if participant.atoms.iter().any(|id| assigned.contains(id)) {
                moved.push(participant.clone());
            }
        }
        reaction
            .participants_mut(old_role)
            .retain(|p| !p.atoms.iter().any(|id| assigned.contains(id)));
    }
    for atoms in parts {
        if moved
            .iter()
            .any(|p| atoms.iter().any(|id| p.atoms.contains(id)))
        {
            continue;
        }
        moved.push(Participant {
            atoms,
            coefficient: 1,
        });
    }
    reaction.participants_mut(role).extend(moved);
    let mut candidate = doc.clone();
    candidate.reactions.retain(|r| r.arrow != arrow);
    candidate.reactions.push(reaction);
    reconcile(&mut candidate)?;
    candidate.version = 15;
    candidate.validate()?;
    *doc = candidate;
    Ok(())
}

pub fn prune(doc: &mut Document) {
    let atoms: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    let arrows: HashSet<_> = doc.arrows.iter().map(|a| a.id).collect();
    let annotations: HashSet<_> = doc.annotations.iter().map(|a| a.id).collect();
    doc.reactions.retain(|r| arrows.contains(&r.arrow));
    for reaction in &mut doc.reactions {
        reaction.annotations.retain(|id| annotations.contains(id));
        for role in Role::ALL {
            for participant in reaction.participants_mut(role) {
                participant.atoms.retain(|id| atoms.contains(id));
            }
            reaction
                .participants_mut(role)
                .retain(|p| !p.atoms.is_empty());
        }
    }
}

/// Grow membership with a molecule. Joining different participants must be explicit.
pub fn reconcile(doc: &mut Document) -> Result<(), String> {
    prune(doc);
    let mut reactions = doc.reactions.clone();
    for reaction in &mut reactions {
        let mut assigned = HashSet::new();
        for role in Role::ALL {
            for participant in reaction.participants_mut(role) {
                let mut atoms: Vec<_> = molecules(doc, &participant.atoms)
                    .into_iter()
                    .flatten()
                    .collect();
                atoms.sort_unstable();
                atoms.dedup();
                if atoms.iter().any(|id| !assigned.insert(*id)) {
                    return Err("This joins separate reaction participants. Clear their reaction roles before joining them.".into());
                }
                participant.atoms = atoms;
            }
        }
    }
    doc.reactions = reactions;
    Ok(())
}

pub fn validate(doc: &Document) -> Result<(), String> {
    let mut arrows = HashSet::new();
    for reaction in &doc.reactions {
        if !arrows.insert(reaction.arrow) || !doc.arrows.iter().any(|a| a.id == reaction.arrow) {
            return Err("Invalid or duplicate reaction arrow".into());
        }
        let mut notes = HashSet::new();
        if reaction
            .annotations
            .iter()
            .any(|id| !notes.insert(id) || !doc.annotations.iter().any(|a| a.id == *id))
        {
            return Err("Invalid reaction annotation".into());
        }
        let mut assigned = HashSet::new();
        for role in Role::ALL {
            for participant in reaction.participants(role) {
                if participant.atoms.is_empty()
                    || !(1..=99).contains(&participant.coefficient)
                    || participant
                        .atoms
                        .iter()
                        .any(|id| doc.atom(*id).is_none() || !assigned.insert(*id))
                {
                    return Err("Invalid, duplicate, or empty reaction participant".into());
                }
                if doc.bonds.iter().any(|b| {
                    b.order != 0
                        && participant.atoms.contains(&b.a) != participant.atoms.contains(&b.b)
                }) {
                    return Err("Assign complete molecules to reaction roles".into());
                }
            }
        }
    }
    Ok(())
}
