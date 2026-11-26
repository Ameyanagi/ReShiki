//! Derive transition-state search inputs from a reaction's drawing and atom map.
use super::{Participant, Role, hints::attached_hydrogens, molecules};
use crate::{
    chemistry::document as chemistry,
    document::{Atom, Document},
    editing, reaction_mapping,
};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Maximum number of atoms in the search, including hydrogens.
pub const MAX_TS_ATOMS: usize = reshiki_qcdata::MAX_TS_ATOMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Reactant,
    Product,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsPlan {
    /// The single-molecule side. For a 1→1 reaction, start with the reactant.
    pub start: Side,
    /// All drawing atom IDs of the molecule used to generate the 3D model.
    pub start_atoms: Vec<u64>,
    /// Map numbers in use, sorted.
    pub mapped: Vec<u32>,
    /// Start-side drawing IDs of all mapped hydrogens, in map order.
    pub mapped_hydrogens: Vec<u64>,
    /// Bonds formed in the search direction, by start-side drawing atom IDs.
    pub form: Vec<(u64, u64)>,
    /// Bonds broken in the search direction, by start-side drawing atom IDs.
    pub break_: Vec<(u64, u64)>,
    /// Start-side (donor, acceptor) drawing IDs, one pair per hydrogen moved.
    pub hydrogen_moves: Vec<(u64, u64)>,
    /// Number of reactant and product molecules, respectively.
    pub shape: (usize, usize),
    /// Drawn side charges, kept for comparison after a charge override.
    pub drawn_charges: (i64, i64),
    /// Total atoms on the start side, including hydrogens.
    pub atoms: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TsPlanError {
    #[error("Assign reactants and products first")]
    NotReaction,
    #[error("Transition-state search handles 1→1, 1→2 and 2→1 reactions")]
    Shape { reactants: usize, products: usize },
    #[error("Atom map: {mapped} of {heavy} heavy atoms. Map every heavy atom first.")]
    Unmapped { mapped: usize, heavy: usize },
    #[error("Map drawn hydrogens that are not terminal on a heavy atom")]
    UnmappedHydrogen,
    #[error("Map every isotope-labelled hydrogen before searching")]
    UnmappedIsotopeHydrogen,
    #[error("{0}")]
    Mismatch(String),
    #[error("Total hydrogen counts differ between reactants and products")]
    HydrogenBalance,
    #[error(
        "Transition-state search handles up to {MAX_TS_ATOMS} atoms (this molecule has {atoms})"
    )]
    TooLarge { atoms: usize },
    #[error("The atom map shows no bonds forming or breaking and no hydrogen moves")]
    NoChange,
    #[error("{0}")]
    Unsupported(String),
}

/// Changes in the arrow direction, identified by reactant-side drawing IDs.
pub struct MapReview {
    pub form: Vec<(u64, u64)>,
    pub break_: Vec<(u64, u64)>,
    pub hydrogen_moves: Vec<(u64, u64)>,
}

/// Read a drawing's atom correspondence regardless of its reaction shape.
pub fn map_review(doc: &Document, arrow: u64) -> Result<MapReview, TsPlanError> {
    let reaction = reaction_mapping::reaction(doc, arrow).map_err(|_| TsPlanError::NotReaction)?;
    if !reaction.ready() {
        return Err(TsPlanError::NotReaction);
    }
    let drawing = |role| {
        let ids: Vec<_> = side_molecules(doc, reaction.participants(role))?
            .into_iter()
            .flatten()
            .collect();
        side_drawing(doc, &ids)
    };
    let reactants = drawing(Role::Reactant)?;
    let products = drawing(Role::Product)?;
    let from = MappedSide::new(&reactants)?;
    let to = MappedSide::new(&products)?;
    compatible_maps(&from, &to)?;
    mapped_changes(&from, &to)
}

