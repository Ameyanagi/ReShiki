//! Independent AO mathematics written for ReShiki (MIT OR Apache-2.0).
//!
//! The evaluator implements the canonical contract in docs/plans/xtb-data.md without
//! production normalization, ordering, or harmonic helpers. The synthetic experiment
//! expands a finite Legendre polynomial, independently of the evaluator's recurrence.
//! The fixture tests compare this evaluator with signed grid values from PySCF,
//! including ORCA references whose convention is fixed by implied AO overlap.
use crate::qcread::tests::{fixture, open};
use reshiki_qcdata::{
    ANGSTROM_PER_BOHR, BasisSet, CartesianNorm, Frame, Orbitals, Shell, mo_from_molden,
    mo_to_molden, primitive_norm,
};
use std::{collections::BTreeMap, f64::consts::PI};

fn factorial(n: u8) -> f64 {
    (1..=n).map(f64::from).product()
}

// (2n-1)!!, including (-1)!! = 1.
fn odd_factorial(n: u8) -> f64 {
    (0..n).map(|i| f64::from(2 * i + 1)).product()
}

fn cartesian_powers(l: u8) -> Vec<[u8; 3]> {
    (0..=l)
        .rev()
        .flat_map(|x| (0..=l - x).rev().map(move |y| [x, y, l - x - y]))
        .collect()
}

// Integral of the squared monomial times exp(-p r²), with p^(-l-3/2) removed.
fn cartesian_factor([x, y, z]: [u8; 3]) -> f64 {
    PI.powf(1.5) * odd_factorial(x) * odd_factorial(y) * odd_factorial(z)
        / 2.0_f64.powi(i32::from(x + y + z))
}

// Radial integral Gamma(l+3/2)/2 times the angular integral of the real
// associated Legendre function. Remove the same p^(-l-3/2) as above.
fn pure_factor(l: u8, m: u8) -> f64 {
    let gamma = PI.sqrt() * odd_factorial(l + 1) / 2.0_f64.powi(i32::from(l + 1));
    let angular = if m == 0 {
        4.0 * PI / f64::from(2 * l + 1)
    } else {
        2.0 * PI * factorial(l + m) / (f64::from(2 * l + 1) * factorial(l - m))
    };
    gamma * angular / 2.0
}

fn primitive_weight(alpha: f64, l: u8) -> f64 {
    (2.0 * alpha / PI).powf(0.75) * (4.0 * alpha).powf(f64::from(l) / 2.0) / odd_factorial(l).sqrt()
}

// Associated Legendre recurrence without the Condon–Shortley phase.
fn legendre(l: u8, m: u8, u: f64) -> f64 {
    let mut previous = odd_factorial(m) * (1.0 - u * u).max(0.0).powf(f64::from(m) / 2.0);
    if l == m {
        return previous;
    }
    let mut current = f64::from(2 * m + 1) * u * previous;
    for degree in m + 2..=l {
        let next = (f64::from(2 * degree - 1) * u * current - f64::from(degree + m - 1) * previous)
            / f64::from(degree - m);
        previous = current;
        current = next;
    }
    current
}

struct Evaluator {
    l: u8,
    pure: bool,
    centre: [f64; 3],
    radial: Vec<(f64, f64)>,
    scales: Vec<f64>,
}

impl Evaluator {
    fn new(shell: &Shell, centre: [f64; 3]) -> Self {
        let radial: Vec<_> = shell
            .exponents_per_bohr2
            .iter()
            .zip(&shell.coefficients)
            .map(|(&a, &d)| (a, d * primitive_weight(a, shell.l)))
            .collect();
        let radial_overlap: f64 = radial
            .iter()
            .flat_map(|&(a, da)| {
                radial
                    .iter()
                    .map(move |&(b, db)| da * db / (a + b).powf(f64::from(shell.l) + 1.5))
            })
            .sum();
        let factors: Vec<_> = if shell.pure {
            (-(shell.l as i8)..=shell.l as i8)
                .map(|m| pure_factor(shell.l, m.unsigned_abs()))
                .collect()
        } else {
            cartesian_powers(shell.l)
                .into_iter()
                .map(cartesian_factor)
                .collect()
        };
        Self {
            l: shell.l,
            pure: shell.pure,
            centre,
            radial,
            scales: factors
                .into_iter()
                .map(|f| (f * radial_overlap).sqrt().recip())
                .collect(),
        }
    }

