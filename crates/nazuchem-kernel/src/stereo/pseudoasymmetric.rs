// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | crates/model/src/chemistry/stereo_lint.rs | 82b8e6e9c0133fe31a59109ec0adc91507bc8d6f
// Changes: copy the pure full-CIP trial onto kernel State; drawing advice stays in ReShiki.

//! Pure classification of a possible tetrahedral centre by full-CIP trials.
use super::{self as stereo, perception};

/// Whether both trial configurations receive lowercase full-CIP codes.
/// Atom maps have no stereochemical meaning, so exclude them from both trials.
pub fn is_pseudoasymmetric(state: &perception::State, atom: usize) -> bool {
    [1, 2].into_iter().all(|tag| {
        let mut trial = state.clone();
        for atom in &mut trial.metadata.atoms {
            atom.map_number = 0;
            atom.map_present = false;
        }
        let Some(metadata) = trial.metadata.atoms.get_mut(atom) else {
            return false;
        };
        metadata.chiral_tag = tag;
        let Some(properties) = trial.properties.atoms.get_mut(atom) else {
            return false;
        };
        properties.unknown = false;
        trial.properties.done = None;
        stereo::cip::label::assign(&trial, &stereo::cip::label::Options::default())
            .ok()
            .is_some_and(|assignment| {
                assignment
                    .state
                    .properties
                    .atoms
                    .get(atom)
                    .and_then(|atom| atom.cip_code.as_deref())
                    .is_some_and(|code| matches!(code, "r" | "s"))
            })
    })
}
