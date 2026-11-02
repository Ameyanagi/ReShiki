//! Bounded recovery of the closed-shell parent for descriptive radical-ion names.
//!
//! RC-85.1 permits a neutral parent's name followed by `radical cation` or
//! `radical anion`. The recovered graph is only a naming/verification target:
//! it must never replace the identity of the original radical ion.
use reshiki_chemistry::{
    graph::Graph,
    kekulize::Direction,
    sanitize, smiles,
    stereo::perception::{self, State},
};

const UNSUPPORTED: &str =
    "openclatura: no supported closed-shell neutral parent was recovered for this radical ion";

/// Recover one neutral parent without adding/removing atoms or hydrogens.
///
/// Only one unit formal charge and one unpaired electron are supported. They
/// may occupy the same atom, or the two ends of a localized single bond that
/// becomes a double bond. More distant resonance searches, multiple charges,
/// and multiple unpaired electrons deliberately remain outside this heuristic.
/// `None` leaves ordinary ions and neutral radicals to their own naming paths.
pub(in crate::naming) fn recover(smiles: &str) -> Result<Option<(String, &'static str)>, String> {
    // Check the unmodified input before native preparation can clean stereo.
    // This also proves the ordinary SMILES retains the original radical state.
    let canonical = super::super::canonical_smiles(smiles)?;
    let source = smiles::prepare(smiles)
        .map_err(|error| error.to_string())?
        .state;
    let radicals: Vec<_> = source
        .graph
        .atoms
        .iter()
        .enumerate()
        .filter(|(_, atom)| atom.radical_electrons != 0)
        .collect();
    let charge: i32 = source
        .graph
        .atoms
        .iter()
        .map(|atom| i32::from(atom.charge))
        .sum();
    if radicals.is_empty() || charge == 0 {
        return Ok(None);
    }
    if canonical.contains('.') {
        return Err(UNSUPPORTED.into());
    }
    let charges: Vec<_> = source
        .graph
        .atoms
        .iter()
        .enumerate()
        .filter(|(_, atom)| atom.charge != 0)
        .collect();
    let ([(radical, radical_atom)], [(charged, charged_atom)]) =
        (radicals.as_slice(), charges.as_slice())
    else {
        return Err(UNSUPPORTED.into());
    };
    if radical_atom.radical_electrons != 1 || !matches!(charged_atom.charge, -1 | 1) {
        return Err(UNSUPPORTED.into());
    }

    let mut graph = source.graph.clone();
    // Freeze every attached H before changing charge/valence. Allowing native
    // implicit H assignment here would turn dehydrogenated ions into another
    // molecule and make a false neutral-parent recovery appear successful.
    for (atom, valence) in graph.atoms.iter_mut().zip(&source.valences) {
        atom.explicit_hydrogens =
            u8::try_from(u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens)
                .map_err(|_| UNSUPPORTED)?;
        atom.no_implicit = true;
    }
    graph.atoms[*radical].radical_electrons = 0;
    graph.atoms[*charged].charge = 0;

    if radical != charged {
        let Some((index, bond)) = graph.bonds.iter_mut().enumerate().find(|(_, bond)| {
            (bond.a == *radical && bond.b == *charged) || (bond.a == *charged && bond.b == *radical)
        }) else {
            return Err(UNSUPPORTED.into());
        };
        // A localized one-electron pi bond can be represented by this single
        // radical/ion pair. Changing an aromatic or already multiple bond
        // would require a resonance search and is intentionally not inferred.
        if bond.order != 1
            || bond.aromatic
            || source.directions.get(index) != Some(&Direction::None)
            || source
                .metadata
                .bonds
                .get(index)
                .is_none_or(|meta| meta.stereo != 0)
        {
            return Err(UNSUPPORTED.into());
        }
        bond.order = 2;
    }

    let neutral = closed_shell_parent(&source, &graph).ok_or(UNSUPPORTED)?;
    let suffix = if charge > 0 {
        "radical cation"
    } else {
        "radical anion"
    };
    Ok(Some((neutral, suffix)))
}