/// Read the explicitly assigned reaction and plan changes away from its
/// single-molecule side. Agents and bond-order changes do not enter the plan.
pub fn plan(doc: &Document, arrow: u64) -> Result<TsPlan, TsPlanError> {
    plan_inner(doc, arrow, true)
}

/// Plan with the drawing's atom identities and bonds. The caller must check
/// both sides' reviewed electronic states before starting the calculation.
pub fn plan_for_review(doc: &Document, arrow: u64) -> Result<TsPlan, TsPlanError> {
    plan_inner(doc, arrow, false)
}

fn plan_inner(doc: &Document, arrow: u64, check_drawn_charge: bool) -> Result<TsPlan, TsPlanError> {
    let reaction = reaction_mapping::reaction(doc, arrow).map_err(|_| TsPlanError::NotReaction)?;
    if !reaction.ready() {
        return Err(TsPlanError::NotReaction);
    }
    let reactants = reaction.participants(Role::Reactant);
    let products = reaction.participants(Role::Product);
    let reactant_molecules = side_molecules(doc, reactants)?;
    let product_molecules = side_molecules(doc, products)?;
    let shape = (reactant_molecules.len(), product_molecules.len());
    let start = match shape {
        (1, 1 | 2) => Side::Reactant,
        (2, 1) => Side::Product,
        (reactants, products) => {
            return Err(TsPlanError::Shape {
                reactants,
                products,
            });
        }
    };
    if reactants.iter().chain(products).any(|p| p.coefficient != 1) {
        return Err(TsPlanError::Unsupported(
            "Expand reaction coefficients before searching for a transition state".into(),
        ));
    }
    let reactant_ids: Vec<_> = reactant_molecules.into_iter().flatten().collect();
    let product_ids: Vec<_> = product_molecules.into_iter().flatten().collect();
    let (start_atoms, other_atoms) = match start {
        Side::Reactant => (reactant_ids, product_ids),
        Side::Product => (product_ids, reactant_ids),
    };
    let start_drawing = side_drawing(doc, &start_atoms)?;
    let other_drawing = side_drawing(doc, &other_atoms)?;
    let from = MappedSide::new(&start_drawing)?;
    let to = MappedSide::new(&other_drawing)?;
    compatible_maps(&from, &to)?;
    if check_drawn_charge && from.charge != to.charge {
        return Err(TsPlanError::Mismatch(
            "Total charge differs between reactants and products".into(),
        ));
    }

    let atoms = from.atoms.len() + from.atoms.values().map(|a| a.hydrogens).sum::<usize>();
    if atoms > MAX_TS_ATOMS {
        return Err(TsPlanError::TooLarge { atoms });
    }
    let MapReview {
        form,
        break_,
        hydrogen_moves,
    } = mapped_changes(&from, &to)?;
    if form.is_empty() && break_.is_empty() && hydrogen_moves.is_empty() {
        return Err(TsPlanError::NoChange);
    }
    Ok(TsPlan {
        start,
        start_atoms,
        mapped: from.atoms.keys().copied().collect(),
        mapped_hydrogens: from
            .atoms
            .values()
            .filter(|a| a.drawing.element == "H")
            .map(|a| a.drawing.id)
            .collect(),
        form,
        break_,
        hydrogen_moves,
        shape,
        drawn_charges: match start {
            Side::Reactant => (from.charge, to.charge),
            Side::Product => (to.charge, from.charge),
        },
        atoms,
    })
}

fn compatible_maps(from: &MappedSide<'_>, to: &MappedSide<'_>) -> Result<(), TsPlanError> {
    let missing = from.unmapped + to.unmapped;
    if missing > 0 {
        let mapped = from.atoms.len() + to.atoms.len();
        return Err(TsPlanError::Unmapped {
            mapped,
            heavy: mapped + missing,
        });
    }
    for (&map, atom) in &from.atoms {
        let other = to.atom(map)?;
        if atom.drawing.element != other.drawing.element
            || atom.drawing.isotope != other.drawing.isotope
        {
            return Err(TsPlanError::Mismatch(format!(
                "Atom map {map} has different elements or isotopes on the two sides"
            )));
        }
    }
    for &map in to.atoms.keys() {
        from.atom(map)?;
    }
    Ok(())
}

