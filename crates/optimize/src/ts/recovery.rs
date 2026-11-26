//! Extra-mode displacement followed by ordinary index-one refinement.
//!
//! Negative-curvature escape follows Heidrich & Quapp (1986) \[Heidrich1986IndexTwo\];
//! P-RFO follows Baker (1986) \[Baker1986PRFO\]. Tangent ordering and the
//! symmetric 0.1-bohr seeds are local recovery choices.

use std::ops::ControlFlow;

use crate::{
    Potential,
    linalg::{dot, norm},
};

use super::{
    Counted, Event, Minimum, Search, Stage, TsError, TsReport, characterize, emit, endpoint,
    mapping, modes, neb::Band, saddle, validate_geometry, validate_saved_band,
};

/// A failed candidate and its geometry-specific Cartesian Hessian (Eh/bohr²,
/// row-major). Coordinates are bohr in the saved band's atom order and frame.
#[derive(Debug, Clone, PartialEq)]
pub struct SaddleCandidate {
    pub positions: Vec<f64>,
    pub hessian: Vec<f64>,
}

// A full Cartesian norm of 0.1 bohr breaks exact higher-order symmetry while
// staying within the standard refinement's initial 0.1-bohr trust radius.
// Normalize after converting from mass-weighted modes: the displacement must
// not grow with atom count or depend on the numerical mass convention.
const DISPLACEMENT_BOHR: f64 = 0.1;
const MAX_DIAGNOSTICS: usize = 32;

