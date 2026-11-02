//! Explicitly unknown stereogenic units transported in private atom maps.
//! The structure used for identity omits only these units' configurations.
use super::{Name, graph_io, molecule::Molecule};
use crate::naming::{AlternativeName, NameReason, NameStatus};
use reshiki_chemistry::stereo::{cip::label, perception};

const UNKNOWN_ATOM: i32 = 900_001;
const UNKNOWN_BOND: i32 = 900_002;
const UNKNOWN_BOTH: i32 = 900_003;

fn atom_marker(map: i32) -> bool {
    matches!(map, UNKNOWN_ATOM | UNKNOWN_BOTH)
}

fn bond_marker(map: i32) -> bool {
    matches!(map, UNKNOWN_BOND | UNKNOWN_BOTH)
}

/// Attach unknown descriptors after the ordinary graph/CIP import. A detached
/// full CIP trial distinguishes pseudoasymmetric ξ from ordinary Ξ wherever
/// the remaining specified stereochemistry makes that distinction possible.
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
        let pseudo = [1, 2].into_iter().all(|tag| {
            let mut trial = state.clone();
            for atom in &mut trial.metadata.atoms {
                atom.map_number = 0;
                atom.map_present = false;
            }
            trial.metadata.atoms[idx].chiral_tag = tag;
            trial.properties.atoms[idx].unknown = false;
            trial.properties.done = None;
            label::assign(&trial, &label::Options::default())
                .ok()
                .and_then(|assignment| assignment.state.properties.atoms[idx].cip_code.clone())
                .is_some_and(|code| matches!(code.as_str(), "r" | "s"))
        });
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
pub(super) fn name_smiles(smiles: &str) -> Result<Option<Name>, String> {
    if ![":900001]", ":900002]", ":900003]"]
        .iter()
        .any(|marker| smiles.contains(marker))
    {
        return Ok(None);
    }
    // Keep the ordinary size and supported-semantics gate ahead of parsing
    // the internal marker transport, just as in graph_io::read_smiles.
    let unspecified = crate::naming::canonical_smiles(smiles)?;
    let mut normal = super::name_smiles(&unspecified)?;
    let status = normal.status;
    normal.status = NameStatus::NotPreferred(NameReason::UnknownConfiguration);
    let mol = graph_io::read_smiles(smiles)?;
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
    // Offer the Ξ/ξ name only when it writes one locanted descriptor per
    // unknown unit (P-91.2.1.2.1). Otherwise (a multiplied substituent, a
    // functional-class name) the unspecified name stands alone and the note
    // counts the omitted units.
    let locanted = |raw: &str| {
        raw.chars()
            .zip(raw.chars().skip(1))
            .filter(|&(locant, descriptor)| {
                matches!(descriptor, 'Ξ' | 'ξ')
                    && (locant.is_ascii_alphanumeric() || locant == '\'')
            })
            .count()
    };
    let Some(explicit_raw) = graph_io::get_connected_components(&mol)
        .first()
        .and_then(|component| super::component_namer::name_component(&mol, component).ok())
        .filter(|raw| locanted(raw) == normal.omitted_units)
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
            status,
        },
    );
    Ok(Some(normal))
}

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
    fn round_trip(input: &str, expected: &str, omitted: &[&str], explicit: &str) {
        let name = super::super::name_smiles(input).unwrap();
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
    fn wavy_tetrahedral_unit_retains_the_known_neighbour() {
        round_trip(
            "C[CH:900001](O)[C@H](Cl)C",
            "(3R)-3-chlorobutan-2-ol",
            &["2"],
            "(2Ξ,3R)-3-chlorobutan-2-ol",
        );
    }

    #[test]
    fn either_double_bond_names_the_unspecified_unit() {
        round_trip(
            "C[CH:900002]=[CH:900002]C",
            "but-2-ene",
            &["2=3"],
            "(2Ξ)-but-2-ene",
        );
    }

    #[test]
    fn blue_book_p91_3_unknown_macrocycle() {
        round_trip(
            "Cl[C:900002]1=[CH:900002]CCCCCCCCCC1",
            "1-chlorocyclododec-1-ene",
            &["1=2"],
            "(1Ξ)-1-chlorocyclododec-1-ene",
        );
    }

    #[test]
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
            let name = super::super::name_smiles(input).unwrap();
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
    fn ordinary_maps_do_not_request_unknown_stereochemistry() {
        let name = super::super::name_smiles("[CH3:1][CH2:2][OH:3]").unwrap();
        assert_eq!(name.text, "ethanol");
        assert!(name.omitted_stereo.is_empty());
        assert!(name.alternatives.is_empty());
    }

    #[test]
    fn enantiomorphic_ligands_use_lowercase_unknown_descriptor() {
        let smiles = "[CH:900001](O)(C[C@H](F)C)C[C@@H](F)C";
        let name = super::super::name_smiles(smiles).unwrap();
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
    fn descriptor_omission_preserves_nested_groups_and_known_stereo() {
        for (raw, omitted) in [
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
    fn symmetric_numbering_keeps_warning_locants_in_the_displayed_name() {
        let smiles = "C[CH:900001](Cl)CC[C@H](Cl)C";
        let name = super::super::name_smiles(smiles).unwrap();
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