fn mapped_changes(from: &MappedSide<'_>, to: &MappedSide<'_>) -> Result<MapReview, TsPlanError> {
    Ok(MapReview {
        form: from.drawing_bonds(to.bonds.difference(&from.bonds))?,
        break_: from.drawing_bonds(from.bonds.difference(&to.bonds))?,
        hydrogen_moves: hydrogen_moves(from, to)?,
    })
}

fn side_molecules(
    doc: &Document,
    participants: &[Participant],
) -> Result<Vec<Vec<u64>>, TsPlanError> {
    let ids: Vec<_> = participants
        .iter()
        .flat_map(|p| p.atoms.iter().copied())
        .collect();
    let selected: HashSet<_> = ids.iter().copied().collect();
    if !super::whole_molecules(doc, &selected) {
        return Err(TsPlanError::Unsupported(
            "Assign complete molecules to reaction roles".into(),
        ));
    }
    // A participant can hold several disconnected components, e.g. a salt.
    let parts = molecules(doc, &ids);
    if parts.iter().flatten().count() != ids.len() {
        return Err(TsPlanError::Unsupported(
            "Invalid reaction atom assignment".into(),
        ));
    }
    Ok(parts)
}

fn side_drawing(doc: &Document, ids: &[u64]) -> Result<Document, TsPlanError> {
    let selected: HashSet<_> = ids.iter().copied().collect();
    if doc
        .abbreviations
        .iter()
        .any(|group| group.members.iter().any(|id| selected.contains(id)))
    {
        return Err(TsPlanError::Unsupported(
            "Expand abbreviations and superatoms before searching for a transition state".into(),
        ));
    }
    // Check contacts before extracting the side, where links to other
    // components are lost. TS bond changes use ordinary bonds only.
    if doc.bonds.iter().any(|bond| {
        (selected.contains(&bond.a) || selected.contains(&bond.b)) && !matches!(bond.order, 1..=4)
    }) {
        return Err(TsPlanError::Unsupported(
            "Transition-state search needs ordinary covalent or aromatic bonds".into(),
        ));
    }
    let part = editing::selection(doc, ids);
    if crate::generic::present(&part)
        || part
            .atoms
            .iter()
            .any(|a| a.element == "*" || a.display.variable.is_some())
    {
        return Err(TsPlanError::Unsupported(
            "Replace query atoms and R-groups with concrete atoms first".into(),
        ));
    }
    Ok(part)
}

struct MappedAtom<'a> {
    drawing: &'a Atom,
    hydrogens: usize,
}

struct MappedSide<'a> {
    atoms: BTreeMap<u32, MappedAtom<'a>>,
    bonds: BTreeSet<(u32, u32)>,
    unmapped: usize,
    charge: i64,
}

