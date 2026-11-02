//! Informational checks of arrow meaning; neither chemistry nor drawing is edited.
use super::{EndSides, molecules};
use crate::{
    chemistry,
    document::{Document, Point},
    editing,
};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowSemanticsHintKind {
    NotContributingStructures,
    ContributingStructures,
}
impl ArrowSemanticsHintKind {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotContributingStructures => {
                "Not contributing structures (atoms or H move) — use ⇌ for tautomers"
            }
            Self::ContributingStructures => "These look like contributing structures — use ↔",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrowSemanticsHint {
    pub arrow: u64,
    pub kind: ArrowSemanticsHintKind,
}

/// Check resonance/equilibrium arrows with one complete molecule per side in their row.
/// Ambiguous placement, incomplete chemistry and multi-molecule sides stay silent.
/// This is a connectivity/composition heuristic, not a claim of interconversion.
pub fn arrow_semantics_hints(doc: &Document) -> Vec<ArrowSemanticsHint> {
    if doc.validate().is_err() {
        return vec![];
    }
    let ids: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
    let parts = molecules(doc, &ids);
    let positions: HashMap<_, _> = doc
        .atoms
        .iter()
        .filter(|atom| doc.atom_visible(atom.id))
        .map(|atom| (atom.id, atom.position))
        .collect();
    let mut signatures = HashMap::new();
    let mut hints = Vec::new();
    for arrow in &doc.arrows {
        if !matches!(arrow.kind.as_str(), "resonance" | "equilibrium") {
            continue;
        }
        let Ok(sides) = EndSides::new(arrow) else {
            continue;
        };
        // Along and across the arrow from its midpoint, scaled by its squared length.
        let length_squared = sides.dx * sides.dx + sides.dy * sides.dy;
        let project = |point: Point| {
            let (x, y) = (f64::from(point.x) - sides.mx, f64::from(point.y) - sides.my);
            (
                (x * sides.dx + y * sides.dy) / length_squared,
                (y * sides.dx - x * sides.dy) / length_squared,
            )
        };
        // A scheme row is the band half an arrow length either side of the arrow.
        // The next arrow in the row ends this arrow's sides (A ↔ B ↔ C).
        let (mut first, mut last) = (f64::NEG_INFINITY, f64::INFINITY);
        for other in doc.arrows.iter().filter(|other| other.id != arrow.id) {
            let (along, across) = project(Point::new(
                (other.start.x + other.end.x) * 0.5,
                (other.start.y + other.end.y) * 0.5,
            ));
            if across.abs() <= 0.5 {
                if along < 0. {
                    first = first.max(along);
                } else {
                    last = last.min(along);
                }
            }
        }
        let tolerance = sides.tolerance / length_squared;
        let mut ends = [Vec::new(), Vec::new()];
        let mut ambiguous = false;
        for (index, members) in parts.iter().enumerate() {
            let (mut lo, mut hi, mut below, mut above) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            for &point in members.iter().filter_map(|id| positions.get(id)) {
                let (along, across) = project(point);
                (lo, hi) = (lo.min(along), hi.max(along));
                (below, above) = (below.min(across), above.max(across));
            }
            if below > 0.5 || above < -0.5 || hi < first || lo > last {
                continue;
            }
            if hi < -tolerance {
                ends[0].push(index);
            } else if lo > tolerance {
                ends[1].push(index);
            } else {
                ambiguous = true;
            }
        }
        let ([left], [right]) = (ends[0].as_slice(), ends[1].as_slice()) else {
            continue;
        };
        if ambiguous {
            continue;
        }
        for &index in [left, right] {
            signatures.entry(index).or_insert_with(|| {
                parts
                    .get(index)
                    .and_then(|members| Signature::from_document(doc, members))
            });
        }
        let (Some(Some(left)), Some(Some(right))) = (signatures.get(left), signatures.get(right))
        else {
            continue;
        };
        let contributing = left.isomorphic(right, false);
        let kind = match arrow.kind.as_str() {
            "resonance" if !contributing => ArrowSemanticsHintKind::NotContributingStructures,
            "equilibrium" if contributing && !left.isomorphic(right, true) => {
                ArrowSemanticsHintKind::ContributingStructures
            }
            _ => continue,
        };
        hints.push(ArrowSemanticsHint {
            arrow: arrow.id,
            kind,
        });
    }
    hints
}

#[derive(Debug, Clone)]
struct Atom {
    number: u8,
    isotope: u16,
    /// Attached H atoms and count labels are equivalent; retain their isotopes.
    hydrogens: Vec<u16>,
    charge: i8,
    radicals: u8,
}
#[derive(Debug)]
struct Signature {
    atoms: Vec<Atom>,
    neighbors: Vec<Vec<(usize, u8)>>,
    charge: i64,
    radicals: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Label {
    number: u8,
    isotope: u16,
    hydrogens: Vec<u16>,
    degree: usize,
    charge: i8,
    radicals: u8,
}
impl Signature {
    // Graph endpoints are validated by drawing_graph; prepare uses the same
    // build. Retained indices are built from these same arrays.
    #[allow(clippy::indexing_slicing)]
    fn from_document(doc: &Document, members: &[u64]) -> Option<Self> {
        let members: HashSet<_> = members.iter().copied().collect();
        if doc
            .abbreviations
            .iter()
            .any(|group| group.members.iter().any(|id| members.contains(id)))
        {
            return None;
        }
        let part = editing::selection(doc, &members.iter().copied().collect::<Vec<_>>());
        if part.atoms.len() != members.len()
            || part
                .bonds
                .iter()
                .any(|bond| !matches!(bond.order, 1..=4) || bond.distance_derived)
        {
            return None;
        }
        let prepared = chemistry::document::prepare(&part).ok()?;
        // Keep the drawn electronic arrangement: sanitization may normalize it.
        // Both graphs come from the same build, so atom indices agree.
        let graph = chemistry::document::drawing_graph(&part).ok()?;
        if graph.atoms.iter().any(|atom| atom.atomic_number == 0) {
            return None;
        }
        let mut neighbors = vec![Vec::new(); graph.atoms.len()];
        for bond in &graph.bonds {
            neighbors[bond.a].push((bond.b, bond.order));
            neighbors[bond.b].push((bond.a, bond.order));
        }
        let folded: Vec<_> = graph
            .atoms
            .iter()
            .enumerate()
            .map(|(i, atom)| {
                atom.atomic_number == 1
                    && atom.charge == 0
                    && prepared.state.graph.atoms[i].radical_electrons == 0
                    && atom.explicit_hydrogens == 0
                    && prepared.state.valences[i].implicit_hydrogens == 0
                    && matches!(neighbors[i].as_slice(), [(other, 1)] if graph.atoms[*other].atomic_number != 1)
            })
            .collect();
        let mut atoms = Vec::new();
        let mut indices = vec![None; graph.atoms.len()];
        for (i, atom) in graph.atoms.iter().enumerate() {
            if folded[i] {
                continue;
            }
            let count = u32::from(prepared.state.graph.atoms[i].explicit_hydrogens)
                + prepared.state.valences[i].implicit_hydrogens;
            let mut hydrogens = vec![0; count as usize];
            for &(other, _) in &neighbors[i] {
                if folded[other] {
                    hydrogens.push(graph.atoms[other].isotope);
                }
            }
            hydrogens.sort_unstable();
            indices[i] = Some(atoms.len());
            atoms.push(Atom {
                number: atom.atomic_number,
                isotope: atom.isotope,
                hydrogens,
                charge: atom.charge,
                radicals: prepared.state.graph.atoms[i].radical_electrons,
            });
        }
        let mut adjacency = vec![Vec::new(); atoms.len()];
        for bond in &graph.bonds {
            if let (Some(a), Some(b)) = (indices[bond.a], indices[bond.b]) {
                adjacency[a].push((b, bond.order));
                adjacency[b].push((a, bond.order));
            }
        }
        Some(Self {
            atoms,
            neighbors: adjacency,
            charge: graph.atoms.iter().map(|atom| i64::from(atom.charge)).sum(),
            radicals: prepared
                .state
                .graph
                .atoms
                .iter()
                .map(|atom| u64::from(atom.radical_electrons))
                .sum(),
        })
    }