    fn at(&self, point: [f64; 3]) -> Vec<f64> {
        let [x, y, z] = std::array::from_fn(|axis| point[axis] - self.centre[axis]);
        let r2 = x * x + y * y + z * z;
        let radial: f64 = self.radial.iter().map(|&(a, d)| d * (-a * r2).exp()).sum();
        if !self.pure {
            return cartesian_powers(self.l)
                .into_iter()
                .zip(&self.scales)
                .map(|([a, b, c], scale)| {
                    scale * radial * x.powi(a.into()) * y.powi(b.into()) * z.powi(c.into())
                })
                .collect();
        }
        if r2 == 0.0 {
            return vec![0.0; self.scales.len()];
        }
        let r = r2.sqrt();
        let phi = y.atan2(x);
        (-(self.l as i8)..=self.l as i8)
            .zip(&self.scales)
            .map(|(m, scale)| {
                let angle = f64::from(m.unsigned_abs()) * phi;
                let azimuth = if m < 0 { angle.sin() } else { angle.cos() };
                scale
                    * radial
                    * r.powi(self.l.into())
                    * legendre(self.l, m.unsigned_abs(), (z / r).clamp(-1.0, 1.0))
                    * azimuth
            })
            .collect()
    }
}

fn evaluate_basis(basis: &BasisSet, frame: &Frame) -> Vec<Evaluator> {
    basis
        .shells
        .iter()
        .map(|shell| {
            let centre =
                frame.positions_angstrom[shell.atom as usize].map(|x| x / ANGSTROM_PER_BOHR);
            Evaluator::new(shell, centre)
        })
        .collect()
}

fn wavefunction(evaluators: &[Evaluator], row: &[f64], point: [f64; 3]) -> f64 {
    evaluators
        .iter()
        .flat_map(|shell| shell.at(point))
        .zip(row)
        .map(|(a, c)| a * c)
        .sum()
}

struct Reference {
    source: String,
    points: Vec<[f64; 3]>,
    orbitals: Vec<ReferenceOrbital>,
}

struct ReferenceOrbital {
    index: u32,
    energy: f64,
    values: Vec<f64>,
}

fn read_reference(stem: &str) -> Reference {
    let text = String::from_utf8(fixture(&format!("reference/{stem}.txt"))).unwrap();
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let source = lines.next().unwrap().strip_prefix("source ").unwrap();
    let n_points = lines.next().unwrap().strip_prefix("points ").unwrap();
    let n_points: usize = n_points.parse().unwrap();
    let points: Vec<_> = lines
        .by_ref()
        .take(n_points)
        .map(|line| {
            let mut fields = line.split_whitespace();
            let point = std::array::from_fn(|_| fields.next().unwrap().parse().unwrap());
            assert!(fields.next().is_none());
            point
        })
        .collect();
    assert_eq!(points.len(), n_points);
    assert!(!points.is_empty());
    let orbitals: Vec<_> = lines
        .map(|line| {
            let mut fields = line.split_whitespace();
            assert_eq!(fields.next(), Some("orbital"));
            assert_eq!(fields.next(), Some("a"));
            let index = fields.next().unwrap().parse().unwrap();
            let energy = fields.next().unwrap().parse().unwrap();
            let values: Vec<_> = fields.map(|field| field.parse().unwrap()).collect();
            assert_eq!(values.len(), n_points);
            ReferenceOrbital {
                index,
                energy,
                values,
            }
        })
        .collect();
    assert!(!orbitals.is_empty());
    Reference {
        source: source.into(),
        points,
        orbitals,
    }
}

