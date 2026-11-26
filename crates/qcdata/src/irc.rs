//! Ordered intrinsic reaction coordinates, including the scalar data kept in light saves.

use crate::{ANGSTROM_PER_BOHR, Error, Frame, Trajectory};
use serde::{Deserialize, Serialize};

/// Classification of a relaxed IRC end against the requested reaction endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrcEndpoint {
    Reactant,
    Product,
    Other,
}

/// One ordered path from its negative end through the saddle to its positive end.
/// The reactant branch is negative when identified; the product branch is positive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrcPath {
    /// Shared atoms, ångström geometries, and hartree energies. The saddle occurs once.
    pub trajectory: Trajectory,
    /// Signed cumulative mass-weighted geometric distance, in amu½·bohr, zero at the saddle.
    pub arc_lengths_sqrt_amu_bohr: Vec<f64>,
    /// NEB cumulative mass-weighted distances, scaled separately on either side of the
    /// climbing image to the IRC end distances. Retained when NEB geometries are omitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neb_arc_lengths_sqrt_amu_bohr: Vec<f64>,
    pub ts_index: usize,
    /// Negative and positive end classifications, respectively.
    pub reached: [IrcEndpoint; 2],
    /// Whether each downhill branch reached its gradient tolerance before relaxation.
    /// This is independent of whether the relaxed end matches a requested endpoint.
    pub completed: [bool; 2],
}

impl IrcPath {
    /// Build an ordered IRC from geometries, using the explicit masses used by the search.
    pub fn new(
        trajectory: Trajectory,
        ts_index: usize,
        reached: [IrcEndpoint; 2],
        completed: [bool; 2],
    ) -> Result<Self, Error> {
        let arc_lengths_sqrt_amu_bohr = Self::geometric_arc_lengths(&trajectory, ts_index)?;
        let path = Self {
            trajectory,
            arc_lengths_sqrt_amu_bohr,
            neb_arc_lengths_sqrt_amu_bohr: Vec::new(),
            ts_index,
            reached,
            completed,
        };
        path.validate()?;
        Ok(path)
    }

    pub fn steps(&self) -> usize {
        self.trajectory.steps()
    }

    pub fn has_frames(&self) -> bool {
        !self.trajectory.positions_angstrom.is_empty()
    }

    /// Return an ordered frame when a full save retained its geometry.
    pub fn frame(&self, index: usize) -> Option<Frame> {
        self.trajectory.frame(index)
    }

    /// Derive a signed arc from successive geometries, without assuming uniform IRC steps.
    pub fn geometric_arc_lengths(
        trajectory: &Trajectory,
        ts_index: usize,
    ) -> Result<Vec<f64>, Error> {
        trajectory.validate()?;
        let atoms = trajectory.numbers.len();
        let width = atoms * 3;
        let steps = trajectory.steps();
        if trajectory.positions_angstrom.is_empty()
            || trajectory.masses_amu.len() != atoms
            || ts_index >= steps
        {
            return Err(Error::Invalid(
                "mass-weighted path lengths require geometries, explicit masses and a valid saddle"
                    .into(),
            ));
        }
        let mut distances = Vec::with_capacity(steps.saturating_sub(1));
        for pair in trajectory
            .positions_angstrom
            .windows(2 * width)
            .step_by(width)
        {
            let distance = pair[..width]
                .iter()
                .zip(&pair[width..])
                .enumerate()
                .map(|(axis, (a, b))| trajectory.masses_amu[axis / 3] * (b - a).powi(2))
                .sum::<f64>()
                .sqrt()
                / ANGSTROM_PER_BOHR;
            distances.push(distance);
        }
        let mut arc = vec![0.; steps];
        for i in (0..ts_index).rev() {
            arc[i] = arc[i + 1] - distances[i];
        }
        for i in ts_index..steps - 1 {
            arc[i + 1] = arc[i] + distances[i];
        }
        if arc.iter().any(|value| !value.is_finite()) {
            return Err(Error::Invalid(
                "non-finite mass-weighted path length".into(),
            ));
        }
        Ok(arc)
    }

