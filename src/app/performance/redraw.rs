//! Redraw and memory workloads: the inspector with everything selected, the
//! template panel, and undo history across tabs.
use super::hover::Ui;
use super::*;
use iced::Size;
use iced::advanced::{renderer::Headless, widget::Tree};

fn gallery(copies: usize) -> Document {
    let gallery: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let mut doc = Document::default();
    for copy in 0..copies {
        let offset = Point::new((copy % 4) as f32 * 1800., (copy / 4) as f32 * 1800.);
        editing::append(&mut doc, &gallery, offset);
    }
    doc
}

fn open(doc: Document) -> App {
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.tab.doc = doc;
    app.tab.saved = app.tab.doc.clone();
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    app
}

#[test]
#[ignore = "release-mode inspector and undo benchmark"]
fn manual_select_all_workloads() {
    println!("workload,median_ms,p95_ms");
    for copies in [1, 4, 16] {
        let mut app = open(gallery(copies));
        let atom = app.tab.doc.atoms.last().unwrap().id;
        app.tab.selected = vec![atom];
        let _ = app.update(Message::ContextKey("o".into()));
        app.tab.labels_dirty = false;
        let _ = app.update(Message::Canvas(Edit::Select(vec![])));
        measure(&format!("gallery_{copies}x_view_none_selected"), || {
            black_box(app.view());
        });
        measure(
            &format!("gallery_{copies}x_undo_redo_none_selected"),
            || {
                let _ = black_box(app.update(Message::Undo));
                let _ = black_box(app.update(Message::Redo));
            },
        );
        let _ = app.update(Message::Canvas(Edit::Select(app.tab.doc.all_ids())));
        measure(&format!("gallery_{copies}x_view_all_selected"), || {
            black_box(app.view());
        });
        measure(&format!("gallery_{copies}x_undo_redo_all_selected"), || {
            let _ = black_box(app.update(Message::Undo));
            let _ = black_box(app.update(Message::Redo));
        });
    }
}

#[tokio::test]
#[ignore = "release-mode template panel benchmark; requires a headless renderer"]
async fn manual_template_panel_workloads() {
    let mut app = open(gallery(1));
    let mut ui = Ui {
        renderer: <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            std::env::var("RESHIKI_PERF_RENDERER").ok().as_deref(),
        )
        .await
        .unwrap(),
        tree: Tree::empty(),
        size: Size::new(1600., 968.),
    };
    println!("renderer,{}", ui.renderer.name());
    println!("workload,median_ms,p95_ms");
    let cursor = iced::Point::new(700., 400.);
    app.inspector_open = false;
    measure("inspector_closed_draw", || ui.draw(&app, cursor));
    app.inspector_open = true;
    measure("properties_draw", || ui.draw(&app, cursor));
    let _ = app.update(Message::Inspector(InspectorTab::Templates));
    measure("template_categories_draw", || ui.draw(&app, cursor));
    let mut sizes = std::collections::BTreeMap::<String, usize>::new();
    for template in app.templates.library.iter() {
        for name in template_library::categories(template) {
            *sizes.entry(name.to_owned()).or_default() += 1;
        }
    }
    let (largest, count) = sizes.into_iter().max_by_key(|(_, count)| *count).unwrap();
    println!("collection,{largest},{count} templates");
    let _ = app.update(Message::Templates(template_library::Action::Collection(
        largest,
    )));
    measure("template_largest_collection_draw", || ui.draw(&app, cursor));
}

/// One chain of `atoms` carbons: plain, with a stored model of all of them,
/// or with that model as a crystal.
fn modeled(atoms: usize, kind: &str) -> Document {
    use reshiki::models3d::{Cell, ForceField, Model3d, Placed, Site};
    let mut doc = Document::default();
    let mut previous = None;
    let mut placed = vec![];
    for i in 0..atoms {
        let (x, y) = ((i % 100) as f32, (i / 100) as f32);
        let id = doc.add_atom("C", Point::new(x * 36., y * 42. + (i % 2) as f32 * 21.));
        if let Some(previous) = previous {
            doc.add_bond(previous, id, 1, "plain");
        }
        previous = Some(id);
        placed.push(Placed {
            id,
            xyz: [x * 1.3, y * 1.5, (i % 2) as f32 * 0.8],
        });
    }
    if kind == "plain" {
        return doc;
    }
    let mut model = Model3d::new(&doc, placed, vec![], ForceField::default());
    if kind == "crystal" {
        let sites = doc
            .atoms
            .iter()
            .map(|atom| Site {
                id: atom.id,
                label: format!("C{}", atom.id),
                adp: None,
                asymmetric: true,
            })
            .collect();
        let cell = Cell {
            vectors: [[140., 0., 0.], [0., 140., 0.], [0., 0., 10.]],
            crossing: vec![],
        };
        model = model.with_cell(&doc, Some(cell), sites);
    }
    doc.models.push(model);
    doc
}

