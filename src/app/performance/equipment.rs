//! Opt-in comparison of repeated equipment views with cold and retained tile drawings.
use super::hover::Ui;
use super::*;
use iced::Size;
use iced::advanced::{renderer::Headless, widget::Tree};
use reshiki::canvas_theme::CanvasTheme;

fn clear_thumbnails(app: &mut App) {
    for entry in &mut app.equipment.libraries.entries {
        entry.clear_thumbnails();
    }
}

/// Cold drops only the prepared Documents: retained widget geometry stays warm,
/// matching the old browser that reconstructed tile drawings on every view.
#[tokio::test]
#[ignore = "release-mode equipment panel benchmark; requires a headless renderer"]
async fn manual_equipment_panel_workloads() {
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    let _ = app.update(Message::Tool(crate::canvas::Tool::Equipment));
    let mut ui = Ui {
        renderer: <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            std::env::var("RESHIKI_PERF_RENDERER").ok().as_deref(),
        )
        .await
        .expect("Headless renderer"),
        tree: Tree::empty(),
        size: Size::new(1600., 968.),
    };
    println!("renderer,{}", ui.renderer.name());
    println!("workload,median_ms,p95_ms");
    let cursor = iced::Point::new(700., 400.);
    for theme in CanvasTheme::ALL {
        app.tab.doc.canvas_theme = theme;
        measure(&format!("equipment_{theme}_panel_cold_drawings"), || {
            clear_thumbnails(&mut app);
            black_box(app.equipment_panel());
        });
        measure(
            &format!("equipment_{theme}_panel_retained_drawings"),
            || {
                black_box(app.equipment_panel());
            },
        );
        measure(&format!("equipment_{theme}_ui_cold_drawings"), || {
            clear_thumbnails(&mut app);
            ui.draw(&app, cursor);
        });
        measure(&format!("equipment_{theme}_ui_retained_drawings"), || {
            ui.draw(&app, cursor);
        });
    }
}
