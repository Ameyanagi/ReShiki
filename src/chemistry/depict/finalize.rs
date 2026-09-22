//! Final deterministic depiction stages from RDKit 2026.03.6 RDDepictor.cpp
//! and EmbeddedFrag.cpp/.h. See licenses/rdkit/NOTICE for BSD-3-Clause attribution.
//!
//! Input fragments retain native list order. This library stage implements the
//! application's `clearConfs=true` contract and returns a new 2D conformer with
//! ID zero; it never appends to or mutates a molecule's conformer collection.

use super::{
    geometry::{self, Coordinates, Point},
    rings::Fragment,
};
use crate::chemistry::stereo::{Point3, wedging::Conformer};

pub const MAX_FRAGMENTS: usize = 100_000;
pub const MAX_FRAGMENT_ATOMS: usize = 1_000_000;
pub const MAX_METADATA_ENTRIES: usize = 1_000_000;
pub const MAX_WORK: usize = 50_000_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid depiction finalization input: {0}")]
    Invalid(&'static str),
    #[error("Depiction finalization work or storage limit exceeded")]
    Limit,
    #[error(transparent)]
    Geometry(#[from] geometry::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Python Compute2DCoords defaults to true; selected cleanup passes false.
    pub canonical_orientation: bool,
    /// May lower, but never exceed, the hard operation budget.
    pub work_limit: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            canonical_orientation: true,
            work_limit: MAX_WORK,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finalized {
    /// Packed fragments, with native pre-translation bounds and metadata.
    /// A one-point coordinate map translates only the output conformer.
    pub fragments: Vec<Fragment>,
    pub conformer: Conformer,
    /// Always zero under this stage's clearConfs=true contract.
    pub conformer_id: u32,
}

struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(amount).ok_or(Error::Limit)?;
        Ok(())
    }
    fn atoms(&mut self, count: usize, passes: usize) -> Result<(), Error> {
        self.spend(count.checked_mul(passes).ok_or(Error::Limit)?)
    }
}
fn input_number(value: f64) -> Result<(), Error> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(geometry::Error::NonFinite.into())
    }
}
fn number(value: f64) -> Result<f64, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(geometry::Error::Numeric.into())
    }
}
fn point(value: Point) -> Result<Point, Error> {
    number(value.x)?;
    number(value.y)?;
    Ok(value)
}
fn atom_id(id: usize, count: usize) -> Result<(), Error> {
    if id < count {
        Ok(())
    } else {
        Err(Error::Invalid("atom index outside conformer"))
    }
}
fn validate(
    count: usize,
    fragments: &[Fragment],
    coordinates: Option<&Coordinates>,
    work: &mut Work,
) -> Result<(), Error> {
    if count > geometry::MAX_POINTS || fragments.len() > MAX_FRAGMENTS {
        return Err(Error::Limit);
    }
    work.spend(count)?;
    let mut atoms = 0usize;
    let mut metadata = 0usize;
    for fragment in fragments {
        work.spend(1)?;
        atoms = atoms
            .checked_add(fragment.atoms.len())
            .ok_or(Error::Limit)?;
        metadata = metadata
            .checked_add(fragment.attachment_points.len())
            .ok_or(Error::Limit)?;
        if atoms > MAX_FRAGMENT_ATOMS || metadata > MAX_METADATA_ENTRIES {
            return Err(Error::Limit);
        }
        for value in [
            fragment.bounds.positive_x,
            fragment.bounds.negative_x,
            fragment.bounds.positive_y,
            fragment.bounds.negative_y,
        ] {
            input_number(value)?;
        }
        for &id in &fragment.attachment_points {
            work.spend(1)?;
            atom_id(id, count)?;
        }
        for (&id, atom) in &fragment.atoms {
            metadata = metadata
                .checked_add(atom.neighbors.len())
                .ok_or(Error::Limit)?;
            if metadata > MAX_METADATA_ENTRIES {
                return Err(Error::Limit);
            }
            work.atoms(1, 8)?;
            atom_id(id, count)?;
            atom_id(atom.id, count)?;
            for id in [atom.neighbor1, atom.neighbor2, atom.cis_trans_neighbor]
                .into_iter()
                .flatten()
            {
                atom_id(id, count)?;
            }
            for &id in &atom.neighbors {
                work.spend(1)?;
                atom_id(id, count)?;
            }
            for value in [
                atom.location.x,
                atom.location.y,
                atom.normal.x,
                atom.normal.y,
                atom.angle,
                atom.density,
            ] {
                input_number(value)?;
            }
        }
    }
    if let Some(coordinates) = coordinates {
        if coordinates.len() > geometry::MAX_POINTS {
            return Err(Error::Limit);
        }
        for (&id, p) in coordinates {
            work.spend(1)?;
            atom_id(id, count)?;
            input_number(p.x)?;
            input_number(p.y)?;
        }
    }
    // Charge detached state copying before allocating it, including neighbors.
    work.atoms(atoms, 2)?;
    work.spend(metadata)?;
    Ok(())
}

fn canonicalize(fragment: &mut Fragment, work: &mut Work) -> Result<(), Error> {
    work.atoms(fragment.atoms.len(), 8)?;
    let coordinates = fragment
        .atoms
        .iter()
        .map(|(&id, atom)| (id, atom.location))
        .collect();
    let (centered, transform) = geometry::canonical_basis(&coordinates)?;
    for (&id, atom) in &mut fragment.atoms {
        atom.location = *centered.get(&id).ok_or(Error::Invalid("centered atom"))?;
        if let Some(transform) = transform {
            // Native EmbeddedAtom::Transform uses the displaced point, not
            // matrix multiplication of the normal in isolation.
            let temporary = point(Point {
                x: atom.location.x + atom.normal.x,
                y: atom.location.y + atom.normal.y,
            })?;
            atom.location = transform.apply(atom.location)?;
            let temporary = transform.apply(temporary)?;
            atom.normal = point(Point {
                x: temporary.x - atom.location.x,
                y: temporary.y - atom.location.y,
            })?;
        }
    }
    Ok(())
}

