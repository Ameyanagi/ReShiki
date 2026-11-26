use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::Thermo;
use glam::DVec3;

mod symmetry;
#[cfg(test)]
mod symmetry_tests;

// SI definitions and CODATA 2022 recommended values.
const PLANCK_J_S: f64 = 6.626_070_15e-34;
const BOLTZMANN_J_PER_K: f64 = 1.380_649e-23;
const LIGHT_CM_PER_S: f64 = 2.997_924_58e10;
const AMU_KG: f64 = 1.660_539_068_92e-27;
const HARTREE_J: f64 = 4.359_744_722_206e-18;
const BOLTZMANN_HARTREE_PER_K: f64 = BOLTZMANN_J_PER_K / HARTREE_J;

/// Molecular geometry, analysis masses and settings for ideal-gas thermochemistry.
pub struct ThermoInput<'a> {
    /// Unscaled vibrational wavenumbers, with imaginary modes represented by negative values.
    pub frequencies_cm1: &'a [f64],
    /// One analysis mass per atom, in the same order as the positions.
    pub masses_amu: &'a [f64],
    /// Cartesian equilibrium geometry in ångströms.
    pub positions_angstrom: &'a [[f64; 3]],
    /// Rotational symmetry number (one or greater).
    pub symmetry_number: u32,
    /// Override the rigid-rotor type; `None` detects linearity from the geometry.
    /// Monatomic geometries always have no rotational contribution.
    pub linear: Option<bool>,
    /// Absolute temperature in kelvin.
    pub temperature_k: f64,
    /// Ideal-gas pressure in pascals.
    pub pressure_pa: f64,
    /// Positive multiplier applied to the vibrational wavenumbers.
    pub frequency_scale: f64,
    /// Wavenumber at which the harmonic and free-rotor entropy weights are equal.
    pub rotor_cutoff_cm1: f64,
    /// Magnitude cutoff for inverting small imaginary modes (absolute value in cm⁻¹).
    pub imaginary_cutoff_cm1: f64,
}

impl ThermoInput<'_> {
    /// Whether the effective rigid rotor is linear, including any override.
    ///
    /// With `linear: None`, uses the same scale-free inertia test as
    /// [`thermochemistry`]. Monatomic geometries have no rigid rotor and return
    /// `false`. Geometry and mass requirements are the same as for that function.
    pub fn is_linear(&self) -> bool {
        assert_eq!(self.masses_amu.len(), self.positions_angstrom.len());
        if self.masses_amu.len() <= 1 {
            return false;
        }
        self.linear.unwrap_or_else(|| {
            let total_mass = self.masses_amu.iter().sum();
            let (trace, determinant) =
                inertia_invariants(self.masses_amu, self.positions_angstrom, total_mass);
            linear_inertia(trace, determinant)
        })
    }
}

fn linear_inertia(trace: f64, determinant: f64) -> bool {
    // For a nearly linear rotor the two large moments approach trace/2.
    // This scale-free test corresponds to I_min/I_max <= 1e-10 in that limit.
    determinant <= 1e-10 * (trace / 2.0).powi(3)
}

fn center_of_mass(masses: &[f64], positions: &[[f64; 3]], total_mass: f64) -> [f64; 3] {
    let mut center = [0.0; 3];
    for (&mass, position) in masses.iter().zip(positions) {
        for (sum, coordinate) in center.iter_mut().zip(position) {
            *sum += mass * coordinate;
        }
    }
    center.map(|sum| sum / total_mass)
}

/// Rotational symmetry number for an isotope-labelled equilibrium geometry.
///
/// Proper rotations about the centre of mass must map each atom to an atom
/// with the same label within a Cartesian distance of 1e-3 Å. Use labels
/// `Z * 1000 + isotope`, with isotope 0 for natural atoms. The search tests
/// axes of orders 2 through 12. The search can undercount when distortions
/// approach the tolerance. Well-converged geometries typically have much
/// smaller deviations. A linear rotor has symmetry number 2 if
/// its ends can be exchanged, and 1 otherwise. A single atom returns 1.
/// Supply finite positions and positive finite masses, one per label.
pub fn rotational_symmetry_number(
    labels: &[u32],
    positions_angstrom: &[[f64; 3]],
    masses_amu: &[f64],
) -> u32 {
    assert_eq!(labels.len(), positions_angstrom.len());
    assert_eq!(labels.len(), masses_amu.len());
    if labels.len() <= 1 {
        return 1;
    }
    let center = DVec3::from(center_of_mass(
        masses_amu,
        positions_angstrom,
        masses_amu.iter().sum(),
    ));
    let positions: Vec<_> = positions_angstrom
        .iter()
        .map(|&p| DVec3::from(p) - center)
        .collect();
    symmetry::count(labels, &positions, masses_amu)
}

// Trace and determinant give the sum and product of the three principal moments
// (in amu Å²) without diagonalizing the symmetric inertia tensor.
fn inertia_invariants(masses: &[f64], positions: &[[f64; 3]], total_mass: f64) -> (f64, f64) {
    let center = center_of_mass(masses, positions, total_mass);
    let (mut xx, mut yy, mut zz, mut xy, mut xz, mut yz) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let [cx, cy, cz] = center;
    for (&mass, &[px, py, pz]) in masses.iter().zip(positions) {
        let (x, y, z) = (px - cx, py - cy, pz - cz);
        xx += mass * (y * y + z * z);
        yy += mass * (x * x + z * z);
        zz += mass * (x * x + y * y);
        xy -= mass * x * y;
        xz -= mass * x * z;
        yz -= mass * y * z;
    }
    let determinant =
        xx * (yy * zz - yz * yz) + xy * (yz * xz - xy * zz) + xz * (xy * yz - yy * xz);
    (xx + yy + zz, determinant)
}

