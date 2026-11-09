//! Retry generated wedges only when the reference drawing loses stereo.
use super::super::{Choice, Conformer, Context, WedgeProperties, WedgeState, Work, can_direct};
use super::{at, at_mut, from_directions, prefer_ring_wedges, tetrahedral, wedged};
use crate::{
    electronic::Hybridization,
    kekulize::Direction,
    ranking::Metadata,
    stereo::{atropisomers, drawing::detect_atropisomers},
};

#[cfg(test)]
mod tests;

// A repair is local, but each readback traverses the whole molecule. Allow
// 256 trials (including second wedges) to cover small coupled cages without
// making interactive import an unbounded combinatorial search.
const READBACK_LIMIT: usize = 256;

/// The normal generated drawing is authoritative whenever it reads back.
/// Failed or exhausted searches retain it: import owns the refusal policy,
/// while other callers may discard or restore these wedges afterwards.
pub fn wedge_molecule(
    input: &WedgeState,
    properties: &WedgeProperties,
    conformer: &Conformer,
    two_bonds: bool,
) -> Result<WedgeState, String> {
    let mut work = Work(50_000_000);
    let mut ctx =
        super::super::with_context(input, properties, Some(conformer), two_bonds, &mut work)?;
    let reference = prefer_ring_wedges(&ctx.state, properties, conformer)?;
    if conformer.is_3d
        || !input
            .metadata
            .bonds
            .iter()
            .any(|b| matches!(b.stereo, 6 | 7))
    {
        return Ok(reference);
    }
    if preserves_stereo(&reference, conformer, &mut work).unwrap_or(false) {
        return Ok(reference);
    }
    // Preserve the reference's second wedges and ring conventions outside
    // the centres that can interact with an axis.
    ctx.state = reference.clone();
    let mut readbacks = READBACK_LIMIT;
    Ok(
        search(input, &mut ctx, two_bonds, &mut readbacks, &mut work)
            .ok()
            .flatten()
            .unwrap_or(reference),
    )
}

fn read_stereo(
    state: &WedgeState,
    conformer: &Conformer,
    work: &mut Work,
) -> Result<Metadata, String> {
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    let rings: usize = state.rings.atoms.iter().map(Vec::len).sum();
    let doubles = state.graph.bonds.iter().filter(|b| b.order == 2).count();
    // Readback repeats validation, valences, electronic perception and
    // metadata copies. Allene perception also scans all bonds per candidate;
    // the number of double bonds bounds those candidates. Precharge both
    // costs before reconstructing the drawing's centres and axes.
    work.spend(
        n.saturating_add(e)
            .saturating_add(rings)
            .saturating_mul(64)
            .saturating_add(doubles.saturating_mul(e).saturating_mul(16)),
    )?;
    let drawn = from_directions(
        &state.graph,
        &Metadata::unspecified(&state.graph),
        &state.directions,
        Some(&conformer.positions),
        true,
    )?;
    let seen = detect_atropisomers(
        &drawn.graph,
        &drawn.metadata,
        &state.directions,
        Some(conformer),
    )
    .map_err(|error| error.to_string())?;
    // Match drawing preparation: provisional axes in small rings are not
    // atropisomers. Recompute hybridizations from the cleared readback too.
    atropisomers(
        &drawn.graph,
        &seen,
        &vec![Hybridization::Unspecified; drawn.graph.atoms.len()],
        &state.rings.atoms,
    )
}