fn reference_error(reference: &Reference, orbitals: &Orbitals, frame: &Frame) -> f64 {
    let basis = orbitals.basis.as_ref().expect("reference requires a basis");
    let evaluators = evaluate_basis(basis, frame);
    let n_ao: usize = basis.shells.iter().map(Shell::n_functions).sum();
    let alpha = &orbitals.alpha;
    let mut worst = 0.0_f64;
    for orbital in &reference.orbitals {
        assert!(
            (orbital.energy - alpha.energies_hartree[orbital.index as usize]).abs() < 1e-6,
            "{} orbital {} energy mismatch",
            reference.source,
            orbital.index
        );
        let index = alpha.rows.binary_search(&orbital.index).unwrap();
        let row = &alpha.coefficients[index * n_ao..(index + 1) * n_ao];
        let observed: Vec<_> = reference
            .points
            .iter()
            .map(|&point| wavefunction(&evaluators, row, point))
            .collect();
        let dot: f64 = observed
            .iter()
            .zip(&orbital.values)
            .map(|(a, b)| a * b)
            .sum();
        let sign = dot.signum();
        for (actual, expected) in observed.iter().zip(&orbital.values) {
            let error = (sign * actual - expected).abs();
            assert!(error.is_finite());
            worst = worst.max(error);
        }
    }
    worst
}

fn compare(reference: &Reference) -> (f64, usize, usize) {
    let file = open(&reference.source);
    let job = &file.jobs[0];
    let orbitals = job.result.as_ref().unwrap().orbitals.as_ref().unwrap();
    let frame = &job.result_step().unwrap().frame;
    let error = reference_error(reference, orbitals, frame);
    (error, reference.points.len(), reference.orbitals.len())
}

macro_rules! reference_tests {
    ($($test:ident => $stem:literal),+ $(,)?) => {
        $(
            #[test]
            fn $test() {
                let (error, points, orbitals) = compare(&read_reference($stem));
                eprintln!("reference {}: {orbitals} orbitals x {points} points, max |error| = {error:.3e}", $stem);
                assert!(error < 1e-9, "{}: {error:e}", $stem);
            }
        )+
    };
}

reference_tests! {
    reference_ch2o_ccpvtz_pure => "ch2o_ccpvtz_pure",
    reference_h2o_ccpvqz_pure => "h2o_ccpvqz_pure",
    reference_h2o_ccpvqz_cart => "h2o_ccpvqz_cart",
    reference_ch3nh2_631gs_cart => "ch3nh2_631gs_cart",
    reference_tih2f2_def2tzvp_pure => "tih2f2_def2tzvp_pure",
    reference_h2o_ccpvqz_mixed_a => "h2o_ccpvqz_mixed_a",
    reference_h2o_ccpvqz_mixed_b => "h2o_ccpvqz_mixed_b",
    reference_g09_mo4ocl4_sp => "g09_Mo4OCl4-sp",
    reference_orca611_water_tzvp => "orca611_water_tzvp",
    reference_orca611_h2co_tddft => "orca611_h2co_tddft",
    reference_orca_ch3f_pbe0 => "orca_ch3f_pbe0",
}

#[test]
fn orbital_references_detect_perturbed_components() {
    #[derive(Clone, Copy)]
    enum Perturbation {
        Swap(u8),
        FlipPureD,
        FlipPureF3,
        BareRadial,
    }
    use Perturbation::{BareRadial, FlipPureD, FlipPureF3, Swap};

    let cases: [(&str, &[Perturbation]); 7] = [
        ("h2o_ccpvqz_pure", &[Swap(2), Swap(3), Swap(4), FlipPureD]),
        ("h2o_ccpvqz_cart", &[Swap(2), Swap(3), Swap(4)]),
        ("ch3nh2_631gs_cart", &[Swap(2)]),
        ("g09_Mo4OCl4-sp", &[Swap(2), FlipPureD]),
        ("orca611_water_tzvp", &[FlipPureF3, BareRadial]),
        ("orca611_h2co_tddft", &[Swap(2)]),
        ("orca_ch3f_pbe0", &[Swap(2), FlipPureD, BareRadial]),
    ];
    for (stem, perturbations) in cases {
        let reference = read_reference(stem);
        let file = open(&reference.source);
        let job = &file.jobs[0];
        let orbitals = job.result.as_ref().unwrap().orbitals.as_ref().unwrap();
        let frame = &job.result_step().unwrap().frame;
        let basis = orbitals.basis.as_ref().unwrap();
        let n_ao: usize = basis.shells.iter().map(Shell::n_functions).sum();
        for &change in perturbations {
            let mut perturbed = orbitals.clone();
            if matches!(change, BareRadial) {
                for shell in &mut perturbed.basis.as_mut().unwrap().shells {
                    for (&alpha, coefficient) in shell
                        .exponents_per_bohr2
                        .iter()
                        .zip(&mut shell.coefficients)
                    {
                        *coefficient *= primitive_norm(alpha, shell.l);
                    }
                }
            } else {
                for row in perturbed.alpha.coefficients.chunks_exact_mut(n_ao) {
                    let mut offset = 0;
                    for shell in &basis.shells {
                        let end = offset + shell.n_functions();
                        match change {
                            Swap(l) if shell.l == l => row.swap(offset, offset + 1),
                            FlipPureD if shell.l == 2 && shell.pure => row[end - 1] *= -1.0,
                            FlipPureF3 if shell.l == 3 && shell.pure => {
                                row[offset] *= -1.0;
                                row[end - 1] *= -1.0;
                            }
                            _ => {}
                        }
                        offset = end;
                    }
                }
            }
            let error = reference_error(&reference, &perturbed, frame);
            let label = match change {
                Swap(l) => format!(
                    "swap first two {} components",
                    ["d", "f", "g"][usize::from(l - 2)]
                ),
                FlipPureD => "flip last pure d component".into(),
                FlipPureF3 => "undo pure f(±3) sign flips".into(),
                BareRadial => "undo radial rescaling".into(),
            };
            eprintln!("perturbed {stem} ({label}): max |error| = {error:.3e}");
            assert!(error > 1e-3, "{stem} ({label}): {error:e}");
        }
    }
}