/// Recompute ideal-gas corrections relative to the electronic energy, including ZPVE.
///
/// Uses classical translation and rigid rotation, harmonic vibrational enthalpies,
/// and the quasi-RRHO entropy of Grimme, Chem. Eur. J. 18, 9955 (2012),
/// <https://doi.org/10.1002/chem.201200497>. The heat-capacity field holds
/// the worker's constant-pressure value. Supply finite positive masses,
/// temperature, pressure and scale, a nonempty geometry, and nonnegative cutoffs.
pub fn thermochemistry(input: &ThermoInput<'_>) -> Thermo {
    assert_eq!(input.masses_amu.len(), input.positions_angstrom.len());
    let mass_amu: f64 = input.masses_amu.iter().sum();
    let kt_j = BOLTZMANN_J_PER_K * input.temperature_k;
    let kt_hartree = kt_j / HARTREE_J;
    let h_squared = PLANCK_J_S.powi(2);
    let translational_q =
        (2.0 * PI * mass_amu * AMU_KG * kt_j / h_squared).powf(1.5) * (kt_j / input.pressure_pa);
    let mut entropy_over_k = translational_q.ln() + 2.5;

    let (trace, determinant) =
        inertia_invariants(input.masses_amu, input.positions_angstrom, mass_amu);
    // Grimme (2012) uses a fixed B_av of 1e-44 kg m². The worker's soft-mode
    // entropies give the isotropic moment of this molecule instead.
    let mean_moment_kg_m2 = (trace / 3.0) * AMU_KG * 1e-20;
    let perpendicular_moment = trace / 2.0;
    let rotor_factor = 8.0 * PI.powi(2) * kt_j / h_squared;
    let rotor_factor_amu_angstrom2 = rotor_factor * AMU_KG * 1e-20;
    let (rotation_over_kt, rotational_q) = if input.masses_amu.len() == 1 {
        (0.0, 1.0)
    } else if input
        .linear
        .unwrap_or_else(|| linear_inertia(trace, determinant))
    {
        (
            1.0,
            rotor_factor_amu_angstrom2 * perpendicular_moment / f64::from(input.symmetry_number),
        )
    } else {
        (
            1.5,
            (PI * determinant * rotor_factor_amu_angstrom2.powi(3)).sqrt()
                / f64::from(input.symmetry_number),
        )
    };
    entropy_over_k += rotation_over_kt + rotational_q.ln();
    let mut enthalpy_hartree = (2.5 + rotation_over_kt) * kt_hartree;
    // The worker reports Cp. Translation includes the ideal-gas pV term, and
    // each quasi-RRHO mode contributes T dS/dT using Grimme's entropy weights.
    let mut capacity_over_k = 2.5 + rotation_over_kt;
    let mut zpve_hartree = 0.0;
    for &raw in input.frequencies_cm1 {
        // The worker scales first and inverts only imaginary modes strictly
        // inside the cutoff. Their magnitudes feed all four vibrational sums.
        let scaled = raw * input.frequency_scale;
        if scaled < 0.0 && -scaled >= input.imaginary_cutoff_cm1.abs() {
            continue;
        }
        let wavenumber = scaled.abs();
        // Sub-1 cm⁻¹ modes still contribute at their actual frequencies.
        // The harmonic partition function is undefined for an exact zero mode.
        if wavenumber == 0.0 {
            continue;
        }
        let hz = wavenumber * LIGHT_CM_PER_S;
        let quantum_j = PLANCK_J_S * hz;
        zpve_hartree += 0.5 * quantum_j / HARTREE_J;
        let x = quantum_j / kt_j;
        // Form 1-exp(-x) accurately for soft modes and avoid exp(+x) for cold ones.
        let denominator = -(-x).exp_m1();
        let thermal_over_kt = x * (-x).exp() / denominator;
        let harmonic_entropy = thermal_over_kt - denominator.ln();
        let harmonic_capacity = (x * (-0.5 * x).exp() / denominator).powi(2);
        let weight = 1.0 / (1.0 + (input.rotor_cutoff_cm1 / wavenumber).powi(4));
        let mu = PLANCK_J_S / (8.0 * PI.powi(2) * hz);
        let effective_moment = mu / (1.0 + mu / mean_moment_kg_m2);
        let rotor_entropy = 0.5 * (1.0 + (PI * rotor_factor * effective_moment).ln());
        entropy_over_k += weight * harmonic_entropy + (1.0 - weight) * rotor_entropy;
        capacity_over_k += weight * harmonic_capacity + (1.0 - weight) * 0.5;
        enthalpy_hartree += kt_hartree * thermal_over_kt;
    }
    enthalpy_hartree += zpve_hartree;
    let entropy_hartree_per_k = entropy_over_k * BOLTZMANN_HARTREE_PER_K;
    Thermo {
        temperature_k: input.temperature_k,
        pressure_pa: input.pressure_pa,
        zero_point_energy_hartree: zpve_hartree,
        enthalpy_correction_hartree: enthalpy_hartree,
        entropy_hartree_per_k,
        gibbs_correction_hartree: enthalpy_hartree - input.temperature_k * entropy_hartree_per_k,
        heat_capacity_hartree_per_k: Some(capacity_over_k * BOLTZMANN_HARTREE_PER_K),
        frequency_scale: input.frequency_scale,
        rotor_cutoff_cm1: input.rotor_cutoff_cm1,
        imaginary_cutoff_cm1: input.imaginary_cutoff_cm1,
        symmetry_number: input.symmetry_number,
        point_group: None,
    }
}

