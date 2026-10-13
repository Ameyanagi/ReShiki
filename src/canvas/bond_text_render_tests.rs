//! Opt-in native canvas evidence for bonding from a formatted free caption.
use super::*;
use iced::advanced::Renderer as _;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::renderer::Headless;
use iced::widget::canvas::Program;
use reshiki::{
    canvas_theme::CanvasTheme,
    document::Annotation,
    typography::{Script, StyleChange},
};

fn caption_document() -> (Document, u64, World, World) {
    let mut doc = reshiki::rings::Preset::Benzene.document(42., false);
    doc.canvas_theme = CanvasTheme::Light;
    let top = doc
        .atoms
        .iter()
        .min_by(|a, b| a.position.y.total_cmp(&b.position.y))
        .unwrap()
        .position;
    let mut caption = Annotation {
        id: doc.next_id(),
        position: top.offset(-8., -56.),
        text: "Pd2+".into(),
        format: Default::default(),
    };
    caption.format.apply(
        &caption.text,
        Some(2..4),
        &StyleChange::Script(Script::Superscript),
    );
    let id = caption.id;
    let ink = reshiki::atom_text::text_first_glyph_center(&caption).unwrap();
    doc.annotations.push(caption);
    let start = reshiki::atom_text::text_atom_position(&doc, id).unwrap();
    (doc, id, ink, start)
}

fn render(
    renderer: &mut Renderer,
    doc: &Document,
    state: &State,
    camera: Camera,
    bounds: Rectangle,
) -> Vec<u8> {
    renderer.reset(bounds);
    let mut canvas = tests::chain_canvas(doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    canvas.camera = camera;
    let cursor = state
        .cursor
        .map_or(mouse::Cursor::Unavailable, mouse::Cursor::Available);
    for geometry in canvas.draw(state, renderer, &Theme::Light, bounds, cursor) {
        renderer.draw_geometry(geometry);
    }
    Headless::screenshot(renderer, iced::Size::new(640, 400), 1., Color::WHITE)
}

#[tokio::test]
#[ignore = "Opt-in formatted-caption bond PNG evidence; set RESHIKI_BOND_TEXT_PNG_DIR"]
async fn bond_from_formatted_text_native_canvas_evidence() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_BOND_TEXT_PNG_DIR")
            .expect("Set RESHIKI_BOND_TEXT_PNG_DIR to the PNG artifact directory"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    let bounds = Rectangle::with_size(iced::Size::new(640., 400.));
    let camera = Camera {
        center: World::new(20., -24.),
        zoom: 2.5,
    };
    let (doc, text, ink, start) = caption_document();
    let before = render(&mut renderer, &doc, &State::default(), camera, bounds);
    let hover_state = State {
        cursor: Some(camera.screen(ink, bounds)),
        ..Default::default()
    };
    let hover = render(&mut renderer, &doc, &hover_state, camera, bounds);
    assert_ne!(
        before, hover,
        "Eligible caption ink has a visible hover cue"
    );

    // The default 30° growth direction, down and to the right.
    let end = start.offset(36.373, 21.);
    let drag_state = State {
        cursor: Some(camera.screen(end, bounds)),
        gesture: Some(Gesture::Draw {
            start,
            pressed: ink,
            id: None,
            text: Some(text),
        }),
        ..Default::default()
    };
    let drag = render(&mut renderer, &doc, &drag_state, camera, bounds);
    let after_doc = crate::app::App::bond_text_evidence_after(
        doc,
        Edit::TextBond {
            source: None,
            source_text: Some(text),
            start,
            end: Some(end),
            target: None,
            target_text: None,
        },
    );
    let after = render(&mut renderer, &after_doc, &State::default(), camera, bounds);
    for (name, pixels) in [
        ("1-before.png", before),
        ("2-hover.png", hover),
        ("3-drag.png", drag),
        ("4-after.png", after),
    ] {
        let path = directory.join(name);
        image::save_buffer(&path, &pixels, 640, 400, image::ColorType::Rgba8).unwrap();
        assert!(path.is_file(), "Produced {}", path.display());
    }
}