    fn labels(&self, exact: bool) -> Vec<Label> {
        self.atoms
            .iter()
            .zip(&self.neighbors)
            .map(|(atom, neighbors)| Label {
                number: atom.number,
                isotope: atom.isotope,
                hydrogens: atom.hydrogens.clone(),
                degree: neighbors.len(),
                charge: if exact { atom.charge } else { 0 },
                radicals: if exact { atom.radicals } else { 0 },
            })
            .collect()
    }

    /// A full bijection, including alternate permutations of indistinguishable atoms.
    /// Iterative traversal avoids a call-stack limit on large connected molecules.
    // Signature adjacency contains validated dense indices. Labels and working
    // arrays share those sizes; equal label classes are checked before traversal.
    #[allow(clippy::indexing_slicing)]
    fn isomorphic(&self, other: &Self, exact: bool) -> bool {
        if self.atoms.len() != other.atoms.len()
            || self.charge != other.charge
            || self.radicals != other.radicals
        {
            return false;
        }
        let left = self.labels(exact);
        let right = other.labels(exact);
        let mut classes = HashMap::<_, Vec<_>>::new();
        for (i, label) in right.iter().enumerate() {
            classes.entry(label).or_default().push(i);
        }
        let mut counts = HashMap::<_, usize>::new();
        for label in &left {
            *counts.entry(label).or_default() += 1;
        }
        if counts.len() != classes.len()
            || counts
                .iter()
                .any(|(label, count)| classes.get(label).is_none_or(|items| items.len() != *count))
        {
            return false;
        }
        let Some(root) = (0..left.len()).min_by_key(|&i| {
            (
                classes[&left[i]].len(),
                std::cmp::Reverse(self.neighbors[i].len()),
            )
        }) else {
            return true;
        };
        // Every later vertex has an earlier neighbor, keeping candidate sets local.
        let mut order = Vec::new();
        let mut seen = vec![false; left.len()];
        let mut pending = VecDeque::from([root]);
        seen[root] = true;
        while let Some(a) = pending.pop_front() {
            order.push(a);
            let mut next: Vec<_> = self.neighbors[a].iter().map(|&(b, _)| b).collect();
            next.sort_unstable_by_key(|&b| {
                (
                    classes[&left[b]].len(),
                    std::cmp::Reverse(self.neighbors[b].len()),
                )
            });
            for b in next {
                if !seen[b] {
                    seen[b] = true;
                    pending.push_back(b);
                }
            }
        }
        if order.len() != left.len() {
            return false;
        }
        let mut mapping = vec![None; left.len()];
        let mut used = vec![false; right.len()];
        let mut choices = vec![classes[&left[root]].clone().into_iter()];
        while !choices.is_empty() {
            let depth = choices.len() - 1;
            let a = order[depth];
            if let Some(b) = mapping[a].take() {
                used[b] = false;
            }
            let found = choices[depth].find(|&b| {
                !used[b]
                    && left[a] == right[b]
                    && self.neighbors[a].iter().all(|&(neighbor, order)| {
                        mapping[neighbor].is_none_or(|mapped| {
                            other.neighbors[b].iter().any(|&(target, target_order)| {
                                mapped == target && (!exact || order == target_order)
                            })
                        })
                    })
            });
            let Some(b) = found else {
                choices.pop();
                continue;
            };
            mapping[a] = Some(b);
            used[b] = true;
            if depth + 1 == order.len() {
                return true;
            }
            let next = order[depth + 1];
            let neighbor = self.neighbors[next]
                .iter()
                .filter_map(|&(neighbor, _)| mapping[neighbor])
                .min_by_key(|&mapped| other.neighbors[mapped].len());
            let Some(neighbor) = neighbor else {
                return false;
            };
            choices.push(
                other.neighbors[neighbor]
                    .iter()
                    .map(|&(candidate, _)| candidate)
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
        }
        false
    }
}

#[cfg(test)]
mod tests;