fn pack(fragments: &mut [Fragment], work: &mut Work) -> Result<(), Error> {
    for fragment in fragments.iter_mut() {
        work.atoms(fragment.atoms.len(), 2)?;
        let coordinates = fragment
            .atoms
            .iter()
            .map(|(&id, atom)| (id, atom.location))
            .collect();
        fragment.bounds = geometry::compute_box(&coordinates)?;
    }
    let Some((first, rest)) = fragments.split_first_mut() else {
        return Ok(());
    };
    let (mut xmax, xmin, mut ymax, ymin) = (
        first.bounds.positive_x,
        first.bounds.negative_x,
        first.bounds.positive_y,
        first.bounds.negative_y,
    );
    for fragment in rest {
        work.atoms(fragment.atoms.len(), 1)?;
        work.spend(1)?;
        let b = fragment.bounds;
        let shift = if number(xmax + xmin)? > number(ymax + ymin)? {
            let y = number(number(ymax + b.negative_y)? + 1.0)?;
            ymax = number(ymax + number(number(b.positive_y + b.negative_y)? + 1.0)?)?;
            Point { x: 0.0, y }
        } else {
            let x = number(number(xmax + b.negative_x)? + 1.0)?;
            xmax = number(xmax + number(number(b.positive_x + b.negative_x)? + 1.0)?)?;
            Point { x, y: 0.0 }
        };
        for atom in fragment.atoms.values_mut() {
            atom.location = point(Point {
                x: atom.location.x + shift.x,
                y: atom.location.y + shift.y,
            })?;
        }
        // Translate leaves bounds and normals unchanged. The accumulated box
        // also leaves its minima and orthogonal maximum unchanged.
    }
    Ok(())
}

/// Canonicalize, pack, assemble and apply the native single-anchor correction.
///
/// `None` and an empty coordinate map allow canonical orientation. Any nonempty
/// map suppresses it; only a single entry translates the assembled conformer.
/// Multi-anchor preservation checks and cleanup's rigid orientation are later
/// application stages. Fragment order is never sorted. Duplicate atom writes
/// use the last fragment; atoms absent from all fragments retain zero positions.
/// Inputs remain unchanged on success and on every checked failure.
pub fn finish(
    atom_count: usize,
    fragments: &[Fragment],
    coord_map: Option<&Coordinates>,
    options: Options,
) -> Result<Finalized, Error> {
    let mut remaining = options.work_limit.min(MAX_WORK);
    finish_with_work(atom_count, fragments, coord_map, options, &mut remaining)
}

/// Share the solver's cumulative budget; inputs remain atomic on failure.
/// Already performed work is charged even if a later stage returns an error.
pub(crate) fn finish_with_work(
    atom_count: usize,
    fragments: &[Fragment],
    coord_map: Option<&Coordinates>,
    options: Options,
    remaining: &mut usize,
) -> Result<Finalized, Error> {
    let initial = (*remaining).min(options.work_limit).min(MAX_WORK);
    let mut work = Work(initial);
    let result = finish_inner(atom_count, fragments, coord_map, options, &mut work);
    let used = initial.checked_sub(work.0).ok_or(Error::Limit)?;
    *remaining = remaining.checked_sub(used).ok_or(Error::Limit)?;
    result
}

fn finish_inner(
    atom_count: usize,
    fragments: &[Fragment],
    coord_map: Option<&Coordinates>,
    options: Options,
    work: &mut Work,
) -> Result<Finalized, Error> {
    validate(atom_count, fragments, coord_map, work)?;
    let mut fragments = fragments.to_vec();
    if options.canonical_orientation && coord_map.is_none_or(Coordinates::is_empty) {
        for fragment in &mut fragments {
            canonicalize(fragment, work)?;
        }
    }
    pack(&mut fragments, work)?;
    work.spend(atom_count)?;
    let mut positions = vec![Point3::default(); atom_count];
    for fragment in &fragments {
        work.atoms(fragment.atoms.len(), 1)?;
        for (&id, atom) in &fragment.atoms {
            *positions
                .get_mut(id)
                .ok_or(Error::Invalid("conformer atom"))? = Point3 {
                x: atom.location.x,
                y: atom.location.y,
                z: 0.0,
            };
        }
    }
    if let Some(coordinates) = coord_map.filter(|m| m.len() == 1) {
        let (&id, target) = coordinates
            .first_key_value()
            .ok_or(Error::Invalid("anchor"))?;
        let current = positions.get(id).ok_or(Error::Invalid("anchor atom"))?;
        let shift = point(Point {
            x: target.x - current.x,
            y: target.y - current.y,
        })?;
        work.spend(atom_count)?;
        for position in &mut positions {
            position.x = number(position.x + shift.x)?;
            position.y = number(position.y + shift.y)?;
        }
    }
    Ok(Finalized {
        fragments,
        conformer: Conformer {
            positions,
            is_3d: false,
        },
        conformer_id: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cumulative_budget_survives_stage_boundaries() {
        let mut remaining = 3;
        assert!(finish_with_work(1, &[], None, Options::default(), &mut remaining).is_ok());
        assert_eq!(remaining, 1);
        assert!(matches!(
            finish_with_work(1, &[], None, Options::default(), &mut remaining),
            Err(Error::Limit)
        ));
        assert_eq!(remaining, 0);
    }
}
