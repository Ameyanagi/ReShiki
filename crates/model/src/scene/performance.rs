//! Opt-in release-mode measurements of how model queries grow with the drawing.
//! `cargo test --release -p reshiki-model scene::performance -- --ignored --nocapture`
use crate::document::{Document, Point};
use crate::editing::{Arrange, arrange};
use std::{hint::black_box, time::Instant};

fn measure(name: &str, count: usize, mut run: impl FnMut()) {
    for _ in 0..3 {
        run();
    }
    let mut samples: Vec<f64> = (0..count)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed().as_secs_f64() * 1_000.
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    println!(
        "PERF,{name},{count},{:.4},{:.4},{:.4}",
        samples[count / 2],
        samples[(count * 95 / 100).min(count - 1)],
        samples.iter().sum::<f64>() / count as f64
    );
}

/// The shortcut gallery repeated side by side, as the canvas harness does.
pub(crate) fn gallery(copies: usize) -> Document {
    let original: Document = serde_json::from_str(include_str!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let mut doc = original.clone();
    let (lo, hi) = original.bounds();
    for copy in 1..copies {
        crate::editing::append(
            &mut doc,
            &original,
            Point::new((hi.x - lo.x + 100.) * copy as f32, 0.),
        );
    }
    doc
}

/// One 1,001-atom molecule shaped like polyalanine-200.
pub(crate) fn polyalanine(residues: usize) -> Document {
    let mut doc = Document::default();
    let mut previous = None;
    let mut x = 0.;
    for _ in 0..residues {
        let n = doc.add_atom("N", Point::new(x, 0.));
        let alpha = doc.add_atom("C", Point::new(x + 31., 18.));
        let beta = doc.add_atom("C", Point::new(x + 31., 54.));
        let carbonyl = doc.add_atom("C", Point::new(x + 62., 0.));
        let oxygen = doc.add_atom("O", Point::new(x + 62., -36.));
        if let Some(previous) = previous {
            doc.add_bond(previous, n, 1, "plain");
        }
        doc.add_bond(n, alpha, 1, "plain");
        doc.add_bond(alpha, beta, 1, "plain");
        doc.add_bond(alpha, carbonyl, 1, "plain");
        doc.add_bond(carbonyl, oxygen, 2, "plain");
        previous = Some(carbonyl);
        x += 93.;
    }
    if let Some(previous) = previous {
        let end = doc.add_atom("O", Point::new(x, 18.));
        doc.add_bond(previous, end, 1, "plain");
    }
    doc
}

#[test]
#[ignore = "Release-mode model scaling profiling"]
fn model_scaling_workloads() {
    let iterations = std::env::var("RESHIKI_PERF_ITERATIONS")
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    println!("PERF,workload,iterations,median_ms,p95_ms,mean_ms");
    for copies in [1, 4, 16] {
        let doc = gallery(copies);
        let ids = doc.all_ids();
        println!(
            "SIZE,{copies}x,{} atoms,{} bonds,{} abbreviations",
            doc.atoms.len(),
            doc.bonds.len(),
            doc.abbreviations.len()
        );
        measure(&format!("scene_primitives_{copies}x"), iterations, || {
            black_box(super::primitives(&doc));
        });
        measure(&format!("crossing_gaps_{copies}x"), iterations, || {
            black_box(crate::crossings::gaps(&doc));
        });
        measure(&format!("selection_bounds_{copies}x"), iterations, || {
            black_box(super::selection_bounds(&doc, &ids));
        });
        measure(&format!("align_left_{copies}x"), iterations, || {
            let mut aligned = doc.clone();
            arrange(&mut aligned, &ids, Arrange::AlignLeft);
            black_box(aligned);
        });
        measure(&format!("document_clone_{copies}x"), iterations, || {
            black_box(doc.clone());
        });
    }
    for residues in [200, 800] {
        let doc = polyalanine(residues);
        println!("SIZE,polyalanine_{residues},{} atoms", doc.atoms.len());
        measure(&format!("polyalanine_{residues}_scene"), iterations, || {
            black_box(super::primitives(&doc));
        });
    }
}