fn preserves_stereo(
    state: &WedgeState,
    conformer: &Conformer,
    work: &mut Work,
) -> Result<bool, String> {
    let seen = read_stereo(state, conformer, work)?;
    Ok(state
        .metadata
        .atoms
        .iter()
        .zip(&seen.atoms)
        .all(|(expected, actual)| {
            if expected.chiral_tag == 5 {
                actual.chiral_tag == 5 && expected.chiral_permutation == actual.chiral_permutation
            } else {
                !tetrahedral(expected.chiral_tag) || expected.chiral_tag == actual.chiral_tag
            }
        })
        && state
            .metadata
            .bonds
            .iter()
            .zip(&seen.bonds)
            .all(|(expected, actual)| {
                if matches!(expected.stereo, 6 | 7) || matches!(actual.stereo, 6 | 7) {
                    expected.stereo == actual.stereo
                } else {
                    true
                }
            }))
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Wedge {
    bond: usize,
    atom: usize,
    direction: Direction,
}

fn search(
    input: &WedgeState,
    ctx: &mut Context<'_>,
    two_bonds: bool,
    readbacks: &mut usize,
    work: &mut Work,
) -> Result<Option<WedgeState>, String> {
    // A centre can lack a reference choice when earlier centres claimed all
    // its carriers. Include every tetrahedral centre except fixed input ones.
    let mut eligible: Vec<_> = input
        .metadata
        .atoms
        .iter()
        .map(|atom| tetrahedral(atom.chiral_tag))
        .collect();
    for (bond, &direction) in input.graph.bonds.iter().zip(&input.directions) {
        work.spend(1)?;
        if wedged(direction) || direction == Direction::Unknown {
            // Match the reference chooser's fixed-input convention.
            let atom = if tetrahedral(at(&input.metadata.atoms, bond.a)?.chiral_tag) {
                bond.a
            } else {
                bond.b
            };
            *at_mut(&mut eligible, atom)? = false;
        }
    }
    let mut originals = vec![None; input.graph.atoms.len()];
    for (&bond, &choice) in &ctx.choices {
        if let Choice::Atom(atom) = choice {
            *at_mut(&mut originals, atom)? = Some(bond);
        }
    }
    let mut centers: Vec<_> = eligible
        .iter()
        .zip(originals)
        .enumerate()
        .filter_map(|(atom, (&eligible, original))| eligible.then_some((atom, original)))
        .collect();
    let mut axis_ends = vec![false; input.graph.atoms.len()];
    for (bond, meta) in input.graph.bonds.iter().zip(&input.metadata.bonds) {
        work.spend(1)?;
        if matches!(meta.stereo, 6 | 7) {
            *at_mut(&mut axis_ends, bond.a)? = true;
            *at_mut(&mut axis_ends, bond.b)? = true;
        }
    }
    let mut movable = vec![false; input.graph.atoms.len()];
    let mut pending = Vec::new();
    for &(atom, _) in &centers {
        for &bond in at(&ctx.adjacent, atom)? {
            work.spend(1)?;
            if *at(&axis_ends, ctx.other(bond, atom)?)? {
                *at_mut(&mut movable, atom)? = true;
                pending.push(atom);
                break;
            }
        }
    }
    // Bonded centres compete for the same carrier. Close over these centres
    // (including the fourth tetrahedrane vertex), never disconnected ones.
    while let Some(atom) = pending.pop() {
        for &bond in at(&ctx.adjacent, atom)? {
            work.spend(1)?;
            let other = ctx.other(bond, atom)?;
            if *at(&eligible, other)? && !*at(&movable, other)? {
                *at_mut(&mut movable, other)? = true;
                pending.push(other);
            }
        }
    }
    centers.retain(|&(atom, _)| movable.get(atom).copied().unwrap_or(false));
    let center_count = centers.len();
    let reference = read_stereo(&ctx.state, ctx.conf, work)?;
    let mut axes = Vec::new();
    let mut movable_axis_ends = vec![false; input.graph.atoms.len()];
    for (axis, metadata) in input.metadata.bonds.iter().enumerate() {
        work.spend(1)?;
        if !matches!(metadata.stereo, 6 | 7) {
            continue;
        }
        let edge = at(&input.graph.bonds, axis)?;
        let mut affected = at(&reference.bonds, axis)?.stereo != metadata.stereo;
        for atom in [edge.a, edge.b] {
            for &bond in at(&ctx.adjacent, atom)? {
                work.spend(1)?;
                affected |= *at(&movable, ctx.other(bond, atom)?)?;
            }
        }
        if affected {
            axes.push(axis);
            *at_mut(&mut movable_axis_ends, edge.a)? = true;
            *at_mut(&mut movable_axis_ends, edge.b)? = true;
        }
    }
    // Keep input wedges and allene carriers fixed. The optional second centre
    // wedge may have added more than one movable bond. Valid axes outside
    // the repair retain their reference carriers and add no search choices.
    for (bond, edge) in ctx.state.graph.bonds.iter_mut().enumerate() {
        work.spend(1)?;
        let original = *at(&input.directions, bond)?;
        if original != Direction::None {
            let fixed = at(&input.graph.bonds, bond)?;
            // Ring presentation may itself have changed an input wedge.
            // In that case retain the reference rather than repair around
            // a changed input or alter an unrelated reference centre.
            if original != *at(&ctx.state.directions, bond)?
                || (edge.a, edge.b) != (fixed.a, fixed.b)
            {
                return Ok(None);
            }
        } else if (*at(&movable, edge.a)?
            || (*at(&movable_axis_ends, edge.a)?
                && matches!(ctx.choices.get(&bond), Some(Choice::Atrop(_)))))
            && wedged(*at(&ctx.state.directions, bond)?)
        {
            *at_mut(&mut ctx.state.directions, bond)? = Direction::None;
            *edge = at(&input.graph.bonds, bond)?.clone();
        }
    }
    let mut alternatives = Vec::new();
    for (atom, original) in centers {
        let mut bonds = Vec::new();
        for &bond in at(&ctx.adjacent, atom)? {
            work.spend(1)?;
            bonds.push((
                *at(&axis_ends, ctx.other(bond, atom)?)?,
                Some(bond) != original,
                bond,
            ));
        }
        // Try away from axis ends first, but retain every candidate: crowded
        // cages may need these bonds to give each centre its own wedge.
        bonds.sort_unstable();
        let mut choices = Vec::new();
        for (_, _, bond) in bonds {
            work.spend(1)?;
            if at(&ctx.state.graph.bonds, bond)?.order != 1
                || *at(&ctx.state.directions, bond)? != Direction::None
            {
                continue;
            }
            let direction = ctx.wedge_direction(bond, atom, work)?;
            if wedged(direction) {
                choices.push(Some(Wedge {
                    bond,
                    atom,
                    direction,
                }));
            }
        }
        alternatives.push(choices);
    }
    for axis in axes {
        work.spend(1)?;
        let edge = at(&input.graph.bonds, axis)?;
        // A centre or fixed input wedge may already define this axis. Both
        // directions must be tried because another axis can share a carrier.
        let mut choices = vec![None];
        for atom in [edge.a, edge.b] {
            for &bond in at(&ctx.adjacent, atom)? {
                work.spend(1)?;
                if bond == axis
                    || !can_direct(at(&ctx.state.graph.bonds, bond)?.order)
                    || *at(&ctx.state.directions, bond)? != Direction::None
                {
                    continue;
                }
                for direction in [Direction::Wedge, Direction::Hash] {
                    choices.push(Some(Wedge {
                        bond,
                        atom,
                        direction,
                    }));
                }
            }
        }
        alternatives.push(choices);
    }
    work.spend(ctx.state.graph.atoms.len() + ctx.state.graph.bonds.len())?;
    let base = ctx.state.clone();
    let mut next = vec![0; alternatives.len()];
    let mut placed: Vec<Option<Wedge>> = vec![None; alternatives.len()];
    let mut depth = 0;
    let conformer = ctx.conf;
    let mut check = |state: &WedgeState, work: &mut Work| {
        *readbacks = readbacks
            .checked_sub(1)
            .ok_or("Atropisomer wedging readback limit exceeded")?;
        preserves_stereo(state, conformer, work)
    };
    // Iterative backtracking avoids a call-stack limit on large molecules.
    loop {
        work.spend(1)?;
        if depth == alternatives.len() {
            if check(&ctx.state, work)? {
                if two_bonds {
                    for wedge in placed.iter().take(center_count).flatten() {
                        work.spend(ctx.state.graph.atoms.len() + ctx.state.graph.bonds.len())?;
                        let primary = ctx.state.clone();
                        ctx.second_wedge(wedge.bond, work)?;
                        if ctx.state.directions != primary.directions && !check(&ctx.state, work)? {
                            ctx.state = primary;
                        }
                    }
                }
                return Ok(Some(ctx.state.clone()));
            }
        } else if let Some(&choice) = at(&alternatives, depth)?.get(*at(&next, depth)?) {
            *at_mut(&mut next, depth)? += 1;
            if let Some(wedge) = choice {
                if *at(&ctx.state.directions, wedge.bond)? != Direction::None {
                    continue;
                }
                *at_mut(&mut ctx.state.directions, wedge.bond)? = wedge.direction;
                ctx.orient(wedge.bond, wedge.atom)?;
            }
            *at_mut(&mut placed, depth)? = choice;
            depth += 1;
            if depth < alternatives.len() {
                *at_mut(&mut next, depth)? = 0;
            }
            continue;
        }
        if depth == 0 {
            return Ok(None);
        }
        depth -= 1;
        if let Some(wedge) = *at(&placed, depth)? {
            *at_mut(&mut ctx.state.directions, wedge.bond)? = Direction::None;
            *at_mut(&mut ctx.state.graph.bonds, wedge.bond)? =
                at(&base.graph.bonds, wedge.bond)?.clone();
        }
    }
}