/// Area-normalized line shape used to broaden infrared transitions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandShape {
    /// Lorentzian profile, whose half width at half maximum is `fwhm / 2`.
    #[default]
    Lorentzian,
    /// Gaussian profile with the specified full width at half maximum.
    Gaussian,
}

#[derive(Clone, Copy)]
struct IrBand {
    center: f64,
    intensity: f64,
}

fn ir_bands(frequencies: &[f64], intensities: &[f64], scale: f64) -> Vec<IrBand> {
    // Stored vibrations can omit the optional IR intensities.
    if intensities.is_empty() {
        return Vec::new();
    }
    assert_eq!(
        frequencies.len(),
        intensities.len(),
        "each IR frequency must have an intensity"
    );
    if !scale.is_finite() || scale <= 0.0 {
        return Vec::new();
    }
    frequencies
        .iter()
        .zip(intensities)
        .filter_map(|(&frequency, &intensity)| {
            let center = frequency * scale;
            (frequency >= 0.0 && center.is_finite() && intensity.is_finite() && intensity > 0.0)
                .then_some(IrBand { center, intensity })
        })
        .collect()
}

fn ir_curve_value(bands: &[IrBand], x: f64, fwhm: f64, shape: BandShape) -> f64 {
    bands
        .iter()
        .map(|band| {
            let offset = (x - band.center) / fwhm;
            let profile = match shape {
                BandShape::Lorentzian => {
                    // The integral of 2 / (pi * w * (1 + 4 (x / w)^2)) is one.
                    (2.0 / std::f64::consts::PI) / fwhm / (1.0 + 4.0 * offset * offset)
                }
                BandShape::Gaussian => {
                    let exponent = 4.0 * std::f64::consts::LN_2;
                    (exponent / std::f64::consts::PI).sqrt() / fwhm
                        * (-exponent * offset * offset).exp()
                }
            };
            band.intensity * profile
        })
        .sum()
}

/// Sample an area-normalized infrared spectrum at evenly spaced wavenumbers.
///
/// Frequencies and the range are in cm⁻¹, input intensities are in km/mol, and
/// returned ordinates are in km/mol per cm⁻¹. Scaling moves band centers without
/// changing their integrated intensities or widths. Negative frequencies
/// (imaginary modes), nonfinite bands, and nonpositive intensities are omitted.
/// Zero-frequency bands are retained. The inclusive grid contains `points`
/// samples in the requested direction. A one-point grid uses the first bound.
///
/// Missing intensities give a zero spectrum. Otherwise each frequency must
/// have an intensity. An invalid scale, width, or nonfinite range returns empty.
pub fn ir_spectrum(
    frequencies_cm1: &[f64],
    intensities_km_mol: &[f64],
    scale: f64,
    fwhm_cm1: f64,
    shape: BandShape,
    range_cm1: (f64, f64),
    points: usize,
) -> Vec<[f64; 2]> {
    let bands = ir_bands(frequencies_cm1, intensities_km_mol, scale);
    let (start, end) = range_cm1;
    if points == 0
        || !scale.is_finite()
        || scale <= 0.0
        || !fwhm_cm1.is_finite()
        || fwhm_cm1 <= 0.0
        || !(end - start).is_finite()
    {
        return Vec::new();
    }
    (0..points)
        .map(|index| {
            let x = if points == 1 || index == 0 {
                start
            } else if index == points - 1 {
                end
            } else {
                start + (end - start) * (index as f64 / (points - 1) as f64)
            };
            [x, ir_curve_value(&bands, x, fwhm_cm1, shape)]
        })
        .collect()
}

fn ir_refine_maximum(
    bands: &[IrBand],
    mut lower: f64,
    mut upper: f64,
    fwhm: f64,
    shape: BandShape,
) -> f64 {
    // Golden-section refinement needs no finite-difference derivative and also
    // handles an isolated Gaussian whose distant tails underflow to zero.
    let ratio = 0.5 * (5.0_f64.sqrt() - 1.0);
    let original_bounds = [lower, upper];
    let mut left = upper - ratio * (upper - lower);
    let mut right = lower + ratio * (upper - lower);
    let mut left_y = ir_curve_value(bands, left, fwhm, shape);
    let mut right_y = ir_curve_value(bands, right, fwhm, shape);
    let tolerance = ir_peak_tolerance(fwhm);
    while upper - lower > tolerance {
        if left <= lower || right >= upper || left >= right {
            break;
        }
        if left_y < right_y {
            lower = left;
            left = right;
            left_y = right_y;
            right = lower + ratio * (upper - lower);
            right_y = ir_curve_value(bands, right, fwhm, shape);
        } else {
            upper = right;
            right = left;
            right_y = left_y;
            left = upper - ratio * (upper - lower);
            left_y = ir_curve_value(bands, left, fwhm, shape);
        }
    }
    [
        original_bounds[0],
        original_bounds[1],
        left,
        right,
        lower + 0.5 * (upper - lower),
    ]
    .into_iter()
    .max_by(|&first, &second| {
        ir_curve_value(bands, first, fwhm, shape)
            .total_cmp(&ir_curve_value(bands, second, fwhm, shape))
    })
    .expect("a refinement bracket always has candidates")
}

