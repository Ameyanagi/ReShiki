//! Opt-in release-mode measurements of how model queries grow with the drawing.
//! `cargo test --release -p reshiki-model scene::performance -- --ignored --nocapture`
use crate::document::{Annotation, Arrow, Document, Point};
use crate::editing::{Arrange, arrange};
use std::{hint::black_box, time::Instant};

const HEADER: &str = "PERF,workload,iterations,median_ms,p95_ms,mean_ms,first_ms";

fn iterations() -> usize {
    std::env::var("RESHIKI_PERF_ITERATIONS")
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1)
}

/// Times the first call on its own, then `count` calls after two more warm-ups.
fn measure(name: &str, count: usize, mut run: impl FnMut()) {
    let start = Instant::now();
    run();
    let first = start.elapsed().as_secs_f64() * 1_000.;
    for _ in 0..2 {
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
        "PERF,{name},{count},{:.4},{:.4},{:.4},{first:.4}",
        samples[count / 2],
        samples[(count * 95 / 100).min(count - 1)],
        samples.iter().sum::<f64>() / count as f64
    );
}

fn measure_align(name: &str, count: usize, doc: &Document) {
    let ids = doc.all_ids();
    measure(name, count, || {
        let mut aligned = doc.clone();
        arrange(&mut aligned, &ids, Arrange::AlignLeft);
        black_box(aligned);
    });
}