/// The plain chain is the baseline: what the panel costs for one molecule
/// of that size without a model.
#[test]
#[ignore = "release-mode stored-model select-all benchmark"]
fn manual_model_select_all_workloads() {
    println!("workload,median_ms,p95_ms");
    for kind in ["plain", "model", "crystal"] {
        for atoms in [250, 1000, 4000] {
            let mut app = open(modeled(atoms, kind));
            let _ = app.update(Message::Canvas(Edit::Select(app.tab.doc.all_ids())));
            assert_eq!(
                app.selected_model().and_then(|(_, index)| index),
                (kind != "plain").then_some(0)
            );
            measure(&format!("{kind}_{atoms}_selected_model"), || {
                black_box(app.selected_model());
            });
            measure(&format!("{kind}_{atoms}_view_all_selected"), || {
                black_box(app.view());
            });
        }
    }
}

fn resident_mb() -> f64 {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .unwrap()
        / 1024.
}

/// Run alone: the counters are process-wide.
#[test]
#[ignore = "release-mode undo history memory measurement"]
fn manual_history_memory() {
    use crate::allocation_metrics::{reset, snapshot};
    const MB: f64 = (1 << 20) as f64;
    // What one copy of the drawing costs, and which part an edit changes.
    println!("copy,allocations,allocated_mb");
    let doc = gallery(16);
    let copy = |name: &str, f: &dyn Fn()| {
        reset();
        f();
        let made = snapshot();
        println!(
            "{name},{},{:.2}",
            made.allocation_count,
            made.allocated_bytes as f64 / MB
        );
    };
    copy("document", &|| drop(black_box(doc.clone())));
    copy("atoms", &|| drop(black_box(doc.atoms.clone())));
    copy("bonds", &|| drop(black_box(doc.bonds.clone())));
    drop(doc);

    // Each row covers the step since the row before: the peak of live bytes
    // in it, and the allocations made in it.
    println!("state,live_mb,peak_mb,resident_mb,frames,allocations,allocated_mb");
    let mut app = open(Document::default());
    let report = |app: &App, state: &str| {
        let frames: usize = std::iter::once(&app.tab)
            .chain(&app.tabs.background)
            .map(|tab| tab.history.frames())
            .sum();
        let now = snapshot();
        println!(
            "{state},{:.1},{:.1},{:.1},{frames},{},{:.1}",
            now.live_bytes as f64 / MB,
            now.peak_bytes as f64 / MB,
            resident_mb(),
            now.allocation_count,
            now.allocated_bytes as f64 / MB,
        );
        reset();
    };
    report(&app, "empty");
    for tab in 0..5 {
        if tab > 0 {
            app.add_tab();
        }
        app.tab.doc = gallery(16);
        app.tab.saved = app.tab.doc.clone();
        report(&app, &format!("tab_{}_opened", tab + 1));
        let atom = app.tab.doc.atoms.last().unwrap().id;
        for edit in 0..60 {
            app.tab.selected = vec![atom];
            let element = if edit % 2 == 0 { "o" } else { "c" };
            let before = (edit == 0 && tab == 0).then(|| app.tab.doc.clone());
            let _ = app.update(Message::ContextKey(element.into()));
            if let Some(before) = before {
                let doc = &app.tab.doc;
                println!(
                    "# one edit changes: atoms {}, bonds {}, annotations {}, graphics {}, arrows {}",
                    before.atoms != doc.atoms,
                    before.bonds != doc.bonds,
                    before.annotations != doc.annotations,
                    before.graphics != doc.graphics,
                    before.arrows != doc.arrows,
                );
            }
            black_box(app.view());
        }
        report(&app, &format!("tab_{}_60_edits", tab + 1));
        if tab == 0 {
            // The undone steps wait on the redo side and still count.
            for _ in 0..60 {
                let _ = app.update(Message::Undo);
            }
            report(&app, "tab_1_all_undone");
            while app.tab.history.can_redo() {
                let _ = app.update(Message::Redo);
            }
            report(&app, "tab_1_all_redone");
        }
    }
}