fn ir_peak_tolerance(fwhm: f64) -> f64 {
    // Relative precision keeps narrow peaks accurate enough to compare their
    // actual heights; the absolute limit resolves positions below 0.1 cm⁻¹.
    (fwhm * 1e-7).min(0.001)
}

/// Find local maxima of the summed, area-normalized infrared spectrum.
///
/// The first five arguments use the same conventions as [`ir_spectrum`]. Peak
/// positions are refined to better than 0.1 cm⁻¹; their ordinates are the summed
/// curve at those positions. Unrounded positions preserve narrow peak heights.
/// Close transitions can therefore yield one peak.
/// Peaks below `min_fraction` times the highest maximum are discarded. Of the
/// remainder, the `max_peaks` highest peaks are returned in descending frequency
/// order. A negative threshold is treated as zero; a nonfinite threshold or one
/// greater than one returns no peaks.
///
/// Invalid scale, invalid width, or missing intensities return no peaks.
/// Otherwise the frequency and intensity slices must have equal lengths.
pub fn ir_peaks(
    frequencies_cm1: &[f64],
    intensities_km_mol: &[f64],
    scale: f64,
    fwhm_cm1: f64,
    shape: BandShape,
    min_fraction: f64,
    max_peaks: usize,
) -> Vec<[f64; 2]> {
    let bands = ir_bands(frequencies_cm1, intensities_km_mol, scale);
    if bands.is_empty()
        || max_peaks == 0
        || !fwhm_cm1.is_finite()
        || fwhm_cm1 <= 0.0
        || !min_fraction.is_finite()
        || min_fraction > 1.0
    {
        return Vec::new();
    }

    let lower = bands
        .iter()
        .map(|band| band.center)
        .fold(f64::INFINITY, f64::min);
    let upper = bands
        .iter()
        .map(|band| band.center)
        .fold(f64::NEG_INFINITY, f64::max);
    // A maximum of a positive mixture must lie within one FWHM of at
    // least one center: outside those windows every component has positive
    // second derivative. Sampling these windows avoids traversing vast empty
    // gaps when a small width is requested for widely separated transitions.
    let mut grid = Vec::with_capacity(bands.len().saturating_mul(33));
    for band in &bands {
        for step in -16..=16 {
            let x = band.center + (f64::from(step) / 16.0) * fwhm_cm1;
            if x.is_finite() && x >= lower && x <= upper {
                grid.push(x);
            }
        }
    }
    grid.sort_by(f64::total_cmp);
    grid.dedup_by(|left, right| *left == *right);
    let samples: Vec<[f64; 2]> = grid
        .into_iter()
        .map(|x| [x, ir_curve_value(&bands, x, fwhm_cm1, shape)])
        .collect();
    let mut maxima = Vec::new();
    if let [[x, _]] = samples.as_slice() {
        maxima.push(*x);
    } else {
        // Endpoint maxima occur for isolated bands with negligible tails from
        // other transitions, as well as when all nonzero centers coincide.
        if let Some([[first_x, first_y], [second_x, second_y]]) = samples.windows(2).next()
            && first_y >= second_y
        {
            maxima.push(ir_refine_maximum(
                &bands, *first_x, *second_x, fwhm_cm1, shape,
            ));
        }
        for window in samples.windows(3) {
            if let [[before_x, before_y], [_, y], [after_x, after_y]] = window
                && y >= before_y
                && y >= after_y
                && (y > before_y || y > after_y)
            {
                maxima.push(ir_refine_maximum(
                    &bands, *before_x, *after_x, fwhm_cm1, shape,
                ));
            }
        }
        if let Some([[before_x, before_y], [last_x, last_y]]) = samples.windows(2).next_back()
            && last_y >= before_y
        {
            maxima.push(ir_refine_maximum(
                &bands, *before_x, *last_x, fwhm_cm1, shape,
            ));
        }
    }
    let mut peaks: Vec<[f64; 2]> = maxima
        .into_iter()
        .map(|position| [position, ir_curve_value(&bands, position, fwhm_cm1, shape)])
        .filter(|peak| peak[1].is_finite() && peak[1] > 0.0)
        .collect();
    peaks.sort_by(|left, right| left[0].total_cmp(&right[0]));
    peaks.dedup_by(|left, right| {
        if (left[0] - right[0]).abs() <= 4.0 * ir_peak_tolerance(fwhm_cm1) {
            if left[1] > right[1] {
                *right = *left;
            }
            true
        } else {
            false
        }
    });
    let highest = peaks.iter().map(|peak| peak[1]).fold(0.0_f64, f64::max);
    let threshold = min_fraction.max(0.0) * highest;
    peaks.retain(|peak| peak[1] >= threshold);
    peaks.sort_by(|left, right| {
        right[1]
            .total_cmp(&left[1])
            .then_with(|| right[0].total_cmp(&left[0]))
    });
    peaks.truncate(max_peaks);
    peaks.sort_by(|left, right| right[0].total_cmp(&left[0]));
    peaks
}

