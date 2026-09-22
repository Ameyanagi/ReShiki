//! RDKit 2026.03.6 RDDepictor.cpp compute2DCoords orchestration.
//! BSD-3-Clause; see licenses/rdkit/NOTICE.
use super::{collision, expansion, finalize, geometry::Coordinates, seeds};
use crate::chemistry::stereo::{perception::State, wedging::Conformer};

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
    if !options.bond_length.is_finite() || options.bond_length <= 0.0 {
        return Err(Error::BondLength);
    }
    let mut remaining = options.work_limit.min(expansion::MAX_WORK);
    if remaining == 0 {
        return Err(Error::Limit);
    }
    let mut initial = expansion::compute_initial_with_work(
        state,
        chiral_ranks,
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
        &mut remaining,
    )?;
    let collision = collision::Input::new_with_work(
        &initial.state.graph,
        &initial.state.metadata,
        &initial.state.rings,
        &mut remaining,
    )?;
    // Native ordering completes flips for every fragment before opening angles
    // or shortening bonds. Never publish the temporary stereo/ring state.
    for fragment in &mut initial.fragments {
        *fragment = collision.apply_with_work(
            fragment,
            collision::Stage::BondAndSpiroFlip,
            &mut remaining,
        )?;
    }
    for fragment in &mut initial.fragments {
        *fragment =
            collision.apply_with_work(fragment, collision::Stage::OpenAngles, &mut remaining)?;
        *fragment =
            collision.apply_with_work(fragment, collision::Stage::ShortenBonds, &mut remaining)?;
    }
    let result = finalize::finish_with_work(
        state.graph.atoms.len(),
        &initial.fragments,
        coordinates,
        finalize::Options {
            canonical_orientation: options.canonical_orientation,
            work_limit: options.work_limit,
        },
        &mut remaining,
    )?;
    Ok(result.conformer)
}