/// The shortcut gallery repeated side by side, as the canvas harness does, or
/// `stacked` one above the other.
pub(crate) fn gallery(copies: usize, stacked: bool) -> Document {
    let original: Document = serde_json::from_str(include_str!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let mut doc = original.clone();
    let (lo, hi) = original.bounds();
    for copy in 1..copies {
        let offset = if stacked {
            Point::new(0., (hi.y - lo.y + 100.) * copy as f32)
        } else {
            Point::new((hi.x - lo.x + 100.) * copy as f32, 0.)
        };
        crate::editing::append(&mut doc, &original, offset);
    }
    doc
}

/// `unit` repeated in a column of 2^`doublings` copies, `pitch` apart.
fn column(unit: Document, doublings: u32, pitch: f32) -> Document {
    let mut doc = unit;
    for step in 0..doublings {
        let half = doc.clone();
        let added = crate::editing::append(
            &mut doc,
            &half,
            Point::new(0., pitch * 2_f32.powi(step as i32)),
        );
        assert!(!added.is_empty());
    }
    doc
}

/// The drawing that importing polyalanine-200 from SMILES leaves on the
/// canvas (`profile_polyalanine_insert_stages` in the app): 1,001 atoms laid
/// out by the importer, folded into about seven by eight bond lengths, with
/// its computed hydrogen labels.
pub(crate) fn imported_polyalanine() -> Document {
    let mut doc = Document::default();
    let mut ids = vec![];
    for (index, line) in include_str!("polyalanine-200.txt").lines().enumerate() {
        let (x, y) = line.split_once(' ').unwrap();
        // N, C alpha, C beta, carbonyl C and O of each residue, then the acid O.
        let (element, hydrogens) = match (index, index % 5) {
            (0, _) => ("N", 2),
            (1000, _) => ("O", 1),
            (_, 0) => ("N", 1),
            (_, 1) => ("C", 1),
            (_, 2) => ("C", 3),
            (_, 3) => ("C", 0),
            _ => ("O", 0),
        };
        let id = doc.add_atom(element, Point::new(x.parse().unwrap(), y.parse().unwrap()));
        doc.atom_mut(id).unwrap().label_h = hydrogens;
        ids.push(id);
    }
    let mut previous = None;
    for residue in ids.chunks(5) {
        let (first, rest) = residue.split_first().unwrap();
        if let Some(previous) = previous {
            doc.add_bond(previous, *first, 1, "plain");
        }
        if let [alpha, beta, carbonyl, oxygen] = *rest {
            doc.add_bond(*first, alpha, 1, "plain");
            doc.add_bond(alpha, beta, 1, "plain");
            doc.add_bond(alpha, carbonyl, 1, "plain");
            doc.add_bond(carbonyl, oxygen, 2, "plain");
            previous = Some(carbonyl);
        }
    }
    doc.validate().unwrap();
    doc
}

#[test]
#[ignore = "Release-mode model scaling profiling"]
fn model_scaling_workloads() {
    let iterations = iterations();
    println!("{HEADER}");
    for (copies, stacked) in [(1, false), (4, false), (16, false), (4, true), (16, true)] {
        let doc = gallery(copies, stacked);
        let copies = format!("{copies}{}", if stacked { "x_stacked" } else { "x" });
        let ids = doc.all_ids();
        println!(
            "SIZE,{copies},{} atoms,{} bonds,{} abbreviations",
            doc.atoms.len(),
            doc.bonds.len(),
            doc.abbreviations.len()
        );
        measure(&format!("scene_primitives_{copies}"), iterations, || {
            black_box(super::primitives(&doc));
        });
        measure(&format!("crossing_gaps_{copies}"), iterations, || {
            black_box(crate::crossings::gaps(&doc));
        });
        measure(&format!("selection_bounds_{copies}"), iterations, || {
            black_box(super::selection_bounds(&doc, &ids));
        });
        measure_align(&format!("align_left_{copies}"), iterations, &doc);
        measure(&format!("document_clone_{copies}"), iterations, || {
            black_box(doc.clone());
        });
        // One two-attachment abbreviation makes every drawing, label and
        // picking query ask for a copy with remapped bonds. An unchanged
        // drawing shares it; each edit takes a fresh one.
        let mut bridged = doc.clone();
        let chain = [(-60., 0.), (0., 0.), (20., 25.), (40., 20.), (80., 0.)]
            .map(|(x, y)| bridged.add_atom("C", Point::new(x, y - 500.)));
        for pair in chain.windows(2) {
            if let [a, b] = pair {
                bridged.add_bond(*a, *b, 1, "plain");
            }
        }
        bridged.contract(&chain[1..4], "Ala", "Back").unwrap();
        measure(&format!("render_copy_shared_{copies}"), iterations, || {
            black_box(&*bridged.abbreviation_render_copy());
        });
        measure(&format!("render_copy_fresh_{copies}"), iterations, || {
            bridged.atoms[0].position.x += 1.;
            black_box(&*bridged.abbreviation_render_copy());
        });
    }
}

/// The audited import: its first scene in a fresh process, then warmed.
#[test]
#[ignore = "Release-mode model scaling profiling"]
fn imported_polyalanine_scene() {
    let doc = imported_polyalanine();
    println!("{HEADER}");
    println!(
        "SIZE,imported_polyalanine_200,{} atoms,{} bonds",
        doc.atoms.len(),
        doc.bonds.len()
    );
    measure("imported_polyalanine_200_scene", iterations(), || {
        black_box(super::primitives(&doc));
    });
    measure(
        "imported_polyalanine_200_crossing_gaps",
        iterations(),
        || {
            black_box(crate::crossings::gaps(&doc));
        },
    );
}

/// Shapes the side-by-side gallery lacks: columns of small objects that share
/// their x range, and one abbreviation that grows.
#[test]
#[ignore = "Release-mode model scaling profiling"]
fn column_and_large_group_workloads() {
    let iterations = iterations();
    println!("{HEADER}");
    let mut ammonia = Document::default();
    let nitrogen = ammonia.add_atom("N", Point::default());
    ammonia.atom_mut(nitrogen).unwrap().label_h = 3;
    let mut bond = Document::default();
    let ends = [0., 40.].map(|x| bond.add_atom("C", Point::new(x, 0.)));
    bond.add_bond(ends[0], ends[1], 1, "plain");
    let mut condensed = Document::default();
    let centre = condensed.add_atom("C", Point::default());
    for x in [-60., 60.] {
        let end = condensed.add_atom("C", Point::new(x, 35.));
        condensed.add_bond(centre, end, 1, "plain");
    }
    let condensed =
        crate::atom_text::apply(&condensed, centre, "NMe", crate::atom_text::Mode::Auto).unwrap();
    let mut labelled = Document::default();
    let arrow = labelled.next_id();
    labelled.arrows.push(Arrow {
        arc: None,
        start_anchor: None,
        end_anchor: None,
        id: arrow,
        start: Point::default(),
        end: Point::new(160., 0.),
        kind: "forward".into(),
        control: None,
        cubic: None,
        nodes: Vec::new(),
        style: None,
        purpose: Default::default(),
    });
    let caption = labelled.next_id();
    labelled.annotations.push(Annotation {
        id: caption,
        position: Point::new(60., -30.),
        text: "heat".into(),
        format: Default::default(),
    });
    labelled.arrow_labels.push(crate::arrow_labels::ArrowLabel {
        arrow,
        annotation: caption,
        side: crate::arrow_labels::Side::Above,
        gap: None,
    });
    for doublings in [8, 10, 12] {
        let count = 1 << doublings;
        let doc = column(ammonia.clone(), doublings, 200.);
        measure(&format!("ammonia_column_scene_{count}"), iterations, || {
            black_box(super::primitives(&doc));
        });
        let doc = column(bond.clone(), doublings, 200.);
        measure(
            &format!("bond_column_crossing_gaps_{count}"),
            iterations,
            || {
                black_box(crate::crossings::gaps(&doc));
            },
        );
        let doc = column(condensed.clone(), doublings, 200.);
        measure(
            &format!("condensed_column_scene_{count}"),
            iterations,
            || {
                black_box(super::primitives(&doc));
            },
        );
        let doc = column(labelled.clone(), doublings, 200.);
        assert_eq!(doc.arrow_labels.len(), count);
        measure_align(
            &format!("labelled_arrows_align_left_{count}"),
            iterations,
            &doc,
        );
        // One collapsed chain of `count` atoms hanging off a visible atom,
        // beside a lone atom to align it with.
        let mut doc = Document::default();
        let chain: Vec<_> = (0..=count)
            .map(|i| doc.add_atom("C", Point::new(i as f32 * 35., (i % 2) as f32 * 20.)))
            .collect();
        for pair in chain.windows(2) {
            if let [a, b] = pair {
                doc.add_bond(*a, *b, 1, "plain");
            }
        }
        doc.contract(chain.get(1..).unwrap(), "R", "R").unwrap();
        doc.add_atom("O", Point::new(-200., 300.));
        measure(&format!("large_group_scene_{count}"), iterations, || {
            black_box(super::primitives(&doc));
        });
        measure_align(&format!("large_group_align_left_{count}"), iterations, &doc);
    }
}