fn normalized_mode(positions: &[[f64; 3]], mode: &[f64]) -> Vec<[f64; 3]> {
    assert_eq!(
        positions.len().checked_mul(3),
        Some(mode.len()),
        "a mode must contain three Cartesian components per atom"
    );
    assert!(
        positions.iter().flatten().all(|value| value.is_finite()),
        "mode positions must be finite"
    );
    assert!(
        mode.iter().all(|value| value.is_finite()),
        "mode components must be finite"
    );
    // Normalize components before computing lengths, so even very large finite
    // input mode coefficients do not overflow a Cartesian norm.
    let component_scale = mode.iter().map(|value| value.abs()).fold(0.0_f64, f64::max);
    if component_scale == 0.0 {
        return vec![[0.0; 3]; positions.len()];
    }
    let mut vectors: Vec<[f64; 3]> = mode
        .as_chunks::<3>()
        .0
        .iter()
        .map(|xyz| xyz.map(|component| component / component_scale))
        .collect();
    let largest = vectors
        .iter()
        .map(|xyz| xyz[0].hypot(xyz[1]).hypot(xyz[2]))
        .fold(0.0_f64, f64::max);
    for xyz in &mut vectors {
        for value in xyz {
            *value /= largest;
        }
    }
    vectors
}

fn displace_along(positions: &[[f64; 3]], unit_mode: &[[f64; 3]], distance: f64) -> Vec<[f64; 3]> {
    positions
        .iter()
        .zip(unit_mode)
        .map(|(&[x, y, z], &[dx, dy, dz])| {
            [x + distance * dx, y + distance * dy, z + distance * dz]
        })
        .collect()
}

/// Displace a geometry along a Cartesian mode, in ångströms.
///
/// The largest per-atom displacement has magnitude `distance_angstrom.abs()`;
/// a negative distance reverses the mode. A zero mode leaves positions intact.
/// Panics if the mode length differs from three times the atom count or if any
/// input coordinate, mode coefficient, or requested distance is nonfinite.
pub fn displaced(
    positions_angstrom: &[[f64; 3]],
    mode: &[f64],
    distance_angstrom: f64,
) -> Vec<[f64; 3]> {
    assert!(
        distance_angstrom.is_finite(),
        "mode distance must be finite"
    );
    let unit = normalized_mode(positions_angstrom, mode);
    displace_along(positions_angstrom, &unit, distance_angstrom)
}

/// Sample one sinusoidal mode cycle, beginning at the equilibrium geometry.
///
/// Frame `k` has signed displacement
/// `amplitude_angstrom * sin(2π * k / frames)`. The endpoint is omitted so a
/// looping movie does not repeat its first frame. Shape and finite-input
/// requirements are the same as for [`displaced`]. Zero frames returns empty.
pub fn mode_frames(
    positions_angstrom: &[[f64; 3]],
    mode: &[f64],
    amplitude_angstrom: f64,
    frames: usize,
) -> Vec<Vec<[f64; 3]>> {
    assert!(
        amplitude_angstrom.is_finite(),
        "mode amplitude must be finite"
    );
    let unit = normalized_mode(positions_angstrom, mode);
    (0..frames)
        .map(|frame| {
            let distance =
                amplitude_angstrom * (std::f64::consts::TAU * (frame as f64 / frames as f64)).sin();
            displace_along(positions_angstrom, &unit, distance)
        })
        .collect()
}

const SYMBOLS: [&str; 118] = [
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge", "As",
    "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd", "In",
    "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb",
    "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl",
    "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk",
    "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn", "Nh",
    "Fl", "Mc", "Lv", "Ts", "Og",
];

/// Serialize an XYZ movie with ångström coordinates at eight decimal places.
///
/// Each frame has an atom-count line and the comment
/// `"<comment> · frame k of n"`, with one-based frame indices. Comment line
/// breaks are replaced by spaces. Panics for invalid atomic numbers, nonfinite
/// coordinates, or a frame whose atom count differs from `numbers.len()`.
pub fn xyz_movie(numbers: &[u8], frames: &[Vec<[f64; 3]>], comment: &str) -> String {
    use std::fmt::Write as _;

    assert!(
        numbers.iter().all(|&number| (1..=118).contains(&number)),
        "XYZ atomic numbers must lie between 1 and 118"
    );
    let comment = comment.replace(['\r', '\n'], " ");
    let mut output = String::new();
    for (frame_index, frame) in frames.iter().enumerate() {
        assert_eq!(
            frame.len(),
            numbers.len(),
            "XYZ frame atom counts must match"
        );
        assert!(
            frame.iter().flatten().all(|value| value.is_finite()),
            "XYZ coordinates must be finite"
        );
        writeln!(output, "{}", numbers.len()).expect("writing a string cannot fail");
        writeln!(
            output,
            "{comment} · frame {} of {}",
            frame_index + 1,
            frames.len()
        )
        .expect("writing a string cannot fail");
        for (&number, xyz) in numbers.iter().zip(frame) {
            writeln!(
                output,
                "{} {:.8} {:.8} {:.8}",
                SYMBOLS
                    .get(usize::from(number) - 1)
                    .expect("atomic numbers were checked above"),
                xyz[0],
                xyz[1],
                xyz[2]
            )
            .expect("writing a string cannot fail");
        }
    }
    output
}

#[cfg(test)]
mod thermo_tests {
    use super::*;

