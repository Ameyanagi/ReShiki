use super::{Error, Result, parse_inner};
use crate::chemistry::{hydrogens, sanitize, stereo::perception};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Prepared {
    pub state: perception::State,
    pub dummy_labels: Vec<Option<String>>,
}

/// Prepare a bare SMILES graph with default H removal, sanitization and stereo.
/// CX extensions, names and drawing layout are separate, unfinished stages.
pub fn prepare(text: &str) -> Result<Prepared> {
    let parsed = parse_inner(text)?;
    let count = parsed.graph.atoms.len();
    let removed = hydrogens::before_sanitization(&hydrogens::Input {
        graph: parsed.graph,
        metadata: parsed.metadata,
        directions: parsed.directions,
        unknown_atoms: vec![false; count],
        annotations: hydrogens::Annotations::default(),
    })?;
    for &bond in &removed.kept_bonds {
        if *parsed.query_bonds.get(bond).ok_or(Error::Limit)? {
            return Err(Error::Unsupported("query bond"));
        }
    }
    let sanitized = hydrogens::finish_sanitization(sanitize::sanitize(
        &removed.graph,
        &removed.metadata,
        &removed.directions,
    )?)?;
    let mut properties = perception::Properties::unspecified(&sanitized.graph);
    for (atom, unknown) in properties.atoms.iter_mut().zip(removed.unknown_atoms) {
        atom.unknown = unknown;
    }
    let state = perception::perceive(
        &perception::State {
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
        },
        perception::Options {
            clean: true,
            force: true,
            flag_possible: true,
        },
    )
    .map_err(Error::Stereo)?;
    let dummy_labels = removed
        .kept_atoms
        .iter()
        .map(|&i| parsed.dummy_labels.get(i).cloned().ok_or(Error::Limit))
        .collect::<Result<Vec<_>>>()?;
    Ok(Prepared {
        state,
        dummy_labels,
    })
}
