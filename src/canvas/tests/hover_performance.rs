//! Opt-in component measurements for the retained Select-hover overlay.
use super::*;
use std::{hint::black_box, time::Instant};

fn measure(name: &str, mut work: impl FnMut()) {
    for _ in 0..3 {
        work();
    }
    let mut samples = Vec::new();
    for _ in 0..30 {
        let start = Instant::now();
        work();
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    samples.sort_by(f64::total_cmp);
    println!("{name},{:.4},{:.4}", samples[15], samples[28]);
}

#[test]
#[ignore = "Release-mode exact-gallery retained-hover component profiling"]
fn retained_hover_components() {
    let path = std::env::var("RESHIKI_PERF_HOVER_DRAWING").expect("exact gallery fixture path");
    let doc = Document::from_json(&std::fs::read(path).unwrap()).unwrap();
    let center = World::new(-8631.204, -5557.835);
    let points = [0., 25., 50., 75., 100., 125.].map(|index| {
        let angle = std::f64::consts::TAU * index / 150.;
        center.offset(
            ((700. - 400. * angle.cos() - 892.) / 1.04) as f32,
            ((400. + 160. * angle.sin() - 580.) / 1.04) as f32,
        )
    });
    println!("workload,median_ms,p95_ms");
    measure("hover_indicators", || {
        black_box(reshiki::atom_labels::indicators(&doc));
    });
    let mut step = 0;
    measure("hover_hit_object", || {
        black_box(hit::hit_object(
            &doc,
            points[step % points.len()],
            10. / 1.04,
        ));
        step += 1;
    });
    measure("hover_hit_selection", || {
        black_box(hit::hit_selection_with_orbital(
            &doc,
            points[step % points.len()],
            10. / 1.04,
            false,
        ));
        step += 1;
    });
    measure("hover_ring_at", || {
        black_box(reshiki::editing::ring_at(&doc, points[step % points.len()]));
        step += 1;
    });
    let mut cache = cache::SceneCache::default();
    measure("hover_cached_ring_at", || {
        black_box(cache.ring_at(&doc, points[step % points.len()]));
        step += 1;
    });
    measure("hover_empty_markers", || {
        black_box(markers::Markers::hover(&doc, &[]));
    });
}