impl<'a> MappedSide<'a> {
    fn new(part: &'a Document) -> Result<Self, TsPlanError> {
        // Folding an isotope-labelled H into its heavy atom's hydrogen count
        // loses its identity, even when an exchange preserves every count.
        if part
            .atoms
            .iter()
            .any(|atom| atom.element == "H" && atom.isotope != 0 && atom.map_num == 0)
        {
            return Err(TsPlanError::UnmappedIsotopeHydrogen);
        }
        let unsupported = |error: chemistry::Error| TsPlanError::Unsupported(error.to_string());
        let prepared = chemistry::prepare(part).map_err(unsupported)?;
        let graph = chemistry::drawing_graph(part).map_err(unsupported)?;
        let hydrogens = attached_hydrogens(&graph, &prepared, |i| {
            part.atoms.get(i).is_some_and(|atom| atom.map_num != 0)
        });
        let mut atoms = BTreeMap::new();
        let mut drawing_maps = BTreeMap::new();
        let mut unmapped = 0;
        for (drawing, hydrogens) in part.atoms.iter().zip(hydrogens) {
            let Some(hydrogens) = hydrogens else {
                continue;
            };
            let map = drawing.map_num;
            if map == 0 {
                if drawing.element == "H" {
                    return Err(TsPlanError::UnmappedHydrogen);
                }
                unmapped += 1;
                continue;
            }
            let atom = MappedAtom {
                drawing,
                hydrogens: hydrogens.len(),
            };
            if atoms.insert(map, atom).is_some() {
                return Err(TsPlanError::Mismatch(format!(
                    "Atom map {map} occurs twice on one reaction side"
                )));
            }
            drawing_maps.insert(drawing.id, map);
        }
        let bonds = part
            .bonds
            .iter()
            .filter_map(|bond| {
                let (&a, &b) = (drawing_maps.get(&bond.a)?, drawing_maps.get(&bond.b)?);
                Some((a.min(b), a.max(b)))
            })
            .collect();
        let charge = part.atoms.iter().map(|a| i64::from(a.charge)).sum();
        Ok(Self {
            atoms,
            bonds,
            unmapped,
            charge,
        })
    }

    fn atom(&self, map: u32) -> Result<&MappedAtom<'a>, TsPlanError> {
        self.atoms.get(&map).ok_or_else(|| {
            TsPlanError::Mismatch(format!("Atom map {map} is missing on one reaction side"))
        })
    }

    fn drawing_bonds<'b>(
        &self,
        pairs: impl Iterator<Item = &'b (u32, u32)>,
    ) -> Result<Vec<(u64, u64)>, TsPlanError> {
        pairs
            .map(|&(a, b)| Ok((self.atom(a)?.drawing.id, self.atom(b)?.drawing.id)))
            .collect()
    }
}

fn hydrogen_moves(
    from: &MappedSide<'_>,
    to: &MappedSide<'_>,
) -> Result<Vec<(u64, u64)>, TsPlanError> {
    let mut donors = Vec::new();
    let mut acceptors = Vec::new();
    for (&map, atom) in &from.atoms {
        let next = to.atom(map)?.hydrogens;
        match atom.hydrogens.cmp(&next) {
            std::cmp::Ordering::Greater => {
                donors.push((atom.drawing, atom.hydrogens - next));
            }
            std::cmp::Ordering::Less => acceptors.push((atom.drawing, next - atom.hydrogens)),
            std::cmp::Ordering::Equal => {}
        }
    }
    let donated: usize = donors.iter().map(|(_, count)| count).sum();
    let accepted: usize = acceptors.iter().map(|(_, count)| count).sum();
    if donated != accepted {
        return Err(TsPlanError::HydrogenBalance);
    }
    let mut candidates = Vec::new();
    for (donor, (a, _)) in donors.iter().enumerate() {
        for (acceptor, (b, _)) in acceptors.iter().enumerate() {
            let dx = f64::from(a.position.x) - f64::from(b.position.x);
            let dy = f64::from(a.position.y) - f64::from(b.position.y);
            candidates.push((dx * dx + dy * dy, donor, acceptor));
        }
    }
    // Both site lists are in map order, so their indices also break ties by map.
    candidates.sort_unstable_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| (a.1, a.2).cmp(&(b.1, b.2)))
    });
    let mut moves = Vec::with_capacity(donated);
    for (_, donor, acceptor) in candidates {
        if let (Some((a, remaining)), Some((b, needed))) =
            (donors.get_mut(donor), acceptors.get_mut(acceptor))
        {
            let count = (*remaining).min(*needed);
            moves.extend(std::iter::repeat_n((a.id, b.id), count));
            *remaining -= count;
            *needed -= count;
        }
    }
    Ok(moves)
}

#[cfg(test)]
mod tests;
