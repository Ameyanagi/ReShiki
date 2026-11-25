//! Trust-region acceptance and radius adjustment shared by both RFO coordinate models.

use super::{Evaluator, Options, Point, Potential, Status, TrialStep};
use crate::linalg::norm;

/// Trust region for both RFO methods. The ratio thresholds and boundary
/// expansion follow Nocedal & Wright, Numerical Optimization, Algorithm 4.1.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Trust {
    /// Bound the Euclidean model-space displacement. Start at 0.3 and adapt
    /// between 0.001 and 0.5. ANC uses bohr; TRIC also contains angular coordinates.
    #[default]
    Global,
    /// Divide the model-space length by √(free atoms). Also bound the actual
    /// Cartesian RMS per free atom by the radius, and each atom's displacement
    /// by `atom_cap` bohr. Restrict the whole direction before evaluation.
    PerAtom {
        start: f64,
        min: f64,
        max: f64,
        atom_cap: f64,
    },
}

impl Trust {
    /// Start at 0.08 bohr RMS per atom, within [0.001, 0.2], with a 0.4 bohr atom cap.
    pub const fn per_atom() -> Self {
        Self::PerAtom {
            start: 0.08,
            min: 0.001,
            max: 0.2,
            atom_cap: 0.4,
        }
    }
}

/// Schedule for transporting the Hessian to a new coordinate basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rebuild {
    /// Rebuild after this many accepted displacements since the previous build.
    Every(usize),
    /// Rebuild after three consecutive accepted steps with actual/predicted < 0.25,
    /// or after 60 accepted steps in one basis.
    OnPoorPrediction,
}

impl Default for Rebuild {
    fn default() -> Self {
        Self::Every(30)
    }
}

impl Options {
    pub(super) fn validate_rfo(&self) -> Result<(), &'static str> {
        if let Trust::PerAtom {
            start,
            min,
            max,
            atom_cap,
        } = self.trust
            && !(min > 0.
                && min <= start
                && start <= max
                && max.is_finite()
                && atom_cap.is_finite()
                && atom_cap > 0.)
        {
            return Err(
                "expected finite positive trust radii with min <= start <= max and a positive finite atom cap",
            );
        }
        if !self.reject_ratio.is_finite() || self.reject_ratio < 0. {
            return Err("reject_ratio must be finite and nonnegative");
        }
        if !self.curvature_floor.is_finite() || self.curvature_floor <= 0. {
            return Err("curvature_floor must be finite and positive");
        }
        if self.rebuild == Rebuild::Every(0) {
            return Err("rebuild interval must be positive");
        }
        Ok(())
    }
}

pub(super) struct TrustRegion {
    /// Radius in the policy's units, as reported to the callback.
    pub radius: f64,
    min: f64,
    max: f64,
    scale: f64,
    atom_cap: Option<f64>,
    reject_ratio: f64,
    failed_transforms: u8,
    poor_predictions: u8,
}

#[derive(Clone, Copy)]
pub(super) struct Bound {
    model_radius: f64,
    atom_cap: Option<f64>,
}

impl Bound {
    pub fn restrict(self, direction: impl Fn(f64) -> Option<TrialStep>) -> Option<TrialStep> {
        let mut radius = self.model_radius;
        for attempt in 0..10 {
            let step = direction(radius)?;
            let Some(cap) = self.atom_cap else {
                return Some(step);
            };
            // A nonlinear back-transform can amplify a Cartesian displacement.
            // Recompute the image and prediction along the shortened model direction.
            let largest = step
                .cartesian
                .as_chunks::<3>()
                .0
                .iter()
                .map(|atom| norm(atom))
                .fold(0., f64::max);
            let factor = (cap / largest).min(self.model_radius / norm(&step.cartesian));
            if factor >= 1. {
                return Some(step);
            }
            // Allow for roundoff, then back off further if the nonlinear image
            // still overshoots the Cartesian bound.
            let margin = if attempt == 0 {
                1. - 8. * f64::EPSILON
            } else {
                0.99
            };
            radius = step.length.min(radius) * factor * margin;
        }
        None
    }
}

pub(super) enum DirectionError {
    NoStep,
    BackTransform,
}

pub(super) enum Search {
    Accepted(Point, f64),
    Exhausted,
    Rebuild,
}

impl Search {
    pub fn accepted(self) -> Option<(Point, f64)> {
        match self {
            Self::Accepted(point, radius) => Some((point, radius)),
            _ => None,
        }
    }
}

impl TrustRegion {
    pub fn new(options: &Options, free_atoms: usize) -> Self {
        let (radius, min, max, scale, atom_cap) = match options.trust {
            Trust::Global => (0.3, 0.001, 0.5, 1., None),
            Trust::PerAtom {
                start,
                min,
                max,
                atom_cap,
            } => (start, min, max, (free_atoms as f64).sqrt(), Some(atom_cap)),
        };
        Self {
            radius,
            min,
            max,
            scale,
            atom_cap,
            reject_ratio: options.reject_ratio,
            failed_transforms: 0,
            poor_predictions: 0,
        }
    }

