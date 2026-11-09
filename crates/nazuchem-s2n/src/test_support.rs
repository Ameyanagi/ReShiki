// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | src/naming/local.rs | 4460ac34c3ad6bf79358c4e4ae07103f69e5f42d
// Source: ReShiki | src/naming/worker.rs | 4460ac34c3ad6bf79358c4e4ae07103f69e5f42d
// Changes: Replace application worker round trips with direct OPSIN parsing and kernel identity checks for library tests.

//! Test-only equivalents of strict import and generated-name verification.
//!
//! Both modes start with OPSIN's strict options and reject all OPSIN warnings,
//! which is stricter than ReShiki's import policy. They perform no symbol or
//! retained-name expansion. Generated-name verification additionally permits
//! radicals and a single relative-stereo CX group. No worker protocol or
//! document types cross the library boundary.

use crate::identity::{canonical_state, relative_stereo_smiles};
use nazuchem_ir::NameStatus;
use nazuchem_kernel::{budget::Budget, smiles};

pub(crate) fn canonical_smiles(text: &str) -> Result<String, String> {
    crate::canonical_smiles(text, &Budget::default())
}

pub(crate) fn verify_identity(expected: &str, actual: &str) -> Result<(), String> {
    crate::verify_identity(expected, actual, &Budget::default())
}

pub(crate) fn verify_alternative_identity(
    expected: &str,
    actual: &str,
    status: NameStatus,
) -> Result<(), String> {
    crate::verify_alternative_identity(expected, actual, status, &Budget::default())
}

pub(crate) fn parse_strict(name: &str) -> Result<String, String> {
    parse(name, false)
}

pub(crate) fn parse_generated(name: &str) -> Result<String, String> {
    parse(name, true)
}

fn parse(name: &str, generation: bool) -> Result<String, String> {
    if name.is_empty() || name.len() > 2_048 || name.chars().any(char::is_control) {
        return Err("Chemical names must contain 1–2048 bytes without control characters".into());
    }
    let budget = Budget::default();
    let parser = opsin::Parser::with_budget(budget.clone()).map_err(|error| error.to_string())?;
    let mut options = opsin::ParseOptions::strict();
    options.allow_radicals = generation;
    let result = parser.parse(name, &options);
    if result.status != opsin::Status::Success || !result.warnings.is_empty() {
        return Err(format!(
            "OPSIN could not interpret {name} unambiguously: {} {:?}",
            result.message, result.warnings
        ));
    }
    let (mut text, locants, axes) = result
        .structure
        .ok_or("OPSIN returned no molecular structure")?
        .semantic_cxsmiles_with_locants_and_axes()
        .map_err(|error| error.to_string())?;
    let relative = generation
        .then(|| relative_stereo_smiles(&text))
        .flatten()
        .map(str::to_owned);
    let graph = relative.as_deref().unwrap_or(&text);
    let canonical = crate::canonical_smiles(graph, &budget)?;
    let atom_count = smiles::parse(graph)
        .map_err(|error| error.to_string())?
        .graph
        .atoms
        .len();
    if locants.len() != atom_count {
        return Err("OPSIN returned locants for a different atom count".into());
    }
    if !axes.is_empty() {
        let mut state = smiles::read(&text, &budget)
            .map_err(|error| error.to_string())?
            .prepared
            .state;
        if state.graph.atoms.len() != atom_count {
            return Err("Native import changed the atom count of an axial structure".into());
        }
        let invalid = || "OPSIN returned an invalid axis".to_owned();
        let mut descriptors = Vec::new();
        for axis in &axes {
            let [a, b] = axis.atoms;
            let index = state
                .graph
                .bonds
                .iter()
                .position(|bond| (bond.a, bond.b) == (a, b) || (bond.a, bond.b) == (b, a))
                .ok_or_else(invalid)?;
            let bond = state.graph.bonds.get(index).ok_or_else(invalid)?;
            let mut refs = axis.refs;
            if bond.a != a {
                refs.reverse();
            }
            let mut clockwise = axis.clockwise;
            let mut carriers = Vec::new();
            for ((atom, other), reference) in
                [(bond.a, bond.b), (bond.b, bond.a)].into_iter().zip(refs)
            {
                let neighbours: Vec<_> = state
                    .graph
                    .bonds
                    .iter()
                    .filter_map(|bond| match (bond.a, bond.b) {
                        (x, y) | (y, x) if x == atom && y != other => Some(y),
                        _ => None,
                    })
                    .collect();
                let carrier = *neighbours.iter().min().ok_or_else(invalid)?;
                if neighbours.len() != 2 || !neighbours.contains(&reference) {
                    return Err(invalid());
                }
                clockwise ^= carrier != reference;
                carriers.push(carrier);
            }
            let metadata = state.metadata.bonds.get_mut(index).ok_or_else(invalid)?;
            if metadata.stereo != 0 {
                return Err(invalid());
            }
            metadata.stereo = if clockwise { 6 } else { 7 };
            metadata.stereo_atoms = carriers;
            descriptors.push((index, axis.descriptor.as_str()));
        }
        let labelled = nazuchem_kernel::stereo::cip::label::assign(&state, &Default::default())
            .map_err(|error| error.to_string())?;
        if descriptors.iter().any(|&(index, descriptor)| {
            labelled
                .state
                .properties
                .bond_codes
                .get(index)
                .and_then(Option::as_deref)
                != Some(descriptor)
        }) {
            return Err("Native and OPSIN CIP assignments disagree on the biaryl axis".into());
        }
        text = canonical_state(&state, &budget)?;
    }
    if !generation
        && smiles::prepare(&canonical, &budget)
            .map_err(|error| error.to_string())?
            .state
            .graph
            .atoms
            .iter()
            .any(|atom| atom.radical_electrons != 0)
    {
        return Err("Importing radical names is not supported by the strict parser".into());
    }
    // Match the application's stable positive-component ordering while keeping
    // the atom indices of relative CX groups intact.
    if relative.is_none() && text.contains('.') {
        let mut components = text
            .split('.')
            .map(|part| {
                let graph = smiles::parse(part)
                    .map_err(|error| error.to_string())?
                    .graph;
                let charge: i32 = graph.atoms.iter().map(|atom| i32::from(atom.charge)).sum();
                Ok((charge <= 0, part))
            })
            .collect::<Result<Vec<_>, String>>()?;
        components.sort_by_key(|(nonpositive, _)| *nonpositive);
        text = components
            .into_iter()
            .map(|(_, part)| part)
            .collect::<Vec<_>>()
            .join(".");
    }
    Ok(text)
}

#[test]
fn strict_and_generated_parsing_keep_radical_modes_separate() {
    assert!(parse_strict("methyl").is_err());
    let parsed = parse_generated("methyl").unwrap();
    verify_identity("[CH3]", &parsed).unwrap();
}

#[test]
fn generated_relative_stereo_keeps_the_semantic_group() {
    let name = "5-endo,7-anti-dimethylbicyclo[2.2.1]heptan-2-exo-ol";
    assert!(parse_strict(name).is_err());
    let parsed = parse_generated(name).unwrap();
    assert!(relative_stereo_smiles(&parsed).is_some());
    verify_alternative_identity(
        "O[C@H]1C[C@@H]2[C@H](C)C[C@H]1[C@H]2C",
        &parsed,
        NameStatus::NotPreferred(nazuchem_ir::NameReason::EndoExoStereo),
    )
    .unwrap();
}
