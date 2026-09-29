//! Opt-in, release-mode timings of editing work on the event-loop thread.
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
#[ignore = "release-mode editing benchmark"]
fn editing_workloads() {
    println!("workload,median_ms,p95_ms");
    let gallery: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    for copies in [0, 1, 4] {
        let mut doc = Document::default();
        for copy in 0..copies {
            editing::append(&mut doc, &gallery, Point::new(copy as f32 * 1800., 0.));
        }
        let start = doc.add_atom("C", Point::new(-100., -100.));
        let endpoint = doc.add_atom("C", Point::new(-58., -100.));
        doc.add_bond(start, endpoint, 1, "plain");
        let ids = doc.all_ids();
        measure(&format!("gallery_{copies}x_validate"), || {
            black_box(doc.validate()).unwrap();
        });
        measure(&format!("gallery_{copies}x_groups"), || {
            black_box(editing::groups(&doc, &ids));
        });
        {
            use reshiki::atom_labels::refresh::Refresh;
            use std::sync::Arc;
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let cached = Arc::new(Refresh::calculate(&doc, &Refresh::default()).unwrap());
            let mut edited = doc.clone();
            edited.atom_mut(endpoint).unwrap().element = "O".into();
            measure(&format!("gallery_{copies}x_labels_cold"), || {
                black_box(Refresh::calculate(&edited, &Refresh::default())).unwrap();
            });
            measure(&format!("gallery_{copies}x_labels_one_changed"), || {
                black_box(Refresh::calculate(&edited, &cached)).unwrap();
            });
            measure(
                &format!("gallery_{copies}x_labels_worker_roundtrip"),
                || {
                    let labels = runtime
                        .block_on(label_refresh::calculate(
                            edited.clone(),
                            Arc::clone(&cached),
                        ))
                        .unwrap();
                    labels.apply(&mut edited);
                },
            );
        }
        let (mut app, _) = App::new();
        app.doc = doc;
        app.selected = vec![endpoint];
        app.hover = None;
        measure(&format!("gallery_{copies}x_oxygen_hotkey"), || {
            let _ = black_box(app.update(Message::ContextKey("o".into())));
            let _ = black_box(app.update(Message::ContextKey("c".into())));
        });
        measure(&format!("gallery_{copies}x_idle_update"), || {
            let _ = black_box(app.update(Message::InspectorScroll(0.)));
        });
        measure(&format!("gallery_{copies}x_view"), || {
            black_box(app.view());
        });
        if copies == 0 {
            let _ = app.context_key("o");
            let runtime = tokio::runtime::Runtime::new().unwrap();
            measure("methanol_full_analysis", || {
                black_box(
                    runtime.block_on(
                        app.engine
                            .execute(Request::molecule("analyze", app.doc.clone())),
                    ),
                )
                .unwrap();
            });
            measure("methanol_drawing_labels", || {
                let molecule = reshiki::chemistry::document::prepare(&app.doc).unwrap();
                let drawing =
                    reshiki::chemistry::document::for_drawing(&molecule, &app.doc).unwrap();
                black_box(drawing.labels()).unwrap();
            });
        }
    }
}
