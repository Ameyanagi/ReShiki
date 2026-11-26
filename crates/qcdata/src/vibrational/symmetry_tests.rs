use super::rotational_symmetry_number;

// Canonical labels encode atomic number * 1000 + explicit isotope number.
const H: u32 = 1000;
const D: u32 = 1002;
const HE: u32 = 2000;
const C: u32 = 6000;
const N: u32 = 7000;
const O: u32 = 8000;
const F: u32 = 9000;
const S: u32 = 16000;

fn masses(labels: &[u32]) -> Vec<f64> {
    labels
        .iter()
        .map(|label| match *label {
            H => 1.007_825_032,
            D => 2.014_101_778,
            HE => 4.002_603_254,
            C => 12.0,
            N => 14.003_074_004,
            O => 15.994_914_620,
            F => 18.998_403_163,
            S => 31.972_071_174,
            _ => panic!("unknown test atom label {label}"),
        })
        .collect()
}

fn check(labels: &[u32], positions: &[[f64; 3]], expected: u32) {
    assert_eq!(
        rotational_symmetry_number(labels, positions, &masses(labels)),
        expected,
        "atom labels: {labels:?}"
    );
}

// Idealized Cartesian geometries in ångströms, shared by isotopologues.
const WATER: [[f64; 3]; 3] = [[0.0, 0.0, 0.0], [0.757, 0.586, 0.0], [-0.757, 0.586, 0.0]];
const METHANE: [[f64; 3]; 5] = [
    [0.0, 0.0, 0.0],
    [0.63, 0.63, 0.63],
    [0.63, -0.63, -0.63],
    [-0.63, 0.63, -0.63],
    [-0.63, -0.63, 0.63],
];
const BENZENE: [[f64; 3]; 12] = [
    [1.4, 0.0, 0.0],
    [0.7, 1.212_435_565_298_214, 0.0],
    [-0.7, 1.212_435_565_298_214, 0.0],
    [-1.4, 0.0, 0.0],
    [-0.7, -1.212_435_565_298_214, 0.0],
    [0.7, -1.212_435_565_298_214, 0.0],
    [2.48, 0.0, 0.0],
    [1.24, 2.147_743_001_385_408, 0.0],
    [-1.24, 2.147_743_001_385_408, 0.0],
    [-2.48, 0.0, 0.0],
    [-1.24, -2.147_743_001_385_408, 0.0],
    [1.24, -2.147_743_001_385_408, 0.0],
];

#[test]
fn water_isotopologues_have_expected_symmetry() {
    check(&[O, H, H], &WATER, 2);
    check(&[O, H, D], &WATER, 1);
    check(&[O, D, D], &WATER, 2);
}

#[test]
fn ammonia_has_three_proper_rotations() {
    let positions = [
        [0.0, 0.0, 0.35],
        [0.94, 0.0, 0.0],
        [-0.47, 0.814_063_879_557_372, 0.0],
        [-0.47, -0.814_063_879_557_372, 0.0],
    ];
    check(&[N, H, H, H], &positions, 3);
}

#[test]
fn methane_isotopologues_have_expected_symmetry() {
    check(&[C, H, H, H, H], &METHANE, 12);
    check(&[C, H, H, D, D], &METHANE, 2);
    check(&[C, H, D, D, D], &METHANE, 3);
    check(&[C, D, D, D, D], &METHANE, 12);
}

#[test]
fn benzene_and_perdeuterobenzene_have_twelve_proper_rotations() {
    check(&[C, C, C, C, C, C, H, H, H, H, H, H], &BENZENE, 12);
    check(&[C, C, C, C, C, C, D, D, D, D, D, D], &BENZENE, 12);
}

#[test]
fn alternating_h_d_benzene_uses_isotope_labels_without_a_com_shift() {
    let labels = [C, C, C, C, C, C, H, D, H, D, H, D];
    let masses = masses(&labels);
    // Each isotope forms a centered equilateral triangle. The unchanged COM
    // means the reduction from D6h (12) to D3h (6) requires label matching.
    for axis in 0..3 {
        let weighted_coordinate: f64 = BENZENE
            .iter()
            .zip(&masses)
            .map(|(position, mass)| position[axis] * mass)
            .sum();
        assert!(weighted_coordinate.abs() < 1e-12);
    }
    assert_eq!(rotational_symmetry_number(&labels, &BENZENE, &masses), 6);
}

#[test]
fn ethylene_has_four_proper_rotations() {
    let positions = [
        [-0.67, 0.0, 0.0],
        [0.67, 0.0, 0.0],
        [-1.24, 0.92, 0.0],
        [-1.24, -0.92, 0.0],
        [1.24, 0.92, 0.0],
        [1.24, -0.92, 0.0],
    ];
    check(&[C, C, H, H, H, H], &positions, 4);
}

#[test]
fn linear_rotors_have_expected_symmetry() {
    let carbon_dioxide = [[0.0, 0.0, -1.16], [0.0, 0.0, 0.0], [0.0, 0.0, 1.16]];
    let hydrogen_cyanide = [[0.0, 0.0, -1.06], [0.0, 0.0, 0.0], [0.0, 0.0, 1.16]];
    check(&[O, C, O], &carbon_dioxide, 2);
    check(&[H, C, N], &hydrogen_cyanide, 1);
    check(&[H, D], &[[1.0, 1.0, 1.0], [2.0, 3.0, 4.0]], 1);
}