/// Sanitize a detached candidate and prove that no implicit hydrogen, radical,
/// charge or atom edit was introduced while producing ordinary neutral SMILES.
fn closed_shell_parent(source: &State, graph: &Graph) -> Option<String> {
    let sanitized = sanitize::sanitize(graph, &source.metadata, &source.directions).ok()?;
    if sanitized.graph.atoms.len() != graph.atoms.len()
        || sanitized.graph.bonds.len() != graph.bonds.len()
        || sanitized
            .graph
            .atoms
            .iter()
            .zip(&sanitized.valences)
            .zip(&graph.atoms)
            .any(|((atom, valence), before)| {
                atom.atomic_number != before.atomic_number
                    || atom.isotope != before.isotope
                    || atom.charge != 0
                    || atom.radical_electrons != 0
                    || u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens
                        != u32::from(before.explicit_hydrogens)
            })
        || sanitized
            .graph
            .bonds
            .iter()
            .zip(&graph.bonds)
            .any(|(bond, before)| {
                bond.a != before.a
                    || bond.b != before.b
                    || !before.aromatic && !bond.aromatic && bond.order != before.order
            })
    {
        return None;
    }
    let properties = perception::Properties::unspecified(&sanitized.graph);
    let state = State {
        graph: sanitized.graph,
        metadata: sanitized.metadata,
        directions: sanitized.directions,
        valences: sanitized.valences,
        conjugated: sanitized.conjugated,
        hybridizations: sanitized.hybridizations,
        rings: perception::RingCache {
            kind: perception::RingKind::Symmetric,
            atoms: sanitized.rings,
        },
        properties,
    };
    let text = smiles::write::write(
        &state,
        smiles::write::Options {
            ignore_maps: true,
            ..smiles::write::Options::default()
        },
    )
    .ok()?
    .text;
    let prepared = smiles::prepare(&text).ok()?;
    if prepared
        .state
        .graph
        .atoms
        .iter()
        .any(|atom| atom.charge != 0 || atom.radical_electrons != 0)
    {
        return None;
    }
    super::super::canonical_smiles(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_radical_ion_pairs_recover_only_the_missing_pi_bond() {
        for (ion, expected, suffix) in [
            ("[CH2+][CH2]", "C=C", "radical cation"),
            ("[CH2-][CH2]", "C=C", "radical anion"),
            ("[CH+]1[CH]C=CC=C1", "c1ccccc1", "radical cation"),
            (
                "[CH-]1[CH]C=CC2=CC=CC=C12",
                "c1ccc2ccccc2c1",
                "radical anion",
            ),
        ] {
            let (neutral, actual_suffix) = recover(ion)
                .unwrap_or_else(|error| panic!("{ion}: {error}"))
                .unwrap();
            assert_eq!(actual_suffix, suffix, "{ion}");
            assert_eq!(
                neutral,
                crate::naming::canonical_smiles(expected).unwrap(),
                "{ion}"
            );
        }
    }

    #[test]
    fn same_atom_recovery_removes_only_charge_and_radical() {
        for (ion, expected, suffix) in [
            ("[NH3+]", "N", "radical cation"),
            ("C[N+](C)C", "CN(C)C", "radical cation"),
            ("[OH2+]", "O", "radical cation"),
        ] {
            let (neutral, actual_suffix) = recover(ion)
                .unwrap_or_else(|error| panic!("{ion}: {error}"))
                .unwrap();
            assert_eq!(actual_suffix, suffix, "{ion}");
            assert_eq!(
                neutral,
                crate::naming::canonical_smiles(expected).unwrap(),
                "{ion}"
            );
        }
    }

    #[test]
    fn neutral_parent_names_reparse_through_strict_opsin_to_the_recovered_graph() {
        let parser = opsin::Parser::new().unwrap();
        for ion in [
            "[CH2+][CH2]",
            "[CH+]1[CH]C=CC=C1",
            "[CH-]1[CH]C=CC2=CC=CC=C12",
            "C[N+](C)C",
        ] {
            let (neutral, _) = recover(ion).unwrap().unwrap();
            let name = super::super::name_smiles(&neutral).unwrap();
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{ion}: {}",
                name.text
            );
            let parsed_smiles = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(&neutral, &parsed_smiles).unwrap();
        }
    }

    #[test]
    fn recovery_does_not_add_hydrogen_or_search_remote_resonance_forms() {
        for ion in [
            "[CH2+]C[CH2]",     // Radical and cation are not adjacent.
            "[CH2+][CH]",       // Double bond still leaves a radical.
            "[CH+]#[C]",        // A multiple bond is never promoted.
            "[CH+][CH2]",       // More than one unpaired electron.
            "[CH2+][CH][CH2+]", // More than one charged center.
        ] {
            assert!(recover(ion).is_err(), "Unexpected recovery: {ion}");
        }
        for ordinary in ["CC", "[CH3]", "C[CH2+]", "[NH4+]", "[O-][N+](=O)C"] {
            assert!(recover(ordinary).unwrap().is_none(), "{ordinary}");
        }
    }

    #[test]
    fn drawn_localized_benzene_radical_cation_recovers_benzene() {
        use crate::document::{Document, Point};
        let mut document = Document::default();
        let atoms: Vec<_> = (0..6)
            .map(|index| {
                let angle = std::f32::consts::TAU * index as f32 / 6.0;
                document.add_atom("C", Point::new(angle.cos() * 28.0, angle.sin() * 28.0))
            })
            .collect();
        // Cation and radical on adjacent atoms, their shared pi bond broken.
        for (index, order) in [1, 1, 2, 1, 2, 1].into_iter().enumerate() {
            document.add_bond(atoms[index], atoms[(index + 1) % 6], order, "plain");
        }
        document.atoms[0].charge = 1;
        document.atoms[1].radical_electrons = 1;
        let identity = crate::naming::document_identity(&document).unwrap();
        let (neutral, suffix) = recover(&identity.smiles).unwrap().unwrap();
        assert_eq!(
            neutral,
            crate::naming::canonical_smiles("c1ccccc1").unwrap()
        );
        assert_eq!(suffix, "radical cation");
        // The same-atom form over-fills a Kekulé carbon and is refused by
        // preparation, before naming.
        document.bonds[0].order = 2;
        document.atoms[1].radical_electrons = 0;
        document.atoms[0].radical_electrons = 1;
        assert!(crate::naming::document_identity(&document).is_err());
    }
}
