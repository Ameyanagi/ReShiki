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

/// A separate minimization started from the last traced point of a branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrcRelaxedEnd {
    pub energy_hartree: f64,
    #[serde(
        with = "crate::compact::f32",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub positions_angstrom: Vec<f64>,
}

/// One ordered path from its negative end through the saddle to its positive end.
/// The reactant branch is negative when identified; the product branch is positive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrcPath {
    /// Traced ångström geometries and hartree energies. The saddle occurs once.
    pub trajectory: Trajectory,
    /// Signed cumulative mass-weighted geometric distance, in amu½·bohr, zero at the saddle.
    #[serde(default)]
    pub arc_lengths_sqrt_amu_bohr: Vec<f64>,
    /// NEB cumulative mass-weighted distances, scaled separately on either side of the
    /// climbing image to the IRC end distances. Retained when NEB geometries are omitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neb_arc_lengths_sqrt_amu_bohr: Vec<f64>,
    pub ts_index: usize,
    /// Negative and positive end classifications, respectively.
    pub reached: [IrcEndpoint; 2],
    /// Relaxed ends in branch order. Early files appended them to the trace instead.
    #[serde(default)]
    pub relaxed_ends: Option<[IrcRelaxedEnd; 2]>,
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
        relaxed_ends: [IrcRelaxedEnd; 2],
    ) -> Result<Self, Error> {
        let arc_lengths_sqrt_amu_bohr = Self::geometric_arc_lengths(&trajectory, ts_index)?;
        let path = Self {
            trajectory,
            arc_lengths_sqrt_amu_bohr,
            neb_arc_lengths_sqrt_amu_bohr: Vec::new(),
            ts_index,
            reached,
            completed,
            relaxed_ends: Some(relaxed_ends),
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

    pub fn relaxed_frame(&self, branch: usize) -> Option<Frame> {
        let positions = &self.relaxed_ends.as_ref()?.get(branch)?.positions_angstrom;
        let mut frame = self.frame(self.ts_index)?;
        frame.positions_angstrom = positions.as_chunks::<3>().0.to_vec();
        (!positions.is_empty()).then_some(frame)
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
        let arc = Self::geometric_arc_lengths(trajectory, climbing)?;
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
        self.scale_neb(arc, climbing);
        Ok(())
    }

    fn scale_neb(&mut self, mut arc: Vec<f64>, climbing: usize) {
        let last = arc.len() - 1;
        let left = arc[0];
        let right = arc[last];
        let negative = self.arc_lengths_sqrt_amu_bohr[0];
        let positive = self.arc_lengths_sqrt_amu_bohr[self.steps() - 1];
        let left_scale = if left == 0. { 0. } else { negative / left };
        let right_scale = if right == 0. { 0. } else { positive / right };
        for value in &mut arc[..climbing] {
            *value *= left_scale;
        }
        for value in &mut arc[climbing + 1..] {
            *value *= right_scale;
        }
        arc[0] = negative;
        arc[last] = positive;
        self.neb_arc_lengths_sqrt_amu_bohr = arc;
    }

    pub(crate) fn restore(
        &mut self,
        band: Option<&Trajectory>,
        climbing: usize,
    ) -> Result<(), Error> {
        let legacy = self.relaxed_ends.is_none();
        if legacy {
            self.separate_legacy_ends()?;
        }
        if self.has_frames() {
            self.arc_lengths_sqrt_amu_bohr =
                Self::geometric_arc_lengths(&self.trajectory, self.ts_index)?;
            if let Some(band) = band.filter(|band| !band.positions_angstrom.is_empty()) {
                self.neb_arc_lengths_sqrt_amu_bohr.clear();
                return self.project_neb(band, climbing);
            }
        }
        // Older full saves have no NEB geometries. Keep the stored proportions
        // and anchor them to the reconstructed trace.
        let neb = &self.neb_arc_lengths_sqrt_amu_bohr;
        if (legacy || self.has_frames())
            && climbing > 0
            && climbing < neb.len().saturating_sub(1)
            && (neb.first() != self.arc_lengths_sqrt_amu_bohr.first()
                || neb.last() != self.arc_lengths_sqrt_amu_bohr.last())
        {
            self.validate()?;
            if (neb[0] == 0. && self.arc_lengths_sqrt_amu_bohr[0] != 0.)
                || (neb[neb.len() - 1] == 0.
                    && self.arc_lengths_sqrt_amu_bohr[self.steps() - 1] != 0.)
            {
                self.neb_arc_lengths_sqrt_amu_bohr.clear();
            } else {
                let neb = std::mem::take(&mut self.neb_arc_lengths_sqrt_amu_bohr);
                self.scale_neb(neb, climbing);
            }
        }
        Ok(())
    }

    fn separate_legacy_ends(&mut self) -> Result<(), Error> {
        fn take_ends<T>(values: &mut Vec<T>) -> [T; 2] {
            [values.remove(0), values.pop().unwrap()]
        }

        let steps = self.steps();
        let has_frames = self.has_frames();
        let path = &mut self.trajectory;
        path.validate()?;
        if steps < 3
            || self.ts_index == 0
            || self.ts_index >= steps - 1
            || path.energies_hartree.len() != steps
            || (!has_frames && self.arc_lengths_sqrt_amu_bohr.len() != steps)
        {
            return Err(Error::Invalid("invalid legacy IRC path".into()));
        }
        self.ts_index -= 1;
        let energies = take_ends(&mut path.energies_hartree);
        let positions: [Vec<f64>; 2] = if has_frames {
            let width = path.numbers.len() * 3;
            let last = path.positions_angstrom.split_off((steps - 1) * width);
            [path.positions_angstrom.drain(..width).collect(), last]
        } else {
            take_ends(&mut self.arc_lengths_sqrt_amu_bohr);
            Default::default()
        };
        if !path.labels.is_empty() {
            take_ends(&mut path.labels);
        }
        if !path.minimizer_energies_hartree.is_empty() {
            take_ends(&mut path.minimizer_energies_hartree);
        }
        if !path.cluster_sizes.is_empty() {
            take_ends(&mut path.cluster_sizes);
        }
        if !path.rmsd_to_best_angstrom.is_empty() {
            take_ends(&mut path.rmsd_to_best_angstrom);
        }
        let mut positions = positions.into_iter();
        self.relaxed_ends = Some(energies.map(|energy_hartree| IrcRelaxedEnd {
            energy_hartree,
            positions_angstrom: positions.next().unwrap(),
        }));
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.trajectory.validate()?;
        let steps = self.steps();
        let arc = &self.arc_lengths_sqrt_amu_bohr;
        if steps == 0
            || self.trajectory.numbers.is_empty()
            || self.ts_index >= steps
            || self.trajectory.energies_hartree.len() != steps
            || self.trajectory.masses_amu.len() != self.trajectory.numbers.len()
            || arc.len() != steps
            || arc.iter().any(|value| !value.is_finite())
            || arc.windows(2).any(|pair| pair[0] > pair[1])
            || arc[self.ts_index] != 0.
            || self.relaxed_ends.iter().flatten().any(|end| {
                !end.energy_hartree.is_finite()
                    || (!end.positions_angstrom.is_empty()
                        && (end.positions_angstrom.len() != self.trajectory.numbers.len() * 3
                            || end
                                .positions_angstrom
                                .iter()
                                .any(|&x| !(x as f32).is_finite())))
            })
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
            + self
                .relaxed_ends
                .iter()
                .flatten()
                .map(|end| size_of_val(end.positions_angstrom.as_slice()))
                .sum::<usize>()
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
            relaxed_ends: self.relaxed_ends.as_ref().map(|ends| {
                ends.each_ref().map(|end| IrcRelaxedEnd {
                    energy_hartree: end.energy_hartree,
                    positions_angstrom: Vec::new(),
                })
            }),
        }
    }
}
