// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: ReShiki | src/naming/openclatura/unknown_stereo.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Source: openclatura | tests/unit/test_analysis.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/evaluations/test_check_regression.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

//! Explicitly unknown stereogenic units transported in private atom maps.
//! The structure used for identity omits only these units' configurations.
use super::{Name, graph_io, molecule::Molecule};
use crate::naming::{AlternativeName, NameReason, NameStatus};
use nazuchem_kernel::stereo::perception;

const UNKNOWN_ATOM: i32 = 900_001;
const UNKNOWN_BOND: i32 = 900_002;
const UNKNOWN_BOTH: i32 = 900_003;

// Provenance: fn atom_marker | original
// Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::atom_marker
fn atom_marker(map: i32) -> bool {
    matches!(map, UNKNOWN_ATOM | UNKNOWN_BOTH)
}

// Provenance: fn bond_marker | original
// Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::bond_marker
fn bond_marker(map: i32) -> bool {
    matches!(map, UNKNOWN_BOND | UNKNOWN_BOTH)
}

/// Attach unknown descriptors after the ordinary graph/CIP import. A detached
/// full CIP trial distinguishes pseudoasymmetric ξ from ordinary Ξ wherever
/// the remaining specified stereochemistry makes that distinction possible.
// Provenance: fn annotate | original
// Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::annotate
pub(super) fn annotate(state: &perception::State, mol: &mut Molecule) -> Result<(), String> {
    let maps: Vec<_> = state
        .metadata
        .atoms
        .iter()
        .map(|atom| atom.map_number)
        .collect();
    for (idx, &map) in maps.iter().enumerate() {
        if !atom_marker(map) {
            continue;
        }
        let pseudo = super::app_chemistry::is_pseudoasymmetric(state, idx)?;
        let atom = mol
            .atoms
            .get_mut(&idx)
            .ok_or("Missing unknown stereocentre")?;
        atom.stereo = Some(if pseudo { "ξ" } else { "Ξ" }.into());
        atom.raw_stereo = None;
    }
    let mut used = vec![false; maps.len()];
    for bond in mol.bonds.values_mut() {
        if bond.order == 2 && bond_marker(maps[bond.u]) && bond_marker(maps[bond.v]) {
            bond.stereo = Some("Ξ".into());
            used[bond.u] = true;
            used[bond.v] = true;
        }
    }
    if maps
        .iter()
        .enumerate()
        .any(|(idx, &map)| bond_marker(map) && !used[idx])
    {
        return Err("openclatura: incomplete unknown double-bond marker".into());
    }
    Ok(())
}

/// An early hook in the ordinary entry point. Ordinary atom maps have no
/// naming meaning; only the reserved markers emitted by document_identity do.
// Provenance: fn name_smiles | original
// Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::name_smiles
pub(super) fn name_smiles(
    smiles: &str,
    style: crate::naming::IsotopeStyle,
    budget: &nazuchem_kernel::budget::Budget,
) -> Result<Option<Name>, String> {
    if ![":900001]", ":900002]", ":900003]"]
        .iter()
        .any(|marker| smiles.contains(marker))
    {
        return Ok(None);
    }
    // Keep the ordinary size and supported-semantics gate ahead of parsing
    // the internal marker transport, just as in graph_io::read_smiles.
    let unspecified = crate::naming::canonical_smiles(smiles)?;
    let mut normal = super::name_smiles_with_style(&unspecified, style, budget)?;
    let status = normal.status;
    normal.status = NameStatus::NotPreferred(NameReason::UnknownConfiguration);
    let mol = graph_io::read_smiles(smiles, budget)?;
    // The explicit route may use a different name family (e.g. an oxide
    // zwitterion under P-62.5); preserve any departures it records.
    mol.set_name_status(status);
    normal.omitted_units = mol
        .atoms
        .values()
        .filter(|atom| {
            atom.stereo
                .as_deref()
                .is_some_and(|s| matches!(s, "Ξ" | "ξ"))
        })
        .count()
        + mol
            .bonds
            .values()
            .filter(|bond| bond.stereo.as_deref() == Some("Ξ"))
            .count();
    // Offer the Ξ/ξ name only when it writes one descriptor per unknown unit
    // (P-91.2.1.2.1). A descriptor is scoped by its locant or, without one
    // (P-91.3), by the substituent bracket around it, as in
    // [(Ξ)-bromofluoromethyl]. A single unlocanted descriptor in front of the
    // whole name must leave the same name, as in (Ξ)-cyclooctene. Otherwise (a
    // multiplied substituent, a functional-class name) the unspecified name
    // stands alone and the note counts the omitted units.
    let one_per_unit = |raw: &str| {
        let chars: Vec<_> = raw.chars().collect();
        let (mut scoped, mut leading) = (0, 0);
        for i in (1..chars.len()).filter(|&i| matches!(chars[i], 'Ξ' | 'ξ')) {
            if chars[i - 1].is_ascii_alphanumeric()
                || chars[i - 1] == '\''
                || (chars[i - 1] == '(' && i > 1 && matches!(chars[i - 2], '(' | '[' | '{'))
            {
                scoped += 1;
            } else {
                leading += 1;
            }
        }
        scoped + leading == normal.omitted_units
            && (leading == 0
                || leading == 1
                    && super::nest_enclosing_marks(&without_unknown_descriptors(raw))
                        == normal.text)
    };
    let Some(explicit_raw) = graph_io::get_connected_components(&mol)
        .first()
        .and_then(|component| super::component_namer::name_component(&mol, component).ok())
        .filter(|raw| one_per_unit(raw))
    else {
        return Ok(Some(normal));
    };
    // Unknown descriptors participate in numbering tie-breaks. Omit them from
    // this same numbered name so the warning locants always identify the
    // displayed primary name, even when the unspecified graph numbers another way.
    normal.text = super::nest_enclosing_marks(&without_unknown_descriptors(&explicit_raw));
    normal.omitted_stereo = mol.unknown_units();
    normal.alternatives.insert(
        0,
        AlternativeName {
            name: super::nest_enclosing_marks(&explicit_raw),
            status: mol.name_status(),
        },
    );
    Ok(Some(normal))
}