/// Recover a first-order saddle from a saved higher-order candidate.
///
/// The candidate's Hessian supplies its significant imaginary modes. The mode
/// with largest absolute Cartesian overlap with the saved band tangent supplies
/// the uphill P-RFO reference. Remaining modes are tried by increasing overlap,
/// with both ±0.1 bohr Cartesian seeds and the ordinary refinement/checks from
/// [`super::search`], including a fresh Hessian at each displaced geometry.
///
/// `verify` independently checks each characterized trial before IRC.
/// False prevents IRC. After both signs are refined, the first verified connection
/// and its payload are retained. Otherwise the last checked candidate is returned.
/// Optional seed failures are recorded; if no seed can be characterized the last
/// error is returned. Cancellation always propagates immediately.
pub fn refine_from_candidate<P: Potential, T>(
    potential: &mut P,
    search: &Search<'_>,
    path: &[Minimum],
    neb_converged: bool,
    candidate: &SaddleCandidate,
    mut on_event: impl FnMut(Event) -> ControlFlow<()>,
    mut verify: impl FnMut(&mut P, &[f64], usize) -> Result<(bool, T), TsError>,
) -> Result<(TsReport, T), TsError> {
    validate_saved_band(search, path)?;
    validate_geometry(search.numbers, &candidate.positions)?;
    endpoint::validate_nuclei_separation(&candidate.positions)?;
    let n = candidate.positions.len();
    if candidate.hessian.len() != n * n || candidate.hessian.iter().any(|v| !v.is_finite()) {
        return Err(TsError::Invalid(
            "saved candidate needs a finite square Cartesian Hessian".into(),
        ));
    }
    let scale = candidate
        .hessian
        .iter()
        .fold(1.0_f64, |a, v| a.max(v.abs()));
    if (0..n).any(|i| {
        (0..i).any(|j| {
            (candidate.hessian[i * n + j] - candidate.hessian[j * n + i]).abs() > 1e-10 * scale
        })
    }) {
        return Err(TsError::Invalid(
            "saved candidate Hessian must be symmetric".into(),
        ));
    }
    let band = Band::from_saved(path, search.masses_amu, neb_converged);
    let mut counted = Counted {
        potential,
        evaluations: 0,
        hessians: 0,
    };
    // The saved band's tangent belongs to its climbing-image frame. Rotate it
    // into the saved candidate frame before comparing physical directions.
    let fit = mapping::Fit::new(
        &candidate.positions,
        &band.path[band.climbing].positions,
        search.masses_amu,
    );
    let tangent = fit.rotate_vector(&band.tangent);
    let (reaction, extra) = recovery_directions(
        &candidate.positions,
        &candidate.hessian,
        search.masses_amu,
        &tangent,
    )?;
    let origin = &candidate.positions;
    let mut diagnostics = Vec::new();
    let mut last = None;
    let mut last_error = None;
    for (mode, direction) in extra.iter().enumerate() {
        let mut verified = None;
        for sign in [1.0, -1.0] {
            emit(&mut on_event, Event::Stage(Stage::Saddle))?;
            emit(
                &mut on_event,
                Event::SaddleStatus("displace extra imaginary mode and refine"),
            )?;
            let positions: Vec<_> = origin
                .iter()
                .zip(direction)
                .map(|(x, v)| x + sign * DISPLACEMENT_BOHR * v)
                .collect();
            let trial = (|| {
                let refinement = saddle::refine_displaced(
                    &mut counted,
                    &positions,
                    &reaction,
                    search.masses_amu,
                    search.options,
                    &mut on_event,
                )?;
                characterize(
                    &mut counted,
                    search,
                    &band,
                    [path[0].clone(), path[path.len() - 1].clone()],
                    refinement,
                    &mut on_event,
                    &mut verify,
                )
            })();
            match trial {
                Ok((mut report, verification)) => {
                    let success =
                        report.saddle_converged && report.imaginary == 1 && report.connects;
                    if diagnostics.len() < MAX_DIAGNOSTICS {
                        diagnostics.push(format!(
                            "extra imaginary mode {} ({:+.1} bohr): {}",
                            mode + 1,
                            sign * DISPLACEMENT_BOHR,
                            if success {
                                "verified first-order connection"
                            } else {
                                "candidate not verified"
                            }
                        ));
                    }
                    let remaining = MAX_DIAGNOSTICS.saturating_sub(diagnostics.len());
                    diagnostics.extend(report.saddle_diagnostics.drain(..).take(remaining));
                    if success && verified.is_none() {
                        verified = Some((report, verification));
                    } else {
                        last = Some((report, verification));
                    }
                }
                Err(error @ (TsError::Scf(_) | TsError::Failed(_))) => {
                    if diagnostics.len() < MAX_DIAGNOSTICS {
                        diagnostics.push(format!(
                            "extra imaginary mode {} ({:+.1} bohr): {error}",
                            mode + 1,
                            sign * DISPLACEMENT_BOHR
                        ));
                    }
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        if let Some(result) = verified {
            last = Some(result);
            break;
        }
    }
    let (mut last, verification) = last.ok_or_else(|| {
        last_error.unwrap_or_else(|| {
            TsError::Failed("extra-mode recovery produced no characterized candidate".into())
        })
    })?;
    last.saddle_diagnostics = diagnostics;
    last.evaluations = counted.evaluations;
    last.hessians = counted.hessians;
    Ok((last, verification))
}

/// Return a unit Cartesian reaction reference and ordered unit escape modes.
fn recovery_directions(
    x: &[f64],
    h: &[f64],
    masses: &[f64],
    tangent: &[f64],
) -> Result<(Vec<f64>, Vec<Vec<f64>>), TsError> {
    let n = x.len();
    let inverse_sqrt: Vec<_> = masses.iter().flat_map(|m| [m.sqrt().recip(); 3]).collect();
    let weighted = modes::mass_weight(h, &inverse_sqrt);
    let (values, vectors) = modes::spectrum(x, &weighted, masses)?;
    let imaginary = modes::imaginary_count(&values);
    if imaginary < 2 {
        return Err(TsError::Invalid(
            "extra-mode recovery requires at least two significant imaginary modes".into(),
        ));
    }
    let mut directions: Vec<_> = vectors
        .chunks_exact(n)
        .take(imaginary)
        .enumerate()
        .map(|(index, v)| {
            let mut direction: Vec<_> = v.iter().zip(&inverse_sqrt).map(|(v, w)| v * w).collect();
            let length = norm(&direction);
            direction.iter_mut().for_each(|v| *v /= length);
            (dot(&direction, tangent).abs(), index, direction)
        })
        .collect();
    // The largest-overlap mode is excluded from the symmetry-breaking seeds.
    // With a zero tangent, stable ordering keeps the lowest mode as reaction.
    let reaction = directions.iter().enumerate().fold(0, |best, (i, current)| {
        if current.0 > directions[best].0 {
            i
        } else {
            best
        }
    });
    let (_, _, reaction) = directions.remove(reaction);
    directions.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    Ok((
        reaction,
        directions.into_iter().map(|(_, _, v)| v).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_modes_use_significance_and_cartesian_overlap_order() {
        let x = [0., 0., 0., 2., 0., 0., 0., 2., 0., 0., 0., 2.];
        let masses: [f64; 4] = [1., 2., 4., 8.];
        let n = x.len();
        let (_, physical) = modes::spectrum(&x, &[0.; 144], &masses).unwrap();
        let sqrt: Vec<_> = masses.iter().flat_map(|m| [m.sqrt(); 3]).collect();
        let values = [-0.4, -0.3, -0.2, -0.00001, 0.7, 0.9];
        let mut h = vec![0.; n * n];
        for (value, vector) in values.iter().zip(physical.chunks_exact(n)) {
            for i in 0..n {
                for j in 0..n {
                    h[i * n + j] += value * vector[i] * vector[j] * sqrt[i] * sqrt[j];
                }
            }
        }
        let cartesian: Vec<Vec<_>> = physical
            .chunks_exact(n)
            .take(3)
            .map(|v| {
                let mut direction: Vec<_> = v.iter().zip(&sqrt).map(|(v, s)| v / s).collect();
                let length = norm(&direction);
                direction.iter_mut().for_each(|v| *v /= length);
                direction
            })
            .collect();
        let tangent = &cartesian[0];
        let (reaction, extra) = recovery_directions(&x, &h, &masses, tangent).unwrap();
        assert!(dot(&reaction, tangent).abs() > 1. - 1e-10);
        assert_eq!(
            extra.len(),
            2,
            "the weak fourth imaginary mode is below threshold"
        );
        assert!(dot(&extra[0], tangent).abs() <= dot(&extra[1], tangent).abs());
        for mode in extra {
            assert!((norm(&mode) - 1.).abs() < 1e-12);
            assert!(
                cartesian[1..]
                    .iter()
                    .any(|v| dot(v, &mode).abs() > 1. - 1e-10)
            );
        }
    }
}
