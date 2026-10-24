//! Opt-in, release-mode timings of editing work on the event-loop thread.
use super::*;
use reshiki::engine::ChemistryEngine;
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
        app.tab.doc = doc;
        app.tab.selected = vec![endpoint];
        app.tab.hover = None;
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
        app.tab.selected = vec![endpoint];
        let request = reshiki::nmr::prepare(&app.tab.doc, &app.tab.selected).unwrap();
        let report = reshiki::nmr::predict(&request, reshiki::nmr::Nucleus::H1).unwrap();
        nmr::test_support::accept(&mut app, std::sync::Arc::new(report));
        measure(&format!("gallery_{copies}x_idle_update_nmr"), || {
            let _ = black_box(app.update(Message::InspectorScroll(0.)));
        });
        if copies == 0 {
            let _ = app.context_key("o");
            let runtime = tokio::runtime::Runtime::new().unwrap();
            measure("methanol_full_analysis", || {
                black_box(
                    runtime.block_on(
                        app.engine
                            .execute(Request::molecule("analyze", app.tab.doc.clone())),
                    ),
                )
                .unwrap();
            });
            measure("methanol_drawing_labels", || {
                let molecule = reshiki::chemistry::document::prepare(&app.tab.doc).unwrap();
                let drawing =
                    reshiki::chemistry::document::for_drawing(&molecule, &app.tab.doc).unwrap();
                black_box(drawing.labels()).unwrap();
            });
        }
    }
}

#[test]
#[ignore = "release-mode camera benchmark"]
fn camera_workloads() {
    println!("workload,median_ms,p95_ms");
    let gallery: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    for copies in [1, 4, 16] {
        let mut doc = Document::default();
        for copy in 0..copies {
            let offset = Point::new((copy % 4) as f32 * 1800., (copy / 4) as f32 * 1800.);
            editing::append(&mut doc, &gallery, offset);
        }
        let (mut app, _) = App::new();
        app.tab.doc = doc;
        measure(&format!("gallery_{copies}x_pan"), || {
            let _ = black_box(app.update(Message::Canvas(Edit::Pan(3., -2.))));
        });
        measure(&format!("gallery_{copies}x_zoom_in_out"), || {
            let at = Point::new(10., 10.);
            let _ = black_box(app.update(Message::Canvas(Edit::Zoom(1.1, at))));
            let _ = black_box(app.update(Message::Canvas(Edit::Zoom(1. / 1.1, at))));
        });
    }
}

#[test]
#[ignore = "release-mode unsaved-changes benchmark"]
fn unsaved_changes_check() {
    println!("workload,median_ms,p95_ms");
    let gallery: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    for copies in [1, 4, 16] {
        let mut doc = Document::default();
        for copy in 0..copies {
            editing::append(&mut doc, &gallery, Point::new(copy as f32 * 1800., 0.));
        }
        let atoms = doc.atoms.len();
        let (mut app, _) = App::new();
        app.tab.doc = doc.clone();
        app.tab.saved = doc;
        // What one update's views ask: the title, then the front tab's dot
        // twice in the tab strip. Each update forgets the answer.
        let view = |app: &App, name: &str| {
            measure(name, || {
                app.tab.forget_unsaved();
                black_box(app.title());
                black_box(app.tab.shown_dirty());
                black_box(app.tab.shown_dirty());
            });
        };
        view(&app, &format!("{atoms}_atoms_views_saved"));
        // The last atom moved, so a comparison walks the whole drawing.
        app.tab.doc.atoms.last_mut().unwrap().position.x += 1.;
        app.tab.revision = app.tab.revision.wrapping_add(1);
        view(&app, &format!("{atoms}_atoms_views_edited"));
    }
}

#[test]
#[ignore = "release-mode label landing benchmark"]
fn label_landing() {
    use reshiki::atom_labels::refresh::Refresh;
    use std::sync::Arc;
    println!("workload,median_ms,p95_ms,revision_bumps");
    let gallery: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    for copies in [1, 4, 16] {
        let mut doc = Document::default();
        for copy in 0..copies {
            editing::append(&mut doc, &gallery, Point::new(copy as f32 * 1800., 0.));
        }
        let atoms = doc.atoms.len();
        let endpoint = doc.atoms.last().unwrap().id;
        let (mut app, _) = App::new();
        app.tab.doc = doc.clone();
        app.tab.saved = doc;
        app.tab.labels_dirty = true;
        let mut bumps = 0;
        let mut samples = Vec::new();
        for run in 0..33 {
            let element = if run % 2 == 0 { "o" } else { "c" };
            app.tab.selected = vec![endpoint];
            let _ = app.update(Message::ContextKey(element.into()));
            let _ = app.start_label_refresh();
            let key = app.tab.label_refresh.pending_key().unwrap();
            let labels = Refresh::calculate(&app.tab.doc, &Refresh::default()).map(Arc::new);
            let revision = app.tab.revision;
            let start = Instant::now();
            let _ = black_box(app.update(Message::LabelsReady(key, labels)));
            if run >= 3 {
                samples.push(start.elapsed().as_secs_f64() * 1000.);
                bumps += usize::from(app.tab.revision != revision);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "{atoms}_atoms_labels_land,{:.4},{:.4},{bumps}",
            samples[15], samples[28]
        );
    }
}