// Provenance: fn without_unknown_descriptors | original
// Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::without_unknown_descriptors
fn without_unknown_descriptors(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut groups = Vec::new();
    let mut omit_hyphen = false;
    for character in raw.chars() {
        if omit_hyphen {
            omit_hyphen = false;
            if character == '-' {
                continue;
            }
        }
        match character {
            '(' => {
                groups.push(text.len());
                text.push(character);
            }
            ')' => {
                let Some(open) = groups.pop() else {
                    text.push(character);
                    continue;
                };
                let body = &text[open + 1..];
                let mut depth = 0usize;
                let mut start = 0;
                let mut items = Vec::new();
                for (idx, character) in body.char_indices() {
                    match character {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' => depth = depth.saturating_sub(1),
                        ',' if depth == 0 => {
                            items.push(&body[start..idx]);
                            start = idx + 1;
                        }
                        _ => (),
                    }
                }
                items.push(&body[start..]);
                let kept: Vec<_> = items
                    .iter()
                    .copied()
                    .filter(|item| !item.trim_end().ends_with(['Ξ', 'ξ']))
                    .collect();
                if kept.len() == items.len() {
                    text.push(character);
                } else {
                    let replacement = kept.join(",");
                    text.truncate(open);
                    if replacement.is_empty() {
                        omit_hyphen = true;
                    } else {
                        text.push('(');
                        text.push_str(&replacement);
                        text.push(')');
                    }
                }
            }
            _ => text.push(character),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    // Provenance: fn tests::round_trip | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::round_trip
    fn round_trip(input: &str, expected: &str, omitted: &[&str], explicit: &str) {
        let name = super::super::name_smiles(input, &Default::default()).unwrap();
        assert_eq!(name.text, expected);
        assert_eq!(name.omitted_stereo, omitted);
        assert_eq!(name.alternatives[0].name, explicit);
        assert_eq!(
            name.status,
            crate::naming::NameStatus::NotPreferred(
                crate::naming::NameReason::UnknownConfiguration
            )
        );
        for name in std::iter::once(name.text.as_str()).chain(
            name.alternatives
                .iter()
                .map(|alternative| alternative.name.as_str()),
        ) {
            let request = crate::naming::local::parse_request(name).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(name, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(input).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{name}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::wavy_tetrahedral_unit_retains_the_known_neighbour | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::wavy_tetrahedral_unit_retains_the_known_neighbour
    fn wavy_tetrahedral_unit_retains_the_known_neighbour() {
        round_trip(
            "C[CH:900001](O)[C@H](Cl)C",
            "(3R)-3-chlorobutan-2-ol",
            &["2"],
            "(2Ξ,3R)-3-chlorobutan-2-ol",
        );
    }

    #[test]
    // Provenance: fn tests::either_double_bond_names_the_unspecified_unit | adapted
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::either_double_bond_names_the_unspecified_unit
    // Source: openclatura | tests/unit/test_analysis.py::test_unsaturation_interfix_survives_a_locant_set
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn either_double_bond_names_the_unspecified_unit() {
        round_trip(
            "C[CH:900002]=[CH:900002]C",
            "but-2-ene",
            &["2=3"],
            "(2Ξ)-but-2-ene",
        );
    }

    #[test]
    // Provenance: fn tests::unlocanted_unknown_descriptors_round_trip | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::unlocanted_unknown_descriptors_round_trip
    fn unlocanted_unknown_descriptors_round_trip() {
        for (input, expected, omitted, explicit) in [
            (
                "F[CH:900001](Cl)Br",
                "bromochlorofluoromethane",
                &["1"] as &[&str],
                "(Ξ)-bromochlorofluoromethane",
            ),
            (
                "C1CCC[CH:900002]=[CH:900002]CC1",
                "cyclooctene",
                &["1=2"],
                "(Ξ)-cyclooctene",
            ),
            (
                "C1CCCC[CH:900002]=[CH:900002]CC1",
                "cyclononene",
                &["1=2"],
                "(Ξ)-cyclononene",
            ),
            (
                "O=C(O)[CH:900001](F)Cl",
                "chlorofluoroacetic acid",
                &["2"],
                "(Ξ)-chlorofluoroacetic acid",
            ),
            // Unlocanted descriptors are scoped by their substituent brackets.
            (
                "F[CH:900001](Cl)c1ccc([CH:900001](F)Br)cc1",
                "1-(bromofluoromethyl)-4-(chlorofluoromethyl)benzene",
                &[],
                "1-[(Ξ)-bromofluoromethyl]-4-[(Ξ)-chlorofluoromethyl]benzene",
            ),
            (
                "F[C@H](Cl)c1ccc([CH:900001](F)Br)cc1",
                "1-(bromofluoromethyl)-4-[(R)-chlorofluoromethyl]benzene",
                &[],
                "1-[(Ξ)-bromofluoromethyl]-4-[(R)-chlorofluoromethyl]benzene",
            ),
        ] {
            round_trip(input, expected, omitted, explicit);
        }
    }

    #[test]
    // Provenance: fn tests::locanted_pseudoasymmetric_unknown_descriptor_is_retained | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::locanted_pseudoasymmetric_unknown_descriptor_is_retained
    fn locanted_pseudoasymmetric_unknown_descriptor_is_retained() {
        round_trip(
            "O=C(O)[C@H](O)[CH:900001](O)[C@H](O)C(=O)O",
            "(2R,4S)-2,3,4-trihydroxypentanedioic acid",
            &["3"],
            "(2R,3ξ,4S)-2,3,4-trihydroxypentanedioic acid",
        );
    }

    #[test]
    // Provenance: fn tests::blue_book_p91_3_unknown_macrocycle | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::blue_book_p91_3_unknown_macrocycle
    fn blue_book_p91_3_unknown_macrocycle() {
        round_trip(
            "Cl[C:900002]1=[CH:900002]CCCCCCCCCC1",
            "1-chlorocyclododec-1-ene",
            &["1=2"],
            "(1Ξ)-1-chlorocyclododec-1-ene",
        );
    }

    #[test]
    // Provenance: fn tests::incomplete_explicit_forms_keep_the_unspecified_name_and_count_units | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::incomplete_explicit_forms_keep_the_unspecified_name_and_count_units
    fn incomplete_explicit_forms_keep_the_unspecified_name_and_count_units() {
        use crate::naming::{NameReason, NameStatus};
        for (input, expected, located, units, explicit) in [
            // One descriptor inside bis(...) describes both units.
            (
                "C[CH:900001](F)c1ccc([CH:900001](F)C)cc1",
                "1,4-bis(1-fluoroethyl)benzene",
                &[] as &[&str],
                2,
                None,
            ),
            // Substituent locants do not identify a position in the parent.
            (
                "C[CH:900001](F)c1ccc(C(=O)O)c([CH:900001](Cl)CC)c1",
                "2-(1-chloropropyl)-4-(1-fluoroethyl)benzoic acid",
                &[],
                2,
                Some("2-[(1Ξ)-1-chloropropyl]-4-[(1Ξ)-1-fluoroethyl]benzoic acid"),
            ),
            // The chiral functional-class form has no locant for an unknown sulfur.
            ("C[S:900001](=O)CC", "(methanesulfinyl)ethane", &[], 1, None),
        ] {
            let name = super::super::name_smiles(input, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{input}");
            assert_eq!(name.omitted_stereo, located, "{input}");
            assert_eq!(name.omitted_units, units, "{input}");
            assert_eq!(
                name.status,
                NameStatus::NotPreferred(NameReason::UnknownConfiguration)
            );
            assert_eq!(
                name.alternatives.first().map(|a| a.name.as_str()),
                explicit,
                "{input}"
            );
            for text in std::iter::once(&name.text).chain(name.alternatives.iter().map(|a| &a.name))
            {
                let request = crate::naming::local::parse_request(text).unwrap();
                let reply = crate::naming::worker::respond(&request).unwrap();
                let parsed = crate::naming::local::parse_result(text, &reply).unwrap();
                assert_eq!(
                    crate::naming::canonical_smiles(input).unwrap(),
                    crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                    "{text}"
                );
            }
        }
    }

    #[test]
    // Provenance: fn tests::ordinary_maps_do_not_request_unknown_stereochemistry | adapted
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::ordinary_maps_do_not_request_unknown_stereochemistry
    // Source: openclatura | tests/evaluations/test_check_regression.py::test_unchanged_cached_names_do_not_invoke_opsin
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn ordinary_maps_do_not_request_unknown_stereochemistry() {
        let name = super::super::name_smiles("[CH3:1][CH2:2][OH:3]", &Default::default()).unwrap();
        assert_eq!(name.text, "ethanol");
        assert!(name.omitted_stereo.is_empty());
        assert!(name.alternatives.is_empty());
    }

    #[test]
    // Provenance: fn tests::enantiomorphic_ligands_use_lowercase_unknown_descriptor | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::enantiomorphic_ligands_use_lowercase_unknown_descriptor
    fn enantiomorphic_ligands_use_lowercase_unknown_descriptor() {
        let smiles = "[CH:900001](O)(C[C@H](F)C)C[C@@H](F)C";
        let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
        assert_eq!(name.omitted_stereo, ["4"]);
        let explicit = &name.alternatives[0].name;
        assert!(explicit.contains("4ξ"), "{explicit}");
        assert!(!explicit.contains('Ξ'), "{explicit}");
        for candidate in [name.text.as_str(), explicit.as_str()] {
            let request = crate::naming::local::parse_request(candidate).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(candidate, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{candidate}"
            );
        }
    }

    #[test]
    // Provenance: fn tests::descriptor_omission_preserves_nested_groups_and_known_stereo | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::descriptor_omission_preserves_nested_groups_and_known_stereo
    fn descriptor_omission_preserves_nested_groups_and_known_stereo() {
        for (raw, omitted) in [
            ("(Ξ)-bromochlorofluoromethane", "bromochlorofluoromethane"),
            ("(Ξ)-cyclooctene", "cyclooctene"),
            ("(Ξ)-cyclononene", "cyclononene"),
            ("(2Ξ,3R)-3-chlorobutan-2-ol", "(3R)-3-chlorobutan-2-ol"),
            ("(1Ξ)-1-chlorocyclododec-1-ene", "1-chlorocyclododec-1-ene"),
            (
                "2-((2Ξ,3R)-3-chlorobutan-2-yl)benzoic acid",
                "2-((3R)-3-chlorobutan-2-yl)benzoic acid",
            ),
            (
                "(2R,4ξ,6S)-2,6-difluoroheptan-4-ol",
                "(2R,6S)-2,6-difluoroheptan-4-ol",
            ),
        ] {
            assert_eq!(super::without_unknown_descriptors(raw), omitted);
        }
    }

    #[test]
    // Provenance: fn tests::symmetric_numbering_keeps_warning_locants_in_the_displayed_name | original
    // Source: ReShiki | src/naming/openclatura/unknown_stereo.rs::tests::symmetric_numbering_keeps_warning_locants_in_the_displayed_name
    fn symmetric_numbering_keeps_warning_locants_in_the_displayed_name() {
        let smiles = "C[CH:900001](Cl)CC[C@H](Cl)C";
        let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
        let explicit = &name.alternatives[0].name;
        assert_eq!(name.text, super::without_unknown_descriptors(explicit));
        for candidate in [name.text.as_str(), explicit.as_str()] {
            let request = crate::naming::local::parse_request(candidate).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(candidate, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{candidate}"
            );
        }
    }
}