    /// Keep the NEB comparison abscissae without retaining another set of geometries.
    pub fn project_neb(&mut self, trajectory: &Trajectory, climbing: usize) -> Result<(), Error> {
        self.validate()?;
        let mut arc = Self::geometric_arc_lengths(trajectory, climbing)?;
        if climbing == 0 || climbing + 1 >= arc.len() {
            return Err(Error::Invalid(
                "the climbing image must lie inside the NEB band".into(),
            ));
        }
        let left = arc[0];
        let right = arc[arc.len() - 1];
        if left >= 0. || right <= 0. {
            // A collapsed NEB branch has no arc to scale. The IRC itself is still usable.
            self.neb_arc_lengths_sqrt_amu_bohr.clear();
            return Ok(());
        }
        let negative = self.arc_lengths_sqrt_amu_bohr[0];
        let positive = self.arc_lengths_sqrt_amu_bohr[self.steps() - 1];
        for value in &mut arc[..climbing] {
            *value *= negative / left;
        }
        for value in &mut arc[climbing + 1..] {
            *value *= positive / right;
        }
        arc[0] = negative;
        let last = arc.len() - 1;
        arc[last] = positive;
        self.neb_arc_lengths_sqrt_amu_bohr = arc;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.trajectory.validate()?;
        let steps = self.steps();
        let arc = &self.arc_lengths_sqrt_amu_bohr;
        if steps < 3
            || self.trajectory.numbers.is_empty()
            || self.ts_index == 0
            || self.ts_index >= steps.saturating_sub(1)
            || self.trajectory.energies_hartree.len() != steps
            || self.trajectory.masses_amu.len() != self.trajectory.numbers.len()
            || arc.len() != steps
            || arc.iter().any(|value| !value.is_finite())
            || arc.windows(2).any(|pair| pair[0] > pair[1])
            || arc[self.ts_index] != 0.
            || (self.reached[0] == IrcEndpoint::Product && self.reached[1] != IrcEndpoint::Product)
            || (self.reached[1] == IrcEndpoint::Reactant
                && self.reached[0] != IrcEndpoint::Reactant)
            || self
                .neb_arc_lengths_sqrt_amu_bohr
                .iter()
                .any(|value| !value.is_finite())
            || self
                .neb_arc_lengths_sqrt_amu_bohr
                .windows(2)
                .any(|pair| pair[0] > pair[1])
        {
            return Err(Error::Invalid("invalid ordered IRC path".into()));
        }
        Ok(())
    }

    pub(crate) fn validate_for(&self, frame: &Frame, energy: Option<f64>) -> Result<(), Error> {
        self.validate()?;
        let path = &self.trajectory;
        if path.numbers != frame.numbers
            || (0..frame.numbers.len()).any(|i| {
                path.isotopes.get(i).copied().unwrap_or(0)
                    != frame.isotopes.get(i).copied().unwrap_or(0)
            })
            || path.masses_amu != frame.masses_amu
            || path.charge != frame.charge
            || path.multiplicity != frame.multiplicity
            || energy != Some(path.energies_hartree[self.ts_index])
        {
            return Err(Error::Invalid(
                "IRC and saddle describe different atoms, states or energies".into(),
            ));
        }
        if let Some(saddle) = self.frame(self.ts_index)
            && saddle
                .positions_angstrom
                .iter()
                .flatten()
                .zip(frame.positions_angstrom.iter().flatten())
                .any(|(&a, &b)| a as f32 != b as f32)
        {
            return Err(Error::Invalid(
                "IRC saddle geometry differs from the result frame".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.trajectory.heap_bytes()
            + size_of_val(self.arc_lengths_sqrt_amu_bohr.as_slice())
            + size_of_val(self.neb_arc_lengths_sqrt_amu_bohr.as_slice())
    }

    pub(crate) fn light(&self) -> Self {
        let path = &self.trajectory;
        Self {
            trajectory: Trajectory {
                numbers: path.numbers.clone(),
                isotopes: path.isotopes.clone(),
                masses_amu: path.masses_amu.clone(),
                charge: path.charge,
                multiplicity: path.multiplicity,
                positions_angstrom: Vec::new(),
                energies_hartree: path.energies_hartree.clone(),
                labels: path.labels.clone(),
                minimizer_energies_hartree: path.minimizer_energies_hartree.clone(),
                cluster_sizes: path.cluster_sizes.clone(),
                rmsd_to_best_angstrom: path.rmsd_to_best_angstrom.clone(),
            },
            arc_lengths_sqrt_amu_bohr: self.arc_lengths_sqrt_amu_bohr.clone(),
            neb_arc_lengths_sqrt_amu_bohr: self.neb_arc_lengths_sqrt_amu_bohr.clone(),
            ts_index: self.ts_index,
            reached: self.reached,
            completed: self.completed,
        }
    }
}
