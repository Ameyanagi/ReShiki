//! Repeatable CPU workloads for the bundled shortcut gallery.
//! Run with --release; --profile repeats scene construction for a sampling profiler.
use reshiki::{
    document::{Document, Point},
    editing, scene,
};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn measure(name: &str, iterations: usize, mut work: impl FnMut()) {
    for _ in 0..3 {
        work();
    }
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        work();
        samples.push(start.elapsed().as_secs_f64() * 1_000.);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{name},{},{:.4},{:.4},{:.4}",
        iterations,
        samples[iterations / 2],
        samples[(iterations * 95 / 100).min(iterations - 1)],
        samples.iter().sum::<f64>() / iterations as f64
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let original: Document =
        serde_json::from_str(include_str!("../assets/examples/shortcut-examples.rsk"))?;
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|s| s == "--profile") {
        eprintln!("Sampling PID {}", std::process::id());
        let start = Instant::now();
        let mut count = 0;
        while start.elapsed() < Duration::from_secs(30) {
            black_box(scene::primitives(black_box(&original)));
            count += 1;
        }
        eprintln!("{count} scenes in {:?}", start.elapsed());
        return Ok(());
    }
    println!("workload,iterations,median_ms,p95_ms,mean_ms");
    for copies in [1, 4] {
        let mut doc = original.clone();
        let (lo, hi) = original.bounds();
        for copy in 1..copies {
            editing::append(
                &mut doc,
                &original,
                Point::new((hi.x - lo.x + 100.) * copy as f32, 0.),
            );
        }
        let ids = doc.all_ids();
        eprintln!(
            "{copies}x gallery: {} atoms, {} bonds, {} captions",
            doc.atoms.len(),
            doc.bonds.len(),
            doc.annotations.len()
        );
        measure(&format!("gallery_{copies}x_clone"), 30, || {
            black_box(doc.clone());
        });
        measure(&format!("gallery_{copies}x_scene"), 30, || {
            black_box(scene::primitives(black_box(&doc)));
        });
        measure(&format!("gallery_{copies}x_selection_bounds"), 30, || {
            black_box(scene::selection_bounds(black_box(&doc), black_box(&ids)));
        });
        measure(&format!("gallery_{copies}x_align_left"), 30, || {
            let mut arranged = doc.clone();
            editing::arrange(&mut arranged, &ids, editing::Arrange::AlignLeft);
            black_box(arranged);
        });
        let mut moved = doc.clone();
        measure(&format!("gallery_{copies}x_translate"), 30, || {
            moved.translate(black_box(&ids), 0.125, -0.125);
            black_box(&moved);
        });
        measure(&format!("gallery_{copies}x_drag_cpu"), 30, || {
            let mut preview = doc.clone();
            preview.translate(&ids, 15., 10.);
            black_box(scene::primitives(&preview));
            black_box(scene::selection_bounds(&preview, &ids));
        });
    }
    // Highlight joins make repeated per-object bounds construction visible.
    let mut highlighted = Document::default();
    for index in 0..64 {
        let start = Point::new((index % 8) as f32 * 100., (index / 8) as f32 * 80.);
        let a = highlighted.add_atom("C", start);
        let b = highlighted.add_atom("N", start.offset(42., 0.));
        highlighted.add_bond(a, b, 1, "bold");
        highlighted.bonds.last_mut().unwrap().highlight =
            Some(reshiki::palette::Color::Custom([190, 230, 240]));
    }
    let ids = highlighted.all_ids();
    measure("highlighted_64_groups_align_left", 30, || {
        let mut arranged = highlighted.clone();
        editing::arrange(&mut arranged, &ids, editing::Arrange::AlignLeft);
        black_box(arranged);
    });
    Ok(())
}
