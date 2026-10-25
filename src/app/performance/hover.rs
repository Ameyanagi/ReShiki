//! Hover profiling through the application update and the real widget renderer.
use super::*;
use iced::advanced::{Layout, layout, mouse, renderer, renderer::Headless, widget::Tree};
use iced::{Rectangle, Size};

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
        let _ = app.update(Message::Canvas(Edit::Hover(Some(point))));
        println!("target,{point:?},{:?}", app.tab.keyboard_drawing.target());
    }
    measure("gallery_16x_hover_complete_app_update", || {
        let _ = black_box(app.update(Message::Canvas(Edit::Hover(Some(
            points[step % points.len()],
        )))));
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
        let _ = black_box(app.update(Message::Canvas(Edit::Hover(Some(
            points[step % points.len()],
        )))));
        step += 1;
        ui.draw(&app, cursor);
    });
}
