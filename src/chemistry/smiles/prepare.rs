use super::{Error, Parsed, Result, parse_inner};
use crate::chemistry::{
    hydrogens,
    kekulize::Direction,
    sanitize,
    stereo::{self, perception, wedging::Conformer},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Prepared {
    pub state: perception::State,
    pub dummy_labels: Vec<Option<String>>,
}

/// Prepare a bare SMILES graph with default H removal, sanitization and stereo.
/// Use `read` for CX extensions and names. Drawing layout remains separate.
pub fn prepare(text: &str) -> Result<Prepared> {
    let parsed = parse_inner(text)?;
    let context = Context::new(&parsed);
    finish(parsed, context).map(|(prepared, _, _)| prepared)
}

pub(super) struct Context {
    pub unknown: Vec<bool>,
    pub annotations: hydrogens::Annotations,
    pub conformers: Vec<Conformer>,
    pub needs_bond_stereo: bool,
    pub unsupported_bonds: Vec<bool>,
    pub property_errors: Vec<Option<&'static str>>,
    pub unknown_errors: Vec<bool>,
    pub permutation_errors: Vec<bool>,
}
impl Context {
    pub fn new(parsed: &Parsed) -> Self {
        Self {
            unknown: vec![false; parsed.graph.atoms.len()],
            annotations: hydrogens::Annotations::default(),
            conformers: Vec::new(),
            needs_bond_stereo: false,
            unsupported_bonds: vec![false; parsed.graph.bonds.len()],
            property_errors: vec![None; parsed.graph.atoms.len()],
            unknown_errors: vec![false; parsed.graph.atoms.len()],
            permutation_errors: vec![false; parsed.graph.atoms.len()],
        }
    }
}

pub(super) fn finish(
    parsed: Parsed,
    mut context: Context,
) -> Result<(Prepared, Vec<Conformer>, Vec<usize>)> {
    let removed = hydrogens::before_sanitization(&hydrogens::Input {
        graph: parsed.graph,
        metadata: parsed.metadata,
        directions: parsed.directions,
        unknown_atoms: context.unknown,
        annotations: context.annotations,
    })?;
    for (index, &atom) in removed.kept_atoms.iter().enumerate() {
        if let Some(error) = context.property_errors.get(atom).ok_or(Error::Limit)? {
            return Err(Error::Unsupported(error));
        }
        if *context.permutation_errors.get(atom).ok_or(Error::Limit)?
            && removed
                .metadata
                .atoms
                .get(index)
                .ok_or(Error::Limit)?
                .chiral_tag
                >= 4
        {
            return Err(Error::Unsupported("invalid chiral permutation"));
        }
    }
    for &bond in &removed.kept_bonds {
        if *parsed.query_bonds.get(bond).ok_or(Error::Limit)? {
            return Err(Error::Unsupported("query bond"));
        }
        if *context.unsupported_bonds.get(bond).ok_or(Error::Limit)? {
            return Err(Error::Unsupported("bond type"));
        }
    }
    for conformer in &mut context.conformers {
        conformer.positions = removed
            .kept_atoms
            .iter()
            .map(|&id| conformer.positions.get(id).copied().ok_or(Error::Limit))
            .collect::<Result<Vec<_>>>()?;
    }
    let mut sanitized = hydrogens::finish_sanitization(sanitize::sanitize(
        &removed.graph,
        &removed.metadata,
        &removed.directions,
    )?)?;
    if context.needs_bond_stereo {
        let conformer = context
            .conformers
            .iter()
            .find(|c| !c.is_3d)
            .or_else(|| context.conformers.first());
        if conformer.is_some() {
            for ((bond, meta), direction) in sanitized
                .graph
                .bonds
                .iter()
                .zip(&mut sanitized.metadata.bonds)
                .zip(&mut sanitized.directions)
            {
                if bond.order == 1 {
                    meta.unknown_stereo |= *direction == Direction::Unknown;
                    *direction = Direction::None;
                }
            }
        }
        let geometry = stereo::double_bond_directions_with_bounds(
            &sanitized.graph,
            &sanitized.metadata,
            &sanitized.directions,
            conformer.map(|c| c.positions.as_slice()),
            &sanitized.rings,
            stereo::CoordinateBounds::NativeImport,
        )
        .map_err(Error::Stereo)?;
        sanitized.metadata = geometry.metadata;
        sanitized.directions = geometry.directions;
    }
    let mut properties = perception::Properties::unspecified(&sanitized.graph);
    for ((atom, unknown), &old) in properties
        .atoms
        .iter_mut()
        .zip(removed.unknown_atoms)
        .zip(&removed.kept_atoms)
    {
        atom.unknown = unknown;
        atom.invalid_unknown = *context.unknown_errors.get(old).ok_or(Error::Limit)?;
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
    Ok((
        Prepared {
            state,
            dummy_labels,
        },
        context.conformers,
        removed.kept_atoms,
    ))
}
