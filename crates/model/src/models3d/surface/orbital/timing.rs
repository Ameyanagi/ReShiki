//! Manual timings: `cargo test -p reshiki-model --release orbital -- --ignored --nocapture`.
//! Both workloads use synthetic MO rows with active AOs of both signs.

use super::{OrbitalField, orbital_grid};
use crate::models3d::surface::{draw::DETAIL, mesh};
use reshiki_qcdata::{BasisSet, Shell};
use std::{
    cell::Cell,
    f64::consts::TAU,
    time::{Duration, Instant},
};

const ISO: f32 = 0.03;

fn fixture(carbons: usize) -> (Vec<[f64; 3]>, BasisSet, Vec<f64>) {
    let mut centres = Vec::new();
    let mut shells = Vec::new();
    let standard = [0.154_328_97, 0.535_328_14, 0.444_634_54];
    let valence = [2.941_249_4, 0.683_483_1, 0.222_289_9];
    for hydrogen in [false, true] {
        for i in 0..carbons {
            let h = if hydrogen { 1.09 } else { 0. };
            let (radius, angle, z) = if carbons == 6 {
                (1.397 + h, TAU * i as f64 / 6., 0.)
            } else {
                // The zigzag helix has 1.36 Å C-C bonds and 4.7 Å between turns.
                let side = if i % 2 == 0 { 1. } else { -1. };
                (3.6 + side * (0.35 + h), i as f64 * 0.32, i as f64 * 0.24)
            };
            let atom = centres.len() as u32;
            let (sin, cos) = angle.sin_cos();
            centres.push([radius * cos, radius * sin, z]);
            // Carbon core s and valence s/p, hydrogen s. Exponents are in bohr⁻².
            let definitions = if hydrogen {
                vec![(0, [3.425_250_91, 0.623_913_73, 0.168_855_40], standard)]
            } else {
                vec![
                    (0, [71.616_837, 13.045_096, 3.530_512_2], standard),
                    (0, valence, [-0.099_967_23, 0.399_512_83, 0.700_115_47]),
                    (1, valence, [0.155_916_27, 0.607_683_72, 0.391_957_39]),
                ]
            };
            for (l, exponents, coefficients) in definitions {
                shells.push(Shell {
                    atom,
                    l,
                    pure: false,
                    exponents_per_bohr2: exponents.to_vec(),
                    coefficients: coefficients.to_vec(),
                });
            }
        }
    }
    let basis = BasisSet { shells };
    let n_ao: usize = basis.shells.iter().map(Shell::n_functions).sum();
    let mut row: Vec<_> = (0..n_ao)
        .map(|i| {
            let sign = if i * 7 % 11 < 5 { 1. } else { -1. };
            sign * (0.6 + (i * 17 % 13) as f64 * 0.05)
        })
        .collect();
    // Scale the synthetic row to unit Euclidean length in both workloads.
    let norm = row.iter().map(|c| c * c).sum::<f64>().sqrt();
    row.iter_mut().for_each(|c| *c /= norm);
    (centres, basis, row)
}

fn measure(name: &str, carbons: usize) {
    let (centres, basis, row) = fixture(carbons);
    let points: Vec<_> = centres.iter().map(|p| p.map(|x| x as f32)).collect();
    let field = OrbitalField::new(&basis, &row, &centres).unwrap();
    let start = Instant::now();
    let grid = orbital_grid(&field, &points, ISO).unwrap();
    let value = |p| field.value(p);
    let sample_time = Cell::new(Duration::ZERO);
    let samples = Cell::new(0usize);
    let extraction = Instant::now();
    let (raw, step) = mesh::isosurface_sampled(&value, grid, ISO, |grid| {
        let start = Instant::now();
        let sampled = field.sample(grid);
        sample_time.set(sample_time.get() + start.elapsed());
        samples.set(samples.get() + grid.size.iter().product::<usize>());
        sampled
    });
    assert!(!raw.truncated);
    let mesh_time = extraction.elapsed() - sample_time.get();
    let stage = Instant::now();
    let fine = mesh::simplify(&raw, DETAIL[0]);
    let simplify_fine = stage.elapsed();
    let stage = Instant::now();
    let coarse = mesh::simplify(&fine, DETAIL[1]);
    let simplify_coarse = stage.elapsed();
    let stage = Instant::now();
    let fine = mesh::refine(&fine, value, ISO, step);
    let refine_fine = stage.elapsed();
    let stage = Instant::now();
    let coarse = mesh::refine(&coarse, value, ISO, step);
    let refine_coarse = stage.elapsed();
    let total = start.elapsed();
    let (atoms, aos, samples) = (centres.len(), row.len(), samples.get());
    let triangles = [&raw, &fine, &coarse].map(|m| m.triangles.len());
    let sample = sample_time.get();
    println!("\n{name}: atoms={atoms} AOs={aos} iso={ISO} samples={samples} (including retries)");
    println!("  triangles [raw, fine, coarse]={triangles:?}");
    println!("  sample={sample:.2?} mesh_excluding_sample={mesh_time:.2?}");
    println!("  simplify_fine={simplify_fine:.2?} simplify_coarse={simplify_coarse:.2?}");
    println!("  refine_fine={refine_fine:.2?} refine_coarse={refine_coarse:.2?} total={total:.2?}");
    for mesh in [&raw, &fine, &coarse] {
        assert!(mesh.positive.iter().any(|&p| p) && mesh.positive.iter().any(|&p| !p));
    }
}

#[test]
#[ignore = "manual production-path timing; run with --release orbital -- --ignored --nocapture"]
fn orbital_release_stage_timings() {
    // Sequential cases avoid interference from this test's own parallelism.
    measure("benzene-layout C6H6 (planar, synthetic dense MO)", 6);
    measure("folded C100H100 scaffold (3D helix, unoptimized)", 100);
}