    #[test]
    fn soft_water_dimer_matches_stored_worker_thermochemistry() {
        // GFN2/Axtb Hessian of the unoptimized dimer in the worker comparison.
        let input = ThermoInput {
            frequencies_cm1: &[
                38.653175808245244,
                195.71777645689923,
                239.98032454200646,
                301.4929571370575,
                377.15957130735086,
                464.13380122918994,
                1584.4788140276714,
                1591.7994691920126,
                3510.116525414899,
                3511.122189799571,
                3524.091531188777,
                3525.6705877822683,
            ],
            masses_amu: &[15.999, 1.008, 1.008, 15.999, 1.008, 1.008],
            positions_angstrom: &[
                [0.0, 0.0, 0.119262],
                [0.0, 0.763239, -0.477047],
                [0.0, -0.763239, -0.477047],
                [3.5, 0.2, 0.519262],
                [3.5, 0.963239, -0.077047],
                [3.5, -0.563239, -0.077047],
            ],
            symmetry_number: 1,
            linear: None,
            temperature_k: 298.15,
            pressure_pa: 101_325.0,
            frequency_scale: 1.0,
            rotor_cutoff_cm1: 50.0,
            imaginary_cutoff_cm1: 20.0,
        };
        let t = thermochemistry(&input);
        for (actual, expected, tolerance) in [
            (t.zero_point_energy_hartree, 0.042976303472343276, 1e-12),
            (t.enthalpy_correction_hartree, 0.04968944907380403, 1e-12),
            (t.gibbs_correction_hartree, 0.01625952033340443, 1e-12),
            (t.entropy_hartree_per_k, 0.00011212453040549927, 1e-14),
            (
                t.heat_capacity_hartree_per_k.unwrap(),
                0.00002780956248387875,
                1e-14,
            ),
        ] {
            assert!(
                (actual - expected).abs() < tolerance,
                "{actual} vs {expected}"
            );
        }
        assert!(t.point_group.is_none());
    }

    #[test]
    fn linear_rotor_is_invariant_under_rotation_and_translation() {
        let input = ThermoInput {
            frequencies_cm1: &[],
            masses_amu: &[12.0, 16.0],
            positions_angstrom: &[[0.0; 3], [1.3, 0.0, 0.0]],
            symmetry_number: 1,
            linear: None,
            temperature_k: 298.15,
            pressure_pa: 101_325.0,
            frequency_scale: 1.0,
            rotor_cutoff_cm1: 50.0,
            imaginary_cutoff_cm1: 20.0,
        };
        let aligned = thermochemistry(&input);
        assert!(input.is_linear());
        let rotated = thermochemistry(&ThermoInput {
            positions_angstrom: &[[2.0, -3.0, 5.0], [2.3, -2.6, 6.2]],
            ..input
        });
        assert!((rotated.entropy_hartree_per_k - aligned.entropy_hartree_per_k).abs() < 1e-15);
        let enthalpy = 3.5 * BOLTZMANN_HARTREE_PER_K * input.temperature_k;
        assert!((rotated.enthalpy_correction_hartree - enthalpy).abs() < 1e-15);
    }

    #[test]
    fn linear_override_changes_the_rotational_terms_of_a_bent_geometry() {
        let mut input = ThermoInput {
            frequencies_cm1: &[],
            masses_amu: &[15.999, 1.008, 1.008],
            positions_angstrom: &[
                [0.0, 0.0, 0.119262],
                [0.0, 0.763239, -0.477047],
                [0.0, -0.763239, -0.477047],
            ],
            symmetry_number: 2,
            linear: None,
            temperature_k: 298.15,
            pressure_pa: 101_325.0,
            frequency_scale: 1.0,
            rotor_cutoff_cm1: 50.0,
            imaginary_cutoff_cm1: 20.0,
        };
        assert!(!input.is_linear());
        let bent = thermochemistry(&input);
        input.linear = Some(false);
        assert!(!input.is_linear());
        assert_eq!(thermochemistry(&input), bent);

        input.linear = Some(true);
        assert!(input.is_linear());
        let linear = thermochemistry(&input);
        let expected_enthalpy_difference = 0.5 * BOLTZMANN_HARTREE_PER_K * input.temperature_k;
        assert!(
            (bent.enthalpy_correction_hartree
                - linear.enthalpy_correction_hartree
                - expected_enthalpy_difference)
                .abs()
                < 1e-15
        );
        assert!(
            (bent.heat_capacity_hartree_per_k.unwrap()
                - linear.heat_capacity_hartree_per_k.unwrap()
                - 0.5 * BOLTZMANN_HARTREE_PER_K)
                .abs()
                < 1e-15
        );
        assert_ne!(linear.entropy_hartree_per_k, bent.entropy_hartree_per_k);
        assert_ne!(
            linear.gibbs_correction_hartree,
            bent.gibbs_correction_hartree
        );
        assert_eq!(
            linear.zero_point_energy_hartree,
            bent.zero_point_energy_hartree
        );
    }
}