// Gauss–Legendre quadrature on [-1,1], found by Newton iteration on P_n.
fn quadrature(n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let mut x = (PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
            let derivative = loop {
                let (mut previous, mut current) = (1.0, x);
                for degree in 2..=n {
                    let next = ((2 * degree - 1) as f64 * x * current
                        - (degree - 1) as f64 * previous)
                        / degree as f64;
                    previous = current;
                    current = next;
                }
                let derivative = n as f64 * (x * current - previous) / (x * x - 1.0);
                let next = x - current / derivative;
                if (next - x).abs() < 2e-15 {
                    x = next;
                    break derivative;
                }
                x = next;
            };
            (x, 2.0 / ((1.0 - x * x) * derivative * derivative))
        })
        .collect()
}

fn synthetic_shell(l: u8, pure: bool) -> Shell {
    Shell {
        atom: u32::from(l % 2),
        l,
        pure,
        exponents_per_bohr2: vec![0.38 + 0.07 * f64::from(l), 1.7 + 0.13 * f64::from(l)],
        coefficients: vec![0.75, -0.12],
    }
}

#[test]
fn independent_d_f_g_aos_have_unit_norm_by_quadrature() {
    // Synthetic signed two-primitive contractions, evaluated about a displaced centre.
    let centre = [0.31, -0.27, 0.19];
    let shells: Vec<_> = (2..=4)
        .flat_map(|l| {
            [false, true].map(move |pure| Evaluator::new(&synthetic_shell(l, pure), centre))
        })
        .collect();
    let mut integrals: Vec<_> = shells.iter().map(|s| vec![0.0; s.scales.len()]).collect();
    // Integrate |AO|² directly with the r² Jacobian: radial [0,12] × cos(theta)
    // [-1,1] × periodic phi. No analytic AO overlap is used for this check.
    for (x, wr) in quadrature(64) {
        let r = 6.0 * (x + 1.0);
        for (u, wu) in quadrature(16) {
            let rho = r * (1.0 - u * u).sqrt();
            for azimuth in 0..24 {
                let phi = 2.0 * PI * f64::from(azimuth) / 24.0;
                let relative = [rho * phi.cos(), rho * phi.sin(), r * u];
                let point = std::array::from_fn(|axis| centre[axis] + relative[axis]);
                let weight = 6.0 * wr * wu * 2.0 * PI / 24.0 * r * r;
                for (shell, integrals) in shells.iter().zip(&mut integrals) {
                    for (ao, integral) in shell.at(point).into_iter().zip(integrals) {
                        *integral += weight * ao * ao;
                    }
                }
            }
        }
    }
    let mut worst = 0.0_f64;
    for (shell, integrals) in shells.iter().zip(integrals) {
        for (component, integral) in integrals.into_iter().enumerate() {
            worst = worst.max((integral - 1.0).abs());
            assert!(
                (integral - 1.0).abs() < 1e-8,
                "l={} pure={} component={component}: {integral}",
                shell.l,
                shell.pure
            );
        }
    }
    eprintln!("independent d/f/g AO quadrature maximum norm error: {worst:.9e}");
}

