// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: ReShiki | src/naming/openclatura/radical_ions.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Source: openclatura | tests/fixtures/diverse_corpus.csv | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_analysis.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/integration/test_describer.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

//! Bounded recovery of the closed-shell parent for descriptive radical-ion names.
//!
//! RC-85.1 permits a neutral parent's name followed by `radical cation` or
//! `radical anion`. The recovered graph is only a naming/verification target:
//! it must never replace the identity of the original radical ion.
use nazuchem_kernel::{
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
// Provenance: fn recover | original
// Source: ReShiki | src/naming/openclatura/radical_ions.rs::recover
pub fn recover(
    smiles: &str,
    budget: &nazuchem_kernel::budget::Budget,
) -> Result<Option<(String, &'static str)>, String> {
    // Check the unmodified input before native preparation can clean stereo.
    // This also proves the ordinary SMILES retains the original radical state.
    let canonical = crate::canonical_smiles(smiles, budget)?;
    let prepared = smiles::prepare(smiles, budget).map_err(|error| error.to_string())?;
    let source = nazuchem_kernel::identity::apply_axis_markers(&prepared.state, budget)
        .map_err(|_| UNSUPPORTED)?;
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

    let neutral = closed_shell_parent(&source, &graph, budget).ok_or(UNSUPPORTED)?;
    let suffix = if charge > 0 {
        "radical cation"
    } else {
        "radical anion"
    };
    Ok(Some((neutral, suffix)))
}

/// Sanitize a detached candidate and prove that no implicit hydrogen, radical,
/// charge or atom edit was introduced while producing ordinary neutral SMILES.
// Provenance: fn closed_shell_parent | original
// Source: ReShiki | src/naming/openclatura/radical_ions.rs::closed_shell_parent
fn closed_shell_parent(
    source: &State,
    graph: &Graph,
    budget: &nazuchem_kernel::budget::Budget,
) -> Option<String> {
    let sanitized = sanitize::sanitize(graph, &source.metadata, &source.directions, budget).ok()?;
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
    // Recompute axial descriptors after neutralization: an M/P identity map
    // from the radical ion need not retain its letter if CIP priorities change.
    let text = nazuchem_kernel::identity::canonical_identity(&state, budget).ok()?;
    let prepared = smiles::prepare(&text, budget).ok()?;
    if prepared
        .state
        .graph
        .atoms
        .iter()
        .any(|atom| atom.charge != 0 || atom.radical_electrons != 0)
    {
        return None;
    }
    crate::canonical_smiles(&text, budget).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // Provenance: fn tests::radical_ion_recovery_preserves_decoded_biaryl_configuration | original
    // Source: ReShiki | src/naming/openclatura/radical_ions.rs::tests::radical_ion_recovery_preserves_decoded_biaryl_configuration
    fn radical_ion_recovery_preserves_decoded_biaryl_configuration() {
        let input = "Oc1ccc(C[N+](C)C)c[c:900004]1-[c:900004]1ccccc1O";
        let (neutral, suffix) = recover(input, &Default::default()).unwrap().unwrap();
        assert_eq!(suffix, "radical cation");
        let expected = input.replace("[N+]", "N");
        crate::test_support::verify_identity(&expected, &neutral).unwrap();
        let prepared = smiles::prepare(&neutral, &Default::default()).unwrap();
        let decoded =
            nazuchem_kernel::identity::apply_axis_markers(&prepared.state, &Default::default())
                .unwrap();
        assert_eq!(
            decoded
                .metadata
                .bonds
                .iter()
                .filter(|bond| matches!(bond.stereo, 6 | 7))
                .count(),
            1
        );
        assert!(
            decoded
                .graph
                .atoms
                .iter()
                .all(|atom| atom.charge == 0 && atom.radical_electrons == 0)
        );
    }

    #[test]
    // Provenance: fn tests::localized_radical_ion_pairs_recover_only_the_missing_pi_bond | adapted
    // Source: ReShiki | src/naming/openclatura/radical_ions.rs::tests::localized_radical_ion_pairs_recover_only_the_missing_pi_bond
    // Source: openclatura | tests/fixtures/diverse_corpus.csv::<fixture data>
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
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
            let (neutral, actual_suffix) = recover(ion, &Default::default())
                .unwrap_or_else(|error| panic!("{ion}: {error}"))
                .unwrap();
            assert_eq!(actual_suffix, suffix, "{ion}");
            assert_eq!(
                neutral,
                crate::test_support::canonical_smiles(expected).unwrap(),
                "{ion}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::same_atom_recovery_removes_only_charge_and_radical | adapted
    // Source: ReShiki | src/naming/openclatura/radical_ions.rs::tests::same_atom_recovery_removes_only_charge_and_radical
    // Source: openclatura | tests/fixtures/diverse_corpus.csv::<fixture data>
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn same_atom_recovery_removes_only_charge_and_radical() {
        for (ion, expected, suffix) in [
            ("[NH3+]", "N", "radical cation"),
            ("C[N+](C)C", "CN(C)C", "radical cation"),
            ("[OH2+]", "O", "radical cation"),
        ] {
            let (neutral, actual_suffix) = recover(ion, &Default::default())
                .unwrap_or_else(|error| panic!("{ion}: {error}"))
                .unwrap();
            assert_eq!(actual_suffix, suffix, "{ion}");
            assert_eq!(
                neutral,
                crate::test_support::canonical_smiles(expected).unwrap(),
                "{ion}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::neutral_parent_names_reparse_through_strict_opsin_to_the_recovered_graph | original
    // Source: ReShiki | src/naming/openclatura/radical_ions.rs::tests::neutral_parent_names_reparse_through_strict_opsin_to_the_recovered_graph
    fn neutral_parent_names_reparse_through_strict_opsin_to_the_recovered_graph() {
        let parser = nazuchem_n2s::Parser::new().unwrap();
        for ion in [
            "[CH2+][CH2]",
            "[CH+]1[CH]C=CC=C1",
            "[CH-]1[CH]C=CC2=CC=CC=C12",
            "C[N+](C)C",
        ] {
            let (neutral, _) = recover(ion, &Default::default()).unwrap().unwrap();
            let name = super::super::name_smiles(&neutral, &Default::default()).unwrap();
            let parsed = parser.parse(&name.text, &nazuchem_n2s::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                nazuchem_n2s::Status::Success,
                "{ion}: {}",
                name.text
            );
            let parsed_smiles = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::test_support::verify_identity(&neutral, &parsed_smiles).unwrap();
        }
    }

    #[test]
    // Provenance: fn tests::recovery_does_not_add_hydrogen_or_search_remote_resonance_forms | adapted
    // Source: ReShiki | src/naming/openclatura/radical_ions.rs::tests::recovery_does_not_add_hydrogen_or_search_remote_resonance_forms
    // Source: openclatura | tests/unit/test_analysis.py::test_public_api_golden_names
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn recovery_does_not_add_hydrogen_or_search_remote_resonance_forms() {
        for ion in [
            "[CH2+]C[CH2]",     // Radical and cation are not adjacent.
            "[CH2+][CH]",       // Double bond still leaves a radical.
            "[CH+]#[C]",        // A multiple bond is never promoted.
            "[CH+][CH2]",       // More than one unpaired electron.
            "[CH2+][CH][CH2+]", // More than one charged center.
        ] {
            assert!(
                recover(ion, &Default::default()).is_err(),
                "Unexpected recovery: {ion}"
            );
        }
        for ordinary in ["CC", "[CH3]", "C[CH2+]", "[NH4+]", "[O-][N+](=O)C"] {
            assert!(
                recover(ordinary, &Default::default()).unwrap().is_none(),
                "{ordinary}"
            );
        }
    }
}
