use super::{Assembly, Error, Output, assemble, at, clean_up, validate_assembly};
use crate::chemistry::{
    hydrogens, sanitize,
    stereo::perception::{self, Properties, RingCache, RingKind, State},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub sanitize: bool,
    pub remove_hydrogens: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            sanitize: true,
            remove_hydrogens: true,
        }
    }
}

/// Apply the original optional RemoveHs/sanitization stage to a cleaned graph.
/// `remove_hydrogens` has no effect when sanitization is disabled.
pub fn prepare(input: &Assembly, options: Options) -> Result<Assembly, Error> {
    validate_assembly(input)?;
    if !options.sanitize {
        return Ok(input.clone());
    }
    let mut result = input.clone();
    let Some(mut state) = result.state.take() else {
        return Ok(result);
    };
    let mut unknown = state
        .properties
        .atoms
        .iter()
        .map(|a| a.unknown)
        .collect::<Vec<_>>();
    if options.remove_hydrogens {
        // Empty RemoveHs clears computed properties but does not sanitize.
        if state.graph.atoms.is_empty() {
            state.properties = Properties::unspecified(&state.graph);
            result.state = Some(state);
            return Ok(result);
        }
        let removed = hydrogens::before_sanitization(&hydrogens::Input {
            graph: state.graph,
            metadata: state.metadata,
            directions: state.directions,
            unknown_atoms: unknown,
            annotations: hydrogens::Annotations::default(),
        })?;
        result.unspecified_bonds = removed
            .kept_bonds
            .iter()
            .map(|&b| at(&result.unspecified_bonds, b).copied())
            .collect::<Result<_, _>>()?;
        state.graph = removed.graph;
        state.metadata = removed.metadata;
        state.directions = removed.directions;
        unknown = removed.unknown_atoms;
    }
    let mut prepared = sanitize::sanitize(&state.graph, &state.metadata, &state.directions)?;
    if options.remove_hydrogens {
        prepared = hydrogens::finish_sanitization(prepared)?;
    }
    let mut properties = Properties::unspecified(&prepared.graph);
    for (p, unknown) in properties.atoms.iter_mut().zip(unknown) {
        p.unknown = unknown;
    }
    result.state = Some(State {
        graph: prepared.graph,
        metadata: prepared.metadata,
        directions: prepared.directions,
        valences: prepared.valences,
        conjugated: prepared.conjugated,
        hybridizations: prepared.hybridizations,
        rings: RingCache {
            kind: RingKind::Symmetric,
            atoms: prepared.rings,
        },
        properties,
    });
    Ok(result)
}

/// Reconstruct the complete native InchiToMol result from detached C-kernel
/// records. Kernel status/message/log remain in `output`; a native null result
/// is distinct from malformed records, stage failures, and resource errors.
/// Forced legacy stereo perception always runs, even without sanitization.
/// This adapter does not assign full CIP labels or generate drawing geometry.
/// The pinned kernel emits one 0D record per stereobond. Injected duplicate
/// records remain intact in `assemble`, but later stages return the explicit
/// `RepeatedStereoRecords` boundary: shared molecular metadata allows two
/// controlling atoms, whereas the native adapter can temporarily accumulate
/// more. No references are silently truncated.
pub fn reconstruct(output: &Output, options: Options) -> Result<Assembly, Error> {
    let assembled = assemble(output)?;
    let cleaned = clean_up(&assembled)?;
    let mut prepared = prepare(&cleaned, options)?;
    if let Some(state) = prepared.state.take() {
        prepared.state = Some(
            perception::perceive(
                &state,
                perception::Options {
                    clean: true,
                    force: true,
                    flag_possible: false,
                },
            )
            .map_err(Error::Chemistry)?,
        );
    }
    Ok(prepared)
}