// The Molden specification's printed component positions, encoded independently.
fn printed_order(shell: &Shell) -> Vec<usize> {
    if shell.pure {
        let l = usize::from(shell.l);
        return std::iter::once(l)
            .chain((1..=l).flat_map(|m| [l + m, l - m]))
            .collect();
    }
    let labels = [
        "s",
        "x y z",
        "xx yy zz xy xz yz",
        "xxx yyy zzz xyy xxy xxz xzz yzz yyz xyz",
        "xxxx yyyy zzzz xxxy xxxz yyyx yyyz zzzx zzzy xxyy xxzz yyzz xxyz yyxz zzxy",
    ];
    let canonical = cartesian_powers(shell.l);
    labels[usize::from(shell.l)]
        .split_whitespace()
        .map(|label| {
            let powers =
                b"xyz".map(|axis| label.bytes().filter(|&letter| letter == axis).count() as u8);
            canonical.iter().position(|&p| p == powers).unwrap()
        })
        .collect()
}

fn axial_scale([x, y, z]: [u8; 3]) -> f64 {
    (odd_factorial(x) * odd_factorial(y) * odd_factorial(z) / odd_factorial(x + y + z)).sqrt()
}

// Single-centre monomial integrals from integration by parts. This is independent
// of both production overlap helpers and the numerical evaluator above.
fn centered_overlap(a: (&Shell, [u8; 3]), b: (&Shell, [u8; 3])) -> f64 {
    let powers = std::array::from_fn::<_, 3, _>(|axis| a.1[axis] + b.1[axis]);
    if powers.iter().any(|p| p % 2 != 0) {
        return 0.0;
    }
    let mut overlap = 0.0;
    for (&alpha, &da) in a.0.exponents_per_bohr2.iter().zip(&a.0.coefficients) {
        for (&beta, &db) in b.0.exponents_per_bohr2.iter().zip(&b.0.coefficients) {
            let moments: f64 = powers
                .iter()
                .map(|&n| {
                    (1..n)
                        .step_by(2)
                        .fold((PI / (alpha + beta)).sqrt(), |v, k| {
                            v * f64::from(k) / (2.0 * (alpha + beta))
                        })
                })
                .product();
            overlap +=
                da * db * primitive_weight(alpha, a.0.l) * primitive_weight(beta, b.0.l) * moments;
        }
    }
    overlap
}

fn overlap_error(rows: &[Vec<f64>], overlap: &[Vec<f64>]) -> f64 {
    let mut worst = 0.0_f64;
    for (i, row) in rows.iter().enumerate() {
        let weighted: Vec<f64> = overlap
            .iter()
            .map(|s| s.iter().zip(row).map(|(a, b)| a * b).sum())
            .collect();
        for (j, other) in rows[..=i].iter().enumerate() {
            let value: f64 = other.iter().zip(&weighted).map(|(a, b)| a * b).sum();
            worst = worst.max((value - f64::from(i == j)).abs());
        }
    }
    worst
}

