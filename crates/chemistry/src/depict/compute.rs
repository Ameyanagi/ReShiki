//! RDKit 2026.03.6 RDDepictor.cpp compute2DCoords orchestration.
//! BSD-3-Clause; see licenses/rdkit/NOTICE.
use super::{collision, expansion, finalize, geometry::Coordinates, ranks, rings, seeds};
use crate::stereo::{perception::State, wedging::Conformer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Depiction requires a finite positive bond length")]
    BondLength,
    #[error("Depiction work limit exceeded")]
    Limit,
    #[error(transparent)]
    Initial(#[from] expansion::Error),
    #[error(transparent)]
    Collision(#[from] collision::Error),
    #[error(transparent)]
    Finalize(#[from] finalize::Error),
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub bond_length: f64,
    pub canonical_orientation: bool,
    pub use_ring_templates: bool,
    pub work_limit: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            bond_length: 1.5,
            canonical_orientation: true,
            use_ring_templates: false,
            work_limit: expansion::MAX_WORK,
        }
    }
}

/// Return one new 2D conformer (ID zero when replacing existing conformers).
/// Chemical state and caller coordinates stay unchanged on success or failure.
///
/// The application uses deterministic layout: no random sampling or CoordGen.
/// Coordination seeds use this request's bond length. Unlike the native static
/// seed caches, a previous request cannot affect a later drawing's style.
pub fn compute(
    state: &State,
    chiral_ranks: &[Option<u32>],
    coordinates: Option<&Coordinates>,
    options: Options,
) -> Result<Conformer, Error> {
    let mut remaining = options.work_limit.min(expansion::MAX_WORK);
    compute_inner(
        state,
        ranks::Input::Numeric(chiral_ranks),
        coordinates,
        options,
        &mut remaining,
    )
}

/// Consume the caller's aggregate allowance, including work spent before an
/// error. The per-call option is a ceiling, never a fresh allowance.
pub fn compute_with_work(
    state: &State,
    chiral_ranks: &[Option<u32>],
    coordinates: Option<&Coordinates>,
    options: Options,
    remaining: &mut usize,
) -> Result<Conformer, Error> {
    let allowance = (*remaining)
        .min(options.work_limit)
        .min(expansion::MAX_WORK);
    let mut available = allowance;
    let result = compute_inner(
        state,
        ranks::Input::Numeric(chiral_ranks),
        coordinates,
        options,
        &mut available,
    );
    let used = allowance.checked_sub(available).ok_or(Error::Limit)?;
    *remaining = remaining.checked_sub(used).ok_or(Error::Limit)?;
    result
}

/// Retain native property-conversion failures until a selected ordering reads
/// the property. Generated CIP ranks take precedence over literal CX values.
pub fn compute_with_rank_properties(
    state: &State,
    properties: &[ranks::AtomProperties],
    coordinates: Option<&Coordinates>,
    options: Options,
) -> Result<Conformer, Error> {
    let mut remaining = options.work_limit.min(expansion::MAX_WORK);
    compute_inner(
        state,
        ranks::Input::Properties(properties),
        coordinates,
        options,
        &mut remaining,
    )
}

fn compute_inner(
    state: &State,
    rank_properties: ranks::Input<'_>,
    coordinates: Option<&Coordinates>,
    options: Options,
    remaining: &mut usize,
) -> Result<Conformer, Error> {
    if !options.bond_length.is_finite() || options.bond_length <= 0.0 {
        return Err(Error::BondLength);
    }
    if *remaining == 0 {
        return Err(Error::Limit);
    }
    let mut initial = expansion::compute_initial_ranks_with_work(
        state,
        rank_properties,
        coordinates,
        expansion::Options {
            bond_length: options.bond_length,
            ideal_lengths: seeds::IdealLengths {
                square_planar: options.bond_length,
                trigonal_bipyramidal: options.bond_length,
                octahedral: options.bond_length,
            },
            use_ring_templates: options.use_ring_templates,
        },
        remaining,
    )?;
    // RDKit has no work cap. When ours stops collision repair, keep the
    // unrepaired layout rather than failing; the abandoned repair is not charged.
    let before = *remaining;
    match repair(&initial, remaining) {
        Ok(fragments) => initial.fragments = fragments,
        Err(collision::Error::Limit) => *remaining = before,
        Err(error) => return Err(error.into()),
    }
    let result = finalize::finish_with_work(
        state.graph.atoms.len(),
        &initial.fragments,
        coordinates,
        finalize::Options {
            canonical_orientation: options.canonical_orientation,
            work_limit: options.work_limit,
        },
        remaining,
    )?;
    Ok(result.conformer)
}

fn repair(
    initial: &expansion::Initial,
    remaining: &mut usize,
) -> Result<Vec<rings::Fragment>, collision::Error> {
    let collision = collision::Input::new_with_work(
        &initial.state.graph,
        &initial.state.metadata,
        &initial.state.rings,
        remaining,
    )?;
    let mut fragments = initial.fragments.clone();
    // Native ordering completes flips for every fragment before opening angles
    // or shortening bonds. Never publish the temporary stereo/ring state.
    for fragment in &mut fragments {
        *fragment =
            collision.apply_with_work(fragment, collision::Stage::BondAndSpiroFlip, remaining)?;
    }
    for fragment in &mut fragments {
        *fragment = collision.apply_with_work(fragment, collision::Stage::OpenAngles, remaining)?;
        *fragment =
            collision.apply_with_work(fragment, collision::Stage::ShortenBonds, remaining)?;
    }
    Ok(fragments)
}
