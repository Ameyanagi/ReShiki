use super::{OrbitalField, orbital_meshes};
use crate::models3d::surface::{mesh::Mesh, tests::topology};
use reshiki_qcdata::{ANGSTROM_PER_BOHR, BasisSet, Shell};
use std::f64::consts::{PI, TAU};

fn shell(l: u8, pure: bool) -> Shell {
    Shell {
        atom: 0,
        l,
        pure,
        exponents_per_bohr2: vec![0.45, 1.2],
        coefficients: vec![1.4, -0.4],
    }
}

// Simpson in radius, Gauss-Legendre in cos(theta), and periodic trapezoids
// in phi. The angular rules integrate squared harmonics through l=5 exactly.
fn norm_quadrature() -> Vec<([f32; 3], f64)> {
    const RADIAL_STEPS: usize = 128;
    const PHI_STEPS: usize = 16;
    const POLAR: [(f64, f64); 6] = [
        (-0.932_469_514_203_152, 0.171_324_492_379_170),
        (-0.661_209_386_466_265, 0.360_761_573_048_139),
        (-0.238_619_186_083_197, 0.467_913_934_572_691),
        (0.238_619_186_083_197, 0.467_913_934_572_691),
        (0.661_209_386_466_265, 0.360_761_573_048_139),
        (0.932_469_514_203_152, 0.171_324_492_379_170),
    ];
    let dr = 10. / RADIAL_STEPS as f64;
    let dphi = TAU / PHI_STEPS as f64;
    let mut samples = Vec::new();
    // At r=0 the volume weight is zero; the omitted tail decays as exp(-90).
    for i in 1..=RADIAL_STEPS {
        let r = i as f64 * dr;
        let radial_weight = if i == RADIAL_STEPS {
            1.
        } else if i % 2 == 0 {
            2.
        } else {
            4.
        };
        for (z, polar_weight) in POLAR {
            let xy = (1. - z * z).sqrt();
            for j in 0..PHI_STEPS {
                let (sin, cos) = (j as f64 * dphi).sin_cos();
                let point = [xy * cos, xy * sin, z].map(|x| (x * r * ANGSTROM_PER_BOHR) as f32);
                // psi is in bohr^-3/2, so the integration volume is in bohr^3.
                let weight = radial_weight * dr / 3. * r * r * polar_weight * dphi;
                samples.push((point, weight));
            }
        }
    }
    samples
}

#[test]
fn contracted_atomic_orbitals_have_unit_norm_by_quadrature() {
    let quadrature = norm_quadrature();
    for (l, pure) in [
        (0, false),
        (1, false),
        (2, false),
        (2, true),
        (3, false),
        (3, true),
        (5, false),
        (5, true),
    ] {
        let basis = BasisSet {
            shells: vec![shell(l, pure)],
        };
        let n_ao = basis.shells[0].n_functions();
        for component in 0..n_ao {
            let mut row = vec![0.; n_ao];
            row[component] = 1.;
            let field = OrbitalField::new(&basis, &row, &[[0.; 3]]).unwrap();
            let norm: f64 = quadrature
                .iter()
                .map(|&(p, weight)| f64::from(field.value(p)).powi(2) * weight)
                .sum();
            assert!(
                (norm - 1.).abs() < 3e-6,
                "l={l}, pure={pure}, component={component}, norm={norm:.9}"
            );
        }
    }
}

#[test]
fn diffuse_h_shell_keeps_its_angular_tail() {
    let basis = BasisSet {
        shells: vec![Shell {
            exponents_per_bohr2: vec![0.001, 1.],
            coefficients: vec![1., 1.],
            ..shell(5, false)
        }],
    };
    let mut row = vec![0.; basis.shells[0].n_functions()];
    row[0] = 1.; // x^5 in canonical Cartesian order.
    let field = OrbitalField::new(&basis, &row, &[[0.; 3]]).unwrap();
    let value = field.value([(50. * ANGSTROM_PER_BOHR) as f32, 0., 0.]);
    assert!((f64::from(value) - 0.002_393_001_56).abs() < 1e-9);
}

#[test]
fn batch_samples_match_scalar_values_on_translated_rectangular_grid() {
    let basis = BasisSet {
        shells: [(0, 2, false), (0, 3, true), (1, 5, false), (1, 4, true)]
            .into_iter()
            .map(|(atom, l, pure)| Shell {
                atom,
                ..shell(l, pure)
            })
            .collect(),
    };
    let n_ao: usize = basis.shells.iter().map(Shell::n_functions).sum();
    let row: Vec<_> = (0..n_ao).map(|i| (i as f64 * 1.37).sin()).collect();
    let field = OrbitalField::new(&basis, &row, &[[4.2, -3.1, 1.3], [6.5, -1.7, 2.8]]).unwrap();
    let grid = super::Grid {
        origin: [2.13, -5.41, 0.27],
        step: 0.37,
        size: [19, 13, 11],
    };
    let values: Vec<_> = field.sample(grid).collect();
    assert_eq!(values.len(), grid.size.iter().product::<usize>());
    let peak = values.iter().copied().map(f32::abs).fold(0., f32::max);
    let [nx, ny, _] = grid.size;
    for (i, &actual) in values.iter().enumerate() {
        let ijk = [i % nx, (i / nx) % ny, i / (nx * ny)];
        let point = std::array::from_fn(|axis| {
            (f64::from(grid.origin[axis]) + ijk[axis] as f64 * f64::from(grid.step)) as f32
        });
        assert!(
            (actual - field.value(point)).abs() <= 1e-7 * peak,
            "sample {i}"
        );
    }
}

