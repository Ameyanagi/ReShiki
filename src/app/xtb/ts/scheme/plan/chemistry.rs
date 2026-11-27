//! Exact atom/isotope/charge accounting and single-molecule inputs.
use super::{Species, Step};
use crate::app::xtb::ts::{card, review::species as composition};
use reshiki::chemistry::{self, document};
use reshiki::document::{Document, Point};

type Builtin = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [(usize, usize, u8)],
);

const BUILTINS: &[Builtin] = &[
    ("H₂O", "water", &["O"], &[]),
    ("HCl", "hydrogen chloride", &["Cl"], &[]),
    ("NH₃", "ammonia", &["N"], &[]),
    ("MeOH", "methanol", &["C", "O"], &[(0, 1, 1)]),
    ("HBr", "hydrogen bromide", &["Br"], &[]),
    ("N₂", "nitrogen", &["N", "N"], &[(0, 1, 3)]),
    (
        "CO₂",
        "carbon dioxide",
        &["O", "C", "O"],
        &[(0, 1, 2), (1, 2, 2)],
    ),
    ("H₂", "hydrogen", &["H", "H"], &[(0, 1, 1)]),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Composition {
    atoms: composition::Counts,
    charge: i64,
}

impl Composition {
    fn add(&mut self, other: &Self, coefficient: i64) -> Result<(), String> {
        for (&atom, &count) in &other.atoms {
            let count = count
                .checked_mul(coefficient)
                .ok_or("Atom count overflow")?;
            let entry = self.atoms.entry(atom).or_default();
            *entry = entry.checked_add(count).ok_or("Atom count overflow")?;
        }
        self.atoms.retain(|_, count| *count != 0);
        self.charge = self
            .charge
            .checked_add(
                other
                    .charge
                    .checked_mul(coefficient)
                    .ok_or("Charge overflow")?,
            )
            .ok_or("Charge overflow")?;
        Ok(())
    }

    pub(super) fn is_zero(&self) -> bool {
        self.atoms.is_empty() && self.charge == 0
    }

    pub(super) fn balanced_by(&self, candidate: &Self, product: bool) -> bool {
        let mut corrected = self.clone();
        corrected
            .add(candidate, if product { 1 } else { -1 })
            .is_ok()
            && corrected.is_zero()
    }

    pub(super) fn description(&self) -> String {
        let mut details: Vec<_> = self
            .atoms
            .iter()
            .map(|(&atom, &count)| {
                format!(
                    "{} {} {}",
                    count.unsigned_abs(),
                    if count > 0 { "more" } else { "fewer" },
                    composition::symbol(atom)
                )
            })
            .collect();
        if self.charge != 0 {
            details.push(format!("a charge difference of {:+}", self.charge));
        }
        format!("Products have {} relative to reactants", details.join(", "))
    }
}

pub(super) fn difference(species: &[Species], step: &Step) -> Result<Composition, String> {
    let mut difference = Composition::default();
    for (side, sign) in [(&step.reactants, -1), (&step.products, 1)] {
        for term in side {
            let species = species
                .get(term.species)
                .ok_or("Unknown reaction species")?;
            difference.add(&species.composition, i64::from(term.coefficient) * sign)?;
        }
    }
    Ok(difference)
}

/// Compound numbering is an instance label; the descriptive name remains
/// available separately for the expanded setup and result views.
pub(super) fn labels(doc: &Document, atoms: &[u64], formula: &str) -> (String, String) {
    let number = card::compound_label(doc, atoms);
    let name = card::molecule_name(doc, atoms).unwrap_or(formula);
    (number.unwrap_or(name).to_owned(), name.to_owned())
}

fn calculation(doc: &Document, atoms: &[u64]) -> Document {
    let source = reshiki::editing::selection(doc, atoms);
    Document {
        atoms: source.atoms,
        bonds: source.bonds,
        models: source.models,
        generic: source.generic,
        abbreviations: source.abbreviations,
        version: source.version,
        ..Document::default()
    }
}

pub(super) fn molecule(doc: &Document, atoms: &[u64]) -> Result<Species, String> {
    let calculation = calculation(doc, atoms);
    let prepared = document::prepare(&calculation).map_err(|error| error.to_string())?;
    let first = *atoms.first().ok_or("A reaction species has no atoms")?;
    let (charge, multiplicity) = reshiki::science::drawn_state(doc, first);
    let composition = Composition {
        atoms: composition::prepared_counts(&prepared)?,
        charge: i64::from(charge),
    };
    let protons = composition
        .atoms
        .iter()
        .try_fold(0i64, |sum, (&(number, _), &count)| {
            sum.checked_add(
                i64::from(number)
                    .checked_mul(count)
                    .ok_or("Electron count overflow")?,
            )
            .ok_or("Electron count overflow")
        })?;
    crate::app::xtb::state::electrons(
        u32::try_from(protons).map_err(|_| "The molecule has too many electrons")?,
        charge,
        multiplicity.saturating_sub(1),
    )?;
    // A SMILES round trip folds ordinary explicitly drawn hydrogens, so O,
    // [H]O[H] and a drawn built-in water are one balancing candidate. Isotopic
    // hydrogens and every supported stereo descriptor remain in the identity.
    let identity = molecular_identity(&prepared.state);
    let key = format!(
        "{}/{charge}/{multiplicity}",
        identity.unwrap_or_else(|| format!(
            "drawn:{atoms:?}:{}",
            reshiki::science::baseline(&calculation, atoms)
        ))
    );
    let formula = composition::formula(&composition.atoms);
    let (label, name) = labels(doc, atoms, &formula);
    Ok(Species {
        atoms: atoms.to_vec(),
        calculation,
        label,
        name,
        formula,
        charge,
        multiplicity,
        key,
        composition,
    })
}

fn molecular_identity(state: &chemistry::stereo::perception::State) -> Option<String> {
    // Naming intentionally removes explicit unknown stereo. Calculation reuse
    // cannot establish a conformer correspondence across that boundary.
    if reshiki::molecule_names::unknown_atom_units(state)
        .into_iter()
        .any(|unknown| unknown)
        || state
            .metadata
            .bonds
            .iter()
            .any(|bond| bond.unknown_stereo || bond.stereo == 1)
    {
        return None;
    }
    let identity = reshiki::molecule_names::canonical_identity(state).ok()?;
    let normalized = chemistry::smiles::prepare(&identity)
        .ok()
        // Canonical identity stores M/P axes in reserved atom maps. Decode
        // them before re-encoding, since ordinary maps are otherwise removed.
        .and_then(|prepared| reshiki::molecule_names::apply_axis_markers(&prepared.state).ok())
        .and_then(|state| reshiki::molecule_names::canonical_identity(&state).ok());
    Some(normalized.unwrap_or(identity))
}

pub(super) fn candidates() -> Result<Vec<Species>, String> {
    let mut candidates = Vec::new();
    for &(label, name, elements, bonds) in BUILTINS {
        let mut doc = Document::default();
        let mut ids = Vec::new();
        for (index, element) in elements.iter().enumerate() {
            ids.push(doc.add_atom(element, Point::new(index as f32 * 42., 0.)));
        }
        for &(a, b, order) in bonds {
            let a = *ids.get(a).ok_or("Missing balancing molecule atom")?;
            let b = *ids.get(b).ok_or("Missing balancing molecule atom")?;
            doc.add_bond(a, b, order, "plain");
        }
        let mut species = molecule(&doc, &ids)?;
        species.atoms.clear();
        species.label = label.into();
        species.name = name.into();
        candidates.push(species);
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_reuse_identity_retains_isotopic_tetrahedral_stereo() {
        let identity =
            |text| molecular_identity(&chemistry::smiles::prepare(text).unwrap().state).unwrap();
        assert_ne!(
            identity("[2H][C@](F)(Cl)Br"),
            identity("[2H][C@@](F)(Cl)Br")
        );
        assert_ne!(identity("[2H][C@](F)(Cl)Br"), identity("[H][C@](F)(Cl)Br"));
    }

    #[test]
    fn enhanced_and_explicit_unknown_stereo_require_the_drawn_instance_fallback() {
        let mut state = chemistry::smiles::prepare("F[C@H](Cl)Br").unwrap().state;
        assert!(molecular_identity(&state).is_some());
        state.metadata.groups.push(chemistry::ranking::StereoGroup {
            kind: 2,
            atoms: vec![1],
            bonds: vec![],
            read_id: 1,
            write_id: 1,
        });
        assert!(molecular_identity(&state).is_none());
        state.metadata.groups.clear();
        state.properties.atoms[1].unknown = true;
        assert!(molecular_identity(&state).is_none());
    }

    #[test]
    fn hydrogen_normalization_decodes_the_private_axial_stereo_markers() {
        let mut state = chemistry::smiles::prepare("Oc1ccccc1-c1ccccc1O")
            .unwrap()
            .state;
        let axis = state
            .graph
            .bonds
            .iter()
            .position(|bond| {
                bond.order == 1
                    && state.graph.atoms[bond.a].aromatic
                    && state.graph.atoms[bond.b].aromatic
            })
            .unwrap();
        let bond = &state.graph.bonds[axis];
        let refs: Vec<_> = [(bond.a, bond.b), (bond.b, bond.a)]
            .into_iter()
            .map(|(end, partner)| {
                state
                    .graph
                    .bonds
                    .iter()
                    .filter_map(|bond| match (bond.a, bond.b) {
                        (a, b) | (b, a) if a == end && b != partner => Some(b),
                        _ => None,
                    })
                    .min()
                    .unwrap()
            })
            .collect();
        state.metadata.bonds[axis].stereo_atoms = refs;
        state.metadata.bonds[axis].stereo = 6;
        let first = molecular_identity(&state).unwrap();
        assert_eq!(
            first,
            reshiki::molecule_names::canonical_identity(&state).unwrap()
        );
        assert_eq!(first.matches(":90000").count(), 2);
        state.metadata.bonds[axis].stereo = 7;
        let second = molecular_identity(&state).unwrap();
        assert_eq!(
            second,
            reshiki::molecule_names::canonical_identity(&state).unwrap()
        );
        assert_ne!(first, second);
    }
}
