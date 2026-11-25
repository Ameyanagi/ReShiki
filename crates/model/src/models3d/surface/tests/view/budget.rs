use super::*;
use std::time::Instant;

fn components(mesh: &Mesh) -> [usize; 2] {
    fn root(parents: &mut [usize], mut i: usize) -> usize {
        while parents[i] != i {
            parents[i] = parents[parents[i]];
            i = parents[i];
        }
        i
    }
    let mut parents: Vec<_> = (0..mesh.positions.len()).collect();
    for triangle in &mesh.triangles {
        let a = root(&mut parents, triangle[0] as usize);
        for &index in &triangle[1..] {
            let b = root(&mut parents, index as usize);
            parents[b] = a;
        }
    }
    let mut sets: [std::collections::HashSet<usize>; 2] =
        std::array::from_fn(|_| Default::default());
    for (triangle, &sign) in mesh.triangles.iter().zip(&mesh.positive) {
        sets[usize::from(sign)].insert(root(&mut parents, triangle[0] as usize));
    }
    sets.map(|set| set.len())
}

// Run with: cargo test -p reshiki-model --release surface_budgets -- --ignored --nocapture
#[test]
#[ignore = "release-mode mesh and frame timing evidence"]
fn surface_budgets() {
    assert!(
        !std::hint::black_box(cfg!(debug_assertions)),
        "run this ignored benchmark with --release"
    );
    let grid = Grid {
        origin: [-5.5; 3],
        step: 11. / 96.,
        size: [97; 3],
    };
    let mut timing_failures = Vec::new();
    for benzene in [false, true] {
        let centers: Vec<[f32; 3]> = (0..6)
            .map(|i| {
                let angle = i as f32 * std::f32::consts::TAU / 6.;
                [1.4 * angle.cos(), 1.4 * angle.sin(), 0.]
            })
            .collect();
        let build_start = Instant::now();
        let raw = isosurface(
            |p| {
                if benzene {
                    centers
                        .iter()
                        .enumerate()
                        .map(|(i, center)| {
                            let d = std::array::from_fn::<_, 3, _>(|j| p[j] - center[j]);
                            let sign = if i % 2 == 0 { 1. } else { -1. };
                            sign * d[2] * (-1.6 * d.iter().map(|v| v * v).sum::<f32>().sqrt()).exp()
                        })
                        .sum()
                } else {
                    p[2] * (-p.iter().map(|x| x * x).sum::<f32>().sqrt()).exp()
                }
            },
            grid,
            if benzene { 0.035 } else { 0.08 },
        );
        let build_ms = build_start.elapsed().as_secs_f64() * 1_000.;
        assert!(!raw.truncated);
        assert!(
            raw.triangles.len() >= 20_000,
            "timing scene must genuinely require the export budget, raw={}",
            raw.triangles.len()
        );
        assert_eq!(
            components(&raw),
            if benzene { [6, 6] } else { [1, 1] },
            "budget scene must contain the intended independent phase lobes"
        );
        let mut molecule = Vec::new();
        let rotation = rotations()[3];
        for i in 0..12 {
            let angle = i as f64 * std::f64::consts::TAU / 12.;
            let next = (i + 1) as f64 * std::f64::consts::TAU / 12.;
            let center = [
                1.6 * angle.cos(),
                1.6 * angle.sin(),
                0.2 * (2. * angle).sin(),
            ];
            let endpoint = [1.6 * next.cos(), 1.6 * next.sin(), 0.2 * (2. * next).sin()];
            molecule.push(Molecule::Ball {
                center,
                radius: 0.28,
            });
            molecule.push(Molecule::Stick {
                a: center,
                b: endpoint,
                half: 0.13,
            });
        }
        for (target, limit_ms) in [(20_000, 150.), (4_000, 30.)] {
            let simplify_start = Instant::now();
            let mesh = simplify(&raw, target);
            let simplify_ms = simplify_start.elapsed().as_secs_f64() * 1_000.;
            assert!(
                mesh.triangles.len() <= target,
                "benchmark must use the requested mesh budget"
            );
            assert!(
                mesh.triangles.len() >= target * 9 / 10,
                "budget measurement must perform real work"
            );
            let (mut frame, _) = prepare(&mesh, &molecule, rotation, true);
            frame.lightness = mesh
                .normals
                .iter()
                .map(|&normal| {
                    let normal = rotate(f64s(normal), rotation);
                    (0.35 + 0.65 * dot(normal, [-0.4, 0.5, 0.768_114_6]).max(0.)) as f32
                })
                .collect();
            if build_ms + simplify_ms > 1_500. {
                timing_failures.push(format!(
                    "benzene={benzene}, target={target}: mesh build took {:.2}ms, budget 1500ms",
                    build_ms + simplify_ms
                ));
            }
            for tones in [&[10][..], &[10; 4][..]] {
                let start = Instant::now();
                let output = visible(&frame.input(&mesh, tones));
                let visible_ms = start.elapsed().as_secs_f64() * 1_000.;
                let contours: usize = output.layers.iter().map(|layer| layer.region.len()).sum();
                let layer_points: usize = output
                    .layers
                    .iter()
                    .flat_map(|layer| &layer.region)
                    .map(Vec::len)
                    .sum();
                let rim_points: usize = output.rims.iter().flatten().map(Vec::len).sum();
                let points = layer_points + rim_points;
                eprintln!(
                    "surface budget scene={} samples={} raw={} build_ms={build_ms:.2} simplify_ms={simplify_ms:.2} triangles={} front={} pieces={} overlaps={} groups={} fragments={} limit_hits={} visible_ms={visible_ms:.2} reported_ms={:.2} layers={} contours={contours} rims={} layer_points={layer_points} rim_points={rim_points} points={points}",
                    if benzene { "12-lobe" } else { "two-lobe" },
                    97_usize.pow(3),
                    raw.triangles.len(),
                    mesh.triangles.len(),
                    output.stats.front,
                    output.stats.pieces,
                    output.stats.overlaps,
                    tones.len(),
                    output.stats.fragments,
                    output.stats.fragment_limit_hits,
                    output.stats.millis,
                    output.layers.len(),
                    output.rims.iter().map(Vec::len).sum::<usize>()
                );
                assert!(output.stats.front > target / 4);
                assert!(output.stats.pieces > 100 && contours > 0 && points > 100);
                assert_eq!(output.stats.fragment_limit_hits, 0);
                if tones.len() == 1 && visible_ms > limit_ms {
                    timing_failures.push(format!("benzene={benzene}, target={target}: visibility took {visible_ms:.2}ms, budget {limit_ms}ms"));
                }
            }
        }
    }
    assert!(timing_failures.is_empty(), "{}", timing_failures.join("\n"));
}
