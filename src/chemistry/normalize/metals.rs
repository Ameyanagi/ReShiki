//! Organometallic normalization from RDKit MolOps.cpp (2026.03.6).
//! Copyright (C) 2001-2023 Greg Landrum and other RDKit contributors.
//! Metal classification from QueryOps.cpp, Copyright (C) 2003-2021.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::at;
use crate::chemistry::{
    ELEMENTS,
    graph::{Atom, Graph, Valence},
    ranking::{self, Metadata},
    rings,
};
use std::cmp::Reverse;

fn is_metal(number: u8) -> bool {
    !matches!(number, 0..=2 | 5..=10 | 14..=18 | 33..=36 | 52..=54 | 85..=86)
}

fn hypervalent_nonmetal(atom: &Atom, valence: &Valence, degree: usize) -> Result<bool, String> {
    if is_metal(atom.atomic_number) {
        return Ok(false);
    }
    let effective = i32::from(atom.atomic_number) - i32::from(atom.charge);
    if effective <= 0 {
        return Ok(false);
    }
    // Unlike the general property cache, this pass does not clamp the effective
    // element. The reference rejects an out-of-range periodic-table lookup.
    let element = ELEMENTS
        .get(effective as usize)
        .ok_or("Invalid effective element in metal cleanup")?;
    let maximum = *element
        .valences
        .last()
        .ok_or("Missing metal-cleanup valence")?;
    let total_degree =
        degree + usize::from(atom.explicit_hydrogens) + valence.implicit_hydrogens as usize;
    Ok(maximum > 0
        && (valence.explicit_valence > maximum as u32
            || valence.explicit_valence == maximum as u32 && atom.aromatic && total_degree == 4))
}

/// Convert one hypervalent donor--metal single bond per donor to donor->metal.
/// Preserve a caller's existing ring cache when supplied; otherwise ranking uses
/// the bounded fast ring pass. Return a copy so failures cannot edit a document.
pub fn organometallics(
    graph: &Graph,
    metadata: &Metadata,
    cached_rings: Option<&[Vec<usize>]>,
) -> Result<Graph, String> {
    let valences = graph.provisional_valences()?;
    let mut neighbors = vec![Vec::new(); graph.atoms.len()];
    let mut datives = vec![0usize; graph.atoms.len()];
    for (id, bond) in graph.bonds.iter().enumerate() {
        for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
            neighbors
                .get_mut(a)
                .ok_or("Missing metal-cleanup endpoint")?
                .push((b, id));
            if bond.order == 5 {
                *datives.get_mut(a).ok_or("Missing dative endpoint")? += 1;
            }
        }
    }
    let mut candidates = Vec::new();
    for (id, atom) in graph.atoms.iter().enumerate() {
        let adjacent = at(&neighbors, id)?;
        // Call the valence test before excluding H/He/F/Ne, matching the
        // reference's validation of extreme formal charges on those atoms.
        if !hypervalent_nonmetal(atom, at(&valences, id)?, adjacent.len())?
            || matches!(atom.atomic_number, 1 | 2 | 9 | 10)
        {
            continue;
        }
        let mut metals = Vec::new();
        for &(other, bond) in adjacent {
            if at(&graph.bonds, bond)?.order == 1
                && is_metal(at(&graph.atoms, other)?.atomic_number)
            {
                metals.push((other, bond));
            }
        }
        if !metals.is_empty() {
            candidates.push((id, metals));
        }
    }
    if candidates.is_empty() {
        return Ok(graph.clone());
    }
    let discovered;
    let cached_rings = match cached_rings {
        Some(rings) => rings,
        None => {
            discovered = rings::fast(graph)?.atoms;
            &discovered
        }
    };
    let ranks = ranking::rank(graph, cached_rings, metadata, ranking::Options::default())?;
    let mut ordered = candidates
        .into_iter()
        .map(|(id, metals)| Ok((*at(&ranks, id)?, id, metals)))
        .collect::<Result<Vec<_>, String>>()?;
    ordered.sort_unstable_by_key(|entry| entry.0);
    let mut result = graph.clone();
    // Changes touch only the current non-metal and a metal. Thus other donors'
    // initial valences remain valid, while each metal's live dative count must
    // be updated before choosing the destination for the next donor.
    for (_, donor, metals) in ordered {
        let mut choice = None;
        for (metal, bond) in metals {
            let key = (
                *at(&datives, metal)?,
                Reverse(*at(&ranks, metal)?),
                metal,
                bond,
            );
            if choice.is_none_or(|previous| key < previous) {
                choice = Some(key);
            }
        }
        let (_, _, metal, id) = choice.ok_or("Missing metal-cleanup candidate")?;
        let bond = result.bonds.get_mut(id).ok_or("Missing donor bond")?;
        bond.order = 5;
        bond.a = donor;
        bond.b = metal;
        for atom in [donor, metal] {
            *datives
                .get_mut(atom)
                .ok_or("Missing updated dative endpoint")? += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chemistry::graph::Bond;

    fn donor() -> Graph {
        Graph {
            atoms: vec![
                Atom {
                    atomic_number: 8,
                    explicit_hydrogens: 2,
                    ..Atom::default()
                },
                Atom {
                    atomic_number: 26,
                    ..Atom::default()
                },
                Atom {
                    atomic_number: 26,
                    ..Atom::default()
                },
            ],
            bonds: (1..3)
                .map(|a| Bond {
                    a,
                    b: 0,
                    order: 1,
                    aromatic: false,
                })
                .collect(),
        }
    }

    #[test]
    fn one_conversion_per_donor_preserves_atoms_and_corrects_direction() -> Result<(), String> {
        let graph = donor();
        let before = serde_json::to_value(&graph).map_err(|error| error.to_string())?;
        let result = organometallics(&graph, &Metadata::unspecified(&graph), None)?;
        assert_eq!(result.bonds.iter().filter(|b| b.order == 5).count(), 1);
        assert_eq!(result.bonds.iter().filter(|b| b.order == 1).count(), 1);
        assert!(
            result
                .bonds
                .iter()
                .filter(|b| b.order == 5)
                .all(|b| b.a == 0)
        );
        assert_eq!(result.provisional_valences()?[0].explicit_valence, 3);
        assert_eq!(
            serde_json::to_value(&result.atoms).map_err(|e| e.to_string())?,
            before["atoms"]
        );
        assert_eq!(
            serde_json::to_value(&graph).map_err(|e| e.to_string())?,
            before
        );
        Ok(())
    }

    #[test]
    fn malformed_inputs_fail_before_any_result_is_exposed() -> Result<(), String> {
        for kind in 0..4 {
            let mut graph = donor();
            let mut metadata = Metadata::unspecified(&graph);
            let mut rings = None;
            match kind {
                0 => graph.bonds[0].a = 999,
                1 => graph.atoms[0].charge = i8::MIN,
                2 => metadata.atoms.clear(),
                _ => rings = Some(vec![vec![0, 1, 999]]),
            }
            let before = serde_json::to_value(&graph).map_err(|error| error.to_string())?;
            assert!(organometallics(&graph, &metadata, rings.as_deref()).is_err());
            assert_eq!(
                serde_json::to_value(&graph).map_err(|e| e.to_string())?,
                before
            );
        }
        Ok(())
    }
}