fn phase_centroid(mesh: &Mesh, positive: bool) -> [f64; 3] {
    let mut sum = [0.; 3];
    let mut total_area = 0.;
    for (&triangle, &phase) in mesh.triangles.iter().zip(&mesh.positive) {
        if phase != positive {
            continue;
        }
        let [a, b, c] = triangle.map(|v| mesh.positions[v as usize].map(f64::from));
        let ab: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
        let ac: [f64; 3] = std::array::from_fn(|i| c[i] - a[i]);
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let area = cross.iter().map(|x| x * x).sum::<f64>().sqrt() * 0.5;
        total_area += area;
        for (i, total) in sum.iter_mut().enumerate() {
            *total += area * (a[i] + b[i] + c[i]) / 3.;
        }
    }
    assert!(total_area > 0., "phase {positive} has no surface");
    sum.map(|x| x / total_area)
}

#[test]
fn translated_p_orbital_keeps_phase_and_closed_meshes() {
    let centres = [
        [-1.25, 0.375, -0.5],
        [0.375, -0.25, 0.625],
        [1.625, 0.875, -0.125],
    ]
    .map(|p| std::array::from_fn(|i| p[i] + [5., -3., 2.][i]));
    // Shell order differs from atom order; the sole nonzero AO is atom 1's p_z.
    let basis = BasisSet {
        shells: vec![
            Shell {
                atom: 2,
                ..shell(0, false)
            },
            shell(0, false),
            Shell {
                atom: 1,
                exponents_per_bohr2: vec![0.6],
                coefficients: vec![1.],
                ..shell(1, false)
            },
        ],
    };
    let field = OrbitalField::new(&basis, &[0., 0., 0., 0., 1.], &centres).unwrap();
    let center = centres[1];
    let at = |d: [f64; 3]| field.value(std::array::from_fn(|i| (center[i] + d[i]) as f32));
    assert!(at([0.25, -0.125, 0.5]) > 0.);
    assert!(at([0.25, -0.125, -0.5]) < 0.);
    assert!(at([0.25, -0.125, 0.]).abs() < 1e-7);
    // Independently fix the amplitude as well as the plane, so a unit error fails.
    let d = [0.25, -0.125, 0.5].map(|x| x / ANGSTROM_PER_BOHR);
    let expected = (1.2 / PI).powf(0.75)
        * 2.4_f64.sqrt()
        * d[2]
        * (-0.6 * d.iter().map(|x| x * x).sum::<f64>()).exp();
    assert!((f64::from(at([0.25, -0.125, 0.5])) - expected).abs() < 1e-6);

    let points = centres.map(|p| p.map(|x| x as f32));
    let meshes = orbital_meshes(&field, &points, 0.05).expect("p lobes must fit the grown box");
    assert!(meshes[1].triangles.len() <= meshes[0].triangles.len());
    for mesh in &meshes {
        assert!(!mesh.truncated);
        // This checks opposing edge uses, one cycle per vertex link, outward
        // winding, and two spherical connected components, one of each phase.
        assert_eq!(topology(mesh), [(false, 2), (true, 2)]);
        for positive in [false, true] {
            let centroid = phase_centroid(mesh, positive);
            for (&actual, &expected) in centroid.iter().zip(&center).take(2) {
                assert!(
                    (actual - expected).abs() < 0.03,
                    "phase {positive} centroid {centroid:?} misses atom {center:?}"
                );
            }
            let signed_height = (centroid[2] - center[2]) * if positive { 1. } else { -1. };
            assert!(
                signed_height > 0.2,
                "phase {positive} centroid {centroid:?} is on the wrong side of {center:?}"
            );
        }
    }
}

#[test]
fn orbital_meshes_distinguish_above_peak_from_persistent_truncation() {
    let basis = BasisSet {
        shells: vec![Shell {
            exponents_per_bohr2: vec![0.001],
            coefficients: vec![1.],
            ..shell(0, false)
        }],
    };
    let field = OrbitalField::new(&basis, &[1.], &[[0.; 3]]).unwrap();
    let peak = field.value([0.; 3]);
    let meshes = orbital_meshes(&field, &[[0.; 3]], 2. * peak).unwrap();
    for mesh in meshes {
        assert!(mesh.positions.is_empty());
        assert!(mesh.triangles.is_empty());
        assert!(!mesh.truncated);
    }
    // This diffuse lobe still crosses the final box after all three growths.
    let iso = 0.25 * peak;
    assert!(field.value([10.125, 0., 0.]) > iso);
    assert!(orbital_meshes(&field, &[[0.; 3]], iso).is_none());
}

#[test]
fn new_requires_one_coefficient_per_ao_and_indexed_atom_centres() {
    let basis = BasisSet {
        shells: vec![
            Shell {
                atom: 1,
                ..shell(1, false)
            },
            shell(0, false),
        ],
    };
    let centres = [[0.; 3], [1., -2., 3.]];
    OrbitalField::new(&basis, &[0., 0., 1., 0.], &centres).unwrap();
    for len in [0, 3, 5] {
        assert!(
            OrbitalField::new(&basis, &vec![0.; len], &centres).is_err(),
            "accepted {len} coefficients for four AOs"
        );
    }
    for count in [0, 1] {
        // An all-zero row must still validate the atoms of inactive shells.
        assert!(
            OrbitalField::new(&basis, &[0.; 4], &centres[..count]).is_err(),
            "accepted {count} centres for a shell on atom 1"
        );
    }
}
