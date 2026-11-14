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
fn select_all_workloads() {
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
async fn template_panel_workloads() {
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
fn history_memory() {
    const MB: f64 = (1 << 20) as f64;
    println!("state,live_mb,resident_mb,undo_frames");
    let mut app = open(Document::default());
    let report = |app: &App, state: &str| {
        let frames: usize = std::iter::once(&app.tab)
            .chain(&app.tabs.background)
            .map(|tab| tab.history.frames())
            .sum();
        println!(
            "{state},{:.1},{:.1},{frames}",
            crate::allocation_metrics::snapshot().live_bytes as f64 / MB,
            resident_mb()
        );
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
            let _ = app.update(Message::ContextKey(element.into()));
            black_box(app.view());
        }
        report(&app, &format!("tab_{}_60_edits", tab + 1));
    }
}