    pub fn model_radius(&self) -> f64 {
        self.radius * self.scale
    }

    pub fn should_rebuild(&self, schedule: Rebuild, age: usize) -> bool {
        self.failed_transforms >= 2
            || match schedule {
                Rebuild::Every(interval) => age >= interval,
                Rebuild::OnPoorPrediction => self.poor_predictions >= 3 || age >= 60,
            }
    }

    pub fn rebuilt(&mut self) {
        self.failed_transforms = 0;
        self.poor_predictions = 0;
    }

    fn accept(&mut self, ratio: f64, on_boundary: bool) {
        if ratio < 0.25 {
            self.poor_predictions = self.poor_predictions.saturating_add(1);
            self.radius = (0.25 * self.radius).max(self.min);
        } else {
            self.poor_predictions = 0;
            if ratio > 0.75 && on_boundary {
                self.radius = (2. * self.radius).min(self.max);
            }
        }
    }

    pub fn search<P: Potential + ?Sized>(
        &mut self,
        eval: &mut Evaluator<P>,
        point: &Point,
        accuracy_changed: bool,
        accelerated: impl FnOnce(Bound) -> Option<Result<TrialStep, DirectionError>>,
        ordinary: impl Fn(Bound) -> Result<TrialStep, DirectionError>,
    ) -> Result<Search, Status> {
        let mut accelerated = Some(accelerated);
        let mut backtrack_radius = None;
        for _ in 0..10 {
            if self.failed_transforms >= 2 {
                return Ok(Search::Rebuild);
            }
            let radius = self.radius;
            let bound = Bound {
                model_radius: backtrack_radius.unwrap_or_else(|| self.model_radius()),
                atom_cap: self.atom_cap,
            };
            let diis = accelerated.take().and_then(|d| d(bound));
            let used_diis = diis.is_some();
            let step = match diis.unwrap_or_else(|| ordinary(bound)) {
                Ok(step) => step,
                Err(DirectionError::NoStep) => return Ok(Search::Exhausted),
                Err(DirectionError::BackTransform) => {
                    self.radius = (0.25 * radius).max(self.min);
                    self.failed_transforms += 1;
                    continue;
                }
            };
            self.failed_transforms = if step.converged_transform {
                0
            } else {
                self.failed_transforms + 1
            };
            let length = if self.atom_cap.is_some() {
                step.length.max(norm(&step.cartesian))
            } else {
                step.length
            };
            let trial = eval.trial(point, &step.cartesian)?;
            let actual = trial.energy - point.energy;
            let ratio = actual / step.predicted;
            let allowed_rise = if backtrack_radius.is_some() { 0. } else { 1e-6 };
            if actual > allowed_rise || (self.reject_ratio > 0. && ratio < self.reject_ratio) {
                // Ordinary RFO gets a try even when DIIS reached the floor.
                if length <= self.min * self.scale && !used_diis {
                    self.radius = self.min;
                    // The model was learned on the previous surface. Try a
                    // shorter downhill displacement inside the minimum trust
                    // radius before switching methods on the refreshed point.
                    if accuracy_changed {
                        backtrack_radius = Some(0.25 * length.min(bound.model_radius));
                        continue;
                    }
                    break;
                }
                self.radius = (0.25 * (length / self.scale)).max(self.min);
                continue;
            }
            let model_radius = self.model_radius();
            let on_boundary = if self.atom_cap.is_some() {
                length >= model_radius * (1. - 32. * f64::EPSILON)
            } else {
                length == model_radius
            };
            self.accept(ratio, on_boundary);
            return Ok(Search::Accepted(trial, radius));
        }
        Ok(if self.failed_transforms >= 2 {
            Search::Rebuild
        } else {
            Search::Exhausted
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poor_prediction_rebuild_counts_consecutive_steps_and_limits_basis_age() {
        let schedule = Rebuild::OnPoorPrediction;
        let mut region = TrustRegion::new(&Options::default(), 5);
        for ratio in [0.1, 0.2, 0.25, 0.15, 0.2] {
            region.accept(ratio, false);
            assert!(!region.should_rebuild(schedule, 5));
        }
        region.accept(0.24, false);
        assert!(region.should_rebuild(schedule, 6));
        assert!(!region.should_rebuild(Rebuild::Every(30), 29));
        assert!(region.should_rebuild(Rebuild::Every(30), 30));
        region.rebuilt();
        assert!(!region.should_rebuild(schedule, 59));
        assert!(region.should_rebuild(schedule, 60));
    }
}