#[test]
fn molden_57_carbon_orthonormality_distinguishes_unit_from_axial() {
    // All four cclib Molden 5.7 fixtures were derived from GAMESS-US 2018 runs;
    // they are not same-calculation partners of the similarly named Gaussian fchk.
    // Other fixture counts/energies are covered in molden::carbon_water_and_divinylbenzene_spectra.
    // Carbon is the discriminating fixture:
    // "C      1  6     0.000000     0.000000     0.000000", 29 GTO shells
    // through "g     1 1.00", and 109 "Sym= A" blocks ending at "139   -0.000000".
    let name = "cclib/molden5.7_C_bigbasis.molden";
    let file = open(name);
    let orbitals = file.jobs[0]
        .result
        .as_ref()
        .unwrap()
        .orbitals
        .as_ref()
        .unwrap();
    let basis = orbitals.basis.as_ref().unwrap();
    assert_eq!(basis.shells.len(), 29);
    let aos: Vec<_> = basis
        .shells
        .iter()
        .flat_map(|shell| {
            assert!(!shell.pure && shell.atom == 0);
            cartesian_powers(shell.l)
                .into_iter()
                .map(move |powers| (shell, powers))
        })
        .collect();
    assert_eq!(aos.len(), 139);
    assert_eq!(orbitals.alpha.rows.len(), 109);
    // First MO: "Ene=   -11.3649", "Occup=   2.000000", "1    0.993702".
    assert_eq!(orbitals.alpha.energies_hartree[0], -11.3649);
    assert_eq!(orbitals.alpha.occupations[0], 2.0);
    assert_eq!(orbitals.alpha.coefficients[0], 0.993702);
    let raw = String::from_utf8(fixture(name)).unwrap();
    let rows: Vec<Vec<f64>> = raw
        .split_once("[MO]")
        .unwrap()
        .1
        .split("Sym=")
        .skip(1)
        .map(|block| {
            let printed: Vec<f64> = block
                .lines()
                .filter_map(|line| {
                    let mut fields = line.split_whitespace();
                    fields.next()?.parse::<usize>().ok()?;
                    Some(fields.next().unwrap().parse().unwrap())
                })
                .collect();
            assert_eq!(printed.len(), aos.len());
            let mut canonical = vec![0.0; aos.len()];
            let mut offset = 0;
            for shell in &basis.shells {
                for (source, target) in printed_order(shell).into_iter().enumerate() {
                    canonical[offset + target] = printed[offset + source];
                }
                offset += shell.n_functions();
            }
            canonical
        })
        .collect();
    assert_eq!(rows.len(), 109);
    assert_eq!(
        rows.iter().flatten().copied().collect::<Vec<_>>(),
        orbitals.alpha.coefficients
    );
    let norms: Vec<_> = aos.iter().map(|&a| centered_overlap(a, a).sqrt()).collect();
    let overlap: Vec<Vec<_>> = aos
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            aos.iter()
                .enumerate()
                .map(|(j, &b)| centered_overlap(a, b) / (norms[i] * norms[j]))
                .collect()
        })
        .collect();
    let axial_rows: Vec<Vec<_>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .zip(&aos)
                .map(|(&c, &(_, powers))| c * axial_scale(powers))
                .collect()
        })
        .collect();
    let unit = overlap_error(&rows, &overlap);
    let axial = overlap_error(&axial_rows, &overlap);
    eprintln!("Molden 5.7 carbon max |C S Cᵀ - I|: Unit {unit:.9e}, Axial {axial:.9e}");
    // With the printed six-decimal coefficients, Unit has a 2.9e-6 residual;
    // the Axial interpretation misses orthonormality by approximately 0.94.
    assert!(unit < 3e-6, "Unit: {unit}");
    assert!((0.93..0.95).contains(&axial), "Axial: {axial}");
}

// Expand r^l P_l^|m|(z/r) times cos(m phi) or sin(|m| phi). Differentiate
// the finite Legendre polynomial and multiply by Re/Im (x+i y)^|m|.
fn harmonic_polynomial(l: u8, signed_m: i8) -> BTreeMap<[u8; 3], f64> {
    let m = signed_m.unsigned_abs();
    let mut polynomial = BTreeMap::new();
    for k in 0..=(l - m) / 2 {
        let legendre = (-1.0_f64).powi(k.into()) * factorial(2 * l - 2 * k)
            / (2.0_f64.powi(l.into()) * factorial(k) * factorial(l - k) * factorial(l - 2 * k - m));
        for y in 0..=m {
            if (y % 2 == 1) != (signed_m < 0) {
                continue;
            }
            let azimuth = (-1.0_f64).powi(i32::from(y / 2)) * factorial(m)
                / (factorial(y) * factorial(m - y));
            for a in 0..=k {
                for b in 0..=k - a {
                    let c = k - a - b;
                    let factor = factorial(k) / (factorial(a) * factorial(b) * factorial(c));
                    let powers = [m - y + 2 * a, y + 2 * b, l - 2 * k - m + 2 * c];
                    *polynomial.entry(powers).or_default() += legendre * azimuth * factor;
                }
            }
        }
    }
    polynomial
}

