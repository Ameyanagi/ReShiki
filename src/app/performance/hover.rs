//! Hover profiling through the application update and the real widget renderer.
use super::*;
use iced::advanced::{Layout, layout, mouse, renderer, renderer::Headless, widget::Tree};
use iced::{Rectangle, Size, Vector};

fn hover(camera: crate::canvas::Camera, point: Point) -> Edit {
    Edit::PointerHover(Some(Vector::new(
        (point.x - camera.center.x) * camera.zoom,
        (point.y - camera.center.y) * camera.zoom,
    )))
}

fn drawing() -> Document {
    if let Some(path) = std::env::var_os("RESHIKI_PERF_HOVER_DRAWING") {
        return Document::from_json(&std::fs::read(path).unwrap()).unwrap();
    }
    let gallery: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let mut doc = Document::default();
    for copy in 0..16 {
        editing::append(
            &mut doc,
            &gallery,
            Point::new((copy % 4) as f32 * 1800., (copy / 4) as f32 * 1800.),
        );
    }
    doc
}

struct Ui {
    renderer: iced::Renderer,
    tree: Tree,
    size: Size,
}

impl Ui {
    /// Feed a real widget event so the retained canvas cursor and its hover
    /// overlay participate. Passing only draw's Cursor does not set that state.
    fn pointer(&mut self, app: &App, cursor: iced::Point) -> Vec<Message> {
        let viewport = Rectangle::with_size(self.size);
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        let mut messages = Vec::new();
        view.as_widget_mut().update(
            &mut self.tree,
            &iced::Event::Mouse(mouse::Event::CursorMoved { position: cursor }),
            Layout::new(&node),
            mouse::Cursor::Available(cursor),
            &self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &viewport,
        );
        messages
    }

    fn draw(&mut self, app: &App, cursor: iced::Point) {
        use iced::advanced::Renderer as _;
        let viewport = Rectangle::with_size(self.size);
        let theme = app.theme();
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        self.renderer.reset(viewport);
        let cursor = mouse::Cursor::Available(cursor);
        view.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            Layout::new(&node),
            cursor,
            &viewport,
        );
        black_box(view.as_widget().mouse_interaction(
            &self.tree,
            Layout::new(&node),
            cursor,
            &viewport,
            &self.renderer,
        ));
    }
}

#[tokio::test]
#[ignore = "Release-mode complete hover update and renderer profiling"]
async fn hover_workloads() {
    let doc = drawing();
    println!(
        "drawing,{} atoms,{} bonds,{} abbreviations",
        doc.atoms.len(),
        doc.bonds.len(),
        doc.abbreviations.len()
    );
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.tab.doc = doc;
    app.tab.saved = app.tab.doc.clone();
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    app.inspector_open = false;
    app.appearance.mode = crate::appearance::Mode::Light;
    app.tab.camera.zoom = 1.04;
    app.tab.camera.center = app.tab.doc.atoms[0].position;
    app.sync_keyboard_drawing();
    let doc = &app.tab.doc;
    // Include glyph ink, near-atom, bond and empty-space queries. Their exact
    // targets are printed so the before/after run can compare hit identities.
    let bond = &doc.bonds[0];
    let a = doc.atom(bond.a).unwrap().position;
    let b = doc.atom(bond.b).unwrap().position;
    let points = [
        doc.atoms[0].position,
        doc.atom(doc.abbreviations[0].anchor).unwrap().position,
        Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.),
        doc.atom(28).unwrap_or(&doc.atoms[10]).position,
        doc.atoms[0].position.offset(300., 100.),
        doc.bounds().1.offset(100., 100.),
    ];
    // Match physical positions in both sources, including the native packet's
    // f32 screen/world round trip. The baseline driver emits the equivalent
    // legacy world Hover after the same round trip.
    let points = points.map(|p| {
        let camera = app.tab.camera;
        camera.center.offset(
            (p.x - camera.center.x) * camera.zoom / camera.zoom,
            (p.y - camera.center.y) * camera.zoom / camera.zoom,
        )
    });
    println!("workload,median_ms,p95_ms");
    let mut step = 0;
    measure("gallery_16x_atom_label_hit", || {
        black_box(reshiki::scene::atom_label_hit(
            doc,
            points[step % points.len()],
            0.,
        ));
        step += 1;
    });
    measure("gallery_16x_nearest_atom", || {
        black_box(doc.nearest(points[step % points.len()], 10. / 1.04));
        step += 1;
    });
    measure("gallery_16x_nearest_bond", || {
        black_box(editing::nearest_bond(
            doc,
            points[step % points.len()],
            7. / 1.04,
        ));
        step += 1;
    });
    for point in points {
        let _ = app.update(Message::Canvas(hover(app.tab.camera, point)));
        println!("target,{point:?},{:?}", app.tab.keyboard_drawing.target());
    }
    measure("gallery_16x_hover_complete_app_update", || {
        let _ = black_box(app.update(Message::Canvas(hover(
            app.tab.camera,
            points[step % points.len()],
        ))));
        step += 1;
    });
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
    let cursor = iced::Point::new(700., 400.);
    measure("gallery_16x_view_layout_draw_interaction", || {
        ui.draw(&app, cursor);
    });
    measure("gallery_16x_hover_update_through_draw", || {
        let _ = black_box(app.update(Message::Canvas(hover(
            app.tab.camera,
            points[step % points.len()],
        ))));
        step += 1;
        ui.draw(&app, cursor);
    });
}

/// This driver includes the retained native cursor and idle Select overlay that
/// the earlier command-generation driver did not exercise. It uses the matched
/// native camera and path, but still excludes returned tasks and GPU presentation.
#[tokio::test]
#[ignore = "Release-mode native-pointer widget/app/overlay profiling"]
async fn native_pointer_hover_workloads() {
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.tab.doc = drawing();
    app.tab.saved = app.tab.doc.clone();
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    app.inspector_open = false;
    app.guides.crosshair = true;
    app.appearance.mode = crate::appearance::Mode::Light;
    app.tab.camera = crate::canvas::Camera {
        center: Point::new(-8631.204, -5557.835),
        zoom: 1.04,
    };
    app.sync_keyboard_drawing();
    let mut ui = Ui {
        renderer: <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            std::env::var("RESHIKI_PERF_RENDERER").ok().as_deref(),
        )
        .await
        .unwrap(),
        tree: Tree::empty(),
        size: Size::new(1680., 988.),
    };
    println!("native_pointer_renderer,{}", ui.renderer.name());
    println!("workload,median_ms,p95_ms");
    let mut step = 0;
    measure("gallery_16x_native_pointer_widget_app_overlay_draw", || {
        let index = (step * 5) % 151;
        let angle = std::f64::consts::TAU * index as f64 / 150.;
        // Native replay is in outer-window points; the client begins 32pt below.
        let cursor = iced::Point::new(
            (700. - 400. * angle.cos()) as f32,
            (368. + 160. * angle.sin()) as f32,
        );
        let messages = ui.pointer(&app, cursor);
        for message in messages {
            let _ = black_box(app.update(message));
        }
        ui.draw(&app, cursor);
        step += 1;
    });
}