#[test]
fn a_nearly_linear_rotor_can_exchange_ends_with_a_half_turn() {
    // The carbon lies within the line tolerance. Inversion misses it by
    // more than 1e-3 Å, but the end-for-end half-turn fixes it exactly.
    let positions = [[-1.16, 0.0, 0.0], [0.0, 0.0012, 0.0], [1.16, 0.0, 0.0]];
    check(&[O, C, O], &positions, 2);
}

#[test]
fn overlapping_position_tolerances_allow_one_to_one_matching() {
    let positions = [
        [-1.0004, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0004, 0.0, 0.0],
    ];
    check(&[H; 4], &positions, 2);
}

#[test]
fn sulfur_hexafluoride_has_twenty_four_proper_rotations() {
    let positions = [
        [0.0, 0.0, 0.0],
        [1.56, 0.0, 0.0],
        [-1.56, 0.0, 0.0],
        [0.0, 1.56, 0.0],
        [0.0, -1.56, 0.0],
        [0.0, 0.0, 1.56],
        [0.0, 0.0, -1.56],
    ];
    check(&[S, F, F, F, F, F, F], &positions, 24);
}

#[test]
fn cubic_symmetry_is_independent_of_coordinate_axes() {
    // Fourfold axes point along (1, 2, -2), (2, 1, 2), and (2, -2, -1).
    let positions = [
        [0.0, 0.0, 0.0],
        [0.52, 1.04, -1.04],
        [-0.52, -1.04, 1.04],
        [1.04, 0.52, 1.04],
        [-1.04, -0.52, -1.04],
        [1.04, -1.04, -0.52],
        [-1.04, 1.04, 0.52],
    ];
    check(&[S, F, F, F, F, F, F], &positions, 24);
}

#[test]
fn higher_order_axes_include_every_distinct_power() {
    for n in [5, 7, 8, 12] {
        let positions: Vec<_> = (0..n)
            .map(|k| {
                let (s, c) = (std::f64::consts::TAU * f64::from(k) / f64::from(n)).sin_cos();
                [c, s, 0.0]
            })
            .collect();
        check(&vec![C; positions.len()], &positions, 2 * n);
    }
}

#[test]
fn alternating_ninefold_ring_has_eighteen_proper_rotations() {
    let sector = std::f64::consts::TAU / 9.0;
    let positions: [_; 18] = std::array::from_fn(|i| {
        // Each repeat spans one sector with successive gaps of 45% and 55%.
        let angle = ((i / 2) as f64 + 0.45 * (i % 2) as f64) * sector;
        let (sin, cos) = angle.sin_cos();
        [4.0 * cos, 4.0 * sin, 0.0]
    });
    check(&[C; 18], &positions, 18);
}

#[test]
fn allene_d2d_has_four_proper_rotations() {
    let positions = [
        [0.0, 0.0, -1.3],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.3],
        [0.94, 0.0, -1.84],
        [-0.94, 0.0, -1.84],
        [0.0, 0.94, 1.84],
        [0.0, -0.94, 1.84],
    ];
    check(&[C, C, C, H, H, H, H], &positions, 4);
}

#[test]
fn staggered_ethane_has_six_proper_rotations() {
    let positions = [
        [0.0, 0.0, -0.77],
        [0.0, 0.0, 0.77],
        [1.02, 0.0, -1.12],
        [-0.51, 0.883_345_911_860_127, -1.12],
        [-0.51, -0.883_345_911_860_127, -1.12],
        [0.51, 0.883_345_911_860_127, 1.12],
        [-1.02, 0.0, 1.12],
        [0.51, -0.883_345_911_860_127, 1.12],
    ];
    check(&[C, C, H, H, H, H, H, H], &positions, 6);
}

#[test]
fn methane_isotopologues_tolerate_one_ten_thousandth_angstrom_noise() {
    // Each atom is displaced by at most 1e-4 Å from METHANE.
    let positions = [
        [0.000_02, -0.000_01, 0.000_03],
        [0.630_1, 0.63, 0.63],
        [0.629_96, -0.629_95, -0.630_03],
        [-0.629_98, 0.630_04, -0.630_06],
        [-0.630_05, -0.630_04, 0.630_02],
    ];
    check(&[C, H, H, H, H], &positions, 12);
    check(&[C, D, D, D, D], &positions, 12);
}

#[test]
fn monatomic_geometry_has_one_proper_rotation() {
    check(&[HE], &[[4.0, -2.0, 7.0]], 1);
}

#[test]
fn two_hundred_same_radius_atoms_have_cubic_symmetry() {
    // Four generic cubic orbits of 48 sites plus eight cube corners. Every
    // atom has radius sqrt(3), so all 200 atoms are possible rotation images.
    let mut positions = Vec::with_capacity(200);
    let mut add_signed_vertices = |vertex: [f64; 3]| {
        for signs in 0..8 {
            positions.push(std::array::from_fn(|axis| {
                vertex[axis] * if signs & (1 << axis) == 0 { 1.0 } else { -1.0 }
            }));
        }
    };
    // Each seed has squared length 675 = 3 * 15² before scaling.
    for seed in [
        [1.0, 7.0, 25.0],
        [3.0, 15.0, 21.0],
        [5.0, 11.0, 23.0],
        [5.0, 17.0, 19.0],
    ] {
        for permutation in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            add_signed_vertices(permutation.map(|axis| seed[axis] / 15.0));
        }
    }
    add_signed_vertices([1.0; 3]);
    let labels = [C; 200];
    let masses = masses(&labels);
    let started = std::time::Instant::now();
    let symmetry = rotational_symmetry_number(&labels, &positions, &masses);
    let elapsed = started.elapsed();
    eprintln!("200-atom rotational symmetry analysis took {elapsed:?}");
    assert_eq!(symmetry, 24);
}
