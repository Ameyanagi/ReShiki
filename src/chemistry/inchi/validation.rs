//! Structural checks for serialized molecules, without chemical preparation.
use super::input::Error;
use crate::chemistry::stereo::{
    Point3,
    perception::{RingKind, State},
};
use std::collections::HashSet;

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

pub(super) fn molecule(state: &State, positions: Option<&[Point3]>) -> Result<(), Error> {
    let (atoms, bonds) = (state.graph.atoms.len(), state.graph.bonds.len());
    if atoms > i16::MAX as usize {
        return Err(Error::Limit("the signed 16-bit input count range"));
    }
    state
        .graph
        .cached_valences(Some(&state.valences))
        .map_err(invalid)?;
    state.metadata.validate(&state.graph).map_err(invalid)?;
    if state.directions.len() != bonds
        || state.conjugated.len() != bonds
        || state.hybridizations.len() != atoms
        || state.properties.atoms.len() != atoms
        || state.properties.bond_codes.len() != bonds
    {
        return Err(invalid("Molecular annotation dimensions changed"));
    }
    if positions.is_some_and(|p| {
        p.len() != atoms
            || p.iter()
                .any(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite())
    }) {
        return Err(invalid("Invalid molecular coordinates"));
    }
    if state.rings.kind == RingKind::None && !state.rings.atoms.is_empty() {
        return Err(invalid("Uninitialized ring cache contains cycles"));
    }
    let edges: HashSet<_> = state
        .graph
        .bonds
        .iter()
        .map(|b| (b.a.min(b.b), b.a.max(b.b)))
        .collect();
    let mut stored = 0usize;
    for ring in &state.rings.atoms {
        stored = stored
            .checked_add(ring.len())
            .ok_or_else(|| invalid("Ring storage overflow"))?;
        if stored > 2_000_000
            || ring.len() < 3
            || ring.iter().any(|&a| a >= atoms)
            || ring.iter().collect::<HashSet<_>>().len() != ring.len()
            || ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .any(|(&a, &b)| !edges.contains(&(a.min(b), a.max(b))))
        {
            return Err(invalid("Invalid molecular ring cache"));
        }
    }
    let mut members = 0usize;
    for properties in &state.properties.atoms {
        if let Some(ring_members) = &properties.ring_members {
            members = members
                .checked_add(ring_members.len())
                .ok_or_else(|| invalid("Stereo member storage overflow"))?;
            if members > 2_000_000
                || ring_members
                    .iter()
                    .any(|&a| a == 0 || a.unsigned_abs() as usize > atoms)
            {
                return Err(invalid("Invalid stereo ring members"));
            }
        }
    }
    Ok(())
}