#[cfg(test)]
mod spectrum_and_geometry_tests {
    use super::*;

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {actual} to be within {tolerance} of {expected}"
        );
    }

    fn trapezoid(points: &[[f64; 2]]) -> f64 {
        points
            .windows(2)
            .map(|pair| {
                let [left, right] = pair else {
                    unreachable!("a two-point window always has two points")
                };
                0.5 * (left[1] + right[1]) * (right[0] - left[0])
            })
            .sum()
    }

    #[test]
    fn gaussian_area_is_the_input_intensity() {
        let spectrum = ir_spectrum(
            &[1000.0],
            &[123.0],
            1.0,
            20.0,
            BandShape::Gaussian,
            (0.0, 2000.0),
            20_001,
        );
        close(trapezoid(&spectrum), 123.0, 1e-9);
    }

    #[test]
    fn lorentzian_area_matches_its_normalized_analytic_integral() {
        let spectrum = ir_spectrum(
            &[1000.0],
            &[123.0],
            1.0,
            20.0,
            BandShape::Lorentzian,
            (-9000.0, 11000.0),
            100_001,
        );
        let expected = 123.0 * 2.0 / std::f64::consts::PI * (1000.0_f64).atan();
        close(trapezoid(&spectrum), expected, 1e-9);
        close(trapezoid(&spectrum), 123.0, 0.08);
    }

    #[test]
    fn both_profiles_have_the_requested_fwhm() {
        for shape in [BandShape::Lorentzian, BandShape::Gaussian] {
            let spectrum = ir_spectrum(&[1000.0], &[100.0], 1.0, 20.0, shape, (990.0, 1010.0), 3);
            close(
                spectrum.first().unwrap()[1] / spectrum.get(1).unwrap()[1],
                0.5,
                1e-14,
            );
            close(
                spectrum.get(2).unwrap()[1] / spectrum.get(1).unwrap()[1],
                0.5,
                1e-14,
            );
        }
    }

    #[test]
    fn nearby_bands_merge_in_the_summed_curve() {
        for shape in [BandShape::Lorentzian, BandShape::Gaussian] {
            let peaks = ir_peaks(&[1000.0, 1010.0], &[50.0, 50.0], 1.0, 40.0, shape, 0.0, 10);
            assert_eq!(peaks.len(), 1);
            close(peaks.first().unwrap()[0], 1005.0, 0.05);
            let curve = ir_spectrum(
                &[1000.0, 1010.0],
                &[50.0, 50.0],
                1.0,
                40.0,
                shape,
                (1005.0, 1005.0),
                1,
            );
            close(peaks.first().unwrap()[1], curve.first().unwrap()[1], 1e-12);
        }
    }

    #[test]
    fn peaks_refine_an_asymmetric_sum_to_better_than_a_tenth() {
        let peaks = ir_peaks(
            &[1000.0, 1010.0],
            &[30.0, 70.0],
            1.0,
            40.0,
            BandShape::Gaussian,
            0.0,
            10,
        );
        assert_eq!(peaks.len(), 1);
        // Check the curve on each side of the refined maximum.
        let left = ir_spectrum(
            &[1000.0, 1010.0],
            &[30.0, 70.0],
            1.0,
            40.0,
            BandShape::Gaussian,
            (
                peaks.first().unwrap()[0] - 0.1,
                peaks.first().unwrap()[0] + 0.1,
            ),
            3,
        );
        assert!(left.get(1).unwrap()[1] > left.first().unwrap()[1]);
        assert!(left.get(1).unwrap()[1] > left.get(2).unwrap()[1]);
    }

    #[test]
    fn broad_asymmetric_peak_is_refined_inside_the_endpoint_interval() {
        for shape in [BandShape::Lorentzian, BandShape::Gaussian] {
            let peaks = ir_peaks(&[100.0, 200.0], &[1.0, 99.0], 1.0, 10_000.0, shape, 0.0, 10);
            assert_eq!(peaks.len(), 1);
            close(peaks.first().unwrap()[0], 199.0, 0.01);
        }
    }

    #[test]
    fn narrow_off_grid_peak_heights_preserve_thresholds_and_ranking() {
        for shape in [BandShape::Lorentzian, BandShape::Gaussian] {
            let peaks = ir_peaks(
                &[100.0123, 200.0456, 300.0789],
                &[1.0, 2.0, 3.0],
                1.0,
                1e-5,
                shape,
                0.5,
                10,
            );
            assert_eq!(peaks.len(), 2);
            close(peaks.first().unwrap()[0], 300.0789, 1e-10);
            close(peaks.get(1).unwrap()[0], 200.0456, 1e-10);
            close(
                peaks.get(1).unwrap()[1] / peaks.first().unwrap()[1],
                2.0 / 3.0,
                1e-12,
            );
            let highest = ir_peaks(
                &[100.0123, 200.0456, 300.0789],
                &[1.0, 2.0, 3.0],
                1.0,
                1e-5,
                shape,
                0.0,
                1,
            );
            assert_eq!(highest.len(), 1);
            close(highest.first().unwrap()[0], 300.0789, 1e-10);
        }
    }

    #[test]
    fn scale_moves_centers_without_changing_area_or_width() {
        let spectrum = ir_spectrum(
            &[1000.0],
            &[123.0],
            0.9,
            20.0,
            BandShape::Gaussian,
            (0.0, 1800.0),
            18_001,
        );
        close(trapezoid(&spectrum), 123.0, 1e-9);
        let peaks = ir_peaks(&[1000.0], &[123.0], 0.9, 20.0, BandShape::Gaussian, 0.0, 10);
        assert_eq!(peaks.len(), 1);
        close(peaks.first().unwrap()[0], 900.0, 1e-12);
        close(
            spectrum.get(8900).unwrap()[1] / spectrum.get(9000).unwrap()[1],
            0.5,
            1e-12,
        );
    }

    #[test]
    fn peaks_keep_the_highest_and_return_descending_frequencies() {
        let peaks = ir_peaks(
            &[100.0, 500.0, 900.0],
            &[5.0, 20.0, 10.0],
            1.0,
            2.0,
            BandShape::Gaussian,
            0.3,
            5,
        );
        assert_eq!(peaks.len(), 2);
        close(peaks.first().unwrap()[0], 900.0, 1e-6);
        close(peaks.get(1).unwrap()[0], 500.0, 1e-6);
        let highest = ir_peaks(
            &[100.0, 500.0, 900.0],
            &[5.0, 20.0, 10.0],
            1.0,
            2.0,
            BandShape::Gaussian,
            0.0,
            1,
        );
        assert_eq!(highest.len(), 1);
        close(highest.first().unwrap()[0], 500.0, 1e-6);
    }

    #[test]
    fn zero_frequency_is_retained_and_imaginary_modes_are_skipped() {
        let spectrum = ir_spectrum(
            &[-100.0, 0.0],
            &[100.0, 20.0],
            1.0,
            10.0,
            BandShape::Lorentzian,
            (0.0, 0.0),
            1,
        );
        close(
            spectrum.first().unwrap()[1],
            4.0 / std::f64::consts::PI,
            1e-14,
        );
        let peaks = ir_peaks(
            &[-100.0, 0.0],
            &[100.0, 20.0],
            1.0,
            10.0,
            BandShape::Lorentzian,
            0.0,
            10,
        );
        assert_eq!(peaks.len(), 1);
        close(peaks.first().unwrap()[0], 0.0, 0.0);
    }

    #[test]
    fn spectrum_grid_includes_both_endpoints() {
        let spectrum = ir_spectrum(
            &[12.0],
            &[],
            1.0,
            10.0,
            BandShape::Gaussian,
            (20.0, 10.0),
            3,
        );
        assert_eq!(spectrum, vec![[20.0, 0.0], [15.0, 0.0], [10.0, 0.0]]);
        assert_eq!(
            ir_spectrum(&[], &[], 1.0, 10.0, BandShape::Gaussian, (10.0, 20.0), 1),
            vec![[10.0, 0.0]]
        );
        assert!(ir_spectrum(&[], &[], 1.0, 10.0, BandShape::Gaussian, (10.0, 20.0), 0).is_empty());
    }

    #[test]
    fn displacement_is_normalized_by_largest_atom_excursion() {
        let positions = [[1.0, 2.0, 3.0], [-1.0, 0.0, 2.0]];
        let mode = [3.0, 4.0, 0.0, 0.0, 0.0, 2.0];
        let shifted = displaced(&positions, &mode, 0.2);
        close(shifted.first().unwrap()[0], 1.12, 1e-14);
        close(shifted.first().unwrap()[1], 2.16, 1e-14);
        close(shifted.get(1).unwrap()[2], 2.08, 1e-14);
        close(
            (shifted.first().unwrap()[0] - 1.0).hypot(shifted.first().unwrap()[1] - 2.0),
            0.2,
            1e-14,
        );
        let reversed = displaced(&positions, &mode, -0.2);
        close(reversed.first().unwrap()[0], 0.88, 1e-14);
        assert_eq!(displaced(&positions, &[0.0; 6], 1.0), positions);
    }

    #[test]
    fn mode_frames_follow_one_sine_cycle_without_repeating_endpoint() {
        let positions = [[1.0, 2.0, 3.0]];
        let frames = mode_frames(&positions, &[0.0, 0.0, 7.0], 0.4, 4);
        assert_eq!(frames.len(), 4);
        for (frame, expected_z) in frames.iter().zip([3.0, 3.4, 3.0, 2.6]) {
            close(frame.first().unwrap()[0], 1.0, 1e-14);
            close(frame.first().unwrap()[1], 2.0, 1e-14);
            close(frame.first().unwrap()[2], expected_z, 1e-14);
        }
        assert!(mode_frames(&positions, &[0.0, 0.0, 7.0], 0.4, 0).is_empty());
    }

    #[test]
    fn very_large_mode_coefficients_do_not_overflow_normalization() {
        let moved = displaced(&[[0.0; 3]], &[f64::MAX, f64::MAX, 0.0], 0.5);
        close(
            moved.first().unwrap()[0].hypot(moved.first().unwrap()[1]),
            0.5,
            1e-14,
        );
    }

    #[test]
    fn xyz_movie_has_counts_comments_symbols_and_eight_decimals() {
        let frames = vec![
            vec![[1.0, -2.0, 0.25], [0.0, 3.0, 4.0]],
            vec![[1.5, -2.5, 0.0], [0.0, 3.5, 4.5]],
        ];
        let xyz = xyz_movie(&[1, 118], &frames, "stretch");
        assert_eq!(
            xyz,
            "2\nstretch · frame 1 of 2\nH 1.00000000 -2.00000000 0.25000000\nOg 0.00000000 3.00000000 4.00000000\n2\nstretch · frame 2 of 2\nH 1.50000000 -2.50000000 0.00000000\nOg 0.00000000 3.50000000 4.50000000\n"
        );
        let mut lines = xyz.lines();
        for frame in &frames {
            let count: usize = lines.next().unwrap().parse().unwrap();
            assert_eq!(count, frame.len());
            lines.next().unwrap(); // comment
            assert_eq!(lines.by_ref().take(count).count(), count);
        }
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn xyz_comments_cannot_create_extra_lines() {
        let xyz = xyz_movie(&[6], &[vec![[0.0; 3]]], "one\ntwo");
        assert_eq!(xyz.lines().count(), 3);
        assert_eq!(xyz.lines().nth(1), Some("one two · frame 1 of 1"));
    }

    #[test]
    #[should_panic(expected = "three Cartesian components")]
    fn mismatched_mode_length_panics() {
        displaced(&[[0.0; 3]], &[1.0, 0.0], 1.0);
    }

    #[test]
    #[should_panic(expected = "XYZ frame atom counts must match")]
    fn mismatched_xyz_frame_atom_count_panics() {
        xyz_movie(&[1, 8], &[vec![[0.0; 3]]], "invalid");
    }
}