fn cartesian_expansion(shell: &Shell, pure: &[f64]) -> Vec<f64> {
    let mut expanded = BTreeMap::<[u8; 3], f64>::new();
    for (m, &coefficient) in (-(shell.l as i8)..=shell.l as i8).zip(pure) {
        for (powers, polynomial) in harmonic_polynomial(shell.l, m) {
            let ratio = (cartesian_factor(powers) / pure_factor(shell.l, m.unsigned_abs())).sqrt();
            *expanded.entry(powers).or_default() += coefficient * polynomial * ratio;
        }
    }
    cartesian_powers(shell.l)
        .iter()
        .map(|p| expanded.get(p).copied().unwrap_or(0.0))
        .collect()
}

#[test]
fn synthetic_same_wavefunction_tests_molden_converters_on_signed_grids() {
    // Construct one pure d/f/g contraction per l at two explicit centres. Every
    // component is nonzero. These are synthetic wavefunctions, not fixture pairs.
    let frame = Frame {
        numbers: vec![2, 2],
        positions_angstrom: vec![[0.13, -0.21, 0.08], [-0.27, 0.19, 0.31]],
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let pure = BasisSet {
        shells: (2..=4).map(|l| synthetic_shell(l, true)).collect(),
    };
    let source: Vec<f64> = (0..21)
        .map(|i| (0.37 * f64::from(i + 1)).sin() - 0.21)
        .collect();
    let reference = evaluate_basis(&pure, &frame);
    // Pure, Cartesian, and mixed d/f/g target bases represent exactly this same
    // function. Algebraic polynomial expansion supplies the Cartesian coefficients.
    for keep_pure in [
        [true; 3],
        [false; 3],
        [true, false, true],
        [false, true, false],
    ] {
        let mut row = Vec::new();
        let mut offset = 0;
        let basis = BasisSet {
            shells: pure
                .shells
                .iter()
                .zip(keep_pure)
                .map(|(shell, keep)| {
                    let block = &source[offset..offset + shell.n_functions()];
                    row.extend(if keep {
                        block.to_vec()
                    } else {
                        cartesian_expansion(shell, block)
                    });
                    offset += shell.n_functions();
                    Shell {
                        pure: keep,
                        ..shell.clone()
                    }
                })
                .collect(),
        };
        let evaluators = evaluate_basis(&basis, &frame);
        for norm in [CartesianNorm::Unit, CartesianNorm::Axial] {
            // Encode the source independently. Neither reference coefficients nor
            // reference grid values pass through a production Molden converter.
            let mut printed = Vec::new();
            let mut offset = 0;
            for shell in &basis.shells {
                let powers = cartesian_powers(shell.l);
                for canonical in printed_order(shell) {
                    let scale = if norm == CartesianNorm::Axial && !shell.pure {
                        axial_scale(powers[canonical])
                    } else {
                        1.0
                    };
                    printed.push(row[offset + canonical] / scale);
                }
                offset += shell.n_functions();
            }
            let imported = mo_from_molden(&basis, &printed, norm).unwrap();
            let exported = mo_to_molden(&basis, &row, norm).unwrap();
            assert_eq!(imported.len(), row.len());
            assert_eq!(exported.len(), printed.len());
            for (&actual, &expected) in imported
                .iter()
                .zip(&row)
                .chain(exported.iter().zip(&printed))
            {
                assert!(
                    (actual - expected).abs() < 2e-14,
                    "{keep_pure:?} {norm:?}: {actual} vs {expected}"
                );
            }
            let mut worst = 0.0_f64;
            for i in 0..64 {
                let t = f64::from(i + 1);
                let point = [
                    (1.23 * t).sin() * 2.8 + 0.11,
                    (0.91 * t).cos() * 2.3 - 0.19,
                    (0.57 * t).sin() * 2.6 + 0.27,
                ];
                let expected = wavefunction(&reference, &source, point);
                let actual = wavefunction(&evaluators, &imported, point);
                worst = worst.max((actual - expected).abs());
            }
            assert!(
                worst < 2e-13,
                "{keep_pure:?} {norm:?}: signed grid error {worst:e}"
            );
        }
    }
}
